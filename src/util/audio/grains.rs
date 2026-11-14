//! Granular noise: a continuous sound built from many short grains instead
//! of one clip that repeats.
//!
//! Each grain is a stretch of noise of its own (its own random start, a
//! random length, a random pitch within about 4 percent, a random volume
//! within about 2 dB and a random place between left and right). A grain
//! starts while the one before it is fading out, over the same number of
//! samples, with equal-power weights (`sin` going up, `cos` coming down:
//! their squares always add to 1). So the loudness never dips or jumps where
//! two grains meet, and no stretch of the sound ever comes round again.
//!
//! Nothing here allocates; a [`Granular`] is a few dozen bytes and is fine
//! to run in the audio callback.

use super::rand::Rng;
use super::variation::db_to_gain;
use crate::math::{clampf, cosf, sinf, sqrtf, PI};

/// Grain pitch varies by this much either way.
pub const PITCH_SPREAD: f32 = 0.04;
/// Grain volume varies by this many decibels either way.
pub const GAIN_DB: f32 = 2.0;

/// Equal-power crossfade weights at `t` (0 to 1): `(rising, falling)`.
/// `rising² + falling² = 1` everywhere.
pub fn xfade(t: f32) -> (f32, f32) {
    let a = clampf(t, 0.0, 1.0) * PI * 0.5;
    (sinf(a), cosf(a))
}

/// Left and right volume for a place `pan` (-1 left, 1 right), equal power.
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let a = (clampf(pan, -1.0, 1.0) + 1.0) * PI * 0.25;
    (cosf(a), sinf(a))
}

#[derive(Clone, Copy, Debug)]
pub struct Grain {
    age: u32,
    len: u32,
    /// Samples it takes to fade in (0 = starts at full volume).
    fade_in: u32,
    /// Samples it takes to fade out at its end.
    fade_out: u32,
    pitch: f32,
    gain: f32,
    pan: f32,
    /// Mono volume from its place between left and right.
    mono: f32,
    rng: Rng,
    phase: f32,
    prev: f32,
    next: f32,
}

impl Grain {
    fn new(rng_seed: u32, len: u32, fade_in: u32, fade_out: u32, pitch: f32, gain: f32, pan: f32) -> Grain {
        let mut rng = Rng::new(rng_seed);
        let (l, r) = pan_gains(pan);
        let (prev, next) = (rng.signed(), rng.signed());
        Grain { age: 0, len, fade_in, fade_out, pitch, gain, pan, mono: (l + r) * 0.707_106_8, rng, phase: 0.0, prev, next }
    }

    /// Its volume now (0 to 1) from the fades.
    pub fn weight(&self) -> f32 {
        let fin = if self.fade_in > 0 && self.age < self.fade_in { xfade(self.age as f32 / self.fade_in as f32).0 } else { 1.0 };
        let fout = if self.fade_out > 0 && self.age + self.fade_out >= self.len {
            xfade((self.age + self.fade_out - self.len) as f32 / self.fade_out as f32).1
        } else {
            1.0
        };
        fin * fout
    }

    /// Its place now: -1 left to 1 right.
    pub fn pan(&self) -> f32 {
        self.pan
    }

    pub fn done(&self) -> bool {
        self.age >= self.len
    }

    /// The next sample of its noise, read at its own pitch.
    fn noise(&mut self) -> f32 {
        self.phase += self.pitch;
        while self.phase >= 1.0 {
            self.phase -= 1.0;
            self.prev = self.next;
            self.next = self.rng.signed();
        }
        self.prev + (self.next - self.prev) * self.phase
    }

    /// The next sample, faded and scaled; moves it on.
    fn step(&mut self) -> f32 {
        let w = self.weight();
        let n = self.noise();
        self.age += 1;
        n * w * self.gain
    }
}

/// One endless stream of overlapping grains.
#[derive(Clone, Copy, Debug)]
pub struct Granular {
    rng: Rng,
    min_len: u32,
    max_len: u32,
    fade_cap: u32,
    cur: Grain,
    incoming: Option<Grain>,
}

impl Granular {
    /// A stream of grains `min_len` to `max_len` samples long (at least 64).
    pub fn new(seed: u32, min_len: u32, max_len: u32) -> Granular {
        let min_len = min_len.max(64);
        let max_len = max_len.max(min_len);
        let mut rng = Rng::new(seed);
        let fade_cap = (min_len / 3).max(2);
        let first = Self::make(&mut rng, min_len, max_len, fade_cap, 0);
        Granular { rng, min_len, max_len, fade_cap, cur: first, incoming: None }
    }

    fn make(rng: &mut Rng, min_len: u32, max_len: u32, fade_cap: u32, fade_in: u32) -> Grain {
        let len = rng.range(min_len as f32, max_len as f32 + 1.0) as u32;
        let len = len.clamp(min_len, max_len);
        let fade_out = (len / 4).min(fade_cap).max(1);
        let pitch = 1.0 + rng.signed() * PITCH_SPREAD;
        let gain = db_to_gain(rng.signed() * GAIN_DB);
        let pan = rng.signed();
        Grain::new(rng.next_u32() | 1, len, fade_in, fade_out, pitch, gain, pan)
    }

    /// The grain playing now (for tests and meters).
    pub fn current(&self) -> &Grain {
        &self.cur
    }

