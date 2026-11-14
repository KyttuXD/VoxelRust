//! Particles a mod asks for (`spawn_particles`): sparks, smoke, flames,
//! bubbles, dust, magic, hearts and clouds.
//!
//! There is one particle system in the game: the session's sparks
//! (`Primed::sparks`), which explosions and burning fuses already used. A
//! particle of a kind is such a spark with its `kind` set; the renderer draws
//! those as little flat pictures (one tile of a small shared picture, tinted
//! the particle's colour, see `client/render/fx.rs`) that shrink or swell and
//! fade out, some added to the picture (`additive`) so that they glow.
//!
//! The budget: at most [`MAX_LIVE`] particles live at once, scripts may add
//! at most [`frame_cap`] in a frame and that number falls when the frame is
//! slow (nothing at all below 15 frames a second), and one that would appear
//! more than [`REACH`] blocks from the player is not made.

use crate::game::primed::Spark;
use crate::math::{cosf, sinf, sqrtf, TAU};
use crate::session::Session;

/// Most particles alive (the same number as the sparks of the explosions).
pub const MAX_LIVE: usize = crate::primed::MAX_SPARKS;
/// Most a single call may make.
pub const MAX_PER_CALL: i64 = 64;
/// Farthest from the player a particle is made.
pub const REACH: f64 = 96.0;
/// Largest spread and speed a call may ask for.
pub const MAX_SPREAD: f64 = 16.0;
pub const MAX_SPEED: f64 = 30.0;

/// What a particle is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Spark,
    Smoke,
    Flame,
    Bubble,
    Dust,
    Magic,
    Heart,
    Cloud,
}

/// How a kind behaves and looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Shortest and longest life, seconds.
    pub life: (f32, f32),
    /// Blocks a second squared it falls (negative: it rises).
    pub grav: f32,
    /// Share of its speed lost each second.
    pub drag: f32,
    /// Half its width, in blocks, when it appears and when it is gone.
    pub size: (f32, f32),
    /// Added to the picture behind it (glows) rather than laid over it.
    pub additive: bool,
}

impl Kind {
    pub const ALL: [Kind; 8] = [Kind::Spark, Kind::Smoke, Kind::Flame, Kind::Bubble, Kind::Dust, Kind::Magic, Kind::Heart, Kind::Cloud];

    pub fn from_name(name: &str) -> Option<Kind> {
        Kind::ALL.iter().copied().find(|k| k.name() == name.to_ascii_lowercase())
    }

    pub fn name(self) -> &'static str {
        match self {
            Kind::Spark => "spark",
            Kind::Smoke => "smoke",
            Kind::Flame => "flame",
            Kind::Bubble => "bubble",
            Kind::Dust => "dust",
            Kind::Magic => "magic",
            Kind::Heart => "heart",
            Kind::Cloud => "cloud",
        }
    }

    /// The number a particle carries (1 to 8; 0 is the plain speck).
    pub fn code(self) -> u8 {
        Kind::ALL.iter().position(|k| *k == self).map_or(1, |i| i as u8 + 1)
    }

    pub fn from_code(code: u8) -> Option<Kind> {
        if code == 0 {
            None
        } else {
            Kind::ALL.get(code as usize - 1).copied()
        }
    }

    /// Which tile of the particle picture it is drawn from (0 to 7).
    pub fn tile(self) -> u8 {
        self.code() - 1
    }

    pub fn look(self) -> Look {
        match self {
            Kind::Spark => Look { life: (0.4, 0.9), grav: 14.0, drag: 0.5, size: (0.07, 0.03), additive: true },
            Kind::Smoke => Look { life: (1.2, 2.6), grav: -0.8, drag: 1.2, size: (0.15, 0.45), additive: false },
            Kind::Flame => Look { life: (0.4, 0.8), grav: -2.5, drag: 1.0, size: (0.14, 0.04), additive: true },
            Kind::Bubble => Look { life: (1.0, 2.0), grav: -1.8, drag: 2.0, size: (0.08, 0.1), additive: false },
            Kind::Dust => Look { life: (0.8, 1.6), grav: 1.5, drag: 1.5, size: (0.06, 0.05), additive: false },
            Kind::Magic => Look { life: (0.8, 1.6), grav: -0.4, drag: 1.5, size: (0.1, 0.02), additive: true },
            Kind::Heart => Look { life: (1.0, 1.6), grav: -1.2, drag: 1.5, size: (0.18, 0.18), additive: false },
            Kind::Cloud => Look { life: (1.8, 3.4), grav: 0.0, drag: 1.0, size: (0.35, 0.9), additive: false },
        }
    }
}

/// How opaque a particle is `age` of the way through its life (0 to 1):
/// fully until half way, then fading out.
pub fn fade(age: f32) -> f32 {
    let a = if age < 0.0 { 0.0 } else if age > 1.0 { 1.0 } else { age };
    if a < 0.5 {
        1.0
    } else {
        (1.0 - a) * 2.0
    }
}

