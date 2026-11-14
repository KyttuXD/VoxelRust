//! Items a mod sets down in the world with a velocity (`drop_item`), as the
//! player's own dropped items and a broken block's are: they fall, float,
//! burn in lava and wait to be picked up (`Session::drops`).
//!
//! The limits: at most [`MAX_LIVE`] lie about at once (a call that would make
//! the 129th is refused, so a script never pushes away the player's own
//! things), a stack is 1 to [`MAX_COUNT`], a speed at most [`MAX_SPEED`]
//! blocks a second in each direction, and [`MAX_PER_FRAME`] a frame.

use crate::session::Session;

/// Most dropped items in the world (the game's own limit).
pub const MAX_LIVE: usize = 128;
/// Largest stack a call may drop.
pub const MAX_COUNT: i64 = 64;
/// Fastest toss, blocks a second in each direction.
pub const MAX_SPEED: f64 = 40.0;
/// Most drops scripts may make in one frame.
pub const MAX_PER_FRAME: u32 = 32;
/// As far from the middle of the world as a drop may be.
pub const EDGE: f64 = 3.0e7;

/// A stack size held to 1 up to [`MAX_COUNT`].
pub fn clamp_count(n: i64) -> u16 {
    n.clamp(1, MAX_COUNT) as u16
}

/// A velocity component held to the limit.
pub fn clamp_speed(v: f64) -> f32 {
    v.clamp(-MAX_SPEED, MAX_SPEED) as f32
}

/// Whether there is room for one more drop, given how many lie about and
/// how many scripts made this frame.
pub fn has_room(live: usize, this_frame: u32) -> bool {
    live < MAX_LIVE && this_frame < MAX_PER_FRAME
}

/// Whether a place is one a drop may be set down at.
pub fn place_ok(p: [f64; 3]) -> bool {
    p.iter().all(|v| v.is_finite()) && p[0] >= -EDGE && p[0] <= EDGE && p[2] >= -EDGE && p[2] <= EDGE
}

/// Set `count` of item number `item` (an index into the registry's items)
/// down at `pos`, thrown with `vel`. `false` when it was refused: the
/// player may not change the world here, no such item, a place that makes
/// no sense, or no room.
pub fn spawn(g: &mut Session, pos: [f64; 3], vel: [f64; 3], item: usize, count: i64) -> bool {
    if !super::may_change(g) || item >= crate::block::reg().items.len() || item > u16::MAX as usize || !place_ok(pos) || !vel.iter().all(|v| v.is_finite()) {
        return false;
    }
    let live = g.drops.len();
    let s = super::state(g);
    if !has_room(live, s.dropped) {
        return false;
    }
    s.dropped += 1;
    let at = [pos[0], pos[1].clamp(0.0, crate::chunk::CH as f64 + 32.0), pos[2]];
    g.drop_item(at, [clamp_speed(vel[0]), clamp_speed(vel[1]), clamp_speed(vel[2])], item as u16, clamp_count(count));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacks_are_one_to_sixty_four() {
        assert_eq!(clamp_count(0), 1);
        assert_eq!(clamp_count(-5), 1);
        assert_eq!(clamp_count(1), 1);
        assert_eq!(clamp_count(64), 64);
        assert_eq!(clamp_count(65), 64);
        assert_eq!(clamp_count(i64::MAX), 64);
        assert_eq!(clamp_count(i64::MIN), 1);
    }

    #[test]
    fn speeds_are_held() {
        assert_eq!(clamp_speed(5.0), 5.0);
        assert_eq!(clamp_speed(1.0e9), 40.0);
        assert_eq!(clamp_speed(-1.0e9), -40.0);
    }

    #[test]
    fn there_is_room_for_128_and_32_a_frame() {
        assert!(has_room(0, 0));
        assert!(has_room(127, 0));
        assert!(!has_room(128, 0));
        assert!(!has_room(0, 32));
        assert!(has_room(0, 31));
        assert!(!has_room(500, 500));
    }

    #[test]
    fn places_must_be_real() {
        assert!(place_ok([0.0, 64.0, 0.0]));
        assert!(place_ok([-2.9e7, 1000.0, 2.9e7]));
        assert!(!place_ok([f64::NAN, 0.0, 0.0]));
        assert!(!place_ok([0.0, f64::INFINITY, 0.0]));
        assert!(!place_ok([4.0e7, 0.0, 0.0]));
        assert!(!place_ok([0.0, 0.0, -4.0e7]));
    }
}
