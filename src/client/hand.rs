//! The player's hand and what it holds.
//!
//! Seen from the first-person view, the hand is a forearm in a sleeve, and
//! the item held is a small block or a flat card. Seen from behind (on
//! yourself in third person, and on everyone else), only the item is drawn,
//! held where the model's right hand is: the arms come from the body model.
//!
//! Everything is flat-shaded quads from the block atlas, so no model file is
//! needed. The first-person hand is drawn in its own pass with a fresh depth
//! buffer (see `Renderer::draw_quads`), so it never clips into walls.

use crate::block::{self, reg, Kind};
use crate::math::{cosf, sinf, PI};
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

/// What a hand holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Held {
    Nothing,
    /// A block: its tiles for the north, south, east, west, top and bottom.
    Cube([u16; 6]),
    /// A picture, as a flat card (items, flowers, eggs, props).
    Card(u16),
    /// A tool or weapon: a bigger card, gripped by its handle (the picture's
    /// bottom-left corner), so the blade stands out of the fist.
    Tool(u16),
    /// A bow: a bigger card held by its middle, where the hand grips it.
    Bow(u16),
}

/// What an item in the hand looks like. `item` is an index into the items.
pub fn held_of(item: Option<usize>) -> Held {
    let r = reg();
    let it = match item.and_then(|i| r.items.get(i)) {
        Some(it) => it,
        None => return Held::Nothing,
    };
    if let Some(b) = it.place {
        let d = block::def(b);
        let solid = matches!(d.kind, Kind::Cube | Kind::Cutout | Kind::Translucent | Kind::Boxes);
        if solid && it.brush == 0 && d.x.prop.is_none() && it.x.egg.is_none() && it.x.turns.is_empty() {
            let [top, side, bottom] = d.tex;
            return Held::Cube([side, side, side, side, top, bottom]);
        }
    }
    if it.place.is_none() && it.x.shoots.is_some() {
        return Held::Bow(it.icon);
    }
    if it.place.is_none() && (it.tool != 0 || it.x.damage > 0) {
        return Held::Tool(it.icon);
    }
    Held::Card(it.icon)
}

/// A tile's place in the atlas, `(u, v)` in 0..1 across the tile.
fn atlas(tile: u16, u: f32, v: f32) -> [f32; 2] {
    let n = PER_ROW as f32;
    [((tile as usize % PER_ROW) as f32 + u) / n, ((tile as usize / PER_ROW) as f32 + v) / n]
}

/// A tint times a face's shade (the same light levels the models use).
fn shade(tint: [u8; 3], k: u32) -> [u8; 4] {
    [(tint[0] as u32 * k / 255) as u8, (tint[1] as u32 * k / 255) as u8, (tint[2] as u32 * k / 255) as u8, 255]
}

/// A whole tile, corners in order round the face.
const FULL: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
/// A whole tile on an upright face: the corners at the bottom of the face
/// take the bottom of the tile, as the world mesh does (its v runs down the
/// tile). Without this, textures come out upside down.
const UPRIGHT: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];

fn quad(out: &mut Vec<Quad>, p: [[f32; 3]; 4], tile: u16, col: [u8; 4], corners: [[f32; 2]; 4]) {
    let uv = corners.map(|c| atlas(tile, c[0], c[1]));
    out.push(Quad { p, uv, col });
}

