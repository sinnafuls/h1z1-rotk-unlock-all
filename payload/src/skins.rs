//! The cosmetics changer.
//!
//! The game renders an item from its definition id, and a skinned item is
//! another definition of the same family. A choice is therefore applied in
//! three places:
//!
//! - the skin collection, which the lobby character is dressed from, is
//!   written the way the server's SetSkinItem packet writes it, followed by
//!   the same refreshers;
//! - the definition lookup translates a prototype with a choice to the
//!   chosen output definition, so the stowed model, HUD and inventory agree;
//! - in both equipment packet handlers the local player's attachments are
//!   left as the client built them instead of being replaced by the
//!   server's standard models.
//!
//! Choices persist in the store and are re-applied after every server
//! rebuild of the collection. Nothing is sent to the server.

use std::sync::OnceLock;

use retour::GenericDetour;

use crate::{
    hook, logf, offsets,
    process::{rd_u32, rd_u64, user_ptr, va},
    store,
};

type FnIsOwned = unsafe extern "C" fn(usize, *const u32) -> u8;
type FnCount = unsafe extern "C" fn(usize, *const u32) -> u32;
type FnDefById = unsafe extern "C" fn(usize, u32) -> usize;
type FnUiSet = unsafe extern "C" fn(usize, *const u32, *const u32, *const u32) -> u8;
type FnUiPreview = unsafe extern "C" fn(usize, *const u32) -> usize;
type FnCollInsert = unsafe extern "C" fn(usize, *const u32, *mut CollEntry, u8) -> usize;
type FnZoneDone = unsafe extern "C" fn(usize) -> usize;
type FnRefresh = unsafe extern "C" fn(usize) -> usize;
type FnRefreshCur = unsafe extern "C" fn(usize, *const u32) -> usize;
type FnMap2Insert = unsafe extern "C" fn(usize, *const u32, *const u32) -> usize;
type FnPreview = unsafe extern "C" fn(usize, *const u32, u8) -> u8;
type FnSkinRecord = unsafe extern "C" fn(*const u32) -> usize;

/// A skin collection entry as the insert receives it.
#[repr(C)]
struct CollEntry {
    prototype: u32,
    _pad: u32,
    instance: u64,
    item: u32,
    flag: u8,
    _tail: [u8; 3],
}

static IS_OWNED: OnceLock<GenericDetour<FnIsOwned>> = OnceLock::new();
static COUNT: OnceLock<GenericDetour<FnCount>> = OnceLock::new();
static DEF_BY_ID: OnceLock<GenericDetour<FnDefById>> = OnceLock::new();
static UI_SET: OnceLock<GenericDetour<FnUiSet>> = OnceLock::new();
static UI_PREVIEW: OnceLock<GenericDetour<FnUiPreview>> = OnceLock::new();
static COLL_INSERT: OnceLock<GenericDetour<FnCollInsert>> = OnceLock::new();
static ZONE_DONE: OnceLock<GenericDetour<FnZoneDone>> = OnceLock::new();

pub unsafe fn install() -> Result<(), String> {
    hook!(IS_OWNED, offsets::IS_OWNED, FnIsOwned, hk_is_owned);
    hook!(COUNT, offsets::ACCT_COUNT, FnCount, hk_count);
    hook!(UI_SET, offsets::UI_SET_BY_ITEM, FnUiSet, hk_ui_set);
    hook!(UI_PREVIEW, offsets::UI_PREVIEW_SKIN, FnUiPreview, hk_ui_preview);
    hook!(COLL_INSERT, offsets::COLL_INSERT, FnCollInsert, hk_coll_insert);
    hook!(ZONE_DONE, offsets::ZONE_DONE, FnZoneDone, hk_zone_done);
    for patch in [&offsets::DRAW_PATCH, &offsets::EQUIP_PATCH] {
        crate::process::apply_patch(patch)?;
        logf!("patched {} at {:#x}", patch.name, patch.rva);
    }
    install_def_by_id()
}

/// Definition-by-id is the provider's first virtual method, resolved from the
/// live object rather than a fixed address.
unsafe fn install_def_by_id() -> Result<(), String> {
    let provider = rd_u64(va(offsets::ITEM_DEF_PROVIDER)) as usize;
    if !user_ptr(provider as u64) {
        return Err("item definition provider is not initialised".into());
    }
    let target = rd_u64(rd_u64(provider) as usize) as usize;
    if !(crate::process::base()..crate::process::base() + 0x8000000).contains(&target) {
        return Err(format!("definition-by-id target {target:#x} is outside the game image"));
    }
    let target: FnDefById = std::mem::transmute(target);
    let detour = GenericDetour::<FnDefById>::new(target, hk_def_by_id).map_err(|e| format!("DefById: {e}"))?;
    detour.enable().map_err(|e| format!("DefById: {e}"))?;
    let _ = DEF_BY_ID.set(detour);
    logf!("hooked DefById (provider vtable[0])");
    Ok(())
}

unsafe extern "C" fn hk_is_owned(_mgr: usize, _id: *const u32) -> u8 {
    1
}

unsafe extern "C" fn hk_count(_mgr: usize, _id: *const u32) -> u32 {
    1
}

unsafe extern "C" fn hk_def_by_id(provider: usize, id: u32) -> usize {
    let id = store::output_for(id).unwrap_or(id);
    DEF_BY_ID.get().unwrap().call(provider, id)
}

