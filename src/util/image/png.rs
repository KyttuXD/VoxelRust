//! PNG decoding and (uncompressed) encoding, zlib inflate, CRC-32, base64.
//!
//! The decoder always produces 8-bit RGBA with row 0 at the top. It handles
//! every non-interlaced PNG: colour types 0, 2, 3, 4 and 6 at all their legal
//! bit depths (16-bit samples keep their high byte), `tRNS`, the five row
//! filters and data split over many `IDAT` chunks. Bad input gives `None`.

use alloc::vec::Vec;

/// 8-bit RGBA pixels, row 0 at the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

/// Largest accepted image side, in pixels.
const MAX_SIDE: usize = 4096;
/// Largest output [`inflate`] will produce.
const MAX_INFLATE: usize = 64 << 20;

const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

// ---------------------------------------------------------------- checksums

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

static CRC_TABLE: [u32; 256] = crc_table();

fn crc_update(mut c: u32, data: &[u8]) -> u32 {
    for &b in data {
        c = CRC_TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c
}

/// CRC-32 (IEEE 802.3), as used by PNG chunks.
pub fn crc32(data: &[u8]) -> u32 {
    !crc_update(!0, data)
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    // 5552 bytes is the most that cannot overflow a u32 between reductions.
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    b << 16 | a
}

// ------------------------------------------------------------------ inflate

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u32,
    cnt: u32,
}

impl<'a> Bits<'a> {
    /// Reads `n` (at most 16) bits, least significant first.
    fn bits(&mut self, n: u32) -> Option<u32> {
        while self.cnt < n {
            let byte = *self.data.get(self.pos)?;
            self.pos += 1;
            self.buf |= (byte as u32) << self.cnt;
            self.cnt += 8;
        }
        let v = self.buf & ((1u32 << n) - 1);
        self.buf >>= n;
        self.cnt -= n;
        Some(v)
    }

    /// Drops the rest of the current byte. `bits` never buffers more than
    /// it needs, so what is left is always less than a byte.
    fn align(&mut self) {
        self.buf = 0;
        self.cnt = 0;
    }
}

/// A canonical Huffman code: how many codes there are of each length, and
/// the symbols ordered by code.
struct Huffman {
    count: [u16; 16],
    symbol: [u16; 288],
}

impl Huffman {
    fn new(lengths: &[u8]) -> Option<Huffman> {
        let mut h = Huffman { count: [0; 16], symbol: [0; 288] };
        if lengths.len() > h.symbol.len() {
            return None;
        }
        for &l in lengths {
            h.count[(l & 15) as usize] += 1;
        }
        // Reject over-subscribed codes. Incomplete ones are allowed; codes
        // that do not exist simply fail to decode.
        let mut left = 1i32;
        for len in 1..16 {
            left = (left << 1) - h.count[len] as i32;
            if left < 0 {
                return None;
            }
        }
        let mut offs = [0u16; 16];
        for len in 1..15 {
            offs[len + 1] = offs[len] + h.count[len];
        }
        for (sym, &l) in lengths.iter().enumerate() {
            let l = (l & 15) as usize;
            if l != 0 {
                h.symbol[offs[l] as usize] = sym as u16;
                offs[l] += 1;
            }
        }
        Some(h)
    }

