//! Water and lava that flow (water: a quarter of a second a block, lava a second and a half; plants and other light things are washed away; two sources make a third; lava meeting water sets to obsidian or cobblestone).
//!
//! A liquid cell is one block per level: the source (level 8, the blocks
//! `water` and `lava`), and flows at levels 1 to 7 (`water_3`, `lava_5`, and
//! so on). A level-7 flow is what falls down a column. Water runs one level
//! down per block sideways, so it reaches 7 blocks from a source; lava runs
//! two levels down per block, so it reaches 3.
//!
//! Flow takes time: a cell is looked at a quarter of a second after what
//! feeds it changed for water, a second and a half for lava (see
//! [`Queue`], in `fluid_queue.rs`, which also spreads a flood over many
//! ticks). When a block changes, the liquid cells around it are looked at
//! again (see [`tick`]): each flow works out its level from what feeds it,
//! drains to air when nothing does, and spreads into the cells it may take
//! (air, and what `game/fluid_rules.rs` says is washed away: plants,
//! torches, carpets, signs, crops; never anything with a collision shape).
//! A source never changes. Changes made here are themselves changes, so the
//! flow carries on from one tick to the next until it settles.
//!
//! Levels only ever fall along a flow away from its sources, so a ring of
//! flows with nothing feeding it runs dry (each cell is the highest of its
//! neighbours less a step) and two liquids cannot trade places for ever:
//! where lava meets water the lava sets and stays set. Cells whose side
//! neighbours are in chunks that are not loaded are never drained (what
//! feeds them cannot be seen) and nothing is written into unloaded chunks.

use crate::block::{self, reg, Block, AIR};
use crate::game::fluid_rules::{self, Touch};
use crate::world::World;
use alloc::vec::Vec;

pub use crate::game::fluid_queue::Queue;

/// The four sideways neighbours.
pub(crate) const SIDES: [[i32; 3]; 4] = [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1]];
pub(crate) const UP: [i32; 3] = [0, 1, 0];
pub(crate) const DOWN: [i32; 3] = [0, -1, 0];

/// How many levels a liquid loses per block it runs sideways.
fn loss(lava: bool) -> u8 {
    if lava {
        2
    } else {
        1
    }
}

fn at(w: &World, c: [i32; 3], d: [i32; 3]) -> ([i32; 3], Block) {
    let p = [c[0] + d[0], c[1] + d[1], c[2] + d[2]];
    (p, w.get(p[0], p[1], p[2]))
}

fn set(w: &mut World, p: [i32; 3], b: Block) {
    w.set_block(p[0], p[1], p[2], b);
}

/// Put liquid into a cell it may take, noting what it washed away.
fn flood(w: &mut World, p: [i32; 3], b: Block, washed: &mut Vec<([i32; 3], Block)>) {
    let old = w.get(p[0], p[1], p[2]);
    if !fluid_rules::can_flow_into(old) {
        return;
    }
    if w.set_block(p[0], p[1], p[2], b) && old != AIR && washed.len() < 256 {
        washed.push((p, old));
    }
}

/// What a cell of lava would set to, if water touches it.
fn lava_hardens(w: &World, c: [i32; 3], level: u8) -> Option<Block> {
    let wet = |d: [i32; 3]| matches!(block::fluid_of(at(w, c, d).1), Some((false, _)));
    let touch = if wet(UP) || SIDES.iter().any(|&d| wet(d)) {
        Touch::AboveOrBeside
    } else if wet(DOWN) {
        Touch::Below
    } else {
        return None;
    };
    fluid_rules::lava_sets(level, touch).map(fluid_rules::hard_block)
}

/// Work out one cell. Anything that is not a liquid is left alone.
pub fn step(w: &mut World, c: [i32; 3]) {
    step_with(w, c, &mut Vec::new());
}

