//! The voice codec: IMA ADPCM, 4 bits a sample, 16 kHz mono, in 20 ms frames
//! (320 samples). A frame is 3 header bytes (the predictor, 16-bit
//! little-endian, and the step index) and then two samples per byte, so it is
//! 163 bytes: about 8 kB a second. The header carries the coder's state, so
//! every frame decodes by itself: a lost frame costs a click, not the rest
//! of the speech.
//!
//! The tables are the ones the WAV reader uses (`util/audio/adpcm.rs`).
//! Also here: the loudness measure and the gate that decides when to talk.

use crate::wav::adpcm::{IMA_INDEX, IMA_STEPS};
use alloc::vec::Vec;

/// Samples a second.
pub const RATE: u32 = 16_000;
/// Samples in a frame (20 ms).
pub const FRAME_SAMPLES: usize = 320;
/// Bytes before the samples.
pub const HEADER: usize = 3;
/// Most samples a frame may hold (the receiver's limit).
pub const MAX_FRAME_SAMPLES: usize = 2 * (crate::proto::MAX_VOICE_FRAME - HEADER);

fn code(pred: &mut i32, index: &mut i32, sample: i32) -> u8 {
    let step = IMA_STEPS[*index as usize];
    let mut diff = sample - *pred;
    let mut nib = 0u8;
    if diff < 0 {
        nib = 8;
        diff = -diff;
    }
    let mut vp = step >> 3;
    if diff >= step {
        nib |= 4;
        diff -= step;
        vp += step;
    }
    if diff >= step >> 1 {
        nib |= 2;
        diff -= step >> 1;
        vp += step >> 1;
    }
    if diff >= step >> 2 {
        nib |= 1;
        vp += step >> 2;
    }
    *pred = (*pred + if nib & 8 != 0 { -vp } else { vp }).clamp(-32768, 32767);
    *index = (*index + IMA_INDEX[nib as usize]).clamp(0, 88);
    nib
}

/// Turns speech into frames. Keep one per talker and feed it in order.
#[derive(Default)]
pub struct Encoder {
    pred: i32,
    index: i32,
}

impl Encoder {
    /// Encode `pcm` (an even number of samples is best; 320 usual) and append the frame to `out`.
    pub fn encode(&mut self, pcm: &[i16], out: &mut Vec<u8>) {
        let pcm = &pcm[..pcm.len().min(MAX_FRAME_SAMPLES)];
        out.extend_from_slice(&(self.pred as i16).to_le_bytes());
        out.push(self.index as u8);
        for pair in pcm.chunks(2) {
            let a = code(&mut self.pred, &mut self.index, pair[0] as i32);
            let b = code(&mut self.pred, &mut self.index, pair.get(1).copied().unwrap_or(pair[0]) as i32);
            out.push(a | (b << 4));
        }
    }
}

/// Turns a frame back into samples. Frames carry their own state, so this
/// holds none. False (and nothing added) if the frame is not one.
pub fn decode(frame: &[u8], out: &mut Vec<i16>) -> bool {
    if frame.len() < HEADER + 1 || frame.len() > crate::proto::MAX_VOICE_FRAME || frame[2] > 88 {
        return false;
    }
    let mut pred = i16::from_le_bytes([frame[0], frame[1]]) as i32;
    let mut index = frame[2] as i32;
    for &byte in &frame[HEADER..] {
        for nib in [byte & 15, byte >> 4] {
            let step = IMA_STEPS[index as usize];
            let mut diff = step >> 3;
            if nib & 1 != 0 {
                diff += step >> 2;
            }
            if nib & 2 != 0 {
                diff += step >> 1;
            }
            if nib & 4 != 0 {
                diff += step;
            }
            pred = (pred + if nib & 8 != 0 { -diff } else { diff }).clamp(-32768, 32767);
            index = (index + IMA_INDEX[nib as usize]).clamp(0, 88);
            out.push(pred as i16);
        }
    }
    true
}

/// How loud a piece of speech is: its root mean square, 0.0 to 1.0.
pub fn level(pcm: &[i16]) -> f32 {
    if pcm.is_empty() {
        return 0.0;
    }
    let sum: i64 = pcm.iter().map(|&s| s as i64 * s as i64).sum();
    crate::math::sqrtf(sum as f32 / pcm.len() as f32) / 32768.0
}

/// The loudness (see [`level`]) that voice activation opens at, for the
/// setting 0 (very sensitive) to 10 (only loud speech).
pub fn threshold(setting: i32) -> f32 {
    const T: [f32; 11] = [0.002, 0.004, 0.006, 0.009, 0.013, 0.019, 0.028, 0.04, 0.06, 0.09, 0.14];
    T[setting.clamp(0, 10) as usize]
}

