//! The block light on a thing standing somewhere (a creature, a dropped item,
//! a player, the hand), from lava, lamps, torches and the rest of what shines.
//!
//! The world's own light is a flood fill the mesher makes for a chunk (see
//! `terrain/mesh.rs`) and does not keep, so a thing out in the world cannot
//! ask it. This is the cheap stand-in: the strongest emitter within [`R`]
//! blocks (a diamond: `emit - distance`, one less for each block, as the
//! flood fill does) that can be seen from the spot, so a wall hides it. The
//! work is bounded: at most 377 cells are looked at, at most 15 sight lines
//! are walked (each one must beat the best so far by a whole level), and
//! [`LightCache`] caps how many places are worked out in a frame (96) and
//! remembers them for the rest of it.

use crate::block::Block;
use crate::chunk::{CH, CW};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// How far an emitter lights a thing, blocks (Manhattan distance).
pub const R: i32 = 6;
/// Places worked out in one frame, at most, and places remembered.
pub const MAX_FRESH: u32 = 96;
pub const MAX_ENTRIES: usize = 512;
/// Things farther than this from the camera are not lit by blocks at all, blocks.
pub const FAR: f64 = 64.0;

/// One loaded chunk's blocks (`ChunkData::blocks`).
#[derive(Clone, Copy)]
pub struct Slice<'a> {
    pub cx: i32,
    pub cz: i32,
    pub blocks: &'a [Block],
}

/// The block at a cell, `None` where nothing is loaded (or too high or low).
fn block_in(chunks: &[Slice], x: i32, y: i32, z: i32) -> Option<Block> {
    if !(0..CH).contains(&y) {
        return None;
    }
    let (cx, cz) = (x >> 4, z >> 4);
    let s = chunks.iter().find(|s| s.cx == cx && s.cz == cz)?;
    s.blocks.get((((x & 15) * CW + (z & 15)) * CH + y) as usize).copied()
}

/// Whether nothing solid lies between the point `from` and the middle of the
/// cell `to` (the cells they are in do not count).
fn can_see(chunks: &[Slice], opaque: &[u8], from: [f64; 3], to: [i32; 3], start: [i32; 3], dist: i32) -> bool {
    let end = [to[0] as f64 + 0.5, to[1] as f64 + 0.5, to[2] as f64 + 0.5];
    let steps = (dist * 2).max(1);
    for i in 1..steps {
        let t = i as f64 / steps as f64;
        let p = [from[0] + (end[0] - from[0]) * t, from[1] + (end[1] - from[1]) * t, from[2] + (end[2] - from[2]) * t];
        let cell = [crate::math::floord(p[0]), crate::math::floord(p[1]), crate::math::floord(p[2])];
        if cell == start || cell == to {
            continue;
        }
        if let Some(b) = block_in(chunks, cell[0], cell[1], cell[2]) {
            if opaque.get(b as usize).copied().unwrap_or(0) != 0 {
                return false;
            }
        }
    }
    true
}

/// The block light, 0 to 15, on a thing whose feet are at `pos`, from the
/// emitter table `emit` (light by block state) and `opaque` (solid by block
/// state), looking at the loaded `chunks`.
pub fn level_at(chunks: &[Slice], emit: &[u8], opaque: &[u8], pos: [f64; 3]) -> u8 {
    if !pos.iter().all(|v| v.is_finite() && *v > -1.0e9 && *v < 1.0e9) {
        return 0;
    }
    // The spot: the middle of the body, about half a block up.
    let from = [pos[0], pos[1] + 0.5, pos[2]];
    let c = [crate::math::floord(from[0]), crate::math::floord(from[1]), crate::math::floord(from[2])];
    let mut best = 0i32;
    for x in c[0] - R..=c[0] + R {
        for z in c[2] - R..=c[2] + R {
            let flat = (x - c[0]).abs() + (z - c[2]).abs();
            if flat > R {
                continue;
            }
            let (cx, cz) = (x >> 4, z >> 4);
            let s = match chunks.iter().find(|s| s.cx == cx && s.cz == cz) {
                Some(s) => s,
                None => continue,
            };
            let base = ((x & 15) * CW + (z & 15)) * CH;
            let reach = R - flat;
            for y in (c[1] - reach).max(0)..=(c[1] + reach).min(CH - 1) {
                let b = match s.blocks.get((base + y) as usize) {
                    Some(b) => *b,
                    None => continue,
                };
                let e = emit.get(b as usize).copied().unwrap_or(0) as i32;
                if e == 0 {
                    continue;
                }
                let dist = flat + (y - c[1]).abs();
                let level = e - dist;
                if level > best && can_see(chunks, opaque, from, [x, y, z], c, dist) {
                    best = level;
                }
            }
        }
    }
    best.clamp(0, 15) as u8
}

/// A block light level as the 0 to 1 brightness things are drawn at.
pub fn brightness(level: u8) -> f32 {
    level.min(15) as f32 / 15.0
}

