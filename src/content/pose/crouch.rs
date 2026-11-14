//! Crouching: hunched over, knees bent, slower.

use super::{Role, Root, Rig};

/// How far the eyes drop, in blocks.
pub const EYE_DROP: f64 = 0.28;
/// What a crouching walker's speed is multiplied by.
pub const SPEED: f32 = 0.3;
/// How fast the crouch comes and goes, per second.
pub const RATE: f32 = 9.0;

pub fn offsets(role: Role) -> [f32; 3] {
    match role {
        Role::Belly => [-20.0, 0.0, 0.0],
        Role::Chest | Role::Torso => [-10.0, 0.0, 0.0],
        // The head comes back up to look ahead.
        Role::Neck => [10.0, 0.0, 0.0],
        Role::Head => [22.0, 0.0, 0.0],
        Role::Thigh(_) => [46.0, 0.0, 0.0],
        Role::Calf(_) => [-84.0, 0.0, 0.0],
        Role::Foot(_) => [38.0, 0.0, 0.0],
        Role::Arm(s) => [12.0, 0.0, -(s as f32) * 4.0],
        Role::Forearm(_) => [18.0, 0.0, 0.0],
        _ => [0.0; 3],
    }
}

/// Bending the knees brings the hips down.
pub fn root(rig: &Rig) -> Root {
    Root { turn: 0.0, shift: [0.0, -rig.hip * 0.24, rig.hip * 0.05] }
}

/// Where a crouch is, 0 (upright) .. 1 (fully down), after `dt` seconds of
/// wanting it or not.
pub fn approach(now: f32, wanted: bool, dt: f32) -> f32 {
    let target = if wanted { 1.0 } else { 0.0 };
    let step = RATE * dt.max(0.0);
    if now < target {
        (now + step).min(target)
    } else {
        (now - step).max(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_crouch_comes_and_goes_smoothly() {
        let mut c = 0.0;
        for _ in 0..3 {
            c = approach(c, true, 0.02);
        }
        assert!(c > 0.3 && c < 1.0);
        for _ in 0..20 {
            c = approach(c, true, 0.02);
        }
        assert_eq!(c, 1.0);
        c = approach(c, false, 1.0);
        assert_eq!(c, 0.0);
        assert_eq!(approach(0.5, true, -1.0), 0.5);
    }

    #[test]
    fn a_crouch_is_a_hunch() {
        assert!(offsets(Role::Belly)[0] < 0.0, "the torso leans forward");
        assert!(offsets(Role::Thigh(1))[0] > 0.0 && offsets(Role::Calf(1))[0] < 0.0, "knees bent");
        assert!(EYE_DROP > 0.15 && EYE_DROP < 0.5 && SPEED < 1.0);
    }
}
