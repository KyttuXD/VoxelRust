//! Reading sound files: WAV (PCM, float, A-law, mu-law, ADPCM), AIFF and FLAC,
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

/// Sound the mixer can play, from a file of any kind this reads: WAV (PCM of
/// 8, 16, 24 or 32 bits, 32-bit float, A-law, mu-law, MS ADPCM, IMA ADPCM),
/// AIFF and AIFF-C (PCM), and FLAC. Any number of channels (averaged), any
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
    } else {
        return None;
    };
    pcm.to_mixer()
}

/// What the readers give: interleaved samples in -1..1.
struct Pcm {
    rate: u32,
    channels: usize,
    data: Vec<f32>,
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

// ---------------------------------------------------------------- WAV

fn wav(bytes: &[u8]) -> Option<Pcm> {
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
        out.truncate(f * ch);
    }
    Some(Pcm { rate, channels: ch, data: out })
}

fn alaw(b: u8) -> i16 {
    let b = b ^ 0x55;
    let (seg, mant) = (((b & 0x70) >> 4) as i32, (b & 0x0F) as i32);
    let mut v = (mant << 4) + 8;
    if seg > 0 {
        v = (v + 0x100) << (seg - 1);
    }
    (if b & 0x80 != 0 { v } else { -v }) as i16
}

fn ulaw(b: u8) -> i16 {
    let b = !b;
    let v = (((((b & 0x0F) as i32) << 3) + 0x84) << ((b & 0x70) >> 4)) - 0x84;
    (if b & 0x80 != 0 { -v } else { v }) as i16
}

const IMA_STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371,
    408, 449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132,
    7845, 8630, 9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const IMA_INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

fn ima_adpcm(data: &[u8], ch: usize, align: usize, out: &mut Vec<f32>) -> Option<()> {
    if align < 4 * ch || align > 65_536 {
        return None;
    }
    for block in data.chunks(align) {
        if block.len() < 4 * ch {
            break;
        }
        let mut pred = [0i32; 8];
        let mut idx = [0i32; 8];
        let mut chans: Vec<Vec<f32>> = alloc::vec![Vec::new(); ch];
        for c in 0..ch {
            pred[c] = i16::from_le_bytes([block[c * 4], block[c * 4 + 1]]) as i32;
            idx[c] = (block[c * 4 + 2] as i32).min(88);
            chans[c].push(pred[c] as f32 / 32768.0);
        }
        // After the headers: groups of four bytes (eight samples) per channel, in turn.
        let body = &block[4 * ch..];
        for group in body.chunks(4 * ch) {
            for c in 0..ch {
                let Some(bytes) = group.get(c * 4..c * 4 + 4) else { continue };
                for &byte in bytes {
                    for nib in [byte & 15, byte >> 4] {
                        let step = IMA_STEPS[idx[c] as usize];
                        let mut diff = step >> 3;
                        if nib & 1 != 0 { diff += step >> 2; }
                        if nib & 2 != 0 { diff += step >> 1; }
                        if nib & 4 != 0 { diff += step; }
                        pred[c] = (pred[c] + if nib & 8 != 0 { -diff } else { diff }).clamp(-32768, 32767);
                        idx[c] = (idx[c] + IMA_INDEX[nib as usize]).clamp(0, 88);
                        chans[c].push(pred[c] as f32 / 32768.0);
                    }
                }
            }
        }
        let frames = chans.iter().map(|c| c.len()).min().unwrap_or(0);
        for f in 0..frames {
            for c in 0..ch {
                out.push(chans[c][f]);
            }
        }
    }
    Some(())
}

fn ms_adpcm(data: &[u8], ch: usize, align: usize, extra: &[u8], out: &mut Vec<f32>) -> Option<()> {
    // extra: samples per block (u16), number of coefficient pairs (u16), then the pairs.
    let ncoef = u16le(extra, 2)? as usize;
    if ncoef == 0 || ncoef > 16 || extra.len() < 4 + ncoef * 4 || align < 7 * ch || align > 65_536 {
        return None;
    }
    let coef: Vec<(i32, i32)> = (0..ncoef).map(|k| (u16le(extra, 4 + k * 4).unwrap() as i16 as i32, u16le(extra, 6 + k * 4).unwrap() as i16 as i32)).collect();
    const ADAPT: [i32; 16] = [230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230];
    for block in data.chunks(align) {
        if block.len() < 7 * ch {
            break;
        }
        let mut pred = [0usize; 8];
        let mut delta = [0i32; 8];
        let mut s1 = [0i32; 8];
        let mut s2 = [0i32; 8];
        let mut chans: Vec<Vec<f32>> = alloc::vec![Vec::new(); ch];
        for c in 0..ch {
            pred[c] = (block[c] as usize).min(ncoef - 1);
        }
        for c in 0..ch {
            delta[c] = i16::from_le_bytes([block[ch + c * 2], block[ch + c * 2 + 1]]) as i32;
        }
        for c in 0..ch {
            s1[c] = i16::from_le_bytes([block[3 * ch + c * 2], block[3 * ch + c * 2 + 1]]) as i32;
        }
        for c in 0..ch {
            s2[c] = i16::from_le_bytes([block[5 * ch + c * 2], block[5 * ch + c * 2 + 1]]) as i32;
        }
        for c in 0..ch {
            chans[c].push(s2[c] as f32 / 32768.0);
            chans[c].push(s1[c] as f32 / 32768.0);
        }
        // Nibbles alternate between the channels.
        let mut turn = 0usize;
        for &byte in &block[7 * ch..] {
            for nib in [(byte >> 4) as i32, (byte & 15) as i32] {
                let c = turn % ch;
                turn += 1;
                let signed = if nib >= 8 { nib - 16 } else { nib };
                let p = ((s1[c] * coef[pred[c]].0 + s2[c] * coef[pred[c]].1) >> 8) + signed * delta[c];
                let p = p.clamp(-32768, 32767);
                s2[c] = s1[c];
                s1[c] = p;
                delta[c] = ((ADAPT[nib as usize] * delta[c]) >> 8).max(16);
                chans[c].push(p as f32 / 32768.0);
            }
        }
        let frames = chans.iter().map(|c| c.len()).min().unwrap_or(0);
        for f in 0..frames {
            for c in 0..ch {
                out.push(chans[c][f]);
            }
        }
    }
    Some(())
}

