//! Falling: arms thrown up and out, legs flailing.

use super::Role;
use crate::math::sinf;

pub fn offsets(role: Role, clock: f32) -> [f32; 3] {
    let flail = sinf(clock * 14.0);
    match role {
        Role::Chest | Role::Torso => [4.0, 0.0, 0.0],
        Role::Head => [10.0, 0.0, 0.0],
        Role::Arm(s) => [20.0 + 12.0 * flail * s as f32, 0.0, -(s as f32) * (75.0 + 16.0 * sinf(clock * 11.0 + s as f32))],
        Role::Forearm(s) => [18.0 + 14.0 * flail * -(s as f32), 0.0, 0.0],
        Role::Thigh(s) => [14.0 * sinf(clock * 12.0) * s as f32, s as f32 * 5.0, 0.0],
        Role::Calf(s) => [-(22.0 + 16.0 * sinf(clock * 12.0 + 1.0) * s as f32), 0.0, 0.0],
        _ => [0.0; 3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arms_go_up_and_out_on_both_sides() {
        for s in [1i8, -1] {
            let o = offsets(Role::Arm(s), 0.37);
            // Out means positive z for the right arm, negative for the left.
            assert!(o[2] * (-(s as f32)) > 40.0, "{o:?}");
        }
    }
}
