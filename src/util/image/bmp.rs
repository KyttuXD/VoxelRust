//! BMP (1, 4, 8, 24 and 32 bits).

use super::{Image, MAX_SIDE};
use alloc::vec::Vec;

pub(super) fn le16(d: &[u8], at: usize) -> Option<u32> {
    Some(u16::from_le_bytes([*d.get(at)?, *d.get(at + 1)?]) as u32)
}

pub(super) fn le32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*d.get(at)?, *d.get(at + 1)?, *d.get(at + 2)?, *d.get(at + 3)?]))
}

pub(super) fn bmp(d: &[u8]) -> Option<Image> {
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