//! Soft particles (`spawn_soft`): one round, blurred speck at a time, laid
//! over the view or added to it, with a opacity of its own that changes over
//! its life and a size that grows or shrinks. For darkness, mist, ash, glows.
//!
//! They are not the sparks of `particles.rs` (flat little pictures of fixed
//! kinds): each one carries its own numbers, moves in a straight line, and is
//! drawn as a soft disc (`client/render/soft_fx.rs`). The budget: [`MAX_LIVE`]
//! alive at once, [`MAX_PER_MOD`] for one mod, and the same slow-frame rule as the
//! other particles (`particles::frame_cap`); a speck more than
//! [`particles::REACH`] blocks from the player is not made. Every number is
//! bounded when it is read, so a script cannot make a huge or endless one.

use super::particles;
use crate::session::Session;
use alloc::string::String;
use alloc::vec::Vec;

/// Most alive at once, all mods together, and for one mod.
pub const MAX_LIVE: usize = 512;
pub const MAX_PER_MOD: usize = 256;
/// Longest life, seconds.
pub const MAX_LIFE: f32 = 10.0;
/// Largest half width, blocks.
pub const MAX_SIZE: f32 = 8.0;
/// Fastest, blocks a second.
pub const MAX_SPEED: f32 = 40.0;
/// Farthest a speck is drawn, blocks.
pub const DRAW_RANGE: f32 = 96.0;

/// A call for a soft particle, as the script gave it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub at: [f64; 3],
    pub vel: [f64; 3],
    pub life: f64,
    /// Half width when it appears and when it is gone, blocks.
    pub size: (f64, f64),
    pub rgb: [u8; 3],
    /// Opacity when it appears and when it is gone, 0 to 255.
    pub alpha: (i64, i64),
    pub additive: bool,
}

/// A soft particle alive.
#[derive(Clone, Debug, PartialEq)]
pub struct Soft {
    pub owner: String,
    pub pos: [f64; 3],
    pub vel: [f32; 3],
    pub life: f32,
    pub max_life: f32,
    pub size: (f32, f32),
    pub rgb: [u8; 3],
    pub alpha: (u8, u8),
    pub additive: bool,
}

impl Soft {
    /// Half its width in blocks, and its opacity (0 to 255), now.
    pub fn look(&self) -> (f32, u8) {
        let age = if self.max_life > 0.0 { (1.0 - self.life / self.max_life).clamp(0.0, 1.0) } else { 1.0 };
        let size = self.size.0 + (self.size.1 - self.size.0) * age;
        let a = self.alpha.0 as f32 + (self.alpha.1 as f32 - self.alpha.0 as f32) * age;
        (size, a.clamp(0.0, 255.0) as u8)
    }
}

#[derive(Default)]
pub struct SoftSet {
    pub list: Vec<Soft>,
}

/// Why a speck was not made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// A number was not finite, or the life is not above nothing.
    Bad,
    ModFull,
    AllFull,
}

impl SoftSet {
    pub fn count_of(&self, owner: &str) -> usize {
        self.list.iter().filter(|p| p.owner == owner).count()
    }

    /// Add a speck for `owner`, its numbers brought inside the limits.
    pub fn add(&mut self, owner: &str, r: &Request) -> Result<(), Refused> {
        let finite3 = |v: &[f64; 3]| v.iter().all(|x| x.is_finite());
        if !finite3(&r.at) || !finite3(&r.vel) || !r.life.is_finite() || !r.size.0.is_finite() || !r.size.1.is_finite() || r.life <= 0.0 {
            return Err(Refused::Bad);
        }
        if self.list.len() >= MAX_LIVE {
            return Err(Refused::AllFull);
        }
        if self.count_of(owner) >= MAX_PER_MOD {
            return Err(Refused::ModFull);
        }
        let life = (r.life as f32).clamp(0.05, MAX_LIFE);
        let v = |x: f64| (x as f32).clamp(-MAX_SPEED, MAX_SPEED);
        let sz = |x: f64| (x as f32).clamp(0.0, MAX_SIZE);
        let al = |x: i64| x.clamp(0, 255) as u8;
        self.list.push(Soft {
            owner: String::from(owner),
            pos: r.at,
            vel: [v(r.vel[0]), v(r.vel[1]), v(r.vel[2])],
            life,
            max_life: life,
            size: (sz(r.size.0), sz(r.size.1)),
            rgb: r.rgb,
            alpha: (al(r.alpha.0), al(r.alpha.1)),
            additive: r.additive,
        });
        Ok(())
    }
}

/// Move the specks on and let the old ones go.
pub fn tick(set: &mut SoftSet, dt: f32) {
    if set.list.is_empty() || !dt.is_finite() || dt <= 0.0 {
        return;
    }
    // (a long stall ages them but does not fling them far)
    let (age, run) = (dt.min(12.0), dt.min(0.25));
    for p in set.list.iter_mut() {
        p.life -= age;
        for a in 0..3 {
            p.pos[a] += (p.vel[a] * run) as f64;
        }
    }
    set.list.retain(|p| p.life > 0.0);
}

