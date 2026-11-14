//! Playing other players' voices: each frame is decoded, resampled from
//! 16 kHz to the mixer's rate and handed to that talker's stream in the
//! mixer (`audio.rs`, `stream.rs`), which holds the jitter buffer.
//!
//! The server has already chosen who hears whom and how loud: all that is
//! left to do here is arithmetic on a few hundred samples per frame.

use super::codec;
use crate::client::audio::Audio;
use alloc::vec::Vec;

/// Talkers remembered at once (the mixer plays fewer).
const MAX_TALKERS: usize = 32;

/// Linear resampling from the codec's rate to the mixer's, one frame after
/// another without a seam between them.
pub struct Resampler {
    /// Where the next output sample falls, in input samples after `prev`.
    pos: f32,
    prev: f32,
    primed: bool,
}

impl Default for Resampler {
    fn default() -> Resampler {
        Resampler { pos: 0.0, prev: 0.0, primed: false }
    }
}

impl Resampler {
    /// Append the input at `from` Hz, resampled to `to` Hz, to `out`.
    pub fn run(&mut self, input: &[i16], from: u32, to: u32, out: &mut Vec<i16>) {
        if input.is_empty() || from == 0 || to == 0 {
            return;
        }
        let step = from as f32 / to as f32;
        if !self.primed {
            self.prev = input[0] as f32;
            self.primed = true;
        }
        // The samples we interpolate between: `prev`, then the input.
        let at = |i: usize| if i == 0 { self.prev } else { input[i - 1] as f32 };
        let last = input.len(); // index of the last sample in that view
        let mut pos = self.pos;
        while pos < last as f32 {
            let i = pos as usize;
            let f = pos - i as f32;
            let (a, b) = (at(i), at(i + 1));
            out.push((a + (b - a) * f) as i16);
            pos += step;
        }
        self.pos = pos - last as f32;
        self.prev = input[input.len() - 1] as f32;
    }
}

/// Whether sequence number `seq` is later than `last` (they wrap around).
pub fn newer(seq: u16, last: u16) -> bool {
    (seq.wrapping_sub(last) as i16) > 0
}

/// How far to the right (1) or left (-1) of a listener a talker is, from
/// the way the listener faces (`yaw`, as in `Player`: facing is
/// `(-sin yaw, -cos yaw)` in x and z) and their positions.
pub fn pan_toward(yaw: f32, listener: [f64; 3], talker: [f64; 3]) -> f32 {
    let (dx, dz) = ((talker[0] - listener[0]) as f32, (talker[2] - listener[2]) as f32);
    let len = crate::math::sqrtf(dx * dx + dz * dz);
    if !(len > 0.5) {
        return 0.0;
    }
    // The listener's right hand.
    let (rx, rz) = (crate::math::cosf(yaw), -crate::math::sinf(yaw));
    ((dx * rx + dz * rz) / len).clamp(-1.0, 1.0)
}

/// Left and right loudness for a pan (equal power: the sum of squares stays 1).
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let p = pan.clamp(-1.0, 1.0);
    (crate::math::sqrtf((1.0 - p) * 0.5), crate::math::sqrtf((1.0 + p) * 0.5))
}

struct Talker {
    id: u16,
    last_seq: u16,
    resampler: Resampler,
}

#[derive(Default)]
pub struct Playback {
    talkers: Vec<Talker>,
    pcm: Vec<i16>,
    out: Vec<i16>,
}

impl Playback {
    /// Play one frame from player `id`. `loud` is the server's distance gain
    /// (0..=255), `volume` the listener's own voice volume (0..1), `pan` as
    /// in [`pan_toward`]. Returns false if the frame was not played (old,
    /// duplicate or damaged).
    pub fn play(&mut self, audio: &mut Audio, id: u16, seq: u16, loud: u8, volume: f32, pan: f32, frame: &[u8]) -> bool {
        let k = match self.talkers.iter().position(|t| t.id == id) {
            Some(k) => k,
            None => {
                if self.talkers.len() >= MAX_TALKERS {
                    self.talkers.remove(0);
                }
                self.talkers.push(Talker { id, last_seq: seq.wrapping_sub(1), resampler: Resampler::default() });
                self.talkers.len() - 1
            }
        };
        if !newer(seq, self.talkers[k].last_seq) {
            return false;
        }
        self.pcm.clear();
        if !codec::decode(frame, &mut self.pcm) {
            return false;
        }
        let t = &mut self.talkers[k];
        t.last_seq = seq;
        self.out.clear();
        t.resampler.run(&self.pcm, codec::RATE, crate::wav::RATE, &mut self.out);
        let gain = (loud as f32 / 255.0) * volume.clamp(0.0, 2.0);
        audio.voice_push(id, &self.out, gain, pan);
        true
    }

