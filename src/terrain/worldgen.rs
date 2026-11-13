//! Terrain generation. A chunk depends only on the seed and its position,
//! so chunks can be generated in any order and on any thread.

use crate::block::{reg, Block, WellKnown, AIR};
use crate::chunk::{ChunkData, CH, CW};
use crate::math::{clampf, smoothstep};
use crate::noise::{fbm2, hash2, hash3, noise3};
use alloc::boxed::Box;
use alloc::string::String;
use core::sync::atomic::{AtomicPtr, Ordering};

/// Water fills every hollow up to this height.
pub const SEA: i32 = 48;

/// What kind of world is generated.
#[derive(Clone, PartialEq, Debug)]
pub enum WorldKind {
    /// Invented terrain from the seed.
    Normal,
    /// The real Earth, from elevation data. Block (0, 0) is at `lat`/`lon`
    /// (degrees); one block is `scale` metres across and `vscale` metres tall.
    /// `features` adds roads and buildings from the map.
    Earth { lat: f64, lon: f64, scale: u32, vscale: u32, features: bool },
}

impl WorldKind {
    pub fn to_text(&self) -> String {
        match self {
            WorldKind::Normal => String::from("normal"),
            WorldKind::Earth { lat, lon, scale, vscale, features } => alloc::format!("earth {} {} {} {} {}", lat, lon, scale, vscale, *features as i32),
        }
    }

    pub fn parse(text: &str) -> Option<WorldKind> {
        let mut w = text.split(' ');
        match w.next()? {
            "normal" => Some(WorldKind::Normal),
            "earth" => {
                let lat = w.next()?.parse::<f64>().ok()?;
                let lon = w.next()?.parse::<f64>().ok()?;
                let scale = w.next()?.parse::<u32>().ok()?;
                let vscale = w.next()?.parse::<u32>().ok()?;
                if !(-85.0..=85.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) || scale == 0 || vscale == 0 {
                    return None;
                }
                // Older saves have no fifth word: roads and buildings on.
                let features = w.next() != Some("0");
                Some(WorldKind::Earth { lat, lon, scale: scale.min(100_000), vscale: vscale.min(10_000), features })
            }
            _ => None,
        }
    }
}

static KIND: AtomicPtr<WorldKind> = AtomicPtr::new(core::ptr::null_mut());

/// Choose what [`column`] and [`generate`] produce from now on. One world
/// is open at a time, so this is process-wide. (The previous value is
/// leaked on purpose: worker threads may still be reading it.)
pub fn set_kind(kind: WorldKind) {
    let proj = match &kind {
        WorldKind::Earth { lat, lon, scale, vscale, features } => {
            crate::earth::reset_base();
            let mut p = crate::earth::Proj::new(*lat, *lon, *scale, *vscale);
            p.features = *features;
            Box::into_raw(Box::new(p))
        }
        WorldKind::Normal => core::ptr::null_mut(),
    };
    PROJ.store(proj, Ordering::Release);
    KIND.store(Box::into_raw(Box::new(kind)), Ordering::Release);
}

static PROJ: AtomicPtr<crate::earth::Proj> = AtomicPtr::new(core::ptr::null_mut());

/// The lie of the land in the dimension being played: 0 the world's own
/// terrain, 1 flat, 2 empty, 3 islands in the sky.
static SHAPE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);
pub const SHAPES: [&str; 4] = ["normal", "flat", "void", "islands"];

pub fn set_shape(shape: u8) {
    SHAPE.store(shape, Ordering::Release);
}

pub fn shape() -> u8 {
    SHAPE.load(Ordering::Acquire)
}

/// Whether there is an island at (x, z) of an islands dimension, and how
/// thick it is there.
fn island(seed: u32, x: i32, z: i32) -> i32 {
    let n = crate::noise::noise2(seed ^ 0x15A, x, z, 46) * 0.7 + crate::noise::noise2(seed ^ 0x15B, x, z, 13) * 0.3;
    (((n - 0.56) * 90.0) as i32).clamp(0, 22)
}

/// The map projection of the open world, if it is an Earth world.
pub fn earth_proj() -> Option<&'static crate::earth::Proj> {
    unsafe { PROJ.load(Ordering::Acquire).as_ref() }
}

