//! Continuous sound beds: rain, wind, water, lava, fire and the quiet of a
//! cave. Each is granular noise ([`super::grains`]) shaped by two filters:
//! a soft low-pass that two one-pole stages make gentle (no hiss), and a
//! second low-pass for body that is mixed in beside it (a low shelf).
//! The game moves two targets on a bed, how loud it is and how bright, and the
//! bed glides to them; the slow breathing and fading are done by
//! [`super::ambience`].
//!
//! A [`Bed`] holds no heap memory, so it lives in the mixer and runs in the
//! audio callback.

use super::grains::Granular;
use super::RATE;
use crate::math::{clampf, TAU};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BedKind {
    Rain,
    Wind,
    Water,
    Lava,
    Fire,
    Cave,
}

/// How many beds the mixer has.
pub const BEDS: usize = 6;

impl BedKind {
    pub const ALL: [BedKind; BEDS] = [BedKind::Rain, BedKind::Wind, BedKind::Water, BedKind::Lava, BedKind::Fire, BedKind::Cave];

    pub fn index(self) -> usize {
        match self {
            BedKind::Rain => 0,
            BedKind::Wind => 1,
            BedKind::Water => 2,
            BedKind::Lava => 3,
            BedKind::Fire => 4,
            BedKind::Cave => 5,
        }
    }

    pub fn recipe(self) -> Recipe {
        match self {
            // A soft noise bed: top rolled off at 6 kHz, body from 350 Hz.
            BedKind::Rain => Recipe { grain_ms: (300, 800), top_hz: 6000.0, body_hz: 350.0, body_mix: 0.55, scale: 0.26 },
            BedKind::Wind => Recipe { grain_ms: (700, 1800), top_hz: 650.0, body_hz: 140.0, body_mix: 0.6, scale: 0.34 },
            BedKind::Water => Recipe { grain_ms: (350, 1000), top_hz: 1900.0, body_hz: 260.0, body_mix: 0.45, scale: 0.26 },
            BedKind::Lava => Recipe { grain_ms: (600, 1400), top_hz: 300.0, body_hz: 70.0, body_mix: 0.9, scale: 0.7 },
            BedKind::Fire => Recipe { grain_ms: (250, 700), top_hz: 2200.0, body_hz: 420.0, body_mix: 0.25, scale: 0.12 },
            BedKind::Cave => Recipe { grain_ms: (900, 2200), top_hz: 230.0, body_hz: 60.0, body_mix: 0.8, scale: 0.6 },
        }
    }
}

/// What a bed is made of.
#[derive(Clone, Copy, Debug)]
pub struct Recipe {
    /// Shortest and longest grain, in milliseconds.
    pub grain_ms: (u32, u32),
    /// Where the soft low-pass starts to roll the top off, when open.
    pub top_hz: f32,
    /// The body filter's corner, and how much of it is added.
    pub body_hz: f32,
    pub body_mix: f32,
    /// Loudness at full level (sample peaks of roughly this times 1).
    pub scale: f32,
}

/// The coefficient of a one-pole low-pass with its corner at `hz`.
pub fn hz_to_k(hz: f32) -> f32 {
    let w = TAU * clampf(hz, 5.0, 10_000.0) / RATE as f32;
    clampf(w / (1.0 + w), 0.0005, 0.9)
}

/// How fast the level glides (per sample): about 50 ms.
const LEVEL_SLEW: f32 = 1.0 / 1100.0;
/// How fast the brightness glides (per sample): about 25 ms.
const TONE_SLEW: f32 = 1.0 / 550.0;

#[derive(Clone, Copy, Debug)]
pub struct Bed {
    grains: Granular,
    level: f32,
    level_to: f32,
    top_k: f32,
    top_k_to: f32,
    p1: f32,
    p2: f32,
    body: f32,
    body_k: f32,
    body_mix: f32,
    scale: f32,
}

impl Bed {
    pub fn new(kind: BedKind, seed: u32) -> Bed {
        let r = kind.recipe();
        let ms = |m: u32| m * RATE / 1000;
        let k = hz_to_k(r.top_hz);
        Bed {
            grains: Granular::new(seed, ms(r.grain_ms.0), ms(r.grain_ms.1)),
            level: 0.0,
            level_to: 0.0,
            top_k: k,
            top_k_to: k,
            p1: 0.0,
            p2: 0.0,
            body: 0.0,
            body_k: hz_to_k(r.body_hz),
            body_mix: r.body_mix,
            scale: r.scale,
        }
    }