/// What a particle looks like now: (half width in blocks, opacity 0 to 255,
/// added, tile). `None` for a plain speck (kind 0), which the renderer draws
/// the old way.
pub fn appearance(s: &Spark) -> Option<(f32, u8, bool, u8)> {
    let kind = Kind::from_code(s.kind)?;
    let look = kind.look();
    let age = if s.max_life > 0.0 { 1.0 - s.life / s.max_life } else { 1.0 };
    let age = if age < 0.0 { 0.0 } else if age > 1.0 { 1.0 } else { age };
    let size = look.size.0 + (look.size.1 - look.size.0) * age;
    Some((size, (fade(age) * 255.0) as u8, look.additive, kind.tile()))
}

/// The most scripts may add in a frame that took `dt` seconds: the slower
/// the frame, the fewer, and none when it is below 15 frames a second.
pub fn frame_cap(dt: f32) -> u32 {
    if !(dt >= 0.0) {
        0
    } else if dt <= 0.025 {
        300
    } else if dt <= 0.04 {
        150
    } else if dt <= 0.066 {
        40
    } else {
        0
    }
}

/// How many of `want` particles may be made now, with `live` in the air and
/// `spawned` already made this frame by scripts, in a frame of `dt` seconds.
pub fn allowance(live: usize, spawned: u32, want: u32, dt: f32) -> u32 {
    let frame_room = frame_cap(dt).saturating_sub(spawned);
    let air_room = MAX_LIVE.saturating_sub(live).min(u32::MAX as usize) as u32;
    want.min(frame_room).min(air_room)
}

/// A call for particles, with its numbers already checked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub kind: Kind,
    pub at: [f64; 3],
    pub count: i64,
    /// Radius of the ball they start in.
    pub spread: f64,
    /// How fast they start out, blocks a second.
    pub speed: f64,
    pub colour: [u8; 3],
}

