//! The player's hand and what it holds.
//!
//! Seen from the first-person view, the hand is a forearm in a sleeve that
//! stays in view and grips what it holds: a tool or weapon big and thick at
//! the lower right with its head up and to the left, a block as a small cube,
//! a flat item (food, a torch, a stick) as a card with thickness, a bow drawn
//! back. The hand swings, bobs as you walk, raises food to the mouth and
//! pulls a bowstring. Seen from behind (on yourself in third person, and on
//! everyone else), only the item is drawn, held where the model's right hand
//! is: the arms come from the body model.
//!
//! Everything is flat-shaded quads from the block atlas, so no model file is
//! needed. The first-person hand is drawn in its own pass with a fresh depth
//! buffer (see `Renderer::draw_quads`), so it never clips into walls.
//!
//! The files: `model` (quads, boxes, pictures made solid), `item` (one
//! item's geometry), `tool` (how a picture sits as a tool), `pose` (the table
//! of how each kind is held), `swing` (the motions), `bow` (the drawn bow's
//! picture), `body` (on a body and on the ground).

mod body;
mod bow;
mod item;
mod model;
mod pose;
mod primed;
mod swing;
mod tool;

pub use body::{dropped, on_body};
pub use primed::primed_blocks;
pub use model::Quad;
pub use pose::{bow_pose, hand_view, pose_of, Pose};
pub use swing::{draw_motion, eat_motion, motion_of, swing_of, use_twist, walk_bob, Motion};

use crate::block::{self, reg, Kind};
use item::item_geometry;
use model::{map, rot_x, rot_y, rot_z};
use pose::TILT_DEG;
use alloc::vec::Vec;

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

/// What the first-person hand is doing this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct View {
    /// How far through a blow or a use: 0 at rest, then (0, 1] from its start to
    /// its end (see `swing_of`).
    pub swing: f32,
    /// How far the walk has moved the hand (see `walk_bob`).
    pub bob: [f32; 3],
    /// How far a bow is drawn, 0..1.
    pub draw: f32,
    /// How far through eating or drinking, 0..1 (0 if not).
    pub eat: f32,
    /// Whether what is eaten is a drink.
    pub drink: bool,
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

/// Where the fingers are, from the wrist: they point along the hand's -y,
/// which the tilt turns up and away.
fn fingers() -> [f32; 3] {
    rot_x([0.0, -0.05, 0.0], TILT_DEG.to_radians())
}

/// The held item for the first-person view, put in `out`.
pub fn first_person_view(held: Held, v: &View, out: &mut Vec<Quad>) {
    if held == Held::Nothing {
        return;
    }
    let start = out.len();
    item_geometry(held, bow::drawn_picture(held, v.draw), out, true);
    let pose = if matches!(held, Held::Bow(_)) { bow_pose(v.draw) } else { pose_of(held) };
    let motion = motion_of(held, v);
    let f = fingers();
    let view = hand_view(held);
    map(&mut out[start..], |p| {
        // Moved so the grip is the origin, turned and leaned about it, put in the fist.
        let g = pose.grip;
        let q = [(p[0] - g[0]) * pose.scale, (p[1] - g[1]) * pose.scale, (p[2] - g[2]) * pose.scale];
        let q = rot_x(rot_y(rot_z(q, pose.roll), pose.yaw), pose.pitch);
        let q = motion.apply([q[0] + f[0], q[1] + f[1], q[2] + f[2]]);
        [q[0] + view[0], q[1] + view[1], q[2] + view[2]]
    });
}

/// Where a point of the hand model goes, for the first-person view: the tilt
/// and the place, after the hand's motion (the swing, the bob, eating, a draw).
pub fn arm_point_view(p: [f32; 3], v: &View, held: Held) -> [f32; 3] {
    let q = motion_of(held, v).apply(rot_x(p, TILT_DEG.to_radians()));
    let view = hand_view(held);
    [q[0] + view[0], q[1] + view[1], q[2] + view[2]]
}

/// [`first_person_view`] for a plain swing and a bob straight up.
pub fn first_person(held: Held, white: u16, swing: f32, bob: f32, out: &mut Vec<Quad>) {
    let _ = white;
    first_person_view(held, &View { swing, bob: [0.0, bob, 0.0], ..View::default() }, out);
}

/// [`arm_point_view`] for a plain swing and a bob straight up.
pub fn arm_point(p: [f32; 3], swing: f32, bob: f32, held: Held) -> [f32; 3] {
    arm_point_view(p, &View { swing, bob: [0.0, bob, 0.0], ..View::default() }, held)
}

