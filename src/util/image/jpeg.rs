//! JPEG (baseline and extended sequential Huffman).

use super::{Image, MAX_SIDE};
use alloc::vec::Vec;

pub(super) struct Huff {
    // For each code length 1..=16: the first code and the index of its first symbol.
    min_code: [i32; 17],
    max_code: [i32; 18],
    val_ptr: [usize; 17],
    vals: Vec<u8>,
}

impl Huff {
    fn new(counts: &[u8], vals: &[u8]) -> Huff {
        let mut h = Huff { min_code: [0; 17], max_code: [-1; 18], val_ptr: [0; 17], vals: vals.to_vec() };
        let (mut code, mut k) = (0i32, 0usize);
        for l in 1..=16 {
            h.val_ptr[l] = k;
            h.min_code[l] = code;
            code += counts[l - 1] as i32;
            k += counts[l - 1] as usize;
            h.max_code[l] = if counts[l - 1] > 0 { code - 1 } else { -1 };
            code <<= 1;
        }
        h
    }
}

pub(super) struct JBits<'a> {
    d: &'a [u8],
    pos: usize,
    acc: u32,
    n: u32,
}

impl JBits<'_> {
    fn bit(&mut self) -> Option<u32> {
        if self.n == 0 {
            let b = *self.d.get(self.pos)?;
            self.pos += 1;
            if b == 0xFF {
                // A stuffed zero follows a data 0xFF; anything else is a marker (end of data).
                let next = *self.d.get(self.pos)?;
                if next == 0 {
                    self.pos += 1;
                } else {
                    self.pos -= 1;
                    return None;
                }
            }
            self.acc = b as u32;
            self.n = 8;
        }
        self.n -= 1;
        Some((self.acc >> self.n) & 1)
    }

    fn bits(&mut self, k: u32) -> Option<i32> {
        let mut v = 0i32;
        for _ in 0..k {
            v = (v << 1) | self.bit()? as i32;
        }
        Some(v)
    }

    fn huff(&mut self, h: &Huff) -> Option<u8> {
        let mut code = 0i32;
        for l in 1..=16 {
            code = (code << 1) | self.bit()? as i32;
            if h.max_code[l] >= 0 && code <= h.max_code[l] && code >= h.min_code[l] {
                return h.vals.get(h.val_ptr[l] + (code - h.min_code[l]) as usize).copied();
            }
        }
        None
    }

    /// A value of `s` bits in JPEG's sign convention.
    fn extend(&mut self, s: u32) -> Option<i32> {
        if s == 0 {
            return Some(0);
        }
        let v = self.bits(s)?;
        Some(if v < (1 << (s - 1)) { v - (1 << s) + 1 } else { v })
    }

    fn restart(&mut self) {
        self.n = 0;
        // Skip to after the next RSTn marker.
        while self.pos + 1 < self.d.len() {
            if self.d[self.pos] == 0xFF && (0xD0..=0xD7).contains(&self.d[self.pos + 1]) {
                self.pos += 2;
                return;
            }
            self.pos += 1;
        }
    }
}

pub(super) const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58,
    59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

pub(super) struct Comp {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
    td: usize,
    ta: usize,
    plane: Vec<u8>,
    stride: usize,
}