    /// Forget a player (they left).
    pub fn forget(&mut self, id: u16) {
        self.talkers.retain(|t| t.id != id);
    }

    pub fn clear(&mut self) {
        self.talkers.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_keeps_the_pitch_and_has_no_seams() {
        let mut r = Resampler::default();
        let mut out = Vec::new();
        // A ramp, in frames of 320, as it arrives.
        for f in 0..10 {
            let input: Vec<i16> = (0..320).map(|i| ((f * 320 + i) * 2) as i16).collect();
            r.run(&input, 16_000, 22_050, &mut out);
        }
        // 3200 samples in at 16 kHz is 200 ms: about 4410 out.
        assert!((out.len() as i32 - 4410).abs() <= 3, "{}", out.len());
        // Still a smooth, rising ramp across the frame joins.
        for w in out.windows(2) {
            let d = w[1] as i32 - w[0] as i32;
            assert!((0..=3).contains(&d), "step {d} between {} and {}", w[0], w[1]);
        }
        // A constant stays constant, and empty input does nothing.
        let mut c = Resampler::default();
        let mut flat = Vec::new();
        c.run(&[500; 320], 16_000, 22_050, &mut flat);
        c.run(&[500; 320], 16_000, 22_050, &mut flat);
        c.run(&[], 16_000, 22_050, &mut flat);
        assert!(flat.iter().all(|&s| s == 500));
    }

    #[test]
    fn sequence_numbers_wrap() {
        assert!(newer(5, 4));
        assert!(!newer(4, 4));
        assert!(!newer(3, 4));
        assert!(newer(0, 65_535));
        assert!(newer(10, 65_530));
        assert!(!newer(65_530, 10));
    }

    #[test]
    fn sound_pans_toward_where_the_talker_is() {
        let me = [0.0, 64.0, 0.0];
        // Facing north (-z): east is on the right.
        assert!(pan_toward(0.0, me, [10.0, 64.0, 0.0]) > 0.99);
        assert!(pan_toward(0.0, me, [-10.0, 64.0, 0.0]) < -0.99);
        assert!(pan_toward(0.0, me, [0.0, 64.0, -10.0]).abs() < 0.01, "straight ahead");
        assert!(pan_toward(0.0, me, [0.0, 64.0, 10.0]).abs() < 0.01, "straight behind");
        // Turned to face west (yaw a quarter turn left), north is on the right.
        assert!(pan_toward(core::f32::consts::FRAC_PI_2, me, [0.0, 64.0, -10.0]) > 0.99);
        // On top of each other: centred.
        assert_eq!(pan_toward(1.0, me, [0.1, 70.0, 0.1]), 0.0);
        let (l, r) = pan_gains(0.0);
        assert!((l - r).abs() < 1e-6 && (l * l + r * r - 1.0).abs() < 1e-5);
        let (l, r) = pan_gains(1.0);
        assert!(l < 0.01 && r > 0.99);
        let (l, r) = pan_gains(-5.0);
        assert!(l > 0.99 && r < 0.01);
    }

    #[test]
    fn old_and_damaged_frames_are_not_played() {
        let mut audio = Audio::silent();
        let mut pb = Playback::default();
        let mut enc = codec::Encoder::default();
        let mut frame = Vec::new();
        enc.encode(&[100i16; codec::FRAME_SAMPLES], &mut frame);
        assert!(pb.play(&mut audio, 3, 10, 255, 1.0, 0.0, &frame));
        assert!(!pb.play(&mut audio, 3, 10, 255, 1.0, 0.0, &frame), "duplicate");
        assert!(!pb.play(&mut audio, 3, 9, 255, 1.0, 0.0, &frame), "late");
        assert!(pb.play(&mut audio, 3, 12, 255, 1.0, 0.0, &frame), "a gap is fine");
        assert!(!pb.play(&mut audio, 3, 13, 255, 1.0, 0.0, &[1, 2]), "damaged");
        // Another talker has a count of their own.
        assert!(pb.play(&mut audio, 4, 10, 255, 1.0, 0.0, &frame));
        pb.forget(3);
        assert!(pb.play(&mut audio, 3, 1, 255, 1.0, 0.0, &frame));
    }
}