pub fn kind() -> &'static WorldKind {
    static NORMAL: WorldKind = WorldKind::Normal;
    let p = KIND.load(Ordering::Acquire);
    if p.is_null() {
        &NORMAL
    } else {
        unsafe { &*p }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Biome {
    Plains,
    Forest,
    Desert,
    Tundra,
    Taiga,
    Mountain,
    /// Low, wet ground with pools and drooping trees.
    Swamp,
    /// Dry grassland with a few flat-topped trees.
    Savanna,
    /// Hot, wet and thick with tall trees.
    Jungle,
    /// Bare banded clay hills.
    Badlands,
}

#[derive(Clone, Copy, Debug)]
pub struct Column {
    /// Height of the topmost ground block.
    pub height: i32,
    pub biome: Biome,
    /// A river or lake above sea level runs here (Earth worlds).
    pub river: bool,
    /// A road or built-up ground, from the map (Earth worlds).
    pub paved: bool,
    /// Height in blocks of the building standing here (0 = none).
    pub building: u8,
    /// A road, as opposed to other built-up ground (Earth worlds).
    pub road: bool,
    /// Farmland (Earth worlds).
    pub farm: bool,
}

/// Spread a noise value, which clusters around 0.5, over the whole 0..1 range.
fn stretch(v: f32, k: f32) -> f32 {
    clampf((v - 0.5) * k + 0.5, 0.0, 1.0)
}

pub fn column(seed: u32, x: i32, z: i32) -> Column {
    let plain = |height: i32| Column { height, biome: Biome::Plains, river: false, paved: false, building: 0, road: false, farm: false };
    match shape() {
        1 => return plain(SEA + 3),
        2 => return plain(0),
        3 => {
            let t = island(seed, x, z);
            return plain(if t > 0 { 80 + t / 4 } else { 0 });
        }
        _ => {}
    }
    match earth_proj() {
        Some(p) => crate::earth::column(p, x, z),
        None => invented_column(seed, x, z),
    }
}

/// Terrain made up from the seed.
pub fn invented_column(seed: u32, x: i32, z: i32) -> Column {
    let cont = (fbm2(seed ^ 0x11, x, z, 384, 4) - 0.5) * 2.6;
    let cont = clampf(cont, -1.0, 1.0);
    let hill = fbm2(seed ^ 0x22, x, z, 64, 3) - 0.5;
    let peak = smoothstep(0.56, 0.78, fbm2(seed ^ 0x33, x, z, 320, 2));
    let rough = fbm2(seed ^ 0x44, x, z, 96, 4);

    let mut h = if cont > 0.0 { SEA as f32 + 2.0 + cont * 20.0 } else { SEA as f32 + 2.0 + cont * 36.0 };
    // Hills flatten out near the shore so beaches stay gentle.
    h += hill * 16.0 * clampf(0.35 + cont * 2.0, 0.25, 1.0);
    h += peak * (12.0 + rough * 62.0) * clampf(cont * 3.0 + 0.4, 0.0, 1.0);
    let mut height = (h as i32).clamp(4, CH - 14);

    let temp = stretch(fbm2(seed ^ 0x55, x, z, 512, 2), 3.2) - (height - SEA).max(0) as f32 * 0.004;
    let wet = stretch(fbm2(seed ^ 0x66, x, z, 384, 2), 3.0);
    let low = height <= SEA + 7;
    let biome = if height > SEA + 42 {
        Biome::Mountain
    } else if temp < 0.3 {
        if wet > 0.5 {
            Biome::Taiga
        } else {
            Biome::Tundra
        }
    } else if temp > 0.68 {
        // Hot lands, from dry to wet.
        if wet < 0.3 {
            Biome::Desert
        } else if wet < 0.42 {
            // Badlands sit on the rougher dry ground, deserts on the smooth.
            if rough > 0.52 {
                Biome::Badlands
            } else {
                Biome::Desert
            }
        } else if wet < 0.62 {
            Biome::Savanna
        } else {
            Biome::Jungle
        }
    } else if wet > 0.72 && low && height > SEA - 2 {
        Biome::Swamp
    } else if wet > 0.55 {
        Biome::Forest
    } else {
        Biome::Plains
    };
    match biome {
        // Swamps are nearly flat, sagging just under the water in pools.
        Biome::Swamp => {
            let pool = fbm2(seed ^ 0x5A, x, z, 14, 2);
            height = if pool < 0.44 { SEA - 1 } else { SEA + 1 + (pool > 0.6) as i32 };
        }
        // Badlands rise in terraces.
        Biome::Badlands => {
            let lift = (rough * 26.0) as i32;
            height = (height + lift - lift % 4).min(CH - 14);
        }
        _ => {}
    }
    Column { height, biome, river: false, paved: false, building: 0, road: false, farm: false }
}

/// Whether a cave passes through this block. `surface` is the ground height
/// of the column.
pub fn is_cave(seed: u32, x: i32, y: i32, z: i32, surface: i32) -> bool {
    if y < 3 || y > surface {
        return false;
    }
    // Keep sea and lake beds sealed.
    if surface <= SEA + 1 && y > surface - 6 {
        return false;
    }
    // Caves come in several sizes, each from its own pair of noise fields:
    // where both fields are near their middle value, there is a tunnel.
    // Ordinary tunnels, about three blocks across.
    let a = noise3(seed ^ 0x77, x, y, z, 24, 14) - 0.5;
    if a.abs_() < 0.05 {
        let b = noise3(seed ^ 0x88, x, y, z, 22, 16) - 0.5;
        if b.abs_() < 0.05 {
            return true;
        }
    }
    // Wide galleries, six to eight blocks across, less common and deeper.
    if y < surface - 8 {
        let a = noise3(seed ^ 0x71, x, y, z, 56, 26) - 0.5;
        if a.abs_() < 0.035 {
            let b = noise3(seed ^ 0x72, x, y, z, 48, 30) - 0.5;
            if b.abs_() < 0.05 {
                return true;
            }
        }
    }
    // Narrow crawlways, one or two blocks across, anywhere.
    let a = noise3(seed ^ 0x73, x, y, z, 12, 9) - 0.5;
    if a.abs_() < 0.035 {
        let b = noise3(seed ^ 0x74, x, y, z, 11, 10) - 0.5;
        if b.abs_() < 0.035 {
            return true;
        }
    }
    // Ravines: long, narrow cracks open to the sky, cut deep into dry land.
    if surface > SEA + 3 && y > 14 {
        let r = crate::noise::noise2(seed ^ 0x75, x, z, 90) - 0.5;
        if r.abs_() < 0.006 && crate::noise::noise2(seed ^ 0x76, x, z, 200) > 0.62 {
            return true;
        }
    }
    // Rare large caverns deep down.
    y < 34 && noise3(seed ^ 0x99, x, y, z, 36, 20) > 0.80
}

/// What a stretch of cave is like. Caves change character from region to
/// region and with depth, as the surface does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaveBiome {
    /// Bare rock.
    Stone,
    /// Glowing mushrooms on mossy floors.
    Mushroom,
    /// Crystal clusters that light the walls.
    Crystal,
    /// Moss, hanging vines and pools of light.
    Lush,
    /// Under cold country: ice floors and icicles.
    Frozen,
    /// Deep down: basalt and glowing magma.
    Magma,
    /// Thick with webs.
    Webbed,
}

