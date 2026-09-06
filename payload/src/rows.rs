//! The cosmetics grid's lock badge.
//!
//! The grid joins every prototype x skin pair against the account-item data
//! source, which holds one row per item the account owns; a skin without a
//! row is drawn locked. After the game populates the source, one row per skin
//! definition is added through the game's own row adder, and the two helpers
//! the adder stores the count and owned columns from answer 1 for those rows.

use std::cell::Cell;
use std::collections::HashSet;
use std::sync::OnceLock;

use retour::GenericDetour;

use crate::{
    hook, logf, offsets,
    process::{rd_u32, rd_u64, rd_u8, user_ptr, va},
};

type FnPopulate = unsafe extern "C" fn(usize, u8);
type FnAddRow = unsafe extern "C" fn(usize, usize, usize, *const u32) -> usize;
type FnCount = unsafe extern "C" fn(usize, *const u32) -> u32;
type FnOwned = unsafe extern "C" fn(usize, *const u32) -> u8;

static POPULATE: OnceLock<GenericDetour<FnPopulate>> = OnceLock::new();
static COUNT_B: OnceLock<GenericDetour<FnCount>> = OnceLock::new();
static OWNED_C: OnceLock<GenericDetour<FnOwned>> = OnceLock::new();

thread_local! {
    static ADDING: Cell<bool> = const { Cell::new(false) };
}

pub unsafe fn install() -> Result<(), String> {
    hook!(POPULATE, offsets::ACCT_POPULATE, FnPopulate, hk_populate);
    hook!(COUNT_B, offsets::ACCT_COUNT_B, FnCount, hk_count_b);
    hook!(OWNED_C, offsets::ACCT_OWNED_C, FnOwned, hk_owned_c);
    Ok(())
}

/// The account-item data source, or 0 before the inventory exists.
pub unsafe fn source() -> usize {
    let inventory = rd_u64(va(offsets::INVENTORY)) as usize;
    if !user_ptr(inventory as u64) {
        return 0;
    }
    inventory + offsets::ACCT_MGR_OFF + offsets::ACCT_SOURCE_OFF
}

/// Repopulates the source, which runs the hook and adds the skin rows. Must
/// run on the game's main thread.
pub unsafe fn repopulate() -> Result<(), String> {
    let source = source();
    if source == 0 {
        return Err("the inventory is not initialised".into());
    }
    let populate: FnPopulate = std::mem::transmute(va(offsets::ACCT_POPULATE.rva));
    populate(source, 1);
    Ok(())
}

unsafe extern "C" fn hk_populate(source: usize, force: u8) {
    let repopulating = force != 0 || rd_u8(source + 16) == 0;
    POPULATE.get().unwrap().call(source, force);
    if !repopulating {
        return;
    }
    let definitions = rd_u64(va(offsets::ITEM_DEFS)) as usize;
    let inventory = rd_u64(va(offsets::INVENTORY)) as usize;
    if !user_ptr(definitions as u64) || !user_ptr(inventory as u64) {
        logf!("populate: the item definitions or the inventory are not initialised");
        return;
    }
    let manager = inventory + offsets::ACCT_MGR_OFF;
    let held = held_ids(manager);
    let add_row: FnAddRow = std::mem::transmute(va(offsets::ACCT_ADD_ROW));
    let mut added = 0;
    ADDING.with(|f| f.set(true));
    for id in skin_ids() {
        if !held.contains(&id) {
            add_row(source, definitions, manager, &id);
            added += 1;
        }
    }
    ADDING.with(|f| f.set(false));
    logf!("populate: {} items held, {added} skin rows added", held.len());
}

unsafe extern "C" fn hk_count_b(manager: usize, id: *const u32) -> u32 {
    let count = COUNT_B.get().unwrap().call(manager, id);
    if count == 0 && ADDING.with(Cell::get) {
        1
    } else {
        count
    }
}

unsafe extern "C" fn hk_owned_c(manager: usize, id: *const u32) -> u8 {
    let owned = OWNED_C.get().unwrap().call(manager, id);
    if owned == 0 && ADDING.with(Cell::get) {
        1
    } else {
        owned
    }
}

/// Definition ids the account-item manager holds, from both of its item lists.
pub unsafe fn held_ids(manager: usize) -> HashSet<u32> {
    let mut held = HashSet::new();
    for (head, next) in [(24usize, 216usize), (1192, 280)] {
        let mut node = rd_u64(manager + head) as usize;
        let mut guard = 0;
        while user_ptr(node as u64) && guard < 4096 {
            guard += 1;
            let item = node + 8;
            held.insert(rd_u32(item + 200));
            node = rd_u64(item + next) as usize;
        }
    }
    held
}

/// Every skin definition the cosmetics screen can show: the UI collection
/// list's prototypes, each expanded through the skin manager's family map.
pub unsafe fn skin_ids() -> Vec<u32> {
    let mut ids = Vec::new();
    let collections = rd_u64(va(offsets::UI_COLLECTIONS)) as usize;
    let skin_manager = rd_u64(va(offsets::SKIN_MGR)) as usize;
    if !user_ptr(collections as u64) || !user_ptr(skin_manager as u64) {
        return ids;
    }
    let mut seen = HashSet::new();
    let mut node = rd_u64(collections + 0x58) as usize;
    let mut guard = 0;
    while user_ptr(node as u64) && guard < 64 {
        guard += 1;
        let item = node + 8;
        let mut prototype_node = rd_u64(item + 72) as usize;
        let mut guard2 = 0;
        while user_ptr(prototype_node as u64) && guard2 < 128 {
            guard2 += 1;
            let prototype = rd_u32(prototype_node + 12);
            let bucket = skin_manager + offsets::SKIN_MGR_FAMILY_MAP + 8 * (prototype as usize & 0x7FF);
            let mut family_node = rd_u64(bucket) as usize;
            let mut guard3 = 0;
            while user_ptr(family_node as u64) && guard3 < 4096 {
                guard3 += 1;
                if rd_u32(family_node) == prototype {
                    let id = rd_u32(family_node + 4);
                    if id != 0 && seen.insert(id) {
                        ids.push(id);
                    }
                }
                family_node = rd_u64(family_node + 32) as usize;
            }
            prototype_node = rd_u64(prototype_node + 40) as usize;
        }
        node = rd_u64(item + 0xF8) as usize;
    }
    ids
}
