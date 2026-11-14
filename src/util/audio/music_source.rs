//! A music file opened for playing as it is decoded.
//!
//! Sound effects are decoded whole into one channel at the mixer's rate
//! (`decode`, at most 12 seconds). A music track is minutes long, so it is
//! never decoded whole: the file's compressed bytes are kept (a few MB) and
//! a block is decoded at a time (an Ogg packet, an MP3 frame, 1024 frames of
//! a WAV) when the player is running short. Ogg Vorbis, MP3 and PCM WAV
//! stream; any other kind the sound reader knows (FLAC, AIFF, ADPCM...) plays
//! through the whole-file decoder, so it must be short (12 seconds).

use super::mp3::stream::Mp3Stream;
use super::ogg_stream::OggStream;
use super::wav_stream::WavStream;
use alloc::vec::Vec;

/// Biggest music file taken, in bytes. A longer one is refused (it would
/// cost that much memory while it plays).
pub const MAX_MUSIC_BYTES: usize = 16 << 20;

pub(crate) enum MusicSource {
    Ogg(OggStream),
    Mp3(Mp3Stream),
    Wav(WavStream),
    /// A short file of another kind, decoded whole: mono at the mixer's rate.
    Short { pcm: Vec<i16>, at: usize },
}

/// Samples per block of the short kind.
const SHORT_BLOCK: usize = 1024;

impl MusicSource {
    /// Open a music file's bytes. `None` if it is not a sound this reads, or
    /// it is too big.
    pub(crate) fn open(bytes: Vec<u8>) -> Option<MusicSource> {
        if bytes.len() < 16 || bytes.len() > MAX_MUSIC_BYTES {
            return None;
        }
        if &bytes[0..4] == b"OggS" {
            return OggStream::open(bytes).map(MusicSource::Ogg);
        }
        if &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" && WavStream::readable(&bytes) {
            return WavStream::open(bytes).map(MusicSource::Wav);
        }
        if bytes.starts_with(b"ID3") || (bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0) {
            return Mp3Stream::open(bytes).map(MusicSource::Mp3);
        }
        // Whole-file decoding keeps its own limit (MAX_BYTES, 12 seconds).
        let pcm = super::decode(&bytes)?;
        Some(MusicSource::Short { pcm, at: 0 })
    }

    /// Samples a second per channel in the blocks.
    pub(crate) fn rate(&self) -> u32 {
        match self {
            MusicSource::Ogg(s) => s.rate,
            MusicSource::Mp3(s) => s.rate,
            MusicSource::Wav(s) => s.rate,
            MusicSource::Short { .. } => super::RATE,
        }
    }

    pub(crate) fn channels(&self) -> usize {
        match self {
            MusicSource::Ogg(s) => s.channels,
            MusicSource::Mp3(s) => s.channels,
            MusicSource::Wav(s) => s.channels,
            MusicSource::Short { .. } => 1,
        }
    }

    /// Decode the next block and append it (interleaved, -1..1). `false`
    /// when the file has no more.
    pub(crate) fn next_block(&mut self, out: &mut Vec<f32>) -> bool {
        match self {
            MusicSource::Ogg(s) => s.next_block(out),
            MusicSource::Mp3(s) => s.next_block(out),
            MusicSource::Wav(s) => s.next_block(out),
            MusicSource::Short { pcm, at } => {
                if *at >= pcm.len() {
                    return false;
                }
                let end = (*at + SHORT_BLOCK).min(pcm.len());
                out.extend(pcm[*at..end].iter().map(|&v| v as f32 / 32768.0));
                *at = end;
                true
            }
        }
    }

    /// Start again from the beginning (for a loop). `false` if that failed.
    pub(crate) fn rewind(&mut self) -> bool {
        match self {
            MusicSource::Ogg(s) => s.rewind(),
            MusicSource::Mp3(s) => {
                s.rewind();
                true
            }
            MusicSource::Wav(s) => {
                s.rewind();
                true
            }
            MusicSource::Short { at, .. } => {
                *at = 0;
                true
            }
        }
    }

    /// Bytes this holds (the compressed file, or the short decoded sound).
    pub(crate) fn bytes(&self) -> usize {
        match self {
            MusicSource::Ogg(s) => s.size(),
            MusicSource::Mp3(s) => s.size(),
            MusicSource::Wav(s) => s.size(),
            MusicSource::Short { pcm, .. } => pcm.len() * 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_is_recognised_and_junk_is_not() {
        let ogg = MusicSource::open(include_bytes!("testdata/stereo44.ogg").to_vec()).unwrap();
        assert!(matches!(ogg, MusicSource::Ogg(_)));
        assert_eq!((ogg.rate(), ogg.channels()), (44_100, 2));
        let mp3 = MusicSource::open(include_bytes!("testdata/mp3_s44.mp3").to_vec()).unwrap();
        assert!(matches!(mp3, MusicSource::Mp3(_)));
        assert_eq!((mp3.rate(), mp3.channels()), (44_100, 2));
        let wav = MusicSource::open(super::super::make_wav(22_050, 1, 16, &[0u8; 400])).unwrap();
        assert!(matches!(wav, MusicSource::Wav(_)));
        let flac = MusicSource::open(include_bytes!("testdata/tone.flac").to_vec()).unwrap();
        assert!(matches!(flac, MusicSource::Short { .. }));
        assert!(MusicSource::open(b"definitely not a sound file".to_vec()).is_none());
        assert!(MusicSource::open(alloc::vec![0u8; 8]).is_none());
        assert!(MusicSource::open(alloc::vec![0u8; MAX_MUSIC_BYTES + 1]).is_none());
    }

    #[test]
    fn an_mp3_in_blocks_matches_the_whole_decode_after_the_encoders_start() {
        // The whole decoder also cuts the padding at the end; the stream plays all of the last frame.
        let file = include_bytes!("testdata/mp3_s44.mp3");
        let whole = super::super::mp3::mp3(file).expect("whole");
        let mut s = MusicSource::open(file.to_vec()).unwrap();
        let mut got = Vec::new();
        let mut blocks = 0;
        while s.next_block(&mut got) {
            blocks += 1;
        }
        assert!(blocks > 5);
        assert!(got.len() >= whole.data.len(), "{} against {}", got.len(), whole.data.len());
        assert_eq!(&got[..whole.data.len()], &whole.data[..]);
    }

    #[test]
    fn rewinding_each_kind_starts_over() {
        for file in [&include_bytes!("testdata/stereo22.ogg")[..], &include_bytes!("testdata/mp3_m22.mp3")[..], &include_bytes!("testdata/tone.flac")[..]] {
            let mut s = MusicSource::open(file.to_vec()).unwrap();
            let mut a = Vec::new();
            assert!(s.next_block(&mut a));
            while s.next_block(&mut Vec::new()) {}
            assert!(s.rewind());
            let mut b = Vec::new();
            assert!(s.next_block(&mut b));
            assert_eq!(a, b);
        }
    }

    #[test]
    fn a_short_file_comes_in_blocks_of_its_samples() {
        let samples: Vec<i16> = (0..2500).map(|i| i as i16).collect();
        let mut s = MusicSource::Short { pcm: samples, at: 0 };
        let mut out = Vec::new();
        let mut n = 0;
        while s.next_block(&mut out) {
            n += 1;
        }
        assert_eq!((n, out.len()), (3, 2500));
        assert_eq!(s.bytes(), 5000);
    }
}