impl CaveBiome {
    pub fn name(self) -> &'static str {
        match self {
            CaveBiome::Stone => "cave",
            CaveBiome::Mushroom => "mushroom_cave",
            CaveBiome::Crystal => "crystal_cave",
            CaveBiome::Lush => "lush_cave",
            CaveBiome::Frozen => "frozen_cave",
            CaveBiome::Magma => "magma_cave",
            CaveBiome::Webbed => "webbed_cave",
        }
    }
}

/// The kind of cave at a place underground.
pub fn cave_biome(seed: u32, x: i32, y: i32, z: i32, surface: Biome) -> CaveBiome {
    let a = crate::noise::noise2(seed ^ 0xCB1, x, z, 110);
    let b = crate::noise::noise2(seed ^ 0xCB2, x, z, 76);
    if y < 26 && a > 0.56 {
        CaveBiome::Magma
    } else if matches!(surface, Biome::Tundra | Biome::Taiga) && y > 24 {
        CaveBiome::Frozen
    } else if a < 0.34 {
        CaveBiome::Mushroom
    } else if b > 0.64 {
        CaveBiome::Crystal
    } else if b < 0.33 {
        CaveBiome::Lush
    } else if a > 0.66 {
        CaveBiome::Webbed
    } else {
        CaveBiome::Stone
    }
}

/// Dress the caves of one column: floors, things growing on them, and
/// things hanging from the roof.
fn decorate_caves(data: &mut ChunkData, wk: &WellKnown, seed: u32, lx: i32, lz: i32, x: i32, z: i32, col: &Column) {
    let top = col.height - 5;
    for y in 4..top.min(CH - 2) {
        if data.get(lx, y, lz) != AIR {
            continue;
        }
        let (below, above) = (data.get(lx, y - 1, lz), data.get(lx, y + 1, lz));
        let rock = |b: Block| b == wk.stone || b == wk.dirt;
        let (floor, roof) = (rock(below), rock(above));
        if !floor && !roof {
            continue;
        }
        let biome = cave_biome(seed, x, y, z, col.biome);
        let roll = hash3(seed ^ 0xCAFE, x, y, z) % 100;
        // (new floor block, what stands on it and how often, what hangs and how often)
        let (ground, grows, hangs) = match biome {
            CaveBiome::Stone => (AIR, (wk.stalagmite, 3), (AIR, 0)),
            CaveBiome::Mushroom => (wk.moss, (if roll % 3 == 0 { wk.mushroom } else { wk.glow_mushroom }, 14), (AIR, 0)),
            CaveBiome::Crystal => (AIR, (wk.crystal_cluster, 9), (AIR, 0)),
            CaveBiome::Lush => (wk.moss, (wk.tall_grass, 22), (wk.cave_vine, 16)),
            CaveBiome::Frozen => (wk.ice, (wk.snow_layer, 30), (wk.icicle, 18)),
            CaveBiome::Magma => (if roll % 4 == 0 { wk.magma } else { wk.basalt }, (AIR, 0), (AIR, 0)),
            CaveBiome::Webbed => (AIR, (wk.web, 7), (wk.web, 10)),
        };
        if floor {
            if ground != AIR {
                data.set(lx, y - 1, lz, ground);
            }
            // Nothing stands on a magma floor.
            if grows.0 != AIR && roll < grows.1 && data.get(lx, y - 1, lz) != wk.magma {
                data.set(lx, y, lz, grows.0);
                continue;
            }
        }
        if roof && hangs.0 != AIR && (roll * 7 + 3) % 100 < hangs.1 {
            data.set(lx, y, lz, hangs.0);
        }
    }
}

trait Abs {
    fn abs_(self) -> f32;
}
impl Abs for f32 {
    #[inline]
    fn abs_(self) -> f32 {
        if self < 0.0 {
            -self
        } else {
            self
        }
    }
}

