//! Reading sound files: WAV (PCM of 8, 16, 24 or 32 bits, or 32-bit float),
//! any number of channels and any rate. The result is what the mixer plays:
//! one channel of 16-bit samples at [`RATE`].

use alloc::vec::Vec;

/// The rate the mixer runs at.
pub const RATE: u32 = 22050;
/// Longest sound taken, in seconds, and biggest file, in bytes.
pub const MAX_SECONDS: u32 = 12;
pub const MAX_BYTES: usize = 8 << 20;

fn u16le(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

fn u32le(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(at)?, *b.get(at + 1)?, *b.get(at + 2)?, *b.get(at + 3)?]))
}

/// Decode a WAV file into mono samples at [`RATE`]. `None` if it is not a
/// WAV file of a kind this reads, or is too long.
pub fn decode(bytes: &[u8]) -> Option<Vec<i16>> {
    if bytes.len() > MAX_BYTES || bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let (mut format, mut channels, mut rate, mut bits) = (0u16, 0u16, 0u32, 0u16);
    let mut data: Option<&[u8]> = None;
    let mut at = 12;
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
            // WAVE_FORMAT_EXTENSIBLE: the real format is the first two bytes of the sub-format.
            if format == 0xFFFE && end - body >= 26 {
                format = u16le(bytes, body + 24)?;
            }
        } else if id == b"data" {
            data = Some(&bytes[body..end]);
        }
        at = body.saturating_add(size).saturating_add(size & 1);
    }
    let data = data?;
    if channels == 0 || channels > 8 || rate < 1000 || rate > 192_000 {
        return None;
    }
    let width = (bits / 8) as usize;
    let ok = matches!((format, bits), (1, 8) | (1, 16) | (1, 24) | (1, 32) | (3, 32));
    if !ok {
        return None;
    }
    let frame = width * channels as usize;
    let frames = data.len() / frame;
    if frames == 0 || frames as u64 > rate as u64 * MAX_SECONDS as u64 {
        return None;
    }
    // One value in -1..1 per frame: the channels averaged.
    let sample = |f: usize, c: usize| -> f32 {
        let p = f * frame + c * width;
        match (format, bits) {
            (1, 8) => (data[p] as f32 - 128.0) / 128.0,
            (1, 16) => i16::from_le_bytes([data[p], data[p + 1]]) as f32 / 32768.0,
            (1, 24) => (((data[p] as i32) << 8 | (data[p + 1] as i32) << 16 | (data[p + 2] as i32) << 24) >> 8) as f32 / 8_388_608.0,
            (1, _) => i32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]) as f32 / 2_147_483_648.0,
            _ => f32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]),
        }
    };
    let mono = |f: usize| -> f32 {
        let mut s = 0.0;
        for c in 0..channels as usize {
            s += sample(f, c);
        }
        s / channels as f32
    };
    // Resample to the mixer's rate by linear interpolation.
    let out_len = ((frames as u64 * RATE as u64) / rate as u64).max(1) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = i as f64 * rate as f64 / RATE as f64;
        let k = pos as usize;
        let frac = (pos - k as f64) as f32;
        let a = mono(k.min(frames - 1));
        let b = mono((k + 1).min(frames - 1));
        let v = (a + (b - a) * frac).clamp(-1.0, 1.0);
        out.push((v * 32767.0) as i16);
    }
    Some(out)
}

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
}
