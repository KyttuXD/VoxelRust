//! Reading pictures: PNG, JPEG (baseline), GIF (the first picture), BMP and
//! TGA. Everything comes out as 8-bit RGBA, top row first. Used for textures
//! the player or a pack supplies, which are fitted to 16 x 16.

use crate::png::{self, Image};
use alloc::vec::Vec;

/// Largest picture accepted, in pixels on a side, and the biggest file.
pub const MAX_SIDE: usize = 4096;
pub const MAX_BYTES: usize = 24 << 20;

/// Read a picture of any of the kinds above, told apart by how it starts.
pub fn decode(d: &[u8]) -> Option<Image> {
    if d.len() < 18 || d.len() > MAX_BYTES {
        return None;
    }
    let img = if d.starts_with(&[0x89, b'P', b'N', b'G']) {
        png::decode(d)
    } else if d.starts_with(&[0xFF, 0xD8]) {
        jpeg(d)
    } else if d.starts_with(b"GIF8") {
        gif(d)
    } else if d.starts_with(b"BM") {
        bmp(d)
    } else {
        tga(d)
    }?;
    if img.w == 0 || img.h == 0 || img.w > MAX_SIDE || img.h > MAX_SIDE || img.rgba.len() != img.w * img.h * 4 {
        return None;
    }
    Some(img)
}

/// Fit a picture into `side` x `side` pixels: smaller ones are enlarged
/// (each pixel stays a hard square), bigger ones averaged down.
pub fn fit(img: &Image, side: usize) -> Vec<u8> {
    let mut out = alloc::vec![0u8; side * side * 4];
    for y in 0..side {
        // The source rows and columns this pixel covers.
        let (y0, y1) = (y * img.h / side, ((y + 1) * img.h / side).max(y * img.h / side + 1).min(img.h));
        for x in 0..side {
            let (x0, x1) = (x * img.w / side, ((x + 1) * img.w / side).max(x * img.w / side + 1).min(img.w));
            let (mut sum, mut a_sum, mut n) = ([0u32; 3], 0u32, 0u32);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = &img.rgba[(sy * img.w + sx) * 4..][..4];
                    // Colours weigh by their alpha, so a see-through pixel does not darken its neighbours.
                    sum = [sum[0] + p[0] as u32 * p[3] as u32, sum[1] + p[1] as u32 * p[3] as u32, sum[2] + p[2] as u32 * p[3] as u32];
                    a_sum += p[3] as u32;
                    n += 1;
                }
            }
            let o = (y * side + x) * 4;
            if a_sum > 0 {
                out[o] = (sum[0] / a_sum) as u8;
                out[o + 1] = (sum[1] / a_sum) as u8;
                out[o + 2] = (sum[2] / a_sum) as u8;
            }
            out[o + 3] = if n > 0 { (a_sum / n) as u8 } else { 0 };
        }
    }
    out
}

// ------------------------------------------------------------------ BMP

fn le16(d: &[u8], at: usize) -> Option<u32> {
    Some(u16::from_le_bytes([*d.get(at)?, *d.get(at + 1)?]) as u32)
}

fn le32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*d.get(at)?, *d.get(at + 1)?, *d.get(at + 2)?, *d.get(at + 3)?]))
}

fn bmp(d: &[u8]) -> Option<Image> {
    let data_at = le32(d, 10)? as usize;
    let head = le32(d, 14)? as usize;
    if head < 40 {
        return None;
    }
    let w = le32(d, 18)? as i32;
    let h = le32(d, 22)? as i32;
    let bits = le16(d, 28)?;
    let compression = le32(d, 30)?;
    let colours = le32(d, 46)? as usize;
    if w <= 0 || h == 0 || w as usize > MAX_SIDE || h.unsigned_abs() as usize > MAX_SIDE || !(compression == 0 || compression == 3) {
        return None;
    }
    let (w, top_down, h) = (w as usize, h < 0, h.unsigned_abs() as usize);
    let palette_at = 14 + head;
    let n_pal = if bits <= 8 { if colours == 0 { 1usize << bits } else { colours } } else { 0 };
    let row = ((w * bits as usize + 31) / 32) * 4;
    let mut rgba = alloc::vec![0u8; w * h * 4];
    // Masks for 16- and 32-bit files with BI_BITFIELDS: assume the usual ones.
    for y in 0..h {
        let src_y = if top_down { y } else { h - 1 - y };
        let line = d.get(data_at + src_y * row..data_at + src_y * row + row)?;
        for x in 0..w {
            let o = (y * w + x) * 4;
            let px: [u8; 4] = match bits {
                32 => [line[x * 4 + 2], line[x * 4 + 1], line[x * 4], line[x * 4 + 3]],
                24 => [line[x * 3 + 2], line[x * 3 + 1], line[x * 3], 255],
                8 | 4 | 1 => {
                    let idx = match bits {
                        8 => line[x] as usize,
                        4 => ((line[x / 2] >> (if x % 2 == 0 { 4 } else { 0 })) & 15) as usize,
                        _ => ((line[x / 8] >> (7 - x % 8)) & 1) as usize,
                    };
                    if idx >= n_pal {
                        return None;
                    }
                    let p = d.get(palette_at + idx * 4..palette_at + idx * 4 + 3)?;
                    [p[2], p[1], p[0], 255]
                }
                _ => return None,
            };
            rgba[o..o + 4].copy_from_slice(&px);
        }
    }
    // A 32-bit file with no alpha at all is opaque.
    if bits == 32 && rgba.chunks(4).all(|p| p[3] == 0) {
        rgba.chunks_mut(4).for_each(|p| p[3] = 255);
    }
    Some(Image { w, h, rgba })
}