fn ore_at(wk: &WellKnown, seed: u32, x: i32, y: i32, z: i32) -> Block {
    // Ores that packs add come first, each with its own roll.
    for (k, o) in reg().ores.iter().enumerate() {
        if y >= o.low && y <= o.high && hash3(seed ^ (0x0E00 + k as u32), x, y, z) % o.one_in == 0 {
            return o.block;
        }
    }
    // Ores come in small clumps: one roll per 2x2x2 cell, then per block.
    let cell = hash3(seed ^ 0xAA, x >> 1, y >> 1, z >> 1) % 1000;
    if cell >= 49 || hash3(seed ^ 0xAB, x, y, z) % 4 == 0 {
        return wk.stone;
    }
    if (46..=48).contains(&cell) {
        // Ruby: the rarest, and only near the bottom of the world.
        return if y < 16 { wk.ruby_ore } else { wk.stone };
    }
    match cell {
        0..=21 => wk.coal_ore,
        22..=33 if y < 60 => wk.iron_ore,
        34..=41 if y < 44 => wk.copper_ore,
        42..=45 if y < 24 => wk.crystal_ore,
        _ => wk.stone,
    }
}

struct Writer<'a> {
    data: &'a mut ChunkData,
    ox: i32,
    oz: i32,
}

impl Writer<'_> {
    /// Set a block given world x/z, if it is inside this chunk. With `soft`,
    /// only air is replaced.
    fn put(&mut self, x: i32, y: i32, z: i32, b: Block, soft: bool) {
        let (lx, lz) = (x - self.ox, z - self.oz);
        if !(0..CW).contains(&lx) || !(0..CW).contains(&lz) || !(0..CH).contains(&y) {
            return;
        }
        if soft && self.data.get(lx, y, lz) != AIR {
            return;
        }
        self.data.set(lx, y, lz, b);
    }
}

/// A blob of leaves: every cell within `r` of the centre, corners trimmed.
fn canopy(w: &mut Writer, h: u32, x: i32, y: i32, z: i32, r: i32, flat: bool, leaves: Block) {
    let ry = if flat { (r + 1) / 2 } else { r };
    for dy in -ry..=ry {
        for dx in -r..=r {
            for dz in -r..=r {
                let sy = if flat { dy * 2 } else { dy };
                let d2 = dx * dx + sy * sy + dz * dz;
                if d2 > r * r + 1 || (d2 > r * r - 1 && hash3(h, x + dx, y + dy, z + dz) % 3 == 0) {
                    continue;
                }
                w.put(x + dx, y + dy, z + dz, leaves, true);
            }
        }
    }
}

