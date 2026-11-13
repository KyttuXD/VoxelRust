//! The game's emblem: an isometric stone block with a "V" on its front
//! face. It is drawn by this code at any size, so there is no image file to
//! ship: the window icon, the title screen and the exported PNG all come
//! from here.

use crate::sys::sdl::sdl;
use crate::textures;
use alloc::vec::Vec;
use core::ffi::c_void;

/// Distance from point `p` to the segment `a`-`b`, squared.
fn seg_dist2(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let (wx, wy) = (p.0 - a.0, p.1 - a.1);
    let t = ((wx * vx + wy * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    let (dx, dy) = (wx - vx * t, wy - vy * t);
    dx * dx + dy * dy
}

/// Colour of the emblem at a point in the unit square, or `None` outside
/// the block.
fn sample(x: f32, y: f32) -> Option<[f32; 3]> {
    // The block: half-width W, top diamond W/2 tall, sides H tall.
    let (cx, w, y0) = (0.5, 0.46, 0.03);
    let h = 1.0 - y0 - w - 0.03;
    let dx = (x - cx) / w;
    if !(-1.0..=1.0).contains(&dx) {
        return None;
    }
    let stone = |u: f32, v: f32, salt: i32| -> [f32; 3] {
        let (tx, ty) = (((u * 16.0) as i32).clamp(0, 15), ((v * 16.0) as i32).clamp(0, 15));
        let p = textures::builtin("stone", (tx + salt * 5) % 16, (ty + salt * 3) % 16).unwrap_or([128, 128, 128, 255]);
        [p[0] as f32, p[1] as f32, p[2] as f32]
    };
    let edge = |u: f32, v: f32| u < 0.035 || v < 0.035 || u > 0.965 || v > 0.965;
    // Top face: a diamond.
    let s = 2.0 * (y - y0) / w;
    let (a, b) = ((s + dx) * 0.5, (s - dx) * 0.5);
    if (0.0..=1.0).contains(&a) && (0.0..=1.0).contains(&b) {
        let c = stone(a, b, 0);
        let k = if edge(a, b) { 0.80 } else { 1.06 };
        return Some([c[0] * k, c[1] * k, c[2] * k]);
    }
    // Side faces: parallelograms hanging from the diamond's lower edges.
    let (u, top, shade, front) = if dx < 0.0 {
        let u = dx + 1.0;
        (u, y0 + w * 0.5 + u * w * 0.5, 0.82, true)
    } else {
        (dx, y0 + w - dx * w * 0.5, 0.58, false)
    };
    let v = (y - top) / h;
    if !(0.0..=1.0).contains(&v) {
        return None;
    }
    let c = stone(u, v, if front { 1 } else { 2 });
    let mut k = if edge(u, v) { shade * 0.74 } else { shade };
    let mut out = [c[0] * k, c[1] * k, c[2] * k];
    if front {
        // The V: two strokes meeting at the bottom, gold with a dark rim.
        let p = (u, v);
        let d2 = seg_dist2(p, (0.22, 0.20), (0.50, 0.80)).min(seg_dist2(p, (0.78, 0.20), (0.50, 0.80)));
        if d2 < 0.085 * 0.085 {
            out = [255.0, 214.0 + (0.5 - v) * 40.0, 96.0];
        } else if d2 < 0.125 * 0.125 {
            k = 0.30;
            out = [c[0] * k, c[1] * k, c[2] * k];
        }
    }
    Some(out)
}

/// The emblem as `size` x `size` RGBA pixels, anti-aliased.
pub fn rgba(size: usize) -> Vec<u8> {
    let mut px = alloc::vec![0u8; size * size * 4];
    const N: usize = 3; // samples per pixel edge
    for y in 0..size {
        for x in 0..size {
            let (mut sum, mut hits) = ([0.0f32; 3], 0);
            for sy in 0..N {
                for sx in 0..N {
                    let fx = (x as f32 + (sx as f32 + 0.5) / N as f32) / size as f32;
                    let fy = (y as f32 + (sy as f32 + 0.5) / N as f32) / size as f32;
                    if let Some(c) = sample(fx, fy) {
                        for i in 0..3 {
                            sum[i] += c[i];
                        }
                        hits += 1;
                    }
                }
            }
            if hits > 0 {
                let o = (y * size + x) * 4;
                for i in 0..3 {
                    px[o + i] = (sum[i] / hits as f32).clamp(0.0, 255.0) as u8;
                }
                px[o + 3] = (hits * 255 / (N * N)) as u8;
            }
        }
    }
    px
}

/// Give the window the emblem as its icon.
pub fn set_window_icon(window: *mut c_void) {
    // Big enough to stay sharp in a dock or a task switcher.
    const SIZE: usize = if cfg!(target_os = "macos") { 256 } else { 128 };
    let mut px = rgba(SIZE);
    let api = sdl();
    unsafe {
        // Byte order R, G, B, A in memory; masks written for either endianness.
        let masks = if cfg!(target_endian = "little") {
            (0x0000_00FFu32, 0x0000_FF00u32, 0x00FF_0000u32, 0xFF00_0000u32)
        } else {
            (0xFF00_0000u32, 0x00FF_0000u32, 0x0000_FF00u32, 0x0000_00FFu32)
        };
        let surface = (api.SDL_CreateRGBSurfaceFrom)(px.as_mut_ptr() as *mut c_void, SIZE as i32, SIZE as i32, 32, (SIZE * 4) as i32, masks.0, masks.1, masks.2, masks.3);
        if !surface.is_null() {
            (api.SDL_SetWindowIcon)(window, surface);
            (api.SDL_FreeSurface)(surface);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emblem_has_a_block_shape_with_a_gold_v_on_the_left_face() {
        let n = 64;
        let px = rgba(n);
        let at = |x: usize, y: usize| -> [u8; 4] { [px[(y * n + x) * 4], px[(y * n + x) * 4 + 1], px[(y * n + x) * 4 + 2], px[(y * n + x) * 4 + 3]] };
        // Corners are transparent, the middle is solid.
        assert_eq!(at(0, 0)[3], 0);
        assert_eq!(at(63, 63)[3], 0);
        assert_eq!(at(32, 32)[3], 255);
        // The top is brighter than the left face, which is brighter than the right.
        let grey = |p: [u8; 4]| p[0] as i32 + p[1] as i32 + p[2] as i32;
        let (top, left, right) = (grey(at(32, 12)), grey(at(8, 40)), grey(at(52, 44)));
        assert!(top > left && left > right, "{top} {left} {right}");
        // Gold pixels exist, and only on the left half.
        let gold = |p: [u8; 4]| p[3] == 255 && p[0] > 230 && p[2] < 140 && p[1] > 170;
        let count = |xs: core::ops::Range<usize>| xs.flat_map(|x| (0..n).map(move |y| (x, y))).filter(|&(x, y)| gold(at(x, y))).count();
        assert!(count(0..32) > 40, "no V found");
        assert_eq!(count(33..64), 0);
    }
}
