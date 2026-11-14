//! Playing a recording at another speed: the pitch of a sound is a playback
//! rate, read by linear interpolation. The mixer does the same sample by
//! sample (`client/audio.rs`); this is the plain version that the tests and
//! the numbers of the docs come from.

use alloc::vec::Vec;

/// Slowest and fastest playback rate a mod may ask for.
pub const MIN_PITCH: f32 = 0.25;
pub const MAX_PITCH: f32 = 4.0;

/// A pitch kept in range; anything that is not a number is 1 (unchanged).
pub fn clamp_pitch(p: f32) -> f32 {
    if p.is_finite() {
        p.clamp(MIN_PITCH, MAX_PITCH)
    } else {
        1.0
    }
}

/// The recording at a fractional position, by linear interpolation. Before
/// the start it is the first sample, past the end the last one, and an empty
/// recording is silence.
pub fn sample_at(s: &[i16], pos: f32) -> f32 {
    let Some(last) = s.len().checked_sub(1) else { return 0.0 };
    if !(pos > 0.0) {
        return s[0] as f32;
    }
    let i = pos as usize;
    if i >= last {
        return s[last] as f32;
    }
    let frac = pos - i as f32;
    let (a, b) = (s[i] as f32, s[i + 1] as f32);
    a + (b - a) * frac
}

/// The whole recording played at `rate` (2 is an octave up and half as long).
/// The result has `floor((len - 1) / rate) + 1` samples; a rate of 1 gives
/// the recording back unchanged.
pub fn resample(s: &[i16], rate: f32) -> Vec<i16> {
    let rate = clamp_pitch(rate);
    if s.is_empty() {
        return Vec::new();
    }
    let n = ((s.len() - 1) as f32 / rate) as usize + 1;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(sample_at(s, i as f32 * rate).clamp(-32768.0, 32767.0) as i16);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(n: usize) -> Vec<i16> {
        (0..n).map(|i| (i as i16) * 10).collect()
    }

    #[test]
    fn a_rate_of_one_is_the_identity() {
        let s = ramp(500);
        assert_eq!(resample(&s, 1.0), s);
    }

    #[test]
    fn a_rate_of_two_halves_the_length_and_skips_every_other_sample() {
        let s = ramp(1000);
        let r = resample(&s, 2.0);
        assert_eq!(r.len(), 500);
        for (i, v) in r.iter().enumerate() {
            assert_eq!(*v, s[i * 2]);
        }
    }

    #[test]
    fn a_rate_of_a_half_doubles_it_and_fills_between_with_the_line() {
        let s = ramp(100);
        let r = resample(&s, 0.5);
        assert_eq!(r.len(), 199);
        assert_eq!(r[1], 5);
        assert_eq!(r[2], 10);
    }

    #[test]
    fn the_rate_is_kept_in_bounds() {
        let s = ramp(1000);
        assert_eq!(resample(&s, 100.0).len(), resample(&s, MAX_PITCH).len());
        assert_eq!(resample(&s, 0.0001).len(), resample(&s, MIN_PITCH).len());
        assert_eq!(resample(&s, f32::NAN), s);
        assert_eq!(clamp_pitch(-3.0), MIN_PITCH);
        assert_eq!(clamp_pitch(f32::INFINITY), 1.0);
    }

    #[test]
    fn reading_outside_the_recording_is_safe() {
        let s = [100i16, 200, 300];
        assert_eq!(sample_at(&s, -5.0), 100.0);
        assert_eq!(sample_at(&s, f32::NAN), 100.0);
        assert_eq!(sample_at(&s, 1.5), 250.0);
        assert_eq!(sample_at(&s, 2.0), 300.0);
        assert_eq!(sample_at(&s, 1e9), 300.0);
        assert_eq!(sample_at(&[], 3.0), 0.0);
        assert!(resample(&[], 1.0).is_empty());
        assert_eq!(resample(&[7], 4.0), alloc::vec![7]);
    }
}
