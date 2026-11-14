//! Explosions: one function, [`explode`], that TNT, exploding creatures,
//! the `/blast` command and mods all use.
//!
//! An explosion of `power` clears a rough ball of blocks of radius
//! `power * 2` round its centre (the edge is ragged, a cell's reach differing
//! by a fixed hash of its place). Water and lava, unbreakable blocks and
//! blocks tagged `tag blastproof` stay. In survival some of what is broken
//! drops as items, and what a chest held is scattered. TNT in the ball is
//! lit with a short random fuse (see `game::primed`), so a stack goes off in
//! a chain. Creatures and the player within a radius half as big again are
//! hurt and thrown the less the farther they are.
//!
//! Mods hear of it three ways (docs/MODDING.md): the native hooks
//! `before_explode` (may change or cancel it) and `after_explode`, and the
//! script events `explode` and `blockexploded`.

use crate::block::{self, reg, Block, Kind, AIR};
use crate::chunk::CH;
use crate::interact;
use crate::math::{cosf, floord, sinf, sqrtf, TAU};
use crate::session::{GameMode, Session};
use alloc::string::String;
use alloc::vec::Vec;

/// The largest power an explosion has (a ball 16 blocks in radius).
pub const MAX_POWER: f32 = 8.0;
/// Blocks of radius for each point of power.
pub const RADIUS_PER_POWER: f32 = 2.0;
/// Creatures and players are hurt out to this many times the blast's radius.
pub const HURT_REACH: f32 = 1.5;
/// Most items one explosion drops (chest contents aside).
const MAX_DROPS: usize = 24;

/// What set an explosion off.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExplosionSource {
    /// A block of TNT.
    Tnt,
    /// A creature (an index into the registry's entities).
    Creature(usize),
    /// The `/blast` command.
    Command,
    /// A mod's own code, with a number of its choosing.
    Mod(u32),
}

impl ExplosionSource {
    /// The name scripts see: `tnt`, `command`, the creature's id, or `mod<N>`.
    pub fn name(&self) -> String {
        match *self {
            ExplosionSource::Tnt => String::from("tnt"),
            ExplosionSource::Command => String::from("command"),
            ExplosionSource::Creature(k) => reg().entities.get(k).map_or_else(|| String::from("creature"), |e| e.id.clone()),
            ExplosionSource::Mod(n) => alloc::format!("mod{}", n),
        }
    }
}

/// An explosion about to happen, as the native hook `before_explode` may
/// change it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExplosionInfo {
    /// The middle of it.
    pub at: [f64; 3],
    pub power: f32,
    pub source: ExplosionSource,
}

// ---- Pure rules.

/// The radius of the ball an explosion of this power clears.
pub fn radius(power: f32) -> f32 {
    power * RADIUS_PER_POWER
}

/// How far from the middle creatures and players are hurt.
pub fn hurt_reach(power: f32) -> f32 {
    radius(power) * HURT_REACH
}

/// A scrambled number from a cell and a seed.
fn hash(c: [i32; 3], seed: u32) -> u32 {
    let mut h = seed ^ (c[0] as u32).wrapping_mul(0x9E37_79B1) ^ (c[1] as u32).wrapping_mul(0x85EB_CA77) ^ (c[2] as u32).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^ (h >> 15)
}

/// How much of the radius reaches a cell: 0.8 to 1 (so the edge is ragged).
fn edge_reach(c: [i32; 3], seed: u32) -> f32 {
    0.8 + (hash(c, seed) & 0xFF) as f32 / 255.0 * 0.2
}

/// A number to make the ragged edge from, made from the place of the blast.
pub fn seed_of(at: [f64; 3]) -> u32 {
    (floord(at[0]) as u32).wrapping_mul(73_856_093) ^ (floord(at[1]) as u32).wrapping_mul(19_349_663) ^ (floord(at[2]) as u32).wrapping_mul(83_492_791)
}

