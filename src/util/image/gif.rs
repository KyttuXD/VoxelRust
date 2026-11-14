//! GIF (the first picture).

use super::bmp::le16;
use super::{Image, MAX_SIDE};
use alloc::vec::Vec;

pub(super) fn gif(d: &[u8]) -> Option<Image> {
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
                if label == 0xF9 && d.get(at)? >= &4 && d.get(at + 1)? & 1 != 0 {
                    transparent = Some(*d.get(at + 4)?);
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
                // The screen size is the file's own word: 65535 x 65535 would be 16 GB of pixels.
                if w > MAX_SIDE || h > MAX_SIDE {
                    return None;
                }
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
pub(super) fn lzw(raw: &[u8], min_code: u32, want: usize) -> Option<Vec<u8>> {
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