    /// Sum of the squared weights of the grains playing now. 1 at every
    /// moment: that is what "no gap, no seam" means.
    pub fn power(&self) -> f32 {
        let a = self.cur.weight();
        let b = self.incoming.as_ref().map_or(0.0, |g| g.weight());
        a * a + b * b
    }

    /// The next sample for the left and right ear.
    pub fn next_stereo(&mut self) -> (f32, f32) {
        let pa = pan_gains(self.cur.pan);
        let a = self.cur.step();
        let mut l = a * pa.0;
        let mut r = a * pa.1;
        if let Some(g) = self.incoming.as_mut() {
            let pb = pan_gains(g.pan);
            let b = g.step();
            l += b * pb.0;
            r += b * pb.1;
        }
        self.advance();
        (l, r)
    }

    /// The next sample for a single speaker (the places folded together,
    /// with the pan's loudness kept, between 0.71 and 1).
    pub fn next_mono(&mut self) -> f32 {
        let mut s = self.cur.mono * self.cur.step();
        if let Some(g) = self.incoming.as_mut() {
            s += g.mono * g.step();
        }
        self.advance();
        s
    }

    /// Hand over from a finished grain, and start the next one while the
    /// current one fades out.
    fn advance(&mut self) {
        if self.cur.done() {
            self.cur = match self.incoming.take() {
                Some(g) => g,
                None => Self::make(&mut self.rng, self.min_len, self.max_len, self.fade_cap, 0),
            };
        }
        if self.incoming.is_none() && self.cur.age + self.cur.fade_out >= self.cur.len {
            let fade = self.cur.fade_out;
            self.incoming = Some(Self::make(&mut self.rng, self.min_len, self.max_len, self.fade_cap, fade));
        }
    }
}

/// How loud a stereo pair is as one number, for tests.
pub fn level(l: f32, r: f32) -> f32 {
    sqrtf(l * l + r * r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossfade_weights_are_equal_power() {
        for i in 0..=100 {
            let (up, down) = xfade(i as f32 / 100.0);
            assert!((up * up + down * down - 1.0).abs() < 1e-4, "{i}");
            assert!(up >= -1e-6 && down >= -1e-6);
        }
        assert!(xfade(0.0).0.abs() < 1e-5 && (xfade(0.0).1 - 1.0).abs() < 1e-5);
        assert!((xfade(1.0).0 - 1.0).abs() < 1e-5 && xfade(1.0).1.abs() < 1e-5);
        let (l, r) = pan_gains(0.3);
        assert!((l * l + r * r - 1.0).abs() < 1e-4);
        let (l, r) = pan_gains(-1.0);
        assert!(l > 0.99 && r.abs() < 1e-4);
    }

    #[test]
    fn grains_always_overlap_with_total_power_one() {
        // Many grains, short ones and long ones: at every sample the weights
        // of the grains playing square-add to 1 (no gap, no bump).
        for seed in [1u32, 2, 77, 12345] {
            let mut g = Granular::new(seed, 300, 900);
            let mut hand_overs = 0;
            let mut last_len = g.current().len;
            for i in 0..200_000 {
                let p = g.power();
                assert!((p - 1.0).abs() < 2e-3, "seed {seed} sample {i}: power {p}");
                g.next_mono();
                if g.current().len != last_len || g.current().age == 0 {
                    hand_overs += 1;
                    last_len = g.current().len;
                }
            }
            assert!(hand_overs > 50, "grains did not change over");
        }
    }

    #[test]
    fn no_hard_seam_the_output_has_no_jump_at_hand_over() {
        // The sound is noise, so compare with the noise's own size: no sample
        // may be far beyond what noise of this volume reaches.
        let mut g = Granular::new(9, 200, 400);
        let mut peak = 0.0f32;
        for _ in 0..100_000 {
            let s = g.next_mono();
            peak = peak.max(s.abs());
        }
        assert!(peak < 2.5, "peak {peak}");
        assert!(peak > 0.5, "silent: {peak}");
    }

    #[test]
    fn grain_properties_stay_in_their_ranges() {
        let mut rng = Rng::new(3);
        for _ in 0..2000 {
            let gr = Granular::make(&mut rng, 500, 1500, 166, 0);
            assert!(gr.len >= 500 && gr.len <= 1500);
            assert!(gr.pitch >= 1.0 - PITCH_SPREAD - 1e-5 && gr.pitch <= 1.0 + PITCH_SPREAD + 1e-5);
            assert!(gr.gain >= 0.79 && gr.gain <= 1.27, "{}", gr.gain);
            assert!(gr.pan >= -1.0 && gr.pan < 1.0);
            assert!(gr.fade_out >= 1 && gr.fade_out <= 166 && gr.fade_in + gr.fade_out < gr.len);
        }
    }

    #[test]
    fn the_stereo_and_mono_paths_agree_on_loudness_roughly() {
        let (mut a, mut b) = (Granular::new(4, 400, 800), Granular::new(4, 400, 800));
        let (mut sa, mut sb) = (0.0f64, 0.0f64);
        for _ in 0..50_000 {
            let (l, r) = a.next_stereo();
            sa += (l * l + r * r) as f64;
            let m = b.next_mono();
            sb += (m * m) as f64;
        }
        // Mono = (l + r) / sqrt 2 per grain, so its power is within 0.5x..1x of the pair's.
        assert!(sb > sa * 0.4 && sb < sa * 1.1, "{sa} {sb}");
        assert!(level(3.0, 4.0) > 4.99);
    }
}