/// The levels worked out in this frame, by the cell they are for.
#[derive(Default)]
pub struct LightCache {
    map: BTreeMap<(i32, i32, i32), u8>,
    fresh: u32,
}

impl LightCache {
    /// The level for a thing at `pos` seen from the camera at `eye`: from the
    /// cache, else worked out by `compute` (while the frame's allowance
    /// lasts; after that, and for things far off, 0).
    pub fn level(&mut self, eye: [f64; 3], pos: [f64; 3], compute: impl FnOnce() -> u8) -> u8 {
        let (dx, dz) = (pos[0] - eye[0], pos[2] - eye[2]);
        if !(dx * dx + dz * dz <= FAR * FAR) || !pos[1].is_finite() {
            return 0;
        }
        let key = (crate::math::floord(pos[0]), crate::math::floord(pos[1]), crate::math::floord(pos[2]));
        if let Some(l) = self.map.get(&key) {
            return *l;
        }
        if self.fresh >= MAX_FRESH || self.map.len() >= MAX_ENTRIES {
            return 0;
        }
        self.fresh += 1;
        let l = compute();
        self.map.insert(key, l);
        l
    }

    /// How many were worked out so far.
    pub fn worked(&self) -> u32 {
        self.fresh
    }
}

/// [`level_at`] on the world's loaded chunks.
pub fn level_in_world(world: &crate::world::World, pos: [f64; 3]) -> u8 {
    if !pos.iter().all(|v| v.is_finite() && *v > -1.0e9 && *v < 1.0e9) {
        return 0;
    }
    let r = crate::block::reg();
    let (x0, x1) = (crate::math::floord(pos[0]) - R, crate::math::floord(pos[0]) + R);
    let (z0, z1) = (crate::math::floord(pos[2]) - R, crate::math::floord(pos[2]) + R);
    let mut chunks: Vec<Slice> = Vec::with_capacity(4);
    for cx in (x0 >> 4)..=(x1 >> 4) {
        for cz in (z0 >> 4)..=(z1 >> 4) {
            if let Some(d) = world.chunks.get(&(cx, cz)).and_then(|c| c.data.as_ref()) {
                chunks.push(Slice { cx, cz, blocks: &d.blocks });
            }
        }
    }
    level_at(&chunks, &r.emit, &r.opaque, pos)
}

/// What a thing's colour is multiplied by: its own light `base` (0 to 1, from
/// the sky) or the block light, whichever is more.
pub fn lit(base: f32, level: u8) -> f32 {
    let b = brightness(level);
    if b > base {
        b
    } else {
        base
    }
}

