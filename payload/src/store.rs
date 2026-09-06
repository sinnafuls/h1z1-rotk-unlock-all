//! The persisted choices: prototype -> (skin item, output item), one line per
//! prototype as `prototype=skin=output`, read once at attach and rewritten on
//! every change.
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use parking_lot::Mutex;

struct Store {
    path: PathBuf,
    map: HashMap<u32, (u32, u32)>,
}

static STORE: OnceLock<Mutex<Store>> = OnceLock::new();

pub fn init(path: PathBuf) {
    let mut map = HashMap::new();
    if let Ok(text) = fs::read_to_string(&path) {
        for line in text.lines() {
            let mut it = line.split('=').map(|s| s.trim().parse::<u32>().ok());
            if let (Some(Some(p)), Some(Some(s)), Some(Some(o))) = (it.next(), it.next(), it.next()) {
                if p != 0 && s != 0 {
                    map.insert(p, (s, o));
                }
            }
        }
    }
    let _ = STORE.set(Mutex::new(Store { path, map }));
}

/// Every (prototype, skin) choice.
pub fn all() -> Vec<(u32, u32)> {
    STORE
        .get()
        .map(|s| s.lock().map.iter().map(|(p, (skin, _))| (*p, *skin)).collect())
        .unwrap_or_default()
}

pub fn len() -> usize {
    STORE.get().map(|s| s.lock().map.len()).unwrap_or(0)
}

/// The (skin item, output item) chosen for a prototype.
pub fn choice_for(prototype: u32) -> Option<(u32, u32)> {
    STORE.get()?.lock().map.get(&prototype).copied()
}

/// The output item a prototype's definition lookup resolves to, when the
/// choice is not the default look.
pub fn output_for(prototype: u32) -> Option<u32> {
    let (_, output) = choice_for(prototype)?;
    (output != 0 && output != prototype).then_some(output)
}

pub fn set(prototype: u32, skin: u32, output: u32) {
    let Some(store) = STORE.get() else { return };
    let mut store = store.lock();
    store.map.insert(prototype, (skin, output));
    let mut lines: Vec<String> = store.map.iter().map(|(p, (s, o))| format!("{p}={s}={o}")).collect();
    lines.sort();
    let _ = fs::write(&store.path, lines.join("\n") + "\n");
}
