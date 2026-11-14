//! Turning a picture the player picked into a skin: a square picture whose
//! side is a multiple of 64 up to 4096 as it is, the older 2:1 layout (64 x 32,
//! 128 x 64...) brought up to a square at its own scale (the left arm and leg
//! are mirrored copies of the right ones). See `content::skin_grid`.

use crate::png::Image;
use alloc::string::String;
use alloc::vec::Vec;

/// Whether a picture of this size can be a skin.
pub fn size_ok(w: usize, h: usize) -> bool {
    crate::skin_grid::valid_size(w, h)
}

/// A skin's pixels as a square (`img.w` x `img.w`) RGBA picture, or the
/// reason it cannot be one.
pub fn from_image(img: &Image) -> Result<Vec<u8>, String> {
    if !size_ok(img.w, img.h) || img.rgba.len() != img.w * img.h * 4 {
        return Err(crate::skin_grid::refusal(img.w, img.h));
    }
    if img.h == img.w {
        return Ok(img.rgba.clone());
    }
    let side = img.w;
    let k = side / 64;
    let mut px = alloc::vec![0u8; side * side * 4];
    px[..img.rgba.len()].copy_from_slice(&img.rgba);
    // Right leg (0, 16) -> left leg (16, 48); right arm (40, 16) -> left arm (32, 48), on the 64 grid.
    mirror_limb(&mut px, side, k, 0, 16, 16, 48);
    mirror_limb(&mut px, side, k, 40, 16, 32, 48);
    Ok(px)
}

/// Whether a skin picture is for slim arms (three pixels wide, "Alex") and not wide ones.
/// The wide-only columns of the arm layouts are left clear in a slim skin: the way
/// skin viewers tell them apart. Works at any size that is a multiple of 64 wide.
pub fn is_slim(px: &[u8], w: usize, h: usize) -> bool {
    if w < 64 || w % 64 != 0 || h != w || px.len() != w * h * 4 {
        return false;
    }
    let k = w / 64;
    // (x, y, width, height) in 64 x 64 units: the spare column of each arm's top and front.
    let regions: [(usize, usize, usize, usize); 4] = [(50, 16, 2, 4), (54, 20, 2, 12), (42, 48, 2, 4), (46, 52, 2, 12)];
    regions.iter().all(|&(rx, ry, rw, rh)| {
        (ry * k..(ry + rh) * k).all(|y| (rx * k..(rx + rw) * k).all(|x| px[(y * w + x) * 4 + 3] == 0))
    })
}

