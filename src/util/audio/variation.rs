//! Keeping sounds that repeat a lot from sounding the same: a different
//! variant each time, a little pitch and volume jitter, and limits so a fast
//! action cannot stack into a wall of sound.

use super::rand::Rng;
use alloc::vec::Vec;

/// Pitch varies by this much either way (6 percent).
pub const PITCH_SPREAD: f32 = 0.06;
/// Volume varies by this many decibels either way.
pub const GAIN_DB: f32 = 1.5;

/// Decibels as a factor (accurate to a percent within about 3 dB).
pub fn db_to_gain(db: f32) -> f32 {
    let x = db * 0.115_129;
    1.0 + x + x * x * 0.5 + x * x * x * (1.0 / 6.0)
}

/// What to change about one playing of a sound.
#[derive(Clone, Copy, Debug)]
pub struct Jitter {
    /// Playback speed (1 = as is).
    pub pitch: f32,
    /// Volume factor (1 = as is).
    pub gain: f32,
}

/// A fresh jitter: pitch within `spread` either way, volume within `db`.
pub fn jitter(rng: &mut Rng, spread: f32, db: f32) -> Jitter {
    Jitter { pitch: 1.0 + rng.signed() * spread, gain: db_to_gain(rng.signed() * db) }
}

/// Picks one of `n` things, never the one picked last time.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRepeat {
    last: Option<usize>,
}

impl NoRepeat {
    pub fn new() -> NoRepeat {
        NoRepeat { last: None }
    }

    /// An index below `n`; different from the last one whenever `n > 1`.
    pub fn pick(&mut self, n: usize, rng: &mut Rng) -> usize {
        if n <= 1 {
            self.last = Some(0);
            return 0;
        }
        // One of the other n - 1 places, then step over the last pick.
        let mut i = rng.below(n as u32 - 1) as usize;
        if let Some(l) = self.last {
            if l < n && i >= l {
                i += 1;
            }
        }
        self.last = Some(i);
        i
    }
}

/// How often one sound may start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Policy {
    /// Seconds that must pass before it may start again.
    pub min_gap: f32,
    /// Most that may play at once.
    pub max_voices: usize,
}

/// The limits for a sound of the volume setting `bus` (0 effects, 1 footsteps,
/// 2 interface) that lasts `seconds`.
pub fn policy_for(bus: u8, seconds: f32) -> Policy {
    match bus {
        1 => Policy { min_gap: 0.14, max_voices: 2 },
        2 => Policy { min_gap: 0.03, max_voices: 3 },
        _ => {
            let gap = (seconds * 0.2).clamp(0.05, 0.35);
            Policy { min_gap: gap, max_voices: 3 }
        }
    }
}

/// A number for a sound's name (FNV-1a), to find it quickly.
pub fn name_key(name: &str) -> u32 {
    let mut h = 0x811C_9DC5u32;
    for b in name.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// Whether the sound file `entry` is `name` itself or another take of it
/// (`name_v2`, `name_v3`...): a pack gives a sound several files to vary it.
pub fn is_variant_of(entry: &str, name: &str) -> bool {
    if entry == name {
        return true;
    }
    match entry.strip_prefix(name).and_then(|r| r.strip_prefix("_v")) {
        Some(digits) => !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()),
        None => false,
    }
}

const ENDS: usize = 6;
const KEYS: usize = 64;

#[derive(Clone, Copy)]
struct Entry {
    key: u32,
    last: f32,
    ends: [f32; ENDS],
}

/// Remembers when each sound started and when it ends, and says whether
/// another may start.
#[derive(Default)]
pub struct RateLimiter {
    entries: Vec<Entry>,
}

impl RateLimiter {
    pub fn new() -> RateLimiter {
        RateLimiter { entries: Vec::new() }
    }