/// Dim the colour of drawn quads by `f` (0 to 1).
pub fn scale_quads(quads: &mut [crate::client::hand::Quad], f: f32) {
    let f = if f > 1.0 {
        1.0
    } else if f > 0.0 {
        f
    } else {
        0.0
    };
    for q in quads {
        for i in 0..3 {
            q.col[i] = (q.col[i] as f32 * f) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAVA: Block = 1;
    const STONE: Block = 2;
    const LAMP: Block = 3;
    const EMIT: [u8; 4] = [0, 12, 0, 15];
    const OPAQUE: [u8; 4] = [0, 0, 1, 1];

    struct Map {
        data: Vec<(i32, i32, Vec<Block>)>,
    }

    impl Map {
        fn new() -> Map {
            let mut data = Vec::new();
            for cx in -1..=1 {
                for cz in -1..=1 {
                    data.push((cx, cz, alloc::vec![0 as Block; (CW * CW * CH) as usize]));
                }
            }
            Map { data }
        }
        fn set(&mut self, x: i32, y: i32, z: i32, b: Block) {
            let d = self.data.iter_mut().find(|d| d.0 == x >> 4 && d.1 == z >> 4).unwrap();
            d.2[((((x & 15) * CW) + (z & 15)) * CH + y) as usize] = b;
        }
        fn level(&self, pos: [f64; 3]) -> u8 {
            let s: Vec<Slice> = self.data.iter().map(|d| Slice { cx: d.0, cz: d.1, blocks: &d.2 }).collect();
            level_at(&s, &EMIT, &OPAQUE, pos)
        }
    }

    #[test]
    fn nothing_shining_is_dark() {
        let m = Map::new();
        assert_eq!(m.level([3.5, 60.0, 3.5]), 0);
    }

    #[test]
    fn lava_beside_a_thing_lights_it() {
        let mut m = Map::new();
        m.set(4, 60, 3, LAVA);
        // The thing in the cell beside it: 12 less one block.
        assert_eq!(m.level([3.5, 60.0, 3.5]), 11);
        // Standing in the lava's own cell: the whole of it.
        assert_eq!(m.level([4.5, 60.0, 3.5]), 12);
    }

    #[test]
    fn the_light_falls_off_by_a_level_a_block_and_ends_at_six() {
        let mut m = Map::new();
        m.set(0, 60, 0, LAVA);
        let mut last = 16;
        for d in 0..=6 {
            let l = m.level([0.5 + d as f64, 60.0, 0.5]);
            assert_eq!(l as i32, 12 - d, "{d} blocks away");
            assert!((l as i32) < last);
            last = l as i32;
        }
        assert_eq!(m.level([7.5, 60.0, 0.5]), 0);
        // Up and down too.
        assert_eq!(m.level([0.5, 63.0, 0.5]), 9);
        assert_eq!(m.level([0.5, 50.0, 0.5]), 0);
    }

    #[test]
    fn the_strongest_wins_not_the_sum() {
        let mut m = Map::new();
        m.set(0, 60, 0, LAVA);
        m.set(2, 60, 0, LAVA);
        m.set(1, 61, 0, LAVA);
        assert_eq!(m.level([1.5, 60.0, 0.5]), 11);
    }

    #[test]
    fn a_wall_hides_the_light() {
        let mut m = Map::new();
        m.set(0, 60, 0, LAVA);
        for y in 55..66 {
            for z in -3..=3 {
                m.set(2, y, z, STONE);
            }
        }
        assert_eq!(m.level([4.5, 60.0, 0.5]), 0);
        // On the lava's side of it, lit.
        assert!(m.level([1.5, 60.0, 0.5]) >= 10);
    }

    #[test]
    fn a_lamp_in_a_block_of_its_own_lights_beside_it() {
        let mut m = Map::new();
        m.set(0, 60, 0, LAMP);
        assert_eq!(m.level([1.5, 60.0, 0.5]), 14);
    }

    #[test]
    fn light_comes_across_the_edge_of_a_chunk() {
        let mut m = Map::new();
        m.set(16, 60, 0, LAVA);
        assert_eq!(m.level([15.5, 60.0, 0.5]), 11);
        m.set(-1, 60, -1, LAVA);
        assert_eq!(m.level([0.5, 60.0, 0.5]), 12 - 2);
    }

    #[test]
    fn a_flooded_world_stays_bounded() {
        let mut m = Map::new();
        for x in -8..8 {
            for z in -8..8 {
                for y in 50..70 {
                    m.set(x, y, z, LAVA);
                }
            }
        }
        assert_eq!(m.level([0.5, 60.0, 0.5]), 12);
        // The top and bottom of the world, outside it, and rubbish positions.
        assert!(m.level([0.5, 0.0, 0.5]) <= 15);
        assert!(m.level([0.5, CH as f64 + 5.0, 0.5]) <= 15);
        assert_eq!(m.level([f64::NAN, 60.0, 0.0]), 0);
        assert_eq!(m.level([1.0e12, 60.0, 0.0]), 0);
        // Unloaded ground gives nothing.
        assert_eq!(level_at(&[], &EMIT, &OPAQUE, [0.0, 60.0, 0.0]), 0);
    }

    #[test]
    fn the_cache_works_each_place_out_once_and_stops_at_the_allowance() {
        let mut c = LightCache::default();
        let eye = [0.0, 60.0, 0.0];
        let mut calls = 0;
        for _ in 0..5 {
            assert_eq!(
                c.level(eye, [3.2, 60.1, 3.7], || {
                    calls += 1;
                    7
                }),
                7
            );
        }
        assert_eq!(calls, 1);
        // Many different cells (400 of them): only so many are worked out, the rest are dark.
        for i in 0..400 {
            let l = c.level(eye, [(i % 50) as f64 + 0.5, 60.0, 10.5 + (i / 50) as f64], || 5);
            assert!(l == 5 || l == 0);
        }
        assert_eq!(c.worked(), MAX_FRESH);
    }

    #[test]
    fn things_far_from_the_camera_get_no_block_light() {
        let mut c = LightCache::default();
        let mut called = false;
        assert_eq!(
            c.level([0.0; 3], [70.0, 60.0, 0.0], || {
                called = true;
                9
            }),
            0
        );
        assert!(!called);
        assert_eq!(c.level([0.0; 3], [f64::NAN, 0.0, 0.0], || 9), 0);
        assert_eq!(c.level([0.0; 3], [60.0, 60.0, 0.0], || 9), 9);
    }

    #[test]
    fn lit_is_the_brighter_of_the_two() {
        assert_eq!(lit(0.4, 0), 0.4);
        assert_eq!(lit(0.4, 15), 1.0);
        assert_eq!(lit(0.9, 6), 0.9);
        assert!(lit(0.4, 12) > 0.79 && lit(0.4, 12) < 0.81);
    }

    #[test]
    fn quads_are_dimmed_not_brightened() {
        let mut q = alloc::vec![crate::client::hand::Quad { p: [[0.0; 3]; 4], uv: [[0.0; 2]; 4], col: [200, 100, 50, 255] }];
        scale_quads(&mut q, 0.5);
        assert_eq!(q[0].col, [100, 50, 25, 255]);
        scale_quads(&mut q, 7.0);
        assert_eq!(q[0].col, [100, 50, 25, 255]);
        scale_quads(&mut q, -1.0);
        assert_eq!(q[0].col, [0, 0, 0, 255]);
    }
}
