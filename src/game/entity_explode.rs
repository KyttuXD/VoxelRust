//! The fuse of a creature that blows up (`explodes <power> <fuse seconds>
//! <trigger range>` in the data). All the rules are here as plain functions
//! of numbers, so they can be tested; `entity.rs` calls them for each such
//! creature, every step, and carries out what they say.
//!
//! A creature that has a player within its trigger range lights its fuse:
//! it stops, hisses, swells and flashes. If the player stays near for the
//! whole fuse it explodes. If the player gets away (further than
//! [`HOLD`] times the range) the fuse is put out and starts over.

use crate::math::floorf;

/// Once lit, the fuse keeps burning until the player is this many times the
/// trigger range away, so stepping back a little does not save you.
pub const HOLD: f32 = 1.6;

/// How much bigger it is at the end of the fuse.
pub const SWELL: f32 = 0.35;

/// What a step of the fuse comes to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    Nothing,
    /// The fuse was just lit (time for the hiss).
    Lit,
    /// The fuse has burnt down: explode now.
    Boom,
}

/// One step. `fuse` is the seconds burnt so far (0 = not lit), `dist` how far
/// the player is, `armed` whether the creature may light up at all (not
/// tamed, not young, a player in survival mode to fear it). Returns the new
/// fuse and what happened.
pub fn step(fuse: f32, dt: f32, dist: f32, range: f32, fuse_len: f32, armed: bool) -> (f32, Event) {
    // Written so that a NaN anywhere puts the fuse out.
    if !armed || !(dist >= 0.0) || !(fuse >= 0.0) {
        return (0.0, Event::Nothing);
    }
    let lit = fuse > 0.0;
    let limit = if lit { range * HOLD } else { range };
    if dist > limit {
        return (0.0, Event::Nothing);
    }
    let next = fuse + if dt > 0.0 { dt } else { 0.0 };
    if next >= fuse_len {
        (fuse_len, Event::Boom)
    } else if !lit && next > 0.0 {
        (next, Event::Lit)
    } else {
        (next, Event::Nothing)
    }
}

/// How far along the fuse is, 0..=1.
pub fn progress(fuse: f32, fuse_len: f32) -> f32 {
    if fuse_len <= 0.0 || !(fuse > 0.0) {
        0.0
    } else if fuse >= fuse_len {
        1.0
    } else {
        fuse / fuse_len
    }
}

/// The factor its size is multiplied by while the fuse burns (1 at rest).
pub fn swell(fuse: f32, fuse_len: f32) -> f32 {
    1.0 + SWELL * progress(fuse, fuse_len)
}

/// Whether it is in the bright half of its flashing: it flashes faster and
/// faster, from about three times a second to about ten.
pub fn flashing(fuse: f32, fuse_len: f32) -> bool {
    if !(fuse > 0.0) {
        return false;
    }
    let beats = floorf(fuse * (6.0 + 14.0 * progress(fuse, fuse_len))) as i32;
    beats % 2 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run the fuse for `seconds` with the player always `dist` away.
    fn burn(mut fuse: f32, dist: f32, seconds: f32) -> (f32, Option<f32>) {
        let mut t = 0.0;
        while t < seconds {
            let (f, ev) = step(fuse, 1.0 / 30.0, dist, 3.0, 1.5, true);
            fuse = f;
            t += 1.0 / 30.0;
            if ev == Event::Boom {
                return (fuse, Some(t));
            }
        }
        (fuse, None)
    }

    #[test]
    fn a_player_inside_the_range_for_the_whole_fuse_sets_it_off() {
        let (_, at) = burn(0.0, 2.0, 5.0);
        let at = at.expect("never exploded");
        assert!(at > 1.4 && at < 1.7, "exploded at {at}");
    }

    #[test]
    fn it_does_not_light_from_outside_the_range() {
        assert_eq!(burn(0.0, 3.5, 5.0), (0.0, None));
        assert_eq!(step(0.0, 0.03, 3.01, 3.0, 1.5, true), (0.0, Event::Nothing));
    }

    #[test]
    fn lighting_is_announced_once() {
        let (f, ev) = step(0.0, 0.1, 2.0, 3.0, 1.5, true);
        assert_eq!((f, ev), (0.1, Event::Lit));
        let (_, ev) = step(f, 0.1, 2.0, 3.0, 1.5, true);
        assert_eq!(ev, Event::Nothing);
    }

    #[test]
    fn moving_away_puts_the_fuse_out() {
        let (f, ev) = step(0.0, 0.5, 2.0, 3.0, 1.5, true);
        assert!(f > 0.0 && ev == Event::Lit);
        // A step back, still within reach: it goes on burning.
        let (f2, _) = step(f, 0.5, 4.0, 3.0, 1.5, true);
        assert!(f2 > f);
        // Well away: out, and starting again takes the whole fuse.
        assert_eq!(step(f2, 0.5, 6.0, 3.0, 1.5, true), (0.0, Event::Nothing));
        let (_, at) = burn(0.0, 2.0, 5.0);
        assert!(at.unwrap() > 1.4);
    }

    #[test]
    fn it_stays_quiet_when_not_armed_or_when_numbers_are_odd() {
        assert_eq!(step(1.0, 0.1, 1.0, 3.0, 1.5, false), (0.0, Event::Nothing));
        assert_eq!(step(0.5, 0.1, f32::NAN, 3.0, 1.5, true), (0.0, Event::Nothing));
        assert_eq!(step(f32::NAN, 0.1, 1.0, 3.0, 1.5, true), (0.0, Event::Nothing));
        // A zero or negative step does not light it.
        assert_eq!(step(0.0, 0.0, 1.0, 3.0, 1.5, true), (0.0, Event::Nothing));
    }

    #[test]
    fn it_swells_and_flashes_faster_as_the_fuse_burns_down() {
        assert_eq!(swell(0.0, 1.5), 1.0);
        assert!((swell(0.75, 1.5) - (1.0 + SWELL * 0.5)).abs() < 1e-5);
        assert!((swell(9.0, 1.5) - (1.0 + SWELL)).abs() < 1e-5);
        assert_eq!(progress(1.0, 0.0), 0.0);
        assert!(!flashing(0.0, 1.5));
        let count = |from: f32, to: f32| {
            let (mut n, mut was, mut t) = (0, false, from);
            while t < to {
                let on = flashing(t, 1.5);
                if on && !was {
                    n += 1;
                }
                was = on;
                t += 0.005;
            }
            n
        };
        let (a, b) = (count(0.01, 0.75), count(0.75, 1.5));
        assert!(a >= 1 && b > a, "{a} flashes, then {b}");
    }
}
