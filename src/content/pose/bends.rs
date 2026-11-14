//! The bends of a walking body: the torso twisting against the hips, the
//! neck and head steadying, shoulders swinging into elbows and wrists,
//! hips into knees and ankles. Each joint runs a little behind the one
//! above it, so a limb flexes like a real one instead of swinging stiffly.

use super::{Role, Walk};
use crate::math::{cosf, sinf, sqrtf};

fn pos(v: f32) -> f32 {
    if v > 0.0 {
        v
    } else {
        0.0
    }
}

/// How far a body's arm is swung forward (positive) at `phase`. The left
/// arm swings against the left leg, as the walk cycle has it.
pub fn arm_forward(side: i8, phase: f32) -> f32 {
    -(side as f32) * sinf(phase)
}

/// How far a leg is swung forward at `phase`.
pub fn leg_forward(side: i8, phase: f32) -> f32 {
    side as f32 * sinf(phase)
}

/// The extra rotation of a bone for the walk cycle, and for the sway of a
/// body at rest. `handled`: the swing of this bone is already done.
pub fn walk(role: Role, handled: bool, w: &Walk) -> [f32; 3] {
    let (ph, st, ck) = (w.phase, w.stride, w.clock);
    let still = 1.0 - st.clamp(0.0, 1.0);
    // Breathing: the chest rises and falls, the arms drift.
    let breath = sinf(ck * 1.7);
    let mut o = [0.0f32; 3];
    match role {
        Role::Belly => {
            o[0] = -3.0 * st;
            o[1] = sinf(ph) * 5.0 * st;
        }
        Role::Chest => {
            o[0] = -2.0 * st + breath * 1.1 * still;
            o[1] = -sinf(ph) * 6.0 * st;
        }
        // A creature's one-piece body only breathes (it may carry its legs).
        Role::Torso => {
            o[0] = breath * 1.0 * still;
        }
        Role::Neck => {
            o[1] = sinf(ph) * 3.0 * st;
        }
        Role::Head => {
            // The head stays level while the body twists under it, with a small nod each step.
            o[1] = sinf(ph) * 4.0 * st;
            o[0] = cosf(ph * 2.0) * 1.8 * st + breath * 0.6 * still;
        }
        Role::Arm(s) => {
            // Shoulders: the idle drift (the walk swing is the model's own).
            o[2] = -(s as f32) * (1.2 + breath * 0.8) * still;
        }
        Role::Forearm(s) if !handled => {
            // The elbow folds as the arm comes forward, a beat behind it: about 10 to 35
            // degrees on the forward swing, straight on the back swing.
            o[0] = sqrtf(pos(arm_forward(s, ph - 0.6)).min(1.0)) * 35.0 * st + 3.0 * still;
        }
        Role::Hand(s) => {
            o[0] = pos(arm_forward(s, ph - 1.2)) * 8.0 * st;
            o[2] = -(s as f32) * sinf(ph - 1.0) * 3.0 * st;
        }
        Role::Calf(s) if !handled => {
            // The knee folds as the leg swings forward, the heel trailing.
            o[0] = -(pos(leg_forward(s, ph + 0.9)) * 46.0 + 4.0) * st;
        }
        Role::Foot(s) => {
            // Toes up as the heel strikes, down as the foot pushes off.
            o[0] = pos(leg_forward(s, ph + 0.3)) * 18.0 * st - pos(-leg_forward(s, ph)) * 22.0 * st;
        }
        _ => {}
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knees_only_fold_one_way_and_only_while_walking() {
        let mut worst = 0.0f32;
        for i in 0..64 {
            let w = Walk { phase: i as f32 * 0.1, stride: 1.0, clock: 0.0 };
            let o = walk(Role::Calf(1), false, &w);
            assert!(o[0] <= 0.0, "a knee must not bend backwards: {o:?}");
            worst = worst.min(o[0]);
        }
        assert!(worst < -40.0 && worst >= -50.5, "the knee should fold well, up to 50 degrees: {worst}");
        assert_eq!(walk(Role::Calf(1), false, &Walk::default()), [0.0; 3]);
        // A bone whose swing is the model's own is left to it.
        assert_eq!(walk(Role::Calf(1), true, &Walk { phase: 1.0, stride: 1.0, clock: 0.0 }), [0.0; 3]);
    }

    #[test]
    fn elbows_fold_forward_and_opposite_arms_alternate() {
        let w = Walk { phase: 2.0, stride: 1.0, clock: 0.0 };
        let (l, r) = (walk(Role::Forearm(1), false, &w), walk(Role::Forearm(-1), false, &w));
        assert!(l[0] >= 0.0 && r[0] >= 0.0);
        assert!((l[0] - r[0]).abs() > 1.0, "the arms should not match: {l:?} {r:?}");
        assert!((arm_forward(1, 0.7) + arm_forward(-1, 0.7)).abs() < 1e-6);
    }

    #[test]
    fn the_chest_breathes_when_still_and_twists_when_walking() {
        let still = walk(Role::Chest, false, &Walk { phase: 0.0, stride: 0.0, clock: 0.9 });
        assert!(still[0].abs() > 0.1 && still[0].abs() < 2.0 && still[1] == 0.0);
        let going = walk(Role::Chest, false, &Walk { phase: 1.0, stride: 1.0, clock: 0.9 });
        assert!(going[1].abs() > 3.0);
    }

    #[test]
    fn both_player_models_have_the_bend_bones_a_mod_body_can_copy() {
        for text in [include_str!("../../../data/models/player.bbmodel"), include_str!("../../../data/models/player_slim.bbmodel")] {
            let m = crate::bbmodel::load(text).unwrap();
            let roles: alloc::vec::Vec<Role> = m.bones.iter().map(|b| super::super::classify(&b.name)).collect();
            for side in [1i8, -1] {
                for want in [Role::Arm(side), Role::Forearm(side), Role::Hand(side), Role::Thigh(side), Role::Calf(side), Role::Foot(side)] {
                    assert_eq!(roles.iter().filter(|r| **r == want).count(), 1, "{want:?}");
                }
            }
            // The elbow and the knee are pivots half way down their limbs: the forearm's pivot is below the arm's.
            let origin = |name: &str| m.bones[m.bone(name).unwrap()].origin;
            assert!(origin("L-Forearm")[1] < origin("L-Arm")[1] && origin("L-Hand")[1] < origin("L-Forearm")[1]);
            assert!(origin("R-Calf")[1] < origin("R-Thigh")[1] && origin("R-Foot")[1] < origin("R-Calf")[1]);
            let rig = super::super::rig_of(&roles, &[origin("L-Thigh")[1], origin("R-Thigh")[1]], m.max[1] - m.min[1]);
            assert!(rig.humanoid);
        }
    }

    #[test]
    fn sprinting_and_walking_stay_in_range_for_every_bone() {
        let all = [Role::Belly, Role::Chest, Role::Neck, Role::Head, Role::Arm(1), Role::Arm(-1), Role::Forearm(1), Role::Forearm(-1), Role::Hand(1), Role::Calf(1), Role::Calf(-1), Role::Foot(1), Role::Thigh(1)];
        for i in 0..200 {
            let w = Walk { phase: i as f32 * 0.05, stride: 1.0, clock: i as f32 * 0.1 };
            let run = super::super::Body { run: 1.0, ..super::super::Body::default() };
            for r in all {
                let mut rot = [0.0f32; 3];
                super::super::apply(r, false, &run, &w, &mut rot);
                assert!(rot.iter().all(|a| a.is_finite() && a.abs() < 130.0), "{r:?} {rot:?}");
            }
        }
    }
}
