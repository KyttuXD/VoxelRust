//! Running: what a body adds to its walk when it sprints. The swing is
//! longer, the elbows fold to about a right angle, the knees come higher
//! and the body leans into the run.
//!
//! How much of it there is (`Body::run`, 0..1) comes from the speed: see
//! [`run_amount`]. Walking itself is in `bends.rs`.

use super::bends::{arm_forward, leg_forward};
use super::{Body, Kind, Role, Walk};
use crate::math::sinf;

/// Blocks a second at which a walker starts to run, and at which the run is full
/// (walking is 4.3 blocks a second, sprinting 5.6).
pub const RUN_FROM: f32 = 4.7;
pub const RUN_FULL: f32 = 5.3;

/// How much of a run (0..1) a speed over the ground is.
pub fn run_amount(speed: f32) -> f32 {
    if !(speed > RUN_FROM) {
        return 0.0;
    }
    let t = ((speed - RUN_FROM) / (RUN_FULL - RUN_FROM)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn pos(v: f32) -> f32 {
    if v > 0.0 {
        v
    } else {
        0.0
    }
}

/// The forward lean of a full run, in degrees (spread over belly and chest).
pub const LEAN: f32 = 8.0;

/// The extra rotation of one bone for a run, on top of the walk. Nothing
/// unless the body is standing (not crouching, swimming...) and moving.
pub fn offsets(role: Role, b: &Body, w: &Walk) -> [f32; 3] {
    let k = b.run.clamp(0.0, 1.0) * b.weight(Kind::Stand) * w.stride.clamp(0.0, 1.0);
    if k <= 0.0 {
        return [0.0; 3];
    }
    let ph = w.phase;
    let mut o = [0.0f32; 3];
    match role {
        Role::Belly => o[0] = -LEAN * 0.55,
        Role::Chest => {
            o[0] = -LEAN * 0.45;
            o[1] = -sinf(ph) * 3.0;
        }
        Role::Torso => o[0] = -LEAN,
        // The head comes back up to look ahead.
        Role::Neck => o[0] = LEAN * 0.4,
        Role::Head => o[0] = LEAN * 0.5,
        // Longer swings: the walk's own swing is the base.
        Role::Arm(s) => o[0] = arm_forward(s, ph) * 24.0,
        Role::Forearm(s) => o[0] = 50.0 + pos(arm_forward(s, ph - 0.6)) * 8.0,
        Role::Hand(_) => o[0] = 6.0,
        Role::Thigh(s) => o[0] = leg_forward(s, ph) * 14.0,
        // The heel kicks up behind, the knee rides high in front.
        Role::Calf(s) => o[0] = -(pos(leg_forward(s, ph + 0.9)) * 26.0 + pos(-leg_forward(s, ph + 0.4)) * 22.0),
        _ => {}
    }
    [o[0] * k, o[1] * k, o[2] * k]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn running() -> Body {
        Body { run: 1.0, ..Body::default() }
    }

    #[test]
    fn running_starts_above_a_walk_and_is_full_at_a_sprint() {
        assert_eq!(run_amount(0.0), 0.0);
        assert_eq!(run_amount(4.317), 0.0);
        assert_eq!(run_amount(f32::NAN), 0.0);
        assert_eq!(run_amount(5.612), 1.0);
        let mid = run_amount(5.0);
        assert!(mid > 0.0 && mid < 1.0);
        assert!(run_amount(5.1) > run_amount(4.9));
    }

    #[test]
    fn a_run_leans_forward_and_folds_the_elbows() {
        let w = Walk { phase: 1.0, stride: 1.0, clock: 0.0 };
        let lean = offsets(Role::Belly, &running(), &w)[0] + offsets(Role::Chest, &running(), &w)[0];
        assert!(lean < -5.0 && lean > -10.0, "a forward lean of 5 to 10 degrees: {lean}");
        for i in 0..40 {
            let w = Walk { phase: i as f32 * 0.17, stride: 1.0, clock: 0.0 };
            let e = offsets(Role::Forearm(1), &running(), &w)[0] + super::super::bends::walk(Role::Forearm(1), false, &w)[0];
            assert!(e >= 49.0 && e < 115.0, "elbows near a right angle: {e}");
        }
    }

    #[test]
    fn a_run_is_nothing_when_standing_still_walking_or_crouching() {
        let w = Walk { phase: 1.0, stride: 1.0, clock: 0.0 };
        assert_eq!(offsets(Role::Belly, &Body::default(), &w), [0.0; 3]);
        assert_eq!(offsets(Role::Belly, &running(), &Walk { stride: 0.0, ..w }), [0.0; 3]);
        let crouch = Body { run: 1.0, ..Body::steady(Kind::Crouch) };
        assert_eq!(offsets(Role::Belly, &crouch, &w), [0.0; 3]);
        // Half way into a crouch, half the run.
        let half = Body { kind: Kind::Crouch, from: Kind::Stand, mix: 0.5, run: 1.0, ..Body::default() };
        let (full, part) = (offsets(Role::Belly, &running(), &w)[0], offsets(Role::Belly, &half, &w)[0]);
        assert!((part - full * 0.5).abs() < 1e-4);
    }

    #[test]
    fn the_run_swings_arms_against_legs_and_opposite_sides_alternate() {
        let w = Walk { phase: 0.8, stride: 1.0, clock: 0.0 };
        let (al, ar) = (offsets(Role::Arm(1), &running(), &w)[0], offsets(Role::Arm(-1), &running(), &w)[0]);
        assert!((al + ar).abs() < 1e-4 && al.abs() > 5.0);
        let (ll, lr) = (offsets(Role::Thigh(1), &running(), &w)[0], offsets(Role::Thigh(-1), &running(), &w)[0]);
        assert!((ll + lr).abs() < 1e-4);
        assert!(al * ll < 0.0, "the left arm swings against the left leg");
        for i in 0..60 {
            let w = Walk { phase: i as f32 * 0.11, stride: 1.0, clock: 0.0 };
            for s in [1, -1] {
                assert!(offsets(Role::Calf(s), &running(), &w)[0] <= 0.0, "knees only bend one way");
            }
        }
    }

    #[test]
    fn the_phase_runs_on_without_a_jump() {
        // One full turn of the cycle ends where it began.
        let w0 = Walk { phase: 0.3, stride: 1.0, clock: 0.0 };
        let w1 = Walk { phase: 0.3 + 2.0 * core::f32::consts::PI, ..w0 };
        for r in [Role::Arm(1), Role::Forearm(-1), Role::Thigh(1), Role::Calf(-1)] {
            let (a, b) = (offsets(r, &running(), &w0), offsets(r, &running(), &w1));
            assert!((a[0] - b[0]).abs() < 1e-3, "{r:?}");
        }
        // And nearby phases give nearby angles.
        for i in 0..100 {
            let ph = i as f32 * 0.063;
            let (a, b) = (offsets(Role::Calf(1), &running(), &Walk { phase: ph, ..w0 }), offsets(Role::Calf(1), &running(), &Walk { phase: ph + 0.063, ..w0 }));
            assert!((a[0] - b[0]).abs() < 4.0);
        }
    }
}
