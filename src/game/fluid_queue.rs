//! The timetable of flowing liquids: which cells are to be looked at, and
//! when, with the work spread over many ticks so that a flood (a dam broken
//! over a whole valley) never makes one frame long.
//!
//! What changed is first noted in a backlog; each tick only a few hundred of
//! those are turned into cells to look at (a liquid cell and the six around
//! it), and only a few hundred cells are worked on. A cell is never queued
//! twice. Water waits a quarter of a second between steps, lava a second and
//! a half, and lava touching water does not wait at all. Each kind has its
//! own first-in first-out line, so every line is in order of time and only
//! its front needs looking at.

use crate::block::{self, Block};
use crate::game::fluid::{step_with, DOWN, SIDES, UP};
use crate::world::World;
use alloc::collections::{BTreeSet, VecDeque};
use alloc::vec::Vec;

/// Seconds a liquid takes to move one step.
pub const WATER_DELAY: f32 = 0.25;
pub const LAVA_DELAY: f32 = 1.5;
/// Lava in a hellish dimension (see [`fast_lava_in`]).
pub const HOT_LAVA_DELAY: f32 = 0.5;
/// Most cells worked on in one tick.
pub const PER_TICK: usize = 300;
/// Most changes turned into cells to look at in one tick.
pub const EXPAND_PER_TICK: usize = 300;
/// Most cells waiting, and most changes in the backlog.
pub const WAITING: usize = 40_000;
pub const BACKLOG: usize = 200_000;

/// Whether lava runs fast in the dimension called `dim`: those whose id has
/// `nether`, `hell` or `inferno` in it (the game has no dimension of that
/// kind of its own; mods name theirs).
pub fn fast_lava_in(dim: &str) -> bool {
    ["nether", "hell", "inferno"].iter().any(|w| dim.contains(w))
}

/// Cells waiting to be looked at again, each with the time it is due.
#[derive(Default)]
pub struct Queue {
    /// Lava runs at [`HOT_LAVA_DELAY`] (the session sets it from the dimension).
    pub fast_lava: bool,
    now: f32,
    /// Everything in the three lines, so no cell is queued twice.
    queued: BTreeSet<[i32; 3]>,
    /// Lava that touches water, and anything else with no wait.
    quick: VecDeque<([i32; 3], f32)>,
    water: VecDeque<([i32; 3], f32)>,
    lava: VecDeque<([i32; 3], f32)>,
    /// Changes not yet turned into cells to look at.
    backlog: VecDeque<[i32; 3]>,
    /// Blocks washed away since the session last collected them (cell, block).
    pub washed: Vec<([i32; 3], Block)>,
}

impl Queue {
    /// How many cells are waiting to be worked on.
    pub fn waiting(&self) -> usize {
        self.queued.len()
    }

    /// How many changes are waiting to be turned into cells to look at.
    pub fn pending(&self) -> usize {
        self.backlog.len()
    }

    fn add(&mut self, w: &World, p: [i32; 3]) {
        let Some((lava, _)) = block::fluid_of(w.get(p[0], p[1], p[2])) else { return };
        if self.queued.contains(&p) {
            return;
        }
        // Lava meeting water sets at once; otherwise each kind has its pace.
        let touches_water = lava && [UP, DOWN, SIDES[0], SIDES[1], SIDES[2], SIDES[3]].iter().any(|d| block::is_water(w.get(p[0] + d[0], p[1] + d[1], p[2] + d[2])));
        let (line, wait) = if lava && touches_water {
            (&mut self.quick, 0.0)
        } else if lava {
            (&mut self.lava, if self.fast_lava { HOT_LAVA_DELAY } else { LAVA_DELAY })
        } else {
            (&mut self.water, WATER_DELAY)
        };
        line.push_back((p, self.now + wait));
        self.queued.insert(p);
    }

    /// Note what changed (liquid cells in and around it wait their turn), advance the
    /// clock by `dt`, and work on every cell whose time has come (not more than
    /// [`PER_TICK`] of them).
    pub fn run(&mut self, w: &mut World, changed: &[[i32; 3]], dt: f32) {
        self.now += dt;
        for &c in changed {
            if self.backlog.len() < BACKLOG {
                self.backlog.push_back(c);
            }
        }
        let around = [[0, 0, 0], UP, DOWN, SIDES[0], SIDES[1], SIDES[2], SIDES[3]];
        let mut budget = EXPAND_PER_TICK;
        while budget > 0 && self.queued.len() < WAITING {
            let Some(c) = self.backlog.pop_front() else { break };
            budget -= 1;
            for d in around {
                self.add(w, [c[0] + d[0], c[1] + d[1], c[2] + d[2]]);
            }
        }
        let now = self.now;
        let mut left = PER_TICK;
        for k in 0..3 {
            while left > 0 {
                let line = match k {
                    0 => &mut self.quick,
                    1 => &mut self.water,
                    _ => &mut self.lava,
                };
                match line.front() {
                    Some(&(c, t)) if t <= now => {
                        line.pop_front();
                        self.queued.remove(&c);
                        left -= 1;
                        // Unloaded chunks are never touched.
                        if w.is_loaded(c[0], c[2]) {
                            step_with(w, c, &mut self.washed);
                        }
                    }
                    _ => break,
                }
            }
        }
    }
}