/// The cells a blast of this radius reaches: those whose middles are within
/// the radius (less a ragged edge: everything within 80% of it is in), from
/// height 1 up (the floor of the world stays) to the top.
pub fn blast_cells(at: [f64; 3], radius: f32, seed: u32) -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    if !(radius > 0.0) || !at[0].is_finite() || !at[1].is_finite() || !at[2].is_finite() {
        return out;
    }
    let r = radius as f64;
    let lo = [floord(at[0] - r), floord(at[1] - r).max(1), floord(at[2] - r)];
    let hi = [floord(at[0] + r), floord(at[1] + r).min(CH - 1), floord(at[2] + r)];
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                let (dx, dy, dz) = ((x as f64 + 0.5 - at[0]) as f32, (y as f64 + 0.5 - at[1]) as f32, (z as f64 + 0.5 - at[2]) as f32);
                let reach = radius * edge_reach([x, y, z], seed);
                if dx * dx + dy * dy + dz * dz <= reach * reach {
                    out.push([x, y, z]);
                }
            }
        }
    }
    out
}

/// How much of its strength a blast of this power still has at `dist`
/// blocks from the middle: 1 at the middle, 0 at [`hurt_reach`] and beyond.
pub fn falloff(dist: f32, power: f32) -> f32 {
    let reach = hurt_reach(power);
    if !(reach > 0.0) || !(dist < reach) {
        return 0.0;
    }
    let d = if dist < 0.0 { 0.0 } else { dist };
    1.0 - d / reach
}

/// Health lost by one `dist` blocks from the middle: a power of 2.5 (TNT)
/// takes 20 point blank, 5 half way out.
pub fn blast_damage(dist: f32, power: f32) -> i32 {
    let f = falloff(dist, power);
    (f * f * power * 8.0 + 0.5) as i32
}

/// The speed one `dist` blocks from the middle is thrown away at.
pub fn blast_knockback(dist: f32, power: f32) -> f32 {
    let k = falloff(dist, power) * power * 3.0;
    if k > 12.0 {
        12.0
    } else {
        k
    }
}

/// Whether an explosion leaves this block standing: liquids, unbreakable
/// blocks, anything tagged `blastproof`, and obsidian or bedrock if a pack
/// has some.
pub fn resists(b: Block) -> bool {
    let d = block::def(b);
    d.unbreakable || d.x.fluid.is_some() || reg().kind[b as usize] == Kind::Liquid || d.x.tag("blastproof").is_some() || d.id.contains("obsidian") || d.id.contains("bedrock")
}

/// The direction from `from` to `to` as a unit vector (straight up if they
/// are in the same place), and the distance.
pub fn away(from: [f64; 3], to: [f64; 3]) -> ([f32; 3], f32) {
    let v = [(to[0] - from[0]) as f32, (to[1] - from[1]) as f32, (to[2] - from[2]) as f32];
    let d = sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    if d < 0.001 {
        ([0.0, 1.0, 0.0], 0.0)
    } else {
        ([v[0] / d, v[1] / d, v[2] / d], d)
    }
}

// ---- The explosion itself.

