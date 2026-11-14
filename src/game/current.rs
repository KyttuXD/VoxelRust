//! The push of running water (and lava), and how far a body is under the
//! surface. One place for the player, the creatures and the dropped items
//! to share, so they are all carried the same way.
//!
//! A liquid cell runs toward the neighbours where there is less of it, and
//! toward open air beside it that has nothing under it (a drop). The push
//! is at most 1; lava runs at half of it.

use crate::block::{self, Block};
use crate::math::{floord, sqrtf};

/// How high the surface of a liquid stands in its cell, in blocks: full
/// where the same liquid is above it, otherwise by its level (the mesher
/// draws the same heights, see terrain/liquid_mesh.rs).
pub fn surface_height(b: Block, above: Block) -> f32 {
    if block::same_fluid(b, above) {
        1.0
    } else {
        block::fluid_of(b).map_or(0.0, |(_, level)| (14 * level as i32 / 8) as f32 / 16.0)
    }
}

/// Which way the liquid in cell `c` runs, as a push of at most 1 along x and
/// z: toward neighbours where it is shallower, or open air over a drop.
/// Sources do not push, and neither does a cell that is not liquid.
pub fn current(get: &dyn Fn(i32, i32, i32) -> Block, c: [i32; 3]) -> (f32, f32) {
    let Some((lava, level)) = block::fluid_of(get(c[0], c[1], c[2])) else { return (0.0, 0.0) };
    if level >= 8 {
        return (0.0, 0.0);
    }
    let (mut x, mut z) = (0.0f32, 0.0f32);
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let n = get(c[0] + dx, c[1], c[2] + dz);
        let lower = match block::fluid_of(n) {
            Some((k, l)) if k == lava => level as f32 - l as f32,
            // The other liquid does not carry this one.
            Some(_) => 0.0,
            None if !block::is_solid(n) && !block::is_solid(get(c[0] + dx, c[1] - 1, c[2] + dz)) => level as f32,
            None => 0.0,
        };
        if lower > 0.0 {
            x += dx as f32 * lower;
            z += dz as f32 * lower;
        }
    }
    let len = sqrtf(x * x + z * z);
    let (x, z) = if len > 1.0 { (x / len, z / len) } else { (x, z) };
    if lava {
        (x * 0.5, z * 0.5)
    } else {
        (x, z)
    }
}

/// The push on a body at `pos` (its feet), felt at the height of its middle
/// (0.4 above the feet, as the player's own test for being in water is).
pub fn push_at(get: &dyn Fn(i32, i32, i32) -> Block, pos: [f64; 3]) -> (f32, f32) {
    current(get, [floord(pos[0]), floord(pos[1] + 0.4), floord(pos[2])])
}

/// The height of the surface of the liquid in the column through the cell
/// `c` (which must hold liquid): the top of the highest liquid cell of the
/// run that goes up from it. `None` if `c` holds none.
pub fn surface_at(get: &dyn Fn(i32, i32, i32) -> Block, c: [i32; 3]) -> Option<f64> {
    block::fluid_of(get(c[0], c[1], c[2]))?;
    let mut y = c[1];
    // Up to 6 cells: more than a body is tall.
    for _ in 0..6 {
        if block::same_fluid(get(c[0], y, c[2]), get(c[0], y + 1, c[2])) {
            y += 1;
        } else {
            break;
        }
    }
    let top = surface_height(get(c[0], y, c[2]), get(c[0], y + 1, c[2]));
    Some(y as f64 + top as f64)
}

/// How much of a body standing at `pos` (feet) with this `height` is under
/// the surface, 0 to 1. Zero when the cell at its middle holds no liquid.
pub fn submerged(get: &dyn Fn(i32, i32, i32) -> Block, pos: [f64; 3], height: f64) -> f32 {
    let c = [floord(pos[0]), floord(pos[1] + 0.4), floord(pos[2])];
    match surface_at(get, c) {
        Some(top) if height > 0.0 => {
            let f = (top - pos[1]) / height;
            if f < 0.0 {
                0.0
            } else if f > 1.0 {
                1.0
            } else {
                f as f32
            }
        }
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{reg, AIR};

    /// Stone below y = 10, then whatever `put` says.
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
    fn a_still_source_does_not_push() {
        let w = world(vec![([0, 10, 0], reg().wk.water)]);
        assert_eq!(current(&w, [0, 10, 0]), (0.0, 0.0));
        assert_eq!(current(&w, [5, 10, 5]), (0.0, 0.0));
    }

    #[test]
    fn a_flow_runs_toward_where_it_is_shallower() {
        let r = reg();
        let w = world(vec![([0, 10, 0], r.flow[0][5]), ([1, 10, 0], r.flow[0][4]), ([2, 10, 0], r.flow[0][3]), ([-1, 10, 0], r.flow[0][6])]);
        let (x, z) = current(&w, [0, 10, 0]);
        // Shallower to +x by 1, deeper to -x: pushed toward +x.
        assert!(x > 0.0 && z == 0.0, "{x} {z}");
        assert!(x <= 1.0);
    }

    #[test]
    fn a_flow_runs_over_an_edge_but_not_onto_flat_ground() {
        let r = reg();
        // Beside the flow: air with stone under it (no push), then a hole.
        let w = world(vec![([0, 10, 0], r.flow[0][3])]);
        assert_eq!(current(&w, [0, 10, 0]), (0.0, 0.0));
        let hole = [([0, 10, 0], r.flow[0][3]), ([1, 9, 0], AIR)];
        let w = world(hole.to_vec());
        let (x, _) = current(&w, [0, 10, 0]);
        assert!(x > 0.0, "{x}");
    }

    #[test]
    fn lava_runs_at_half_the_strength_and_is_not_carried_by_water() {
        let r = reg();
        let hole_l = [([0, 10, 0], r.flow[1][3]), ([1, 9, 0], AIR)];
        let hole_w = [([0, 10, 0], r.flow[0][3]), ([1, 9, 0], AIR)];
        let (l, _) = current(&world(hole_l.to_vec()), [0, 10, 0]);
        let (w, _) = current(&world(hole_w.to_vec()), [0, 10, 0]);
        assert!((l * 2.0 - w).abs() < 1e-5, "{l} {w}");
        // Water beside lava does not carry the lava along.
        let mixed = [([0, 10, 0], r.flow[1][3]), ([1, 10, 0], r.flow[0][1])];
        assert_eq!(current(&world(mixed.to_vec()), [0, 10, 0]).0, 0.0);
    }

    #[test]
    fn the_surface_stands_at_the_level_and_a_body_is_under_it_by_a_fraction() {
        let r = reg();
        let w = world(vec![([0, 10, 0], r.wk.water), ([0, 11, 0], r.wk.water)]);
        let top = surface_at(&w, [0, 10, 0]).unwrap();
        // Source: 14/16 of the upper cell.
        assert!((top - 11.875).abs() < 1e-9, "{top}");
        assert!(surface_at(&w, [0, 12, 0]).is_none());
        // A 1.8 tall body standing on the floor: 1.875 of water over its feet, all of it.
        assert_eq!(submerged(&w, [0.5, 10.0, 0.5], 1.8), 1.0);
        // Floating higher, the top of it is out.
        let f = submerged(&w, [0.5, 10.875, 0.5], 1.8);
        assert!(f > 0.4 && f < 0.7, "{f}");
        // Out of the water.
        assert_eq!(submerged(&w, [3.5, 10.0, 0.5], 1.8), 0.0);
    }
}