/// A box from `lo` to `hi`. `tiles` gives each face its tile (north, south,
/// east, west, top, bottom); each face is shaded as the models shade theirs.
fn boxq(out: &mut Vec<Quad>, lo: [f32; 3], hi: [f32; 3], tiles: [u16; 6], tint: [u8; 3]) {
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

/// The held item, centred on the origin, facing forward (-z).
fn item_geometry(held: Held, white: u16, out: &mut Vec<Quad>, first: bool) {
    let start = out.len();
    match held {
        Held::Nothing => {}
        Held::Cube(tiles) => {
            // A block the size of a die, a little forward of the grip.
            boxq(out, [-0.05, -0.05, -0.05], [0.05, 0.05, 0.05], tiles, [255, 255, 255]);
        }
        Held::Card(tile) => {
            // One flat card, facing forward. A second face would show the
            // picture mirrored behind it, which reads as a cross.
            let (lo, hi, z) = ([-0.08, -0.08], [0.08, 0.08], 0.0);
            let front = [[lo[0], lo[1], z], [hi[0], lo[1], z], [hi[0], hi[1], z], [lo[0], hi[1], z]];
            quad(out, front, tile, [255, 255, 255, 255], UPRIGHT);
        }
        Held::Tool(tile) => {
            // The handle (bottom-left of the picture) is in the fist and the
            // blade stands out of it.
            if first {
                // The model, as in LastRoute's pose table: the handle's end (the grip) at
                // the origin and the long axis up (+Y), the flat of it facing +Z.
                // A picture's long axis may be the diagonal (the game's own tools) or
                // straight up (a texture pack's): find it, and the end that is held.
                let (angle, grip) = tool_frame(tile);
                stacked(out, tile, [0.0, 0.0], [TOOL_SIZE, TOOL_SIZE], false);
                map(&mut out[start..], |p| {
                    let q = [p[0] - grip[0] * TOOL_SIZE, p[1] - grip[1] * TOOL_SIZE, p[2]];
                    rot_z(q, angle)
                });
            } else {
                stacked(out, tile, [-0.025, -0.025], [0.19, 0.19], false);
                map(&mut out[start..], |p| rot_y(p, -0.5));
            }
        }
        Held::Bow(tile) => {
            if first {
                let (angle, _) = tool_frame(tile);
                stacked(out, tile, [-TOOL_SIZE * 0.5, -TOOL_SIZE * 0.5], [TOOL_SIZE * 0.5, TOOL_SIZE * 0.5], false);
                map(&mut out[start..], |p| rot_z(p, angle));
            } else {
                stacked(out, tile, [-0.1, -0.1], [0.1, 0.1], false);
                map(&mut out[start..], |p| rot_y(p, -0.5));
            }
        }
    }
    let _ = white;
}

/// A picture made solid: every opaque pixel is a little block, as a held tool
/// is in the game it comes from, so it has a real edge and thickness. `lo`
/// and `hi` are the picture's corners (the handle end first); `mirror` reads
/// the picture from its right side to its left. Falls back to a thin slab
/// if the picture's shape is not known yet.
fn stacked(out: &mut Vec<Quad>, tile: u16, lo: [f32; 2], hi: [f32; 2], mirror: bool) {
    let Some(alpha) = crate::textures::tile_alpha(tile as usize) else {
        return slab(out, tile, lo, hi, mirror);
    };
    let n = 16usize;
    let (pw, ph) = ((hi[0] - lo[0]) / n as f32, (hi[1] - lo[1]) / n as f32);
    // As thick as a pixel is wide.
    let t = (if pw < 0.0 { -pw } else { pw }).max(0.004);
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
    for q in out.iter_mut().rev().take(0) {
        let _ = q;
    }
}

/// The thin version of [`stacked`]: a few copies one behind another.
fn slab(out: &mut Vec<Quad>, tile: u16, lo: [f32; 2], hi: [f32; 2], mirror: bool) {
    for k in 0..5 {
        let z = (k as f32 - 2.0) * 0.007;
        let front = [[lo[0], lo[1], z], [hi[0], lo[1], z], [hi[0], hi[1], z], [lo[0], hi[1], z]];
        let uv = if mirror { [UPRIGHT[1], UPRIGHT[0], UPRIGHT[3], UPRIGHT[2]] } else { UPRIGHT };
        quad(out, front, tile, [255, 255, 255, 255], uv);
    }
}

/// Side of a tool's picture in the first-person model (blocks).
const TOOL_SIZE: f32 = 0.30;

/// How a picture sits as a held tool: the turn (radians, about z) that puts its
/// long axis upright, and the held end (the handle's end) as a fraction of the
/// picture from its bottom left. Found from which pixels are solid: the axis is
/// the direction they spread most, and the handle is at the end nearer the
/// picture's bottom.
fn tool_frame(tile: u16) -> (f32, [f32; 2]) {
    let diagonal = (core::f32::consts::FRAC_PI_4, [0.0, 0.0]);
    let Some(alpha) = crate::textures::tile_alpha(tile as usize) else { return diagonal };
    let (mut n, mut sx, mut sy) = (0.0f32, 0.0f32, 0.0f32);
    for y in 0..16 {
        for x in 0..16 {
            if alpha[y * 16 + x] > 128 {
                n += 1.0;
                sx += x as f32 + 0.5;
                sy += 15.5 - y as f32;
            }
        }
    }
    if n < 3.0 {
        return diagonal;
    }
    let (mx, my) = (sx / n, sy / n);
    let (mut cxx, mut cyy, mut cxy) = (0.0f32, 0.0f32, 0.0f32);
    for y in 0..16 {
        for x in 0..16 {
            if alpha[y * 16 + x] > 128 {
                let (dx, dy) = (x as f32 + 0.5 - mx, 15.5 - y as f32 - my);
                cxx += dx * dx;
                cyy += dy * dy;
                cxy += dx * dy;
            }
        }
    }
    // The principal axis: its angle from the x axis, folded so that it points up.
    let mut theta = 0.5 * crate::math::atan2f(2.0 * cxy, cxx - cyy);
    if theta < 0.0 {
        theta += core::f32::consts::PI;
    }
    // Rotating by (pi/2 - theta) turns the axis to point up (+y).
    let angle = core::f32::consts::FRAC_PI_2 - theta;
    let axis = [crate::math::cosf(theta), crate::math::sinf(theta)];
    // The solid pixel furthest back along the axis, the end with the lower
    // picture position, is where it is held.
    let (mut best, mut at) = (f32::MAX, [0.0f32, 0.0]);
    for y in 0..16 {
        for x in 0..16 {
            if alpha[y * 16 + x] > 128 {
                let (px, py) = (x as f32 + 0.5, 15.5 - y as f32);
                let along = px * axis[0] + py * axis[1];
                if along < best {
                    best = along;
                    at = [px / 16.0, py / 16.0];
                }
            }
        }
    }
    (angle, at)
}

fn rot_z(p: [f32; 3], a: f32) -> [f32; 3] {
    let (c, s) = (cosf(a), sinf(a));
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]]
}

