//! Climbing: hand over hand, with the knees coming up one at a time.
//!
//! Blocks that can be climbed carry the tag `climb` (see docs/DATA.md).

use super::{Role, Walk};
use crate::block::{def, Block};
use crate::math::{floord, sinf};

/// Blocks a second going up and down a climbable block.
pub const SPEED: f32 = 3.0;

pub fn offsets(role: Role, w: &Walk) -> [f32; 3] {
    // Holds still when not moving, and moves along with the climb.
    let amp = w.stride.clamp(0.0, 1.0);
    let ph = w.phase;
    let alt = |s: i8| if s > 0 { ph } else { ph + core::f32::consts::PI };
    match role {
        Role::Belly => [-7.0, 0.0, 0.0],
        Role::Chest | Role::Torso => [-5.0, sinf(ph) * 4.0 * amp, 0.0],
        Role::Head => [26.0, 0.0, 0.0],
        // The hands reach up the wall, one then the other.
        Role::Arm(s) => [150.0 + 22.0 * sinf(alt(s)) * (0.3 + 0.7 * amp), 0.0, -(s as f32) * 10.0],
        Role::Forearm(s) => [(40.0 - 30.0 * sinf(alt(s))) * (0.4 + 0.6 * amp), 0.0, 0.0],
        Role::Thigh(s) => [(10.0 + 34.0 * pos(sinf(alt(s) + 3.14))) * (0.3 + 0.7 * amp), 0.0, 0.0],
        Role::Calf(s) => [-(18.0 + 55.0 * pos(sinf(alt(s) + 3.14))) * (0.3 + 0.7 * amp), 0.0, 0.0],
        Role::Foot(_) => [14.0, 0.0, 0.0],
        _ => [0.0; 3],
    }
}

fn pos(v: f32) -> f32 {
    if v > 0.0 {
        v
    } else {
        0.0
    }
}

/// Whether a block can be climbed.
pub fn climbable(b: Block) -> bool {
    def(b).x.tags.iter().any(|t| t.0 == "climb")
}

/// Whether a player standing at `pos` (the feet) is on something to climb:
/// a climbable block at the knee or at the chest. Flying players are not climbing.
pub fn climbing(get: &dyn Fn(i32, i32, i32) -> Block, pos: [f64; 3], flying: bool) -> bool {
    if flying {
        return false;
    }
    let (x, z) = (floord(pos[0]), floord(pos[2]));
    [0.4, 1.2].iter().any(|dy| climbable(get(x, floord(pos[1] + dy), z)))
}

/// The vertical speed of a climber: up when jumping, down when descending
/// and clinging otherwise.
pub fn speed(up: bool, down: bool) -> f32 {
    match (up, down) {
        (true, false) => SPEED,
        (false, true) => -SPEED,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn climbers_go_up_down_or_cling() {
        assert_eq!(speed(true, false), SPEED);
        assert_eq!(speed(false, true), -SPEED);
        assert_eq!(speed(false, false), 0.0);
        assert_eq!(speed(true, true), 0.0);
    }

    #[test]
    fn the_hands_reach_up_and_alternate() {
        let w = Walk { phase: 1.0, stride: 1.0, clock: 0.0 };
        let (l, r) = (offsets(Role::Arm(1), &w), offsets(Role::Arm(-1), &w));
        assert!(l[0] > 100.0 && r[0] > 100.0, "arms above the shoulders");
        assert!((l[0] - r[0]).abs() > 10.0);
        // Knees: only ever bent the right way.
        for i in 0..30 {
            let w = Walk { phase: i as f32 * 0.3, stride: 1.0, clock: 0.0 };
            assert!(offsets(Role::Calf(1), &w)[0] < 0.0);
            assert!(offsets(Role::Thigh(1), &w)[0] > 0.0);
        }
    }

    #[test]
    fn nothing_is_climbed_in_the_open_or_when_flying() {
        let air = |_: i32, _: i32, _: i32| crate::block::AIR;
        assert!(!climbing(&air, [0.5, 64.0, 0.5], false));
        let vine = crate::block::reg().blocks.iter().position(|b| b.x.tags.iter().any(|t| t.0 == "climb"));
        if let Some(v) = vine {
            let wall = move |_: i32, _: i32, _: i32| v as Block;
            assert!(climbing(&wall, [0.5, 64.0, 0.5], false));
            assert!(!climbing(&wall, [0.5, 64.0, 0.5], true));
        }
    }
}
