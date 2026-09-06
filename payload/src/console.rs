//! The command port the injector attaches to: `127.0.0.1:27015`, one command
//! per line, replies terminated by a line containing `.`.
//!
//! Commands:
//! - `ping`
//! - `read <hex addr> <len>` / `u32 <hex addr>` / `u64 <hex addr>`
//! - `jobs`, `run <job> [args]` - jobs run on the game's main thread and
//!   the reply is their result
//! - `tail` - stream every log line from now on, prefixed `log `

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc};

use parking_lot::Mutex;

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows_sys::Win32::System::Memory::{VirtualQuery, MEMORY_BASIC_INFORMATION, MEM_COMMIT, PAGE_GUARD, PAGE_NOACCESS};
use windows_sys::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible};

pub const ADDRESS: &str = "127.0.0.1:27015";

pub type Job = Box<dyn FnOnce(&[String]) -> String + Send>;
type JobFn = fn(&[String]) -> String;
type Queued = (Job, Vec<String>, mpsc::Sender<String>);

static JOBS: Mutex<Vec<(&'static str, JobFn)>> = Mutex::new(Vec::new());
static QUEUE: Mutex<Vec<Queued>> = Mutex::new(Vec::new());
static TAILS: Mutex<Vec<mpsc::Sender<String>>> = Mutex::new(Vec::new());
static MAIN_THREAD: Mutex<u32> = Mutex::new(0);

pub fn register(name: &'static str, f: JobFn) {
    JOBS.lock().push((name, f));
}

/// Sends a line to every attached `tail`; closed ones are dropped.
pub fn publish(line: &str) {
    TAILS.lock().retain(|tx| tx.send(line.to_string()).is_ok());
}

/// Queues a job for the main thread and waits for its reply.
pub fn run_on_main(job: Job, args: Vec<String>, wait_ms: u64) -> String {
    let (tx, rx) = mpsc::channel();
    QUEUE.lock().push((job, args, tx));
    rx.recv_timeout(std::time::Duration::from_millis(wait_ms))
        .unwrap_or_else(|_| "error: the main thread did not run the job in time".into())
}

/// The thread owning the game's visible top-level window.
pub fn find_main_thread() -> Option<u32> {
    unsafe extern "system" fn visit(hwnd: HWND, out: LPARAM) -> BOOL {
        let mut pid = 0;
        let tid = GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == GetCurrentProcessId() && IsWindowVisible(hwnd) != 0 && GetWindowTextLengthW(hwnd) > 0 {
            *(out as *mut u32) = tid;
            return 0;
        }
        1
    }
    let mut tid = 0u32;
    unsafe { EnumWindows(Some(visit), &mut tid as *mut u32 as LPARAM) };
    if tid == 0 {
        return None;
    }
    *MAIN_THREAD.lock() = tid;
    Some(tid)
}

/// Runs the queued jobs when called on the main thread.
pub fn drain_on_main() {
    let main = *MAIN_THREAD.lock();
    if main == 0 || unsafe { GetCurrentThreadId() } != main {
        return;
    }
    let queued: Vec<Queued> = std::mem::take(&mut *QUEUE.lock());
    for (job, args, reply) in queued {
        let _ = reply.send(job(&args));
    }
}

pub fn serve() {
    std::thread::spawn(|| {
        let Ok(listener) = TcpListener::bind(ADDRESS) else {
            crate::logf!("console: could not bind {ADDRESS}");
            return;
        };
        crate::logf!("console: listening on {ADDRESS}");
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || client(stream));
        }
    });
}

fn client(stream: TcpStream) {
    let writer = Arc::new(Mutex::new(stream.try_clone().unwrap()));
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line == "tail" {
            let (tx, rx) = mpsc::channel::<String>();
            TAILS.lock().push(tx);
            let tail_writer = writer.clone();
            std::thread::spawn(move || {
                for l in rx {
                    if tail_writer.lock().write_all(format!("log {l}\n").as_bytes()).is_err() {
                        break;
                    }
                }
            });
            let _ = writer.lock().write_all(b"tailing\n.\n");
            continue;
        }
        let reply = handle(line);
        if writer.lock().write_all(format!("{reply}\n.\n").as_bytes()).is_err() {
            break;
        }
    }
}

fn handle(line: &str) -> String {
    let parts: Vec<String> = line.split_whitespace().map(String::from).collect();
    let Some(command) = parts.first() else { return String::new() };
    let hex = |s: &String| usize::from_str_radix(s.trim_start_matches("0x"), 16).ok();
    match command.as_str() {
        "ping" => "pong".into(),
        "read" | "u32" | "u64" => {
            let Some(addr) = parts.get(1).and_then(hex) else {
                return "error: address".into();
            };
            let len = match command.as_str() {
                "u32" => 4,
                "u64" => 8,
                _ => parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(8),
            };
            if len > 65536 {
                return "error: length".into();
            }
            unsafe {
                if !readable(addr, len) {
                    return "error: unreadable".into();
                }
                let bytes = std::slice::from_raw_parts(addr as *const u8, len);
                match command.as_str() {
                    "u32" => format!("{:#x}", u32::from_le_bytes(bytes[..4].try_into().unwrap())),
                    "u64" => format!("{:#x}", u64::from_le_bytes(bytes[..8].try_into().unwrap())),
                    _ => bytes.iter().map(|b| format!("{b:02x}")).collect(),
                }
            }
        }
        "jobs" => JOBS.lock().iter().map(|(name, _)| *name).collect::<Vec<_>>().join(" "),
        "run" => {
            let Some(name) = parts.get(1) else {
                return "error: job name".into();
            };
            let Some(job) = JOBS.lock().iter().find(|(n, _)| n == name).map(|(_, f)| *f) else {
                return "error: no such job".into();
            };
            run_on_main(Box::new(job), parts[2..].to_vec(), 5000)
        }
        _ => "error: unknown command".into(),
    }
}

unsafe fn readable(addr: usize, len: usize) -> bool {
    let mut info: MEMORY_BASIC_INFORMATION = std::mem::zeroed();
    if VirtualQuery(addr as *const _, &mut info, std::mem::size_of::<MEMORY_BASIC_INFORMATION>()) == 0 {
        return false;
    }
    if info.State != MEM_COMMIT || info.Protect & (PAGE_NOACCESS | PAGE_GUARD) != 0 {
        return false;
    }
    addr + len <= info.BaseAddress as usize + info.RegionSize
}