/// Copy a 4 x 12 x 4 limb laid out at (sx, sy) to (dx, dy), mirrored: every
/// face flips, and the two sides swap. Places and sizes are in cells of the 64
/// grid; a cell is `k` pixels across, and the picture `side` pixels wide.
fn mirror_limb(px: &mut [u8], side: usize, k: usize, sx: usize, sy: usize, dx: usize, dy: usize) {
    // (source x, source y, width, height, destination x offset, destination y offset): the faces
    // in the layout are top, bottom, right, front, left, back.
    let faces: [(usize, usize, usize, usize, usize, usize); 6] = [
        (4, 0, 4, 4, 4, 0),
        (8, 0, 4, 4, 8, 0),
        (0, 4, 4, 12, 8, 4),
        (4, 4, 4, 12, 4, 4),
        (8, 4, 4, 12, 0, 4),
        (12, 4, 4, 12, 12, 4),
    ];
    for (fx, fy, w, h, tx, ty) in faces {
        let (pw, ph) = (w * k, h * k);
        for y in 0..ph {
            for x in 0..pw {
                let from = (((sy + fy) * k + y) * side + (sx + fx) * k + x) * 4;
                let to = (((dy + ty) * k + y) * side + (dx + tx) * k + (pw - 1 - x)) * 4;
                let p = [px[from], px[from + 1], px[from + 2], px[from + 3]];
                px[to..to + 4].copy_from_slice(&p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_skin_sizes_are_accepted() {
        assert!(size_ok(64, 64) && size_ok(64, 32));
        assert!(size_ok(128, 128) && size_ok(128, 64) && size_ok(1024, 1024) && size_ok(4096, 4096));
        assert!(!size_ok(32, 32) && !size_ok(64, 48) && !size_ok(100, 100) && !size_ok(8192, 8192) && !size_ok(192, 64));
        let img = Image { w: 16, h: 16, rgba: alloc::vec![0; 16 * 16 * 4] };
        let e = from_image(&img).unwrap_err();
        assert!(e.contains("16 X 16"), "{}", e);
    }

    #[test]
    fn a_square_skin_is_kept_as_it_is() {
        let rgba: Vec<u8> = (0..64 * 64 * 4).map(|i| (i % 251) as u8).collect();
        let out = from_image(&Image { w: 64, h: 64, rgba: rgba.clone() }).unwrap();
        assert_eq!(out, rgba);
        let big: Vec<u8> = (0..256 * 256 * 4).map(|i| (i % 249) as u8).collect();
        assert_eq!(from_image(&Image { w: 256, h: 256, rgba: big.clone() }).unwrap(), big);
    }

    #[test]
    fn an_old_layout_skin_at_twice_the_size_is_mirrored_at_its_own_scale() {
        // 128 x 64: the right leg's front face is at (4, 20) in the 64 grid, (8, 40) here, 8 wide.
        let mut rgba = alloc::vec![0u8; 128 * 64 * 4];
        for (x, v) in [(8usize, 11u8), (15, 22)] {
            let o = (40 * 128 + x) * 4;
            rgba[o..o + 4].copy_from_slice(&[v, v, v, 255]);
        }
        let out = from_image(&Image { w: 128, h: 64, rgba: rgba.clone() }).unwrap();
        assert_eq!(out.len(), 128 * 128 * 4);
        assert_eq!(&out[..128 * 64 * 4], &rgba[..]);
        // The left leg's front face is at (20, 52) on the grid: (40, 104) here, flipped across its 8 pixels.
        assert_eq!(out[(104 * 128 + 47) * 4], 11);
        assert_eq!(out[(104 * 128 + 40) * 4], 22);
    }

    #[test]
    fn an_old_skin_gets_mirrored_left_limbs() {
        let mut rgba = alloc::vec![0u8; 64 * 32 * 4];
        // Mark the left edge of the right leg's front face (4, 20) and its
        // right edge (7, 20).
        for (x, v) in [(4usize, 11u8), (7, 22)] {
            let o = (20 * 64 + x) * 4;
            rgba[o..o + 4].copy_from_slice(&[v, v, v, 255]);
        }
        // And the first pixel of the right arm's right face (40, 20).
        let o = (20 * 64 + 40) * 4;
        rgba[o..o + 4].copy_from_slice(&[33, 33, 33, 255]);
        let out = from_image(&Image { w: 64, h: 32, rgba: rgba.clone() }).unwrap();
        assert_eq!(out.len(), 64 * 64 * 4);
        // The top half is untouched.
        assert_eq!(&out[..64 * 32 * 4], &rgba[..]);
        // The left leg's front is at x 20..24, rows 52..: flipped sideways.
        assert_eq!(out[(52 * 64 + 23) * 4], 11);
        assert_eq!(out[(52 * 64 + 20) * 4], 22);
        // The arm's right face goes to the left face of the left arm (x 40..44 from 32+8).
        assert_eq!(out[(52 * 64 + 32 + 8 + 3) * 4], 33);
    }

    #[test]
    fn slim_skins_are_told_from_wide_ones_by_their_clear_columns() {
        let mut wide = alloc::vec![255u8; 64 * 64 * 4];
        assert!(!is_slim(&wide, 64, 64));
        let mut slim = wide.clone();
        for (rx, ry, rw, rh) in [(50usize, 16usize, 2usize, 4usize), (54, 20, 2, 12), (42, 48, 2, 4), (46, 52, 2, 12)] {
            for y in ry..ry + rh {
                for x in rx..rx + rw {
                    slim[(y * 64 + x) * 4 + 3] = 0;
                }
            }
        }
        assert!(is_slim(&slim, 64, 64));
        // The same skin at twice the size.
        let mut big = alloc::vec![255u8; 128 * 128 * 4];
        for (rx, ry, rw, rh) in [(50usize, 16usize, 2usize, 4usize), (54, 20, 2, 12), (42, 48, 2, 4), (46, 52, 2, 12)] {
            for y in ry * 2..(ry + rh) * 2 {
                for x in rx * 2..(rx + rw) * 2 {
                    big[(y * 128 + x) * 4 + 3] = 0;
                }
            }
        }
        assert!(is_slim(&big, 128, 128));
        assert!(!is_slim(&wide[..64 * 32 * 4], 64, 32));
        wide[3] = 0;
        assert!(!is_slim(&wide, 64, 64));
    }
}
