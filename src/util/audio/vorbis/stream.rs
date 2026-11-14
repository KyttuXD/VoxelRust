//! The Vorbis decoder one packet at a time, for music that is played as it
//! is decoded (see `music_source.rs`). It is the same decoder `decode` runs,
//! so the samples are the same.

use super::{header_start, identification, parse_setup, slope, Decoder, Mdct};
use alloc::vec;
use alloc::vec::Vec;

pub(crate) struct Stream {
    d: Decoder,
    pub(crate) rate: u32,
    pub(crate) channels: usize,
}

impl Stream {
    /// Start a stream from its three header packets.
    pub(crate) fn new(p0: &[u8], p1: &[u8], p2: &[u8]) -> Option<Stream> {
        let (channels, rate, bs0, bs1) = identification(p0)?;
        if !header_start(p1, 3) {
            return None;
        }
        let setup = parse_setup(p2, channels)?;
        let d = Decoder {
            channels,
            bs0,
            bs1,
            setup,
            mdct0: Mdct::new(bs0),
            mdct1: Mdct::new(bs1),
            slope0: slope(bs0 / 2),
            slope1: slope(bs1 / 2),
            db: super::floor::inverse_db_table(),
            pending: vec![Vec::new(); channels],
            prev_n: 0,
        };
        Some(Stream { d, rate, channels })
    }

    /// Decode one audio packet and append the samples it completes (channels
    /// interleaved, -1..1). `false` if the packet is damaged (nothing is
    /// appended and the stream goes on with the next).
    pub(crate) fn block(&mut self, pkt: &[u8], out: &mut Vec<f32>) -> bool {
        let before = out.len();
        let ok = self.d.block(pkt, out).is_some();
        if !ok {
            out.truncate(before);
        }
        ok
    }
}