/// How a held tool sits in the first-person view, as LastRoute's `HeldItemPoses`
/// has it: `scale` the model's size, `yaw` turns the flat of the blade toward
/// the target, `pitch` (negative: the head tipped forward and up) leans the long axis.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    /// Lean of the long axis to the left (+) in the view, in radians.
    pub roll: f32,
    pub scale: f32,
    pub yaw: f32,
    pub pitch: f32,
}

pub fn pose_of(held: Held) -> Pose {
    match held {
        Held::Bow(_) => Pose { roll: 0.3, scale: 1.3, yaw: 0.4, pitch: -0.3 },
        _ => Pose { roll: 0.62, scale: 1.6, yaw: 0.35, pitch: -0.5 },
    }
}

fn rot_x(p: [f32; 3], a: f32) -> [f32; 3] {
    let (c, s) = (cosf(a), sinf(a));
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}

fn rot_y(p: [f32; 3], a: f32) -> [f32; 3] {
    let (c, s) = (cosf(a), sinf(a));
    [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c]
}

fn map(quads: &mut [Quad], f: impl Fn([f32; 3]) -> [f32; 3]) {
    for q in quads {
        q.p = q.p.map(&f);
    }
}

/// How far the hand has swung: 0 at rest, 1 at the top of a blow or a use.
/// It comes back to rest along a curve.
fn swing_angle(swing: f32) -> f32 {
    -sinf(PI * swing.clamp(0.0, 1.0)) * 0.9
}

/// The first-person hand is LastRoute's (data/render/fp_hands.json): the
/// mannequin's arm, its hand at the origin and the forearm running up from
/// it, is tilted 118 degrees about X and placed with the hand at `BARE_VIEW`.
const TILT_DEG: f32 = 118.0;
const BARE_VIEW: [f32; 3] = [0.27, -0.24, -0.52];

/// Where a point of the hand model goes, for the first-person view: a swing
/// (a blow or a use) turns it about the wrist first, then the tilt and the
/// place. `bob` lifts it as the player walks.
pub fn arm_point(p: [f32; 3], swing: f32, bob: f32, held: Held) -> [f32; 3] {
    let (angle, off) = swing_of(held, swing);
    let p = rot_x(p, angle);
    let p = rot_x(p, TILT_DEG.to_radians());
    let v = hand_view(held);
    [p[0] + v[0] + off[0], p[1] + v[1] + bob + off[1], p[2] + v[2] + off[2]]
}

/// Where the hand is in the view. A tool is held where LastRoute's crowbar and
/// axe are (HeldItemPoses.FIRST_PERSON `view`), a little lower and nearer the
/// middle than the bare hand.
pub fn hand_view(held: Held) -> [f32; 3] {
    match held {
        Held::Tool(_) | Held::Bow(_) => [0.24, -0.28, -0.54],
        _ => BARE_VIEW,
    }
}

