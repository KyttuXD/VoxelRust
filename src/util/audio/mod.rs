//! Reading sound files: WAV (PCM, float, A-law, mu-law, ADPCM), AIFF, FLAC, Ogg Vorbis and MP3,
//! any number of channels and any rate. The result is what the mixer plays:
//! one channel of 16-bit samples at [`RATE`].

use alloc::vec::Vec;

pub mod clip;

// What mods use: playing and controlling sounds and music (docs/SOUND.md, "Sound for mods").
pub mod fade;
pub mod handles;
pub mod loopseam;
pub mod music_feed;
pub mod resample;
pub mod spatial;
pub(crate) mod music_source;

// How the game sounds around you: see docs/SOUND.md.
pub mod ambience;
pub mod bed;
pub mod grains;
pub mod limiter;
pub mod rand;
pub mod variation;

/// The rate the mixer runs at.
pub const RATE: u32 = 22050;
/// Longest sound taken, in seconds, and biggest file, in bytes.
pub const MAX_SECONDS: u32 = 12;
pub const MAX_BYTES: usize = 8 << 20;

pub(super) fn u16le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

pub(super) fn u32le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(at)?, *b.get(at + 1)?, *b.get(at + 2)?, *b.get(at + 3)?]))
}

/// Sound the mixer can play, from a file of any kind this reads: WAV (PCM of
/// 8, 16, 24 or 32 bits, 32-bit float, A-law, mu-law, MS ADPCM, IMA ADPCM),
/// AIFF and AIFF-C (PCM), FLAC, Ogg Vorbis, and MP3 (MPEG-1/2/2.5 layer III). Any number of channels (averaged), any
/// rate (resampled). `None` if it is none of those, damaged, or too long.
pub fn decode(bytes: &[u8]) -> Option<Vec<i16>> {
    if bytes.len() > MAX_BYTES || bytes.len() < 16 {
        return None;
    }
    let pcm = if &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        wav(bytes)?
    } else if &bytes[0..4] == b"FORM" && (&bytes[8..12] == b"AIFF" || &bytes[8..12] == b"AIFC") {
        aiff(bytes)?
    } else if &bytes[0..4] == b"fLaC" {
        flac(bytes)?
    } else if &bytes[0..4] == b"OggS" {
        ogg(bytes)?
    } else if mp3::is_mp3(bytes) {
        mp3::mp3(bytes)?
    } else {
        return None;
    };
    pcm.to_mixer()
}

/// What the readers give: interleaved samples in -1..1.
pub(super) struct Pcm {
    pub(super) rate: u32,
    pub(super) channels: usize,
    pub(super) data: Vec<f32>,
}

impl Pcm {
    fn to_mixer(self) -> Option<Vec<i16>> {
        let (rate, ch) = (self.rate, self.channels);
        if ch == 0 || ch > 8 || rate < 1000 || rate > 192_000 {
            return None;
        }
        let frames = self.data.len() / ch;
        if frames == 0 || frames as u64 > rate as u64 * MAX_SECONDS as u64 {
            return None;
        }
        let mono = |f: usize| -> f32 { self.data[f * ch..f * ch + ch].iter().sum::<f32>() / ch as f32 };
        let out_len = ((frames as u64 * RATE as u64) / rate as u64).max(1) as usize;
        let mut out = Vec::with_capacity(out_len);
        for i in 0..out_len {
            let pos = i as f64 * rate as f64 / RATE as f64;
            let k = pos as usize;
            let frac = (pos - k as f64) as f32;
            let (a, b) = (mono(k.min(frames - 1)), mono((k + 1).min(frames - 1)));
            out.push(((a + (b - a) * frac).clamp(-1.0, 1.0) * 32767.0) as i16);
        }
        Some(out)
    }
}


pub(crate) mod adpcm;
mod aiff;
mod flac;
mod ogg;
mod vorbis;
mod mp3;
mod ogg_stream;
mod wav;
mod wav_stream;

use aiff::aiff;
use flac::flac;
use ogg::ogg;
use wav::wav;

