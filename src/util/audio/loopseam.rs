//! Looping a recording without a click. A clip has no loop points, so the
//! last few milliseconds are crossfaded into the start: while the play
//! position is inside the last `xf` samples the output is the tail fading
//! out (cosine) mixed with the head fading in (sine, equal power), and when
//! the position reaches the end it wraps to `xf` samples in, which is where
//! the head's fade-in ended. The two sides of the seam are then neighbours of
//! the original recording, so the join is as smooth as the recording is.
//!
//! (The first play-through starts at sample 0 as written, and a loop is
//! `len - xf` samples long.)

use super::resample::sample_at;
use crate::math::{cosf, sinf};

/// How long the crossfade is, in milliseconds.
pub const SEAM_MS: u32 = 40;

/// Samples of crossfade for a clip of `len` samples: 40 ms at the mixer's
/// rate, at most a quarter of the clip (a short clip loops with a short
/// seam), none for a clip too short to have one.
pub fn seam_len(len: usize) -> usize {
    let want = (SEAM_MS as usize * super::RATE as usize) / 1000;
    let n = want.min(len / 4);
    if n < 2 {
        0
    } else {
        n
    }
}

/// The play position after advancing: past the end it goes back by one loop
/// (`len - xf` samples), as many times as it takes.
pub fn wrap(pos: f32, len: usize, xf: usize) -> f32 {
    let period = len.saturating_sub(xf) as f32;
    if !(period >= 1.0) || !pos.is_finite() {
        return 0.0;
    }
    let mut p = pos;
    while p >= len as f32 {
        p -= period;
    }
    if p < 0.0 {
        0.0
    } else {
        p
    }
}

/// The sample of a looping clip at `pos` (0 up to `len`), with the seam
/// applied when `pos` is in the last `xf` samples.
pub fn loop_sample(s: &[i16], pos: f32, xf: usize) -> f32 {
    let len = s.len();
    if xf == 0 || xf >= len {
        return sample_at(s, pos);
    }
    let start = (len - xf) as f32;
    if pos < start {
        return sample_at(s, pos);
    }
    let k = pos - start;
    let t = (k / xf as f32).clamp(0.0, 1.0) * core::f32::consts::FRAC_PI_2;
    cosf(t) * sample_at(s, pos) + sinf(t) * sample_at(s, k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    /// A tone whose period does not divide the clip length, so the
    /// recording's end and start do not meet.
    fn tone(len: usize) -> Vec<i16> {
        (0..len).map(|i| (sinf(i as f32 * 0.31) * 12000.0) as i16).collect()
    }

    #[test]
    fn the_seam_is_forty_milliseconds_or_a_quarter_of_a_short_clip() {
        assert_eq!(seam_len(100_000), 882);
        assert_eq!(seam_len(2000), 500);
        assert_eq!(seam_len(5), 0);
        assert_eq!(seam_len(0), 0);
    }

    #[test]
    fn the_join_is_as_smooth_as_the_tone_itself() {
        let s = tone(5000);
        let xf = seam_len(s.len());
        let mut out = Vec::new();
        let mut pos = 0.0f32;
        // Three loops and a bit.
        for _ in 0..(s.len() - xf) * 3 + 100 {
            out.push(loop_sample(&s, pos, xf));
            pos = wrap(pos + 1.0, s.len(), xf);
        }
        // The tone's own biggest step.
        let own = s.windows(2).map(|w| (w[1] as f32 - w[0] as f32).abs()).fold(0.0, f32::max);
        let worst = out.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(worst < own * 1.5, "worst step {worst} against the tone's own {own}");
        // The step across the wrap itself (the sample at the end, then the one `xf` in).
        let join = (out[s.len()] - out[s.len() - 1]).abs();
        assert!(join < own * 1.5, "{join}");
    }

    #[test]
    fn outside_the_seam_the_clip_is_untouched() {
        let s = tone(3000);
        let xf = seam_len(s.len());
        for i in 0..(s.len() - xf) {
            assert_eq!(loop_sample(&s, i as f32, xf), s[i] as f32);
        }
        assert_eq!(loop_sample(&s, 10.5, 0), sample_at(&s, 10.5));
    }

    #[test]
    fn wrapping_goes_back_by_one_loop_and_is_safe_on_nonsense() {
        assert_eq!(wrap(99.0, 100, 10), 99.0);
        assert_eq!(wrap(100.0, 100, 10), 10.0);
        assert!((wrap(100.5, 100, 10) - 10.5).abs() < 1e-4);
        // A step bigger than the loop wraps more than once.
        assert!(wrap(400.0, 100, 90) < 100.0);
        assert_eq!(wrap(f32::NAN, 100, 10), 0.0);
        assert_eq!(wrap(5.0, 0, 0), 0.0);
        assert_eq!(wrap(5.0, 10, 10), 0.0);
    }
}
