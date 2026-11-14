//! Handles for the sounds a mod starts, so it can stop, fade or change them
//! later.
//!
//! A handle is a small whole number: the slot of the sound in a table of 64
//! in the low byte (plus one, so 0 is never a handle) and a generation in
//! the bits above. A slot's generation goes up each time it is let go, so a
//! handle kept after its sound ended is *stale* and does nothing, even when
//! the slot has been given to another sound. A mod can only touch handles
//! that it was given.
//!
//! Limits (see docs/SCRIPTING.md): a mod owns at most [`PER_OWNER`] live
//! handles, of which at most [`LOOPS_PER_OWNER`] are loops, and starts at
//! most [`PLAYS_PER_FRAME`] sounds in a frame. Beyond the limit a one-shot
//! still plays (without a handle: it cannot be stopped); a loop is refused.

use alloc::string::String;
use alloc::vec::Vec;

/// Handles alive at once, over all mods.
pub const SLOTS: usize = 64;
/// Live handles one mod may hold.
pub const PER_OWNER: usize = 8;
/// Of those, loops.
pub const LOOPS_PER_OWNER: usize = 4;
/// Sounds one mod may start in one frame.
pub const PLAYS_PER_FRAME: usize = 8;

/// Why a handle was not given.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Refused {
    /// The mod has [`PER_OWNER`] handles already.
    OwnerFull,
    /// The mod has [`LOOPS_PER_OWNER`] loops already.
    LoopsFull,
    /// Every slot is in use.
    NoSlot,
    /// The mod started too many sounds this frame.
    FrameLimit,
}

#[derive(Clone)]
struct Slot {
    gen: u16,
    owner: Option<String>,
    looping: bool,
}

pub struct HandleBook {
    slots: Vec<Slot>,
    /// Sounds started this frame, per mod.
    plays: Vec<(String, usize)>,
}

impl Default for HandleBook {
    fn default() -> HandleBook {
        HandleBook::new()
    }
}

fn pack(gen: u16, slot: usize) -> u32 {
    ((gen as u32) << 8) | (slot as u32 + 1)
}

impl HandleBook {
    pub fn new() -> HandleBook {
        HandleBook { slots: alloc::vec![Slot { gen: 1, owner: None, looping: false }; SLOTS], plays: Vec::new() }
    }

    /// A new frame: the per-frame play counts start again.
    pub fn new_frame(&mut self) {
        self.plays.clear();
    }

    /// Count a sound `owner` wants to start now; `false` when it has started
    /// too many this frame (the sound is dropped).
    pub fn may_play(&mut self, owner: &str) -> bool {
        match self.plays.iter_mut().find(|p| p.0 == owner) {
            Some(p) => {
                if p.1 >= PLAYS_PER_FRAME {
                    return false;
                }
                p.1 += 1;
                true
            }
            None => {
                self.plays.push((String::from(owner), 1));
                true
            }
        }
    }

    /// How many live handles `owner` holds, and how many of them are loops.
    pub fn counts(&self, owner: &str) -> (usize, usize) {
        let mine = self.slots.iter().filter(|s| s.owner.as_deref() == Some(owner));
        let (mut all, mut loops) = (0, 0);
        for s in mine {
            all += 1;
            loops += s.looping as usize;
        }
        (all, loops)
    }

    /// Give `owner` a handle for a sound it starts (`looping`: until stopped).
    pub fn alloc(&mut self, owner: &str, looping: bool) -> Result<u32, Refused> {
        let (all, loops) = self.counts(owner);
        if all >= PER_OWNER {
            return Err(Refused::OwnerFull);
        }
        if looping && loops >= LOOPS_PER_OWNER {
            return Err(Refused::LoopsFull);
        }
        let i = self.slots.iter().position(|s| s.owner.is_none()).ok_or(Refused::NoSlot)?;
        let s = &mut self.slots[i];
        s.owner = Some(String::from(owner));
        s.looping = looping;
        Ok(pack(s.gen, i))
    }

    /// The slot a handle stands for, if it is alive and not stale.
    fn find(&self, h: u32) -> Option<usize> {
        let slot = (h & 0xFF) as usize;
        if slot == 0 || slot > SLOTS {
            return None;
        }
        let s = &self.slots[slot - 1];
        if s.owner.is_some() && (h >> 8) == s.gen as u32 {
            Some(slot - 1)
        } else {
            None
        }
    }

    /// Whether the handle is alive (not stale, not let go).
    pub fn live(&self, h: u32) -> bool {
        self.find(h).is_some()
    }

    /// Whether the handle is alive and `owner` was the one given it.
    pub fn owned_by(&self, h: u32, owner: &str) -> bool {
        self.find(h).map_or(false, |i| self.slots[i].owner.as_deref() == Some(owner))
    }

    /// Whether the handle is a live loop.
    pub fn is_loop(&self, h: u32) -> bool {
        self.find(h).map_or(false, |i| self.slots[i].looping)
    }

    /// Let a handle go (its sound ended or was stopped). `false` if it was
    /// stale already.
    pub fn release(&mut self, h: u32) -> bool {
        let Some(i) = self.find(h) else { return false };
        let s = &mut self.slots[i];
        s.owner = None;
        s.looping = false;
        s.gen = if s.gen == u16::MAX { 1 } else { s.gen + 1 };
        true
    }

