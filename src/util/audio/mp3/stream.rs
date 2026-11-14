//! The MP3 decoder one frame at a time, for music that is played as it is
//! decoded (see `music_source.rs`). The frames are found and decoded exactly
//! as `mp3()` does; what differs is that the encoder's silence at the start
//! (from the LAME tag) is cut off and the padding at the end is not (a stream
//! does not know where its end is until it gets there).

use super::frame::Decoder;
use super::header::{self, Header};
use super::{find_frame, is_info_frame, lame_delays, skip_id3};
use alloc::vec;
use alloc::vec::Vec;

pub(crate) struct Mp3Stream {
    bytes: Vec<u8>,
    pos: usize,
    stream: Option<Header>,
    dec: Option<Decoder>,
    decoded: usize,
    frames_seen: usize,
    /// Frames (of samples) of the encoder's delay still to drop.
    skip: usize,
    pub(crate) rate: u32,
    pub(crate) channels: usize,
}

impl Mp3Stream {
    /// Start a stream on a whole file's bytes (kept, compressed, while it plays).
    pub(crate) fn open(bytes: Vec<u8>) -> Option<Mp3Stream> {
        let (_, h) = find_frame(&bytes, skip_id3(&bytes))?;
        let mut s = Mp3Stream { bytes, pos: 0, stream: None, dec: None, decoded: 0, frames_seen: 0, skip: 0, rate: h.rate, channels: h.channels };
        s.rewind();
        Some(s)
    }

    /// Bytes of the compressed file held.
    pub(crate) fn size(&self) -> usize {
        self.bytes.len()
    }

    /// Go back to the start of the audio.
    pub(crate) fn rewind(&mut self) {
        self.pos = skip_id3(&self.bytes);
        self.stream = None;
        self.dec = None;
        self.decoded = 0;
        self.frames_seen = 0;
        self.skip = 0;
    }

    /// Decode frames until one gives samples; append them (interleaved,
    /// -1..1). `false` at the end of the file.
    pub(crate) fn next_block(&mut self, out: &mut Vec<f32>) -> bool {
        loop {
            let b = &self.bytes[..];
            let found = match self.stream {
                Some(s) => match header::parse(b, self.pos) {
                    Some(h) if h.same_stream(&s) && self.pos + h.frame_len <= b.len() => Some((self.pos, h)),
                    _ => find_frame(b, self.pos),
                },
                None => find_frame(b, self.pos),
            };
            let Some((at, h)) = found else { return false };
            let s = *self.stream.get_or_insert(h);
            if !h.same_stream(&s) {
                self.pos = at + 1;
                continue;
            }
            self.pos = at + h.frame_len;
            self.frames_seen += 1;
            let frame = &b[at..self.pos];
            if is_info_frame(frame, &h) {
                if self.frames_seen == 1 {
                    if let Some((delay, _)) = lame_delays(frame, &h) {
                        self.skip = delay + 529;
                    }
                }
                continue;
            }
            let ch = s.channels.max(1);
            let before = out.len();
            let dec = self.dec.get_or_insert_with(Decoder::new);
            match dec.frame(&h, frame, out) {
                Some(()) => self.decoded += 1,
                None => {
                    // Keep the time right with silence, once there is sound to keep it with.
                    if self.decoded > 0 {
                        out.extend(vec![0.0f32; h.granules() * 576 * ch]);
                    }
                }
            }
            if self.skip > 0 {
                let n = (out.len() - before) / ch;
                let cut = self.skip.min(n);
                out.drain(before..before + cut * ch);
                self.skip -= cut;
            }
            if out.len() > before {
                return true;
            }
        }
    }
}