// ------------------------------------------------------------------ TGA

fn tga(d: &[u8]) -> Option<Image> {
    let (id_len, map_type, kind) = (d[0] as usize, d[1], d[2]);
    if !matches!((map_type, kind), (0, 2) | (0, 3) | (0, 10) | (0, 11) | (1, 1) | (1, 9)) {
        return None;
    }
    let (map_len, map_bits) = (le16(d, 5)? as usize, d[7] as usize);
    let (w, h, bpp) = (le16(d, 12)? as usize, le16(d, 14)? as usize, d[16] as usize);
    let desc = d[17];
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || !matches!(bpp, 8 | 16 | 24 | 32) {
        return None;
    }
    let map_bytes = if map_type == 1 { map_len * ((map_bits + 7) / 8) } else { 0 };
    let pal = d.get(18 + id_len..18 + id_len + map_bytes)?;
    let mut at = 18 + id_len + map_bytes;
    let bytes = bpp / 8;
    let rle = kind >= 9;
    // Decode every pixel to RGBA in file order.
    let pixel = |p: &[u8]| -> [u8; 4] {
        match (kind & 7, bpp) {
            (1, _) => {
                let i = p[0] as usize;
                let eb = (map_bits + 7) / 8;
                match pal.get(i * eb..i * eb + eb) {
                    Some(c) if eb >= 3 => [c[2], c[1], c[0], if eb == 4 { c[3] } else { 255 }],
                    _ => [0, 0, 0, 255],
                }
            }
            (3, 8) => [p[0], p[0], p[0], 255],
            (3, 16) => [p[0], p[0], p[0], p[1]],
            (_, 16) => {
                let v = p[0] as u16 | (p[1] as u16) << 8;
                let c = |s: u32| (((v >> s) & 31) as u32 * 255 / 31) as u8;
                [c(10), c(5), c(0), if v & 0x8000 != 0 { 255 } else { 255 }]
            }
            (_, 24) => [p[2], p[1], p[0], 255],
            _ => [p[2], p[1], p[0], p[3]],
        }
    };
    let total = w * h;
    let mut px: Vec<[u8; 4]> = Vec::with_capacity(total);
    if !rle {
        let data = d.get(at..at + total * bytes)?;
        for c in data.chunks_exact(bytes) {
            px.push(pixel(c));
        }
    } else {
        while px.len() < total {
            let head = *d.get(at)?;
            at += 1;
            let n = (head & 0x7F) as usize + 1;
            if head & 0x80 != 0 {
                let p = pixel(d.get(at..at + bytes)?);
                at += bytes;
                for _ in 0..n {
                    px.push(p);
                }
            } else {
                for _ in 0..n {
                    px.push(pixel(d.get(at..at + bytes)?));
                    at += bytes;
                }
            }
        }
        px.truncate(total);
    }
    // A 32-bit file whose alpha is all zero is meant to be opaque.
    if bpp == 32 && px.iter().all(|p| p[3] == 0) {
        px.iter_mut().for_each(|p| p[3] = 255);
    }
    let top_first = desc & 0x20 != 0;
    let right_first = desc & 0x10 != 0;
    let mut rgba = alloc::vec![0u8; total * 4];
    for y in 0..h {
        let sy = if top_first { y } else { h - 1 - y };
        for x in 0..w {
            let sx = if right_first { w - 1 - x } else { x };
            rgba[(y * w + x) * 4..][..4].copy_from_slice(&px[sy * w + sx]);
        }
    }
    Some(Image { w, h, rgba })
}

// ------------------------------------------------------------------ GIF

