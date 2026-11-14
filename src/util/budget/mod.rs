//! The memory budget: what the game holds, by kind, against a soft limit, and
//! what to give up when it holds too much.
//!
//! This is only counting and deciding: nothing here touches the game. The
//! client gathers the numbers (`client/memory`), asks [`Ladder::update`] what
//! to do and does it. Numbers are estimates of resident bytes, good to
//! maybe 20 per cent, which is plenty for a limit this soft.
//!
//! The order of giving up (cheapest to the player's eyes first):
//! 1. drop decoded sounds that are not playing (they are decoded again when played);
//! 2. simplify distant terrain (LOD detail, real-block tiles, trees);
//! 3. shorten how far distant terrain reaches, down to off;
//! 4. shrink the view distance, one chunk at a time, down to [`MIN_VIEW`].
//!
//! When memory has been well under the limit for a while, the steps are taken
//! back in the opposite order, one at a time.

use alloc::string::String;

pub mod ladder;
pub use ladder::{Action, Effective, Ladder, Wanted};

/// What the memory is spent on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cat {
    /// Block data of loaded chunks.
    Chunks,
    /// Vertex buffers of chunks and distant tiles (graphics memory, but on
    /// many machines it is the same memory, and drivers keep copies).
    Meshes,
    /// Pictures: the block atlas, creature and model pictures.
    Textures,
    /// Decoded sounds.
    Audio,
    /// Model definitions and the files they came from.
    Models,
    /// Creatures, items, particles and the like.
    Entities,
    /// What the game needs whatever is loaded (program, fonts, registry).
    Other,
}

pub const CATS: [Cat; 7] = [Cat::Chunks, Cat::Meshes, Cat::Textures, Cat::Audio, Cat::Models, Cat::Entities, Cat::Other];

impl Cat {
    pub fn name(self) -> &'static str {
        match self {
            Cat::Chunks => "chunks",
            Cat::Meshes => "meshes",
            Cat::Textures => "textures",
            Cat::Audio => "audio",
            Cat::Models => "models",
            Cat::Entities => "entities",
            Cat::Other => "other",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// The least view distance (in chunks) the budget will shrink to.
pub const MIN_VIEW: i32 = 3;

const MB: usize = 1 << 20;

/// The soft limit in bytes: about 300 MB without mods, 700 MB with.
pub fn soft_limit(with_mods: bool) -> usize {
    if with_mods {
        700 * MB
    } else {
        300 * MB
    }
}

/// Bytes by category.
#[derive(Clone, Debug)]
pub struct Budget {
    bytes: [usize; 7],
    pub limit: usize,
}

impl Budget {
    pub fn new(limit: usize) -> Budget {
        Budget { bytes: [0; 7], limit }
    }

    pub fn set(&mut self, cat: Cat, bytes: usize) {
        self.bytes[cat.index()] = bytes;
    }

    pub fn get(&self, cat: Cat) -> usize {
        self.bytes[cat.index()]
    }

    pub fn total(&self) -> usize {
        self.bytes.iter().fold(0usize, |a, b| a.saturating_add(*b))
    }

    /// The total as a share of the limit, in percent.
    pub fn percent(&self) -> usize {
        if self.limit == 0 {
            return 100;
        }
        (self.total() / 1024).saturating_mul(100) / (self.limit / 1024).max(1)
    }

    pub fn over(&self) -> bool {
        self.total() > self.limit
    }

    /// One line for the log: `memory 212/300 MB (70%): chunks 90, meshes 60, ...`.
    pub fn line(&self) -> String {
        let mut s = alloc::format!("memory {}/{} MB ({}%):", self.total() / MB, self.limit / MB, self.percent());
        for (i, c) in CATS.iter().enumerate() {
            s.push_str(&alloc::format!("{} {} {}", if i == 0 { "" } else { "," }, c.name(), self.get(*c) / MB));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totals_add_up_by_category() {
        let mut b = Budget::new(soft_limit(false));
        b.set(Cat::Chunks, 100 * MB);
        b.set(Cat::Meshes, 50 * MB);
        b.set(Cat::Audio, 1);
        assert_eq!(b.total(), 150 * MB + 1);
        assert_eq!(b.get(Cat::Chunks), 100 * MB);
        assert_eq!(b.get(Cat::Textures), 0);
        assert_eq!(b.percent(), 50);
        assert!(!b.over());
        b.set(Cat::Other, 200 * MB);
        assert!(b.over());
    }

    #[test]
    fn the_limits_are_300_and_700_megabytes() {
        assert_eq!(soft_limit(false), 300 << 20);
        assert_eq!(soft_limit(true), 700 << 20);
    }

    #[test]
    fn setting_a_category_replaces_it_and_big_numbers_do_not_overflow() {
        let mut b = Budget::new(10);
        b.set(Cat::Chunks, usize::MAX);
        b.set(Cat::Meshes, usize::MAX);
        assert_eq!(b.total(), usize::MAX);
        let _ = b.percent();
        b.set(Cat::Chunks, 5);
        b.set(Cat::Meshes, 0);
        assert_eq!(b.total(), 5);
    }

    #[test]
    fn the_log_line_names_every_category() {
        let mut b = Budget::new(300 * MB);
        b.set(Cat::Chunks, 90 * MB);
        let l = b.line();
        assert!(l.starts_with("memory 90/300 MB (30%):"), "{}", l);
        for c in CATS {
            assert!(l.contains(c.name()), "{}", l);
        }
        assert!(l.contains("chunks 90"));
    }

    #[test]
    fn a_zero_limit_counts_as_full() {
        assert_eq!(Budget::new(0).percent(), 100);
    }
}
