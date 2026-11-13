//! FLAC.

use super::{Pcm, MAX_SECONDS};
use alloc::vec::Vec;

pub(super) struct Bits<'a> {
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

pub(super) fn flac(bytes: &[u8]) -> Option<Pcm> {
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

pub(super) fn subframe(r: &mut Bits, block: usize, bps: u32) -> Option<Vec<i64>> {
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
pub(super) fn residual(r: &mut Bits, block: usize, order: usize, s: &mut Vec<i64>) -> Option<()> {
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
