//! What the world remembers for the mod hooks: chunks that were just
//! generated and blocks that were just placed, with why.
//!
//! The world only writes lists here (cheap, bounded); the session reads them
//! once a frame (`game/world_hooks.rs`) and calls the hooks a few at a time,
//! so neither generation nor a burst of edits can stall a frame.

use crate::block::{Block, AIR};
use alloc::collections::VecDeque;
use alloc::vec::Vec;

/// Most newly generated chunks waiting for their hook (the oldest is dropped
/// beyond it). A view distance of 48 chunks is about 10 000 of them.
pub const MAX_GENERATED: usize = 16_384;
/// Most block placements waiting for their hook; the newest are dropped beyond it.
pub const MAX_PLACED: usize = 2048;
/// How many hooks deep a chain may go: a hook that places a block makes the
/// next event one level deeper, and events at this depth are not passed on.
pub const MAX_PLACE_DEPTH: u8 = 2;

/// The causes a placement is given (`on_block_placed`'s last argument).
pub mod cause {
    pub const PLAYER: &str = "player";
    pub const SCRIPT: &str = "script";
    pub const REMOTE: &str = "remote";
    pub const FALL: &str = "fall";
    pub const FLUID: &str = "fluid";
    /// Everything else: crops growing, villagers building, commands.
    pub const OTHER: &str = "other";
    pub const ALL: [&str; 6] = [PLAYER, SCRIPT, REMOTE, FALL, FLUID, OTHER];
}

/// Whether an event that came `depth` hooks deep is still passed to the hooks.
pub fn may_dispatch(depth: u8) -> bool {
    depth < MAX_PLACE_DEPTH
}

/// One block placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub cell: [i32; 3],
    pub block: Block,
    pub cause: &'static str,
    /// How many hooks deep the code that placed it was (0: not from a hook).
    pub depth: u8,
}

/// The world's two lists and the context placements are noted in.
#[derive(Default)]
pub struct WorldLog {
    /// Chunks generated (not loaded from a save) since the session looked.
    pub generated: VecDeque<(i32, i32)>,
    pub placed: VecDeque<Placed>,
    /// Whether placements are noted at all (a hook wants them).
    pub track_placed: bool,
    /// The depth the code placing blocks right now is at (see [`may_dispatch`]).
    pub depth: u8,
    /// The cause the next placements get; empty means [`cause::OTHER`].
    pub cause: &'static str,
}

impl WorldLog {
    pub fn note_generated(&mut self, cx: i32, cz: i32) {
        if self.generated.len() >= MAX_GENERATED {
            self.generated.pop_front();
        }
        self.generated.push_back((cx, cz));
    }

    /// A block went to `cell`. Air (a block taken away) is not a placement.
    pub fn note_placed(&mut self, cell: [i32; 3], block: Block, cause: &'static str) {
        if !self.track_placed || block == AIR || self.placed.len() >= MAX_PLACED {
            return;
        }
        let cause = if cause.is_empty() { cause::OTHER } else { cause };
        self.placed.push_back(Placed { cell, block, cause, depth: self.depth });
    }

    /// Up to `n` of the oldest waiting chunks.
    pub fn take_generated(&mut self, n: usize) -> Vec<(i32, i32)> {
        let k = n.min(self.generated.len());
        self.generated.drain(..k).collect()
    }

    /// Up to `n` of the oldest waiting placements.
    pub fn take_placed(&mut self, n: usize) -> Vec<Placed> {
        let k = n.min(self.placed.len());
        self.placed.drain(..k).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_two_is_not_passed_on() {
        assert!(may_dispatch(0));
        assert!(may_dispatch(1));
        assert!(!may_dispatch(2));
        assert!(!may_dispatch(200));
    }

    #[test]
    fn untracked_placements_and_air_are_not_kept() {
        let mut l = WorldLog::default();
        l.note_placed([0, 1, 2], 5, cause::SCRIPT);
        assert!(l.placed.is_empty());
        l.track_placed = true;
        l.note_placed([0, 1, 2], AIR, cause::SCRIPT);
        assert!(l.placed.is_empty());
        l.note_placed([0, 1, 2], 5, "");
        l.depth = 1;
        l.note_placed([3, 4, 5], 6, cause::FALL);
        let got = l.take_placed(10);
        assert_eq!(got.len(), 2);
        assert_eq!((got[0].cause, got[0].depth), (cause::OTHER, 0));
        assert_eq!((got[1].cause, got[1].depth), (cause::FALL, 1));
        assert!(l.placed.is_empty());
    }

    #[test]
    fn the_lists_are_bounded() {
        let mut l = WorldLog::default();
        l.track_placed = true;
        for i in 0..(MAX_PLACED as i32 + 50) {
            l.note_placed([i, 0, 0], 3, cause::PLAYER);
        }
        assert_eq!(l.placed.len(), MAX_PLACED);
        for i in 0..(MAX_GENERATED as i32 + 10) {
            l.note_generated(i, 0);
        }
        assert_eq!(l.generated.len(), MAX_GENERATED);
        // The oldest were dropped.
        assert_eq!(l.generated.front(), Some(&(10, 0)));
    }

    #[test]
    fn taking_gives_the_oldest_first_and_no_more_than_there_is() {
        let mut l = WorldLog::default();
        for i in 0..5 {
            l.note_generated(i, -i);
        }
        assert_eq!(l.take_generated(2), alloc::vec![(0, 0), (1, -1)]);
        assert_eq!(l.take_generated(99).len(), 3);
        assert!(l.take_generated(1).is_empty());
    }
}
