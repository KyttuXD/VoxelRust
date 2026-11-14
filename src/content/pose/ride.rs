//! Riding: astride a mount or in a vehicle's seat, hands forward.

use super::{Role, Root, Rig};

pub fn offsets(role: Role) -> [f32; 3] {
    match role {
        Role::Belly | Role::Chest | Role::Torso => [-4.0, 0.0, 0.0],
        Role::Thigh(s) => [62.0, s as f32 * 10.0, 0.0],
        Role::Calf(_) => [-68.0, 0.0, 0.0],
        Role::Foot(_) => [8.0, 0.0, 0.0],
        Role::Arm(s) => [52.0, 0.0, -(s as f32) * 4.0],
        Role::Forearm(_) => [28.0, 0.0, 0.0],
        _ => [0.0; 3],
    }
}

/// The body stays where the rider's position puts it (the eyes are placed by
/// whatever carries the rider); only the legs fold.
pub fn root(_rig: &Rig) -> Root {
    Root::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riders_straddle_the_mount() {
        assert!(offsets(Role::Thigh(1))[1] > 0.0 && offsets(Role::Thigh(-1))[1] < 0.0);
        assert_eq!(root(&Rig { humanoid: true, hip: 40.0, height: 100.0 }), Root::default());
    }
}
