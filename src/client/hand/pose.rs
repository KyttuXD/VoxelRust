//! How an item sits in the first-person hand, after LastRoute's
//! `HeldItemPoses` (game/player/held_item_poses.gd): one table with, for
//! each kind of thing, the model's size, the turn that shows its flat to the
//! target, the lean of its long axis, and the point of it that is in the fist.

use super::Held;

/// How a held item sits in the first-person view: `scale` the model's size,
/// `roll` leans the long axis to the left (+) in the view, `yaw` turns the
/// flat of the blade toward the target, `pitch` (negative: the head tipped
/// forward and up) leans the long axis. `grip` is the point of the model
/// (before scaling) that is in the fist.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    /// Lean of the long axis to the left (+) in the view, in radians.
    pub roll: f32,
    pub scale: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub grip: [f32; 3],
}

pub fn pose_of(held: Held) -> Pose {
    match held {
        // Big and thick, the head up and to the left.
        Held::Tool(_) => Pose { roll: 0.62, scale: 1.6, yaw: 0.35, pitch: -0.5, grip: [0.0; 3] },
        Held::Bow(_) => Pose { roll: 0.3, scale: 1.3, yaw: 0.4, pitch: -0.3, grip: [0.0; 3] },
        // A small cube, turned to show its top and two of its sides.
        Held::Cube(_) => Pose { roll: 0.0, scale: 1.4, yaw: 0.6, pitch: 0.15, grip: [0.0; 3] },
        // A card with its lower edge in the fist, leaning back a little.
        Held::Card(_) => Pose { roll: 0.25, scale: 1.5, yaw: 0.45, pitch: -0.3, grip: [0.0, -0.07, 0.0] },
        Held::Nothing => Pose { roll: 0.0, scale: 1.0, yaw: 0.0, pitch: 0.0, grip: [0.0; 3] },
    }
}

/// The pose of a bow drawn `draw` (0..1) of the way: it comes upright and
/// turns its flat to the side as the string comes back.
pub fn bow_pose(draw: f32) -> Pose {
    let d = draw.clamp(0.0, 1.0);
    let rest = pose_of(Held::Bow(0));
    Pose { roll: rest.roll * (1.0 - 0.8 * d), scale: rest.scale, yaw: rest.yaw * (1.0 - 0.6 * d), pitch: rest.pitch * (1.0 - 0.6 * d), grip: rest.grip }
}

/// The first-person hand is LastRoute's (data/render/fp_hands.json): the
/// mannequin's arm, its hand at the origin and the forearm running up from
/// it, is tilted 118 degrees about X and placed with the hand at `BARE_VIEW`.
pub(super) const TILT_DEG: f32 = 118.0;
pub(super) const BARE_VIEW: [f32; 3] = [0.27, -0.24, -0.52];

/// Where the hand is in the view. A tool is held where LastRoute's crowbar and
/// axe are (HeldItemPoses.FIRST_PERSON `view`), a little lower and nearer the
/// middle than the bare hand.
pub fn hand_view(held: Held) -> [f32; 3] {
    match held {
        Held::Tool(_) | Held::Bow(_) => [0.24, -0.28, -0.54],
        _ => BARE_VIEW,
    }
}
