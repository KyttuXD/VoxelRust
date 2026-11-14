//! The PNG encoder: picks the smallest honest way to write an 8-bit RGBA
//! picture (see docs/IMAGES.md).
//!
//! - 256 colours or fewer: an indexed picture (colour type 3, one byte a
//!   pixel, `PLTE` and, if anything is see-through, `tRNS`);
//! - otherwise, if nothing is see-through: RGB (colour type 2);
//! - otherwise RGBA (colour type 6).
//!
//! Every row gets whichever of the five PNG filters (none, sub, up,
//! average, Paeth) leaves the smallest sum of absolute differences; the rows
//! are then compressed with `deflate.rs`. Nothing is lost: the decoder in
//! `png.rs` reads back exactly the pixels that were written.

use super::deflate;
use super::png::{push_chunk, SIGNATURE};
use alloc::vec;
use alloc::vec::Vec;

/// A picture's colours if there are at most 256 of them: the palette (in
/// the order of first use, those with an alpha below 255 moved to the front
/// so that `tRNS` can stop early) and one index per pixel.
pub fn palette_of(px: &[u8]) -> Option<(Vec<[u8; 4]>, Vec<u8>)> {
    const SLOTS: usize = 1024;
    let mut keys = [0u32; SLOTS];
    let mut vals = [u16::MAX; SLOTS];
    let mut colours: Vec<[u8; 4]> = Vec::new();
    let mut indices: Vec<u8> = Vec::with_capacity(px.len() / 4);
    let (mut last_key, mut last_idx) = (0u32, u16::MAX);
    for p in px.chunks_exact(4) {
        let key = u32::from_le_bytes([p[0], p[1], p[2], p[3]]);
        if last_idx != u16::MAX && key == last_key {
            indices.push(last_idx as u8);
            continue;
        }
        let mut slot = (key.wrapping_mul(0x9E37_79B1) >> 22) as usize;
        let idx = loop {
            if vals[slot] == u16::MAX {
                if colours.len() >= 256 {
                    return None;
                }
                let i = colours.len() as u16;
                colours.push([p[0], p[1], p[2], p[3]]);
                keys[slot] = key;
                vals[slot] = i;
                break i;
            }
            if keys[slot] == key {
                break vals[slot];
            }
            slot = (slot + 1) % SLOTS;
        };
        last_key = key;
        last_idx = idx;
        indices.push(idx as u8);
    }
    if colours.iter().any(|c| c[3] != 255) {
        // See-through colours first, in their old order, then the opaque ones.
        let mut order: Vec<usize> = (0..colours.len()).collect();
        order.sort_by_key(|&i| (colours[i][3] == 255, i));
        let mut new_of = vec![0u8; colours.len()];
        for (new, &old) in order.iter().enumerate() {
            new_of[old] = new as u8;
        }
        for i in indices.iter_mut() {
            *i = new_of[*i as usize];
        }
        colours = order.iter().map(|&i| colours[i]).collect();
    }
    Some((colours, indices))
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

/// The sum of absolute differences of a filtered row, taking each byte as signed.
fn badness(row: &[u8]) -> u32 {
    row.iter().map(|&b| (b as i8 as i32).abs() as u32).sum()
}

/// Filter every row of `data` (`h` rows of `stride` bytes, `bpp` bytes to
/// the pixel on the left): each comes out as a filter byte and the row.
fn filter_rows(data: &[u8], stride: usize, h: usize, bpp: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(h * (stride + 1));
    let zero = vec![0u8; stride];
    let mut cand: [Vec<u8>; 5] = [vec![0; stride], vec![0; stride], vec![0; stride], vec![0; stride], vec![0; stride]];
    for y in 0..h {
        let cur = &data[y * stride..(y + 1) * stride];
        let prev: &[u8] = if y == 0 { &zero } else { &data[(y - 1) * stride..y * stride] };
        // A row like the one above is all zeros under "up": the common case of flat areas.
        if y > 0 && cur == prev {
            out.push(2);
            out.resize(out.len() + stride, 0);
            continue;
        }
        for i in 0..stride {
            let left = if i >= bpp { cur[i - bpp] } else { 0 };
            let up = prev[i];
            let diag = if i >= bpp { prev[i - bpp] } else { 0 };
            cand[0][i] = cur[i];
            cand[1][i] = cur[i].wrapping_sub(left);
            cand[2][i] = cur[i].wrapping_sub(up);
            cand[3][i] = cur[i].wrapping_sub(((left as u16 + up as u16) / 2) as u8);
            cand[4][i] = cur[i].wrapping_sub(paeth(left, up, diag));
        }
        let mut best = 0;
        let mut best_score = badness(&cand[0]);
        for f in 1..5 {
            let s = badness(&cand[f]);
            if s < best_score {
                best = f;
                best_score = s;
            }
        }
        out.push(best as u8);
        out.extend_from_slice(&cand[best]);
    }
    out
}

/// Writes an 8-bit RGBA picture as a PNG, as small as the kinds above allow.
/// Pixels missing from a too-short `rgba` are written as transparent black.
pub fn encode(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    let need = w.saturating_mul(h).saturating_mul(4);
    let padded;
    let px: &[u8] = if rgba.len() >= need {
        &rgba[..need]
    } else {
        let mut v = rgba.to_vec();
        v.resize(need, 0);
        padded = v;
        &padded
    };

    let (ctype, bpp, plte, trns, data): (u8, usize, Vec<u8>, Vec<u8>, Vec<u8>) = if need == 0 {
        (6, 4, Vec::new(), Vec::new(), Vec::new())
    } else if let Some((colours, indices)) = palette_of(px) {
        let plte: Vec<u8> = colours.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
        // tRNS only has to reach the last colour that is not opaque.
        let last = colours.iter().rposition(|c| c[3] != 255);
        let trns: Vec<u8> = match last {
            Some(i) => colours[..=i].iter().map(|c| c[3]).collect(),
            None => Vec::new(),
        };
        (3, 1, plte, trns, indices)
    } else if px.chunks_exact(4).all(|p| p[3] == 255) {
        (2, 3, Vec::new(), Vec::new(), px.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect())
    } else {
        (6, 4, Vec::new(), Vec::new(), px.to_vec())
    };

    let stride = if need == 0 { 0 } else { data.len() / h };
    let filtered = if need == 0 { Vec::new() } else { filter_rows(&data, stride, h, bpp) };
    drop(data);
    let z = deflate::zlib(&filtered);
    drop(filtered);

    let mut out = Vec::with_capacity(z.len() + plte.len() + 100);
    out.extend_from_slice(&SIGNATURE);
    let mut ihdr = [0u8; 13];
    ihdr[..4].copy_from_slice(&(w as u32).to_be_bytes());
    ihdr[4..8].copy_from_slice(&(h as u32).to_be_bytes());
    ihdr[8] = 8; // bit depth
    ihdr[9] = ctype;
    push_chunk(&mut out, b"IHDR", &ihdr);
    if ctype == 3 {
        push_chunk(&mut out, b"PLTE", &plte);
        if !trns.is_empty() {
            push_chunk(&mut out, b"tRNS", &trns);
        }
    }
    push_chunk(&mut out, b"IDAT", &z);
    push_chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::png::{decode, Image};

    fn back(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
        let file = encode(w, h, rgba);
        let img = decode(&file).expect("the encoder's file is read");
        assert_eq!((img.w, img.h), (w, h));
        assert_eq!(img.rgba, rgba, "pixels differ");
        file
    }

    fn colour_type(file: &[u8]) -> u8 {
        file[25]
    }

    #[test]
    fn few_colours_make_an_indexed_picture_with_alpha_kept() {
        // Four colours, one of them half see-through, one fully clear.
        let cols = [[10u8, 20, 30, 255], [200, 100, 50, 128], [0, 0, 0, 0], [255, 255, 255, 255]];
        let (w, h) = (64usize, 64usize);
        let mut px = Vec::new();
        for i in 0..w * h {
            px.extend_from_slice(&cols[(i / 7 + i / 64) % 4]);
        }
        let file = back(w, h, &px);
        assert_eq!(colour_type(&file), 3);
        assert!(file.windows(4).any(|x| x == b"tRNS"));
        assert!(file.len() < w * h, "{}", file.len());
    }

    #[test]
    fn many_colours_make_rgb_or_rgba() {
        let (w, h) = (40usize, 30usize);
        let opaque: Vec<u8> = (0..w * h).flat_map(|i| [(i * 7) as u8, (i / 3) as u8, (i * 13 >> 2) as u8, 255]).collect();
        let f = back(w, h, &opaque);
        assert_eq!(colour_type(&f), 2);
        let mut clear = opaque.clone();
        clear[3] = 99;
        let f = back(w, h, &clear);
        assert_eq!(colour_type(&f), 6);
    }

    #[test]
    fn a_picture_of_every_gradient_round_trips_through_each_filter() {
        // Rows that favour sub, up, average and Paeth in turn.
        let (w, h) = (97usize, 80usize);
        let mut px = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let v = match y / 20 {
                    0 => (x * 5) as u8,
                    1 => (y * 9 + x % 2) as u8,
                    2 => ((x * 3 + y * 3) / 2) as u8,
                    _ => ((x * x + y * 31) % 251) as u8,
                };
                let o = (y * w + x) * 4;
                px[o..o + 4].copy_from_slice(&[v, v ^ 0x55, v.wrapping_mul(3), 255 - (x % 5) as u8]);
            }
        }
        back(w, h, &px);
    }

    #[test]
    fn sizes_from_a_tile_to_a_big_skin_round_trip_and_flat_ones_shrink() {
        for side in [1usize, 16, 64, 128, 512] {
            // Mostly flat: one colour with a few marks.
            let mut px = vec![0u8; side * side * 4];
            for p in px.chunks_exact_mut(4) {
                p.copy_from_slice(&[90, 140, 60, 255]);
            }
            for i in 0..side.min(20) {
                let o = (i * side + i) * 4;
                px[o..o + 4].copy_from_slice(&[200, 30, 30, 255]);
            }
            let file = back(side, side, &px);
            if side >= 64 {
                assert!(file.len() < side * side / 4 + 400, "{} px: {} bytes", side, file.len());
            }
        }
    }

    #[test]
    fn four_thousand_ninety_six_square_of_mostly_flat_pixels_round_trips() {
        let side = 4096usize;
        let mut px = vec![0u8; side * side * 4];
        for p in px.chunks_exact_mut(4) {
            p.copy_from_slice(&[30, 60, 90, 255]);
        }
        // A patch of 300 colours (more than a palette holds), so that the picture is RGB.
        for i in 0..300usize {
            let o = (i * 5 * side + i * 3) * 4;
            px[o..o + 4].copy_from_slice(&[(i % 256) as u8, (i * 7 % 256) as u8, (i / 2) as u8, 255]);
        }
        let file = back(side, side, &px);
        assert_eq!(colour_type(&file), 2);
        assert!(file.len() < 200_000, "{}", file.len());
    }

    #[test]
    fn a_noisy_picture_is_not_much_bigger_than_its_pixels() {
        let (w, h) = (128usize, 128usize);
        let mut s = 12345u32;
        let px: Vec<u8> = (0..w * h * 4)
            .map(|i| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                if i % 4 == 3 {
                    255
                } else {
                    (s >> 24) as u8
                }
            })
            .collect();
        let file = back(w, h, &px);
        assert!(file.len() < w * h * 3 + 300, "{}", file.len());
    }

    #[test]
    fn short_and_empty_input_is_padded_not_a_panic() {
        let img = decode(&encode(2, 2, &[1, 2, 3, 4, 5, 6])).unwrap();
        assert_eq!(img.rgba, [1, 2, 3, 4, 5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(decode(&encode(0, 0, &[])).is_none());
        assert!(decode(&encode(0, 5, &[])).is_none());
    }

    #[test]
    fn the_palette_puts_see_through_colours_first() {
        let px = [1u8, 1, 1, 255, 2, 2, 2, 10, 3, 3, 3, 255, 4, 4, 4, 0];
        let (cols, idx) = palette_of(&px).unwrap();
        assert_eq!(cols.len(), 4);
        assert!(cols[0][3] != 255 && cols[1][3] != 255 && cols[2][3] == 255 && cols[3][3] == 255);
        for (i, p) in px.chunks_exact(4).enumerate() {
            assert_eq!(&cols[idx[i] as usize][..], p);
        }
        // 257 colours are too many.
        let many: Vec<u8> = (0..257u32).flat_map(|i| [i as u8, (i >> 8) as u8, 0, 255]).collect();
        assert!(palette_of(&many).is_none());
        let exactly: Vec<u8> = (0..256u32).flat_map(|i| [i as u8, 0, 0, 255]).collect();
        assert_eq!(palette_of(&exactly).unwrap().0.len(), 256);
    }

    #[test]
    fn encoded_files_survive_the_picture_reader() {
        let img = Image { w: 3, h: 2, rgba: vec![9; 24] };
        let got = crate::image::decode(&encode(img.w, img.h, &img.rgba)).unwrap();
        assert_eq!(got, img);
    }
}
