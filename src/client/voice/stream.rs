//! One talker's voice inside the mixer: a small ring of ready-to-play
//! samples at the mixer's rate, with a jitter buffer in front.
//!
//! The game thread pushes decoded, resampled frames in (while holding SDL's
//! audio lock, as the mixer's other voices are added); the audio callback
//! pulls from it. Playing only starts once a couple of frames have
//! arrived, and stops (to fill up again) if the stream runs dry, so network
//! hiccups become a short pause rather than stutter. If frames pile up
//! (a burst after a lag), the oldest are dropped: delay stays bounded.
//! Fixed size, no allocation: it lives in the audio thread's state.

/// Samples held at most (a power of two): 186 ms at 22 050 Hz.
pub const CAP: usize = 4096;
/// Playing starts when this many samples wait: two 20 ms frames.
pub const PRIME: usize = 882;
/// A stream with nothing to play for this many samples is given up (about 1 s).
const IDLE_LIMIT: u32 = 22_050;

#[derive(Clone)]
pub struct VoiceStream {
    /// Whose voice (0 = this slot is free).
    pub id: u16,
    buf: [i16; CAP],
    /// Samples ever written and ever read (differences matter).
    wr: usize,
    rd: usize,
    /// Loudness, 0..1 and more, from the server's distance gain and the
    /// listener's volume settings.
    pub gain: f32,
    /// Where they are, -1 (left) to 1 (right). The mixer is mono, so this is
    /// kept for when it is stereo.
    pub pan: f32,
    playing: bool,
    idle: u32,
}

impl VoiceStream {
    pub const fn new() -> VoiceStream {
        VoiceStream { id: 0, buf: [0; CAP], wr: 0, rd: 0, gain: 0.0, pan: 0.0, playing: false, idle: 0 }
    }

    pub fn free(&self) -> bool {
        self.id == 0
    }

    /// Samples waiting to be played.
    pub fn buffered(&self) -> usize {
        self.wr.wrapping_sub(self.rd)
    }

    /// Start over for `id`.
    pub fn claim(&mut self, id: u16) {
        self.id = id;
        self.wr = 0;
        self.rd = 0;
        self.playing = false;
        self.idle = 0;
    }

    pub fn release(&mut self) {
        self.id = 0;
        self.wr = 0;
        self.rd = 0;
        self.playing = false;
    }

    /// Add samples, dropping the oldest if there is no room.
    pub fn push(&mut self, samples: &[i16], gain: f32, pan: f32) {
        self.gain = gain;
        self.pan = pan;
        self.idle = 0;
        let samples = &samples[samples.len().saturating_sub(CAP)..];
        let over = (self.buffered() + samples.len()).saturating_sub(CAP);
        self.rd = self.rd.wrapping_add(over);
        for &s in samples {
            self.buf[self.wr & (CAP - 1)] = s;
            self.wr = self.wr.wrapping_add(1);
        }
    }

    /// Add this stream's next samples into `out` (the mixer's buffer),
    /// which is `out.len()` samples long. Silent and cheap when it has
    /// nothing; frees itself after a long silence.
    pub fn mix_into(&mut self, out: &mut [i16]) {
        if self.id == 0 {
            return;
        }
        if !self.playing {
            if self.buffered() >= PRIME {
                self.playing = true;
            } else {
                self.idle = self.idle.saturating_add(out.len() as u32);
                if self.idle > IDLE_LIMIT {
                    self.release();
                }
                return;
            }
        }
        let n = out.len().min(self.buffered());
        for o in out[..n].iter_mut() {
            let s = self.buf[self.rd & (CAP - 1)] as f32 * self.gain;
            self.rd = self.rd.wrapping_add(1);
            *o = (*o as i32 + s as i32).clamp(-32000, 32000) as i16;
        }
        if n < out.len() {
            // Ran dry: wait for the next frames to build up again.
            self.playing = false;
            self.idle = (out.len() - n) as u32;
        }
    }
}

impl Default for VoiceStream {
    fn default() -> Self {
        VoiceStream::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_waits_for_the_jitter_buffer_then_plays_in_order() {
        let mut s = VoiceStream::new();
        s.claim(7);
        let frame: alloc::vec::Vec<i16> = (0..441).map(|i| (i % 100) as i16 * 10).collect();
        s.push(&frame, 1.0, 0.0);
        let mut out = [0i16; 100];
        s.mix_into(&mut out);
        assert!(out.iter().all(|&x| x == 0), "one frame is not enough to start");
        s.push(&frame, 1.0, 0.0);
        s.mix_into(&mut out);
        assert_eq!(&out[..5], &[0, 10, 20, 30, 40]);
        assert_eq!(s.buffered(), 2 * 441 - 100);
    }

    #[test]
    fn gain_scales_and_mixing_adds_without_overflow() {
        let mut s = VoiceStream::new();
        s.claim(1);
        s.push(&[10_000; 1000], 0.5, 0.0);
        let mut out = [30_000i16; 10];
        s.mix_into(&mut out);
        assert!(out.iter().all(|&x| x == 32_000), "clamped");
        let mut quiet = [0i16; 10];
        s.mix_into(&mut quiet);
        assert!(quiet.iter().all(|&x| x == 5_000));
    }

    #[test]
    fn running_dry_rebuffers_and_a_long_silence_frees_the_slot() {
        let mut s = VoiceStream::new();
        s.claim(3);
        s.push(&[1000; 1000], 1.0, 0.0);
        let mut out = [0i16; 600];
        s.mix_into(&mut out);
        s.mix_into(&mut out); // only 400 left: runs dry
        assert_eq!(s.buffered(), 0);
        // Not enough yet to start again.
        s.push(&[1000; 300], 1.0, 0.0);
        let mut again = [0i16; 100];
        s.mix_into(&mut again);
        assert!(again.iter().all(|&x| x == 0));
        // Then nothing more arrives.
        let mut big = [0i16; 4096];
        for _ in 0..8 {
            s.mix_into(&mut big);
        }
        assert!(s.free());
    }

    #[test]
    fn a_burst_keeps_the_newest_and_the_delay_bounded() {
        let mut s = VoiceStream::new();
        s.claim(2);
        let long: alloc::vec::Vec<i16> = (0..10_000).map(|i| (i % 30_000) as i16).collect();
        s.push(&long, 1.0, 0.0);
        assert_eq!(s.buffered(), CAP);
        let mut out = [0i16; 4];
        s.mix_into(&mut out);
        assert_eq!(out[0], ((10_000 - CAP) % 30_000) as i16);
        // Pushing in pieces past the end keeps order too.
        let mut t = VoiceStream::new();
        t.claim(2);
        for k in 0..50 {
            let piece: alloc::vec::Vec<i16> = (0..441).map(|i| (k * 441 + i) as i16).collect();
            t.push(&piece, 1.0, 0.0);
        }
        assert!(t.buffered() <= CAP);
        let mut o = [0i16; 2];
        t.mix_into(&mut o);
        assert_eq!(o[1] - o[0], 1);
    }
}