    /// Let go of everything `owner` holds; the handles that were alive.
    pub fn release_owner(&mut self, owner: &str) -> Vec<u32> {
        let mut out = Vec::new();
        for i in 0..self.slots.len() {
            if self.slots[i].owner.as_deref() == Some(owner) {
                let h = pack(self.slots[i].gen, i);
                self.release(h);
                out.push(h);
            }
        }
        out
    }

    /// The mods that hold a live handle (each once).
    pub fn owners(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.slots {
            if let Some(o) = &s.owner {
                if !out.contains(o) {
                    out.push(o.clone());
                }
            }
        }
        out
    }

    /// Every live handle.
    pub fn all(&self) -> Vec<u32> {
        (0..self.slots.len()).filter(|&i| self.slots[i].owner.is_some()).map(|i| pack(self.slots[i].gen, i)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_handle_is_nonzero_and_alive_until_let_go() {
        let mut b = HandleBook::new();
        let h = b.alloc("a", false).unwrap();
        assert!(h > 0 && b.live(h) && b.owned_by(h, "a") && !b.owned_by(h, "b"));
        assert!(b.release(h));
        assert!(!b.live(h));
        assert!(!b.release(h), "a second release is harmless");
    }

    #[test]
    fn a_stale_handle_does_nothing_even_when_its_slot_is_reused() {
        let mut b = HandleBook::new();
        let old = b.alloc("a", false).unwrap();
        b.release(old);
        let new = b.alloc("a", false).unwrap();
        assert_ne!(old, new, "the generation moved on");
        assert_eq!(old & 0xFF, new & 0xFF, "the same slot");
        assert!(!b.live(old) && b.live(new));
        assert!(!b.owned_by(old, "a"));
        assert!(!b.release(old));
        assert!(b.live(new), "the new sound was not touched by the stale handle");
    }

    #[test]
    fn made_up_numbers_are_not_handles() {
        let mut b = HandleBook::new();
        let _ = b.alloc("a", false).unwrap();
        for h in [0u32, 1, 2, 65, 255, 256, 0xFFFF_FFFF, 0x1234_5678] {
            if h & 0xFF != 1 || h >> 8 != 1 {
                assert!(!b.live(h), "{h}");
            }
        }
        assert!(!b.release(0));
    }

    #[test]
    fn a_mod_holds_at_most_eight_handles_and_four_loops() {
        let mut b = HandleBook::new();
        let mut hs = Vec::new();
        for _ in 0..LOOPS_PER_OWNER {
            hs.push(b.alloc("a", true).unwrap());
        }
        assert_eq!(b.alloc("a", true), Err(Refused::LoopsFull));
        for _ in LOOPS_PER_OWNER..PER_OWNER {
            hs.push(b.alloc("a", false).unwrap());
        }
        assert_eq!(b.alloc("a", false), Err(Refused::OwnerFull));
        assert_eq!(b.counts("a"), (8, 4));
        // Another mod is not held back by it.
        assert!(b.alloc("b", true).is_ok());
        // Letting one go makes room.
        b.release(hs[0]);
        assert!(b.alloc("a", true).is_ok());
    }

    #[test]
    fn the_table_is_limited_over_all_mods() {
        let mut b = HandleBook::new();
        let mut n = 0;
        'outer: for m in 0..SLOTS {
            let name = alloc::format!("m{m}");
            for _ in 0..PER_OWNER {
                match b.alloc(&name, false) {
                    Ok(_) => n += 1,
                    Err(Refused::NoSlot) => break 'outer,
                    Err(e) => panic!("{e:?}"),
                }
            }
        }
        assert_eq!(n, SLOTS);
        assert_eq!(b.alloc("late", false), Err(Refused::NoSlot));
    }

    #[test]
    fn plays_are_counted_per_mod_and_per_frame() {
        let mut b = HandleBook::new();
        for _ in 0..PLAYS_PER_FRAME {
            assert!(b.may_play("a"));
        }
        assert!(!b.may_play("a"));
        assert!(b.may_play("b"));
        b.new_frame();
        assert!(b.may_play("a"));
    }

    #[test]
    fn letting_go_of_a_mod_frees_all_its_handles() {
        let mut b = HandleBook::new();
        let a1 = b.alloc("a", true).unwrap();
        let a2 = b.alloc("a", false).unwrap();
        let o = b.alloc("b", false).unwrap();
        assert_eq!(b.owners().len(), 2);
        let mut gone = b.release_owner("a");
        gone.sort();
        let mut want = alloc::vec![a1, a2];
        want.sort();
        assert_eq!(gone, want);
        assert!(!b.live(a1) && !b.live(a2) && b.live(o));
        assert_eq!(b.counts("a"), (0, 0));
        assert_eq!(b.all(), alloc::vec![o]);
    }

    #[test]
    fn the_generation_wraps_without_reaching_zero() {
        let mut b = HandleBook::new();
        for _ in 0..70_000 {
            let h = b.alloc("a", false).unwrap();
            assert!(h >> 8 != 0);
            b.release(h);
        }
        assert!(b.alloc("a", false).is_ok());
    }
}
