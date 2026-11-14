//! Item data: a small string map any stack can carry (a charge, an owner, a
//! name a mod gave it). A stack holds only a handle (`Stack::data`, 0 for
//! none) so stacks stay `Copy`; the maps live in a table here, each different
//! map once. Two stacks with the same handle carry the same data, so stacks
//! with different data never merge.
//!
//! Limits: at most 8 keys, a key is 1..16 of letters, digits, `_`, `-` and
//! `.`, a value is up to 64 bytes without control characters, and at most
//! 4096 different maps are alive in one run (a world load starts afresh).
//! Maps are kept as text in saves (see `stack_text.rs`).

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

pub const MAX_KEYS: usize = 8;
pub const MAX_KEY: usize = 16;
pub const MAX_VALUE: usize = 64;
pub const MAX_MAPS: usize = 4096;

/// The pairs of one map, sorted by key.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct DataMap(pub Vec<(String, String)>);

impl DataMap {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|p| p.0 == key).map(|p| p.1.as_str())
    }

    /// Set a key. An error says what is wrong with the key or value, or that
    /// the map is full.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        check_key(key)?;
        check_value(value)?;
        match self.0.iter_mut().find(|p| p.0 == key) {
            Some(p) => p.1 = String::from(value),
            None => {
                if self.0.len() >= MAX_KEYS {
                    return Err(String::from("item data holds at most 8 keys"));
                }
                self.0.push((String::from(key), String::from(value)));
                self.0.sort();
            }
        }
        Ok(())
    }

    /// Take a key away; whether it was there.
    pub fn clear(&mut self, key: &str) -> bool {
        let before = self.0.len();
        self.0.retain(|p| p.0 != key);
        self.0.len() != before
    }
}

pub fn check_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > MAX_KEY || !key.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')) {
        return Err(String::from("an item data key is 1 to 16 letters, digits, `_`, `-` or `.`"));
    }
    Ok(())
}

pub fn check_value(value: &str) -> Result<(), String> {
    if value.len() > MAX_VALUE || value.chars().any(|c| c.is_control()) {
        return Err(String::from("an item data value is at most 64 bytes with no control characters"));
    }
    Ok(())
}

struct Table {
    maps: Vec<DataMap>,
    ids: BTreeMap<DataMap, u16>,
}

/// A spin lock round the table (the game has meshing threads; the tests run in parallel).
struct Locked {
    busy: AtomicBool,
    table: UnsafeCell<Option<Table>>,
}

// SAFETY: every access takes the `busy` flag first.
unsafe impl Sync for Locked {}

static TABLE: Locked = Locked { busy: AtomicBool::new(false), table: UnsafeCell::new(None) };

fn with_table<R>(f: impl FnOnce(&mut Table) -> R) -> R {
    while TABLE.busy.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    // SAFETY: the flag is held.
    let t = unsafe { &mut *TABLE.table.get() }.get_or_insert_with(|| Table { maps: Vec::new(), ids: BTreeMap::new() });
    let r = f(t);
    TABLE.busy.store(false, Ordering::Release);
    r
}

/// The handle of a map (0 for an empty one); `None` when the table is full.
pub fn intern(map: &DataMap) -> Option<u16> {
    if map.0.is_empty() {
        return Some(0);
    }
    with_table(|t| {
        if let Some(&h) = t.ids.get(map) {
            return Some(h);
        }
        if t.maps.len() >= MAX_MAPS {
            return None;
        }
        t.maps.push(map.clone());
        let h = t.maps.len() as u16;
        t.ids.insert(map.clone(), h);
        Some(h)
    })
}

/// The map of a handle (empty for 0 or an unknown handle).
pub fn map_of(handle: u16) -> DataMap {
    if handle == 0 {
        return DataMap::default();
    }
    with_table(|t| t.maps.get(handle as usize - 1).cloned().unwrap_or_default())
}

pub fn get(handle: u16, key: &str) -> Option<String> {
    map_of(handle).get(key).map(String::from)
}

/// The handle of the map `handle` names with `key` set.
pub fn with_key(handle: u16, key: &str, value: &str) -> Result<u16, String> {
    let mut m = map_of(handle);
    m.set(key, value)?;
    intern(&m).ok_or_else(|| String::from("too many different item data maps (4096)"))
}

/// The handle of the map `handle` names without `key`.
pub fn without_key(handle: u16, key: &str) -> u16 {
    let mut m = map_of(handle);
    m.clear(key);
    intern(&m).unwrap_or(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_map_has_the_same_handle_and_different_maps_differ() {
        let a = with_key(0, "gd_owner", "kyttu").unwrap();
        let b = with_key(0, "gd_owner", "kyttu").unwrap();
        let c = with_key(0, "gd_owner", "other").unwrap();
        assert_ne!(a, 0);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(get(a, "gd_owner").as_deref(), Some("kyttu"));
        assert_eq!(get(a, "missing"), None);
    }

    #[test]
    fn keys_are_sorted_so_the_order_of_setting_does_not_matter() {
        let a = with_key(with_key(0, "gd_a", "1").unwrap(), "gd_b", "2").unwrap();
        let b = with_key(with_key(0, "gd_b", "2").unwrap(), "gd_a", "1").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn clearing_the_last_key_gives_no_data() {
        let a = with_key(0, "gd_k", "v").unwrap();
        assert_eq!(without_key(a, "gd_k"), 0);
        assert_eq!(without_key(0, "gd_k"), 0);
    }

    #[test]
    fn limits_are_kept() {
        assert!(with_key(0, "", "x").is_err());
        assert!(with_key(0, "a very long key indeed", "x").is_err());
        assert!(with_key(0, "bad key", "x").is_err());
        assert!(with_key(0, "gd_v", &"x".repeat(65)).is_err());
        assert!(with_key(0, "gd_v", &"x".repeat(64)).is_ok());
        assert!(with_key(0, "gd_v", "line\nbreak").is_err());
        let mut m = DataMap::default();
        for i in 0..8 {
            m.set(&alloc::format!("k{}", i), "v").unwrap();
        }
        assert!(m.set("k8", "v").is_err());
        assert!(m.set("k3", "other").is_ok());
        assert_eq!(m.0.len(), 8);
        assert!(m.clear("k0"));
        assert!(!m.clear("k0"));
    }
}
