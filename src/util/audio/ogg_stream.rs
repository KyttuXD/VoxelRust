//! An Ogg Vorbis file read page by page and decoded packet by packet, for
//! music (see `music_source.rs`): the compressed file stays in memory, the
//! samples exist only a block at a time.

use super::vorbis::stream::Stream;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

/// The packets of the first logical stream of an Ogg file, one page read at
/// a time (as `ogg.rs` reads them all at once).
struct Packets {
    at: usize,
    serial: Option<u32>,
    cur: Vec<u8>,
    queue: VecDeque<Vec<u8>>,
    done: bool,
}

impl Packets {
    fn new() -> Packets {
        Packets { at: 0, serial: None, cur: Vec::new(), queue: VecDeque::new(), done: false }
    }

    /// The next packet, reading pages as needed.
    fn next(&mut self, bytes: &[u8]) -> Option<Vec<u8>> {
        loop {
            if let Some(p) = self.queue.pop_front() {
                return Some(p);
            }
            if self.done {
                return None;
            }
            self.page(bytes);
        }
    }

    /// Read the page at `at`: the packets it completes go to the queue.
    fn page(&mut self, b: &[u8]) {
        let at = self.at;
        if at + 27 > b.len() || &b[at..at + 4] != b"OggS" || b[at + 4] != 0 {
            self.done = true;
            return;
        }
        let kind = b[at + 5];
        let ser = u32::from_le_bytes([b[at + 14], b[at + 15], b[at + 16], b[at + 17]]);
        let nsegs = b[at + 26] as usize;
        let table = at + 27;
        let Some(lacing) = b.get(table..table + nsegs) else {
            self.done = true;
            return;
        };
        let body: usize = lacing.iter().map(|&l| l as usize).sum();
        let start = table + nsegs;
        let Some(data) = b.get(start..start + body) else {
            self.done = true; // cut short
            return;
        };
        self.at = start + body;
        match self.serial {
            None => self.serial = Some(ser),
            Some(s) if s != ser => return, // another stream in the same file
            _ => {}
        }
        if kind & 1 == 0 {
            self.cur.clear(); // a packet that should have continued from a lost page
        }
        let mut p = 0usize;
        for &l in lacing {
            let l = l as usize;
            self.cur.extend_from_slice(&data[p..p + l]);
            p += l;
            if l < 255 {
                self.queue.push_back(core::mem::take(&mut self.cur));
            }
        }
        if kind & 4 != 0 {
            self.done = true;
        }
    }
}

pub(crate) struct OggStream {
    bytes: Vec<u8>,
    packets: Packets,
    vorbis: Stream,
    pub(crate) rate: u32,
    pub(crate) channels: usize,
}

impl OggStream {
    /// Open a stream on a whole file's bytes (kept, compressed, while it plays).
    pub(crate) fn open(bytes: Vec<u8>) -> Option<OggStream> {
        let (packets, vorbis) = Self::start(&bytes)?;
        let (rate, channels) = (vorbis.rate, vorbis.channels);
        if channels == 0 || channels > 8 || rate < 1000 || rate > 192_000 {
            return None;
        }
        Some(OggStream { bytes, packets, vorbis, rate, channels })
    }

    fn start(bytes: &[u8]) -> Option<(Packets, Stream)> {
        let mut packets = Packets::new();
        let p0 = packets.next(bytes)?;
        let p1 = packets.next(bytes)?;
        let p2 = packets.next(bytes)?;
        let vorbis = Stream::new(&p0, &p1, &p2)?;
        Some((packets, vorbis))
    }

    /// Bytes of the compressed file held.
    pub(crate) fn size(&self) -> usize {
        self.bytes.len()
    }

    /// Go back to the start of the audio.
    pub(crate) fn rewind(&mut self) -> bool {
        match Self::start(&self.bytes) {
            Some((p, v)) => {
                self.packets = p;
                self.vorbis = v;
                true
            }
            None => false,
        }
    }

    /// Decode packets until one gives samples; append them (interleaved,
    /// -1..1). `false` at the end of the file.
    pub(crate) fn next_block(&mut self, out: &mut Vec<f32>) -> bool {
        loop {
            let Some(pkt) = self.packets.next(&self.bytes) else { return false };
            let before = out.len();
            self.vorbis.block(&pkt, out);
            if out.len() > before {
                return true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILES: [&[u8]; 4] = [
        include_bytes!("testdata/mono22.ogg"),
        include_bytes!("testdata/stereo22.ogg"),
        include_bytes!("testdata/mono44.ogg"),
        include_bytes!("testdata/stereo44.ogg"),
    ];

    #[test]
    fn decoding_in_blocks_gives_the_samples_decoding_the_whole_file_does() {
        for (n, file) in FILES.iter().enumerate() {
            let whole = super::super::ogg::ogg(file).expect("whole");
            let mut s = OggStream::open(file.to_vec()).expect("stream");
            assert_eq!((s.rate, s.channels), (whole.rate, whole.channels), "file {n}");
            let mut got: Vec<f32> = Vec::new();
            let mut blocks = 0;
            while s.next_block(&mut got) {
                blocks += 1;
            }
            assert!(blocks > 3, "file {n}: only {blocks} blocks");
            // The whole-file decode is cut at the length the container says; the stream plays on to the end of its last block.
            assert!(got.len() >= whole.data.len(), "file {n}");
            assert_eq!(&got[..whole.data.len()], &whole.data[..], "file {n}");
        }
    }

    #[test]
    fn rewinding_starts_over_with_the_same_samples() {
        let mut s = OggStream::open(FILES[1].to_vec()).unwrap();
        let mut first: Vec<f32> = Vec::new();
        for _ in 0..5 {
            assert!(s.next_block(&mut first));
        }
        while s.next_block(&mut Vec::new()) {}
        assert!(s.rewind());
        let mut again: Vec<f32> = Vec::new();
        for _ in 0..5 {
            assert!(s.next_block(&mut again));
        }
        assert_eq!(first, again);
    }

    #[test]
    fn not_a_vorbis_file_is_refused_and_damage_ends_the_stream_without_a_panic() {
        assert!(OggStream::open(b"OggS".to_vec()).is_none());
        assert!(OggStream::open(alloc::vec![0u8; 400]).is_none());
        let f = FILES[0];
        for cut in [30usize, 100, 500, f.len() / 2] {
            if let Some(mut s) = OggStream::open(f[..cut.min(f.len())].to_vec()) {
                let mut out = Vec::new();
                while s.next_block(&mut out) {}
            }
        }
        let mut copy = f.to_vec();
        for i in 0..64 {
            copy[f.len() / 2 + i] = (i * 37) as u8;
        }
        if let Some(mut s) = OggStream::open(copy) {
            let mut out = Vec::new();
            while s.next_block(&mut out) {}
        }
    }
}
