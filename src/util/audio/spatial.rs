//! Where a sound is: how loud it is at a distance, how dull, and where
//! between the left and right ears. The mixer's stereo is a pair of gains
//! (see `client/audio.rs`), not a model of a head.

use crate::math::{cosf, sinf, sqrtf};

/// Up to this many blocks away a sound is at full volume.
pub const NEAR: f32 = 4.0;
/// Farther than this many blocks it is not heard at all.
pub const FAR: f32 = 64.0;

/// Loudness (0..1) of a sound `d` blocks from the listener: full inside
/// [`NEAR`], then the inverse of the distance (half at 8 blocks, a twelfth
/// at 48), and over the last quarter before [`FAR`] it falls to nothing, so
/// a sound never cuts off.
pub fn distance_gain(d: f32) -> f32 {
    if !d.is_finite() || d >= FAR {
        return 0.0;
    }
    if d <= NEAR {
        return 1.0;
    }
    let taper_from = FAR * 0.75;
    let taper = if d > taper_from { (FAR - d) / (FAR - taper_from) } else { 1.0 };
    NEAR / d * taper
}

/// How open a recording's highs are at distance `d`: 1 near, down to 0.3 far
/// away (a one-pole filter coefficient for the mixer).
pub fn brightness(d: f32) -> f32 {
    let x = if d.is_finite() { ((d - NEAR) / (FAR - NEAR)).clamp(0.0, 1.0) } else { 1.0 };
    1.0 - 0.7 * x
}

/// Distance between two points.
pub fn distance(a: [f64; 3], b: [f64; 3]) -> f32 {
    let (dx, dy, dz) = ((a[0] - b[0]) as f32, (a[1] - b[1]) as f32, (a[2] - b[2]) as f32);
    sqrtf(dx * dx + dy * dy + dz * dz)
}

/// Where a source at `src` is heard from by a listener at `ear` looking along
/// `yaw` (the game's yaw: 0 looks down -Z, positive turns left): -1 is hard
/// left, 1 hard right, 0 ahead or behind. Close to the listener it narrows to
/// the middle (a sound inside the head has no side).
pub fn pan_of(ear: [f64; 3], yaw: f32, src: [f64; 3]) -> f32 {
    let (dx, dz) = ((src[0] - ear[0]) as f32, (src[2] - ear[2]) as f32);
    // The listener's right-hand side on the ground.
    let (rx, rz) = (cosf(yaw), -sinf(yaw));
    let d = sqrtf(dx * dx + dz * dz);
    if d < 1e-4 {
        return 0.0;
    }
    let side = (dx * rx + dz * rz) / d;
    let near = (d / 2.0).min(1.0);
    (side * near).clamp(-1.0, 1.0)
}

/// The left and right gains for a pan from -1 to 1: a balance control. The
/// middle plays both ears in full; moving to one side lowers the other.
pub fn balance(pan: f32) -> (f32, f32) {
    let p = if pan.is_finite() { pan.clamp(-1.0, 1.0) } else { 0.0 };
    (if p > 0.0 { 1.0 - p } else { 1.0 }, if p < 0.0 { 1.0 + p } else { 1.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_is_full_far_is_nothing_and_it_never_jumps() {
        assert_eq!(distance_gain(0.0), 1.0);
        assert_eq!(distance_gain(NEAR), 1.0);
        assert!((distance_gain(8.0) - 0.5).abs() < 1e-6);
        assert_eq!(distance_gain(FAR), 0.0);
        assert_eq!(distance_gain(1000.0), 0.0);
        assert_eq!(distance_gain(f32::NAN), 0.0);
        let mut last = 1.0;
        let mut d = 0.0;
        while d < FAR + 2.0 {
            let g = distance_gain(d);
            assert!(g <= last + 1e-6, "louder further away at {d}");
            assert!(last - g < 0.05, "a jump at {d}: {last} to {g}");
            last = g;
            d += 0.05;
        }
    }

    #[test]
    fn far_recordings_are_duller() {
        assert_eq!(brightness(0.0), 1.0);
        assert!((brightness(FAR) - 0.3).abs() < 1e-6);
        assert!(brightness(30.0) < brightness(10.0));
    }

    #[test]
    fn pan_follows_the_side_the_sound_is_on() {
        let ear = [0.0, 0.0, 0.0];
        // Yaw 0 looks down -Z: +X is on the right.
        assert!(pan_of(ear, 0.0, [10.0, 0.0, 0.0]) > 0.99);
        assert!(pan_of(ear, 0.0, [-10.0, 0.0, 0.0]) < -0.99);
        assert!(pan_of(ear, 0.0, [0.0, 0.0, -10.0]).abs() < 0.01);
        assert!(pan_of(ear, 0.0, [0.0, 0.0, 10.0]).abs() < 0.01);
        // Turned left by a quarter turn (yaw pi/2), -X is ahead, -Z is on the right and +Z on the left.
        let q = core::f32::consts::FRAC_PI_2;
        assert!(pan_of(ear, q, [0.0, 0.0, -10.0]) > 0.99);
        assert!(pan_of(ear, q, [0.0, 0.0, 10.0]) < -0.99);
        assert!(pan_of(ear, q, [-10.0, 0.0, 0.0]).abs() < 0.01);
        // At the listener's own place: the middle.
        assert_eq!(pan_of(ear, 0.0, ear), 0.0);
        assert!(pan_of(ear, 0.0, [0.2, 0.0, 0.0]).abs() < 0.2);
    }

    #[test]
    fn balance_lowers_the_far_ear_only() {
        assert_eq!(balance(0.0), (1.0, 1.0));
        assert_eq!(balance(1.0), (0.0, 1.0));
        assert_eq!(balance(-1.0), (1.0, 0.0));
        assert_eq!(balance(0.5), (0.5, 1.0));
        assert_eq!(balance(7.0), (0.0, 1.0));
        assert_eq!(balance(f32::NAN), (1.0, 1.0));
    }

    #[test]
    fn distance_is_euclidean() {
        assert!((distance([0.0, 0.0, 0.0], [3.0, 4.0, 0.0]) - 5.0).abs() < 1e-3);
    }
}
