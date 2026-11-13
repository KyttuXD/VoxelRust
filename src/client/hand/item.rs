//! The geometry of one held item, before it is posed: centred on the origin
//! (a tool by its grip), facing forward (-z).

use super::model::{boxq, map, quad, rot_y, rot_z, stacked, Quad, UPRIGHT};
use super::tool::{tool_frame, TOOL_SIZE};
use super::Held;
use alloc::vec::Vec;

/// How many pixels thick a picture made solid is, in the hand: more than the
/// one a pixel is wide, so it reads as a thing with weight.
pub(super) const TOOL_DEPTH: f32 = 1.8;
pub(super) const CARD_DEPTH: f32 = 1.6;

/// The held item, put in `out`. `first` is for the first-person view (where
/// pictures are made solid), otherwise for a body or the ground. `shown` is
/// the picture to draw a bow with if it is not the bow's own (drawn back).
pub(super) fn item_geometry(held: Held, shown: Option<u16>, out: &mut Vec<Quad>, first: bool) {
    let start = out.len();
    match held {
        Held::Nothing => {}
        Held::Cube(tiles) => {
            // A block the size of a die, a little forward of the grip.
            boxq(out, [-0.05, -0.05, -0.05], [0.05, 0.05, 0.05], tiles, [255, 255, 255]);
        }
        Held::Card(tile) => {
            if first {
                // A card with thickness, so it has an edge to catch the light.
                stacked(out, tile, [-0.09, -0.09], [0.09, 0.09], false, CARD_DEPTH);
            } else {
                // One flat card, facing forward. A second face would show the
                // picture mirrored behind it, which reads as a cross.
                let (lo, hi, z) = ([-0.08, -0.08], [0.08, 0.08], 0.0);
                let front = [[lo[0], lo[1], z], [hi[0], lo[1], z], [hi[0], hi[1], z], [lo[0], hi[1], z]];
                quad(out, front, tile, [255, 255, 255, 255], UPRIGHT);
            }
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
                stacked(out, tile, [0.0, 0.0], [TOOL_SIZE, TOOL_SIZE], false, TOOL_DEPTH);
                map(&mut out[start..], |p| {
                    let q = [p[0] - grip[0] * TOOL_SIZE, p[1] - grip[1] * TOOL_SIZE, p[2]];
                    rot_z(q, angle)
                });
            } else {
                stacked(out, tile, [-0.025, -0.025], [0.19, 0.19], false, 1.0);
                map(&mut out[start..], |p| rot_y(p, -0.5));
            }
        }
        Held::Bow(tile) => {
            // The turn that stands the bow up is found from its picture at rest, so
            // the frames of it being drawn (which add an arrow) do not turn it about.
            let picture = shown.unwrap_or(tile);
            if first {
                let (angle, _) = tool_frame(tile);
                stacked(out, picture, [-TOOL_SIZE * 0.5, -TOOL_SIZE * 0.5], [TOOL_SIZE * 0.5, TOOL_SIZE * 0.5], false, TOOL_DEPTH);
                map(&mut out[start..], |p| rot_z(p, angle));
            } else {
                stacked(out, picture, [-0.1, -0.1], [0.1, 0.1], false, 1.0);
                map(&mut out[start..], |p| rot_y(p, -0.5));
            }
        }
    }
}