#[cfg(test)]
pub fn make_wav(rate: u32, channels: u16, bits: u16, pcm: &[u8]) -> Vec<u8> {
    let mut w = Vec::new();
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&((36 + pcm.len()) as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&channels.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * channels as u32 * bits as u32 / 8).to_le_bytes());
    w.extend_from_slice(&(channels * bits / 8).to_le_bytes());
    w.extend_from_slice(&bits.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(pcm);
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_16_bit_at_the_mixers_rate_comes_through_unchanged() {
        let pcm: Vec<u8> = [0i16, 1000, -1000, 32767].iter().flat_map(|v| v.to_le_bytes()).collect();
        let out = decode(&make_wav(RATE, 1, 16, &pcm)).unwrap();
        assert!(out.iter().zip([0i16, 1000, -1000, 32767]).all(|(a, b)| (a - b).abs() <= 1), "{out:?}");
    }

    #[test]
    fn stereo_is_averaged_and_other_rates_are_resampled() {
        // 44100 Hz stereo: left full, right silent, 8 frames -> 4 frames at 22050.
        let pcm: Vec<u8> = (0..8).flat_map(|_| [16384i16, 0].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>()).collect();
        let out = decode(&make_wav(44_100, 2, 16, &pcm)).unwrap();
        assert_eq!(out.len(), 4);
        assert!(out.iter().all(|&s| (s - 8192).abs() < 3), "{out:?}");
    }

    #[test]
    fn eight_and_twenty_four_bits_are_understood() {
        let out = decode(&make_wav(RATE, 1, 8, &[128, 255, 0])).unwrap();
        assert_eq!(out[0], 0);
        assert!(out[1] > 32000 && out[2] < -32000, "{out:?}");
        let pcm: Vec<u8> = [0x40_0000i32, -0x40_0000].iter().flat_map(|v| v.to_le_bytes()[..3].to_vec()).collect();
        let out = decode(&make_wav(RATE, 1, 24, &pcm)).unwrap();
        assert!((out[0] - 16383).abs() < 2 && (out[1] + 16384).abs() < 2, "{out:?}");
    }

    #[test]
    fn things_that_are_not_wav_files_are_refused() {
        assert!(decode(b"not a sound").is_none());
        assert!(decode(&[0u8; 100]).is_none());
        let mut w = make_wav(RATE, 1, 16, &[0, 0, 0, 0]);
        w[20] = 2; // ADPCM
        assert!(decode(&w).is_none());
        // Too long: a minute of silence.
        assert!(decode(&make_wav(RATE, 1, 16, &alloc::vec![0u8; RATE as usize * 2 * 60])).is_none());
    }

    #[test]
    fn a_flac_cut_off_in_its_header_or_with_a_wild_rate_is_refused() {
        // STREAMINFO announced (34 bytes) but not there.
        let mut f = alloc::vec::Vec::from(&b"fLaC"[..]);
        f.extend_from_slice(&[0, 0, 0, 34, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(decode(&f).is_none());
        // A rate of 1,048,575 Hz (the most 20 bits hold) is no sound the mixer can play.
        let mut f = alloc::vec::Vec::from(&b"fLaC"[..]);
        f.extend_from_slice(&[0x80, 0, 0, 34]);
        let mut info = [0u8; 34];
        info[10] = 0xFF;
        info[11] = 0xFF;
        info[12] = 0xF0; // rate's last 4 bits, 1 channel, bits per sample high bit
        info[13] = 0x00; // 1 bit a sample
        f.extend_from_slice(&info);
        f.extend_from_slice(&[0xFF, 0xF8, 0x69, 0x08, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(decode(&f).is_none());
    }

    #[test]
    fn a_microsoft_adpcm_file_with_wild_steps_does_not_overflow() {
        // One block of two channels' worth of header, then nibbles that make the step grow every time.
        let mut extra = alloc::vec![0u8, 0, 1, 0];
        extra.extend_from_slice(&[0xFF, 0x7F, 0xFF, 0x7F]); // one coefficient pair, both 32767
        let mut data = alloc::vec![0u8; 7];
        data[1] = 0xFF;
        data[2] = 0x7F; // delta 32767
        data[3] = 0xFF;
        data[4] = 0x7F; // sample 1
        data[5] = 0xFF;
        data[6] = 0x7F; // sample 2
        data.extend(core::iter::repeat(0x77u8).take(400));
        let mut out = alloc::vec::Vec::new();
        assert!(adpcm::ms_adpcm(&data, 1, 407, &extra, &mut out).is_some());
        assert!(out.iter().all(|v| v.abs() <= 1.0));
    }

    fn rms_error(a: &[i16], b: &[i16]) -> f64 {
        let n = a.len().min(b.len());
        let sum: f64 = (0..n).map(|i| (a[i] as f64 - b[i] as f64).powi(2)).sum();
        (sum / n as f64).sqrt()
    }

    #[test]
    fn flac_and_aiff_decode_exactly_what_the_wav_holds() {
        let want = decode(include_bytes!("testdata/ref.wav")).unwrap();
        for (name, file) in [("flac", &include_bytes!("testdata/tone.flac")[..]), ("aiff", &include_bytes!("testdata/tone.aiff")[..])] {
            let got = decode(file).unwrap_or_else(|| panic!("{name} not read"));
            assert_eq!(got, want, "{name}");
        }
        // Stereo (joint) FLAC at another rate.
        let want = decode(include_bytes!("testdata/stereo_ref.wav")).unwrap();
        let got = decode(include_bytes!("testdata/stereo.flac")).expect("stereo flac");
        assert_eq!(got.len(), want.len());
        assert!(got.iter().zip(&want).all(|(a, b)| (a - b).abs() <= 2), "stereo flac differs");
    }

    #[test]
    fn the_older_wav_codecs_come_out_close() {
        let want = decode(include_bytes!("testdata/ref.wav")).unwrap();
        for (name, file, limit) in [
            ("ms adpcm", &include_bytes!("testdata/tone_ms.wav")[..], 1500.0),
            ("ima adpcm", &include_bytes!("testdata/tone_ima.wav")[..], 1500.0),
            ("a-law", &include_bytes!("testdata/tone_alaw.wav")[..], 600.0),
            ("mu-law", &include_bytes!("testdata/tone_ulaw.wav")[..], 600.0),
        ] {
            let got = decode(file).unwrap_or_else(|| panic!("{name} not read"));
            assert!(got.len() + 40 >= want.len() && got.len() <= want.len() + 40, "{name}: {} vs {}", got.len(), want.len());
            let e = rms_error(&got, &want);
            assert!(e < limit, "{name}: error {e}");
        }
    }
}
