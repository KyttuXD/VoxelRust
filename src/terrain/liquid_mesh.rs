//! How high the surface of a liquid is drawn.
//!
//! A liquid cell's top is not one flat height: each of its four top corners
//! stands at a height worked out from the cells around that corner, so a
//! source beside a lower flow slopes down toward it, a flow slopes toward
//! where it is thinner, and two neighbouring cells of the same liquid meet
//! exactly along their shared edge (the corner is the same point for both).
//! That is why faces between two cells of the same liquid are never drawn:
//! there is no gap and nothing to see.
//!
//! The heights are in sixteenths of a block. A source reaches 14, a level-7
//! flow 12, a level-1 flow 1; where the same liquid is above, a full 16.

use crate::block::{self, Block};

/// How high a liquid cell stands when it is flat, in sixteenths: full where
/// the same liquid is above it, otherwise by how much of it there is. (The
/// same heights `game/current.rs` swims and floats by.)
pub fn flat_top(b: Block, above: Block) -> i32 {
    if block::same_fluid(b, above) {
        16
    } else {
        block::fluid_of(b).map_or(14, |(_, level)| (14 * level as i32 / 8).max(1))
    }
}

/// The height of the top corner of the liquid cell `b` at (x, y, z) that is
/// toward `cx` and `cz` (each -1 or 1), in sixteenths. `get` gives the block
/// at a cell (`u16::MAX` for what is not known).
///
/// The four cells that meet at the corner are looked at: if any of them has
/// the same liquid above it the corner is full height; otherwise it is the
/// average of the heights of those holding the liquid (a source counts ten
/// times as much as a flow), with open cells counting as height 0; solid
/// cells and unknown ones do not count.
pub fn corner_top(get: &dyn Fn(i32, i32, i32) -> Block, b: Block, x: i32, y: i32, z: i32, cx: i32, cz: i32) -> i32 {
    let mut sum = 0;
    let mut weight = 0;
    for (dx, dz) in [(0, 0), (cx, 0), (0, cz), (cx, cz)] {
        let (px, pz) = (x + dx, z + dz);
        let here = get(px, y, pz);
        if here == u16::MAX {
            continue;
        }
        if block::same_fluid(b, here) {
            let above = get(px, y + 1, pz);
            if block::same_fluid(here, above) {
                return 16;
            }
            let w = if matches!(block::fluid_of(here), Some((_, 8))) { 10 } else { 1 };
            sum += flat_top(here, above) * w;
            weight += w;
        } else if !block::is_solid(here) && block::fluid_of(here).is_none() {
            // Open air (or a plant, a torch...) beside it: the surface falls away toward it.
            weight += 1;
        }
    }
    if weight == 0 {
        flat_top(b, get(x, y + 1, z))
    } else {
        (sum / weight).max(1)
    }
}

/// Whether the face of liquid cell `b` toward neighbour `nb` is hidden
/// because the neighbour holds the same liquid (any level).
pub fn face_hidden(b: Block, nb: Block) -> bool {
    block::same_fluid(b, nb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{reg, AIR};

    /// A world of `put` cells over air, with stone below y = 10.
    fn world(put: Vec<([i32; 3], Block)>) -> impl Fn(i32, i32, i32) -> Block {
        move |x, y, z| {
            for (p, b) in &put {
                if *p == [x, y, z] {
                    return *b;
                }
            }
            if y < 10 {
                reg().wk.stone
            } else {
                AIR
            }
        }
    }

    #[test]
    fn flat_heights_follow_the_level() {
        let r = reg();
        assert_eq!(flat_top(r.wk.water, AIR), 14);
        assert_eq!(flat_top(r.flow[0][7], AIR), 12);
        assert_eq!(flat_top(r.flow[0][1], AIR), 1);
        assert_eq!(flat_top(r.flow[0][3], r.flow[0][7]), 16);
        // Lava is drawn by the same rule.
        assert_eq!(flat_top(r.wk.lava, AIR), 14);
        assert_eq!(flat_top(r.flow[1][6], AIR), 10);
    }

    #[test]
    fn a_pool_is_flat_inside_and_slopes_down_at_the_edge() {
        let r = reg();
        let mut put = Vec::new();
        for x in 0..5 {
            for z in 0..5 {
                put.push(([x, 10, z], r.wk.water));
            }
        }
        let get = world(put);
        // The middle of the pool: every corner at 14.
        assert_eq!(corner_top(&get, r.wk.water, 2, 10, 2, 1, 1), 14);
        assert_eq!(corner_top(&get, r.wk.water, 2, 10, 2, -1, -1), 14);
        // The corner of the pool (open ground beside it) is lower than 14, but not flat 0.
        let edge = corner_top(&get, r.wk.water, 0, 10, 0, -1, -1);
        assert!(edge < 14 && edge > 0, "edge = {edge}");
    }

    #[test]
    fn a_source_beside_a_lower_flow_slopes_toward_it() {
        let r = reg();
        let get = world(vec![([0, 10, 0], r.wk.water), ([1, 10, 0], r.flow[0][3])]);
        // The source's corner toward the flow is lower than its far corner.
        let near = corner_top(&get, r.wk.water, 0, 10, 0, 1, 1);
        let far = corner_top(&get, r.wk.water, 0, 10, 0, -1, 1);
        assert!(near < 14, "near = {near}");
        assert!(far <= 14);
        assert!(near < 14 && near > flat_top(r.flow[0][3], AIR) - 1);
    }

    #[test]
    fn neighbours_agree_on_their_shared_corners() {
        let r = reg();
        let get = world(vec![([0, 10, 0], r.flow[0][5]), ([1, 10, 0], r.flow[0][4]), ([0, 10, 1], r.flow[0][6]), ([1, 10, 1], r.flow[0][2])]);
        // The corner where the four meet is the same height from each cell.
        let a = corner_top(&get, r.flow[0][5], 0, 10, 0, 1, 1);
        let b = corner_top(&get, r.flow[0][4], 1, 10, 0, -1, 1);
        let c = corner_top(&get, r.flow[0][6], 0, 10, 1, 1, -1);
        let d = corner_top(&get, r.flow[0][2], 1, 10, 1, -1, -1);
        assert_eq!((a, b, c), (b, c, d));
        assert_eq!(a, d);
    }

    #[test]
    fn a_column_of_water_is_full_at_every_corner_below_the_top() {
        let r = reg();
        let get = world(vec![([0, 10, 0], r.wk.water), ([0, 11, 0], r.flow[0][7])]);
        assert_eq!(corner_top(&get, r.wk.water, 0, 10, 0, 1, 1), 16);
        // The top cell is not full.
        assert!(corner_top(&get, r.flow[0][7], 0, 11, 0, 1, 1) < 16);
    }

    #[test]
    fn solid_neighbours_do_not_pull_the_surface_down() {
        let r = reg();
        let stone = r.wk.stone;
        let get = world(vec![([0, 10, 0], r.wk.water), ([1, 10, 0], stone), ([0, 10, 1], stone), ([1, 10, 1], stone)]);
        assert_eq!(corner_top(&get, r.wk.water, 0, 10, 0, 1, 1), 14);
    }

    #[test]
    fn faces_between_the_same_liquid_are_hidden_and_between_two_liquids_are_not() {
        let r = reg();
        assert!(face_hidden(r.wk.water, r.flow[0][2]));
        assert!(!face_hidden(r.wk.water, r.wk.lava));
        assert!(!face_hidden(r.wk.water, AIR));
        assert!(!face_hidden(r.wk.water, r.wk.stone));
    }
}
