//! Making pictures smaller: the box filter that shrinks an HD tile to the
//! TEXTURE SIZE or to the 16 x 16 of the normal atlas, and the halving that
//! makes the mip levels of the HD atlas.
//!
//! Colours are weighted by alpha, so a see-through pixel does not darken its
//! neighbours, and the alpha of the result is the average (the shrunk tile
//! of a leaf keeps its holes).

use alloc::vec::Vec;

/// Shrink a `from` x `from` picture to `to` x `to` (`to` at most `from`).
pub fn box_down(src: &[u8], from: usize, to: usize) -> Vec<u8> {
    box_down_wh(src, from, from, to, to)
}

/// Shrink a `w` x `h` picture to `nw` x `nh` (no bigger). Each new pixel is
/// the average of the old ones it covers; the sizes need not divide.
pub fn box_down_wh(src: &[u8], w: usize, h: usize, nw: usize, nh: usize) -> Vec<u8> {
    let (nw, nh) = (nw.min(w).max(1), nh.min(h).max(1));
    if (nw, nh) == (w, h) {
        return src.to_vec();
    }
    let mut out = alloc::vec![0u8; nw * nh * 4];
    for y in 0..nh {
        let y0 = y * h / nh;
        let y1 = ((y + 1) * h / nh).max(y0 + 1).min(h);
        for x in 0..nw {
            let x0 = x * w / nw;
            let x1 = ((x + 1) * w / nw).max(x0 + 1).min(w);
            let (mut r, mut g, mut b, mut asum, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for sy in y0..y1 {
                let row = &src[(sy * w + x0) * 4..(sy * w + x1) * 4];
                for p in row.chunks_exact(4) {
                    let a = p[3] as u64;
                    r += p[0] as u64 * a;
                    g += p[1] as u64 * a;
                    b += p[2] as u64 * a;
                    asum += a;
                    n += 1;
                }
            }
            let o = (y * nw + x) * 4;
            if asum > 0 {
                out[o] = ((r + asum / 2) / asum) as u8;
                out[o + 1] = ((g + asum / 2) / asum) as u8;
                out[o + 2] = ((b + asum / 2) / asum) as u8;
            }
            out[o + 3] = if n > 0 { ((asum + n / 2) / n) as u8 } else { 0 };
        }
    }
    out
}

/// Halve a `w` x `h` picture (both even) for the next mip level. Colours are
/// weighted by alpha; alpha keeps its maximum, so cut-outs do not fade away
/// in the distance (the same rule as `textures::half_size`).
pub fn half_wh(src: &[u8], w: usize, h: usize) -> Vec<u8> {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = alloc::vec![0u8; nw * nh * 4];
    for y in 0..nh {
        for x in 0..nw {
            let (mut r, mut g, mut b, mut wsum, mut amax) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (sx, sy) = ((x * 2 + dx).min(w - 1), (y * 2 + dy).min(h - 1));
                let o = (sy * w + sx) * 4;
                let a = src[o + 3] as u32;
                r += src[o] as u32 * a;
                g += src[o + 1] as u32 * a;
                b += src[o + 2] as u32 * a;
                wsum += a;
                amax = amax.max(a);
            }
            let o = (y * nw + x) * 4;
            if wsum > 0 {
                out[o] = (r / wsum) as u8;
                out[o + 1] = (g / wsum) as u8;
                out[o + 2] = (b / wsum) as u8;
            }
            out[o + 3] = amax as u8;
        }
    }
    out
}

/// Put a `side` x `side` picture into a page `page_w` pixels wide at (`x`, `y`).
pub fn blit(page: &mut [u8], page_w: usize, x: usize, y: usize, px: &[u8], side: usize) {
    for row in 0..side {
        let dst = ((y + row) * page_w + x) * 4;
        page[dst..dst + side * 4].copy_from_slice(&px[row * side * 4..(row + 1) * side * 4]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(side: usize, rgba: [u8; 4]) -> Vec<u8> {
        (0..side * side).flat_map(|_| rgba).collect()
    }

    #[test]
    fn the_same_size_is_a_copy() {
        let a = solid(16, [1, 2, 3, 4]);
        assert_eq!(box_down(&a, 16, 16), a);
    }

    #[test]
    fn a_solid_picture_stays_that_colour_at_any_size() {
        for (from, to) in [(512, 16), (512, 256), (128, 16), (64, 32), (48, 32), (80, 64), (96, 16), (400, 16)] {
            let out = box_down(&solid(from, [200, 100, 50, 255]), from, to);
            assert_eq!(out.len(), to * to * 4, "{from}->{to}");
            assert!(out.chunks(4).all(|p| p == [200, 100, 50, 255]), "{from}->{to}");
        }
    }

    #[test]
    fn four_pixels_average_to_one() {
        // 2 x 2: red, green, blue, white, all solid.
        let src = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
        let out = box_down(&src, 2, 1);
        assert_eq!(out.len(), 4);
        assert_eq!(out[3], 255);
        assert_eq!(out[0], 128, "red: (255+0+0+255)/4");
        assert_eq!(out[1], 128);
        assert_eq!(out[2], 128);
    }

    #[test]
    fn see_through_pixels_do_not_darken_their_neighbours() {
        // One white solid pixel and three clear black ones.
        let src = [255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let out = box_down(&src, 2, 1);
        assert_eq!(&out[0..3], &[255, 255, 255], "colour is that of the solid pixel");
        assert_eq!(out[3], 64, "alpha is the average");
        let clear = box_down(&solid(4, [9, 9, 9, 0]), 4, 2);
        assert!(clear.chunks(4).all(|p| p[3] == 0));
    }

    #[test]
    fn a_checker_of_tiles_keeps_its_halves() {
        // 32 x 32, left half red, right half blue: at 16 x 16 still so.
        let mut src = alloc::vec![0u8; 32 * 32 * 4];
        for y in 0..32 {
            for x in 0..32 {
                let o = (y * 32 + x) * 4;
                src[o..o + 4].copy_from_slice(if x < 16 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
            }
        }
        let out = box_down(&src, 32, 16);
        assert_eq!(&out[0..4], &[255, 0, 0, 255]);
        assert_eq!(&out[(15 * 16 + 15) * 4..][..4], &[0, 0, 255, 255]);
    }

    #[test]
    fn halving_keeps_the_largest_alpha() {
        let src = [10, 20, 30, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let out = half_wh(&src, 2, 2);
        assert_eq!(out, alloc::vec![10, 20, 30, 255]);
        assert_eq!(half_wh(&solid(8, [5, 5, 5, 255]), 8, 4).len(), 4 * 2 * 4);
    }

    #[test]
    fn blitting_puts_a_picture_where_it_is_told() {
        let mut page = alloc::vec![0u8; 8 * 8 * 4];
        blit(&mut page, 8, 4, 2, &solid(2, [9, 8, 7, 6]), 2);
        assert_eq!(&page[(2 * 8 + 4) * 4..][..4], &[9, 8, 7, 6]);
        assert_eq!(&page[(3 * 8 + 5) * 4..][..4], &[9, 8, 7, 6]);
        assert_eq!(&page[(2 * 8 + 3) * 4..][..4], &[0, 0, 0, 0]);
        assert_eq!(&page[(4 * 8 + 4) * 4..][..4], &[0, 0, 0, 0]);
    }
}
