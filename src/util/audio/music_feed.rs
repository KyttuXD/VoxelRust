//! Music on its way to the mixer: a [`MusicSource`] decoded a block at a
//! time and turned into stereo 16-bit frames at the mixer's rate.
//!
//! The feed holds only the samples between the play position and the end of
//! the last block it decoded (a few thousand frames), so a five minute track
//! costs its compressed file and a little more, never its decoded length.
//! The play position is kept as a 32.32 fixed-point number of source frames,
//! so the samples it makes do not depend on how many frames are asked for at
//! a time (a test checks that).

use super::music_source::MusicSource;
use alloc::vec::Vec;

pub struct MusicFeed {
    src: MusicSource,
    ch: usize,
    /// Source frames per output frame, 32.32.
    step: u64,
    /// Decoded source samples, interleaved, from the first frame still needed.
    buf: Vec<f32>,
    /// The play position in `buf`, in source frames, 32.32.
    pos: u64,
    /// The source has no more blocks (and is not looping).
    eof: bool,
    looping: bool,
}

impl MusicFeed {
    pub(crate) fn new(src: MusicSource, looping: bool) -> MusicFeed {
        let ch = src.channels().max(1);
        let step = ((src.rate().max(1) as u64) << 32) / super::RATE as u64;
        MusicFeed { src, ch, step: step.max(1), buf: Vec::new(), pos: 0, eof: false, looping }
    }

    /// Whether the track will go round again when it ends.
    pub fn set_looping(&mut self, on: bool) {
        // (A track that has already ended stays ended.)
        self.looping = on;
    }

    /// Nothing more will come out.
    pub fn finished(&self) -> bool {
        let frames = self.buf.len() / self.ch;
        self.eof && ((self.pos >> 32) as usize) + 1 >= frames
    }

    /// Bytes this holds: the compressed file and the decoded block.
    pub fn bytes(&self) -> usize {
        self.src.bytes() + self.buf.capacity() * 4
    }

    fn frame(&self, i: usize) -> (f32, f32) {
        let l = self.buf[i * self.ch];
        if self.ch == 1 {
            (l, l)
        } else {
            (l, self.buf[i * self.ch + 1])
        }
    }

    /// Throw away the frames behind the play position and decode the next block.
    fn refill(&mut self) {
        let i = ((self.pos >> 32) as usize).min(self.buf.len() / self.ch);
        self.buf.drain(..i * self.ch);
        self.pos -= (i as u64) << 32;
        if !self.src.next_block(&mut self.buf) {
            if self.looping && self.src.rewind() && self.src.next_block(&mut self.buf) {
                return;
            }
            self.eof = true;
        }
    }

