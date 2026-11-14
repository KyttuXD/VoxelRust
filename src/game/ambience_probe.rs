//! Looks at the blocks around the player to tell the sound system what it
//! should hear: how much roof there is overhead (rain and wind are muffled
//! under it), how deep in rock the player is (cave air and drips), whether
//! the head is under water, and how much open water, lava and fire is
//! close by. A few hundred block lookups; the session runs it every
//! 0.4 seconds, not every frame. See docs/SOUND.md.

use crate::block::{fluid_of, is_water, reg, Kind, AIR};
use crate::math::{clampf, floord, smoothstep};
use crate::wav::ambience::Env;
use crate::weather::Sky;
use crate::world::World;

/// How far above the head a roof is looked for.
const ROOF_REACH: i32 = 48;

/// How much of a block there is overhead: a solid one is a roof, leaves
/// let part of the sound through, water above is muffling too.
fn cover_of(b: crate::block::Block) -> f32 {
    match reg().kind.get(b as usize) {
        Some(Kind::Cube) | Some(Kind::Boxes) | Some(Kind::Translucent) => 1.0,
        Some(Kind::Cutout) => 0.35,
        Some(Kind::Liquid) => 0.6,
        _ => 0.0,
    }
}

/// (shelter, depth) over one column of blocks: the share of the sky it hides
/// (0 to 1) and how thick the rock above is (0 to 1; a house roof is thin,
/// a cave's ceiling is not).
fn column_cover(world: &World, x: i32, z: i32, from_y: i32) -> (f32, f32) {
    let top = (from_y + ROOF_REACH).min(crate::chunk::CH - 1);
    let (mut cover, mut solid) = (0.0f32, 0.0f32);
    for y in (from_y + 1)..=top {
        let b = world.get(x, y, z);
        if b == AIR {
            continue;
        }
        let c = cover_of(b);
        cover += c;
        if c >= 1.0 {
            solid += 1.0;
        }
    }
    (clampf(cover, 0.0, 1.0), smoothstep(3.0, 10.0, solid))
}

/// What the player at `pos` (the feet) can hear around them, with the
/// weather `sky` there.
pub fn sample(world: &World, pos: [f64; 3], sky: Sky) -> Env {
    let (px, py, pz) = (floord(pos[0]), floord(pos[1]), floord(pos[2]));
    let head = floord(pos[1] + 1.6);
    let underwater = is_water(world.get(px, head, pz));

    // Roof: the middle and four columns two blocks out.
    let (mut shelter, mut depth) = (0.0, 0.0);
    for (dx, dz) in [(0, 0), (2, 0), (-2, 0), (0, 2), (0, -2)] {
        let (s, d) = column_cover(world, px + dx, pz + dz, head);
        shelter += s;
        depth += d;
    }
    let (shelter, depth) = (shelter / 5.0, depth / 5.0);

    // Wind: open ground, higher up is windier.
    let altitude = clampf((py - crate::worldgen::SEA) as f32 / 70.0, 0.0, 1.0);
    let wind = (1.0 - shelter) * (0.2 + 0.5 * altitude);

    // Water within about eight blocks, at the player's level and just below
    // (lava is looked for farther, by `lava_probe.rs`).
    let mut water = 0u32;
    for dx in [-8, -4, 0, 4, 8] {
        for dz in [-8, -4, 0, 4, 8] {
            for dy in [-2, -1, 0] {
                let b = world.get(px + dx, py + dy, pz + dz);
                if b == AIR {
                    continue;
                }
                match fluid_of(b) {
                    Some((false, _)) if dy >= -1 => water += 1,
                    _ => {}
                }
            }
        }
    }

    // Fire (a campfire, or any block marked `tag fire`) within four blocks.
    let fire_id = reg().block("fire");
    let mut fire = 0.0f32;
    for dx in -4..=4i32 {
        for dz in -4..=4i32 {
            for dy in -1..=1i32 {
                let b = world.get(px + dx, py + dy, pz + dz);
                if b == AIR {
                    continue;
                }
                let burning = Some(b) == fire_id || (reg().kind.get(b as usize) != Some(&Kind::Cube) && crate::block::def(b).x.tag("fire").is_some());
                if burning {
                    let d = crate::math::sqrtf((dx * dx + dz * dz) as f32);
                    fire += clampf(1.0 - d / 6.0, 0.0, 1.0);
                }
            }
        }
    }

    Env {
        rain: sky.rain,
        snow: sky.snow,
        storm: sky.storm,
        shelter,
        cave: depth,
        underwater,
        wind,
        water: clampf(water as f32 / 20.0, 0.0, 1.0),
        lava: crate::game::lava_probe::lava_near(&|x, y, z| world.get(x, y, z), [px, py, pz]),
        fire: clampf(fire / 1.5, 0.0, 1.0),
    }
}
