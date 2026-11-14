//! What to do with a server's announcement of its mods: which to take,
//! which are already here from earlier in the session and which folders are
//! out of date. Pure functions, so the decisions can be tested.

use super::path::*;
use crate::proto::ModInfo;
use alloc::string::String;
use alloc::vec::Vec;

/// A mod already on disk in the server's folder, as its manifest says.
#[derive(Debug, Clone, PartialEq)]
pub struct Cached {
    pub name: String,
    pub version: String,
    pub sum: u64,
}

/// The outcome of [`plan`].
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    /// Announced mods already on disk, unchanged: names, in announced order.
    pub keep: Vec<String>,
    /// Places in the announcement of the mods to ask for, in order.
    pub fetch: Vec<usize>,
    /// Mod folders on disk to delete first: changed, or no longer offered.
    pub remove: Vec<String>,
    /// Announced mods not taken, with the reason (for the log).
    pub skip: Vec<(String, &'static str)>,
}

/// Decide what to take from `announced`, given what `cached` holds.
///
/// A mod is taken when its name is a plain folder name, it has not been seen
/// earlier in the list, it is within the size limits, and the server's
/// total stays within the limit. A taken mod whose name, version and
/// checksum match a cached one is kept; otherwise it is fetched (and any
/// older copy is removed first).
pub fn plan(announced: &[ModInfo], cached: &[Cached]) -> Plan {
    let mut p = Plan::default();
    let mut taken: Vec<&str> = Vec::new();
    let mut total = 0usize;
    for (i, m) in announced.iter().enumerate() {
        let reason = if taken.len() >= MAX_MODS {
            Some("too many mods")
        } else if clean_mod_name(&m.name).is_none() {
            Some("its name is not allowed")
        } else if taken.iter().any(|t| t.eq_ignore_ascii_case(&m.name)) {
            Some("announced twice")
        } else if m.size as usize > MAX_MOD_BYTES + MAX_FILES * (MAX_PATH_LEN + 8) || m.files as usize > MAX_FILES {
            Some("too big")
        } else if total + m.size as usize > MAX_TOTAL_BYTES {
            Some("the server's mods add up to too much")
        } else {
            None
        };
        if let Some(why) = reason {
            p.skip.push((m.name.clone(), why));
            continue;
        }
        total += m.size as usize;
        taken.push(&m.name);
        let same = cached.iter().any(|c| c.name == m.name && c.version == m.version && c.sum == m.sum);
        if same {
            p.keep.push(m.name.clone());
        } else {
            p.fetch.push(i);
        }
    }
    for c in cached {
        let keep = p.keep.iter().any(|k| *k == c.name);
        if !keep {
            p.remove.push(c.name.clone());
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, version: &str, sum: u64, size: u32) -> ModInfo {
        ModInfo { name: String::from(name), version: String::from(version), sum, size, files: 3 }
    }
    fn cached(name: &str, version: &str, sum: u64) -> Cached {
        Cached { name: String::from(name), version: String::from(version), sum }
    }

    #[test]
    fn an_unchanged_mod_is_kept_and_a_changed_one_fetched_again() {
        let ann = [info("a", "1", 10, 100), info("b", "1", 20, 100), info("c", "2", 30, 100)];
        let have = [cached("a", "1", 10), cached("b", "1", 99), cached("gone", "1", 5)];
        let p = plan(&ann, &have);
        assert_eq!(p.keep, ["a"]);
        assert_eq!(p.fetch, [1, 2]);
        assert_eq!(p.remove, ["b", "gone"]);
        assert!(p.skip.is_empty());
        // A new version of the same bytes still counts as changed.
        assert_eq!(plan(&[info("a", "2", 10, 1)], &have).fetch, [0]);
    }

    #[test]
    fn nothing_cached_means_everything_is_fetched() {
        let p = plan(&[info("a", "", 1, 5), info("b", "", 2, 5)], &[]);
        assert_eq!(p.fetch, [0, 1]);
        assert!(p.keep.is_empty() && p.remove.is_empty());
        assert_eq!(plan(&[], &[cached("x", "", 1)]).remove, ["x"]);
    }

    #[test]
    fn bad_announcements_are_skipped_with_a_reason() {
        let big = (MAX_TOTAL_BYTES / 3) as u32;
        let ann = [info("../x", "", 1, 1), info("ok", "", 1, 1), info("OK", "", 2, 1), info("w1", "", 3, big), info("w2", "", 4, big), info("w3", "", 5, big), info("huge", "", 6, u32::MAX)];
        let p = plan(&ann, &[]);
        assert_eq!(p.fetch, [1, 3, 4]);
        let why: Vec<&str> = p.skip.iter().map(|s| s.0.as_str()).collect();
        assert_eq!(why, ["../x", "OK", "w3", "huge"]);
        let many: Vec<ModInfo> = (0..MAX_MODS + 3).map(|i| info(&alloc::format!("m{}", i), "", i as u64, 1)).collect();
        let p = plan(&many, &[]);
        assert_eq!(p.fetch.len(), MAX_MODS);
        assert_eq!(p.skip.len(), 3);
    }
}