// --------------------------------------------------------------- AIFF

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(at)?, *b.get(at + 1)?, *b.get(at + 2)?, *b.get(at + 3)?]))
}

fn aiff(bytes: &[u8]) -> Option<Pcm> {
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

fn pow2(e: i32) -> f64 {
    let mut r = 1.0f64;
    let (mut n, base) = (e.unsigned_abs(), if e < 0 { 0.5 } else { 2.0 });
    while n > 0 {
        r *= base;
        n -= 1;
    }
    r
}

// --------------------------------------------------------------- FLAC

struct Bits<'a> {
    d: &'a [u8],
    pos: usize, // in bits
}

impl Bits<'_> {
    fn bit(&mut self) -> Option<u32> {
        let byte = *self.d.get(self.pos >> 3)?;
        let v = (byte >> (7 - (self.pos & 7))) & 1;
        self.pos += 1;
        Some(v as u32)
    }

    fn bits(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u64;
        for _ in 0..n {
            v = (v << 1) | self.bit()? as u64;
        }
        Some(v as u32)
    }

    fn signed(&mut self, n: u32) -> Option<i64> {
        if n == 0 {
            return Some(0);
        }
        let v = self.bits(n)? as i64;
        Some(if v >> (n - 1) != 0 { v - (1i64 << n) } else { v })
    }

    fn unary(&mut self) -> Option<u32> {
        let mut n = 0;
        while self.bit()? == 0 {
            n += 1;
            if n > 1 << 20 {
                return None;
            }
        }
        Some(n)
    }

    fn align(&mut self) {
        self.pos = (self.pos + 7) & !7;
    }
}