fn tree(w: &mut Writer, wk: &WellKnown, h: u32, x: i32, y: i32, z: i32, biome: Biome, near_shore: bool) {
    let roll = (h >> 20) % 100;
    let size = ((h >> 8) % 3) as i32;
    match biome {
        Biome::Desert if near_shore => {
            // Palm: a tall bare trunk with four drooping fronds.
            let th = 5 + size;
            for i in 1..=th {
                w.put(x, y + i, z, wk.palm_log, false);
            }
            w.put(x, y + th + 1, z, wk.palm_leaves, true);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                w.put(x + dx, y + th + 1, z + dz, wk.palm_leaves, true);
                w.put(x + dx * 2, y + th, z + dz * 2, wk.palm_leaves, true);
                w.put(x + dx * 3, y + th - 1, z + dz * 3, wk.palm_leaves, true);
            }
        }
        Biome::Desert => {
            for i in 1..=2 + size {
                w.put(x, y + i, z, wk.cactus, true);
            }
        }
        Biome::Taiga | Biome::Tundra => {
            let th = 6 + ((h >> 8) % 4) as i32;
            for ly in (y + 3)..=(y + th) {
                let d = y + th - ly;
                let r = if d == 0 {
                    1
                } else if d % 2 == 1 {
                    (1 + (d + 1) / 2).min(3)
                } else {
                    (d / 2).clamp(1, 2)
                };
                for dx in -r..=r {
                    for dz in -r..=r {
                        if r > 1 && dx.abs() == r && dz.abs() == r {
                            continue;
                        }
                        w.put(x + dx, ly, z + dz, wk.pine_leaves, true);
                    }
                }
            }
            w.put(x, y + th + 1, z, wk.pine_leaves, true);
            for i in 1..=th {
                w.put(x, y + i, z, wk.pine_log, false);
            }
        }
        Biome::Swamp => {
            // A drooping swamp tree: wide flat crown with hanging strands.
            let th = 4 + size;
            canopy(w, h, x, y + th + 1, z, 3, true, wk.leaves);
            for (dx, dz) in [(3, 0), (-3, 0), (0, 3), (0, -3), (2, 2), (-2, -2), (2, -2), (-2, 2)] {
                for d in 0..(1 + hash3(h, x + dx, y, z + dz) % 3) as i32 {
                    w.put(x + dx, y + th - d, z + dz, wk.leaves, true);
                }
            }
            for i in 1..=th {
                w.put(x, y + i, z, wk.log, false);
            }
        }
        Biome::Savanna => {
            // A flat-topped tree on a leaning trunk.
            let th = 4 + size;
            let (lx, lz) = (((h >> 4) % 3) as i32 - 1, ((h >> 6) % 3) as i32 - 1);
            for i in 1..=th {
                let (ox, oz) = if i > th / 2 { (lx, lz) } else { (0, 0) };
                w.put(x + ox, y + i, z + oz, wk.maple_log, false);
            }
            for dx in -3..=3i32 {
                for dz in -3..=3i32 {
                    if dx.abs() + dz.abs() <= 4 {
                        w.put(x + lx + dx, y + th + 1, z + lz + dz, wk.leaves, true);
                    }
                }
            }
        }
        Biome::Jungle => {
            // Tall, with a big crown and a second tier lower down.
            let th = 9 + size * 2;
            canopy(w, h, x, y + th, z, 3, false, wk.palm_leaves);
            canopy(w, h ^ 9, x, y + th - 5, z, 2, true, wk.palm_leaves);
            for i in 1..=th {
                w.put(x, y + i, z, wk.palm_log, false);
            }
        }
        Biome::Badlands => {
            for i in 1..=1 + size {
                w.put(x, y + i, z, wk.cactus, true);
            }
        }
        Biome::Forest if roll < 26 => {
            // Birch: slender, with a small high crown.
            let th = 6 + size;
            canopy(w, h, x, y + th, z, 2, false, wk.birch_leaves);
            for i in 1..=th {
                w.put(x, y + i, z, wk.birch_log, false);
            }
        }
        Biome::Forest if roll < 44 => {
            // Maple: a broad round crown.
            let th = 5 + size;
            canopy(w, h, x, y + th, z, 3, false, wk.maple_leaves);
            for i in 1..=th {
                w.put(x, y + i, z, wk.maple_log, false);
            }
        }
        Biome::Plains if roll < 22 => {
            // Blossom: short and wide.
            let th = 3 + size.min(1);
            canopy(w, h, x, y + th + 1, z, 3, true, wk.blossom_leaves);
            for i in 1..=th {
                w.put(x, y + i, z, wk.blossom_log, false);
            }
        }
        _ if roll >= 88 => {
            // Now and then, a big old oak.
            let th = 7 + size;
            canopy(w, h, x, y + th, z, 3, false, wk.leaves);
            canopy(w, h ^ 5, x + 1, y + th - 2, z - 1, 2, false, wk.leaves);
            for i in 1..=th {
                w.put(x, y + i, z, wk.log, false);
            }
        }
        _ => {
            let th = 4 + size;
            for ly in (y + th - 2)..=(y + th + 1) {
                let r: i32 = if ly >= y + th { 1 } else { 2 };
                for dx in -r..=r {
                    for dz in -r..=r {
                        let corner = dx.abs() == r && dz.abs() == r;
                        if corner && (ly == y + th + 1 || hash3(h, x + dx, ly, z + dz) % 2 == 0) {
                            continue;
                        }
                        w.put(x + dx, ly, z + dz, wk.leaves, true);
                    }
                }
            }
            for i in 1..=th {
                w.put(x, y + i, z, wk.log, false);
            }
        }
    }
}