/// The tile used for plain colours (the "white" tile of the base game).
pub fn white_tile() -> u16 {
    reg().tile("white").unwrap_or(0) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    const BARE_VIEW: [f32; 3] = pose::BARE_VIEW;

    fn centre(out: &[Quad]) -> [f32; 3] {
        let n = (out.len() * 4) as f32;
        let mut c = [0.0f32; 3];
        for q in out {
            for p in q.p {
                for a in 0..3 {
                    c[a] += p[a] / n;
                }
            }
        }
        c
    }

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
        let bow = r.items.iter().position(|i| i.id == "bow").unwrap();
        assert!(matches!(held_of(Some(bow)), Held::Bow(_)));
        assert_eq!(held_of(None), Held::Nothing);
    }

    #[test]
    fn a_card_in_the_hand_has_thickness_but_on_a_body_it_is_one_face() {
        // Before the atlas exists the picture's shape is unknown and it is a few
        // slices, never fewer than two: a card with depth.
        let mut out = Vec::new();
        first_person(Held::Card(3), 0, 0.0, 0.0, &mut out);
        assert!(out.len() >= 2, "{}", out.len());
        // Seen from outside a card is still one face, so it never shows as a cross.
        let mut out = Vec::new();
        on_body(Held::Card(3), 0, 0.0, [0.0; 3], 0.0, 1.8, &mut out);
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

    #[test]
    fn a_block_or_card_uses_last_routes_twist_and_comes_back() {
        assert_eq!(use_twist(0.0), (0.0, [0.0; 3]));
        let (rz, off) = use_twist(0.36);
        assert!((rz + 0.72).abs() < 1e-3 && (off[0] + 0.10).abs() < 1e-3, "{rz} {off:?}");
        let (rz, off) = use_twist(0.999);
        assert!(rz.abs() < 0.01 && off.iter().all(|v| v.abs() < 0.01));
        // The held card is turned by it: the motion is not nothing mid-use.
        let v = View { swing: 0.36, ..View::default() };
        assert!(motion_of(Held::Card(1), &v).rz < -0.7);
        // A bow does not twist for the click that starts the draw.
        assert_eq!(motion_of(Held::Bow(1), &v).rz, 0.0);
    }

    #[test]
    fn eating_raises_the_hand_to_the_mouth_and_brings_it_down() {
        assert_eq!(eat_motion(0.0, false), Motion::default());
        let mid = eat_motion(0.5, false);
        assert!(mid.off[1] > 0.1 && mid.off[2] > 0.2 && mid.off[0] < -0.1, "{mid:?}");
        assert!(eat_motion(0.5, true).rx > mid.rx, "a drink is tipped back further");
        let end = eat_motion(0.999, false);
        assert!(end.off[1].abs() < 0.05, "{end:?}");
        // The held food goes up with the hand.
        let (mut at_rest, mut eating) = (Vec::new(), Vec::new());
        first_person_view(Held::Card(1), &View::default(), &mut at_rest);
        first_person_view(Held::Card(1), &View { eat: 0.5, ..View::default() }, &mut eating);
        assert!(centre(&eating)[1] > centre(&at_rest)[1] + 0.1);
        // And so does the arm.
        let rest = arm_point_view([0.0; 3], &View::default(), Held::Card(1));
        let up = arm_point_view([0.0; 3], &View { eat: 0.5, ..View::default() }, Held::Card(1));
        assert!(up[1] > rest[1] + 0.1 && up[2] > rest[2] + 0.2);
    }

    #[test]
    fn drawing_a_bow_pulls_the_arm_back_toward_you_and_the_item_goes_with_it() {
        let rest = arm_point_view([0.0; 3], &View::default(), Held::Bow(1));
        let full = arm_point_view([0.0; 3], &View { draw: 1.0, ..View::default() }, Held::Bow(1));
        assert!(full[2] > rest[2] + 0.08, "{rest:?} {full:?}");
        assert!(full[0] < rest[0] - 0.1, "toward the middle");
        let (mut a, mut b) = (Vec::new(), Vec::new());
        first_person_view(Held::Bow(1), &View::default(), &mut a);
        first_person_view(Held::Bow(1), &View { draw: 1.0, ..View::default() }, &mut b);
        assert!(centre(&b)[2] > centre(&a)[2] + 0.08);
        // Drawn, the bow stands more upright.
        assert!(bow_pose(1.0).roll < bow_pose(0.0).roll);
    }

    #[test]
    fn walking_bobs_the_hand_and_standing_still_barely_does() {
        let still = walk_bob(1.0, 0.0, true);
        assert!(still.iter().all(|v| v.abs() < 0.003), "{still:?}");
        let mut high = 0.0f32;
        for i in 0..100 {
            let b = walk_bob(i as f32 * 0.05, 4.4, true);
            high = high.max(b[0].abs()).max(b[1].abs());
        }
        assert!(high > 0.01, "{high}");
        assert!(walk_bob(0.3, 4.4, false).iter().all(|v| v.abs() < 0.003));
    }

    #[test]
    fn a_tool_is_held_big_with_its_head_up_and_to_the_left() {
        let p = pose_of(Held::Tool(1));
        assert!(p.scale > 1.4 && p.roll >= 0.0 && p.roll < 0.3, "a tool stands up, big, near the side of the view");
        // The head of a diagonal picture ends up above and left of the fist.
        let mut out = Vec::new();
        first_person_view(Held::Tool(1), &View::default(), &mut out);
        let grip = arm_point_view([0.0; 3], &View::default(), Held::Tool(1));
        let c = centre(&out);
        assert!(c[1] > grip[1] && c[0] < grip[0] + 0.05, "{c:?} {grip:?}");
    }
}