/// Every insert into the collection, including the server's rebuilds, keeps
/// the chosen skin for a prototype that has one.
unsafe extern "C" fn hk_coll_insert(collections: usize, collection: *const u32, entry: *mut CollEntry, in_zone: u8) -> usize {
    if let Some(entry) = entry.as_mut() {
        if let Some((skin, _)) = store::choice_for(entry.prototype) {
            entry.item = skin;
        }
    }
    COLL_INSERT.get().unwrap().call(collections, collection, entry, in_zone)
}

/// The equip path: the local apply, then the preview the original ends with.
unsafe extern "C" fn hk_ui_set(mgr: usize, collection: *const u32, prototype: *const u32, item: *const u32) -> u8 {
    let (Some(&c), Some(&p), Some(&i)) = (collection.as_ref(), prototype.as_ref(), item.as_ref()) else {
        return 0;
    };
    if !apply(mgr, c, p, i) {
        return 0;
    }
    let preview: FnPreview = std::mem::transmute(va(offsets::PREVIEW_SKIN));
    preview(mgr, item, 1);
    1
}

/// The preview path: the game's preview, then the apply for the editor's
/// target prototype.
unsafe extern "C" fn hk_ui_preview(mgr: usize, item: *const u32) -> usize {
    let r = UI_PREVIEW.get().unwrap().call(mgr, item);
    let Some(&i) = item.as_ref() else { return r };
    if rd_u32(mgr + offsets::MGR_EDITOR_MODE) == offsets::EDITOR_MODE_CRATE_PREVIEW {
        return r;
    }
    let c = rd_u32(mgr + offsets::MGR_EDITOR_COLLECTION);
    let p = rd_u32(mgr + offsets::MGR_EDITOR_PROTOTYPE);
    if c == 0 || p == 0 {
        logf!("click: item {i} without an editor target (collection {c}, prototype {p})");
        return r;
    }
    apply(mgr, c, p, i);
    r
}

/// After a server rebuild of the collection every stored choice is applied again.
unsafe extern "C" fn hk_zone_done(mgr: usize) -> usize {
    let r = ZONE_DONE.get().unwrap().call(mgr);
    let c = rd_u32(va(offsets::CURRENT_COLLECTION));
    if c == 0 {
        logf!("zone done: no current collection, choices not re-applied");
        return r;
    }
    let applied = store::all().into_iter().filter(|&(p, skin)| apply(mgr, c, p, skin)).count();
    logf!("zone done: {applied} choices re-applied to collection {c}");
    r
}

/// The SetSkinItem packet handler's body: the collection insert and its
/// refreshers, with the choice persisted first so the insert hook honours it.
unsafe fn apply(mgr: usize, collection: u32, prototype: u32, skin: u32) -> bool {
    if prototype == 0 || skin == 0 {
        return false;
    }
    let skin_record: FnSkinRecord = std::mem::transmute(va(offsets::SKIN_RECORD));
    let record = skin_record(&skin);
    if record == 0 {
        logf!("apply: {skin} is not a skin item");
        return false;
    }
    let output = rd_u32(record + 0x10);
    logf!("apply: collection {collection} prototype {prototype} skin {skin} -> output {output}");
    store::set(prototype, skin, output);

    let client = rd_u64(va(offsets::CLIENT_SLOT)) as usize;
    let in_zone = user_ptr(client as u64) && rd_u32(client + offsets::CLIENT_IN_ZONE) == 4;
    let mut entry = CollEntry {
        prototype,
        _pad: 0,
        instance: 0,
        item: skin,
        flag: 0,
        _tail: [0; 3],
    };
    if COLL_INSERT
        .get()
        .unwrap()
        .call(mgr + offsets::MGR_COLLECTIONS, &collection, &mut entry, in_zone as u8)
        == 0
    {
        logf!("apply: the collection refused the entry");
        return false;
    }

    let refresh_a: FnRefresh = std::mem::transmute(va(offsets::REFRESH_A));
    let refresh_cur: FnRefreshCur = std::mem::transmute(va(offsets::REFRESH_CUR));
    let refresh_c1: FnRefresh = std::mem::transmute(va(offsets::REFRESH_C1));
    let refresh_c2: FnRefresh = std::mem::transmute(va(offsets::REFRESH_C2));
    let map2_insert: FnMap2Insert = std::mem::transmute(va(offsets::MAP2_INSERT));

    let a = rd_u64(mgr + offsets::MGR_REFRESH_A) as usize;
    if a != 0 {
        refresh_a(a);
    }
    if collection == rd_u32(mgr + offsets::MGR_EDITOR_COLLECTION) {
        let cur = rd_u64(mgr + offsets::MGR_REFRESH_CUR) as usize;
        if cur != 0 {
            refresh_cur(cur, &collection);
        }
    }
    let character = rd_u64(mgr + offsets::MGR_REFRESH_C) as usize;
    if character != 0 {
        if rd_u32(va(offsets::COLL_VERSION)) >= rd_u32(mgr + offsets::MGR_COLLECTION_VERSION) {
            refresh_c1(character);
        } else {
            refresh_c2(character);
        }
    }
    map2_insert(mgr + offsets::MGR_MAP2, &collection, &prototype);
    true
}