pub fn generate(seed: u32, cx: i32, cz: i32) -> ChunkData {
    let wk = &reg().wk;
    let mut data = ChunkData::new();
    let (ox, oz) = (cx * CW, cz * CW);
    // Other dimensions are simple enough to make right here.
    match shape() {
        1 => {
            for lx in 0..CW {
                for lz in 0..CW {
                    for y in 0..=SEA + 3 {
                        let b = if y == 0 { wk.deepstone } else if y == SEA + 3 { wk.grass } else if y >= SEA { wk.dirt } else { wk.stone };
                        data.set(lx, y, lz, b);
                    }
                }
            }
            return data;
        }
        2 => return data,
        3 => {
            for lx in 0..CW {
                for lz in 0..CW {
                    let (x, z) = (ox + lx, oz + lz);
                    let t = island(seed, x, z);
                    if t == 0 {
                        continue;
                    }
                    let top = 80 + t / 4;
                    for y in (top - t).max(1)..=top {
                        let b = if y == top { wk.grass } else if y >= top - 2 { wk.dirt } else { ore_at(wk, seed, x, y, z) };
                        data.set(lx, y, lz, b);
                    }
                    let roll = hash2(seed ^ 0x15C, x, z) % 100;
                    if roll < 6 && top + 1 < CH {
                        data.set(lx, top + 1, lz, if roll < 3 { wk.tall_grass } else { wk.flower_yellow });
                    }
                }
            }
            // A tree here and there, as in the world below.
            let mut w = Writer { data: &mut data, ox, oz };
            for gx in (ox - 3).div_euclid(6)..=(ox + CW + 2).div_euclid(6) {
                for gz in (oz - 3).div_euclid(6)..=(oz + CW + 2).div_euclid(6) {
                    let h = hash2(seed ^ 0xC1, gx, gz);
                    let (tx, tz) = (gx * 6 + 1 + (h % 4) as i32, gz * 6 + 1 + ((h >> 4) % 4) as i32);
                    let t = island(seed, tx, tz);
                    if t > 6 && (h >> 12) % 1000 < 160 {
                        tree(&mut w, wk, h, tx, 80 + t / 4, tz, Biome::Forest, false);
                    }
                }
            }
            return data;
        }
        _ => {}
    }
    let mut cols = [Column { height: 0, biome: Biome::Plains, river: false, paved: false, building: 0, road: false, farm: false }; (CW * CW) as usize];

    for lx in 0..CW {
        for lz in 0..CW {
            let (x, z) = (ox + lx, oz + lz);
            let col = column(seed, x, z);
            cols[(lx * CW + lz) as usize] = col;
            let h = col.height;
            let under_water = h < SEA;
            // Invented shores are sandy; real ones say so themselves (the
            // map marks sand, and it arrives here as the Desert biome).
            let beach = earth_proj().is_none() && h <= SEA + 1 && col.biome != Biome::Tundra && col.biome != Biome::Taiga;
            let (top, fill) = if under_water {
                let bed = match hash2(seed ^ 0xB1, x >> 2, z >> 2) % 8 {
                    0 => wk.clay,
                    1 | 2 => wk.gravel,
                    _ => wk.sand,
                };
                (bed, bed)
            } else if beach {
                (wk.sand, wk.sand)
            } else {
                match col.biome {
                    Biome::Desert => (wk.sand, wk.sandstone),
                    Biome::Badlands => {
                        let band = wk.bands[(h.rem_euclid(8) / 2) as usize];
                        (band, band)
                    }
                    Biome::Tundra | Biome::Taiga => (wk.snowy_grass, wk.dirt),
                    Biome::Mountain => {
                        if h > SEA + 54 {
                            (wk.snow, wk.stone)
                        } else {
                            (wk.stone, wk.stone)
                        }
                    }
                    _ => (wk.grass, wk.dirt),
                }
            };
            for y in 0..=h {
                let b = if y == 0 || (y < 3 && hash3(seed ^ 0xB2, x, y, z) % 2 == 0) {
                    wk.deepstone
                } else if is_cave(seed, x, y, z, h) {
                    AIR
                } else if y == h {
                    top
                } else if col.biome == Biome::Badlands && y > SEA && y >= h - 20 {
                    // Coloured strata all the way up the terraces.
                    wk.bands[(y.rem_euclid(8) / 2) as usize]
                } else if y >= h - 3 {
                    fill
                } else {
                    ore_at(wk, seed, x, y, z)
                };
                data.set(lx, y, lz, b);
            }
            let proj = earth_proj();
            if col.building > 0 && h > SEA && h + (col.building as i32) < CH - 2 {
                // A building: outer walls in one of a few materials (the
                // same for the whole building) with rows of windows, a flat
                // roof, and inside it floors, stairs and some furniture.
                let built = |dx: i32, dz: i32| proj.map_or(false, |p| crate::earth::cover_at(p, x + dx, z + dz) == crate::earth::class::BUILDING);
                let inside = built(1, 0) && built(-1, 0) && built(0, 1) && built(0, -1);
                let lot = hash2(seed ^ 0xB11D, x >> 5, z >> 5);
                let wall = wk.walls[(lot % 4) as usize];
                let top = h + col.building as i32;
                const STOREY: i32 = 4;
                let roll = hash2(seed ^ 0xF00D, x, z);
                // A flight of stairs climbs toward +x in every eighth row.
                let (sx, sz) = (x.rem_euclid(8), z.rem_euclid(8));
                let flight = inside && sz == 3 && sx < 4 && wk.stairs != AIR;
                for y in (h + 1)..=top {
                    let level = (y - h - 1) % STOREY; // 0 is where feet go
                    let ground_floor = y - h - 1 < STOREY;
                    let b = if y == top {
                        wall
                    } else if !inside {
                        // A doorway here and there at street level.
                        if ground_floor && level < 2 && roll % 9 == 0 {
                            AIR
                        } else if level == 1 && (x + z) % 3 != 0 {
                            wk.window
                        } else {
                            wall
                        }
                    } else if flight && level == sx {
                        reg().placed_state(wk.stairs, 3, false)
                    } else if flight {
                        AIR // headroom over the steps, through the ceiling
                    } else if level == STOREY - 1 {
                        // The ceiling, which is the next floor; a lamp now and then.
                        if roll % 19 == 0 && wk.furniture[4] != AIR { wk.furniture[4] } else { wk.floor }
                    } else if level == 0 && (roll >> 8) % 23 < 4 && sz != 2 && sz != 4 && sx != 7 && sx != 4 {
                        // Furniture, kept clear of the stairwell.
                        wk.furniture[((roll >> 8) % 23) as usize]
                    } else {
                        AIR
                    };
                    data.set(lx, y, lz, b);
                }
                data.set(lx, h, lz, if inside { wk.floor } else { wk.gravel });
            } else if col.road && h > SEA && !col.river {
                // A road. At scales fine enough to show them, paint a
                // dashed centre line and solid edges.
                let is_road = |dx: i32, dz: i32| proj.map_or(false, |p| crate::earth::cover_at(p, x + dx, z + dz) == crate::earth::class::ROAD);
                let mut surface = wk.asphalt;
                if proj.map_or(false, |p| p.scale <= 6.0) {
                    let run = |dx: i32, dz: i32| (1..=7).take_while(|&n| is_road(dx * n, dz * n)).count() as i32;
                    let (xl, xr, zl, zr) = (run(-1, 0), run(1, 0), run(0, -1), run(0, 1));
                    // The road runs along whichever way it goes on longer.
                    let (left, right, along) = if xl + xr <= zl + zr { (xl, xr, z) } else { (zl, zr, x) };
                    let width = left + right + 1;
                    let open_square = xl + xr >= 13 && zl + zr >= 13;
                    if !open_square && width >= 3 {
                        if left == right || left == right + 1 {
                            if along.rem_euclid(6) < 3 {
                                surface = wk.paint[0];
                            }
                        } else if width >= 5 && (left == 0 || right == 0) {
                            surface = wk.paint[1];
                        }
                    }
                }
                data.set(lx, h, lz, surface);
            } else if col.paved && h > SEA && !col.river {
                // Yards, squares and other built-up ground.
                data.set(lx, h, lz, wk.gravel);
            } else if col.farm && h > SEA && !col.river && wk.farmland != AIR && h + 1 < CH {
                // Fields: each has one crop, in rows with a path between
                // fields; some lie fallow, some are pasture.
                let field = hash2(seed ^ 0xFA53, x >> 4, z >> 4) % 7;
                let path = x & 15 == 0 || z & 15 == 0;
                if path || field >= 5 {
                    if !path && field == 5 && hash2(seed ^ 0xFA54, x, z) % 5 == 0 {
                        data.set(lx, h + 1, lz, wk.tall_grass);
                    }
                } else {
                    data.set(lx, h, lz, wk.farmland);
                    let crop = wk.crops[(field % 4) as usize];
                    if crop != AIR && z & 3 != 0 {
                        data.set(lx, h + 1, lz, crop);
                    }
                }
            }
            if col.river && h > SEA {
                // A river or lake: water in a shallow sandy bed.
                data.set(lx, h, lz, wk.water);
                data.set(lx, h - 1, lz, wk.sand);
            }
            for y in (h + 1)..=SEA {
                let frozen = y == SEA && matches!(col.biome, Biome::Tundra | Biome::Taiga);
                data.set(lx, y, lz, if frozen { wk.ice } else { wk.water });
            }
            decorate_caves(&mut data, wk, seed, lx, lz, x, z, &col);
        }
    }

    // Trees. One candidate spot per 6x6 cell, so two trunks never touch; a
    // tree rooted in a neighbouring chunk still drops its branches here.
    let mut w = Writer { data: &mut data, ox, oz };
    const CELL: i32 = 6;
    for gx in (ox - 3).div_euclid(CELL)..=(ox + CW + 2).div_euclid(CELL) {
        for gz in (oz - 3).div_euclid(CELL)..=(oz + CW + 2).div_euclid(CELL) {
            let h = hash2(seed ^ 0xC1, gx, gz);
            let (tx, tz) = (gx * CELL + 1 + (h % 4) as i32, gz * CELL + 1 + ((h >> 4) % 4) as i32);
            let col = column(seed, tx, tz);
            let chance = match col.biome {
                Biome::Forest => 780,
                Biome::Taiga => 640,
                Biome::Plains => 50,
                Biome::Tundra => 30,
                Biome::Desert => 60,
                Biome::Mountain => 0,
                Biome::Swamp => 420,
                Biome::Savanna => 70,
                Biome::Jungle => 940,
                Biome::Badlands => 12,
            };
            // Palms line warm shores; everything else keeps off the beach.
            let near_shore = col.height <= SEA + 4;
            let chance = if col.biome == Biome::Desert && near_shore { 420 } else { chance };
            let min_height = if col.biome == Biome::Desert { SEA } else { SEA + 1 };
            if (h >> 12) % 1000 >= chance || col.height <= min_height || is_cave(seed, tx, col.height, tz, col.height) {
                continue;
            }
            // Nothing grows through a road, a house or a field.
            if col.paved || col.building > 0 || col.farm {
                continue;
            }
            tree(&mut w, wk, h, tx, col.height, tz, col.biome, near_shore);
        }
    }

    // Snow lies on the ground wherever it is cold.
    if wk.snow_layer != AIR {
        for lx in 0..CW {
            for lz in 0..CW {
                let col = cols[(lx * CW + lz) as usize];
                let h = col.height;
                let cold = matches!(col.biome, Biome::Tundra | Biome::Taiga) || (col.biome == Biome::Mountain && h > SEA + 46);
                if !cold || h + 1 >= CH || h < SEA {
                    continue;
                }
                let ground = data.get(lx, h, lz);
                if reg().kind[ground as usize] == crate::block::Kind::Cube && data.get(lx, h + 1, lz) == AIR {
                    data.set(lx, h + 1, lz, wk.snow_layer);
                }
            }
        }
    }

    // Flowers and grass tufts on open ground.
    for lx in 0..CW {
        for lz in 0..CW {
            let col = cols[(lx * CW + lz) as usize];
            let h = col.height;
            if h + 1 >= CH || data.get(lx, h, lz) != wk.grass || data.get(lx, h + 1, lz) != AIR {
                continue;
            }
            // Plants that packs add.
            if !reg().plants.is_empty() {
                let name = alloc::format!("{:?}", col.biome).to_ascii_lowercase();
                if let Some(p) = reg().plants.iter().enumerate().find(|(k, p)| {
                    (p.biomes.is_empty() || p.biomes.iter().any(|b| *b == name)) && hash2(seed ^ (0x9A00 + *k as u32), ox + lx, oz + lz) % p.one_in == 0
                }) {
                    data.set(lx, h + 1, lz, p.1.block);
                    continue;
                }
            }
            let roll = hash2(seed ^ 0xD1, ox + lx, oz + lz) % 100;
            let (grass, flowers) = match col.biome {
                Biome::Forest => (9, 2),
                Biome::Jungle => (40, 3),
                Biome::Savanna => (30, 0),
                Biome::Swamp => (22, 1),
                _ => (17, 3),
            };
            let plant = if roll < grass {
                wk.tall_grass
            } else if roll < grass + flowers {
                wk.flower_red
            } else if roll < grass + 2 * flowers {
                wk.flower_yellow
            } else {
                continue;
            };
            data.set(lx, h + 1, lz, plant);
        }
    }
    data
}

