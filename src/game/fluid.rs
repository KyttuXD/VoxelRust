//! Water and lava that flow (water: a quarter of a second a block, lava a second and a half; plants are washed away; two sources make a third; lava meeting water sets to obsidian or cobblestone).
//!
//! A liquid cell is one block per level: the source (level 8, the blocks
//! `water` and `lava`), and flows at levels 1 to 7 (`water_3`, `lava_5`, and
//! so on). A level-7 flow is what falls down a column. Water runs one level
//! down per block sideways, so it reaches 7 blocks from a source; lava runs
//! two levels down per block, so it reaches 3.
//!
//! Flow takes time: a cell is looked at a quarter of a second after what
//! feeds it changed for water, a second and a half for lava (see [`Queue`]).
//! When a block changes, the liquid cells
//! around it are looked at again (see [`tick`]): each flow works out its
//! level from what feeds it, drains to air when nothing does, and spreads
//! into air it can reach. A source never changes. Changes made here are
//! themselves changes, so the flow carries on from one tick to the next
//! until it settles.

use crate::block::{self, reg, Block, AIR};
use crate::world::World;
use alloc::collections::BTreeMap;

/// The four sideways neighbours.
const SIDES: [[i32; 3]; 4] = [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]];
const UP: [i32; 3] = [0, 1, 0];
const DOWN: [i32; 3] = [0, -1, 0];

/// How many levels a liquid loses per block it runs sideways.
fn loss(lava: bool) -> u8 {
    if lava {
        2
    } else {
        1
    }
}

/// Whether a liquid may take the cell: air, and plants, which it washes away.
fn open(b: Block) -> bool {
    b == AIR || reg().kind[b as usize] == crate::block::Kind::Cross
}

fn at(w: &World, c: [i32; 3], d: [i32; 3]) -> ([i32; 3], Block) {
    let p = [c[0] + d[0], c[1] + d[1], c[2] + d[2]];
    (p, w.get(p[0], p[1], p[2]))
}

fn set(w: &mut World, p: [i32; 3], b: Block) {
    w.set_block(p[0], p[1], p[2], b);
}

/// Work out one cell. Anything that is not a liquid is left alone.
pub fn step(w: &mut World, c: [i32; 3]) {
    let r = reg();
    let Some((lava, level)) = block::fluid_of(w.get(c[0], c[1], c[2])) else { return };
    // Lava that touches water hardens to stone; water is never changed by it.
    if lava && [UP, DOWN, SIDES[0], SIDES[1], SIDES[2], SIDES[3]].iter().any(|&d| matches!(block::fluid_of(at(w, c, d).1), Some((false, _)))) {
        // A source turns to obsidian (stone if the game has none), a flow to cobblestone.
        let hard = if level == 8 { r.block("obsidian").unwrap_or(r.wk.stone) } else { r.block("cobble").unwrap_or(r.wk.stone) };
        set(w, c, hard);
        return;
    }
    if level == 8 {
        spread(w, c, lava, 8);
        return;
    }
    // A flow takes the most it can from what feeds it: the same liquid
    // falling onto it, or a neighbour's level less one step.
    let mut best = if matches!(block::fluid_of(at(w, c, UP).1), Some((k, _)) if k == lava) { 7 } else { 0 };
    for d in SIDES {
        if let Some((k, l)) = block::fluid_of(at(w, c, d).1) {
            if k == lava {
                best = best.max(l.saturating_sub(loss(lava)));
            }
        }
    }
    // Water with two sources beside it, over something firm, becomes a source too.
    if !lava {
        let sources = SIDES.iter().filter(|&&d| matches!(block::fluid_of(at(w, c, d).1), Some((false, 8)))).count();
        let (_, under) = at(w, c, DOWN);
        if sources >= 2 && (block::is_solid(under) || matches!(block::fluid_of(under), Some((false, 8)))) {
            set(w, c, r.wk.water);
            return;
        }
    }
    if best == 0 {
        set(w, c, AIR);
        return;
    }
    if best != level {
        set(w, c, r.flow[lava as usize][best as usize]);
    }
    spread(w, c, lava, best);
}

/// A liquid at `level` pushes into the air around it. Air below comes first:
/// the liquid falls, and only spreads sideways once it lands on something.
/// A falling column (level 7) on top of more of the same liquid hands its
/// spreading to the liquid below.
fn spread(w: &mut World, c: [i32; 3], lava: bool, level: u8) {
    let r = reg();
    let (below, under) = at(w, c, DOWN);
    if open(under) {
        set(w, below, r.flow[lava as usize][7]);
        return;
    }
    if level == 7 && block::same_fluid(under, r.flow[lava as usize][7]) {
        return;
    }
    let step = loss(lava);
    if level <= step {
        return;
    }
    for d in SIDES {
        let (p, nb) = at(w, c, d);
        if open(nb) {
            set(w, p, r.flow[lava as usize][(level - step) as usize]);
        }
    }
}