    /// Make up to `max` frames, handing each (left, right) to `put`, decoding
    /// at most `blocks` blocks to do it. Returns how many were made; fewer
    /// than `max` means the decoding budget ran out or the track ended
    /// ([`MusicFeed::finished`]).
    pub fn fill(&mut self, max: usize, blocks: usize, put: &mut dyn FnMut(i16, i16)) -> usize {
        let mut made = 0;
        let mut budget = blocks;
        while made < max {
            let i = (self.pos >> 32) as usize;
            if i + 1 >= self.buf.len() / self.ch {
                if self.eof || budget == 0 {
                    break;
                }
                budget -= 1;
                self.refill();
                continue;
            }
            let frac = (self.pos & 0xFFFF_FFFF) as f32 / 4_294_967_296.0;
            let (l0, r0) = self.frame(i);
            let (l1, r1) = self.frame(i + 1);
            let l = (l0 + (l1 - l0) * frac).clamp(-1.0, 1.0);
            let r = (r0 + (r1 - r0) * frac).clamp(-1.0, 1.0);
            put((l * 32767.0) as i16, (r * 32767.0) as i16);
            self.pos += self.step;
            made += 1;
        }
        made
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::make_wav;

    fn wav_source(rate: u32, channels: usize, frames: usize) -> MusicSource {
        let mut pcm = Vec::new();
        for i in 0..frames {
            for c in 0..channels {
                let v = (((i * (5 + 3 * c)) % 4000) as i32 - 2000) * 10;
                pcm.extend_from_slice(&(v as i16).to_le_bytes());
            }
        }
        MusicSource::open(make_wav(rate, channels as u16, 16, &pcm)).unwrap()
    }

    fn all(feed: &mut MusicFeed, chunk: usize) -> Vec<(i16, i16)> {
        let mut out = Vec::new();
        loop {
            let n = feed.fill(chunk, 1000, &mut |l, r| out.push((l, r)));
            if n == 0 {
                break;
            }
        }
        out
    }

    #[test]
    fn the_samples_do_not_depend_on_how_many_are_asked_for_at_a_time() {
        let whole = all(&mut MusicFeed::new(wav_source(44_100, 2, 20_000), false), 100_000);
        for chunk in [1usize, 7, 100, 333, 4096] {
            let got = all(&mut MusicFeed::new(wav_source(44_100, 2, 20_000), false), chunk);
            assert_eq!(got, whole, "chunk {chunk}");
        }
        // And the same with a tight decoding budget.
        let mut f = MusicFeed::new(wav_source(44_100, 2, 20_000), false);
        let mut slow = Vec::new();
        let mut idle = 0;
        while !f.finished() && idle < 10_000 {
            if f.fill(50, 1, &mut |l, r| slow.push((l, r))) == 0 {
                idle += 1;
            }
        }
        assert_eq!(slow, whole);
    }

    #[test]
    fn a_track_at_twice_the_rate_is_half_as_many_frames() {
        let out = all(&mut MusicFeed::new(wav_source(44_100, 1, 10_000), false), 512);
        assert!((out.len() as i64 - 5000).abs() <= 2, "{}", out.len());
        let same = all(&mut MusicFeed::new(wav_source(22_050, 1, 10_000), false), 512);
        assert!((same.len() as i64 - 10_000).abs() <= 2, "{}", same.len());
    }

    #[test]
    fn at_the_mixers_rate_the_samples_come_through_unchanged() {
        let src = wav_source(22_050, 1, 3000);
        let want: Vec<i16> = (0..3000).map(|i| ((((i * 5) % 4000) as i32 - 2000) * 10) as i16).collect();
        let out = all(&mut MusicFeed::new(src, false), 700);
        for (i, f) in out.iter().enumerate() {
            assert!((f.0 as i32 - want[i] as i32).abs() <= 1 && f.0 == f.1, "frame {i}: {f:?} against {}", want[i]);
        }
    }

    #[test]
    fn a_loop_goes_on_past_the_end_and_a_plain_track_stops() {
        let mut looped = MusicFeed::new(wav_source(22_050, 1, 3000), true);
        let mut n = 0;
        for _ in 0..40 {
            n += looped.fill(500, 1000, &mut |_, _| {});
        }
        assert_eq!(n, 20_000);
        assert!(!looped.finished());
        let mut once = MusicFeed::new(wav_source(22_050, 1, 3000), false);
        let mut m = 0;
        for _ in 0..40 {
            m += once.fill(500, 1000, &mut |_, _| {});
        }
        assert!(m <= 3000 && m >= 2990, "{m}");
        assert!(once.finished());
    }

    #[test]
    fn what_the_feed_holds_stays_small() {
        // A long track: 60 seconds at 44.1 kHz stereo is far more than a block.
        let mut f = MusicFeed::new(wav_source(44_100, 2, 44_100 * 60), false);
        let mut peak = 0;
        let mut total = 0;
        while !f.finished() {
            total += f.fill(2000, 1000, &mut |_, _| {});
            peak = peak.max(f.buf.len());
        }
        assert!(total > 1_300_000);
        // A few blocks of 1024 frames at most, however long the track.
        assert!(peak <= 4 * 1024 * 2 + 4096, "{peak}");
    }
}
