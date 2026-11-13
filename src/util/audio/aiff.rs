//! AIFF and AIFF-C (PCM).

use super::Pcm;
use alloc::vec::Vec;

pub(super) fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

pub(super) fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(at)?, *b.get(at + 1)?, *b.get(at + 2)?, *b.get(at + 3)?]))
}

pub(super) fn aiff(bytes: &[u8]) -> Option<Pcm> {
    let aifc = &bytes[8..12] == b"AIFC";
    let (mut channels, mut bits, mut rate, mut little) = (0usize, 0u16, 0u32, false);
    let mut data: Option<&[u8]> = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = be32(bytes, at + 4)? as usize;
        let body = at + 8;
        let end = body.saturating_add(size).min(bytes.len());
        if id == b"COMM" && end - body >= 18 {
            channels = be16(bytes, body)? as usize;
            bits = be16(bytes, body + 6)?;
            // An 80-bit extended float: exponent (15 bits), then a 64-bit mantissa.
            let exp = (be16(bytes, body + 8)? & 0x7FFF) as i32 - 16383;
            let mant = ((be32(bytes, body + 10)? as u64) << 32 | be32(bytes, body + 14)? as u64) as f64;
            let r = mant * pow2(exp - 63);
            rate = if r.is_finite() && r > 0.0 && r < 1e7 { r as u32 } else { 0 };
            if aifc && end - body >= 22 {
                match &bytes[body + 18..body + 22] {
                    b"NONE" | b"twos" => {}
                    b"sowt" => little = true,
                    _ => return None,
                }
            }
        } else if id == b"SSND" && end - body >= 8 {
            let skip = be32(bytes, body)? as usize;
            data = bytes.get((body + 8 + skip).min(end)..end);
        }
        at = body.saturating_add(size).saturating_add(size & 1);
    }
    let data = data?;
    let width = (bits as usize + 7) / 8;
    if !(1..=4).contains(&width) || channels == 0 {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() / width);
    for c in data.chunks_exact(width) {
        let mut v: i32 = 0;
        for k in 0..width {
            let byte = if little { c[width - 1 - k] } else { c[k] };
            v = (v << 8) | byte as i32;
        }
        let shift = 32 - 8 * width as u32;
        let v = (v << shift) >> shift; // sign-extend
        out.push(v as f32 / (1u64 << (8 * width - 1)) as f32);
    }
    Some(Pcm { rate, channels, data: out })
}

pub(super) fn pow2(e: i32) -> f64 {
    let mut r = 1.0f64;
    let (mut n, base) = (e.unsigned_abs(), if e < 0 { 0.5 } else { 2.0 });
    while n > 0 {
        r *= base;
        n -= 1;
    }
    r
}