/// The swing of a tool, as LastRoute's `_animate_tool_use` tweens it on the
/// view node: a windup that raises the hand and tips the head back toward you
/// (to `arc` x 0.70, easing out), a strike that drops it forward past rest (to
/// -`arc` x 1.15, easing in), and a recovery (easing out). `t` is 0 at rest, then
/// runs 0..1 through the whole use. Gives the turn about the wrist and how far
/// the hand moves. Anything else swings as before.
pub fn swing_of(held: Held, t: f32) -> (f32, [f32; 3]) {
    if !matches!(held, Held::Tool(_)) {
        return (swing_angle(t), [0.0; 3]);
    }
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return (0.0, [0.0; 3]);
    }
    let arc = 70.0f32.to_radians();
    let (up, down) = ([0.02f32, 0.05, 0.06], [-0.03f32, -0.05, -0.12]);
    let lerp = |a: [f32; 3], b: [f32; 3], k: f32| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k];
    if t < 0.30 {
        let u = t / 0.30;
        let e = 1.0 - (1.0 - u) * (1.0 - u);
        (arc * 0.70 * e, lerp([0.0; 3], up, e))
    } else if t < 0.52 {
        let u = (t - 0.30) / 0.22;
        let e = u * u;
        (arc * (0.70 + (-1.15 - 0.70) * e), lerp(up, down, e))
    } else {
        let u = (t - 0.52) / 0.48;
        let e = 1.0 - (1.0 - u) * (1.0 - u);
        (-arc * 1.15 * (1.0 - e), lerp(down, [0.0; 3], e))
    }
}

/// Where the held item goes: in the fist, at the end of the fingers. The
/// fingers point along the hand's -y, which the tilt turns up and away.
pub fn item_point(p: [f32; 3], swing: f32, bob: f32) -> [f32; 3] {
    let fingers = rot_x([0.0, -0.05, 0.0], TILT_DEG.to_radians());
    let q = [p[0] + fingers[0], p[1] + fingers[1], p[2] + fingers[2]];
    let q = rot_x(q, swing_angle(swing));
    [q[0] + BARE_VIEW[0], q[1] + BARE_VIEW[1] + bob, q[2] + BARE_VIEW[2]]
}

/// The held item for the first-person view, put in `out`.
pub fn first_person(held: Held, white: u16, swing: f32, bob: f32, out: &mut Vec<Quad>) {
    let start = out.len();
    item_geometry(held, white, out, true);
    if matches!(held, Held::Tool(_) | Held::Bow(_)) {
        let pose = pose_of(held);
        let fingers = rot_x([0.0, -0.05, 0.0], TILT_DEG.to_radians());
        let (angle, off) = swing_of(held, swing);
        let view = hand_view(held);
        map(&mut out[start..], |p| {
            // Turned and leaned about the grip, then put in the fist.
            let q = rot_x(rot_y(rot_z([p[0] * pose.scale, p[1] * pose.scale, p[2] * pose.scale], pose.roll), pose.yaw), pose.pitch);
            let q = [q[0] + fingers[0], q[1] + fingers[1], q[2] + fingers[2]];
            let q = rot_x(q, angle);
            [q[0] + view[0] + off[0], q[1] + view[1] + bob + off[1], q[2] + view[2] + off[2]]
        });
        return;
    }
    map(&mut out[start..], |p| item_point(p, swing, bob));
}

/// The held item on a body, seen from outside: `feet` is where the body
/// stands (camera-relative), `height` its height in blocks, `yaw` which way
/// it faces. The item is held out to the body's right, a little forward.
pub fn on_body(held: Held, white: u16, swing: f32, feet: [f32; 3], yaw: f32, height: f32, out: &mut Vec<Quad>) {
    if held == Held::Nothing {
        return;
    }
    let start = out.len();
    item_geometry(held, white, out, false);
    // Seen from farther off, the item is drawn larger than in the hand: a
    // stick's picture is mostly empty space.
    // A tool or weapon is gripped by its handle and seen at about the length it
    // has in the hand: big, at the fist, the blade up and out to the side.
    let scale = match held {
        Held::Tool(_) => 3.1,
        Held::Bow(_) => 2.8,
        _ => 1.8,
    };
    if matches!(held, Held::Tool(_) | Held::Bow(_)) {
        // Undo the first-person turn, and point the blade ahead and a little out, plane on edge.
        map(&mut out[start..], |p| rot_y(p, 0.5 + PI * 0.5 - 0.7));
    }
    map(&mut out[start..], |p| [p[0] * scale, p[1] * scale, p[2] * scale]);
    let s = swing_angle(swing);
    // Right hand: about the height of the chest, to the right, forward a bit.
    // At the right hand, at the side of the body and about hip height, where
    // LastRoute's remote items sit (its R-Hand bone), so the torso does not hide it.
    // The model's right hand is at x 0.22..0.45 blocks, at about 0.73 blocks up
    // (its arm's lower end). The item is held just outside it, so it shows from
    // the front and the back alike.
    let hand = match held {
        // At the right fist (the lower end of the arm), a little ahead of the hip.
        Held::Tool(_) | Held::Bow(_) => [0.48_f32, height * 0.40, -0.10],
        _ => [0.50_f32, height * 0.42, 0.0],
    };
    map(&mut out[start..], |p| {
        let p = rot_x(p, s * 0.6);
        let p = rot_y([p[0] + hand[0], p[1] + hand[1], p[2] + hand[2]], yaw);
        [p[0] + feet[0], p[1] + feet[1], p[2] + feet[2]]
    });
}