/// How long the gate stays open after the last loud frame, so word ends and
/// pauses between words are not cut off.
pub const HANG_SECONDS: f32 = 0.45;

/// Voice activation: open when speech is louder than the threshold, and stay
/// open a little while after.
#[derive(Default)]
pub struct Gate {
    hang: f32,
}

impl Gate {
    /// Feed one frame's loudness; true if the frame should be sent.
    pub fn update(&mut self, level: f32, threshold: f32, dt: f32) -> bool {
        if level >= threshold {
            self.hang = HANG_SECONDS;
        } else {
            self.hang -= dt;
        }
        self.hang > 0.0
    }

    pub fn close(&mut self) {
        self.hang = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, from: usize, amp: f32) -> Vec<i16> {
        (0..n)
            .map(|i| {
                let t = (from + i) as f32 / RATE as f32;
                let x = crate::math::sinf(t * 440.0 * 6.283_185) + 0.5 * crate::math::sinf(t * 1300.0 * 6.283_185);
                (x * amp) as i16
            })
            .collect()
    }

    #[test]
    fn speech_survives_the_codec() {
        let mut enc = Encoder::default();
        let (mut sig, mut back) = (Vec::new(), Vec::new());
        for f in 0..20 {
            let pcm = tone(FRAME_SAMPLES, f * FRAME_SAMPLES, 9000.0);
            let mut frame = Vec::new();
            enc.encode(&pcm, &mut frame);
            assert_eq!(frame.len(), HEADER + FRAME_SAMPLES / 2);
            assert!(frame.len() <= crate::proto::MAX_VOICE_FRAME);
            assert!(decode(&frame, &mut back));
            sig.extend(pcm);
        }
        assert_eq!(back.len(), sig.len());
        // After the coder has settled, the error is small next to the signal.
        let (mut err, mut pow) = (0f64, 0f64);
        for i in 2 * FRAME_SAMPLES..sig.len() {
            let d = (sig[i] as i32 - back[i] as i32) as f64;
            err += d * d;
            pow += (sig[i] as f64) * (sig[i] as f64);
        }
        assert!(err < pow * 0.01, "signal to noise too low: error {err} of {pow}");
    }

    #[test]
    fn a_frame_decodes_by_itself_after_losses() {
        let mut enc = Encoder::default();
        let frames: Vec<Vec<u8>> = (0..8)
            .map(|f| {
                let mut v = Vec::new();
                enc.encode(&tone(FRAME_SAMPLES, f * FRAME_SAMPLES, 7000.0), &mut v);
                v
            })
            .collect();
        let mut all = Vec::new();
        for f in &frames {
            decode(f, &mut all);
        }
        // Frame 5 on its own, as if 0 to 4 never arrived.
        let mut alone = Vec::new();
        assert!(decode(&frames[5], &mut alone));
        assert_eq!(&all[5 * FRAME_SAMPLES..6 * FRAME_SAMPLES], &alone[..]);
    }

    #[test]
    fn silence_stays_quiet_and_junk_is_refused() {
        let mut enc = Encoder::default();
        let mut frame = Vec::new();
        enc.encode(&[0i16; FRAME_SAMPLES], &mut frame);
        let mut out = Vec::new();
        assert!(decode(&frame, &mut out));
        assert!(out.iter().all(|&s| s.abs() < 20));
        assert!(!decode(&[], &mut out));
        assert!(!decode(&[0, 0, 0], &mut out));
        assert!(!decode(&[0, 0, 89, 1, 2], &mut out), "step index out of range");
        assert!(!decode(&alloc::vec![0u8; 201], &mut out));
        // Extremes do not overflow.
        let mut loud = Vec::new();
        let sq: Vec<i16> = (0..FRAME_SAMPLES).map(|i| if i % 2 == 0 { i16::MAX } else { i16::MIN }).collect();
        enc.encode(&sq, &mut loud);
        assert!(decode(&loud, &mut out));
    }

    #[test]
    fn loudness_and_the_gate() {
        assert_eq!(level(&[]), 0.0);
        assert_eq!(level(&[0; 10]), 0.0);
        assert!((level(&[16384; 100]) - 0.5).abs() < 0.01);
        assert!(threshold(0) < threshold(5) && threshold(5) < threshold(10));
        assert_eq!(threshold(-3), threshold(0));
        assert_eq!(threshold(99), threshold(10));
        let mut g = Gate::default();
        assert!(!g.update(0.0, 0.01, 0.02));
        assert!(g.update(0.5, 0.01, 0.02));
        // Stays open through a short pause, then closes.
        assert!(g.update(0.0, 0.01, 0.2));
        assert!(g.update(0.0, 0.01, 0.2));
        assert!(!g.update(0.0, 0.01, 0.2));
    }
}