/// Make the particles. Returns how many were made (0 when the player is
/// too far to see them, when the frame is slow, or when the air is full).
pub fn spawn(g: &mut Session, r: &Request) -> u32 {
    if !r.at.iter().all(|v| v.is_finite()) || !r.spread.is_finite() || !r.speed.is_finite() {
        return 0;
    }
    let p = g.player.pos;
    let (dx, dy, dz) = (r.at[0] - p[0], r.at[1] - p[1], r.at[2] - p[2]);
    if dx * dx + dy * dy + dz * dz > REACH * REACH {
        return 0;
    }
    let want = r.count.clamp(0, MAX_PER_CALL) as u32;
    let live = g.primed.sparks.len();
    let s = super::state(g);
    let n = allowance(live, s.spawned, want, s.frame_dt);
    s.spawned += n;
    let spread = r.spread.clamp(0.0, MAX_SPREAD);
    let speed = r.speed.clamp(0.0, MAX_SPEED) as f32;
    let look = r.kind.look();
    let mut made = 0;
    for _ in 0..n {
        let (a, e, u1, u2, v, w, c) = (g.primed.unit(), g.primed.unit(), g.primed.unit(), g.primed.unit(), g.primed.unit(), g.primed.unit(), g.primed.unit());
        // A random direction on a sphere, and another for the start place.
        let y = e * 2.0 - 1.0;
        let ring = sqrtf(1.0 - y * y);
        let dir = [cosf(a * TAU) * ring, y, sinf(a * TAU) * ring];
        let ring2 = sqrtf(1.0 - (u1 * 2.0 - 1.0) * (u1 * 2.0 - 1.0));
        let off = [cosf(u2 * TAU) * ring2, u1 * 2.0 - 1.0, sinf(u2 * TAU) * ring2];
        let reach = spread * v as f64;
        let life = look.life.0 + (look.life.1 - look.life.0) * w;
        let v0 = speed * (0.3 + 0.7 * c);
        let shade = 0.85 + 0.3 * u2;
        let tint = |x: u8| {
            let t = x as f32 * shade;
            if t > 255.0 {
                255
            } else {
                t as u8
            }
        };
        let ok = g.primed.push_particle(Spark {
            pos: [r.at[0] + off[0] as f64 * reach, r.at[1] + off[1] as f64 * reach, r.at[2] + off[2] as f64 * reach],
            vel: [dir[0] * v0, dir[1] * v0, dir[2] * v0],
            life,
            colour: [tint(r.colour[0]), tint(r.colour[1]), tint(r.colour[2])],
            kind: r.kind.code(),
            max_life: life,
            grav: look.grav,
            drag: look.drag,
        });
        if ok {
            made += 1;
        }
    }
    made
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_eight_kinds_have_names_and_codes() {
        for (i, k) in Kind::ALL.iter().enumerate() {
            assert_eq!(Kind::from_name(k.name()), Some(*k));
            assert_eq!(Kind::from_code(k.code()), Some(*k));
            assert_eq!(k.code() as usize, i + 1);
            assert_eq!(k.tile() as usize, i);
        }
        assert_eq!(Kind::from_name("SMOKE"), Some(Kind::Smoke));
        assert_eq!(Kind::from_name("fireworks"), None);
        assert_eq!(Kind::from_code(0), None);
        assert_eq!(Kind::from_code(9), None);
        for n in ["spark", "smoke", "flame", "bubble", "dust", "magic", "heart", "cloud"] {
            assert!(Kind::from_name(n).is_some(), "{}", n);
        }
    }

    #[test]
    fn every_look_is_sane() {
        for k in Kind::ALL {
            let l = k.look();
            assert!(l.life.0 > 0.0 && l.life.1 >= l.life.0 && l.life.1 < 10.0, "{:?}", k);
            assert!(l.size.0 > 0.0 && l.size.1 > 0.0 && l.size.0 < 2.0 && l.size.1 < 2.0, "{:?}", k);
            assert!(l.drag >= 0.0 && l.drag < 10.0);
        }
        // Flames and sparks glow, smoke does not.
        assert!(Kind::Flame.look().additive && Kind::Spark.look().additive && Kind::Magic.look().additive);
        assert!(!Kind::Smoke.look().additive && !Kind::Cloud.look().additive);
        // Smoke rises, a spark falls.
        assert!(Kind::Smoke.look().grav < 0.0 && Kind::Spark.look().grav > 0.0);
    }

    #[test]
    fn they_stay_solid_then_fade() {
        assert_eq!(fade(0.0), 1.0);
        assert_eq!(fade(0.5), 1.0);
        assert!((fade(0.75) - 0.5).abs() < 1e-6);
        assert_eq!(fade(1.0), 0.0);
        assert_eq!(fade(-3.0), 1.0);
        assert_eq!(fade(7.0), 0.0);
    }

    #[test]
    fn appearance_follows_the_life() {
        let mut s = Spark { pos: [0.0; 3], vel: [0.0; 3], life: 1.0, colour: [1, 2, 3], kind: Kind::Cloud.code(), max_life: 1.0, grav: 0.0, drag: 0.0 };
        let (size0, a0, add, tile) = appearance(&s).unwrap();
        assert!((size0 - 0.35).abs() < 1e-6 && a0 == 255 && !add && tile == 7);
        s.life = 0.0;
        let (size1, a1, _, _) = appearance(&s).unwrap();
        assert!((size1 - 0.9).abs() < 1e-6 && a1 == 0);
        // A life of nothing or less than nothing does not divide by zero.
        s.max_life = 0.0;
        assert!(appearance(&s).is_some());
        s.life = -5.0;
        s.max_life = 1.0;
        assert_eq!(appearance(&s).unwrap().1, 0);
        // The plain speck is the old kind.
        s.kind = 0;
        assert!(appearance(&s).is_none());
    }

    #[test]
    fn a_slow_frame_makes_fewer() {
        assert_eq!(frame_cap(0.016), 300);
        assert_eq!(frame_cap(0.033), 150);
        assert_eq!(frame_cap(0.05), 40);
        assert_eq!(frame_cap(0.1), 0);
        assert_eq!(frame_cap(f32::NAN), 0);
        assert_eq!(frame_cap(-1.0), 0);
        assert!(frame_cap(0.02) >= frame_cap(0.03));
        assert!(frame_cap(0.03) >= frame_cap(0.05));
        assert!(frame_cap(0.05) >= frame_cap(0.07));
    }

    #[test]
    fn the_allowance_respects_the_air_and_the_frame() {
        // Plenty of room: all of them.
        assert_eq!(allowance(0, 0, 10, 0.016), 10);
        // The frame's share is used up.
        assert_eq!(allowance(0, 295, 10, 0.016), 5);
        assert_eq!(allowance(0, 300, 10, 0.016), 0);
        assert_eq!(allowance(0, 9999, 10, 0.016), 0);
        // The air is full.
        assert_eq!(allowance(MAX_LIVE, 0, 10, 0.016), 0);
        assert_eq!(allowance(MAX_LIVE - 3, 0, 10, 0.016), 3);
        assert_eq!(allowance(MAX_LIVE + 50, 0, 10, 0.016), 0);
        // A slow frame drops them all.
        assert_eq!(allowance(0, 0, 10, 0.2), 0);
        // Nothing asked for.
        assert_eq!(allowance(0, 0, 0, 0.016), 0);
    }

    #[test]
    fn the_cap_is_two_thousand() {
        assert_eq!(MAX_LIVE, 2000);
        assert!(MAX_PER_CALL <= 64);
    }
}