    /// Whether the sound `key` may start at `now` (seconds, only ever
    /// growing) and last `duration`. If so, it is counted as started.
    pub fn allow(&mut self, key: u32, now: f32, duration: f32, policy: Policy) -> bool {
        let at = match self.entries.iter().position(|e| e.key == key) {
            Some(i) => i,
            None => {
                if self.entries.len() >= KEYS {
                    // Forget the one that started longest ago.
                    let oldest = (0..self.entries.len()).min_by(|&a, &b| self.entries[a].last.total_cmp(&self.entries[b].last)).unwrap_or(0);
                    self.entries.swap_remove(oldest);
                }
                self.entries.push(Entry { key, last: -1.0e9, ends: [-1.0e9; ENDS] });
                self.entries.len() - 1
            }
        };
        let e = &mut self.entries[at];
        if now < e.last {
            // The clock went back (a new world): start afresh.
            e.last = -1.0e9;
            e.ends = [-1.0e9; ENDS];
        }
        if now - e.last < policy.min_gap {
            return false;
        }
        let playing = e.ends.iter().filter(|&&t| t > now).count();
        if playing >= policy.max_voices.min(ENDS) {
            return false;
        }
        // Take the slot that is free (or ends soonest).
        let mut slot = 0;
        for i in 1..ENDS {
            if e.ends[i] < e.ends[slot] {
                slot = i;
            }
        }
        e.ends[slot] = now + duration.max(0.0);
        e.last = now;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chooser_never_repeats_and_uses_every_place() {
        let mut rng = Rng::new(5);
        for n in 2..8usize {
            let mut c = NoRepeat::new();
            let mut last = usize::MAX;
            let mut seen = alloc::vec![false; n];
            for _ in 0..500 {
                let i = c.pick(n, &mut rng);
                assert!(i < n);
                assert_ne!(i, last, "repeated with n = {n}");
                last = i;
                seen[i] = true;
            }
            assert!(seen.iter().all(|&s| s), "n = {n}: {seen:?}");
        }
        let mut c = NoRepeat::new();
        assert_eq!(c.pick(1, &mut rng), 0);
        assert_eq!(c.pick(0, &mut rng), 0);
    }

    #[test]
    fn the_chooser_copes_with_fewer_places_than_before() {
        let mut rng = Rng::new(9);
        let mut c = NoRepeat::new();
        for _ in 0..20 {
            c.pick(6, &mut rng);
        }
        for _ in 0..50 {
            assert!(c.pick(2, &mut rng) < 2);
        }
    }

    #[test]
    fn jitter_stays_within_six_percent_and_one_and_a_half_decibels() {
        let mut rng = Rng::new(11);
        let (mut lo, mut hi) = (10.0f32, 0.0f32);
        for _ in 0..5000 {
            let j = jitter(&mut rng, PITCH_SPREAD, GAIN_DB);
            assert!(j.pitch >= 0.94 - 1e-4 && j.pitch <= 1.06 + 1e-4);
            assert!(j.gain >= 0.84 && j.gain <= 1.19, "{}", j.gain);
            lo = lo.min(j.pitch);
            hi = hi.max(j.pitch);
        }
        assert!(lo < 0.95 && hi > 1.05, "the jitter does not use its range");
        assert!((db_to_gain(0.0) - 1.0).abs() < 1e-6);
        assert!((db_to_gain(1.5) - 1.188).abs() < 0.01);
    }

    #[test]
    fn a_minimum_gap_is_kept() {
        let mut l = RateLimiter::new();
        let p = Policy { min_gap: 0.1, max_voices: 3 };
        assert!(l.allow(1, 1.0, 0.05, p));
        assert!(!l.allow(1, 1.05, 0.05, p));
        assert!(l.allow(1, 1.11, 0.05, p));
        // Another sound has its own limit.
        assert!(l.allow(2, 1.11, 0.05, p));
    }

    #[test]
    fn the_voices_of_one_sound_are_limited_until_they_end() {
        let mut l = RateLimiter::new();
        let p = Policy { min_gap: 0.0, max_voices: 3 };
        for i in 0..3 {
            assert!(l.allow(7, 10.0 + i as f32 * 0.01, 1.0, p));
        }
        assert!(!l.allow(7, 10.5, 1.0, p), "a fourth at once");
        assert!(l.allow(7, 11.015, 1.0, p), "after the first ended");
    }

    #[test]
    fn the_limiter_starts_over_when_the_clock_goes_back_and_forgets_old_names() {
        let mut l = RateLimiter::new();
        let p = Policy { min_gap: 1.0, max_voices: 1 };
        assert!(l.allow(1, 100.0, 5.0, p));
        assert!(l.allow(1, 0.5, 5.0, p));
        for k in 0..200u32 {
            l.allow(1000 + k, k as f32, 0.1, p);
        }
        assert!(l.entries.len() <= KEYS);
    }

    #[test]
    fn policies_are_gentle_on_footsteps_and_clicks() {
        assert!(policy_for(1, 0.09).min_gap > policy_for(2, 0.05).min_gap);
        assert_eq!(policy_for(0, 10.0).min_gap, 0.35);
        assert_eq!(policy_for(0, 0.01).min_gap, 0.05);
        assert!(policy_for(0, 1.0).max_voices >= 2);
    }

    #[test]
    fn variant_names_are_recognised() {
        assert!(is_variant_of("step", "step"));
        assert!(is_variant_of("step_v2", "step"));
        assert!(is_variant_of("step_v12", "step"));
        assert!(!is_variant_of("step_v", "step"));
        assert!(!is_variant_of("step_vx", "step"));
        assert!(!is_variant_of("stepper", "step"));
        assert!(!is_variant_of("step2", "step"));
        assert_ne!(name_key("step"), name_key("break"));
    }
}
