//! Process-level primitives: the image base, unaligned reads, pointer sanity
//! and the hook-site byte check.

use std::sync::LazyLock;

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

use crate::offsets::Site;

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
