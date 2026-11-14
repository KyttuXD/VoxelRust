//! Where the heads of creatures look.
//!
//! A creature whose model has a bone called `head` turns it, smoothly, toward
//! the nearest player within [`WATCH_RANGE`] blocks (48 for a creature with
//! the trait `watches`), as far as [`MAX_YAW`] degrees to either side and
//! [`MAX_PITCH`] up or down, unless it is asleep or running away. A script
//! may force the look with `set_creature_look` (see `Extra::look`); a shared
//! game sends only a forced look, every machine works out the ordinary one.
//!
//! Angles are degrees, relative to the creature's body: yaw positive to its
//! left, pitch positive up (as `model::Pose::head_yaw` and `head_pitch`).
//! The turning itself is plain arithmetic ([`aim`], [`clamp_look`],
//! [`approach`]) and tested without a game.

use crate::block::{traits, Usable};
use crate::math::{absf, atan2f, floord, floorf, sqrtf, PI};
use crate::session::Session;
use alloc::vec::Vec;

/// How far a creature notices a player to look at, blocks.
pub const WATCH_RANGE: f32 = 12.0;
/// The same for a creature with the trait `watches`.
pub const WATCHES_RANGE: f32 = 48.0;
/// Farthest the head turns on its own (degrees to either side, and up or down).
pub const MAX_YAW: f32 = 70.0;
pub const MAX_PITCH: f32 = 40.0;
/// What a script may force.
pub const SCRIPT_YAW: f32 = 90.0;
pub const SCRIPT_PITCH: f32 = 60.0;
/// How quickly the head follows (the share of the way it goes in a second, about)
/// and the fastest it ever turns (degrees a second).
pub const EASE: f32 = 7.0;
pub const TURN_RATE: f32 = 420.0;
/// A creature that flees stops looking at a player this close.
const FLEE_DIST: f32 = 7.0;
/// Most watchers (players) considered.
const MAX_WATCHERS: usize = 16;

/// Where a creature's head is turned now, degrees.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Head {
    pub yaw: f32,
    pub pitch: f32,
}

