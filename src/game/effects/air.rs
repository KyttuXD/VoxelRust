//! Drowning: the air meter of a body whose head is under water.
//!
//! The air starts full (10 bubbles). While the head is under water, one
//! bubble is lost every second; when none is left, damage is taken every
//! second until the head is out again. On surfacing the air comes back four
//! times as fast as it went. A body that can breathe (the water breathing
//! effect, or any effect that says `breath`) loses nothing. All of it is
//! arithmetic on [`Air`], so it is tested without a world; the session and
//! the creatures call [`step`].

/// A full meter, in bubbles.
pub const MAX_AIR: f32 = 10.0;
/// Bubbles lost a second under water.
pub const DRAIN: f32 = 1.0;
/// Bubbles gained a second in the open air.
pub const REFILL: f32 = 4.0;
/// Seconds between blows once the air has run out.
pub const DROWN_EVERY: f32 = 1.0;
/// Health points each blow takes.
pub const DROWN_DAMAGE: i32 = 2;
/// Most blows worked out in one step (a long pause does not kill at once).
const MAX_BLOWS: i32 = 3;

/// The air of one body: what is left, and the time since the last blow
/// while it is out.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Air {
    pub air: f32,
    pub drown: f32,
}

impl Air {
    pub const FULL: Air = Air { air: MAX_AIR, drown: 0.0 };
}

impl Default for Air {
    fn default() -> Air {
        Air::FULL
    }
}

/// Let `dt` seconds pass. `under` is whether the head is in water,
/// `breathing` whether the body can breathe there anyway. Returns the new
/// state and the health points of damage that came due.
pub fn step(a: Air, dt: f32, under: bool, breathing: bool) -> (Air, i32) {
    if !(dt > 0.0) {
        return (a, 0);
    }
    if !under || breathing {
        return (Air { air: (a.air + dt * REFILL).min(MAX_AIR), drown: 0.0 }, 0);
    }
    let mut air = a.air.clamp(0.0, MAX_AIR);
    // The part of this step that was spent with no air left.
    let dry = if air > 0.0 {
        let left = air / DRAIN;
        if dt <= left {
            air -= dt * DRAIN;
            0.0
        } else {
            air = 0.0;
            dt - left
        }
    } else {
        dt
    };
    let mut drown = a.drown.max(0.0) + dry;
    let mut blows = 0;
    while drown >= DROWN_EVERY && blows < MAX_BLOWS {
        drown -= DROWN_EVERY;
        blows += 1;
    }
    if drown >= DROWN_EVERY {
        drown = 0.0;
    }
    (Air { air, drown }, blows * DROWN_DAMAGE)
}

/// How many bubbles to draw for this much air: none when it is full (the
/// meter is only shown when something is missing), a bubble for each whole
/// or started unit otherwise.
pub fn bubbles(air: f32) -> i32 {
    if air >= MAX_AIR - 0.1 {
        return 0;
    }
    ((air + 0.99) as i32).clamp(0, MAX_AIR as i32)
}

/// Whether the meter is shown at all (something is missing).
pub fn shown(air: f32) -> bool {
    air < MAX_AIR - 0.1
}

