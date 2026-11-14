//! Embers over lava: small glowing specks that rise from lava open to the air
//! near the player. They are the session's particles (`Primed::sparks`, the
//! `Spark` kind, drawn additive like the sparks of mods) with a rise instead of
//! a fall, so they cost nothing new to draw.
//!
//! The budget, all of it checked in [`wanted`]:
//! * none farther than [`REACH`] blocks (24) from the player;
//! * at most [`MAX_PER_PATCH`] (40) alive within [`PATCH_RADIUS`] of the place
//!   a new one would start (one patch of lava), and [`MAX_TOTAL`] in all;
//! * none when the particle budget is low: [`ROOM`] or more particles alive, or
//!   a frame slow enough that `particles::frame_cap` is 40 or less.
//!
//! Called a few times a second from `Session::weather_update`; about 300 block
//! lookups each time.

use super::particles::{self, Kind};
use crate::block::{fluid_of, Block, AIR};
use crate::game::primed::Spark;
use crate::session::Session;

/// Farthest from the player an ember starts, blocks.
pub const REACH: f64 = 24.0;
/// Embers alive within this many blocks of a new one's start, at most [`MAX_PER_PATCH`].
pub const PATCH_RADIUS: f64 = 6.0;
pub const MAX_PER_PATCH: usize = 40;
/// Embers alive anywhere, at most.
pub const MAX_TOTAL: usize = 120;
/// With this many particles alive (of [`particles::MAX_LIVE`]) no ember is made.
pub const ROOM: usize = particles::MAX_LIVE * 3 / 4;
/// Seconds between two rounds, and the columns looked at in a round.
pub const EVERY: f32 = 0.2;
pub const TRIES: usize = 12;
/// How far above and below the player a surface is looked for.
pub const UP: i32 = 12;
pub const DOWN: i32 = 14;

/// Whether a particle is one of these embers (a spark that rises).
pub fn is_ember(s: &Spark) -> bool {
    s.kind == Kind::Spark.code() && s.grav < 0.0
}

/// How many embers may be made now (0 or 1): `near` alive in the patch,
/// `total` alive, `dist2` the squared distance of the start from the player,
/// `live` particles of every kind alive, `dt` the frame's seconds.
pub fn wanted(near: usize, total: usize, dist2: f64, live: usize, dt: f32) -> u32 {
    if !(dist2 <= REACH * REACH) || near >= MAX_PER_PATCH || total >= MAX_TOTAL || live >= ROOM || particles::frame_cap(dt) <= 40 {
        0
    } else {
        1
    }
}

/// The height of the highest lava with air over it in the column (x, z)
/// between `lo` and `hi` (inclusive), if any.
pub fn surface_in(get: &dyn Fn(i32, i32, i32) -> Block, x: i32, z: i32, lo: i32, hi: i32) -> Option<i32> {
    let mut y = hi;
    while y >= lo {
        let b = get(x, y, z);
        if b != AIR && matches!(fluid_of(b), Some((true, _))) && get(x, y + 1, z) == AIR {
            return Some(y);
        }
        y -= 1;
    }
    None
}

/// The ember at `at`, from random numbers `r` (each 0 to 1).
pub fn ember(at: [f64; 3], r: [f32; 5]) -> Spark {
    let life = 1.6 + 1.4 * r[0];
    // Now and then a livelier one pops off.
    let up = if r[4] < 0.06 { 2.2 + r[1] } else { 0.5 + 0.7 * r[1] };
    Spark {
        pos: at,
        vel: [(r[2] - 0.5) * 0.5, up, (r[3] - 0.5) * 0.5],
        life,
        colour: [255, (130.0 + 90.0 * r[1]) as u8, (20.0 + 40.0 * r[2]) as u8],
        kind: Kind::Spark.code(),
        max_life: life,
        grav: -0.9,
        drag: 0.6,
    }
}

