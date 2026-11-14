//! WAV files: PCM, float, A-law, mu-law, and the ADPCM codecs of `adpcm`.

use super::adpcm::{ima_adpcm, ms_adpcm};
use super::{u16le, u32le, Pcm};
use alloc::vec::Vec;

pub(super) fn wav(bytes: &[u8]) -> Option<Pcm> {
    let (mut format, mut channels, mut rate, mut bits, mut align) = (0u16, 0u16, 0u32, 0u16, 0u16);
    let mut fmt_extra: &[u8] = &[];
    let mut data: Option<&[u8]> = None;
    let mut fact: Option<usize> = None;
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
            align = u16le(bytes, body + 12)?;
            bits = u16le(bytes, body + 14)?;
            fmt_extra = bytes.get(body + 18..end).unwrap_or(&[]);
            // WAVE_FORMAT_EXTENSIBLE: the real format is the first two bytes of the sub-format.
            if format == 0xFFFE && end - body >= 26 {
                format = u16le(bytes, body + 24)?;
            }
        } else if id == b"fact" && end - body >= 4 {
            fact = Some(u32le(bytes, body)? as usize);
        } else if id == b"data" {
            data = Some(&bytes[body..end]);
        }
        at = body.saturating_add(size).saturating_add(size & 1);
    }
    let data = data?;
    let ch = channels as usize;
    if ch == 0 || ch > 8 {
        return None;
    }
    let mut out: Vec<f32> = Vec::new();
    match (format, bits) {
        (1, 8) | (1, 16) | (1, 24) | (1, 32) | (3, 32) => {
            let width = (bits / 8) as usize;
            let n = data.len() / width;
            out.reserve(n);
            for k in 0..n {
                let p = k * width;
                out.push(match (format, bits) {
                    (1, 8) => (data[p] as f32 - 128.0) / 128.0,
                    (1, 16) => i16::from_le_bytes([data[p], data[p + 1]]) as f32 / 32768.0,
                    (1, 24) => (((data[p] as i32) << 8 | (data[p + 1] as i32) << 16 | (data[p + 2] as i32) << 24) >> 8) as f32 / 8_388_608.0,
                    (1, _) => i32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]) as f32 / 2_147_483_648.0,
                    _ => f32::from_le_bytes([data[p], data[p + 1], data[p + 2], data[p + 3]]),
                });
            }
        }
        (6, 8) => out.extend(data.iter().map(|&b| alaw(b) as f32 / 32768.0)),
        (7, 8) => out.extend(data.iter().map(|&b| ulaw(b) as f32 / 32768.0)),
        (0x11, 4) => ima_adpcm(data, ch, align as usize, &mut out)?,
        (2, 4) => ms_adpcm(data, ch, align as usize, fmt_extra, &mut out)?,
        _ => return None,
    }
    // The last block of an ADPCM file is padded: the fact chunk says how many frames are real.
    if let Some(f) = fact.filter(|&f| f > 0 && matches!(format, 2 | 0x11)) {
        out.truncate(f.saturating_mul(ch));
    }
    Some(Pcm { rate, channels: ch, data: out })
}

pub(super) fn alaw(b: u8) -> i16 {
    let b = b ^ 0x55;
    let (seg, mant) = (((b & 0x70) >> 4) as i32, (b & 0x0F) as i32);
    let mut v = (mant << 4) + 8;
    if seg > 0 {
        v = (v + 0x100) << (seg - 1);
    }
    (if b & 0x80 != 0 { v } else { -v }) as i16
}

pub(super) fn ulaw(b: u8) -> i16 {
    let b = !b;
    let v = (((((b & 0x0F) as i32) << 3) + 0x84) << ((b & 0x70) >> 4)) - 0x84;
    (if b & 0x80 != 0 { -v } else { v }) as i16
}
