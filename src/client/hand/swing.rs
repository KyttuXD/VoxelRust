//! What the first-person hand does: the swing of a blow or a use (LastRoute's
//! tweens, see `anim/held.rs` there), the walking bob, eating and drinking,
//! and the arm pulling back to draw a bow. Each gives a [`Motion`]; the arm
//! and the item both go through the same one so they never part.

use super::model::{rot_x, rot_z};
use super::{Held, View};
use crate::math::{cosf, sinf, PI};

/// A turn and a move of the hand, about the wrist, in the view's own axes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Motion {
    /// Turn about the view's x axis (the hand tips its top toward you with +).
    pub rx: f32,
    /// Turn about the view's z axis (a roll).
    pub rz: f32,
    pub off: [f32; 3],
}

impl Motion {
    /// A point of the hand (relative to the wrist) after the motion.
    pub fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let q = rot_z(rot_x(p, self.rx), self.rz);
        [q[0] + self.off[0], q[1] + self.off[1], q[2] + self.off[2]]
    }

    fn plus(self, o: Motion) -> Motion {
        Motion { rx: self.rx + o.rx, rz: self.rz + o.rz, off: [self.off[0] + o.off[0], self.off[1] + o.off[1], self.off[2] + o.off[2]] }
    }
}

fn lerp3(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]
}

/// Godot's TRANS_QUAD with EASE_IN_OUT (the Tween default for positions).
fn quad_in_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) * 2.0;
    if t < 1.0 { 0.5 * t * t } else { -0.5 * ((t - 1.0) * (t - 3.0) - 1.0) }
}

/// How far the hand has swung: 0 at rest, 1 at the top of a blow or a use.
/// It comes back to rest along a curve.
pub(super) fn swing_angle(swing: f32) -> f32 {
    -sinf(PI * swing.clamp(0.0, 1.0)) * 0.9
}

/// The swing of a tool, as LastRoute's `_animate_tool_use` tweens it on the
/// view node: a windup that raises the hand and tips the head back toward you
/// (to `arc` x 0.70, easing out), a strike that drops it forward past rest (to
/// -`arc` x 1.15, easing in), and a recovery (easing out); the hand moves
/// with each (easing in and out). `t` is 0 at rest, then runs 0..1 through
/// the whole use. Gives the turn about the wrist and how far the hand moves.
/// Anything else swings as before.
pub fn swing_of(held: Held, t: f32) -> (f32, [f32; 3]) {
    if !matches!(held, Held::Tool(_)) {
        return (swing_angle(t), [0.0; 3]);
    }
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return (0.0, [0.0; 3]);
    }
    let arc = 70.0f32.to_radians();
    let (up, down) = ([0.02f32, 0.05, 0.06], [-0.03f32, -0.05, -0.12]);
    if t < 0.30 {
        let u = t / 0.30;
        let e = 1.0 - (1.0 - u) * (1.0 - u);
        (arc * 0.70 * e, lerp3([0.0; 3], up, quad_in_out(u)))
    } else if t < 0.52 {
        let u = (t - 0.30) / 0.22;
        let e = u * u;
        (arc * (0.70 + (-1.15 - 0.70) * e), lerp3(up, down, quad_in_out(u)))
    } else {
        let u = (t - 0.52) / 0.48;
        let e = 1.0 - (1.0 - u) * (1.0 - u);
        (-arc * 1.15 * (1.0 - e), lerp3(down, [0.0; 3], quad_in_out(u)))
    }
}

/// LastRoute's use of an item that has no swing (`_animate_tool_use`
/// without one): the hand twists to rotation.z -0.72 and goes in and across
/// by (-.10, .04, -.06) in 0.09 s, and comes back in 0.16 s. `t` runs 0..1
/// through the 0.25 s.
pub fn use_twist(t: f32) -> (f32, [f32; 3]) {
    if t <= 0.0 || t >= 1.0 {
        return (0.0, [0.0; 3]);
    }
    let k = if t < 0.36 { quad_in_out(t / 0.36) } else { 1.0 - quad_in_out((t - 0.36) / 0.64) };
    (-0.72 * k, [-0.10 * k, 0.04 * k, -0.06 * k])
}

/// The hand rising to the mouth, taking bites, and coming down. `eat` is how
/// far through the eating, 0..1 (see `game::eating`). A drink is tipped back further.
pub fn eat_motion(eat: f32, drink: bool) -> Motion {
    let e = eat.clamp(0.0, 1.0);
    if e <= 0.0 {
        return Motion::default();
    }
    // Up quickly, held while the bites go, down at the end.
    let up = (e * 6.0).min((1.0 - e) * 8.0).clamp(0.0, 1.0);
    let k = quad_in_out(up);
    // Each bite dips the food toward the mouth a little.
    let bite = if up >= 1.0 { (sinf(e * 8.0 * PI)) * 0.012 } else { 0.0 };
    let tip = if drink { 1.05 } else { 0.55 };
    Motion { rx: tip * k, rz: -0.12 * k, off: [-0.20 * k, 0.06 * k + bite, -0.12 * k] }
}

/// The hand coming up and in to draw a bow, and pulling back as the string
/// does: `draw` is 0..1.
pub fn draw_motion(draw: f32) -> Motion {
    let d = draw.clamp(0.0, 1.0);
    if d <= 0.0 {
        return Motion::default();
    }
    // Up and toward the middle at once, then back toward you as it is drawn.
    let lift = quad_in_out((d * 4.0).min(1.0));
    Motion { rx: 0.12 * lift, rz: 0.0, off: [-0.17 * lift, 0.09 * lift, -0.02 * lift + 0.13 * d] }
}

/// The walking bob: how far the hand moves (right, up, forward) for the
/// game's `clock`, the player's speed over the ground, and whether they are
/// on it. A step lifts the hand and a pair of steps sways it.
pub fn walk_bob(clock: f32, speed: f32, on_ground: bool) -> [f32; 3] {
    let k = if on_ground { (speed / 4.0).clamp(0.0, 1.0) } else { 0.0 };
    let phase = clock * 9.0;
    let breathe = sinf(clock * 1.7) * 0.002;
    [cosf(phase * 0.5) * 0.012 * k, sinf(phase) * 0.014 * k + breathe, sinf(phase * 0.5) * 0.006 * k]
}

/// Everything the hand is doing in this view, as one motion.
pub fn motion_of(held: Held, v: &View) -> Motion {
    let mut m = Motion { off: v.bob, ..Motion::default() };
    if held == Held::Nothing {
        let (rx, off) = swing_of(held, v.swing);
        return m.plus(Motion { rx, rz: 0.0, off });
    }
    match held {
        // A bow's own motion is the draw; the click that starts it is not a blow.
        Held::Bow(_) => {}
        Held::Tool(_) => {
            let (rx, off) = swing_of(held, v.swing);
            m = m.plus(Motion { rx, rz: 0.0, off });
        }
        _ => {
            let (rz, off) = use_twist(v.swing);
            m = m.plus(Motion { rx: 0.0, rz, off });
        }
    }
    if matches!(held, Held::Bow(_)) {
        m = m.plus(draw_motion(v.draw));
    }
    if v.eat > 0.0 {
        m = m.plus(eat_motion(v.eat, v.drink));
    }
    m
}
