//! unlock-all injector and console.
//!
//! Loads the payload into H1Z1.exe by remote `LoadLibraryW`, then attaches to
//! the payload's command port: the session's log so far, every new line as
//! it happens, and typed lines sent as commands. Run again while the game is
//! up and it only attaches.

use std::env;
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, FALSE, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Diagnostics::Debug::WriteProcessMemory;
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, Process32FirstW, Process32NextW, MODULEENTRY32W, PROCESSENTRY32W,
    TH32CS_SNAPMODULE, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Memory::{VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE};
use windows_sys::Win32::System::Threading::{
    CreateRemoteThread, GetExitCodeThread, OpenProcess, WaitForSingleObject, PROCESS_ALL_ACCESS,
};

const GAME: &str = "H1Z1.exe";
const PAYLOAD: &str = "unlock_all.dll";
const PORT: &str = "127.0.0.1:27015";

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

fn find_process(exe: &str) -> Option<u32> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = None;
        let mut ok = Process32FirstW(snapshot, &mut entry) != 0;
        while ok {
            let name = String::from_utf16_lossy(&entry.szExeFile);
            if name.trim_end_matches('\0').eq_ignore_ascii_case(exe) {
                found = Some(entry.th32ProcessID);
                break;
            }
            ok = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
        found
    }
}

fn module_loaded(pid: u32, module: &str) -> bool {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid);
        if snapshot == INVALID_HANDLE_VALUE {
            return false;
        }
        let mut entry: MODULEENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
        let mut loaded = false;
        let mut ok = Module32FirstW(snapshot, &mut entry) != 0;
        while ok {
            let name = String::from_utf16_lossy(&entry.szModule);
            if name.trim_end_matches('\0').eq_ignore_ascii_case(module) {
                loaded = true;
                break;
            }
            ok = Module32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
        loaded
    }
}

/// Loads the payload into the process with a remote LoadLibraryW thread.
fn inject(pid: u32, dll: &Path) -> Result<(), String> {
    unsafe {
        let process: HANDLE = OpenProcess(PROCESS_ALL_ACCESS, FALSE, pid);
        if process.is_null() {
            return Err(format!(
                "OpenProcess failed with error {} (run as administrator if the game is elevated)",
                GetLastError()
            ));
        }
        let path = wide(dll.as_os_str());
        let bytes = path.len() * 2;
        let remote = VirtualAllocEx(process, ptr::null(), bytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
        if remote.is_null() {
            CloseHandle(process);
            return Err("VirtualAllocEx failed".into());
        }
        let mut written = 0usize;
        if WriteProcessMemory(process, remote, path.as_ptr().cast(), bytes, &mut written) == 0 || written != bytes {
            CloseHandle(process);
            return Err("WriteProcessMemory failed".into());
        }
        let kernel32 = GetModuleHandleW(wide(OsStr::new("kernel32.dll")).as_ptr());
        let load_library = GetProcAddress(kernel32, c"LoadLibraryW".as_ptr().cast()).expect("LoadLibraryW");
        let start: unsafe extern "system" fn(*mut core::ffi::c_void) -> u32 = std::mem::transmute(load_library);
        let thread = CreateRemoteThread(process, ptr::null(), 0, Some(start), remote, 0, ptr::null_mut());
        if thread.is_null() {
            CloseHandle(process);
            return Err(format!("CreateRemoteThread failed with error {}", GetLastError()));
        }
        let waited = WaitForSingleObject(thread, 15_000);
        let mut module = 0u32;
        GetExitCodeThread(thread, &mut module);
        CloseHandle(thread);
        VirtualFreeEx(process, remote, 0, MEM_RELEASE);
        CloseHandle(process);
        if waited != WAIT_OBJECT_0 {
            return Err("LoadLibraryW did not return within 15 s".into());
        }
        if module == 0 {
            return Err("LoadLibraryW returned NULL: the payload refused to load (see unlock_all.log next to it)".into());
        }
        Ok(())
    }
}

/// Attaches to the payload's port: prints the current session's log, streams
/// new lines, and sends typed lines as commands.
fn attach(dll: &Path) -> Result<(), String> {
    if let Ok(log) = std::fs::read_to_string(dll.with_file_name("unlock_all.log")) {
        let session = log.rsplit("---- unlock-all attached").next().unwrap_or("");
        for line in session.lines() {
            println!("  {line}");
        }
    }
    let mut stream = None;
    for _ in 0..100 {
        if let Ok(s) = TcpStream::connect(PORT) {
            stream = Some(s);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let Some(stream) = stream else {
        return Err(format!("the payload's port ({PORT}) did not open; see the log above"));
    };
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    writer.write_all(b"tail\n").map_err(|e| e.to_string())?;

    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            match line.strip_prefix("log ") {
                Some(l) => println!("  {l}"),
                None if line == "." => {}
                None => println!("> {line}"),
            }
        }
        println!("connection closed");
        std::process::exit(0);
    });

    println!("---- live. commands: ping | read <hex> <n> | u32 <hex> | u64 <hex> | jobs | run <job> | quit ----");
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "quit" || line == "exit" {
            break;
        }
        if writer.write_all(format!("{line}\n").as_bytes()).is_err() {
            break;
        }
    }
    Ok(())
}

fn pause() {
    println!("\npress Enter to close");
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
}

fn run() -> Result<(), String> {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default();
    let dll = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| exe_dir.join(PAYLOAD));
    let dll = dll.canonicalize().unwrap_or(dll);
    if !dll.exists() {
        return Err(format!("payload not found: {}", dll.display()));
    }
    let pid = find_process(GAME).ok_or_else(|| format!("{GAME} is not running"))?;
    let name = dll.file_name().unwrap_or_default().to_string_lossy();
    if module_loaded(pid, &name) {
        println!("{name} is already loaded in pid {pid}; attaching");
    } else {
        inject(pid, &dll)?;
        println!("injected {} into pid {pid}", dll.display());
    }
    attach(&dll)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        pause();
        std::process::exit(1);
    }
}