/// An item lying on the ground: the held item, small, turning about its
/// upright axis. `at` is where it lies (camera-relative), `turn` the angle.
pub fn dropped(held: Held, white: u16, at: [f32; 3], turn: f32, out: &mut Vec<Quad>) {
    let start = out.len();
    item_geometry(held, white, out, false);
    map(&mut out[start..], |p| {
        let p = [p[0] * 1.6, p[1] * 1.6, p[2] * 1.6];
        let p = rot_y(p, turn);
        [p[0] + at[0], p[1] + at[1], p[2] + at[2]]
    });
}

/// The tile used for plain colours (the "white" tile of the base game).
pub fn white_tile() -> u16 {
    reg().tiles.iter().position(|t| t.id == "white").unwrap_or(0) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hand_is_placed_as_last_route_places_it() {
        // The hand (the model origin) is at LastRoute's bare view, and the
        // forearm (up the model) comes down from it and toward the viewer.
        let wrist = arm_point([0.0, 0.0, 0.0], 0.0, 0.0, Held::Nothing);
        assert!(wrist.iter().zip(BARE_VIEW).all(|(a, b)| (a - b).abs() < 1e-5), "{wrist:?}");
        let elbow = arm_point([0.0, 0.5, 0.0], 0.0, 0.0, Held::Nothing);
        assert!(elbow[1] < wrist[1] - 0.2 && elbow[2] > wrist[2] + 0.2, "{wrist:?} {elbow:?}");
    }

    #[test]
    fn held_items_look_like_blocks_or_cards() {
        let r = reg();
        let stone = r.items.iter().position(|i| i.id == "stone").unwrap();
        assert!(matches!(held_of(Some(stone)), Held::Cube(_)));
        let stick = r.items.iter().position(|i| i.id == "stick").unwrap();
        assert!(matches!(held_of(Some(stick)), Held::Card(_)));
        assert_eq!(held_of(None), Held::Nothing);
    }

    #[test]
    fn a_card_is_one_face_so_it_never_shows_as_a_cross() {
        let mut out = Vec::new();
        first_person(Held::Card(3), 0, 0.0, 0.0, &mut out);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn the_body_holds_its_item_out_to_the_right() {
        let mut out = Vec::new();
        on_body(Held::Card(1), 0, 0.0, [0.0, 0.0, 0.0], 0.0, 1.8, &mut out);
        let c = out[0].p.iter().fold([0.0f32; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]);
        assert!(c[0] / 4.0 > 0.4 && c[1] / 4.0 > 0.6, "held at the right hand: {:?}", c);
    }

    #[test]
    fn a_tool_swing_winds_up_strikes_past_rest_and_comes_back() {
        let tool = Held::Tool(1);
        let arc = 70.0f32.to_radians();
        assert_eq!(swing_of(tool, 0.0), (0.0, [0.0; 3]));
        let (a, _) = swing_of(tool, 0.299);
        assert!((a - arc * 0.7).abs() < 0.03, "{a}");
        let (a, off) = swing_of(tool, 0.519);
        assert!((a + arc * 1.15).abs() < 0.05 && off[2] < -0.1, "{a} {off:?}");
        let (a, off) = swing_of(tool, 1.0);
        assert!(a.abs() < 1e-4 && off.iter().all(|v| v.abs() < 1e-4));
        // A block in the hand swings as it always did.
        assert_eq!(swing_of(Held::Cube([0; 6]), 0.5).1, [0.0; 3]);
    }
}