pub(super) fn jpeg(d: &[u8]) -> Option<Image> {
    let mut qt = [[1i32; 64]; 4];
    let mut dc: [Option<Huff>; 4] = [None, None, None, None];
    let mut ac: [Option<Huff>; 4] = [None, None, None, None];
    let mut comps: Vec<Comp> = Vec::new();
    let (mut w, mut h) = (0usize, 0usize);
    let mut restart = 0usize;
    let mut adobe_transform: Option<u8> = None;
    let mut at = 2;
    loop {
        // Find the next marker.
        while *d.get(at)? != 0xFF {
            at += 1;
        }
        while *d.get(at)? == 0xFF {
            at += 1;
        }
        let m = *d.get(at)?;
        at += 1;
        if m == 0xD9 {
            return None;
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            continue;
        }
        let len = u16::from_be_bytes([*d.get(at)?, *d.get(at + 1)?]) as usize;
        let seg = d.get(at + 2..at + len)?;
        at += len;
        match m {
            0xC0 | 0xC1 => {
                if seg[0] != 8 {
                    return None;
                }
                h = u16::from_be_bytes([seg[1], seg[2]]) as usize;
                w = u16::from_be_bytes([seg[3], seg[4]]) as usize;
                let n = seg[5] as usize;
                if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || !(n == 1 || n == 3) || seg.len() < 6 + n * 3 {
                    return None;
                }
                comps.clear();
                for k in 0..n {
                    let (id, hv, tq) = (seg[6 + k * 3], seg[7 + k * 3], seg[8 + k * 3] as usize);
                    let (ch, cv) = ((hv >> 4) as usize, (hv & 15) as usize);
                    if !(1..=4).contains(&ch) || !(1..=4).contains(&cv) || tq > 3 {
                        return None;
                    }
                    comps.push(Comp { id, h: ch, v: cv, tq, td: 0, ta: 0, plane: Vec::new(), stride: 0 });
                }
            }
            // Progressive and the lossless/arithmetic kinds are not read.
            0xC2 | 0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF => return None,
            0xC4 => {
                let mut p = 0;
                while p + 17 <= seg.len() {
                    let (class, id) = (seg[p] >> 4, (seg[p] & 15) as usize);
                    let counts = &seg[p + 1..p + 17];
                    let total: usize = counts.iter().map(|&c| c as usize).sum();
                    let vals = seg.get(p + 17..p + 17 + total)?;
                    if id > 3 {
                        return None;
                    }
                    let table = Huff::new(counts, vals);
                    if class == 0 { dc[id] = Some(table) } else { ac[id] = Some(table) }
                    p += 17 + total;
                }
            }
            0xDB => {
                let mut p = 0;
                while p < seg.len() {
                    let (prec, id) = (seg[p] >> 4, (seg[p] & 15) as usize);
                    p += 1;
                    if id > 3 {
                        return None;
                    }
                    for k in 0..64 {
                        let v = if prec == 0 {
                            let v = *seg.get(p)? as i32;
                            p += 1;
                            v
                        } else {
                            let v = u16::from_be_bytes([*seg.get(p)?, *seg.get(p + 1)?]) as i32;
                            p += 2;
                            v
                        };
                        qt[id][ZIGZAG[k]] = v;
                    }
                }
            }
            0xDD => restart = u16::from_be_bytes([seg[0], seg[1]]) as usize,
            0xEE if seg.starts_with(b"Adobe") && seg.len() >= 12 => adobe_transform = Some(seg[11]),
            0xDA => {
                if comps.is_empty() {
                    return None;
                }
                let n = seg[0] as usize;
                if n != comps.len() {
                    return None;
                }
                for k in 0..n {
                    let (cid, t) = (seg[1 + k * 2], seg[2 + k * 2]);
                    let c = comps.iter_mut().find(|c| c.id == cid)?;
                    c.td = (t >> 4) as usize & 3;
                    c.ta = (t & 15) as usize & 3;
                }
                return scan(d, at, w, h, &mut comps, &qt, &dc, &ac, restart, adobe_transform);
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn scan(d: &[u8], at: usize, w: usize, h: usize, comps: &mut [Comp], qt: &[[i32; 64]; 4], dc: &[Option<Huff>; 4], ac: &[Option<Huff>; 4], restart: usize, adobe: Option<u8>) -> Option<Image> {
    let hmax = comps.iter().map(|c| c.h).max()?;
    let vmax = comps.iter().map(|c| c.v).max()?;
    let (mcu_w, mcu_h) = (8 * hmax, 8 * vmax);
    let (mcus_x, mcus_y) = ((w + mcu_w - 1) / mcu_w, (h + mcu_h - 1) / mcu_h);
    for c in comps.iter_mut() {
        c.stride = mcus_x * c.h * 8;
        c.plane = alloc::vec![0u8; c.stride * mcus_y * c.v * 8];
    }
    let mut r = JBits { d, pos: at, acc: 0, n: 0 };
    let mut pred = alloc::vec![0i32; comps.len()];
    let mut count = 0usize;
    let mut block = [0i32; 64];
    for my in 0..mcus_y {
        for mx in 0..mcus_x {
            if restart > 0 && count > 0 && count % restart == 0 {
                r.restart();
                pred.iter_mut().for_each(|p| *p = 0);
            }
            count += 1;
            for (ci, c) in comps.iter_mut().enumerate() {
                let (dct, act) = (dc[c.td].as_ref()?, ac[c.ta].as_ref()?);
                for by in 0..c.v {
                    for bx in 0..c.h {
                        block.iter_mut().for_each(|v| *v = 0);
                        let t = r.huff(dct)? as u32;
                        pred[ci] += r.extend(t)?;
                        block[0] = pred[ci] * qt[c.tq][0];
                        let mut k = 1;
                        while k < 64 {
                            let rs = r.huff(act)?;
                            let (run, size) = ((rs >> 4) as usize, (rs & 15) as u32);
                            if size == 0 {
                                if run == 15 {
                                    k += 16;
                                    continue;
                                }
                                break;
                            }
                            k += run;
                            if k > 63 {
                                return None;
                            }
                            block[ZIGZAG[k]] = r.extend(size)? * qt[c.tq][ZIGZAG[k]];
                            k += 1;
                        }
                        let (x0, y0) = ((mx * c.h + bx) * 8, (my * c.v + by) * 8);
                        idct(&block, &mut c.plane[y0 * c.stride + x0..], c.stride);
                    }
                }
            }
        }
    }
    // Upsample and convert to RGB.
    let mut rgba = alloc::vec![255u8; w * h * 4];
    let colour = comps.len() == 3 && adobe != Some(0);
    for y in 0..h {
        for x in 0..w {
            let sample = |c: &Comp| -> f32 { c.plane[(y * c.v / vmax) * c.stride + x * c.h / hmax] as f32 };
            let o = (y * w + x) * 4;
            if comps.len() == 1 {
                let v = sample(&comps[0]) as u8;
                rgba[o..o + 3].copy_from_slice(&[v, v, v]);
            } else if colour {
                let (yy, cb, cr) = (sample(&comps[0]), sample(&comps[1]) - 128.0, sample(&comps[2]) - 128.0);
                rgba[o] = (yy + 1.402 * cr).clamp(0.0, 255.0) as u8;
                rgba[o + 1] = (yy - 0.344_136 * cb - 0.714_136 * cr).clamp(0.0, 255.0) as u8;
                rgba[o + 2] = (yy + 1.772 * cb).clamp(0.0, 255.0) as u8;
            } else {
                rgba[o] = sample(&comps[0]) as u8;
                rgba[o + 1] = sample(&comps[1]) as u8;
                rgba[o + 2] = sample(&comps[2]) as u8;
            }
        }
    }
    Some(Image { w, h, rgba })
}

/// An 8 x 8 inverse DCT (separable, in floating point), written into `out`
/// as unsigned bytes `stride` apart.
pub(super) fn idct(block: &[i32; 64], out: &mut [u8], stride: usize) {
    use crate::math::cosf;
    // cos((2x + 1) u pi / 16) * c(u) / 2, computed once.
    static TABLE: core::sync::atomic::AtomicPtr<[[f32; 8]; 8]> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
    let mut p = TABLE.load(core::sync::atomic::Ordering::Acquire);
    if p.is_null() {
        let mut t = [[0f32; 8]; 8];
        for x in 0..8 {
            for u in 0..8 {
                let cu = if u == 0 { core::f32::consts::FRAC_1_SQRT_2 } else { 1.0 };
                t[x][u] = cu * 0.5 * cosf((2 * x + 1) as f32 * u as f32 * core::f32::consts::PI / 16.0);
            }
        }
        p = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(t));
        TABLE.store(p, core::sync::atomic::Ordering::Release);
    }
    // SAFETY: set once from a leaked box, never freed.
    let t = unsafe { &*p };
    let mut tmp = [0f32; 64];
    for y in 0..8 {
        for x in 0..8 {
            let mut s = 0.0;
            for u in 0..8 {
                s += t[x][u] * block[y * 8 + u] as f32;
            }
            tmp[y * 8 + x] = s;
        }
    }
    for x in 0..8 {
        for y in 0..8 {
            let mut s = 0.0;
            for v in 0..8 {
                s += t[y][v] * tmp[v * 8 + x];
            }
            out[y * stride + x] = (s + 128.0).clamp(0.0, 255.0) as u8;
        }
    }
}