fn fin(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

/// An angle in degrees brought into -180 to 180.
pub fn wrap_deg(a: f32) -> f32 {
    let a = fin(a);
    a - 360.0 * floorf((a + 180.0) / 360.0)
}

/// The head turn (yaw, pitch) that looks from `from` to `to`, for a body
/// facing `body_yaw` (radians, as the creature's own).
pub fn aim(body_yaw: f32, from: [f64; 3], to: [f64; 3]) -> (f32, f32) {
    let (dx, dy, dz) = ((to[0] - from[0]) as f32, (to[1] - from[1]) as f32, (to[2] - from[2]) as f32);
    let flat = sqrtf(dx * dx + dz * dz);
    if !(flat.is_finite() && dy.is_finite()) || (flat < 0.01 && absf(dy) < 0.01) {
        return (0.0, 0.0);
    }
    // (a creature facing yaw looks along (-sin yaw, -cos yaw))
    let want = if flat < 0.01 { body_yaw } else { atan2f(-dx, -dz) };
    let yaw = wrap_deg((want - body_yaw) * 180.0 / PI);
    let pitch = atan2f(dy, flat.max(0.01)) * 180.0 / PI;
    (yaw, pitch)
}

/// A look held within the limits.
pub fn clamp_look(yaw: f32, pitch: f32, max_yaw: f32, max_pitch: f32) -> (f32, f32) {
    (fin(yaw).clamp(-max_yaw, max_yaw), fin(pitch).clamp(-max_pitch, max_pitch))
}

/// Move one angle toward its target: an easing, but never faster than
/// [`TURN_RATE`].
fn step(cur: f32, target: f32, dt: f32) -> f32 {
    let (cur, target) = (fin(cur), fin(target));
    let k = (EASE * dt).clamp(0.0, 1.0);
    let max = TURN_RATE * dt.clamp(0.0, 0.5);
    let d = ((target - cur) * k).clamp(-max, max);
    let now = cur + d;
    // Close enough: settle (so a head at rest is exactly at rest).
    if absf(target - now) < 0.05 {
        target
    } else {
        now
    }
}

/// The head after `dt` seconds of turning toward `target` (yaw, pitch).
pub fn approach(h: Head, target: (f32, f32), dt: f32) -> Head {
    if !(dt > 0.0) {
        return h;
    }
    Head { yaw: step(h.yaw, target.0, dt), pitch: step(h.pitch, target.1, dt) }
}

/// How far a creature with these traits notices players.
pub fn range_of(trait_bits: u32) -> f32 {
    if trait_bits & traits::WATCHES != 0 {
        WATCHES_RANGE
    } else {
        WATCH_RANGE
    }
}

/// The look a creature wants this frame: `watchers` are the places players'
/// eyes are, `head_at` where its own head is. `None` when nothing is in range.
/// A creature that `flees` and has a player within a few blocks wants to look
/// straight ahead (it is running).
pub fn wanted(body_yaw: f32, head_at: [f64; 3], watchers: &[[f64; 3]], trait_bits: u32, tamed: bool) -> Option<(f32, f32)> {
    let range = range_of(trait_bits);
    let mut best: Option<(f32, [f64; 3])> = None;
    for w in watchers {
        let (dx, dy, dz) = ((w[0] - head_at[0]) as f32, (w[1] - head_at[1]) as f32, (w[2] - head_at[2]) as f32);
        let d2 = dx * dx + dy * dy + dz * dz;
        if d2.is_finite() && d2 <= range * range && best.map_or(true, |b| d2 < b.0) {
            best = Some((d2, *w));
        }
    }
    let (d2, at) = best?;
    if trait_bits & traits::FLEES != 0 && !tamed && d2 < FLEE_DIST * FLEE_DIST {
        return None;
    }
    let (yaw, pitch) = aim(body_yaw, head_at, at);
    Some(clamp_look(yaw, pitch, MAX_YAW, MAX_PITCH))
}

/// Once a frame: turn the heads of the creatures.
pub fn tick(g: &mut Session, dt: f32) {
    if !(dt > 0.0) || g.mobs.list.is_empty() {
        return;
    }
    let mut eyes: Vec<[f64; 3]> = Vec::new();
    let p = g.player.pos;
    eyes.push([p[0], p[1] + crate::player::EYE, p[2]]);
    if let Some(net) = &g.net {
        for r in net.players.values().filter(|r| r.seen).take(MAX_WATCHERS - 1) {
            eyes.push([r.pos[0], r.pos[1] + crate::player::EYE, r.pos[2]]);
        }
    }
    let reg = crate::block::reg();
    let world = &g.world;
    for m in g.mobs.list.iter_mut() {
        let forced = m.extra.as_ref().and_then(|e| e.look_deg());
        let bits = reg.entities[m.kind].x.traits;
        let rest = m.head == Head::default();
        // Far from everyone and facing front: nothing to do.
        let range = range_of(bits) + 4.0;
        let near = eyes.iter().any(|e| {
            let (dx, dz) = ((e[0] - m.body.pos[0]) as f32, (e[2] - m.body.pos[2]) as f32);
            dx * dx + dz * dz <= range * range
        });
        if forced.is_none() && !near && rest {
            continue;
        }
        let target = match forced {
            Some((yaw, pitch)) => clamp_look(yaw, pitch, SCRIPT_YAW, SCRIPT_PITCH),
            None if near && !asleep(world, m.body.pos, m.stride) => {
                let height = m.look().2 as f64;
                let head_at = [m.body.pos[0], m.body.pos[1] + height * 0.85, m.body.pos[2]];
                wanted(m.body.yaw, head_at, &eyes, bits, m.tamed).unwrap_or((0.0, 0.0))
            }
            None => (0.0, 0.0),
        };
        m.head = approach(m.head, target, dt);
    }
}

/// Whether a creature standing still where it is lies in a bed.
fn asleep(world: &crate::world::World, pos: [f64; 3], stride: f32) -> bool {
    if stride > 0.05 {
        return false;
    }
    let (x, y, z) = (floord(pos[0]), floord(pos[1] + 0.02), floord(pos[2]));
    crate::block::def(world.get(x, y, z)).usable == Usable::Bed && pos[1] - y as f64 <= 0.8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_wrap_into_half_a_turn() {
        assert_eq!(wrap_deg(0.0), 0.0);
        assert!((wrap_deg(190.0) + 170.0).abs() < 1e-3);
        assert!((wrap_deg(-190.0) - 170.0).abs() < 1e-3);
        assert!((wrap_deg(720.0 + 10.0) - 10.0).abs() < 1e-3);
        assert_eq!(wrap_deg(f32::NAN), 0.0);
        assert!(wrap_deg(1.0e9).abs() <= 180.0);
    }

    #[test]
    fn a_player_ahead_is_straight_on_left_is_positive_and_above_is_up() {
        // A creature at the origin facing -z (yaw 0): a player at -z is dead ahead.
        let head = [0.0, 1.0, 0.0];
        let (y, p) = aim(0.0, head, [0.0, 1.0, -5.0]);
        assert!(y.abs() < 1e-3 && p.abs() < 1e-3);
        // At -x is to its left (it faces -z, its left is -x): positive.
        let (y, _) = aim(0.0, head, [-5.0, 1.0, 0.0]);
        assert!((y - 90.0).abs() < 0.1, "{y}");
        let (y, _) = aim(0.0, head, [5.0, 1.0, 0.0]);
        assert!((y + 90.0).abs() < 0.1, "{y}");
        // Turned to face -x (yaw +90 degrees), a player at -x is ahead again.
        let (y, _) = aim(PI * 0.5, head, [-5.0, 1.0, 0.0]);
        assert!(y.abs() < 0.1, "{y}");
        // Above: pitch up, 45 degrees at equal rise and run; below: down.
        let (_, p) = aim(0.0, head, [0.0, 6.0, -5.0]);
        assert!((p - 45.0).abs() < 0.1);
        let (_, p) = aim(0.0, head, [0.0, -4.0, -5.0]);
        assert!(p < -30.0);
        // Straight overhead or at the same place: no spin, no NaN.
        let (y, p) = aim(0.3, head, [0.0, 9.0, 0.0]);
        assert!(y.abs() < 1e-3 && (p - 90.0).abs() < 0.5);
        assert_eq!(aim(0.3, head, head), (0.0, 0.0));
        assert_eq!(aim(0.0, head, [f64::NAN, 0.0, 0.0]), (0.0, 0.0));
    }

    #[test]
    fn the_look_is_clamped_to_seventy_and_forty() {
        assert_eq!(clamp_look(170.0, 80.0, MAX_YAW, MAX_PITCH), (70.0, 40.0));
        assert_eq!(clamp_look(-170.0, -80.0, MAX_YAW, MAX_PITCH), (-70.0, -40.0));
        assert_eq!(clamp_look(10.0, -5.0, MAX_YAW, MAX_PITCH), (10.0, -5.0));
        assert_eq!(clamp_look(f32::NAN, f32::INFINITY, MAX_YAW, MAX_PITCH), (0.0, 0.0));
        // A player right behind: the head goes as far as it can.
        let w = wanted(0.0, [0.0, 1.0, 0.0], &[[0.0, 1.0, 5.0]], 0, false).unwrap();
        assert!(w.0.abs() == 70.0 || w.0.abs() > 69.99, "{w:?}");
    }

    #[test]
    fn the_head_follows_smoothly_without_overshooting_and_settles() {
        let mut h = Head::default();
        let mut last = 0.0;
        for _ in 0..30 {
            h = approach(h, (60.0, 20.0), 1.0 / 60.0);
            assert!(h.yaw >= last && h.yaw <= 60.0 && h.pitch <= 20.0, "{h:?}");
            last = h.yaw;
        }
        // Half a second in, it is on its way but not there.
        assert!(h.yaw > 10.0 && h.yaw < 60.0, "{h:?}");
        // No faster than the top rate, even in one long frame.
        let jump = approach(Head::default(), (70.0, 0.0), 0.5);
        assert!(jump.yaw <= TURN_RATE * 0.5 + 1e-3 && jump.yaw > 0.0);
        for _ in 0..600 {
            h = approach(h, (60.0, 20.0), 1.0 / 60.0);
        }
        assert_eq!(h, Head { yaw: 60.0, pitch: 20.0 });
        // And back to front.
        for _ in 0..600 {
            h = approach(h, (0.0, 0.0), 1.0 / 60.0);
        }
        assert_eq!(h, Head::default());
        // Odd times change nothing, bad numbers do not stick.
        assert_eq!(approach(h, (50.0, 0.0), 0.0), h);
        assert_eq!(approach(h, (50.0, 0.0), -1.0), h);
        assert!(approach(Head { yaw: f32::NAN, pitch: 0.0 }, (1.0, 1.0), 0.016).yaw.is_finite());
    }

    #[test]
    fn a_head_swinging_from_one_side_to_the_other_stays_within_its_limits_and_goes_the_short_way() {
        // From full left to full right: every frame is between the two, moving one way only.
        let mut h = Head { yaw: MAX_YAW, pitch: MAX_PITCH };
        let mut last = h.yaw;
        for _ in 0..400 {
            h = approach(h, (-MAX_YAW, -MAX_PITCH), 1.0 / 60.0);
            assert!(h.yaw <= last && h.yaw >= -MAX_YAW && h.pitch >= -MAX_PITCH && h.pitch <= MAX_PITCH, "{h:?}");
            last = h.yaw;
        }
        assert_eq!(h, Head { yaw: -MAX_YAW, pitch: -MAX_PITCH });
        // A script's forced look goes beyond the creature's own limits, but not past its own.
        assert_eq!(clamp_look(500.0, -500.0, SCRIPT_YAW, SCRIPT_PITCH), (90.0, -60.0));
        // The same walk at a slow frame rate (10 a second) and a fast one (240) ends in the same place.
        for dt in [0.1f32, 1.0 / 240.0] {
            let mut h = Head::default();
            for _ in 0..(10.0 / dt) as usize {
                h = approach(h, (45.0, -10.0), dt);
            }
            assert_eq!(h, Head { yaw: 45.0, pitch: -10.0 });
        }
        // A frame of a whole minute (a stall) turns no faster than the top rate allows for half a second.
        let jump = approach(Head::default(), (70.0, 40.0), 60.0);
        assert!(jump.yaw <= TURN_RATE * 0.5 + 1e-3 && jump.pitch <= TURN_RATE * 0.5 + 1e-3);
    }

    #[test]
    fn creatures_watch_the_nearest_player_in_range() {
        let head = [0.0, 1.0, 0.0];
        let near = [-3.0, 1.0, 0.0];
        let far = [0.0, 1.0, -9.0];
        // The nearer is looked at (to the left, 90 degrees).
        let w = wanted(0.0, head, &[far, near], 0, false).unwrap();
        assert!((w.0 - 90.0f32.min(MAX_YAW)).abs() < 1e-3);
        // Nobody within twelve blocks: no look; a watcher sees forty out.
        let distant = [0.0, 1.0, -30.0];
        assert_eq!(wanted(0.0, head, &[distant], 0, false), None);
        let w = wanted(0.0, head, &[distant], traits::WATCHES, false).unwrap();
        assert!(w.0.abs() < 1e-3);
        assert_eq!(wanted(0.0, head, &[[0.0, 1.0, -60.0]], traits::WATCHES, false), None);
        // The range is the trait's.
        assert_eq!(range_of(0), 12.0);
        assert_eq!(range_of(traits::WATCHES | traits::HERD), 48.0);
        // A runner (flees) close to the player does not look at them; a tame one does.
        assert_eq!(wanted(0.0, head, &[[0.0, 1.0, -4.0]], traits::FLEES, false), None);
        assert!(wanted(0.0, head, &[[0.0, 1.0, -4.0]], traits::FLEES, true).is_some());
        assert!(wanted(0.0, head, &[[0.0, 1.0, -10.0]], traits::FLEES, false).is_some());
        assert_eq!(wanted(0.0, head, &[], 0, false), None);
    }
}