    fn decode(&self, br: &mut Bits) -> Option<usize> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= br.bits(1)? as i32;
            let count = self.count[len] as i32;
            if code - count < first {
                return self.symbol.get((index + code - first) as usize).map(|&s| s as usize);
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        None
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] =
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// The order code-length code lengths are stored in.
const CLEN_ORDER: [u8; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

fn fixed_codes() -> Option<(Huffman, Huffman)> {
    let mut l = [0u8; 288];
    for (i, v) in l.iter_mut().enumerate() {
        *v = match i {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    Some((Huffman::new(&l)?, Huffman::new(&[5u8; 30])?))
}

fn dynamic_codes(br: &mut Bits) -> Option<(Huffman, Huffman)> {
    let nlen = br.bits(5)? as usize + 257;
    let ndist = br.bits(5)? as usize + 1;
    let ncode = br.bits(4)? as usize + 4;
    if nlen > 286 || ndist > 30 {
        return None;
    }
    let mut cl = [0u8; 19];
    for &slot in CLEN_ORDER.iter().take(ncode) {
        cl[slot as usize] = br.bits(3)? as u8;
    }
    let clen = Huffman::new(&cl)?;

    let mut lengths = [0u8; 286 + 30];
    let total = nlen + ndist;
    let mut i = 0;
    while i < total {
        let sym = clen.decode(br)?;
        if sym < 16 {
            lengths[i] = sym as u8;
            i += 1;
            continue;
        }
        let (value, repeat) = match sym {
            16 => {
                if i == 0 {
                    return None;
                }
                (lengths[i - 1], 3 + br.bits(2)? as usize)
            }
            17 => (0, 3 + br.bits(3)? as usize),
            _ => (0, 11 + br.bits(7)? as usize),
        };
        if i + repeat > total {
            return None;
        }
        for l in &mut lengths[i..i + repeat] {
            *l = value;
        }
        i += repeat;
    }
    if lengths[256] == 0 {
        return None; // no end-of-block code
    }
    Some((Huffman::new(&lengths[..nlen])?, Huffman::new(&lengths[nlen..total])?))
}

fn inflate_block(
    br: &mut Bits,
    out: &mut Vec<u8>,
    lit: &Huffman,
    dist: &Huffman,
    limit: usize,
) -> Option<()> {
    loop {
        let sym = lit.decode(br)?;
        if sym < 256 {
            if out.len() >= limit {
                return None;
            }
            out.push(sym as u8);
            continue;
        }
        if sym == 256 {
            return Some(());
        }
        let li = sym - 257;
        let len = *LEN_BASE.get(li)? as usize + br.bits(LEN_EXTRA[li] as u32)? as usize;
        let di = dist.decode(br)?;
        let d = *DIST_BASE.get(di)? as usize + br.bits(DIST_EXTRA[di] as u32)? as usize;
        if d > out.len() || len > limit - out.len() {
            return None;
        }
        // The source may overlap the bytes being written, so copy one by one.
        let from = out.len() - d;
        for k in 0..len {
            let b = out[from + k];
            out.push(b);
        }
    }
}

fn inflate_limited(zlib: &[u8], limit: usize) -> Option<Vec<u8>> {
    if zlib.len() < 6 {
        return None;
    }
    let (cmf, flg) = (zlib[0] as u32, zlib[1] as u32);
    // Deflate, a valid header check, and no preset dictionary.
    if cmf & 0x0F != 8 || cmf >> 4 > 7 || (cmf << 8 | flg) % 31 != 0 || flg & 0x20 != 0 {
        return None;
    }
    let (out, end) = inflate_blocks(zlib, 2, limit)?;
    let tail = zlib.get(end..end.checked_add(4)?)?;
    let want = u32::from_be_bytes([tail[0], tail[1], tail[2], tail[3]]);
    if adler32(&out) != want {
        return None;
    }
    Some(out)
}

/// Decompresses raw deflate data (no zlib header or checksum), as zip files
/// store it. `None` for corrupt input or output larger than `limit`.
pub fn inflate_raw(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    inflate_blocks(data, 0, limit).map(|(out, _)| out)
}

/// The deflate blocks from byte `start`: the output, and where the data ends.
fn inflate_blocks(zlib: &[u8], start: usize, limit: usize) -> Option<(Vec<u8>, usize)> {
    let mut br = Bits { data: zlib, pos: start, buf: 0, cnt: 0 };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = br.bits(1)?;
        match br.bits(2)? {
            0 => {
                br.align();
                let head = zlib.get(br.pos..br.pos.checked_add(4)?)?;
                let len = head[0] as usize | (head[1] as usize) << 8;
                let nlen = head[2] as usize | (head[3] as usize) << 8;
                if len ^ nlen != 0xFFFF {
                    return None;
                }
                br.pos += 4;
                let body = zlib.get(br.pos..br.pos.checked_add(len)?)?;
                if len > limit - out.len() {
                    return None;
                }
                out.extend_from_slice(body);
                br.pos += len;
            }
            1 => {
                let (lit, dist) = fixed_codes()?;
                inflate_block(&mut br, &mut out, &lit, &dist, limit)?;
            }
            2 => {
                let (lit, dist) = dynamic_codes(&mut br)?;
                inflate_block(&mut br, &mut out, &lit, &dist, limit)?;
            }
            _ => return None,
        }
        if last == 1 {
            break;
        }
    }
    br.align();
    Some((out, br.pos))
}

/// Decompresses a zlib stream (header, deflate data, Adler-32). Gives `None`
/// for corrupt input or output larger than 64 MB.
pub fn inflate(zlib: &[u8]) -> Option<Vec<u8>> {
    inflate_limited(zlib, MAX_INFLATE)
}

// ------------------------------------------------------------------- decode

fn be32(b: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes([*b.first()?, *b.get(1)?, *b.get(2)?, *b.get(3)?]))
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (a as i32, b as i32, c as i32);
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Undoes the row filters in place. `raw` holds `h` rows of one filter byte
/// plus `stride` data bytes; `bpp` is the filter distance in bytes.
fn unfilter(raw: &mut [u8], h: usize, stride: usize, bpp: usize) -> Option<()> {
    let line = stride + 1;
    for y in 0..h {
        let (before, rest) = raw.split_at_mut(y * line);
        let filter = *rest.first()?;
        let cur = rest.get_mut(1..line)?;
        let prev: &[u8] = if y == 0 { &[] } else { &before[before.len() - stride..] };
        let up = |i: usize| -> u8 { prev.get(i).copied().unwrap_or(0) };
        match filter {
            0 => {}
            1 => {
                for i in bpp..stride {
                    cur[i] = cur[i].wrapping_add(cur[i - bpp]);
                }
            }
            2 => {
                for i in 0..stride {
                    cur[i] = cur[i].wrapping_add(up(i));
                }
            }
            3 => {
                for i in 0..stride {
                    let left = if i >= bpp { cur[i - bpp] as u16 } else { 0 };
                    cur[i] = cur[i].wrapping_add(((left + up(i) as u16) / 2) as u8);
                }
            }
            4 => {
                for i in 0..stride {
                    let (left, diag) =
                        if i >= bpp { (cur[i - bpp], up(i - bpp)) } else { (0, 0) };
                    cur[i] = cur[i].wrapping_add(paeth(left, up(i), diag));
                }
            }
            _ => return None,
        }
    }
    Some(())
}

/// Decodes a PNG file into 8-bit RGBA. Interlaced images, images larger
/// than 4096x4096 and anything malformed give `None`.
pub fn decode(data: &[u8]) -> Option<Image> {
    if data.get(..8)? != SIGNATURE {
        return None;
    }
    let mut pos = 8;
    let mut header: Option<(usize, usize, u8, u8)> = None;
    let mut palette: &[u8] = &[];
    let mut trns: &[u8] = &[];
    let mut idat: Vec<u8> = Vec::new();
    loop {
        let len = be32(data.get(pos..)?)? as usize;
        let body_end = pos.checked_add(8)?.checked_add(len)?;
        let kind = data.get(pos + 4..pos + 8)?;
        let body = data.get(pos + 8..body_end)?;
        if be32(data.get(body_end..)?)? != crc32(data.get(pos + 4..body_end)?) {
            return None;
        }
        pos = body_end + 4;
        if header.is_none() && kind != b"IHDR" {
            return None;
        }
        match kind {
            b"IHDR" => {
                if header.is_some() || body.len() != 13 {
                    return None;
                }
                let (w, h) = (be32(body)? as usize, be32(&body[4..])? as usize);
                if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
                    return None;
                }
                // Compression and filter method must be 0; no interlacing.
                if body[10] != 0 || body[11] != 0 || body[12] != 0 {
                    return None;
                }
                header = Some((w, h, body[8], body[9]));
            }
            b"PLTE" => palette = body,
            b"tRNS" => trns = body,
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
    }
    let (w, h, depth, ctype) = header?;
    let channels: usize = match (ctype, depth) {
        (0, 1) | (0, 2) | (0, 4) | (0, 8) | (0, 16) => 1,
        (2, 8) | (2, 16) => 3,
        (3, 1) | (3, 2) | (3, 4) | (3, 8) => 1,
        (4, 8) | (4, 16) => 2,
        (6, 8) | (6, 16) => 4,
        _ => return None,
    };
    let depth = depth as usize;
    let bits_pp = channels * depth;
    let stride = (w * bits_pp + 7) / 8;
    let raw_len = h * (stride + 1);
    // A 4096 x 4096 16-bit RGBA picture is bigger than any real texture: refuse
    // before anything is inflated.
    if raw_len > MAX_INFLATE {
        return None;
    }
    let mut raw = inflate_limited(&idat, raw_len)?;
    if raw.len() != raw_len {
        return None;
    }
    drop(idat);
    unfilter(&mut raw, h, stride, (bits_pp + 7) / 8)?;

    // Sample `i` of a row as (8-bit value, full-precision value).
    let max = (1u32 << depth.min(8)) - 1;
    let sample = |row: &[u8], i: usize| -> (u8, u16) {
        match depth {
            8 => (row[i], row[i] as u16),
            16 => (row[2 * i], (row[2 * i] as u16) << 8 | row[2 * i + 1] as u16),
            _ => {
                let bit = i * depth;
                let v = (row[bit >> 3] >> (8 - depth - (bit & 7))) as u32 & max;
                ((v * 255 / max) as u8, v as u16)
            }
        }
    };
    // A tRNS chunk for grey or RGB names one fully transparent colour.
    let key_at = |i: usize| -> Option<u16> {
        Some((*trns.get(2 * i)? as u16) << 8 | *trns.get(2 * i + 1)? as u16)
    };
    let key: Option<[u16; 3]> = match ctype {
        0 => key_at(0).map(|g| [g, g, g]),
        2 => match (key_at(0), key_at(1), key_at(2)) {
            (Some(r), Some(g), Some(b)) => Some([r, g, b]),
            _ => None,
        },
        _ => None,
    };

    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        let start = y * (stride + 1) + 1;
        let row = &raw[start..start + stride];
        for x in 0..w {
            let px: [u8; 4] = match ctype {
                0 => {
                    let (g, full) = sample(row, x);
                    let a = if key == Some([full; 3]) { 0 } else { 255 };
                    [g, g, g, a]
                }
                2 => {
                    let (r, g, b) = (sample(row, 3 * x), sample(row, 3 * x + 1), sample(row, 3 * x + 2));
                    let a = if key == Some([r.1, g.1, b.1]) { 0 } else { 255 };
                    [r.0, g.0, b.0, a]
                }
                3 => {
                    let idx = sample(row, x).1 as usize;
                    // Indices past the palette decode as opaque black.
                    let c = palette.get(3 * idx..3 * idx + 3).unwrap_or(&[0, 0, 0]);
                    [c[0], c[1], c[2], trns.get(idx).copied().unwrap_or(255)]
                }
                4 => {
                    let g = sample(row, 2 * x).0;
                    [g, g, g, sample(row, 2 * x + 1).0]
                }
                _ => [
                    sample(row, 4 * x).0,
                    sample(row, 4 * x + 1).0,
                    sample(row, 4 * x + 2).0,
                    sample(row, 4 * x + 3).0,
                ],
            };
            rgba.extend_from_slice(&px);
        }
    }
    Some(Image { w, h, rgba })
}

// ------------------------------------------------------------------- encode

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Writes an 8-bit RGBA PNG without compression (stored deflate blocks).
/// Pixels missing from a too-short `rgba` are written as transparent black.
pub fn encode(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    let stride = w * 4;
    let mut raw = Vec::with_capacity(h * (stride + 1));
    for y in 0..h {
        raw.push(0); // filter: none
        let have = rgba.get(y * stride..).unwrap_or(&[]);
        let have = &have[..have.len().min(stride)];
        raw.extend_from_slice(have);
        raw.resize(raw.len() + stride - have.len(), 0);
    }

    let mut z = Vec::with_capacity(raw.len() + raw.len() / 65535 * 5 + 16);
    z.extend_from_slice(&[0x78, 0x01]);
    if raw.is_empty() {
        z.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    let blocks = (raw.len() + 65534) / 65535;
    for (i, block) in raw.chunks(65535).enumerate() {
        z.push((i + 1 == blocks) as u8);
        let len = block.len() as u16;
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = Vec::with_capacity(z.len() + 64);
    out.extend_from_slice(&SIGNATURE);
    let mut ihdr = [0u8; 13];
    ihdr[..4].copy_from_slice(&(w as u32).to_be_bytes());
    ihdr[4..8].copy_from_slice(&(h as u32).to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = 6; // colour type: RGBA
    push_chunk(&mut out, b"IHDR", &ihdr);
    push_chunk(&mut out, b"IDAT", &z);
    push_chunk(&mut out, b"IEND", &[]);
    out
}

// ------------------------------------------------------------------- base64

/// Decodes standard-alphabet base64. Whitespace is skipped; `=` padding is
/// optional but nothing except padding may follow it.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3 + 3);
    let (mut acc, mut n) = (0u32, 0u32);
    let mut padded = false;
    for &c in s.as_bytes() {
        let v = match c {
            b' ' | b'\t' | b'\n' | b'\r' => continue,
            b'=' => {
                padded = true;
                continue;
            }
            _ if padded => return None,
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = acc << 6 | v as u32;
        n += 1;
        if n == 4 {
            out.extend_from_slice(&[(acc >> 16) as u8, (acc >> 8) as u8, acc as u8]);
            acc = 0;
            n = 0;
        }
    }
    match n {
        0 => {}
        2 => out.push((acc >> 4) as u8),
        3 => out.extend_from_slice(&[(acc >> 10) as u8, (acc >> 2) as u8]),
        _ => return None,
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TERRARIUM: &[u8] = include_bytes!("../../../tests/fixtures/terrarium_8_40_97.png");
    const MAP_TILE: &[u8] = include_bytes!("../../../tests/fixtures/map_8_40_97.png");
    const ATLAS: &[u8] = include_bytes!("../../../tests/fixtures/player_atlas.png");

    fn px(img: &Image, x: usize, y: usize) -> [u8; 4] {
        let i = (y * img.w + x) * 4;
        [img.rgba[i], img.rgba[i + 1], img.rgba[i + 2], img.rgba[i + 3]]
    }

    fn hex(s: &str) -> Vec<u8> {
        let s: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        s.chunks(2).map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap()).collect()
    }

    #[test]
    fn checksums() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b""), 1);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(&vec![0xFF; 100_000]), 0x149A_302C);
    }

    #[test]
    fn base64() {
        assert_eq!(base64_decode("").unwrap(), b"");
        assert_eq!(base64_decode("Zg==").unwrap(), b"f");
        assert_eq!(base64_decode("Zm8=").unwrap(), b"fo");
        assert_eq!(base64_decode("Zm9v").unwrap(), b"foo");
        assert_eq!(base64_decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(base64_decode("Zm9v\r\n YmE=\n").unwrap(), b"fooba");
        assert_eq!(base64_decode("Zm9vYg").unwrap(), b"foob"); // padding optional
        assert_eq!(base64_decode("+/+/").unwrap(), [0xFB, 0xFF, 0xBF]);
        assert!(base64_decode("Z").is_none());
        assert!(base64_decode("Zm9v*").is_none());
        assert!(base64_decode("Zg==Zg==").is_none());
        assert!(base64_decode("Zm-_").is_none());
    }

    #[test]
    fn inflate_stored_fixed_dynamic() {
        // Stored block.
        let z = hex("7801 010500faff 68656c6c6f 062c0215");
        assert_eq!(inflate(&z).unwrap(), b"hello");
        // zlib.compress(b"hello hello hello hello"): fixed Huffman with a match.
        let z = hex(FIXED);
        assert_eq!(inflate(&z).unwrap(), b"hello hello hello hello");
        // zlib.compress(bytes((i*i) % 251 % 7 + 97 for i in range(600)), 9): dynamic Huffman.
        let z = hex(DYNAMIC);
        let want: Vec<u8> = (0..600u32).map(|i| ((i * i) % 251 % 7 + 97) as u8).collect();
        assert_eq!(inflate(&z).unwrap(), want);
    }

    #[test]
    fn inflate_rejects_bad_input() {
        let good = hex(FIXED);
        assert!(inflate(&[]).is_none());
        assert!(inflate(&good[..good.len() - 1]).is_none());
        for cut in 0..good.len() {
            assert!(inflate(&good[..cut]).is_none());
        }
        let mut bad = good.clone();
        *bad.last_mut().unwrap() ^= 1; // adler
        assert!(inflate(&bad).is_none());
        let mut bad = good.clone();
        bad[1] ^= 1; // header check
        assert!(inflate(&bad).is_none());
        // Reserved block type 3.
        assert!(inflate(&hex("789c 07 00000000 00")).is_none());
        // Flipping any single bit must never panic.
        let dynamic = hex(DYNAMIC);
        for src in [&good, &dynamic] {
            for i in 0..src.len() * 8 {
                let mut m = src.clone();
                m[i / 8] ^= 1 << (i % 8);
                let _ = inflate(&m);
            }
        }
        // The output limit stops decompression bombs.
        assert!(inflate_limited(&good, 22).is_none());
        assert!(inflate_limited(&good, 23).is_some());
    }

    #[test]
    fn terrarium_rgb() {
        let img = decode(TERRARIUM).unwrap();
        assert_eq!((img.w, img.h, img.rgba.len()), (256, 256, 256 * 256 * 4));
        assert_eq!(px(&img, 0, 0), [129, 146, 0, 255]);
        assert_eq!(px(&img, 255, 255), [129, 180, 0, 255]);
        assert_eq!(px(&img, 100, 37), [129, 163, 0, 255]);
        assert_eq!(px(&img, 17, 200), [129, 184, 0, 255]);
        assert_eq!(px(&img, 63, 63), [131, 51, 0, 255]);
        assert_eq!(px(&img, 40, 52), [130, 22, 0, 255]);
        assert_eq!(crc32(&img.rgba), TERRARIUM_CRC);
        // elevation = R*256 + G + B/256 - 32768 metres
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for p in img.rgba.chunks(4) {
            let e = p[0] as f32 * 256.0 + p[1] as f32 + p[2] as f32 / 256.0 - 32768.0;
            lo = lo.min(e);
            hi = hi.max(e);
        }
        assert_eq!((lo, hi), TERRARIUM_RANGE);
    }

    #[test]
    fn osm_palette() {
        let img = decode(MAP_TILE).unwrap();
        assert_eq!((img.w, img.h), (256, 256));
        assert_eq!(px(&img, 0, 0), [232, 201, 185, 255]);
        assert_eq!(px(&img, 255, 255), [242, 239, 233, 255]);
        assert_eq!(px(&img, 8, 8), [246, 245, 241, 255]);
        assert_eq!(px(&img, 20, 20), [178, 211, 164, 255]);
        assert_eq!(crc32(&img.rgba), OSM_CRC);
    }

    #[test]
    fn player_atlas_rgba() {
        let img = decode(ATLAS).unwrap();
        assert_eq!((img.w, img.h), (64, 64));
        assert_eq!(px(&img, 0, 0), [0, 0, 0, 0]);
        assert_eq!(px(&img, 63, 63), [0, 0, 0, 0]);
        assert_eq!(px(&img, 8, 8), [125, 94, 47, 255]);
        assert_eq!(px(&img, 20, 20), [102, 77, 38, 255]);
        assert_eq!(px(&img, 40, 52), [125, 94, 47, 255]);
        let mut colours: Vec<&[u8]> = img.rgba.chunks(4).collect();
        colours.sort();
        colours.dedup();
        assert_eq!(colours.len(), 9);
        assert_eq!(crc32(&img.rgba), ATLAS_CRC);
    }

    #[test]
    fn encode_round_trip() {
        // Wide enough that the data needs more than one stored block.
        for (w, h) in [(1usize, 1usize), (3, 5), (300, 70), (64, 64)] {
            let rgba: Vec<u8> = (0..w * h * 4).map(|i| (i * 31 + i / 7) as u8).collect();
            let file = encode(w, h, &rgba);
            assert_eq!(decode(&file), Some(Image { w, h, rgba }));
        }
        // Re-encoding a decoded fixture keeps every pixel.
        let img = decode(ATLAS).unwrap();
        assert_eq!(decode(&encode(img.w, img.h, &img.rgba)), Some(img));
        // A short buffer is padded, not a panic.
        let img = decode(&encode(2, 2, &[1, 2, 3, 4, 5, 6])).unwrap();
        assert_eq!(img.rgba, [1, 2, 3, 4, 5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        // Empty images are valid files that this decoder refuses.
        assert!(decode(&encode(0, 0, &[])).is_none());
    }

    #[test]
    fn decode_rejects_bad_input() {
        assert!(decode(&[]).is_none());
        assert!(decode(b"not a png at all").is_none());
        assert!(decode(&SIGNATURE).is_none());
        for cut in [9, 20, 33, 40, 100, ATLAS.len() - 12, ATLAS.len() - 1] {
            assert!(decode(&ATLAS[..cut]).is_none(), "cut {}", cut);
        }
        // A flipped bit anywhere in the pixel data fails a checksum.
        let mut bad = ATLAS.to_vec();
        let mid = bad.len() / 2;
        bad[mid] ^= 0x10;
        assert!(decode(&bad).is_none());

        fn with_ihdr(patch: impl Fn(&mut [u8])) -> Vec<u8> {
            let mut f = encode(2, 2, &[7; 16]);
            patch(&mut f[16..29]);
            let crc = crc32(&f[12..29]);
            f[29..33].copy_from_slice(&crc.to_be_bytes());
            f
        }
        assert!(decode(&with_ihdr(|_| {})).is_some());
        assert!(decode(&with_ihdr(|h| h[12] = 1)).is_none()); // interlaced
        assert!(decode(&with_ihdr(|h| h[8] = 4)).is_none()); // 4-bit RGBA
        assert!(decode(&with_ihdr(|h| h[9] = 5)).is_none()); // bad colour type
        assert!(decode(&with_ihdr(|h| h[3] = 3)).is_none()); // wider than the data
        assert!(decode(&with_ihdr(|h| h[..4].copy_from_slice(&[0, 0, 0x10, 1]))).is_none()); // 4097
        assert!(decode(&with_ihdr(|h| h[..4].copy_from_slice(&[0xFF; 4]))).is_none());
        assert!(decode(&with_ihdr(|h| h[..4].copy_from_slice(&[0; 4]))).is_none());

        // Single-byte corruption anywhere must never panic.
        for src in [ATLAS, &MAP_TILE[..2000]] {
            for i in 0..src.len() {
                let mut m = src.to_vec();
                m[i] = m[i].wrapping_add(0x55);
                let _ = decode(&m);
            }
        }
    }

    #[test]
    fn small_formats() {
        for (name, file, want) in SMALL {
            let img = decode(&hex(file)).unwrap_or_else(|| panic!("{} failed to decode", name));
            assert_eq!((img.w, img.h), (5, 5), "{}", name);
            assert_eq!(img.rgba, hex(want), "{}", name);
        }
    }

    /// Tiny 5x5 files (name, PNG, expected RGBA) written by a script: random pixels,
    /// a different filter on every row, some with several IDAT chunks.
    const SMALL: &[(&str, &str, &str)] = &[
        ("gray1", "89504e470d0a1a0a0000000d4948445200000005000000050100000000a5141b480000001249444154789c6358c0e8c0a4c02cc0c200000862011b7d5b951f0000000049454e44ae426082", "ffffffff000000ffffffffff000000ff000000ff000000ffffffffff000000ff000000ff000000ff000000ffffffffffffffffff000000ff000000ff000000ffffffffff000000ff000000ff000000ff000000ffffffffff000000ff000000ff000000ff"),
        ("gray2", "89504e470d0a1a0a0000000d4948445200000005000000050200000000e2b461980000001749444154789c63cc9dcca4d2c0bcb381e5dd4486a90c0025be04fca6548c2a0000000049454e44ae426082", "555555ffaaaaaaffffffffff555555ff000000ffaaaaaaff555555ff000000ff555555ffaaaaaaff000000ff000000ff000000ff555555ffffffffffffffffffaaaaaaffffffffffffffffffaaaaaaffaaaaaaff555555ff555555ff555555ff000000ff"),
        ("gray4", "89504e470d0a1a0a0000000d49484452000000050000000504000000006df494380000000a49444154789c639abf6e02f395dcaf64a8300000000a49444154772c3a7f1a1856ed7fc0d0fe96e90000000949444154a8943b1100603e0928c6cc271000000003744558746b0076cb04f3900000000049454e44ae426082", "999999ffffffffffaaaaaaffeeeeeeff999999ff222222ff333333ffddddddff555555ffaaaaaaff444444ffffffffffddddddff111111ff222222ffaaaaaaffaaaaaaffbbbbbbffffffffffeeeeeeff222222ff222222ff888888ffffffffff222222ff"),
        ("gray8", "89504e470d0a1a0a0000000d4948445200000005000000050800000000a80479390000002749444154789c6396ef9f2a5fcb3229eaa1ca4406eb3f72f993199d6c7cff9a313dbf2ec53a0100a50d0b2e546e02f50000000049454e44ae426082", "1f1f1fff9e9e9effe4e4e4ff919191ffc5c5c5ffb1b1b1ff0b0b0bffecececffb5b5b5ff565656ff3b3b3bfffcfcfcff1e1e1eff6f6f6fff939393ff424242ff7e7e7effcbcbcbffc8c8c8fffefefeff292929ff555555ffe5e5e5ffcdcdcdff8e8e8eff"),
        ("gray16", "89504e470d0a1a0a0000000d4948445200000005000000051000000000f894a57a0000004249444154789c013700c8ff04461b9652b2204614e30f00c2c976254d452a7c5a39014d76294d01b48fb6f2200210e610c319e2fceb524903a8520f8bfdeeee24d6392b62168ec89935480000000049454e44ae426082", "464646ffdcdcdcff8e8e8effd4d4d4ffb7b7b7ffc2c2c2ff767676ff4d4d4dff2a2a2aff5a5a5aff4d4d4dff767676ff777777ff060606fff8f8f8ff5d5d5dff868686ff909090ff020202ff4a4a4affd6d6d6ffbdbdbdffa3a3a3ff404040ff1b1b1bff"),
        ("gray8_trns", "89504e470d0a1a0a0000000d4948445200000005000000050800000000a80479390000000274524e53004d7efef0150000002749444154789c63f0f5397de624a3efcaeb414e4c0c252226d799d5f76fdf6bc8c2b02058451600b2a10ab766ad93920000000049454e44ae426082", "4d4d4d004c4c4cffcbcbcbffccccccffc9c9c9ff4d4d4d00f6f6f6ffcdcdcdff1f1f1fff616161ff4d4d4d006a6a6affe1e1e1ff535353ff383838ff4d4d4d001a1a1aff343434ff000000ff4d4d4d004d4d4d00bababaff0d0d0dff242424ff6a6a6aff"),
        ("gray16_trns", "89504e470d0a1a0a0000000d4948445200000005000000051000000000f894a57a0000000274524e5312342fd3495e0000004249444154789c013700c8ff01123400016ff230b609960200002cb0bae84807342303091acf2d06475f32bd02040000b8b4e821ca145d9b0012346911b9374b0f0dd8b0131052504ccc350000000049454e44ae426082", "12121200121212ff818181ffb1b1b1ffbababaff121212003e3e3eff3b3b3bfff9f9f9ffeeeeeeff12121200f7f7f7ff9f9f9fff2b2b2bff494949ff12121200afafafff878787fff5f5f5ff525252ff12121200696969ffb9b9b9ff4b4b4bff0d0d0dff"),
        ("rgb8", "89504e470d0a1a0a0000000d4948445200000005000000050802000000020db1b20000005b49444154789c015000afff04982e85232731b753bcf1d25b11ec2f00b60e0e8ff18463b0e4b2ba2970347401f064ac7893548db92b48163bb7957802bac81e85d62b5ca716d187885697d503fa2da569ac2db21e289828bf69c217a8232430b5573e440000000049454e44ae426082", "982e85ffbb55b6ff72a872ff637acdff7466fcffb60e0eff8ff184ff63b0e4ffb2ba29ff703474fff064acff68f700fff5b02bff3dc666fff45bdeffaa2ccaffedcd2bff515741ff0e4deeff4af2b3ff4f430aff073447ffde636cff0e806cff957ba6ff"),
        ("rgb16", "89504e470d0a1a0a0000000d4948445200000005000000051002000000529d6df10000002a49444154789c019b0064ff0284cbd688431c1f2eb523ea94d75c42f34dbd0993e1585dc002034cb2583d030615872ac9353c0000002a49444154291c8eec4ad39f6470bbd6a92addd8427f0e91a71e01917bc6d8d804d8f7f084691a477ae7faf0918ea9338e87cb0000002a4944415461b5055073c75856314da346572382ad00e25da1c825247b35db4f25706ce59b053ea44f13bb7c4934814dbf56ef0000002849444154974646ef7b01706d3031cbe8890d2328a7a2593189975c365bf5fbc1967809f6cc64cbfcbe0c48a9d0149c6f00000003744558746b0076cb04f3900000000049454e44ae426082", "84d643ff1fb5eaffd7424dff09e15dff024c58ff48f23dff1fa6f7ff361d7fff618d15ff32e70eff20e2a6ff668de7fff47e84ff67e546ffd53ec8ffe2a125ff7bdb25ff6c9b3eff4fbb49ff8146efff7030cbfff95372ff52dcceffadd764ffb6a32fff"),
        ("rgb8_trns", "89504e470d0a1a0a0000000d4948445200000005000000050802000000020db1b20000000674524e53000a0014001ec53629ff0000005749444154789c63e6129163e3e2df3fbf63df532d557e5f160606867abd5d07d44aeee63d9c5866c00094e7d06dd52a546ab77bc1baf62a23905fa062c2f390eb619b03d3570f26a0fa7e8f038d1f587bee799cf5940400e1711a7421065aad0000000049454e44ae426082", "0a141e000b141effc4a997ff203975ff352b87ff0a141e008a42d8ff84cf4cfffda72dff8e1d5dff0a141e00082d85ff2a7122ff873ee8ff05add5ff0a141e007a3852ff86195cff679f9cff6994e4ff0a141e00098012ff070961fff37de4ff36ddfdff"),
        ("pal1", "89504e470d0a1a0a0000000d4948445200000005000000050103000000b7a1b4a600000006504c5445c99d6e75af6527920a680000001249444154789c634c60bac02cc152c07003000bf6029b91419e960000000049454e44ae426082", "c99d6eff75af65ff75af65ffc99d6effc99d6effc99d6effc99d6eff75af65ff75af65ffc99d6effc99d6effc99d6eff75af65ff75af65ffc99d6eff75af65ffc99d6eff75af65ffc99d6effc99d6eff75af65ff75af65ffc99d6eff75af65ff75af65ff"),
        ("pal2_trns", "89504e470d0a1a0a0000000d4948445200000005000000050203000000f001ce760000000c504c5445a8a57d119e6fb65d00abc32ad69025670000000274524e5300809b2b4e180000001749444154789c637acac0bc7e038be00106f706c69bea00266704e7f335bc220000000049454e44ae426082", "abc32affb65d00ff119e6f80119e6f80a8a57d00a8a57d00b65d00ffa8a57d00119e6f80abc32affa8a57d00abc32affa8a57d00b65d00ffb65d00ff119e6f80a8a57d00119e6f80abc32affb65d00ffabc32aff119e6f80b65d00ff119e6f80a8a57d00"),
        ("pal4", "89504e470d0a1a0a0000000d49484452000000050000000504030000007f413bd600000030504c5445db4708752b0f1544b835c0e719097dfa8701e9232f21f2812687786976ebfcc327f5931765274ba9829b4406f61ff8891b0f0f210000001074524e530909090909090909090909090909090950be50ee0000001d49444154789c6336bb27c2b2e3d50586491f26303e62bbc1d462d50000597608b5b9fa6d760000000049454e44ae426082", "35c0e709e9232f091ff889096976eb096976eb094406f6094406f6094406f60935c0e709e9232f096976eb091544b8091ff88909db4708096976eb094406f6091544b8094406f6092687780965274b09e9232f09e9232f091544b8091544b80919097d09"),
        ("pal8", "89504e470d0a1a0a0000000d4948445200000005000000050803000000bab1d6d700000258504c544586b8438f39ba76fef8c90c5101fbe6cf9a48d5b0c0a13da900a6adcb3d64069481be21c9c727b8db8c188f341a924c7f88dfa161bfdb0ecc682919d2e64692f8194157f1d4af90988285cf7a9af7c93d5552266afe70e7aae6da47627c2e59af2ea37abc84670ad3c4d36bc08aad1fff8eb8406e2f8a7fc4cce4dd9f0b4110d9f2fa0025c8efe57f37724f4d37ea2b14004077139b4180df3932249962c6857200059aeb8ea17cf3787e0ed29d1c0b63ffd7298374d9bd74fc11add7b9ca6503952269fd669f6376ee71879737fd5f72f8d51c4ac91b6d0c48d41a1e5ec9e6a0392854a8615eef109fc1bfa9e2563701288f29b3d73f6ac2b69edd2c19f264bee462a5baf20fd27ecf14c011ed201f836320adb98bab1686a28d9801210c7736f3eec580dcfc43fe5d049b4d78a7a3ebb92865c8517ed02111f6a652da3524872b6a31d7ffe4587744d5eb783e96968f89be828565e07e5f7d784e9060a721ca807d7633ed123402f376e5bf1496773d19616326be5be5850336b36f13bcae48166882136805a7d1be5e9f276810fdf720d033ca4f2e53cb8ad1919dd51a9fb6d4d509ba64c8cf6803de50d83a2ecfbaeb5342071a48cb2dbd574ab29152572237c4fb659a4016f7a11bc62c5271cf64f25d6f15cc50c4b73f4c7e621513a53cc7e99cd79d7fd9c7bce4e05b0b01faee78e4ea5bf2cc362241b7dcbb2ee21414422aa0281bc1450d21386343fb93547121b38151a58ce94982f56a8679a3be12655dce528ea7c056873a18b8e73581c9be87c0bc4ab8a929e2755a1897819ea00011714c94ddd5ba1843fa74170b1b01b59b36b672d39a4490a25d600000001449444154789c6331d174bab59e4189d96eab1a63f1d21f9305e4c06200000013494441549b98cc75c316dc65de59c3f5211b009c040b678f22b81900000003744558746b0076cb04f3900000000049454e44ae426082", "62c685ff86a28dff13a53cffbf1496ffcce4ddff0ad3c4ffc90c51ffadd7b9ff12655dff406e2fff784e90ff8285cfff88dfa1ffe4e05bffff8eb8ff141442ff72f8d5ff65c851ff718797ff76fef8ff8f341affee78e4ff2ecfbaff11ed20ffa11bc6ff"),
        ("ga8", "89504e470d0a1a0a0000000d49484452000000050000000508040000002766ee6e0000004249444154789c013700c8ff00e3fc7f5400161f0ccf5f017951a4e4e92f426f1e0102e04d03641290bbed791a03fd9835b3fdd25a71952b045d3302c52eb8e818c8ab6d07175e2b7a0a160000000049454e44ae426082", "e3e3e3fc7f7f7f54000000161f1f1f0ccfcfcf5f797979511d1d1d3506060664484848d3666666d45959599e20202099181818f4030303c0dfdfdfee292929e75959597335353585767676133f3f3fab8686861a888888df878787976f6f6f2b07070756"),
        ("ga16", "89504e470d0a1a0a0000000d494844520000000500000005100400000077f6322d0000007449444154789c01690096ff03854f78e325ab150f74f23a0574d7771617726e2b046b1279d913721c1378966a73be36c4ce4ee9bb4f0057d44a0810da0dc6394a369e52d8b091489f0eb6010fce1553370e0082dcde0212fff1a329456167b302b5b921843863547f464a226df0ab5790c6bc725e9c712b73f48b82020000000049454e44ae426082", "8585857867676751a7a7a762c7c7c7a87a7a7ac2f0f0f0f10303030ddfdfdf779d9d9d6cc8c8c8275757574a1010100d39393936525252b04848480e0f0f0f154646461522222217212121ba66666621c4c4c4367e7e7e6968686839111111112c2c2c93"),
        ("rgba8", "89504e470d0a1a0a0000000d49484452000000050000000508060000008d6f26e50000001849444154789c01690096ff01f43343327463607a70ef6707ab0b0e09af30a4fd000000184944415421637b5302dfdc9c00495a75c2560e4d2c7dd77b5f5ebf686f32e5800000001849444154210392578e98afaa07de1d8de092ee48c27ea15a355f04318d2632b300000018494441547a0f0d3dc37a6eeec32310174f5ebc99d5d88c00ede08376ab6dd21a000000144944415440abec7962889a4f4f7ea7b25278a760f44b2e59b87d0bd000000003744558746b0076cb04f3900000000049454e44ae426082", "f43343326896a3acd8850ab3839018bca4f3930fd30fdf32b1f0186e2e9357df0067931b02b2fb30fb5efdb18551916d76ff543829fb35a7b630cdca2cd80cbe699b86db57c277eb4011b2a74fe6a556ede0837640abec7962889a4f4f7ea7b25278a760"),
        ("rgba16", "89504e470d0a1a0a0000000d4948445200000005000000051006000000ddfffaa6000000d849444154789c01cd0032ff048472341f54463409e09d909bf90417e7369ed48b91644142cac69fae5809039305408fe1b7cf829a000675cc4cdf8071e597a9ed320b52489b83b1cf3602d37c0cdc2cd7a17506750b5cee3f99e865dd7401a0438505321bd6d3dcd847d91e00aa345cc42b4c99f48a09f9b66690bee7fb61f66f9d128f810e1602e05dea89203e907a362d67e399c4e8ec1bb41115d456a57a01848c36c49a59aa01f044de808909760341f255319a54997d8638a73450cf3c854ec9adc2165175839eb3cdc7546226902bc18014ab870eb48ad5608b8f79f8e90000000049454e44ae426082", "8434543464c44d4b9a98de8c6437368f69c6ed1106ccdf7197ed0b4883cf027cdcd775755c3fe8dda08532d67ccc5080d8f7e90ad15da705c7fa3613806f5266b233e968f308bdafd2e96b5ec83eb61c818cc3cc1f0626d6d7b48737729bcd70c8ec6c54"),
    ];
    const DYNAMIC: &str = "78daed8fb91143410c426b451ceabf03b3df67e0d49923cd48801e18931e3c47a470b8f64a4676e22546a8a4375167b9590dcdd1269340646fc68255c6b536e12c952bbd090b0d586b25dfc643f2e6a0d681dd84c62ecc6b89ebe51be0e014a070aaf5fea49e2e4b7c9ab4d08be3b3ecbffacfaadf00adf3e9fb";
    const FIXED: &str = "789ccb48cdc9c957c8402701680308b1";
    const TERRARIUM_CRC: u32 = 0x0C32D85C;
    const TERRARIUM_RANGE: (f32, f32) = (-103.71484375, 2235.0);
    const OSM_CRC: u32 = 0xFB97C4D8;
    const ATLAS_CRC: u32 = 0xF8A27C24;
}
