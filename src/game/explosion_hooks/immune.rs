//! The blocks mods declared immune to explosions (`set_immune_blocks` of a
//! script, `immune_blocks` of a native mod). The engine skips them without
//! asking anybody, so a blast pays one bit test for each block it reaches.
//!
//! The lists are kept per mod: a mod's own list is replaced by its next
//! `set_immune_blocks` and dropped by `clear_immune_blocks`. A name the game
//! does not know is reported once (the caller logs it) and left out.

use crate::block::Block;
use alloc::string::String;
use alloc::vec::Vec;

/// Most entries one mod's list may have (the rest are left out).
pub const MAX_ENTRIES: usize = 4096;
/// Most unknown names remembered, so that each is reported once.
const MAX_WARNED: usize = 256;

/// One entry of a list as the mod wrote it: a block's id or its number.
#[derive(Clone, Debug, PartialEq)]
pub enum Spec {
    Name(String),
    Number(i64),
}

impl Spec {
    /// How the entry reads in a message.
    pub fn show(&self) -> String {
        match self {
            Spec::Name(n) => n.clone(),
            Spec::Number(n) => alloc::format!("{}", n),
        }
    }
}

struct Owner {
    name: String,
    specs: Vec<Spec>,
    ids: Vec<Block>,
}

/// What `set` made of a list.
#[derive(Debug, Default, PartialEq)]
pub struct SetOutcome {
    /// How many entries were taken.
    pub accepted: usize,
    /// Names not seen before that are no block (to be logged once).
    pub unknown: Vec<String>,
    /// The list was longer than [`MAX_ENTRIES`].
    pub truncated: bool,
}

/// All the lists, and the union of them as a bit set over block numbers.
#[derive(Default)]
pub struct Registry {
    owners: Vec<Owner>,
    bits: Vec<u64>,
    /// How many blocks the game had when the bits were made.
    blocks: usize,
    warned: Vec<String>,
}

fn set_bit(bits: &mut Vec<u64>, b: Block) {
    let (w, k) = (b as usize / 64, b as usize % 64);
    if bits.len() <= w {
        bits.resize(w + 1, 0);
    }
    bits[w] |= 1u64 << k;
}

/// Whether `b` is in a bit set made by [`Registry::bits`] and [`add_bit`].
pub fn bit_set(bits: &[u64], b: Block) -> bool {
    bits.get(b as usize / 64).map_or(false, |w| (w >> (b as usize % 64)) & 1 == 1)
}

/// Add `b` to a bit set (for the lists of native code).
pub fn add_bit(bits: &mut Vec<u64>, b: Block) {
    set_bit(bits, b);
}

impl Registry {
    /// Take `specs` as the list of `owner` (replacing its earlier one).
    /// `resolve` finds the block an entry means; `blocks` is how many blocks
    /// the game has now.
    pub fn set(&mut self, owner: &str, specs: &[Spec], blocks: usize, resolve: &dyn Fn(&Spec) -> Option<Block>) -> SetOutcome {
        let mut out = SetOutcome::default();
        let mut kept = Vec::new();
        let mut ids = Vec::new();
        for (i, s) in specs.iter().enumerate() {
            if i >= MAX_ENTRIES {
                out.truncated = true;
                break;
            }
            match resolve(s) {
                Some(b) => {
                    kept.push(s.clone());
                    ids.push(b);
                }
                None => {
                    let key = alloc::format!("{}/{}", owner, s.show());
                    if !self.warned.contains(&key) && self.warned.len() < MAX_WARNED {
                        self.warned.push(key);
                        out.unknown.push(s.show());
                    }
                }
            }
        }
        out.accepted = ids.len();
        match self.owners.iter_mut().find(|o| o.name == owner) {
            Some(o) => {
                o.specs = kept;
                o.ids = ids;
            }
            None => self.owners.push(Owner { name: String::from(owner), specs: kept, ids }),
        }
        self.remake(blocks);
        out
    }

    /// Drop the list of `owner`. Whether it had one.
    pub fn clear(&mut self, owner: &str) -> bool {
        let before = self.owners.len();
        self.owners.retain(|o| o.name != owner);
        let had = self.owners.len() != before;
        if had {
            let blocks = self.blocks;
            self.remake(blocks);
        }
        had
    }