/// Look again at the liquid cells around each changed cell (the cell itself
/// and its six neighbours). Called once per tick with the cells that changed.
pub fn tick(w: &mut World, changed: &[[i32; 3]]) {
    for &c in changed {
        let around = [[0, 0, 0], UP, DOWN, SIDES[0], SIDES[1], SIDES[2], SIDES[3]];
        for d in around {
            let p = [c[0] + d[0], c[1] + d[1], c[2] + d[2]];
            if block::fluid_of(w.get(p[0], p[1], p[2])).is_some() {
                step(w, p);
            }
        }
    }
}

/// Cells waiting to be looked at again, each with the time it is due.
#[derive(Default)]
pub struct Queue {
    now: f32,
    due: BTreeMap<[i32; 3], f32>,
}

/// Seconds a liquid takes to move one step.
const WATER_DELAY: f32 = 0.25;
const LAVA_DELAY: f32 = 1.5;
/// Most cells looked at in one go, and most waiting.
const PER_TICK: usize = 300;
const WAITING: usize = 6000;

impl Queue {
    /// Note what changed (liquid cells in and around it wait their turn), advance the
    /// clock by `dt`, and work on every cell whose time has come.
    pub fn run(&mut self, w: &mut World, changed: &[[i32; 3]], dt: f32) {
        self.now += dt;
        let around = [[0, 0, 0], UP, DOWN, SIDES[0], SIDES[1], SIDES[2], SIDES[3]];
        for &c in changed {
            for d in around {
                let p = [c[0] + d[0], c[1] + d[1], c[2] + d[2]];
                let Some((lava, _)) = block::fluid_of(w.get(p[0], p[1], p[2])) else { continue };
                if self.due.len() >= WAITING || self.due.contains_key(&p) {
                    continue;
                }
                // Lava meeting water sets at once; otherwise each kind has its pace.
                let touches_water = lava && [UP, DOWN, SIDES[0], SIDES[1], SIDES[2], SIDES[3]].iter().any(|&e| block::is_water(at(w, p, e).1));
                let wait = if lava && !touches_water { LAVA_DELAY } else if lava { 0.0 } else { WATER_DELAY };
                self.due.insert(p, self.now + wait);
            }
        }
        let now = self.now;
        let ready: alloc::vec::Vec<[i32; 3]> = self.due.iter().filter(|(_, &t)| t <= now).map(|(&c, _)| c).take(PER_TICK).collect();
        for c in ready {
            self.due.remove(&c);
            if w.is_loaded(c[0], c[2]) {
                step(w, c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::Pool;
    use crate::world::{GpuMesh, MeshSink, NullStore};
    use crate::mesh::MeshData;

    struct NoSink;
    impl MeshSink for NoSink {
        fn upload(&mut self, _: &MeshData) -> GpuMesh {
            GpuMesh::default()
        }
        fn free(&mut self, _: GpuMesh) {}
    }

    /// A loaded flat floor of stone at y = 119, with air above it.
    fn floor() -> World {
        let mut w = World::new(5);
        let mut pool = Pool::new(0, 5);
        for _ in 0..3000 {
            w.update(0, 0, 1, &mut pool, &mut NoSink, &mut NullStore);
            if w.settled {
                break;
            }
        }
        for x in 0..16 {
            for z in 0..16 {
                w.set_block(x, 119, z, reg().wk.stone);
                for y in 120..126 {
                    w.set_block(x, y, z, AIR);
                }
            }
        }
        w.changed.clear();
        w
    }

    /// Let the liquids move until nothing changes (or a long time has passed).
    fn settle(w: &mut World) -> usize {
        let mut rounds = 0;
        while !w.changed.is_empty() && rounds < 200 {
            let pending = core::mem::take(&mut w.changed);
            tick(w, &pending);
            rounds += 1;
        }
        rounds
    }

    fn state(w: &World, p: [i32; 3]) -> Block {
        w.get(p[0], p[1], p[2])
    }

    fn level_at(w: &World, p: [i32; 3]) -> Option<(bool, u8)> {
        block::fluid_of(state(w, p))
    }

    #[test]
    fn a_source_spreads_across_the_floor_and_thins_out() {
        let mut w = floor();
        let source = reg().wk.water;
        w.set_block(8, 120, 8, source);
        settle(&mut w);
        // The source itself never changes.
        assert_eq!(state(&w, [8, 120, 8]), source);
        // Water runs one level down per block: level 7 next to it, then less.
        assert_eq!(level_at(&w, [9, 120, 8]), Some((false, 7)));
        assert_eq!(level_at(&w, [10, 120, 8]), Some((false, 6)));
        assert_eq!(level_at(&w, [14, 120, 8]), Some((false, 2)));
        // It stops once the level runs out: 7 blocks from the source.
        assert_eq!(state(&w, [16, 120, 8]), AIR);
    }

    #[test]
    fn water_falls_before_it_spreads() {
        let mut w = floor();
        w.set_block(8, 123, 8, reg().wk.water);
        settle(&mut w);
        // Down the column, at level 7 (falling), and onto the floor it lands on.
        assert_eq!(level_at(&w, [8, 122, 8]), Some((false, 7)));
        assert_eq!(level_at(&w, [8, 121, 8]), Some((false, 7)));
        assert_eq!(level_at(&w, [8, 120, 8]), Some((false, 7)));
        // Where it lands it runs out sideways, along the floor.
        assert!(level_at(&w, [9, 120, 8]).is_some());
        assert!(level_at(&w, [11, 120, 8]).is_some());
    }

    #[test]
    fn a_flow_drains_when_its_source_is_taken_away() {
        let mut w = floor();
        let source = reg().wk.water;
        w.set_block(8, 120, 8, source);
        settle(&mut w);
        assert!(level_at(&w, [12, 120, 8]).is_some());
        w.set_block(8, 120, 8, AIR);
        settle(&mut w);
        for x in 0..16 {
            assert_eq!(state(&w, [x, 120, 8]), AIR, "x = {x}");
        }
    }

    #[test]
    fn water_flows_into_a_tunnel_dug_beside_it() {
        let mut w = floor();
        // A pool of source water, behind a wall that runs the whole length of
        // the floor, with the tunnel beyond it.
        let wall = reg().wk.stone;
        for z in 0..16 {
            w.set_block(4, 120, z, wall);
        }
        for x in 0..4 {
            w.set_block(x, 120, 4, reg().wk.water);
        }
        settle(&mut w);
        assert_eq!(state(&w, [5, 120, 4]), AIR);
        // Digging the wall out lets the water in, as the tick sees the change.
        w.set_block(4, 120, 4, AIR);
        settle(&mut w);
        assert!(level_at(&w, [5, 120, 4]).is_some(), "the tunnel is still dry");
    }

    #[test]
    fn lava_reaches_three_blocks_not_seven() {
        let mut w = floor();
        w.set_block(8, 120, 8, reg().wk.lava);
        settle(&mut w);
        assert_eq!(level_at(&w, [9, 120, 8]), Some((true, 6)));
        assert_eq!(level_at(&w, [10, 120, 8]), Some((true, 4)));
        assert_eq!(level_at(&w, [11, 120, 8]), Some((true, 2)));
        assert_eq!(state(&w, [12, 120, 8]), AIR);
    }

    #[test]
    fn lava_that_meets_water_hardens_and_the_water_stays() {
        let mut w = floor();
        w.set_block(8, 120, 8, reg().wk.water);
        w.set_block(9, 120, 8, reg().wk.lava);
        settle(&mut w);
        assert_eq!(state(&w, [9, 120, 8]), reg().wk.stone);
        assert_eq!(state(&w, [8, 120, 8]), reg().wk.water);
    }

    #[test]
    fn every_flow_level_has_a_block() {
        let r = reg();
        for lava in [false, true] {
            for level in 1..=8u8 {
                let b = r.flow[lava as usize][level as usize];
                assert_ne!(b, AIR, "no block for {} level {level}", if lava { "lava" } else { "water" });
                assert_eq!(block::fluid_of(b), Some((lava, level)));
            }
        }
        assert_eq!(block::fluid_of(AIR), None);
    }

    /// Run the timed flow for `seconds`.
    fn run_for(w: &mut World, q: &mut Queue, seconds: f32) {
        for _ in 0..(seconds / 0.05) as i32 {
            let pending = core::mem::take(&mut w.changed);
            q.run(w, &pending, 0.05);
        }
    }

    #[test]
    fn water_takes_time_to_spread_and_lava_more() {
        let mut w = floor();
        let mut q = Queue::default();
        w.set_block(4, 120, 4, reg().wk.water);
        w.set_block(12, 120, 12, reg().wk.lava);
        run_for(&mut w, &mut q, 0.4);
        assert!(level_at(&w, [5, 120, 4]).is_some(), "water has not moved a block in 0.4 s");
        assert!(level_at(&w, [8, 120, 4]).is_none(), "water ran too far too fast");
        assert!(level_at(&w, [11, 120, 12]).is_none(), "lava is quicker than water");
        run_for(&mut w, &mut q, 4.0);
        assert!(level_at(&w, [11, 120, 12]).is_some(), "lava never spread");
        assert!(level_at(&w, [7, 120, 4]).is_some(), "water stopped short of 3 blocks");
    }

    #[test]
    fn water_washes_plants_away() {
        let mut w = floor();
        w.set_block(9, 120, 8, reg().wk.tall_grass);
        w.set_block(8, 120, 8, reg().wk.water);
        settle(&mut w);
        assert!(block::is_water(state(&w, [9, 120, 8])), "the grass held the water back");
    }

    #[test]
    fn two_sources_make_a_third() {
        let mut w = floor();
        w.set_block(6, 120, 8, reg().wk.water);
        w.set_block(8, 120, 8, reg().wk.water);
        settle(&mut w);
        assert_eq!(level_at(&w, [7, 120, 8]), Some((false, 8)));
    }

    #[test]
    fn a_lava_source_met_by_water_is_not_left_as_lava() {
        let mut w = floor();
        w.set_block(8, 120, 8, reg().wk.lava);
        w.set_block(9, 120, 8, reg().wk.water);
        settle(&mut w);
        assert!(block::fluid_of(state(&w, [8, 120, 8])).is_none());
        assert!(state(&w, [8, 120, 8]) != AIR);
    }
}
