//! A WAV file of plain PCM (8, 16, 24 or 32 bits, or 32-bit float) read a
//! block of frames at a time, for music (see `music_source.rs`). The other
//! WAV codecs (ADPCM, A-law, mu-law) are short-sound formats; the music
//! reader plays those through the whole-file decoder when they are short
//! enough.

use super::{u16le, u32le};
use alloc::vec::Vec;

/// Frames decoded per block.
const BLOCK_FRAMES: usize = 1024;

pub(crate) struct WavStream {
    bytes: Vec<u8>,
    /// The sample data: where it starts and how long it is, in bytes.
    data: usize,
    len: usize,
    /// Bytes read so far.
    at: usize,
    format: u16,
    bits: u16,
    pub(crate) rate: u32,
    pub(crate) channels: usize,
}

impl WavStream {
    /// Whether `open` can read these bytes.
    pub(crate) fn readable(bytes: &[u8]) -> bool {
        Self::parse(bytes).is_some()
    }

    pub(crate) fn open(bytes: Vec<u8>) -> Option<WavStream> {
        let (format, channels, rate, bits, data, len) = Self::parse(&bytes)?;
        Some(WavStream { bytes, data, len, at: 0, format, bits, rate, channels })
    }

    /// The format, channels, rate, bits, and where the sample data is and how long.
    fn parse(bytes: &[u8]) -> Option<(u16, usize, u32, u16, usize, usize)> {
        if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return None;
        }
        let (mut format, mut channels, mut rate, mut bits) = (0u16, 0u16, 0u32, 0u16);
        let mut data: Option<(usize, usize)> = None;
        let mut at = 12usize;
        while at + 8 <= bytes.len() {
            let id = &bytes[at..at + 4];
            let size = u32le(bytes, at + 4)? as usize;
            let body = at + 8;
            let end = body.saturating_add(size).min(bytes.len());
            if id == b"fmt " && end - body >= 16 {
                format = u16le(bytes, body)?;
                channels = u16le(bytes, body + 2)?;
                rate = u32le(bytes, body + 4)?;
                bits = u16le(bytes, body + 14)?;
                if format == 0xFFFE && end - body >= 26 {
                    format = u16le(bytes, body + 24)?;
                }
            } else if id == b"data" {
                data = Some((body, end - body));
            }
            at = body.saturating_add(size).saturating_add(size & 1);
        }
        let (data, len) = data?;
        let ok = matches!((format, bits), (1, 8) | (1, 16) | (1, 24) | (1, 32) | (3, 32));
        if !ok || channels == 0 || channels > 8 || rate < 1000 || rate > 192_000 {
            return None;
        }
        Some((format, channels as usize, rate, bits, data, len))
    }

    /// Bytes of the compressed file held.
    pub(crate) fn size(&self) -> usize {
        self.bytes.len()
    }

    pub(crate) fn rewind(&mut self) {
        self.at = 0;
    }

    /// Append the next block of frames (interleaved, -1..1). `false` at the end.
    pub(crate) fn next_block(&mut self, out: &mut Vec<f32>) -> bool {
        let width = (self.bits / 8) as usize;
        let frame = width * self.channels;
        let frames = ((self.len - self.at) / frame).min(BLOCK_FRAMES);
        if frames == 0 {
            return false;
        }
        let base = self.data + self.at;
        out.reserve(frames * self.channels);
        for k in 0..frames * self.channels {
            let p = base + k * width;
            let d = &self.bytes[p..p + width];
            out.push(match (self.format, self.bits) {
                (1, 8) => (d[0] as f32 - 128.0) / 128.0,
                (1, 16) => i16::from_le_bytes([d[0], d[1]]) as f32 / 32768.0,
                (1, 24) => (((d[0] as i32) << 8 | (d[1] as i32) << 16 | (d[2] as i32) << 24) >> 8) as f32 / 8_388_608.0,
                (1, _) => i32::from_le_bytes([d[0], d[1], d[2], d[3]]) as f32 / 2_147_483_648.0,
                _ => f32::from_le_bytes([d[0], d[1], d[2], d[3]]),
            });
        }
        self.at += frames * frame;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::make_wav;

    fn tone16(frames: usize, channels: usize) -> Vec<u8> {
        let mut pcm = Vec::new();
        for i in 0..frames {
            for c in 0..channels {
                let v = (((i * (7 + c)) % 2000) as i32 - 1000) * 20;
                pcm.extend_from_slice(&(v as i16).to_le_bytes());
            }
        }
        make_wav(44_100, channels as u16, 16, &pcm)
    }

    #[test]
    fn blocks_put_together_are_the_whole_file() {
        for ch in [1usize, 2] {
            let file = tone16(5000, ch);
            let whole = super::super::wav::wav(&file).unwrap();
            let mut s = WavStream::open(file).unwrap();
            assert_eq!((s.rate, s.channels), (44_100, ch));
            let mut got = Vec::new();
            let mut blocks = 0;
            while s.next_block(&mut got) {
                blocks += 1;
            }
            assert_eq!(blocks, 5);
            assert_eq!(got, whole.data);
        }
    }

    #[test]
    fn other_widths_and_a_rewind() {
        let pcm: Vec<u8> = [0x40_0000i32, -0x40_0000].iter().flat_map(|v| v.to_le_bytes()[..3].to_vec()).collect();
        let mut s = WavStream::open(make_wav(22_050, 1, 24, &pcm)).unwrap();
        let mut a = Vec::new();
        assert!(s.next_block(&mut a));
        assert!(!s.next_block(&mut a));
        assert!((a[0] - 0.5).abs() < 1e-3 && (a[1] + 0.5).abs() < 1e-3);
        s.rewind();
        let mut b = Vec::new();
        assert!(s.next_block(&mut b));
        assert_eq!(a, b);
    }

    #[test]
    fn what_it_cannot_read_is_refused() {
        assert!(WavStream::open(alloc::vec![0u8; 100]).is_none());
        let mut adpcm = make_wav(22_050, 1, 16, &[0u8; 100]);
        adpcm[20] = 2;
        assert!(WavStream::open(adpcm).is_none());
        // A data chunk that claims more than the file holds is read as far as there is data.
        let mut w = make_wav(22_050, 1, 16, &[1u8; 100]);
        w[40] = 0xFF;
        w[41] = 0xFF;
        let mut s = WavStream::open(w).unwrap();
        let mut out = Vec::new();
        assert!(s.next_block(&mut out));
        assert_eq!(out.len(), 50);
    }
}
