//! Every address the payload touches, as an RVA from the H1Z1.exe image base.
//!
//! A hook site carries the first bytes the supported build has at that
//! address. A hook is refused when the bytes differ (the game was updated and
//! the code moved), and in that case nothing is installed.

pub struct Site {
    pub name: &'static str,
    pub rva: usize,
    pub head: &'static [u8],
}

// ---- hooked -------------------------------------------------------------------

/// `AccountItemManager::IsOwned(mgr, &itemId)`: the equip gate and the
/// account-item data source's IsOwned field.
pub const IS_OWNED: Site = Site {
    name: "IsOwned",
    rva: 0x1129D80,
    head: &[0x48, 0x89, 0x5C, 0x24, 0x08, 0x57, 0x48, 0x83],
};

/// The account-item data source's AccountItemCount getter.
pub const ACCT_COUNT: Site = Site {
    name: "AccountItemCount",
    rva: 0x19F47A0,
    head: &[0x48, 0x89, 0x5C, 0x24, 0x08, 0x48, 0x89, 0x74],
};

/// `ClientSkinItemManager::UiSetSkinItemByItemId(mgr, &collectionId,
/// &prototypeItemId, &itemId) -> bool`: the equip path of the cosmetics grid.
/// The original checks ownership, sends a SetSkinItem request and previews.
pub const UI_SET_BY_ITEM: Site = Site {
    name: "UiSetSkinItemByItemId",
    rva: 0x1150420,
    head: &[0x48, 0x8B, 0xC4, 0x41, 0x56, 0x48, 0x83, 0xEC],
};

/// `ClientSkinItemManager::UiPreviewSkinItem(mgr, &itemId)`: the preview path
/// of the cosmetics grid (a click on a tile the grid does not consider owned).
pub const UI_PREVIEW_SKIN: Site = Site {
    name: "UiPreviewSkinItem",
    rva: 0x1151CA0,
    head: &[
        0x48, 0x89, 0x5C, 0x24, 0x08, 0x57, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xDA, 0x48, 0x8B, 0xF9,
    ],
};

/// Skin collection insert: `(collections, &collectionId, &entry, inZone) -> entry`.
/// The path the server's SetSkinItem packet takes; the lobby character
/// re-dresses from it.
pub const COLL_INSERT: Site = Site {
    name: "CollectionInsert",
    rva: 0x289E200,
    head: &[0x48, 0x89, 0x5C, 0x24, 0x08, 0x48, 0x89, 0x6C],
};

/// `ClientCharacterEquipmentManager::UpdateProxiedCharacterAttachments(mgr,
/// &playerId, ...)`: applies the server's attachment data for every slot of
/// an equipment packet.
pub const UPD_PROXIED_MANY: Site = Site {
    name: "UpdateProxiedCharacterAttachments",
    rva: 0x10F1BF0,
    head: &[0x48, 0x8B, 0xC4, 0x55, 0x41, 0x56, 0x41, 0x57],
};

/// `ClientCharacterEquipmentManager::UpdateProxiedCharacterAttachment(mgr,
/// &playerId, record, attachmentData)`: the single-slot variant.
pub const UPD_PROXIED_ONE: Site = Site {
    name: "UpdateProxiedCharacterAttachment",
    rva: 0x10F1A30,
    head: &[0x48, 0x89, 0x5C, 0x24, 0x18, 0x48, 0x89, 0x74],
};

/// The account-item data source's populate: `(source, force)`. One row per
/// account item held; the cosmetics grid joins against it to decide the lock
/// badge.
pub const ACCT_POPULATE: Site = Site {
    name: "AccountItemsPopulate",
    rva: 0x19F1D40,
    head: &[
        0x40, 0x57, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xF9, 0x84, 0xD2, 0x75, 0x09, 0x38, 0x51, 0x10,
    ],
};

/// The two helpers the row adder stores an account-item row's count and
/// owned columns from: `count = A(mgr+1176, &id) + B(mgr, &id)`,
/// `owned = C(mgr, &id) && count > 0`.
pub const ACCT_COUNT_B: Site = Site {
    name: "AccountItemCountB",
    rva: 0x27C77C0,
    head: &[
        0x44, 0x8B, 0x12, 0x4C, 0x8B, 0xC1, 0x48, 0xB8, 0x11, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08,
    ],
};
pub const ACCT_OWNED_C: Site = Site {
    name: "AccountItemOwnedC",
    rva: 0x1129E90,
    head: &[
        0x44, 0x8B, 0x0A, 0x4C, 0x8B, 0xC1, 0x48, 0xB8, 0x93, 0x24, 0x49, 0x92, 0x24, 0x49, 0x92, 0x24,
    ],
};

