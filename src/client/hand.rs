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

const FULL: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

fn quad(out: &mut Vec<Quad>, p: [[f32; 3]; 4], tile: u16, col: [u8; 4]) {
    let uv = FULL.map(|c| atlas(tile, c[0], c[1]));
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
        quad(out, p, tiles[i], shade(tint, k));
    }
}

/// The held item, centred on the origin, facing forward (-z).
fn item_geometry(held: Held, white: u16, out: &mut Vec<Quad>) {
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
            quad(out, front, tile, [255, 255, 255, 255]);
        }
    }
    let _ = white;
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
pub fn arm_point(p: [f32; 3], swing: f32, bob: f32) -> [f32; 3] {
    let p = rot_x(p, swing_angle(swing));
    let p = rot_x(p, TILT_DEG.to_radians());
    [p[0] + BARE_VIEW[0], p[1] + BARE_VIEW[1] + bob, p[2] + BARE_VIEW[2]]
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
    item_geometry(held, white, out);
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
    item_geometry(held, white, out);
    // Seen from farther off, the item is drawn larger than in the hand: a
    // stick's picture is mostly empty space.
    map(&mut out[start..], |p| [p[0] * 1.8, p[1] * 1.8, p[2] * 1.8]);
    let s = swing_angle(swing);
    // Right hand: about the height of the chest, to the right, forward a bit.
    // At the right hand, at the side of the body and about hip height, where
    // LastRoute's remote items sit (its R-Hand bone), so the torso does not hide it.
    // The model's right hand is at x 0.22..0.45 blocks, at about 0.73 blocks up
    // (its arm's lower end). The item is held just outside it, so it shows from
    // the front and the back alike.
    let hand = [0.50_f32, height * 0.42, 0.0];
    map(&mut out[start..], |p| {
        let p = rot_x(p, s * 0.6);
        let p = rot_y([p[0] + hand[0], p[1] + hand[1], p[2] + hand[2]], yaw);
        [p[0] + feet[0], p[1] + feet[1], p[2] + feet[2]]
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
        let wrist = arm_point([0.0, 0.0, 0.0], 0.0, 0.0);
        assert!(wrist.iter().zip(BARE_VIEW).all(|(a, b)| (a - b).abs() < 1e-5), "{wrist:?}");
        let elbow = arm_point([0.0, 0.5, 0.0], 0.0, 0.0);
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
}