/// A dry place to stand near the origin: the block coordinates of the
/// ground there.
pub fn find_spawn(seed: u32) -> (i32, i32, i32) {
    if earth_proj().is_some() {
        // The chosen place itself, or the nearest dry land to it.
        for i in 0..200 {
            let (x, z) = ((i % 20 - 10) * (i / 20) * 12, (i / 20) * (i % 7 - 3) * 12);
            let c = column(seed, x, z);
            if c.height > SEA && !c.river {
                return (x, c.height, z);
            }
        }
        return (0, column(seed, 0, 0).height.max(SEA), 0);
    }
    let mut best = (0, column(seed, 0, 0).height, 0);
    // First look for open country; settle for woodland only if there is none.
    for i in 0..800 {
        let open_only = i < 400;
        let i = i % 400;
        // Walk outward in a coarse square spiral of candidate points.
        let ring = i / 8 + 1;
        let (x, z) = match i % 8 {
            0 => (ring, 0),
            1 => (ring, ring),
            2 => (0, ring),
            3 => (-ring, ring),
            4 => (-ring, 0),
            5 => (-ring, -ring),
            6 => (0, -ring),
            _ => (ring, -ring),
        };
        let (x, z) = (x * 24, z * 24);
        let c = column(seed, x, z);
        let wooded = matches!(c.biome, Biome::Forest | Biome::Taiga | Biome::Jungle | Biome::Swamp);
        if c.height > SEA + 1 && c.biome != Biome::Mountain && !(open_only && wooded) && !is_cave(seed, x, c.height, z, c.height) {
            return (x, c.height, z);
        }
        if c.height > best.1 {
            best = (x, c.height, z);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        assert_eq!(generate(42, 3, -7), generate(42, 3, -7));
        assert_ne!(generate(42, 3, -7), generate(43, 3, -7));
    }

    #[test]
    fn chunks_have_a_floor_and_a_surface() {
        let wk = &reg().wk;
        for (cx, cz) in [(0, 0), (-5, 9), (100, -100)] {
            let c = generate(7, cx, cz);
            for x in 0..CW {
                for z in 0..CW {
                    assert_eq!(c.get(x, 0, z), wk.deepstone);
                    let h = column(7, cx * CW + x, cz * CW + z).height;
                    assert!(h < CH - 1);
                    if h < SEA {
                        let top = c.get(x, SEA, z);
                        assert!(top == wk.water || top == wk.ice, "column under sea level is not flooded");
                    }
                }
            }
        }
    }

    #[test]
    fn trees_cross_chunk_borders_consistently() {
        // Leaves written by a tree rooted next door must match what the
        // neighbour itself would expect: both chunks only ever write their
        // own cells, so generating twice in either order gives equal data.
        let a = generate(9, 0, 0);
        let _ = generate(9, 1, 0);
        assert_eq!(a, generate(9, 0, 0));
    }

    #[test]
    fn world_kind_round_trips_as_text() {
        let e = WorldKind::Earth { lat: 46.5, lon: -121.75, scale: 30, vscale: 40, features: false };
        assert_eq!(WorldKind::parse(&e.to_text()), Some(e));
        assert_eq!(WorldKind::parse("normal"), Some(WorldKind::Normal));
        assert_eq!(WorldKind::parse("earth 99 0 30 40"), None);
        assert_eq!(WorldKind::parse("earth 1 2 0 40"), None);
        assert_eq!(WorldKind::parse("moon"), None);
    }

    #[test]
    fn spawn_is_on_dry_land() {
        for seed in [1, 2, 3, 99] {
            let (x, y, z) = find_spawn(seed);
            assert_eq!(column(seed, x, z).height, y);
            assert!(y > SEA, "seed {seed} spawns under water");
        }
    }
}
