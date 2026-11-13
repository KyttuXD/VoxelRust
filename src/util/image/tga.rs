//! TGA (true colour, grey, mapped; plain and run-length).

use super::bmp::{le16};
use super::{Image, MAX_SIDE};
use alloc::vec::Vec;

pub(super) fn tga(d: &[u8]) -> Option<Image> {
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