/// Seconds the air lasts under water from now, before the first blow.
pub fn seconds_left(a: Air) -> f32 {
    a.air.max(0.0) / DRAIN + (DROWN_EVERY - a.drown).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `secs` seconds in frames of `dt`; the state and the damage.
    fn run(mut a: Air, secs: f32, dt: f32, under: bool, breathing: bool) -> (Air, i32) {
        let (mut hurt, n) = (0, (secs / dt) as i32);
        for _ in 0..n {
            let (b, d) = step(a, dt, under, breathing);
            a = b;
            hurt += d;
        }
        (a, hurt)
    }

    #[test]
    fn a_bubble_goes_every_second_and_nothing_is_taken_until_the_air_is_out() {
        let (a, hurt) = run(Air::FULL, 5.0, 1.0 / 60.0, true, false);
        assert!((a.air - 5.0).abs() < 0.05, "{}", a.air);
        assert_eq!(hurt, 0);
        // Ten seconds empties it; the first blow is a second after that.
        let (a, hurt) = run(Air::FULL, 10.9, 1.0 / 60.0, true, false);
        assert_eq!(a.air, 0.0);
        assert_eq!(hurt, 0);
    }

    #[test]
    fn once_the_air_is_out_a_blow_comes_every_second() {
        let (_, hurt) = run(Air::FULL, 14.5, 1.0 / 60.0, true, false);
        assert_eq!(hurt, 4 * DROWN_DAMAGE, "blows at 11, 12, 13 and 14 seconds");
        // The same in coarse steps: the time is the same.
        let (_, hurt) = run(Air::FULL, 14.5, 0.25, true, false);
        assert_eq!(hurt, 4 * DROWN_DAMAGE);
        // From an empty meter: the first blow is one second on.
        let (_, hurt) = run(Air { air: 0.0, drown: 0.0 }, 0.9, 0.1, true, false);
        assert_eq!(hurt, 0);
        let (_, hurt) = run(Air { air: 0.0, drown: 0.0 }, 1.25, 0.1, true, false);
        assert_eq!(hurt, DROWN_DAMAGE);
    }

    #[test]
    fn the_time_that_ran_the_air_out_does_not_count_twice() {
        // One big step of 10.5 seconds: the air goes at 10, half a second of drowning is owed.
        let (a, hurt) = step(Air::FULL, 10.5, true, false);
        assert_eq!((a.air, hurt), (0.0, 0));
        assert!((a.drown - 0.5).abs() < 1e-4);
        let (_, hurt) = step(a, 0.6, true, false);
        assert_eq!(hurt, DROWN_DAMAGE);
    }

    #[test]
    fn a_long_pause_cannot_kill_at_once() {
        let (a, hurt) = step(Air { air: 0.0, drown: 0.0 }, 100.0, true, false);
        assert_eq!(hurt, 3 * DROWN_DAMAGE);
        assert!(a.drown < DROWN_EVERY);
    }

    #[test]
    fn surfacing_refills_fast_and_stops_the_drowning() {
        let a = Air { air: 0.0, drown: 0.8 };
        let (b, hurt) = step(a, 0.5, false, false);
        assert_eq!(hurt, 0);
        assert!((b.air - 2.0).abs() < 1e-5);
        assert_eq!(b.drown, 0.0);
        // Empty to full in two and a half seconds, never over.
        let (b, _) = run(Air { air: 0.0, drown: 0.0 }, 2.6, 1.0 / 60.0, false, false);
        assert_eq!(b.air, MAX_AIR);
        let (b, _) = step(Air::FULL, 5.0, false, false);
        assert_eq!(b.air, MAX_AIR);
    }

    #[test]
    fn breathing_under_water_loses_nothing_and_refills() {
        let (a, hurt) = run(Air::FULL, 30.0, 0.1, true, true);
        assert_eq!((a.air, hurt), (MAX_AIR, 0));
        let (a, hurt) = run(Air { air: 3.0, drown: 0.5 }, 2.0, 0.1, true, true);
        assert_eq!((a.air, a.drown, hurt), (MAX_AIR, 0.0, 0));
    }

    #[test]
    fn no_time_or_rubbish_time_changes_nothing() {
        let a = Air { air: 4.0, drown: 0.3 };
        assert_eq!(step(a, 0.0, true, false), (a, 0));
        assert_eq!(step(a, -1.0, true, false), (a, 0));
        assert_eq!(step(a, f32::NAN, true, false), (a, 0));
    }

    #[test]
    fn the_meter_shows_when_something_is_missing() {
        assert!(!shown(MAX_AIR));
        assert!(shown(9.5));
        assert_eq!(bubbles(MAX_AIR), 0);
        assert_eq!(bubbles(9.5), 10);
        assert_eq!(bubbles(9.0), 9);
        assert_eq!(bubbles(0.4), 1);
        assert_eq!(bubbles(0.0), 0);
        assert_eq!(bubbles(-3.0), 0);
        assert_eq!(seconds_left(Air::FULL), 11.0);
        assert_eq!(seconds_left(Air { air: 0.0, drown: 0.25 }), 0.75);
    }
}
