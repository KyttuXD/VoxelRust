//! What the arms are doing on top of a posture: drawing a bow, eating or
//! drinking, striking a blow, carrying a load, and playing an emote.

use super::{Body, Role, Walk};
use crate::math::{sinf, PI};

/// A bow drawn this far (0..1) when only the fact that it is drawn is known.
const SHOWN_DRAW: f32 = 0.8;

/// Add the offsets of the activities of `b` for one bone to `o`.
pub fn add(role: Role, b: &Body, w: &Walk, o: &mut [f32; 3]) {
    if b.flags & super::BOW != 0 {
        let draw = if b.draw > 0.0 { b.draw } else { SHOWN_DRAW };
        super::add(o, bow(role, draw), 1.0);
    }
    if b.flags & super::EAT != 0 {
        super::add(o, eat(role, w.clock), 1.0);
    }
    if b.flags & super::SWING != 0 {
        super::add(o, strike(role, b.swing), 1.0);
    }
    if b.flags & super::CARRY != 0 {
        super::add(o, carry(role), 1.0);
    }
}

/// Left arm out holding the bow, right arm drawing the string back to the
/// cheek, the shoulders turned side-on to the target.
pub fn bow(role: Role, draw: f32) -> [f32; 3] {
    let d = draw.clamp(0.0, 1.0);
    match role {
        Role::Belly => [0.0, -10.0 * d, 0.0],
        Role::Chest | Role::Torso => [0.0, -14.0 * d, 0.0],
        Role::Head => [0.0, 24.0 * d, 0.0],
        // The bow arm: straight out ahead.
        Role::Arm(1) => [86.0, 0.0, 0.0],
        Role::Forearm(1) => [4.0, 0.0, 0.0],
        // The string arm: out, then the elbow folds back as the string is drawn.
        Role::Arm(-1) => [78.0 + 8.0 * d, 0.0, 0.0],
        Role::Forearm(-1) => [10.0 + 100.0 * d, 0.0, 0.0],
        _ => [0.0; 3],
    }
}

/// The right hand to the mouth, over and over.
pub fn eat(role: Role, clock: f32) -> [f32; 3] {
    let bite = 0.5 + 0.5 * sinf(clock * 9.0);
    match role {
        Role::Arm(-1) => [58.0 + 6.0 * bite, 0.0, 0.0],
        Role::Forearm(-1) => [92.0 + 14.0 * bite, 0.0, 0.0],
        Role::Hand(-1) => [-10.0, 0.0, 0.0],
        Role::Head => [-4.0 * bite, 0.0, 0.0],
        _ => [0.0; 3],
    }
}

/// A blow with the right arm: up and back, then forward and down.
/// `p` is how far through the blow it is, 0..1.
pub fn strike(role: Role, p: f32) -> [f32; 3] {
    let p = p.clamp(0.0, 1.0);
    // Winding up for the first third, then a fast stroke through.
    let reach = if p < 0.3 { -70.0 * (p / 0.3) } else { -70.0 + 190.0 * ease_out((p - 0.3) / 0.7) };
    let settle = if p < 0.3 { 0.0 } else { (1.0 - (p - 0.3) / 0.7).max(0.0) };
    match role {
        Role::Arm(-1) => [60.0 + reach * 0.9, 0.0, 0.0],
        Role::Forearm(-1) => [30.0 * settle + 10.0, 0.0, 0.0],
        Role::Chest | Role::Torso => [0.0, 16.0 * sinf(p * PI), 0.0],
        _ => [0.0; 3],
    }
}

fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t)
}

/// Both arms out in front, holding something against the chest.
pub fn carry(role: Role) -> [f32; 3] {
    match role {
        Role::Arm(s) => [62.0, 0.0, (s as f32) * 6.0],
        Role::Forearm(_) => [46.0, 0.0, 0.0],
        Role::Hand(_) => [-12.0, 0.0, 0.0],
        _ => [0.0; 3],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drawn_bow_folds_the_string_arm_and_holds_the_other_out() {
        let slack = bow(Role::Forearm(-1), 0.0)[0];
        let drawn = bow(Role::Forearm(-1), 1.0)[0];
        assert!(drawn > slack + 80.0);
        assert!(bow(Role::Arm(1), 1.0)[0] > 70.0 && bow(Role::Arm(-1), 1.0)[0] > 70.0);
        assert_eq!(bow(Role::Thigh(1), 1.0), [0.0; 3]);
    }

    #[test]
    fn a_blow_winds_up_and_follows_through() {
        let start = strike(Role::Arm(-1), 0.0)[0];
        let wound = strike(Role::Arm(-1), 0.3)[0];
        let end = strike(Role::Arm(-1), 1.0)[0];
        assert!(wound < start - 40.0, "the arm goes back first");
        assert!(end > wound + 100.0, "then through");
    }

    #[test]
    fn eating_brings_the_hand_up() {
        let o = eat(Role::Forearm(-1), 0.5);
        assert!(o[0] > 85.0);
        assert_eq!(eat(Role::Forearm(1), 0.5), [0.0; 3]);
    }

    #[test]
    fn activities_leave_a_plain_body_alone() {
        let mut o = [0.0; 3];
        add(Role::Arm(-1), &Body::default(), &Walk::default(), &mut o);
        assert_eq!(o, [0.0; 3]);
    }
}