fn gif(d: &[u8]) -> Option<Image> {
    let (sw, sh) = (le16(d, 6)? as usize, le16(d, 8)? as usize);
    let flags = *d.get(10)?;
    let mut at = 13;
    let mut global: &[u8] = &[];
    if flags & 0x80 != 0 {
        let n = 3usize << ((flags & 7) + 1);
        global = d.get(at..at + n)?;
        at += n;
    }
    let mut transparent: Option<u8> = None;
    loop {
        match *d.get(at)? {
            0x21 => {
                let label = *d.get(at + 1)?;
                at += 2;
                if label == 0xF9 && d.get(at)? >= &4 {
                    if d[at + 1] & 1 != 0 {
                        transparent = Some(*d.get(at + 4)?);
                    }
                }
                // Skip the blocks.
                loop {
                    let n = *d.get(at)? as usize;
                    at += 1 + n;
                    if n == 0 {
                        break;
                    }
                }
            }
            0x2C => {
                let (ix, iy, iw, ih) = (le16(d, at + 1)? as usize, le16(d, at + 3)? as usize, le16(d, at + 5)? as usize, le16(d, at + 7)? as usize);
                let lf = *d.get(at + 9)?;
                at += 10;
                let mut table = global;
                if lf & 0x80 != 0 {
                    let n = 3usize << ((lf & 7) + 1);
                    table = d.get(at..at + n)?;
                    at += n;
                }
                let min_code = *d.get(at)? as u32;
                at += 1;
                if !(2..=8).contains(&min_code) || iw == 0 || ih == 0 || iw > MAX_SIDE || ih > MAX_SIDE {
                    return None;
                }
                // The LZW data, gathered from its blocks.
                let mut raw = Vec::new();
                loop {
                    let n = *d.get(at)? as usize;
                    at += 1;
                    if n == 0 {
                        break;
                    }
                    raw.extend_from_slice(d.get(at..at + n)?);
                    at += n;
                }
                let idx = lzw(&raw, min_code, iw * ih)?;
                let (w, h) = (sw.max(ix + iw), sh.max(iy + ih));
                let mut rgba = alloc::vec![0u8; w * h * 4];
                let interlaced = lf & 0x40 != 0;
                // Row order of an interlaced picture.
                let rows: Vec<usize> = if interlaced {
                    let mut v = Vec::with_capacity(ih);
                    for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
                        v.extend((start..ih).step_by(step));
                    }
                    v
                } else {
                    (0..ih).collect()
                };
                for (n, &row) in rows.iter().enumerate() {
                    for x in 0..iw {
                        let c = *idx.get(n * iw + x)?;
                        if Some(c) == transparent {
                            continue;
                        }
                        let p = table.get(c as usize * 3..c as usize * 3 + 3)?;
                        let o = ((iy + row) * w + ix + x) * 4;
                        rgba[o..o + 4].copy_from_slice(&[p[0], p[1], p[2], 255]);
                    }
                }
                return Some(Image { w, h, rgba });
            }
            _ => return None,
        }
    }
}

/// GIF's variable-width LZW, giving `want` colour indices.
fn lzw(raw: &[u8], min_code: u32, want: usize) -> Option<Vec<u8>> {
    let (clear, end) = (1u32 << min_code, (1u32 << min_code) + 1);
    let mut out: Vec<u8> = Vec::with_capacity(want);
    // The dictionary: each entry is (previous entry, last byte); roots are single bytes.
    let mut prefix: Vec<u16> = alloc::vec![0; 4096];
    let mut suffix: Vec<u8> = alloc::vec![0; 4096];
    let mut next = end + 1;
    let mut width = min_code + 1;
    let (mut bits, mut have) = (0u32, 0u32);
    let mut prev: Option<u32> = None;
    let mut stack: Vec<u8> = Vec::new();
    let expand = |code: u32, prefix: &[u16], suffix: &[u8], stack: &mut Vec<u8>| -> u8 {
        let mut c = code;
        stack.clear();
        while c >= end + 1 {
            stack.push(suffix[c as usize]);
            c = prefix[c as usize] as u32;
        }
        stack.push(c as u8);
        *stack.last().unwrap()
    };
    for &byte in raw {
        bits |= (byte as u32) << have;
        have += 8;
        while have >= width {
            let code = bits & ((1 << width) - 1);
            bits >>= width;
            have -= width;
            if code == clear {
                next = end + 1;
                width = min_code + 1;
                prev = None;
                continue;
            }
            if code == end {
                return if out.len() >= want { Some(out) } else { None };
            }
            let first;
            match prev {
                None => {
                    if code >= clear {
                        return None;
                    }
                    out.push(code as u8);
                    prev = Some(code);
                    continue;
                }
                Some(p) => {
                    if code < next {
                        first = expand(code, &prefix, &suffix, &mut stack);
                        out.extend(stack.iter().rev());
                    } else if code == next {
                        let f = expand(p, &prefix, &suffix, &mut stack);
                        out.extend(stack.iter().rev());
                        out.push(f);
                        first = f;
                    } else {
                        return None;
                    }
                    if next < 4096 {
                        prefix[next as usize] = p as u16;
                        suffix[next as usize] = first;
                        next += 1;
                        if next == (1 << width) && width < 12 {
                            width += 1;
                        }
                    }
                    prev = Some(code);
                }
            }
            if out.len() > want + 4096 {
                return None;
            }
        }
    }
    if out.len() >= want { Some(out) } else { None }
}

