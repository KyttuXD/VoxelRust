//! Chest lids that open and shut.
//!
//! A chest is four blocks in the data (see `tools/gen_base.py`): the shut
//! chest and three hidden ones, `<id>_lid1..3`, whose lids are raised a
//! little more each time. Each lid block carries `tag chest_lid <n>` and
//! `rest <the shut chest>`, so a world that was saved with a chest open
//! reads it back shut (`block::at_rest`).
//!
//! [`ChestAnim`] is the timer: given which chest the player has open and
//! what stage every lid it knows of stands at, it says which lids move on a
//! stage and when a sound is due. It knows nothing of the world, so it is
//! tested alone. [`tick`] is the small piece that joins it to a `Session`:
//! it writes each new stage into the world with `set_block`, which is also
//! the edit that is sent to the other players in a shared game, so they see
//! the lid move too.

use crate::audio::Audio;
use crate::block::{self, reg, Block};
use crate::session::Session;
use alloc::vec::Vec;

/// The stage of a lid wide open (0 is shut).
pub const OPEN: u8 = 3;
/// Seconds a lid takes to move one stage.
pub const STEP: f32 = 0.06;
/// How far the player may be from the chest, in blocks, before it shuts.
pub const REACH: f64 = 8.0;

/// A sound to make as a lid starts to move.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Noise {
    Open,
    Close,
}

/// One thing that happened to a lid: it is now at `stage`, and `noise` says
/// whether it just started opening or shutting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Change {
    pub cell: [i32; 3],
    pub stage: u8,
    pub noise: Option<Noise>,
}

#[derive(Clone, Debug)]
struct Lid {
    cell: [i32; 3],
    stage: u8,
    target: u8,
    timer: f32,
}

/// The lids that are moving or standing open.
#[derive(Clone, Debug, Default)]
pub struct ChestAnim {
    lids: Vec<Lid>,
}

impl ChestAnim {
    /// Lids that are not shut, or on their way there.
    pub fn count(&self) -> usize {
        self.lids.len()
    }

    /// The stage a lid is at, if it is being kept track of.
    pub fn stage(&self, cell: [i32; 3]) -> Option<u8> {
        self.lids.iter().find(|l| l.cell == cell).map(|l| l.stage)
    }

    /// Move time on. `wanted` is the chest that should be open (the one the
    /// player is looking into), if any; every other lid shuts. `stage_of`
    /// says what stage the world shows at a cell, `None` where there is no
    /// chest (any more). Returns what changed, in order.
    pub fn update(&mut self, dt: f32, wanted: Option<[i32; 3]>, stage_of: &dyn Fn([i32; 3]) -> Option<u8>) -> Vec<Change> {
        let mut out = Vec::new();
        let dt = dt.clamp(0.0, 0.25);
        // A chest that is no longer there needs nothing more.
        self.lids.retain(|l| stage_of(l.cell).is_some());
        if let Some(cell) = wanted {
            if let Some(stage) = stage_of(cell) {
                match self.lids.iter_mut().find(|l| l.cell == cell) {
                    Some(l) => {
                        if l.target != OPEN {
                            l.target = OPEN;
                            out.push(Change { cell, stage: l.stage, noise: Some(Noise::Open) });
                        }
                    }
                    None => {
                        // Whatever stage the world shows is where it starts
                        // (a chest another player left half open too).
                        self.lids.push(Lid { cell, stage, target: OPEN, timer: 0.0 });
                        out.push(Change { cell, stage, noise: if stage < OPEN { Some(Noise::Open) } else { None } });
                    }
                }
            }
        }
        for l in self.lids.iter_mut() {
            if Some(l.cell) != wanted && l.target != 0 {
                l.target = 0;
                if l.stage > 0 {
                    out.push(Change { cell: l.cell, stage: l.stage, noise: Some(Noise::Close) });
                }
            }
            if l.stage == l.target {
                l.timer = 0.0;
                continue;
            }
            l.timer += dt;
            while l.timer >= STEP && l.stage != l.target {
                l.timer -= STEP;
                l.stage = if l.stage < l.target { l.stage + 1 } else { l.stage - 1 };
                out.push(Change { cell: l.cell, stage: l.stage, noise: None });
            }
        }
        self.lids.retain(|l| l.stage != 0 || l.target != 0);
        out
    }
}

/// What `b` is as a chest: the shut chest of its family and how far its lid
/// is raised (0 shut), or `None` if it is not a chest with lid stages.
pub fn chest_stage(b: Block) -> Option<(Block, u8)> {
    let d = block::def(b);
    if let Some(lid) = d.x.tag("chest_lid") {
        let shut = d.x.rest?;
        return Some((shut + (b - d.first), (lid as i32).clamp(1, OPEN as i32) as u8));
    }
    if d.usable != block::Usable::Chest || d.rotations != 4 {
        return None;
    }
    // A shut chest has stages only if the data made them.
    reg().block(&alloc::format!("{}_lid1", d.id))?;
    Some((b, 0))
}

