//! Making a picture smaller: a box filter (every new pixel is the average of
//! the square of old ones it covers). Skins are shown smaller than they are
//! to players far away and to older game versions, which know only 64 x 64.

use super::png::Image;
use alloc::vec;
use alloc::vec::Vec;

/// `rgba` (`w` x `h` pixels) averaged down to `side` x `side`. Colours weigh by
/// their alpha, so see-through pixels do not darken their neighbours; a flat
/// picture stays exactly as flat. No copy of the source is made.
fn average(rgba: &[u8], w: usize, h: usize, side: usize) -> Vec<u8> {
    let mut out = vec![0u8; side * side * 4];
    for y in 0..side {
        // The source rows and columns this pixel covers.
        let (y0, y1) = (y * h / side, ((y + 1) * h / side).max(y * h / side + 1).min(h));
        for x in 0..side {
            let (x0, x1) = (x * w / side, ((x + 1) * w / side).max(x * w / side + 1).min(w));
            let (mut sum, mut a_sum, mut n) = ([0u32; 3], 0u32, 0u32);
            for sy in y0..y1 {
                for p in rgba[(sy * w + x0) * 4..(sy * w + x1) * 4].chunks_exact(4) {
                    let a = p[3] as u32;
                    sum = [sum[0] + p[0] as u32 * a, sum[1] + p[1] as u32 * a, sum[2] + p[2] as u32 * a];
                    a_sum += a;
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

/// `img` (square or not) averaged down to `side` x `side`. A picture already
/// that small, or smaller, comes back as it is.
pub fn downscale(img: &Image, side: usize) -> Image {
    if side == 0 || (img.w <= side && img.h <= side) || img.rgba.len() != img.w * img.h * 4 {
        return img.clone();
    }
    Image { w: side, h: side, rgba: average(&img.rgba, img.w, img.h, side) }
}

/// The same for the bare pixels of a `w` x `w` picture.
pub fn downscale_square(rgba: &[u8], w: usize, side: usize) -> Vec<u8> {
    if side == 0 || side >= w || rgba.len() != w * w * 4 {
        return rgba.to_vec();
    }
    average(rgba, w, w, side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_pictures_stay_flat_and_alpha_is_kept() {
        for (c, a) in [([10u8, 200, 30], 255u8), ([7, 8, 9], 100), ([0, 0, 0], 0)] {
            let rgba: Vec<u8> = (0..128 * 128).flat_map(|_| [c[0], c[1], c[2], a]).collect();
            let small = downscale(&Image { w: 128, h: 128, rgba }, 64);
            assert_eq!((small.w, small.h), (64, 64));
            for p in small.rgba.chunks_exact(4) {
                assert_eq!(p[3], a);
                if a > 0 {
                    assert_eq!(&p[..3], &c[..]);
                }
            }
        }
    }

    #[test]
    fn a_checker_averages_to_grey_and_clear_pixels_do_not_darken() {
        // Black and white squares of one pixel: every 2 x 2 block averages to the middle.
        let mut rgba = Vec::new();
        for y in 0..8usize {
            for x in 0..8usize {
                let v = if (x + y) % 2 == 0 { 0 } else { 254 };
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let s = downscale(&Image { w: 8, h: 8, rgba }, 4);
        assert!(s.rgba.chunks_exact(4).all(|p| p[0] == 127 && p[3] == 255));
        // One opaque red pixel among three clear ones: the colour is red, the alpha a quarter.
        let rgba = alloc::vec![255, 0, 0, 255, 9, 9, 9, 0, 9, 9, 9, 0, 9, 9, 9, 0];
        let s = downscale(&Image { w: 2, h: 2, rgba }, 1);
        assert_eq!(s.rgba, [255, 0, 0, 63]);
    }

    #[test]
    fn a_big_skin_is_averaged_to_64_and_keeps_its_parts() {
        // Left half red, right half blue: the 64 version has the same halves.
        let side = 256usize;
        let mut rgba = Vec::with_capacity(side * side * 4);
        for _y in 0..side {
            for x in 0..side {
                rgba.extend_from_slice(if x < side / 2 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
            }
        }
        let small = downscale_square(&rgba, side, 64);
        assert_eq!(&small[..4], &[255, 0, 0, 255]);
        assert_eq!(&small[(63 * 64 + 63) * 4..][..4], &[0, 0, 255, 255]);
        assert_eq!(&small[(10 * 64 + 31) * 4..][..4], &[255, 0, 0, 255]);
        assert_eq!(&small[(10 * 64 + 32) * 4..][..4], &[0, 0, 255, 255]);
    }

    #[test]
    fn small_pictures_are_not_enlarged() {
        let img = Image { w: 64, h: 64, rgba: alloc::vec![5; 64 * 64 * 4] };
        assert_eq!(downscale(&img, 64), img);
        assert_eq!(downscale(&img, 256), img);
        assert_eq!(downscale_square(&img.rgba, 64, 256), img.rgba);
        let big = alloc::vec![9u8; 256 * 256 * 4];
        assert_eq!(downscale_square(&big, 256, 64).len(), 64 * 64 * 4);
    }
}
