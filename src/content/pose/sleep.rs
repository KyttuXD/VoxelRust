//! Sleeping: lying on the back in the bed, head on the pillow.
//!
//! The body is turned a quarter turn about the hip, so the head lies behind
//! the way the body faced and the feet toward it; the camera sits at the
//! eyes of the lying body and looks up.

use super::{Role, Root, Rig};
use crate::math::{cosf, sinf};

/// How high the hip's axis lies above the bed, as a part of the body's
/// height (half the torso's thickness, and a little more so the bigger
/// head does not sink in).
const THICKNESS: f32 = 0.1;

/// Where the pillow end of the body is from the middle of the bed, in
/// blocks, and how far the hip is from the middle of the body.
const HIP_FROM_MIDDLE: f64 = 0.2;
const EYE_ALONG: f64 = 0.95;
/// Height of the eyes above the bed, and how far up they look (radians).
pub const EYE_ABOVE_BED: f64 = 0.3;
pub const LOOK_UP: f32 = 1.38;

pub fn offsets(role: Role, clock: f32) -> [f32; 3] {
    let breath = sinf(clock * 1.3);
    match role {
        Role::Chest | Role::Torso => [breath * 1.6, 0.0, 0.0],
        Role::Head => [-6.0, 0.0, 0.0],
        Role::Arm(s) => [0.0, 0.0, -(s as f32) * (7.0 + breath)],
        Role::Forearm(_) => [5.0, 0.0, 0.0],
        Role::Thigh(s) => [0.0, s as f32 * 3.0, 0.0],
        Role::Foot(s) => [0.0, 0.0, -(s as f32) * 8.0],
        _ => [0.0; 3],
    }
}

/// Lying on the back: a quarter turn about the hip, then down onto the bed.
pub fn root(rig: &Rig) -> Root {
    Root { turn: 90.0, shift: [0.0, -(rig.hip - rig.height * THICKNESS), 0.0] }
}

/// Where the hip of a body lying with its middle at `middle` (the top of
/// the bed) is, for a body facing `yaw` (it lies with its feet that way).
pub fn hip_at(middle: [f64; 3], yaw: f32) -> [f64; 3] {
    let (back_x, back_z) = (sinf(yaw) as f64, cosf(yaw) as f64);
    [middle[0] - back_x * HIP_FROM_MIDDLE, middle[1], middle[2] - back_z * HIP_FROM_MIDDLE]
}

/// Where the eyes of that body are.
pub fn eye_at(middle: [f64; 3], yaw: f32) -> [f64; 3] {
    let hip = hip_at(middle, yaw);
    let (back_x, back_z) = (sinf(yaw) as f64, cosf(yaw) as f64);
    [hip[0] + back_x * EYE_ALONG, hip[1] + EYE_ABOVE_BED, hip[2] + back_z * EYE_ALONG]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_body_lies_flat_on_the_bed() {
        let rig = Rig { humanoid: true, hip: 42.72, height: 113.92 };
        let r = root(&rig);
        assert_eq!(r.turn, 90.0);
        // The hip's centre ends up half a body-thickness above the bed.
        let hip_height = rig.hip + r.shift[1];
        assert!((hip_height - 113.92 * THICKNESS).abs() < 1e-3 && hip_height > 0.0);
    }

    #[test]
    fn the_eyes_are_low_and_behind_the_hip() {
        let mid = [10.5, 65.5, -3.5];
        for yaw in [0.0f32, 1.0, -2.2, 3.0] {
            let eye = eye_at(mid, yaw);
            assert!(eye[1] - mid[1] < 0.5 && eye[1] > mid[1]);
            // About a block from the middle of the bed, along the body.
            let (dx, dz) = (eye[0] - mid[0], eye[2] - mid[2]);
            let d = (dx * dx + dz * dz).sqrt();
            assert!(d > 0.6 && d < 0.9, "{d}");
            // The head end is the back of the way the body faced.
            let facing = (-(yaw.sin() as f64), -(yaw.cos() as f64));
            assert!(dx * facing.0 + dz * facing.1 < 0.0);
        }
        // Looking nearly straight up.
        assert!(LOOK_UP > 1.2 && LOOK_UP < 1.57);
    }
}