/// The block that shows the chest `b` (any stage of it) at lid stage `stage`.
pub fn at_stage(b: Block, stage: u8) -> Option<Block> {
    let (shut, _) = chest_stage(b)?;
    if stage == 0 {
        return Some(shut);
    }
    let d = block::def(shut);
    let lid = reg().block(&alloc::format!("{}_lid{}", d.id, stage.min(OPEN)))?;
    Some(lid + (shut - d.first))
}

/// Once a frame: step the lids and write the new stages into the world.
/// `g.chest_view` is the chest whose screen is open (set by the app).
pub fn tick(g: &mut Session, dt: f32, audio: &mut Audio) {
    if g.chest_view.is_none() && g.chest_anim.count() == 0 {
        return;
    }
    // The chest is only open while its screen is, and while the player is near.
    let near = |c: [i32; 3]| {
        let p = g.player.pos;
        let (dx, dy, dz) = (c[0] as f64 + 0.5 - p[0], c[1] as f64 + 0.5 - p[1], c[2] as f64 + 0.5 - p[2]);
        dx * dx + dy * dy + dz * dz <= REACH * REACH
    };
    let wanted = g.chest_view.filter(|&c| near(c));
    let changes = {
        let world = &g.world;
        g.chest_anim.update(dt, wanted, &|c| chest_stage(world.get(c[0], c[1], c[2])).map(|s| s.1))
    };
    for ch in changes {
        let [x, y, z] = ch.cell;
        let now = g.world.get(x, y, z);
        if let Some(b) = at_stage(now, ch.stage) {
            if b != now {
                g.world.set_block(x, y, z, b);
            }
        }
        if let Some(n) = ch.noise {
            let d = [x as f64 + 0.5 - g.player.pos[0], y as f64 + 0.5 - g.player.pos[1], z as f64 + 0.5 - g.player.pos[2]];
            let far = crate::math::sqrtf((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) as f32);
            audio.play_named(if n == Noise::Open { "chest_open" } else { "chest_close" }, (1.0 - far / 16.0).clamp(0.1, 1.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::RefCell;

    const A: [i32; 3] = [1, 2, 3];
    const B: [i32; 3] = [5, 2, 3];

    /// A little world of two chests whose stages the test reads and the
    /// "session" writes back.
    struct Shelf(RefCell<Vec<([i32; 3], u8)>>);

    impl Shelf {
        fn run(&self, anim: &mut ChestAnim, dt: f32, wanted: Option<[i32; 3]>) -> Vec<Change> {
            let list = self.0.borrow().clone();
            let out = anim.update(dt, wanted, &|c| list.iter().find(|e| e.0 == c).map(|e| e.1));
            for ch in &out {
                if let Some(e) = self.0.borrow_mut().iter_mut().find(|e| e.0 == ch.cell) {
                    e.1 = ch.stage;
                }
            }
            out
        }
        fn stage(&self, c: [i32; 3]) -> u8 {
            self.0.borrow().iter().find(|e| e.0 == c).unwrap().1
        }
    }

    fn shelf() -> Shelf {
        Shelf(RefCell::new(alloc::vec![(A, 0), (B, 0)]))
    }

    #[test]
    fn a_lid_rises_stage_by_stage_and_makes_one_sound() {
        let (s, mut a) = (shelf(), ChestAnim::default());
        let first = s.run(&mut a, 0.0, Some(A));
        assert_eq!(first, [Change { cell: A, stage: 0, noise: Some(Noise::Open) }]);
        let mut seen = alloc::vec![];
        for _ in 0..10 {
            for c in s.run(&mut a, STEP, Some(A)) {
                seen.push((c.stage, c.noise));
            }
        }
        assert_eq!(seen, [(1, None), (2, None), (3, None)]);
        assert_eq!(s.stage(A), OPEN);
        // Still open while looked into, and nothing more happens.
        assert!(s.run(&mut a, 1.0, Some(A)).is_empty());
        assert_eq!(a.count(), 1);
    }

    #[test]
    fn a_slow_frame_moves_several_stages_and_a_fast_one_none() {
        let (s, mut a) = (shelf(), ChestAnim::default());
        s.run(&mut a, 0.0, Some(A));
        s.run(&mut a, STEP * 0.4, Some(A));
        assert_eq!(s.stage(A), 0);
        s.run(&mut a, STEP * 0.7, Some(A));
        assert_eq!(s.stage(A), 1);
        s.run(&mut a, STEP * 2.1, Some(A));
        assert_eq!(s.stage(A), OPEN);
    }

    #[test]
    fn closing_lowers_it_and_forgets_it() {
        let (s, mut a) = (shelf(), ChestAnim::default());
        s.run(&mut a, 0.0, Some(A));
        s.run(&mut a, 1.0, Some(A));
        assert_eq!(s.stage(A), OPEN);
        let out = s.run(&mut a, 0.0, None);
        assert_eq!(out, [Change { cell: A, stage: OPEN, noise: Some(Noise::Close) }]);
        let down: Vec<u8> = (0..5).flat_map(|_| s.run(&mut a, STEP, None)).map(|c| c.stage).collect();
        assert_eq!(down, [2, 1, 0]);
        assert_eq!(a.count(), 0);
        assert!(s.run(&mut a, 1.0, None).is_empty());
    }

    #[test]
    fn opening_another_chest_shuts_the_first() {
        let (s, mut a) = (shelf(), ChestAnim::default());
        s.run(&mut a, 0.0, Some(A));
        s.run(&mut a, 1.0, Some(A));
        let out = s.run(&mut a, 0.0, Some(B));
        assert!(out.contains(&Change { cell: A, stage: OPEN, noise: Some(Noise::Close) }));
        assert!(out.contains(&Change { cell: B, stage: 0, noise: Some(Noise::Open) }));
        s.run(&mut a, 1.0, Some(B));
        assert_eq!((s.stage(A), s.stage(B)), (0, OPEN));
    }

    #[test]
    fn reopening_during_the_close_turns_it_round_without_a_jump() {
        let (s, mut a) = (shelf(), ChestAnim::default());
        s.run(&mut a, 0.0, Some(A));
        s.run(&mut a, 1.0, Some(A));
        s.run(&mut a, 0.0, None);
        s.run(&mut a, STEP, None);
        assert_eq!(s.stage(A), 2);
        let out = s.run(&mut a, 0.0, Some(A));
        assert_eq!(out, [Change { cell: A, stage: 2, noise: Some(Noise::Open) }]);
        s.run(&mut a, 1.0, Some(A));
        assert_eq!(s.stage(A), OPEN);
    }

    #[test]
    fn a_chest_found_open_starts_from_where_it_stands() {
        // Another player's chest (or one left open) is shut by closing the screen of it.
        let s = Shelf(RefCell::new(alloc::vec![(A, 2)]));
        let mut a = ChestAnim::default();
        let out = s.run(&mut a, 0.0, Some(A));
        assert_eq!(out, [Change { cell: A, stage: 2, noise: Some(Noise::Open) }]);
        let mut a = ChestAnim::default();
        s.0.borrow_mut()[0].1 = OPEN;
        assert!(s.run(&mut a, 0.0, Some(A)).is_empty(), "already wide open: no sound");
    }

    #[test]
    fn a_broken_chest_is_forgotten() {
        let (s, mut a) = (shelf(), ChestAnim::default());
        s.run(&mut a, 0.0, Some(A));
        s.0.borrow_mut().retain(|e| e.0 != A);
        assert!(s.run(&mut a, 1.0, Some(A)).is_empty());
        assert_eq!(a.count(), 0);
        // And a cell that holds no chest is never opened.
        assert!(s.run(&mut a, 1.0, Some([9, 9, 9])).is_empty());
    }

    #[test]
    fn the_data_has_every_stage_of_every_chest() {
        let r = reg();
        let mut chests = 0;
        for (i, d) in r.blocks.iter().enumerate().filter(|(i, d)| *i == d.first as usize && d.id.ends_with("_chest") && r.block(&alloc::format!("{}_lid1", d.id)).is_some()) {
            let _ = i;
            chests += 1;
            for turn in 0..4 {
                let shut = d.first + turn;
                assert_eq!(chest_stage(shut), Some((shut, 0)), "{}", d.id);
                let mut last = at_stage(shut, 0).unwrap();
                assert_eq!(last, shut);
                for stage in 1..=OPEN {
                    let b = at_stage(shut, stage).unwrap();
                    assert_ne!(b, last);
                    assert_eq!(chest_stage(b), Some((shut, stage)), "{} stage {}", d.id, stage);
                    assert_eq!(block::at_rest(b), shut, "a saved open chest is read back shut");
                    assert_eq!(block::def(b).usable, block::Usable::Chest);
                    // The same way round at every stage, and the same front.
                    assert_eq!(block::def(b).front.map(|f| f.0), block::def(shut).front.map(|f| f.0));
                    last = b;
                }
            }
        }
        assert!(chests >= 20, "{chests}");
    }

    #[test]
    fn only_the_front_of_a_chest_has_the_latch_picture() {
        let r = reg();
        let oak = r.block("oak_chest").unwrap();
        let fronts: Vec<u8> = (0..4).map(|t| block::def(oak + t).front.unwrap().0).collect();
        // +z for a chest placed facing north, then a quarter turn each: +x, -z, -x.
        assert_eq!(fronts, [4, 0, 5, 1]);
        let d = block::def(oak);
        assert_ne!(d.front.unwrap().1, d.tex[1], "the front is its own picture");
        assert_ne!(d.tex[0], d.tex[1], "the top is not the side");
    }
}
