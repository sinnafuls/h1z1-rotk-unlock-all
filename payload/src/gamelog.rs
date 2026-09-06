//! The game's own log, mirrored: every line the skin and equipment managers
//! report is formatted and written to the payload log, then forwarded.

use std::ffi::CStr;
use std::sync::OnceLock;

use retour::GenericDetour;

use crate::{hook, offsets, process::user_ptr};

/// `Log(ctx, fmt, ...)`; the variadic arguments are taken as the register
/// and stack slots a printf-style call leaves.
type FnLog = unsafe extern "C" fn(usize, *const u8, usize, usize, usize, usize, usize, usize) -> usize;

static LOG: OnceLock<GenericDetour<FnLog>> = OnceLock::new();

pub unsafe fn install() -> Result<(), String> {
    hook!(LOG, offsets::GAME_LOG, FnLog, hk_log);
    Ok(())
}

fn wanted(fmt: &str) -> bool {
    fmt.starts_with("ClientSkinItemManager")
        || fmt.starts_with("ClientCharacterEquipmentManager")
        || fmt.contains("Skin")
        || fmt.contains("AccountItem")
}

unsafe extern "C" fn hk_log(
    ctx: usize,
    fmt: *const u8,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
) -> usize {
    if !fmt.is_null() {
        let f = CStr::from_ptr(fmt as *const _).to_string_lossy();
        if wanted(&f) {
            crate::log(&format!("game: {}", format(&f, &[a1, a2, a3, a4, a5, a6])));
        }
    }
    LOG.get().unwrap().call(ctx, fmt, a1, a2, a3, a4, a5, a6)
}

/// Expands `%d %u %x %s %I64u %llu %%` with the given argument slots.
unsafe fn format(fmt: &str, args: &[usize]) -> String {
    let b = fmt.as_bytes();
    let mut out = String::with_capacity(fmt.len() + 32);
    let mut next = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'%' || i + 1 >= b.len() {
            out.push(b[i] as char);
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < b.len() && matches!(b[j], b'l' | b'h' | b'I' | b'.' | b'0'..=b'9') {
            j += 1;
        }
        if j >= b.len() {
            out.push('%');
            i += 1;
            continue;
        }
        let modifiers = &fmt[i + 1..j];
        let wide = modifiers.contains("I64") || modifiers.contains("ll");
        let conv = b[j];
        if conv == b'%' {
            out.push('%');
            i = j + 1;
            continue;
        }
        let arg = args.get(next).copied().unwrap_or(0);
        next += 1;
        match conv {
            b'd' | b'i' if wide => out.push_str(&(arg as i64).to_string()),
            b'd' | b'i' => out.push_str(&(arg as u32 as i32).to_string()),
            b'u' if wide => out.push_str(&(arg as u64).to_string()),
            b'u' => out.push_str(&(arg as u32).to_string()),
            b'x' | b'X' if wide => out.push_str(&format!("{:x}", arg as u64)),
            b'x' | b'X' => out.push_str(&format!("{:x}", arg as u32)),
            b's' if user_ptr(arg as u64) => out.push_str(&CStr::from_ptr(arg as *const _).to_string_lossy()),
            b's' => out.push_str("(null)"),
            _ => out.push_str(&arg.to_string()),
        }
        i = j + 1;
    }
    out
}
