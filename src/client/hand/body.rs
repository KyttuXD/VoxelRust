//! Held items seen from outside: in the hand of a body (yourself in third
//! person, other players) and lying on the ground.

use super::item::item_geometry;
use super::model::{map, rot_x, rot_y, Quad};
use super::swing::swing_angle;
use super::Held;
use crate::math::PI;
use alloc::vec::Vec;

/// The held item on a body, seen from outside: `feet` is where the body
/// stands (camera-relative), `height` its height in blocks, `yaw` which way
/// it faces. The item is held out to the body's right, a little forward.
pub fn on_body(held: Held, white: u16, swing: f32, feet: [f32; 3], yaw: f32, height: f32, out: &mut Vec<Quad>) {
    let _ = white;
    if held == Held::Nothing {
        return;
    }
    let start = out.len();
    item_geometry(held, None, out, false);
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
    let _ = white;
    let start = out.len();
    item_geometry(held, None, out, false);
    map(&mut out[start..], |p| {
        let p = [p[0] * 1.6, p[1] * 1.6, p[2] * 1.6];
        let p = rot_y(p, turn);
        [p[0] + at[0], p[1] + at[1], p[2] + at[2]]
    });
}