    /// Whether no mod has a list.
    pub fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }

    /// The entries of `owner`'s list that were accepted.
    pub fn specs_of(&self, owner: &str) -> &[Spec] {
        self.owners.iter().find(|o| o.name == owner).map_or(&[], |o| o.specs.as_slice())
    }

    /// Whether some mod's list has this block.
    pub fn contains(&self, b: Block) -> bool {
        bit_set(&self.bits, b)
    }

    /// The union of all the lists as a bit set (a copy for one blast).
    pub fn bits(&self) -> Vec<u64> {
        self.bits.clone()
    }

    /// The game's blocks changed (content was reloaded): resolve the lists
    /// again by what the mods wrote. Returns whether that was needed.
    pub fn refresh(&mut self, blocks: usize, resolve: &dyn Fn(&Spec) -> Option<Block>) -> bool {
        if blocks == self.blocks || self.owners.is_empty() {
            self.blocks = blocks;
            return false;
        }
        for o in self.owners.iter_mut() {
            o.ids = o.specs.iter().filter_map(|s| resolve(s)).collect();
        }
        self.remake(blocks);
        true
    }

    fn remake(&mut self, blocks: usize) {
        self.blocks = blocks;
        let mut bits = Vec::new();
        for o in &self.owners {
            for &b in &o.ids {
                set_bit(&mut bits, b);
            }
        }
        self.bits = bits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<Spec> {
        list.iter().map(|n| Spec::Name(String::from(*n))).collect()
    }

    /// A made-up game of five blocks.
    fn resolve(s: &Spec) -> Option<Block> {
        let known = ["air", "stone", "obsidian", "bedrock", "tnt"];
        match s {
            Spec::Name(n) => known.iter().position(|k| k == n).map(|i| i as Block),
            Spec::Number(n) if *n >= 0 && (*n as usize) < known.len() => Some(*n as Block),
            _ => None,
        }
    }

    #[test]
    fn a_list_makes_its_blocks_immune_and_only_those() {
        let mut r = Registry::default();
        let out = r.set("m", &names(&["obsidian", "bedrock"]), 5, &resolve);
        assert_eq!(out.accepted, 2);
        assert!(out.unknown.is_empty() && !out.truncated);
        assert!(r.contains(2) && r.contains(3));
        assert!(!r.contains(0) && !r.contains(1) && !r.contains(4));
        // Past the end of the bit set is simply not immune.
        assert!(!r.contains(60_000));
    }

    #[test]
    fn numbers_and_names_both_work() {
        let mut r = Registry::default();
        let list = alloc::vec![Spec::Number(4), Spec::Name(String::from("stone"))];
        assert_eq!(r.set("m", &list, 5, &resolve).accepted, 2);
        assert!(r.contains(4) && r.contains(1));
        assert_eq!(r.specs_of("m"), list.as_slice());
    }

    #[test]
    fn an_unknown_name_is_reported_once_and_ignored() {
        let mut r = Registry::default();
        let out = r.set("m", &names(&["obsidian", "unobtainium"]), 5, &resolve);
        assert_eq!(out.accepted, 1);
        assert_eq!(out.unknown, alloc::vec![String::from("unobtainium")]);
        let again = r.set("m", &names(&["obsidian", "unobtainium"]), 5, &resolve);
        assert_eq!(again.accepted, 1);
        assert!(again.unknown.is_empty(), "reported twice");
        // Another mod writing the same wrong name is reported on its own.
        assert_eq!(r.set("other", &names(&["unobtainium"]), 5, &resolve).unknown.len(), 1);
        // Numbers out of range are unknown too.
        assert_eq!(r.set("n", &[Spec::Number(99), Spec::Number(-1)], 5, &resolve).accepted, 0);
    }

    #[test]
    fn a_list_is_capped() {
        let mut r = Registry::default();
        let mut list = Vec::new();
        for i in 0..MAX_ENTRIES + 50 {
            list.push(Spec::Number((i % 5) as i64));
        }
        let out = r.set("m", &list, 5, &resolve);
        assert_eq!(out.accepted, MAX_ENTRIES);
        assert!(out.truncated);
    }

    #[test]
    fn each_mod_keeps_its_own_list() {
        let mut r = Registry::default();
        r.set("a", &names(&["stone"]), 5, &resolve);
        r.set("b", &names(&["tnt"]), 5, &resolve);
        assert!(r.contains(1) && r.contains(4));
        // A mod's second list replaces its first and leaves the other mod's alone.
        r.set("a", &names(&["obsidian"]), 5, &resolve);
        assert!(!r.contains(1) && r.contains(2) && r.contains(4));
        assert!(r.clear("b"));
        assert!(!r.contains(4) && r.contains(2));
        assert!(!r.clear("b"));
        assert!(r.clear("a") && r.is_empty() && !r.contains(2));
    }

    #[test]
    fn a_reload_resolves_the_names_again() {
        let mut r = Registry::default();
        r.set("m", &names(&["stone"]), 5, &resolve);
        assert!(!r.refresh(5, &resolve));
        // The game now numbers its blocks differently.
        let moved = |s: &Spec| if *s == Spec::Name(String::from("stone")) { Some(7 as Block) } else { None };
        assert!(r.refresh(9, &moved));
        assert!(r.contains(7) && !r.contains(1));
    }

    #[test]
    fn bit_sets_can_be_added_to() {
        let mut bits = Vec::new();
        assert!(!bit_set(&bits, 3));
        add_bit(&mut bits, 3);
        add_bit(&mut bits, 200);
        assert!(bit_set(&bits, 3) && bit_set(&bits, 200) && !bit_set(&bits, 4));
    }
}
