//! Swimming: the body lies face down along the way it moves; the arms
//! take turns to reach over the head and pull back, the legs kick.

use super::{Role, Root, Rig, Walk};
use crate::math::sinf;

pub fn offsets(role: Role, w: &Walk) -> [f32; 3] {
    // Strokes even when drifting, harder when going fast.
    let amp = 0.35 + 0.65 * w.stride.clamp(0.0, 1.0);
    let ph = w.phase;
    match role {
        // Lifted so the face looks ahead rather than down.
        Role::Neck => [20.0, 0.0, 0.0],
        Role::Head => [48.0, 0.0, 0.0],
        Role::Chest | Role::Torso => [sinf(ph) * 3.0 * amp, 0.0, 0.0],
        Role::Arm(s) => {
            // From the hip (down in the swimmer's own frame) past the chest to over the head.
            let stroke = sinf(ph + if s > 0 { 0.0 } else { core::f32::consts::PI });
            [105.0 + 70.0 * amp * stroke, 0.0, -(s as f32) * 8.0]
        }
        Role::Forearm(s) => {
            let stroke = sinf(ph + if s > 0 { 0.0 } else { core::f32::consts::PI } + 0.8);
            [(22.0 + 24.0 * stroke) * amp, 0.0, 0.0]
        }
        Role::Thigh(s) => {
            let kick = sinf(ph * 2.0 + if s > 0 { 0.0 } else { core::f32::consts::PI });
            [20.0 * amp * kick, 0.0, 0.0]
        }
        Role::Calf(s) => {
            let kick = sinf(ph * 2.0 + if s > 0 { 0.0 } else { core::f32::consts::PI } + 1.2);
            [-(14.0 + 14.0 * kick) * amp, 0.0, 0.0]
        }
        Role::Foot(_) => [-18.0, 0.0, 0.0],
        _ => [0.0; 3],
    }
}

/// Face down, the hip a little below where it stood.
pub fn root(rig: &Rig) -> Root {
    Root { turn: -90.0, shift: [0.0, -rig.hip * 0.3, 0.0] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arms_stroke_in_turn() {
        let w = Walk { phase: 0.9, stride: 1.0, clock: 0.0 };
        let (l, r) = (offsets(Role::Arm(1), &w), offsets(Role::Arm(-1), &w));
        assert!((l[0] - r[0]).abs() > 20.0, "{l:?} {r:?}");
        // Both stay within a sensible reach, from the hip to straight up.
        for i in 0..40 {
            let w = Walk { phase: i as f32 * 0.2, stride: 1.0, clock: 0.0 };
            let a = offsets(Role::Arm(1), &w)[0];
            assert!(a > 20.0 && a < 190.0, "{a}");
        }
    }

    #[test]
    fn the_body_turns_face_down() {
        assert_eq!(root(&Rig { humanoid: true, hip: 40.0, height: 100.0 }).turn, -90.0);
    }
}
