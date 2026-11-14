//! Sitting: thighs forward, calves down, the torso upright, hands on the knees.

use super::{Role, Root, Rig};

/// How high the hip joint sits above the seat once seated, as a part of
/// its height when standing (the thigh rests on the seat).
const HIP_ABOVE_SEAT: f32 = 0.164;

/// The player's hip joint, as a part of the standing height (the model's).
pub const HIP_FRACTION: f64 = 0.375;

pub fn offsets(role: Role) -> [f32; 3] {
    match role {
        Role::Belly | Role::Chest | Role::Torso => [-2.0, 0.0, 0.0],
        Role::Head => [3.0, 0.0, 0.0],
        // The thighs run forward and a little apart.
        Role::Thigh(s) => [90.0, s as f32 * 5.0, 0.0],
        // The calves hang from the knees, tucked back a touch.
        Role::Calf(_) => [-96.0, 0.0, 0.0],
        Role::Foot(_) => [6.0, 0.0, 0.0],
        // The hands rest on the knees.
        Role::Arm(s) => [16.0, 0.0, -(s as f32) * 5.0],
        Role::Forearm(_) => [30.0, 0.0, 0.0],
        Role::Hand(_) => [-12.0, 0.0, 0.0],
        _ => [0.0; 3],
    }
}

/// The whole body drops until the hip sits on the seat.
pub fn root(rig: &Rig) -> Root {
    Root { turn: 0.0, shift: [0.0, -drop(rig.hip), 0.0] }
}

/// How far a body with its hip joint `hip` above its feet is lowered to
/// sit, in the same units.
pub fn drop(hip: f32) -> f32 {
    hip * (1.0 - HIP_ABOVE_SEAT)
}

/// How far a seated player's body is lowered, in blocks (the drop of
/// [`drop`] for the player's own proportions).
pub fn drop_blocks() -> f64 {
    crate::player::HEIGHT * HIP_FRACTION * (1.0 - HIP_ABOVE_SEAT as f64)
}

/// How high above a seat the eyes of a seated player are, in blocks: the
/// standing eye height less the drop.
pub fn eye_above_seat() -> f64 {
    crate::player::EYE - drop_blocks()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seated_eye_is_lower_than_a_standing_one_but_above_the_seat() {
        let e = eye_above_seat();
        assert!(e < crate::player::EYE - 0.4 && e > 0.8, "{e}");
        assert!((e - 1.056).abs() < 0.01, "{e}");
    }

    #[test]
    fn the_drop_puts_the_hip_on_the_seat() {
        let rig = Rig { humanoid: true, hip: 42.72, height: 113.92 };
        let dropped = rig.hip + root(&rig).shift[1];
        assert!((dropped - 42.72 * HIP_ABOVE_SEAT).abs() < 1e-3);
        // As blocks (1.8 tall): the hip is a bit over a tenth of a block above the seat.
        assert!(dropped / 113.92 * 1.8 < 0.15);
    }

    #[test]
    fn thighs_go_forward_and_calves_hang_down() {
        let thigh = offsets(Role::Thigh(1));
        let calf = offsets(Role::Calf(1));
        assert_eq!(thigh[0], 90.0);
        // Together the leg below the knee points (nearly) straight down.
        assert!((thigh[0] + calf[0]).abs() < 10.0);
        // The two knees open to opposite sides.
        assert_eq!(offsets(Role::Thigh(-1))[1], -offsets(Role::Thigh(1))[1]);
    }
}