/// Set off an explosion of `power` at `at`. See the module's notes; the
/// mods' `before_explode` hooks may change or cancel it first.
pub fn explode(g: &mut Session, at: [f64; 3], power: f32, source: ExplosionSource) {
    if !(power > 0.0) || !at[0].is_finite() || !at[1].is_finite() || !at[2].is_finite() {
        return;
    }
    let mut info = ExplosionInfo { at, power: if power > MAX_POWER { MAX_POWER } else { power }, source };
    for (_, n) in crate::native::active() {
        if let Some(before) = n.before_explode {
            if !before(g, &mut info) {
                return;
            }
        }
    }
    // Scripts (script/bridge.rs) may cancel it or change its power too.
    if !crate::script::bridge::explode(g, &mut info) {
        return;
    }
    // A hook may have made it nonsense.
    if !(info.power > 0.0) || !info.at.iter().all(|v| v.is_finite()) {
        return;
    }
    if info.power > MAX_POWER {
        info.power = MAX_POWER;
    }
    let (at, power) = (info.at, info.power);
    g.fire("explode", &info.source.name());

    let survival = g.mode == GameMode::Survival;
    let wanted = reg().scripts.iter().any(|s| s.event == "blockexploded");
    let mut kinds: Vec<Block> = Vec::new();
    let mut chain: Vec<[i32; 3]> = Vec::new();
    let mut drops_left = MAX_DROPS;
    let mut destroyed = 0usize;
    // Under water the blast breaks no blocks and hurts less (game/fluid_rules.rs).
    let wet = block::is_water(g.world.get(floord(at[0]), floord(at[1]), floord(at[2])));
    let breaks = if wet { crate::game::fluid_rules::underwater_block_power(power) } else { power };
    for c in blast_cells(at, radius(breaks), seed_of(at)) {
        let b = g.world.get(c[0], c[1], c[2]);
        if b == AIR || resists(b) {
            continue;
        }
        let d = block::def(b);
        destroyed += 1;
        if wanted && !kinds.contains(&d.first) {
            kinds.push(d.first);
        }
        // Another block of TNT catches instead of breaking.
        if crate::primed::power_of(b).is_some() {
            chain.push(c);
            continue;
        }
        let here = [c[0] as f64 + 0.5, c[1] as f64 + 0.5, c[2] as f64 + 0.5];
        if let Some(inside) = g.chests.remove(&c) {
            if survival {
                let stacks: Vec<(u16, u16)> = inside.slots.iter().flatten().map(|s| (s.item, s.count)).collect();
                for (item, count) in stacks {
                    let v = scatter(g);
                    g.drop_item(here, v, item, count);
                }
            }
        }
        if survival && drops_left > 0 {
            if let Some(item) = d.drop {
                let (r, r2) = (g.primed.roll(), g.primed.roll());
                if r % 100 < d.drop_chance as u32 && r2 % 3 == 0 {
                    let v = scatter(g);
                    g.drop_item(here, v, item as u16, 1);
                    drops_left -= 1;
                }
            }
        }
        interact::remove(&mut g.world, c);
    }
    for c in chain {
        let fuse = crate::primed::chain_fuse(g.primed.roll());
        crate::primed::prime_at(g, c, fuse);
    }

    if wet {
        let softened = ExplosionInfo { power: power * crate::game::fluid_rules::UNDERWATER_HURT, ..info };
        hurt_things(g, &softened);
    } else {
        hurt_things(g, &info);
    }
    show(g, &info);

    // Scripts: once for each kind of block that went.
    for b in kinds {
        g.fire("blockexploded", &block::def(b).id);
    }
    for (_, n) in crate::native::active() {
        if let Some(after) = n.after_explode {
            after(g, &info, destroyed);
        }
    }
}

/// A small random toss for a dropped item.
fn scatter(g: &mut Session) -> [f32; 3] {
    let (a, b, c) = (g.primed.unit(), g.primed.unit(), g.primed.unit());
    [(a - 0.5) * 4.0, 2.0 + b * 3.0, (c - 0.5) * 4.0]
}