/// Make a speck for a mod: refused (`false`) when the player is too far to see
/// it, the frame is slow, or a cap is used up; `Err` for numbers that are not finite.
pub fn spawn(g: &mut Session, owner: &str, r: &Request) -> Result<bool, Refused> {
    if r.at.iter().any(|v| !v.is_finite()) {
        return Err(Refused::Bad);
    }
    let p = g.player.pos;
    let (dx, dy, dz) = (r.at[0] - p[0], r.at[1] - p[1], r.at[2] - p[2]);
    if dx * dx + dy * dy + dz * dz > particles::REACH * particles::REACH {
        return Ok(false);
    }
    let s = super::state(g);
    // The frame's share of new particles is the one the other particles use.
    if particles::allowance(0, s.spawned, 1, s.frame_dt) == 0 {
        return Ok(false);
    }
    match s.soft.add(owner, r) {
        Ok(()) => {
            s.spawned += 1;
            Ok(true)
        }
        Err(Refused::Bad) => Err(Refused::Bad),
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req() -> Request {
        Request { at: [0.0, 64.0, 0.0], vel: [0.0, 1.0, 0.0], life: 2.0, size: (0.2, 1.0), rgb: [10, 10, 10], alpha: (200, 0), additive: false }
    }

    #[test]
    fn a_speck_grows_and_fades_over_its_life() {
        let mut set = SoftSet::default();
        set.add("m", &req()).unwrap();
        let p = set.list[0].clone();
        assert_eq!(p.look(), (0.2, 200));
        let mut half = p.clone();
        half.life = 1.0;
        let (size, a) = half.look();
        assert!((size - 0.6).abs() < 1e-5 && a == 100, "{size} {a}");
        let mut gone = p;
        gone.life = 0.0;
        assert_eq!(gone.look(), (1.0, 0));
        gone.life = -3.0;
        assert_eq!(gone.look(), (1.0, 0));
        // A life of nothing does not divide by it.
        gone.max_life = 0.0;
        assert_eq!(gone.look().1, 0);
    }

    #[test]
    fn they_move_and_expire() {
        let mut set = SoftSet::default();
        set.add("m", &req()).unwrap();
        tick(&mut set, 0.25);
        tick(&mut set, 0.25);
        assert!((set.list[0].pos[1] - 64.5).abs() < 1e-6);
        // A long stall ages it by the whole time but moves it by a quarter second at most.
        tick(&mut set, 1.0);
        assert_eq!(set.list.len(), 1);
        assert!((set.list[0].pos[1] - 64.75).abs() < 1e-6);
        tick(&mut set, 0.6);
        assert!(set.list.is_empty());
        // Odd times change nothing.
        set.add("m", &req()).unwrap();
        let before = set.list[0].clone();
        tick(&mut set, f32::NAN);
        tick(&mut set, -1.0);
        tick(&mut set, 0.0);
        assert_eq!(set.list[0], before);
    }

    #[test]
    fn numbers_are_brought_inside_the_limits() {
        let mut set = SoftSet::default();
        let wild = Request { vel: [1.0e9, -1.0e9, 3.0], life: 1.0e9, size: (-5.0, 1.0e9), alpha: (-40, 9999), ..req() };
        set.add("m", &wild).unwrap();
        let p = &set.list[0];
        assert_eq!((p.vel[0], p.vel[1], p.vel[2]), (MAX_SPEED, -MAX_SPEED, 3.0));
        assert_eq!(p.life, MAX_LIFE);
        assert_eq!(p.size, (0.0, MAX_SIZE));
        assert_eq!(p.alpha, (0, 255));
        for bad in [Request { life: 0.0, ..req() }, Request { life: -1.0, ..req() }, Request { life: f64::NAN, ..req() }, Request { at: [f64::INFINITY, 0.0, 0.0], ..req() }, Request { vel: [0.0, f64::NAN, 0.0], ..req() }, Request { size: (f64::NAN, 1.0), ..req() }] {
            assert_eq!(set.add("m", &bad), Err(Refused::Bad));
        }
        assert_eq!(set.list.len(), 1);
        // A very short life is held to a twentieth of a second at least.
        set.add("m", &Request { life: 0.0001, ..req() }).unwrap();
        assert_eq!(set.list[1].life, 0.05);
    }

    #[test]
    fn a_mod_and_all_the_mods_have_a_share() {
        let mut set = SoftSet::default();
        for _ in 0..MAX_PER_MOD {
            set.add("a", &req()).unwrap();
        }
        assert_eq!(set.add("a", &req()), Err(Refused::ModFull));
        assert_eq!(set.count_of("a"), MAX_PER_MOD);
        // Another mod still has its own share, up to the total.
        for _ in 0..(MAX_LIVE - MAX_PER_MOD) {
            set.add("b", &req()).unwrap();
        }
        assert_eq!(set.list.len(), MAX_LIVE);
        assert_eq!(set.add("c", &req()), Err(Refused::AllFull));
        assert_eq!(set.add("b", &req()), Err(Refused::AllFull));
        // Room again when they have gone.
        tick(&mut set, 5.0);
        assert!(set.list.is_empty());
        assert!(set.add("c", &req()).is_ok());
    }
}