/// Once a frame (the session's `weather_update`): now and then look for lava
/// around the player and let a few embers up.
pub fn tick(g: &mut Session, dt: f32) {
    {
        let s = super::state(g);
        s.ember_clock += if dt > 0.0 && dt < 1.0 { dt } else { 0.0 };
        if s.ember_clock < EVERY {
            return;
        }
        s.ember_clock = 0.0;
    }
    let live = g.primed.sparks.len();
    if live >= ROOM || particles::frame_cap(dt) <= 40 {
        return;
    }
    let mut total = g.primed.sparks.iter().filter(|s| is_ember(s)).count();
    let p = g.player.pos;
    for _ in 0..TRIES {
        if total >= MAX_TOTAL {
            break;
        }
        // A column, nearer the player more often than far.
        let (a, d, jx, jz) = (g.primed.unit(), g.primed.unit(), g.primed.unit(), g.primed.unit());
        let (ang, dist) = (a * crate::math::TAU, REACH as f32 * d * d);
        let x = crate::math::floord(p[0] + (crate::math::cosf(ang) * dist) as f64);
        let z = crate::math::floord(p[2] + (crate::math::sinf(ang) * dist) as f64);
        let py = crate::math::floord(p[1]);
        let y = match surface_in(&|x, y, z| g.world.get(x, y, z), x, z, py - DOWN, py + UP) {
            Some(y) => y,
            None => continue,
        };
        let at = [x as f64 + jx as f64, y as f64 + 1.05, z as f64 + jz as f64];
        let d2 = (at[0] - p[0]) * (at[0] - p[0]) + (at[1] - p[1]) * (at[1] - p[1]) + (at[2] - p[2]) * (at[2] - p[2]);
        let near = g.primed.sparks.iter().filter(|s| is_ember(s) && (s.pos[0] - at[0]) * (s.pos[0] - at[0]) + (s.pos[1] - at[1]) * (s.pos[1] - at[1]) + (s.pos[2] - at[2]) * (s.pos[2] - at[2]) < PATCH_RADIUS * PATCH_RADIUS).count();
        if wanted(near, total, d2, g.primed.sparks.len(), dt) == 0 {
            continue;
        }
        let r = [g.primed.unit(), g.primed.unit(), g.primed.unit(), g.primed.unit(), g.primed.unit()];
        if g.primed.push_particle(ember(at, r)) {
            total += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::reg;

    #[test]
    fn nothing_beyond_twenty_four_blocks() {
        assert_eq!(wanted(0, 0, 23.9 * 23.9, 0, 0.016), 1);
        assert_eq!(wanted(0, 0, 24.1 * 24.1, 0, 0.016), 0);
        assert_eq!(wanted(0, 0, f64::NAN, 0, 0.016), 0);
    }

    #[test]
    fn a_patch_holds_at_most_forty() {
        assert_eq!(wanted(39, 39, 100.0, 0, 0.016), 1);
        assert_eq!(wanted(40, 40, 100.0, 0, 0.016), 0);
        assert_eq!(wanted(400, 40, 100.0, 0, 0.016), 0);
        // Filling a patch one at a time stops at the cap.
        let (mut near, mut made) = (0usize, 0usize);
        for _ in 0..1000 {
            let n = wanted(near, near, 100.0, near, 0.016);
            near += n as usize;
            made += n as usize;
        }
        assert_eq!(made, MAX_PER_PATCH);
    }

    #[test]
    fn all_patches_together_stop_at_the_total() {
        assert_eq!(wanted(0, MAX_TOTAL - 1, 100.0, 0, 0.016), 1);
        assert_eq!(wanted(0, MAX_TOTAL, 100.0, 0, 0.016), 0);
        assert!(MAX_TOTAL <= particles::MAX_LIVE / 10);
    }

    #[test]
    fn none_when_the_particle_budget_is_low() {
        assert_eq!(wanted(0, 0, 100.0, ROOM - 1, 0.016), 1);
        assert_eq!(wanted(0, 0, 100.0, ROOM, 0.016), 0);
        assert_eq!(wanted(0, 0, 100.0, particles::MAX_LIVE, 0.016), 0);
        // A slow frame (under about 25 frames a second) makes none.
        assert_eq!(wanted(0, 0, 100.0, 0, 0.033), 1);
        assert_eq!(wanted(0, 0, 100.0, 0, 0.05), 0);
        assert_eq!(wanted(0, 0, 100.0, 0, 0.5), 0);
        assert_eq!(wanted(0, 0, 100.0, 0, f32::NAN), 0);
    }

    #[test]
    fn an_ember_is_a_rising_spark_and_a_blast_spark_is_not() {
        let e = ember([0.0; 3], [0.5; 5]);
        assert!(is_ember(&e));
        assert!(e.vel[1] > 0.0 && e.life > 1.0 && e.life <= 3.0 && e.max_life == e.life);
        // The livelier ones.
        let pop = ember([0.0; 3], [0.5, 0.5, 0.5, 0.5, 0.01]);
        assert!(pop.vel[1] > e.vel[1]);
        let mut blast = e;
        blast.grav = 14.0;
        assert!(!is_ember(&blast));
        blast.grav = -1.0;
        blast.kind = Kind::Smoke.code();
        assert!(!is_ember(&blast));
        // Any random numbers give a sane ember.
        for r in [[0.0; 5], [1.0; 5]] {
            let e = ember([1.0, 2.0, 3.0], r);
            assert!(e.vel.iter().all(|v| v.abs() < 4.0) && e.life > 0.0);
        }
    }

    #[test]
    fn the_surface_is_the_highest_lava_with_air_over_it() {
        let lava = reg().wk.lava;
        let stone = reg().wk.stone;
        // Lava at 10 and 12 (air over 12), stone at 11 between? No: lava 10, stone 11, lava 12, air above.
        let get = move |_x: i32, y: i32, _z: i32| match y {
            10 | 12 => lava,
            11 => stone,
            _ => AIR,
        };
        assert_eq!(surface_in(&get, 0, 0, 0, 20), Some(12));
        // Looking only below: the lava at 10 has stone over it.
        assert_eq!(surface_in(&get, 0, 0, 0, 11), None);
        assert_eq!(surface_in(&get, 0, 0, 13, 20), None);
        // Lava covered by a roof is not a surface.
        let roofed = move |_x: i32, y: i32, _z: i32| match y {
            10 => lava,
            11 => stone,
            _ => AIR,
        };
        assert_eq!(surface_in(&roofed, 0, 0, 0, 20), None);
        // An empty range gives nothing.
        assert_eq!(surface_in(&get, 0, 0, 5, 4), None);
    }
}