/// Hurt and throw the creatures and the player near the blast.
fn hurt_things(g: &mut Session, info: &ExplosionInfo) {
    let (at, power) = (info.at, info.power);
    let reach = hurt_reach(power);
    // Creatures first: who is where is looked up before any is struck.
    let mut hit: Vec<(usize, i32, f32, [f32; 3])> = Vec::new();
    for (i, m) in g.mobs.list.iter().enumerate() {
        let middle = [m.body.pos[0], m.body.pos[1] + m.body.height * 0.5, m.body.pos[2]];
        let (dir, dist) = away(at, middle);
        if dist < reach {
            hit.push((i, blast_damage(dist, power), blast_knockback(dist, power), dir));
        }
    }
    for (i, damage, knock, dir) in hit {
        if damage > 0 {
            g.strike(i, damage, at);
        }
        if let Some(m) = g.mobs.list.get_mut(i) {
            m.body.vel[0] += dir[0] * knock;
            m.body.vel[1] += dir[1] * knock + knock * 0.4;
            m.body.vel[2] += dir[2] * knock;
        }
    }
    // The player.
    if !g.dead {
        let p = &g.player;
        let middle = [p.pos[0], p.pos[1] + p.height * 0.5, p.pos[2]];
        let (dir, dist) = away(at, middle);
        if dist < reach {
            let (damage, knock) = (blast_damage(dist, power), blast_knockback(dist, power));
            if damage > 0 {
                if let ExplosionSource::Creature(k) = info.source {
                    // So that a mod's `hurt` hook can tell who it was.
                    g.mobs.last_biter = Some(k);
                }
                // (Armour takes a share of a blast; see game/gear/armor.rs.)
                g.ev.pending_cause = alloc::string::String::from("explosion");
                g.hurt_later(damage);
            }
            if !g.player.flying && knock > 0.0 {
                g.player.vel[0] += dir[0] * knock;
                g.player.vel[1] += dir[1] * knock + knock * 0.4;
                g.player.vel[2] += dir[2] * knock;
            }
        }
    }
}