// ----------------------------------------------------------------- JPEG

struct Huff {
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

struct JBits<'a> {
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

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58,
    59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

struct Comp {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
    td: usize,
    ta: usize,
    plane: Vec<u8>,
    stride: usize,
}

fn jpeg(d: &[u8]) -> Option<Image> {
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
fn scan(d: &[u8], at: usize, w: usize, h: usize, comps: &mut [Comp], qt: &[[i32; 64]; 4], dc: &[Option<Huff>; 4], ac: &[Option<Huff>; 4], restart: usize, adobe: Option<u8>) -> Option<Image> {
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
fn idct(block: &[i32; 64], out: &mut [u8], stride: usize) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: &Image, b: &Image) -> bool {
        a.w == b.w && a.h == b.h && a.rgba == b.rgba
    }

    #[test]
    fn lossless_formats_read_back_the_same_picture() {
        let want = decode(include_bytes!("testdata/pic.png")).expect("png");
        assert_eq!((want.w, want.h), (24, 16));
        for (name, file) in [
            ("bmp24", &include_bytes!("testdata/pic24.bmp")[..]),
            ("bmp8", &include_bytes!("testdata/pic8.bmp")[..]),
            ("tga", &include_bytes!("testdata/pic.tga")[..]),
            ("tga rle", &include_bytes!("testdata/pic_rle.tga")[..]),
        ] {
            let got = decode(file).unwrap_or_else(|| panic!("{name} not read"));
            assert!(same(&got, &want), "{name} differs");
        }
        // GIF has a 256-colour palette: the picture was made with few enough colours.
        let g = decode(include_bytes!("testdata/pic.gif")).expect("gif");
        assert_eq!((g.w, g.h), (24, 16));
    }

    #[test]
    fn jpeg_is_close_to_the_original() {
        let want = decode(include_bytes!("testdata/pic.png")).unwrap();
        for (name, file) in [("jpeg 4:4:4", &include_bytes!("testdata/pic444.jpg")[..]), ("jpeg 4:2:0", &include_bytes!("testdata/pic420.jpg")[..]), ("gray", &include_bytes!("testdata/gray.jpg")[..])] {
            let got = decode(file).unwrap_or_else(|| panic!("{name} not read"));
            assert_eq!((got.w, got.h), (want.w, want.h), "{name}");
            let mut err = 0.0f64;
            for i in 0..want.w * want.h {
                for c in 0..3 {
                    let a = got.rgba[i * 4 + c] as f64;
                    let b = if name == "gray" { (want.rgba[i * 4] as f64 * 0.299 + want.rgba[i * 4 + 1] as f64 * 0.587 + want.rgba[i * 4 + 2] as f64 * 0.114) } else { want.rgba[i * 4 + c] as f64 };
                    err += (a - b) * (a - b);
                }
            }
            let rms = (err / (want.w * want.h * 3) as f64).sqrt();
            assert!(rms < 24.0, "{name}: rms {rms}");
        }
    }

    #[test]
    fn pictures_are_fitted_to_a_tile() {
        let big = decode(include_bytes!("testdata/pic.png")).unwrap();
        let t = fit(&big, 16);
        assert_eq!(t.len(), 16 * 16 * 4);
        let small = Image { w: 2, h: 2, rgba: alloc::vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 0] };
        let t = fit(&small, 16);
        assert_eq!(&t[0..4], &[255, 0, 0, 255]);
        assert_eq!(&t[(15 * 16) * 4..(15 * 16) * 4 + 4], &[0, 0, 255, 255]);
        assert_eq!(t[(15 * 16 + 15) * 4 + 3], 0, "a see-through corner stays so");
    }

    #[test]
    fn things_that_are_not_pictures_are_refused() {
        assert!(decode(b"this is plainly not a picture at all").is_none());
        assert!(decode(&[0xFF, 0xD8, 0xFF, 0xC2, 0, 8, 8, 0, 8, 0, 8, 1, 0, 0, 0, 0, 0, 0]).is_none(), "progressive jpeg");
    }
}