    /// Where the bed should go: `level` (0 silent to 1 full) and `top_hz`,
    /// where its soft low-pass starts (lower is more muffled).
    pub fn set(&mut self, level: f32, top_hz: f32) {
        self.level_to = clampf(level, 0.0, 1.0);
        self.top_k_to = hz_to_k(top_hz);
    }

    /// Nothing to hear now and nothing coming (the mixer skips it).
    pub fn quiet(&self) -> bool {
        self.level < 0.000_3 && self.level_to < 0.000_3
    }

    /// The level it has reached (for tests and meters).
    pub fn level(&self) -> f32 {
        self.level
    }

    /// The next sample, about -0.5..0.5 at most; multiply by 32767.
    pub fn next(&mut self) -> f32 {
        self.level += (self.level_to - self.level) * LEVEL_SLEW;
        self.top_k += (self.top_k_to - self.top_k) * TONE_SLEW;
        let n = self.grains.next_mono();
        self.p1 += (n - self.p1) * self.top_k;
        self.p2 += (self.p1 - self.p2) * self.top_k;
        self.body += (n - self.body) * self.body_k;
        (self.p2 + self.body * self.body_mix) * self.level * self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(bed: &mut Bed, n: usize) -> f32 {
        let mut s = 0.0f64;
        for _ in 0..n {
            let x = bed.next() as f64;
            s += x * x;
        }
        (s / n as f64).sqrt() as f32
    }

    #[test]
    fn every_kind_has_a_distinct_slot_and_sane_recipe() {
        for (i, k) in BedKind::ALL.iter().enumerate() {
            assert_eq!(k.index(), i);
            let r = k.recipe();
            assert!(r.grain_ms.0 >= 200 && r.grain_ms.1 >= r.grain_ms.0);
            assert!(r.top_hz > r.body_hz);
            assert!(r.scale > 0.0 && r.scale <= 1.0);
        }
    }

    #[test]
    fn rain_is_soft_not_a_hiss() {
        let r = BedKind::Rain.recipe();
        assert!(r.top_hz >= 5000.0 && r.top_hz <= 7000.0);
    }

    #[test]
    fn a_bed_is_silent_until_raised_and_glides_up_and_down() {
        let mut b = Bed::new(BedKind::Rain, 5);
        assert!(b.quiet());
        assert_eq!(rms(&mut b, 2000), 0.0);
        b.set(1.0, 6000.0);
        assert!(!b.quiet());
        let loud = {
            rms(&mut b, 8000);
            rms(&mut b, 8000)
        };
        assert!(loud > 0.01 && loud < 0.5, "{loud}");
        b.set(0.0, 6000.0);
        rms(&mut b, 30_000);
        assert!(b.quiet(), "level {}", b.level());
    }

    #[test]
    fn the_output_stays_gentle_for_every_kind() {
        for k in BedKind::ALL {
            let mut b = Bed::new(k, 77);
            b.set(1.0, k.recipe().top_hz);
            let mut peak = 0.0f32;
            for _ in 0..60_000 {
                peak = peak.max(b.next().abs());
            }
            assert!(peak < 0.7, "{k:?} peaks at {peak}");
        }
    }

    #[test]
    fn a_lower_cutoff_is_duller() {
        // Muffled (under a roof) takes the highs away: less change from sample to sample.
        let roughness = |hz: f32| {
            let mut b = Bed::new(BedKind::Rain, 3);
            b.set(1.0, hz);
            for _ in 0..30_000 {
                b.next();
            }
            let (mut prev, mut sum, mut sq) = (b.next(), 0.0f64, 0.0f64);
            for _ in 0..30_000 {
                let x = b.next();
                sum += ((x - prev) as f64) * ((x - prev) as f64);
                sq += (x as f64) * (x as f64);
                prev = x;
            }
            sum / sq
        };
        assert!(roughness(900.0) < roughness(6000.0) * 0.5);
    }

    #[test]
    fn the_filter_coefficients_are_sane() {
        assert!(hz_to_k(6000.0) > hz_to_k(600.0));
        assert!(hz_to_k(1.0) >= 0.0005 && hz_to_k(1e9) <= 0.9);
    }
}