/// The sound, the sparks, the flash and the shaking of the view.
fn show(g: &mut Session, info: &ExplosionInfo) {
    let (at, power) = (info.at, info.power);
    g.sound_queue.push(String::from("explosion"));
    let n = ((power * 10.0) as usize).clamp(8, 80);
    for _ in 0..n {
        let (a, e, s, life, pick) = (g.primed.unit(), g.primed.unit(), g.primed.unit(), g.primed.unit(), g.primed.unit());
        let y = e * 2.0 - 1.0;
        let r = sqrtf(1.0 - y * y);
        let speed = 3.0 + s * 9.0;
        let colour = if pick < 0.4 {
            [255, 170, 40]
        } else if pick < 0.7 {
            [255, 232, 130]
        } else {
            [96, 96, 100]
        };
        g.primed.spark(at, [cosf(a * TAU) * r * speed, y * speed + 2.0, sinf(a * TAU) * r * speed], 0.4 + life * 0.8, colour);
    }
    let (_, dist) = away(at, [g.player.pos[0], g.player.pos[1] + g.player.height * 0.5, g.player.pos[2]]);
    let near = radius(power) * 4.0;
    if dist < near {
        let k = (1.0 - dist / near) * 0.8;
        if k > g.shake {
            g.shake = k;
        }
    }
    if dist < radius(power) * 2.0 && g.flash < 0.1 {
        g.flash = 0.1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dist2(at: [f64; 3], c: [i32; 3]) -> f32 {
        let (dx, dy, dz) = (c[0] as f64 + 0.5 - at[0], c[1] as f64 + 0.5 - at[1], c[2] as f64 + 0.5 - at[2]);
        (dx * dx + dy * dy + dz * dz) as f32
    }

    #[test]
    fn the_ball_has_the_right_size() {
        let at = [100.5, 64.5, -30.5];
        let cells = blast_cells(at, 5.0, 7);
        // About a sphere of radius 5 (volume 523), a little less for the ragged edge.
        assert!(cells.len() > 300 && cells.len() < 540, "{} cells", cells.len());
        for c in &cells {
            assert!(dist2(at, *c) <= 25.0 + 0.01, "{c:?} is outside");
        }
        // Everything within 80% of the radius is in; the middle certainly.
        assert!(cells.contains(&[100, 64, -31]));
        for x in 96..=104 {
            for y in 60..=68 {
                for z in -35..=-27 {
                    if dist2(at, [x, y, z]) <= 16.0 {
                        assert!(cells.contains(&[x, y, z]), "{x} {y} {z} should be in");
                    }
                }
            }
        }
    }

    #[test]
    fn the_edge_is_ragged_but_the_same_every_time() {
        let at = [0.5, 80.5, 0.5];
        assert_eq!(blast_cells(at, 6.0, 3), blast_cells(at, 6.0, 3));
        assert_ne!(blast_cells(at, 6.0, 3), blast_cells(at, 6.0, 4));
    }

    #[test]
    fn a_blast_never_digs_below_the_floor_or_above_the_sky() {
        let low = blast_cells([0.5, 2.5, 0.5], 6.0, 1);
        assert!(!low.is_empty() && low.iter().all(|c| c[1] >= 1));
        let high = blast_cells([0.5, 254.5, 0.5], 6.0, 1);
        assert!(!high.is_empty() && high.iter().all(|c| c[1] < CH));
    }

    #[test]
    fn nonsense_blasts_are_empty() {
        assert!(blast_cells([0.0; 3], 0.0, 0).is_empty());
        assert!(blast_cells([0.0; 3], -3.0, 0).is_empty());
        assert!(blast_cells([f64::NAN, 0.0, 0.0], 3.0, 0).is_empty());
        assert!(blast_cells([0.0, f64::INFINITY, 0.0], 3.0, 0).is_empty());
        assert!(blast_cells([0.0; 3], f32::NAN, 0).is_empty());
    }

    #[test]
    fn damage_falls_off_with_distance() {
        let tnt = 2.5;
        assert_eq!(blast_damage(0.0, tnt), 20);
        let mut last = i32::MAX;
        for k in 0..=80 {
            let d = blast_damage(k as f32 * 0.1, tnt);
            assert!(d <= last, "damage grew at {k}");
            last = d;
        }
        assert_eq!(blast_damage(hurt_reach(tnt), tnt), 0);
        assert_eq!(blast_damage(100.0, tnt), 0);
        assert!(blast_damage(3.0, tnt) > blast_damage(6.0, tnt));
        // A stronger blast hurts more at the same place.
        assert!(blast_damage(4.0, 4.0) > blast_damage(4.0, 2.0));
        assert_eq!(blast_damage(-1.0, tnt), 20);
    }

    #[test]
    fn knockback_falls_off_and_is_capped() {
        assert!(blast_knockback(0.0, 2.5) > blast_knockback(4.0, 2.5));
        assert_eq!(blast_knockback(50.0, 2.5), 0.0);
        assert_eq!(blast_knockback(0.0, MAX_POWER), 12.0);
    }

    #[test]
    fn away_points_from_the_middle() {
        let (dir, d) = away([0.0; 3], [3.0, 4.0, 0.0]);
        assert!((d - 5.0).abs() < 1e-4);
        assert!((dir[0] - 0.6).abs() < 1e-4 && (dir[1] - 0.8).abs() < 1e-4 && dir[2] == 0.0);
        // In the same place: up.
        assert_eq!(away([1.0, 2.0, 3.0], [1.0, 2.0, 3.0]), ([0.0, 1.0, 0.0], 0.0));
    }

    #[test]
    fn sources_have_names_for_scripts() {
        assert_eq!(ExplosionSource::Tnt.name(), "tnt");
        assert_eq!(ExplosionSource::Command.name(), "command");
        assert_eq!(ExplosionSource::Mod(7).name(), "mod7");
        assert_eq!(ExplosionSource::Creature(usize::MAX).name(), "creature");
        if !reg().entities.is_empty() {
            assert_eq!(ExplosionSource::Creature(0).name(), reg().entities[0].id);
        }
    }

    #[test]
    fn water_and_unbreakable_blocks_stay_and_stone_does_not() {
        let r = reg();
        assert!(resists(r.wk.water));
        assert!(resists(r.wk.lava));
        assert!(!resists(r.wk.stone));
        assert!(!resists(r.wk.dirt));
        for (i, d) in r.blocks.iter().enumerate() {
            if d.unbreakable {
                assert!(resists(i as Block), "{} is unbreakable", d.id);
            }
        }
    }
}
