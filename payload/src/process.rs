//! Process-level primitives: the image base, unaligned reads, pointer sanity
//! and the hook-site byte check.

use std::sync::LazyLock;

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{VirtualProtect, PAGE_EXECUTE_READWRITE};

use crate::offsets::{Patch, Site};

static BASE: LazyLock<usize> = LazyLock::new(|| unsafe { GetModuleHandleW(std::ptr::null()) as usize });

/// The game image's base address.
pub fn base() -> usize {
    *BASE
}

/// Virtual address of an image RVA.
pub fn va(rva: usize) -> usize {
    base() + rva
}

pub unsafe fn rd_u64(addr: usize) -> u64 {
    std::ptr::read_unaligned(addr as *const u64)
}

pub unsafe fn rd_u32(addr: usize) -> u32 {
    std::ptr::read_unaligned(addr as *const u32)
}

pub unsafe fn rd_u8(addr: usize) -> u8 {
    std::ptr::read_unaligned(addr as *const u8)
}

/// True for a plausible user-mode heap or image pointer.
pub fn user_ptr(p: u64) -> bool {
    (0x10000..0x7FFF_FFFF_FFFF).contains(&p)
}

/// The bytes at a hook site, when they differ from the supported build's.
pub unsafe fn mismatch(site: &Site) -> Option<Vec<u8>> {
    let got = std::slice::from_raw_parts(va(site.rva) as *const u8, site.head.len());
    if got == site.head {
        None
    } else {
        Some(got.to_vec())
    }
}

/// Writes a patch over its original bytes.
pub unsafe fn apply_patch(patch: &Patch) -> Result<(), String> {
    let addr = va(patch.rva);
    let len = patch.original.len();
    let current = std::slice::from_raw_parts(addr as *const u8, len);
    if current == patch.patched {
        return Ok(());
    }
    if current != patch.original {
        return Err(format!(
            "{}: bytes at {:#x} are {:02X?}, expected {:02X?}",
            patch.name, patch.rva, current, patch.original
        ));
    }
    let mut previous = 0;
    if VirtualProtect(addr as *const _, len, PAGE_EXECUTE_READWRITE, &mut previous) == 0 {
        return Err(format!("{}: VirtualProtect failed", patch.name));
    }
    std::ptr::copy_nonoverlapping(patch.patched.as_ptr(), addr as *mut u8, len);
    VirtualProtect(addr as *const _, len, previous, &mut previous);
    Ok(())
}
