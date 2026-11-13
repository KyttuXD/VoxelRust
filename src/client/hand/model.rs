//! The geometry the hand and the items it holds are made of: flat-shaded
//! quads from the block atlas, boxes, and pictures made solid.

use crate::math::{cosf, sinf};
use crate::textures::PER_ROW;
use alloc::vec::Vec;

/// One flat-shaded quad: corners in order round its face, atlas coordinates
/// (0..1 over the whole atlas), and a colour that multiplies the texture.
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub p: [[f32; 3]; 4],
    pub uv: [[f32; 2]; 4],
    pub col: [u8; 4],
}

/// A tile's place in the atlas, `(u, v)` in 0..1 across the tile.
pub(super) fn atlas(tile: u16, u: f32, v: f32) -> [f32; 2] {
    let n = PER_ROW as f32;
    [((tile as usize % PER_ROW) as f32 + u) / n, ((tile as usize / PER_ROW) as f32 + v) / n]
}

/// A tint times a face's shade (the same light levels the models use).
pub(super) fn shade(tint: [u8; 3], k: u32) -> [u8; 4] {
    [(tint[0] as u32 * k / 255) as u8, (tint[1] as u32 * k / 255) as u8, (tint[2] as u32 * k / 255) as u8, 255]
}

/// A whole tile, corners in order round the face.
pub(super) const FULL: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
/// A whole tile on an upright face: the corners at the bottom of the face
/// take the bottom of the tile, as the world mesh does (its v runs down the
/// tile). Without this, textures come out upside down.
pub(super) const UPRIGHT: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

pub(super) fn quad(out: &mut Vec<Quad>, p: [[f32; 3]; 4], tile: u16, col: [u8; 4], corners: [[f32; 2]; 4]) {
    let uv = corners.map(|c| atlas(tile, c[0], c[1]));
    out.push(Quad { p, uv, col });
}

/// A box from `lo` to `hi`. `tiles` gives each face its tile (north, south,
/// east, west, top, bottom); each face is shaded as the models shade theirs.
pub(super) fn boxq(out: &mut Vec<Quad>, lo: [f32; 3], hi: [f32; 3], tiles: [u16; 6], tint: [u8; 3]) {
    let [x0, y0, z0] = lo;
    let [x1, y1, z1] = hi;
    let faces: [([[f32; 3]; 4], u32); 6] = [
        ([[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]], 228), // north, -z
        ([[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]], 228), // south, +z
        ([[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]], 200), // east, +x
        ([[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]], 200), // west, -x
        ([[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]], 255), // top
        ([[x0, y0, z1], [x1, y0, z1], [x1, y0, z0], [x0, y0, z0]], 140), // bottom
    ];
    for (i, (p, k)) in faces.into_iter().enumerate() {
        // The four sides are upright; the top and bottom lie flat.
        let corners = if i < 4 { UPRIGHT } else { FULL };
        quad(out, p, tiles[i], shade(tint, k), corners);
    }
}