/// `ClientSkinItemManager::HandleZoneDoneSendingInitialData(mgr)`: the end of
/// every server rebuild of the skin collection (lobby return, match join).
pub const ZONE_DONE: Site = Site {
    name: "HandleZoneDoneSendingInitialData",
    rva: 0x114DE40,
    head: &[0x40, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x8B, 0xD9, 0xE8],
};

/// The game's logger, `Log(ctx, fmt, ...)`. Mirrored so the console shows the
/// skin and equipment managers' own trace.
pub const GAME_LOG: Site = Site {
    name: "GameLog",
    rva: 0xDEB1E0,
    head: &[
        0x48, 0x89, 0x54, 0x24, 0x10, 0x4C, 0x89, 0x44, 0x24, 0x18, 0x4C, 0x89, 0x4C, 0x24, 0x20,
    ],
};

// ---- called ---------------------------------------------------------------------

/// Refreshers run after a collection change, in the order the SetSkinItem
/// packet handler runs them.
pub const REFRESH_A: usize = 0x1A08C70;
pub const REFRESH_CUR: usize = 0x1A08D10;
pub const REFRESH_C1: usize = 0x1A09040;
pub const REFRESH_C2: usize = 0x1A08F20;
pub const MAP2_INSERT: usize = 0x1A03740;

/// `PreviewSkinItem(mgr, &itemId, saveToCache)`: resolves a skin item to its
/// output item and dresses the lobby character.
pub const PREVIEW_SKIN: usize = 0x114EED0;

/// Skin family record by skin item id; `+0x10` is the output item.
pub const SKIN_RECORD: usize = 0x114AE20;

/// `AddAccountItemRow(source, definitions, accountItemManager, &definitionId)`.
pub const ACCT_ADD_ROW: usize = 0x19F1330;

// ---- globals --------------------------------------------------------------------

/// The item-definition provider; its vtable slot 0 is "definition by id".
pub const ITEM_DEF_PROVIDER: usize = 0x476E428;
/// The item-definition table the row adder resolves ids in.
pub const ITEM_DEFS: usize = 0x476E418;
/// The client object; `+CLIENT_IN_ZONE == 4` while in a zone.
pub const CLIENT_SLOT: usize = 0x476DC08;
pub const CLIENT_IN_ZONE: usize = 0x389DC;
/// The inventory object; `+INVENTORY_LOCAL_GUID` is the local character's guid.
pub const INVENTORY: usize = 0x476DF38;
pub const INVENTORY_LOCAL_GUID: usize = 0xD8;
/// The account-item manager inside the inventory, and its UiDb data source.
pub const ACCT_MGR_OFF: usize = 0xE8B0;
pub const ACCT_SOURCE_OFF: usize = 0x8A0;
/// The UI collection list: head at `+0x58`, item = node + 8, prototype nodes
/// at item `+72` (id at `+12`, next at `+40`), next item node at item `+0xF8`.
pub const UI_COLLECTIONS: usize = 0x476DD38;
/// The skin manager; `+SKIN_MGR_FAMILY_MAP` is a 2048-bucket multimap of
/// prototype id -> skin item id (node: key, value, next at `+32`).
pub const SKIN_MGR: usize = 0x476DE38;
pub const SKIN_MGR_FAMILY_MAP: usize = 0x8E0;
/// The local player id and the collection-version threshold the refreshers test.
pub const LOCAL_PLAYER_ID: usize = 0x478FDB0;
pub const COLL_VERSION: usize = 0x4798958;
/// The current skin collection id.
pub const CURRENT_COLLECTION: usize = 0x4446688;

// ---- ClientSkinItemManager fields -------------------------------------------------

pub const MGR_COLLECTIONS: usize = 56;
pub const MGR_EDITOR_COLLECTION: usize = 280;
pub const MGR_COLLECTION_VERSION: usize = 288;
pub const MGR_EDITOR_PROTOTYPE: usize = 292;
pub const MGR_REFRESH_A: usize = 360;
pub const MGR_REFRESH_CUR: usize = 368;
pub const MGR_REFRESH_C: usize = 408;
pub const MGR_MAP2: usize = 536;
pub const MGR_EDITOR_MODE: usize = 0x230;
pub const EDITOR_MODE_CRATE_PREVIEW: u32 = 4;
