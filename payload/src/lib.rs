//! unlock-all: an in-process cosmetics changer for H1Z1 (ROTK).
//!
//! Every skin in the game's cosmetics screen is selectable, a click applies
//! it locally, the choice persists next to the DLL and is restored after
//! every server rebuild. Nothing is sent to the server, and other players see
//! the server's view.
//!
//! Modules: `skins` (the changer), `rows` (the grid's lock badge), `gamelog`
//! (the game's own trace mirrored into the log), `console` (the command port
//! the injector attaches to), `store` (the persisted choices).

mod console;
mod gamelog;
mod offsets;
mod process;
mod rows;
mod skins;
mod store;

use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use retour::GenericDetour;
use windows_sys::Win32::Foundation::{HMODULE, HWND};
use windows_sys::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows_sys::Win32::System::SystemServices::DLL_PROCESS_ATTACH;
use windows_sys::Win32::System::Threading::CreateThread;
use windows_sys::Win32::UI::WindowsAndMessaging::{PeekMessageW, MSG};

static LOG_PATH: OnceLock<PathBuf> = OnceLock::new();

/// Appends a line to the payload log and to every attached console.
pub fn log(line: &str) {
    console::publish(line);
    if let Some(path) = LOG_PATH.get() {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

#[macro_export]
macro_rules! logf {
    ($($t:tt)*) => { $crate::log(&format!($($t)*)) };
}

/// Installs a detour at a hook site after verifying the site's bytes.
#[macro_export]
macro_rules! hook {
    ($slot:ident, $site:expr, $ty:ty, $hook:expr) => {{
        if let Some(got) = $crate::process::mismatch(&$site) {
            return Err(format!(
                "{}: bytes at {:#x} are {:02X?}, expected {:02X?}",
                $site.name, $site.rva, got, $site.head
            ));
        }
        let target: $ty = std::mem::transmute($crate::process::va($site.rva));
        let detour = retour::GenericDetour::<$ty>::new(target, $hook).map_err(|e| format!("{}: {e}", $site.name))?;
        detour.enable().map_err(|e| format!("{}: {e}", $site.name))?;
        let _ = $slot.set(detour);
        $crate::logf!("hooked {} at {:#x}", $site.name, $site.rva);
    }};
}

type FnPeekMessage = unsafe extern "system" fn(*mut MSG, HWND, u32, u32, u32) -> i32;

static PEEK_MESSAGE: OnceLock<GenericDetour<FnPeekMessage>> = OnceLock::new();

/// The window thread's message pump, where console jobs run.
unsafe extern "system" fn hk_peek_message(msg: *mut MSG, hwnd: HWND, min: u32, max: u32, remove: u32) -> i32 {
    console::drain_on_main();
    PEEK_MESSAGE.get().unwrap().call(msg, hwnd, min, max, remove)
}

unsafe fn install() -> Result<(), String> {
    skins::install()?;
    rows::install()?;
    gamelog::install()?;
    let detour = GenericDetour::<FnPeekMessage>::new(PeekMessageW, hk_peek_message).map_err(|e| format!("PeekMessageW: {e}"))?;
    detour.enable().map_err(|e| format!("PeekMessageW: {e}"))?;
    let _ = PEEK_MESSAGE.set(detour);
    Ok(())
}

fn register_jobs() {
    console::register("populate", |_| unsafe {
        match rows::repopulate() {
            Ok(()) => "repopulated".into(),
            Err(e) => format!("error: {e}"),
        }
    });
    console::register("held", |_| unsafe {
        let source = rows::source();
        if source == 0 {
            return "error: the inventory is not initialised".into();
        }
        let mut ids: Vec<u32> = rows::held_ids(source - offsets::ACCT_SOURCE_OFF).into_iter().collect();
        ids.sort_unstable();
        format!("{} {ids:?}", ids.len())
    });
    console::register("skins", |_| unsafe {
        let ids = rows::skin_ids();
        format!("{} {:?}", ids.len(), &ids[..ids.len().min(40)])
    });
}

unsafe extern "system" fn init(_: *mut c_void) -> u32 {
    // The definition provider is created during startup; wait for it rather
    // than hook a null vtable.
    for _ in 0..600 {
        if process::user_ptr(process::rd_u64(process::va(offsets::ITEM_DEF_PROVIDER))) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    match install() {
        Ok(()) => {
            logf!("unlock-all: {} choices loaded, all hooks installed", store::len());
            register_jobs();
            match console::find_main_thread() {
                Some(tid) => logf!("main thread {tid}"),
                None => logf!("main thread: no visible window found; console jobs will not run"),
            }
            console::serve();
            // The account-item source was populated before the payload was
            // loaded; one repopulate on the main thread adds the skin rows.
            let result = console::run_on_main(
                Box::new(|_| unsafe { rows::repopulate().map(|_| "repopulated".to_string()).unwrap_or_else(|e| e) }),
                vec![],
                10_000,
            );
            logf!("startup populate: {result}");
        }
        Err(e) => logf!("unlock-all: install failed: {e}"),
    }
    0
}

/// # Safety
/// Called by the loader with a valid module handle.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "system" fn DllMain(module: HMODULE, reason: u32, _: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        let mut buf = [0u16; 1024];
        let n = GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) as usize;
        let dir = PathBuf::from(String::from_utf16_lossy(&buf[..n]))
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();
        let _ = LOG_PATH.set(dir.join("unlock_all.log"));
        store::init(dir.join("unlock_all_skins.txt"));
        logf!("---- unlock-all attached, base {:#x}", process::base());
        CreateThread(std::ptr::null(), 0, Some(init), std::ptr::null(), 0, std::ptr::null_mut());
    }
    1
}