/// A picture made solid: every opaque pixel is a little block, as a held tool
/// is in the game it comes from, so it has a real edge and thickness. `lo`
/// and `hi` are the picture's corners (the handle end first); `mirror` reads
/// the picture from its right side to its left; `depth` is how many pixels
/// thick it is (1 is as thick as a pixel is wide). Falls back to a thin slab
/// if the picture's shape is not known yet.
pub(super) fn stacked(out: &mut Vec<Quad>, tile: u16, lo: [f32; 2], hi: [f32; 2], mirror: bool, depth: f32) {
    let Some(alpha) = crate::textures::tile_alpha(tile as usize) else {
        return slab(out, tile, lo, hi, mirror);
    };
    let n = 16usize;
    let (pw, ph) = ((hi[0] - lo[0]) / n as f32, (hi[1] - lo[1]) / n as f32);
    let t = ((if pw < 0.0 { -pw } else { pw }) * depth.max(0.1)).max(0.004);
    let solid = |px: i32, py: i32| -> bool { px >= 0 && py >= 0 && px < n as i32 && py < n as i32 && alpha[py as usize * n + px as usize] > 128 };
    // Where the pixel in column `px` and row `py` is: x runs with the picture
    // (or against it, mirrored), y runs up as the rows go up the picture.
    let col_x = |px: usize| -> (f32, f32) {
        if mirror { (hi[0] - (px + 1) as f32 * pw, hi[0] - px as f32 * pw) } else { (lo[0] + px as f32 * pw, lo[0] + (px + 1) as f32 * pw) }
    };
    let row_y = |py: usize| -> (f32, f32) { (hi[1] - (py + 1) as f32 * ph, hi[1] - py as f32 * ph) };
    let (z0, z1) = (-t * 0.5, t * 0.5);
    let tint = [255u8, 255, 255];
    // The two big faces: the whole picture, cut out by its own holes.
    let front = [[lo[0], lo[1], z1], [hi[0], lo[1], z1], [hi[0], hi[1], z1], [lo[0], hi[1], z1]];
    let fuv = if mirror { [UPRIGHT[1], UPRIGHT[0], UPRIGHT[3], UPRIGHT[2]] } else { UPRIGHT };
    quad(out, front, tile, shade(tint, 255), fuv);
    let back = [[hi[0], lo[1], z0], [lo[0], lo[1], z0], [lo[0], hi[1], z0], [hi[0], hi[1], z0]];
    let buv = if mirror { [UPRIGHT[0], UPRIGHT[1], UPRIGHT[2], UPRIGHT[3]] } else { [UPRIGHT[1], UPRIGHT[0], UPRIGHT[3], UPRIGHT[2]] };
    quad(out, back, tile, shade(tint, 205), buv);
    // The edges: wherever an opaque pixel has an empty one beside it.
    for py in 0..n {
        for px in 0..n {
            if !solid(px as i32, py as i32) {
                continue;
            }
            let (x0, x1) = col_x(px);
            let (y0, y1) = row_y(py);
            // The pixel's own colour: all four corners at its middle.
            let mid = [(px as f32 + 0.5) / n as f32, (py as f32 + 0.5) / n as f32];
            let uv = [mid; 4];
            // In world terms: the pixel to the left (-x) is column px-1 unless mirrored.
            let (left, right) = if mirror { (px as i32 + 1, px as i32 - 1) } else { (px as i32 - 1, px as i32 + 1) };
            if !solid(left, py as i32) {
                quad(out, [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]], tile, shade(tint, 200), uv);
            }
            if !solid(right, py as i32) {
                quad(out, [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]], tile, shade(tint, 200), uv);
            }
            if !solid(px as i32, py as i32 - 1) {
                quad(out, [[x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1]], tile, shade(tint, 255), uv);
            }
            if !solid(px as i32, py as i32 + 1) {
                quad(out, [[x0, y0, z1], [x1, y0, z1], [x1, y0, z0], [x0, y0, z0]], tile, shade(tint, 140), uv);
            }
        }
    }
}

/// The thin version of [`stacked`]: a few copies one behind another.
pub(super) fn slab(out: &mut Vec<Quad>, tile: u16, lo: [f32; 2], hi: [f32; 2], mirror: bool) {
    for k in 0..5 {
        let z = (k as f32 - 2.0) * 0.007;
        let front = [[lo[0], lo[1], z], [hi[0], lo[1], z], [hi[0], hi[1], z], [lo[0], hi[1], z]];
        let uv = if mirror { [UPRIGHT[1], UPRIGHT[0], UPRIGHT[3], UPRIGHT[2]] } else { UPRIGHT };
        quad(out, front, tile, [255, 255, 255, 255], uv);
    }
}

pub(super) fn rot_z(p: [f32; 3], a: f32) -> [f32; 3] {
    let (c, s) = (cosf(a), sinf(a));
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]]
}

pub(super) fn rot_x(p: [f32; 3], a: f32) -> [f32; 3] {
    let (c, s) = (cosf(a), sinf(a));
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}

pub(super) fn rot_y(p: [f32; 3], a: f32) -> [f32; 3] {
    let (c, s) = (cosf(a), sinf(a));
    [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c]
}

pub(super) fn map(quads: &mut [Quad], f: impl Fn([f32; 3]) -> [f32; 3]) {
    for q in quads {
        q.p = q.p.map(&f);
    }
}