fn flac(bytes: &[u8]) -> Option<Pcm> {
    // Metadata blocks: only STREAMINFO matters.
    let mut at = 4;
    let (mut rate, mut channels, mut bps) = (0u32, 0usize, 0u32);
    loop {
        let head = *bytes.get(at)?;
        let size = ((*bytes.get(at + 1)? as usize) << 16) | ((*bytes.get(at + 2)? as usize) << 8) | *bytes.get(at + 3)? as usize;
        if head & 0x7F == 0 && size >= 34 {
            let b = &bytes[at + 4..at + 4 + 34];
            rate = ((b[10] as u32) << 12) | ((b[11] as u32) << 4) | (b[12] as u32 >> 4);
            channels = ((b[12] >> 1) & 7) as usize + 1;
            bps = ((((b[12] & 1) as u32) << 4) | (b[13] as u32 >> 4)) + 1;
        }
        at += 4 + size;
        if head & 0x80 != 0 {
            break;
        }
    }
    if rate == 0 || bps == 0 || bps > 24 {
        return None;
    }
    let mut r = Bits { d: bytes, pos: at * 8 };
    let mut out: Vec<f32> = Vec::new();
    let limit = rate as usize * MAX_SECONDS as usize * channels;
    let scale = (1u64 << (bps - 1)) as f32;
    while r.pos / 8 + 4 < bytes.len() && out.len() <= limit {
        // Frame header.
        if r.bits(14)? != 0x3FFE {
            break;
        }
        r.bits(2)?; // reserved, blocking strategy
        let (bs_code, sr_code) = (r.bits(4)?, r.bits(4)?);
        let (ch_code, size_code) = (r.bits(4)?, r.bits(3)?);
        r.bits(1)?;
        // The frame or sample number, in UTF-8 style.
        let first = r.bits(8)?;
        let extra = if first >= 0xFC { 5 } else if first >= 0xF8 { 4 } else if first >= 0xF0 { 3 } else if first >= 0xE0 { 2 } else if first >= 0xC0 { 1 } else { 0 };
        for _ in 0..extra {
            r.bits(8)?;
        }
        let block = match bs_code {
            1 => 192,
            2..=5 => 576 << (bs_code - 2),
            6 => r.bits(8)? as usize + 1,
            7 => r.bits(16)? as usize + 1,
            8..=15 => 256 << (bs_code - 8),
            _ => return None,
        };
        match sr_code {
            12 => {
                r.bits(8)?;
            }
            13 | 14 => {
                r.bits(16)?;
            }
            _ => {}
        }
        r.bits(8)?; // header CRC
        let frame_bps = match size_code {
            0 => bps,
            1 => 8,
            2 => 12,
            4 => 16,
            5 => 20,
            6 => 24,
            _ => return None,
        };
        let (nch, mode) = if ch_code <= 7 { (ch_code as usize + 1, 0) } else if ch_code <= 10 { (2, ch_code - 7) } else { return None };
        if nch != channels {
            return None;
        }
        let mut subs: Vec<Vec<i64>> = Vec::with_capacity(nch);
        for c in 0..nch {
            // A side channel has one more bit.
            let extra_bit = (mode == 1 && c == 1) || (mode == 2 && c == 0) || (mode == 3 && c == 1);
            subs.push(subframe(&mut r, block, frame_bps + extra_bit as u32)?);
        }
        r.align();
        r.bits(16)?; // frame CRC
        match mode {
            1 => (0..block).for_each(|i| subs[1][i] = subs[0][i] - subs[1][i]),
            2 => (0..block).for_each(|i| subs[0][i] += subs[1][i]),
            3 => {
                for i in 0..block {
                    let (m, s) = (subs[0][i], subs[1][i]);
                    let mid = (m << 1) | (s & 1);
                    subs[0][i] = (mid + s) >> 1;
                    subs[1][i] = (mid - s) >> 1;
                }
            }
            _ => {}
        }
        for i in 0..block {
            for c in 0..nch {
                out.push(subs[c][i] as f32 / scale);
            }
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(Pcm { rate, channels, data: out })
}

fn subframe(r: &mut Bits, block: usize, bps: u32) -> Option<Vec<i64>> {
    r.bit()?; // padding
    let kind = r.bits(6)?;
    // Wasted bits: samples that are all multiples of a power of two.
    let mut wasted = 0;
    if r.bit()? == 1 {
        wasted = r.unary()? + 1;
    }
    let bps = bps.checked_sub(wasted)?;
    let mut s: Vec<i64> = Vec::with_capacity(block);
    match kind {
        0 => {
            let v = r.signed(bps)?;
            s.resize(block, v);
        }
        1 => {
            for _ in 0..block {
                s.push(r.signed(bps)?);
            }
        }
        8..=12 => {
            let order = (kind - 8) as usize;
            for _ in 0..order.min(block) {
                s.push(r.signed(bps)?);
            }
            residual(r, block, order, &mut s)?;
            // Fixed predictors of order 0..4.
            const COEF: [&[i64]; 5] = [&[], &[1], &[2, -1], &[3, -3, 1], &[4, -6, 4, -1]];
            for i in order..block {
                let mut p = 0i64;
                for (k, c) in COEF[order].iter().enumerate() {
                    p += c * s[i - 1 - k];
                }
                s[i] += p;
            }
        }
        32..=63 => {
            let order = (kind - 31) as usize;
            for _ in 0..order.min(block) {
                s.push(r.signed(bps)?);
            }
            let precision = r.bits(4)? + 1;
            if precision == 16 {
                return None;
            }
            let shift = r.signed(5)?;
            if shift < 0 {
                return None;
            }
            let mut coef = Vec::with_capacity(order);
            for _ in 0..order {
                coef.push(r.signed(precision)?);
            }
            residual(r, block, order, &mut s)?;
            for i in order..block {
                let mut p = 0i64;
                for (k, c) in coef.iter().enumerate() {
                    p += c * s[i - 1 - k];
                }
                s[i] += p >> shift;
            }
        }
        _ => return None,
    }
    if wasted > 0 {
        for v in s.iter_mut() {
            *v <<= wasted;
        }
    }
    Some(s)
}

/// Rice-coded residuals for samples `order..block`, put into `s`.
fn residual(r: &mut Bits, block: usize, order: usize, s: &mut Vec<i64>) -> Option<()> {
    let method = r.bits(2)?;
    if method > 1 {
        return None;
    }
    let (param_bits, escape) = if method == 0 { (4, 15) } else { (5, 31) };
    let order_p = r.bits(4)? as usize;
    let parts = 1usize << order_p;
    if block % parts != 0 || block / parts < order {
        return None;
    }
    for p in 0..parts {
        let n = if p == 0 { block / parts - order } else { block / parts };
        let k = r.bits(param_bits)?;
        if k == escape {
            let bits = r.bits(5)?;
            for _ in 0..n {
                s.push(r.signed(bits)?);
            }
        } else {
            for _ in 0..n {
                let q = r.unary()? as i64;
                let low = r.bits(k)? as i64;
                let v = (q << k) | low;
                s.push((v >> 1) ^ -(v & 1));
            }
        }
    }
    Some(())
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