/// Like [`step`], and notes the blocks washed away (cell and block) in `washed`.
pub fn step_with(w: &mut World, c: [i32; 3], washed: &mut Vec<([i32; 3], Block)>) {
    let r = reg();
    let Some((lava, level)) = block::fluid_of(w.get(c[0], c[1], c[2])) else { return };
    // Lava that touches water sets (see the table in fluid_rules.rs); water is never changed by it.
    if lava {
        if let Some(hard) = lava_hardens(w, c, level) {
            set(w, c, hard);
            return;
        }
    }
    if level == 8 {
        spread(w, c, lava, 8, washed);
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
    // What feeds a cell at the edge of the loaded world cannot be seen: keep it as it is.
    let edge = SIDES.iter().any(|d| !w.is_loaded(c[0] + d[0], c[2] + d[2]));
    if edge && best < level {
        spread(w, c, lava, level, washed);
        return;
    }
    if best == 0 {
        set(w, c, AIR);
        return;
    }
    if best != level {
        set(w, c, r.flow[lava as usize][best as usize]);
    }
    spread(w, c, lava, best, washed);
}

/// A liquid at `level` pushes into the cells around it that it may take.
/// Below comes first: the liquid falls, and only spreads sideways once it
/// lands on something. A falling column (level 7) on top of more of the
/// same liquid hands its spreading to the liquid below.
fn spread(w: &mut World, c: [i32; 3], lava: bool, level: u8, washed: &mut Vec<([i32; 3], Block)>) {
    let r = reg();
    let (below, under) = at(w, c, DOWN);
    if fluid_rules::can_flow_into(under) {
        flood(w, below, r.flow[lava as usize][7], washed);
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
        if fluid_rules::can_flow_into(nb) {
            flood(w, p, r.flow[lava as usize][(level - step) as usize], washed);
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

    // ---- Round 2: other blocks, hardening, draining, floods.

    /// A channel one block wide and walled in along x, at z = 8 on the floor.
    fn channel(w: &mut World) {
        for x in 0..16 {
            w.set_block(x, 120, 7, reg().wk.stone);
            w.set_block(x, 120, 9, reg().wk.stone);
        }
        w.changed.clear();
    }

    #[test]
    fn liquids_stop_at_every_block_with_a_collision_shape() {
        let r = reg();
        for id in ["stone_slab", "stone_stairs", "planks_fence", "glass", "oak_door", "oak_door_open", "oak_chest", "big_chest", "cobble_wall", "snow", "log", "leaves"] {
            let Some(b) = r.block(id) else { continue };
            for lava in [false, true] {
                let mut w = floor();
                // Walled in so only the one block is beside the source.
                channel(&mut w);
                w.set_block(9, 120, 8, b);
                let kept = state(&w, [9, 120, 8]);
                w.set_block(8, 120, 8, if lava { r.wk.lava } else { r.wk.water });
                settle(&mut w);
                assert_eq!(state(&w, [9, 120, 8]), kept, "{id} was replaced by {}", if lava { "lava" } else { "water" });
                assert_eq!(state(&w, [10, 120, 8]), AIR, "{id} let the liquid through");
            }
        }
    }

    #[test]
    fn liquid_goes_round_a_slab_never_into_it() {
        let mut w = floor();
        let slab = reg().block("stone_slab").unwrap_or(reg().wk.stone);
        w.set_block(9, 120, 8, slab);
        w.set_block(8, 120, 8, reg().wk.water);
        settle(&mut w);
        assert_eq!(state(&w, [9, 120, 8]), slab);
        assert!(level_at(&w, [9, 120, 9]).is_some(), "it did not go round");
        assert!(level_at(&w, [10, 120, 8]).is_some(), "it did not come out the far side");
    }

    #[test]
    fn washed_away_things_are_reported_so_they_can_drop() {
        let mut w = floor();
        let mut q = Queue::default();
        w.set_block(9, 120, 8, reg().wk.tall_grass);
        w.set_block(8, 120, 8, reg().wk.water);
        run_for(&mut w, &mut q, 1.0);
        assert!(block::is_water(state(&w, [9, 120, 8])));
        assert!(q.washed.iter().any(|&(c, b)| c == [9, 120, 8] && b == reg().wk.tall_grass), "{:?}", q.washed);
        // Plain air is not reported.
        assert!(q.washed.iter().all(|&(_, b)| b != AIR));
    }

    #[test]
    fn a_snow_layer_is_washed_but_a_rail_is_not() {
        let r = reg();
        if let Some(snow) = r.block("snow_layer") {
            assert!(block::def(snow).x.washable);
        }
        if let Some(rail) = r.block("rail") {
            assert!(!block::def(rail).x.washable, "rails stay");
            let mut w = floor();
            w.set_block(9, 120, 8, rail);
            w.set_block(8, 120, 8, r.wk.water);
            settle(&mut w);
            assert_eq!(state(&w, [9, 120, 8]), rail);
        }
    }

    #[test]
    fn lava_source_beside_water_becomes_obsidian_or_stone() {
        let mut w = floor();
        w.set_block(8, 120, 8, reg().wk.lava);
        w.set_block(9, 120, 8, reg().wk.water);
        settle(&mut w);
        assert_eq!(state(&w, [8, 120, 8]), fluid_rules::hard_block(fluid_rules::Hard::Obsidian));
        assert!(block::is_water(state(&w, [9, 120, 8])));
    }

    #[test]
    fn flowing_lava_meeting_water_becomes_cobble() {
        let mut w = floor();
        w.set_block(8, 120, 8, reg().flow[1][5]);
        w.set_block(9, 120, 8, reg().wk.water);
        settle(&mut w);
        assert_eq!(state(&w, [8, 120, 8]), fluid_rules::hard_block(fluid_rules::Hard::Cobble));
    }

    #[test]
    fn water_falling_on_lava_makes_cobble_or_obsidian() {
        let mut w = floor();
        w.set_block(8, 120, 8, reg().flow[1][4]);
        w.set_block(8, 121, 8, reg().wk.water);
        settle(&mut w);
        assert_eq!(state(&w, [8, 120, 8]), fluid_rules::hard_block(fluid_rules::Hard::Cobble));
        let mut w = floor();
        w.set_block(8, 120, 8, reg().wk.lava);
        w.set_block(8, 121, 8, reg().wk.water);
        settle(&mut w);
        assert_eq!(state(&w, [8, 120, 8]), fluid_rules::hard_block(fluid_rules::Hard::Obsidian));
    }

    #[test]
    fn lava_running_onto_water_makes_stone_and_a_source_over_water_stays() {
        let mut w = floor();
        w.set_block(8, 120, 8, reg().wk.water);
        w.set_block(8, 121, 8, reg().flow[1][5]);
        settle(&mut w);
        assert_eq!(state(&w, [8, 121, 8]), reg().wk.stone);
        assert!(block::is_water(state(&w, [8, 120, 8])));
        // Lava source with only water below it is left alone.
        let mut w = floor();
        w.set_block(8, 120, 8, reg().wk.water);
        w.set_block(8, 121, 8, reg().wk.lava);
        settle(&mut w);
        assert_eq!(level_at(&w, [8, 121, 8]), Some((true, 8)));
    }

    #[test]
    fn water_does_not_spread_onto_lava() {
        let mut w = floor();
        w.set_block(8, 120, 8, reg().wk.lava);
        settle(&mut w);
        w.set_block(8, 123, 8, reg().wk.water);
        settle(&mut w);
        // The lava was set (obsidian or stone) instead of being replaced by water.
        assert!(block::fluid_of(state(&w, [8, 120, 8])).is_none());
        assert_ne!(state(&w, [8, 120, 8]), AIR);
    }

    #[test]
    fn a_block_put_in_the_channel_cuts_off_what_it_fed() {
        let mut w = floor();
        channel(&mut w);
        w.set_block(1, 120, 8, reg().wk.water);
        settle(&mut w);
        assert!(level_at(&w, [7, 120, 8]).is_some(), "the channel did not fill");
        w.set_block(3, 120, 8, reg().wk.stone);
        settle(&mut w);
        assert!(level_at(&w, [2, 120, 8]).is_some());
        for x in 4..16 {
            assert_eq!(state(&w, [x, 120, 8]), AIR, "x = {x} still holds water");
        }
    }

    #[test]
    fn a_ring_of_flows_with_no_source_runs_dry_and_settles() {
        let mut w = floor();
        w.set_block(8, 120, 8, reg().wk.stone);
        let ring = [[7, 7], [8, 7], [9, 7], [9, 8], [9, 9], [8, 9], [7, 9], [7, 8]];
        for [x, z] in ring {
            w.set_block(x, 120, z, reg().flow[0][5]);
        }
        let rounds = settle(&mut w);
        assert!(rounds < 200, "it never settled ({rounds} rounds)");
        for [x, z] in ring {
            assert_eq!(state(&w, [x, 120, z]), AIR);
        }
    }

    #[test]
    fn a_lava_and_water_standoff_settles() {
        let mut w = floor();
        for z in 4..12 {
            w.set_block(4, 120, z, reg().wk.water);
            w.set_block(12, 120, z, reg().wk.lava);
        }
        let rounds = settle(&mut w);
        assert!(rounds < 200, "no end to it ({rounds} rounds)");
        // Nothing is left changing: another look at every cell leaves it all as it is.
        let snapshot = |w: &World| -> alloc::vec::Vec<Block> { (0..16).flat_map(|x| (0..16).map(move |z| (x, z))).map(|(x, z)| w.get(x, 120, z)).collect() };
        let before = snapshot(&w);
        for x in 0..16 {
            for z in 0..16 {
                step(&mut w, [x, 120, z]);
            }
        }
        settle(&mut w);
        assert_eq!(before, snapshot(&w));
    }

    #[test]
    fn a_flow_at_the_edge_of_the_loaded_world_is_not_drained() {
        let mut w = floor();
        // The first column outside what is loaded.
        let edge = (0..2000).find(|&x| !w.is_loaded(x, 8)).expect("everything is loaded?");
        assert!(edge > 0);
        let c = [edge - 1, 120, 8];
        w.set_block(c[0], c[1], c[2], reg().flow[0][3]);
        step(&mut w, c);
        assert!(level_at(&w, c).is_some(), "drained for want of a neighbour it could not see");
        // And nothing is written into the chunk beyond.
        assert!(!w.set_block(edge, 120, 8, reg().wk.water));
        assert_eq!(state(&w, [edge, 120, 8]), AIR);
    }

    #[test]
    fn a_flood_of_changes_is_spread_over_many_ticks() {
        let mut w = floor();
        let mut q = Queue::default();
        let mut many = alloc::vec::Vec::new();
        for i in 0..5000 {
            many.push([i % 16, 121, (i / 16) % 16]);
        }
        q.run(&mut w, &many, 0.05);
        assert_eq!(q.pending(), 5000 - crate::game::fluid_queue::EXPAND_PER_TICK);
        let mut ticks = 1;
        while q.pending() > 0 && ticks < 100 {
            q.run(&mut w, &[], 0.05);
            ticks += 1;
        }
        assert_eq!(q.pending(), 0);
        assert!(ticks > 10, "all at once");
    }

    #[test]
    fn a_big_flow_front_is_worked_through_a_few_hundred_cells_a_tick() {
        let mut w = floor();
        let mut q = Queue::default();
        // 16 x 16 sources, three layers: far more cells than one tick may work on.
        for x in 0..16 {
            for z in 0..16 {
                for y in 120..123 {
                    w.set_block(x, y, z, reg().wk.water);
                }
            }
        }
        let pending = core::mem::take(&mut w.changed);
        q.run(&mut w, &pending, 0.0);
        assert!(q.waiting() <= crate::game::fluid_queue::EXPAND_PER_TICK * 7);
        // Running it on, it stays within bounds and finishes.
        for _ in 0..400 {
            let pending = core::mem::take(&mut w.changed);
            q.run(&mut w, &pending, 0.05);
            assert!(q.waiting() <= crate::game::fluid_queue::WAITING);
        }
        assert_eq!(q.pending(), 0);
    }

    #[test]
    fn a_flow_carries_the_same_properties_as_its_source() {
        let r = reg();
        for lava in [false, true] {
            let src = block::def(r.flow[lava as usize][8]);
            for level in 1..8usize {
                let d = block::def(r.flow[lava as usize][level]);
                let what = alloc::format!("{} level {level}", if lava { "lava" } else { "water" });
                assert_eq!(d.sky_loss, src.sky_loss, "{what}: skyloss");
                assert_eq!(d.emit, src.emit, "{what}: light");
                assert_eq!(d.x.hurts, src.x.hurts, "{what}: hurts");
                assert_eq!(d.tex, src.tex, "{what}: texture");
                assert_eq!(d.kind, src.kind, "{what}: shape");
                assert_eq!(d.solid, src.solid, "{what}: solid");
            }
        }
        assert!(block::def(r.wk.water).sky_loss > 0, "water should dim the light");
        assert!(block::def(r.wk.lava).emit > 0, "lava should glow");
    }

    #[test]
    fn a_falling_block_lands_in_water_and_lava() {
        let r = reg();
        for liquid in [r.wk.water, r.wk.lava] {
            let mut w = floor();
            w.set_block(8, 121, 8, liquid);
            w.set_block(8, 122, 8, r.wk.sand);
            assert!(crate::interact::fall_step(&mut w, [8, 122, 8]));
            assert_eq!(state(&w, [8, 121, 8]), r.wk.sand);
            assert_eq!(state(&w, [8, 122, 8]), AIR);
        }
    }

    #[test]
    fn lava_in_a_hot_dimension_runs_faster() {
        assert!(crate::game::fluid_queue::fast_lava_in("nether"));
        assert!(crate::game::fluid_queue::fast_lava_in("the_hell"));
        assert!(!crate::game::fluid_queue::fast_lava_in(""));
        assert!(!crate::game::fluid_queue::fast_lava_in("crystalis"));
        let mut w = floor();
        let mut q = Queue::default();
        q.fast_lava = true;
        w.set_block(8, 120, 8, reg().wk.lava);
        run_for(&mut w, &mut q, 0.8);
        assert!(level_at(&w, [9, 120, 8]).is_some(), "hot lava is as slow as ordinary lava");
    }
}
