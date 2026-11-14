//! How big a creature may be and how tough, in one place: the data parser
//! (`height`, `width`, `health` lines), the script calls and the native API
//! all clamp with these. Ordinary creatures (up to 6 blocks tall, 3 wide)
//! behave exactly as before; only bodies past those old limits are "big" and
//! get the cheaper handling below (no single-cell pathing, a swept-slab
//! collision test, a longer keep-alive distance).

/// Smallest body, in blocks.
pub const MIN_SIZE: f32 = 0.1;
/// Tallest and widest body, in blocks.
pub const MAX_HEIGHT: f32 = 64.0;
pub const MAX_WIDTH: f32 = 32.0;
/// The limits before big creatures existed: a body inside them is "ordinary".
pub const ORDINARY_HEIGHT: f32 = 6.0;
pub const ORDINARY_WIDTH: f32 = 3.0;
/// Most health a creature may have.
pub const MAX_HEALTH: i32 = 1_000_000;

/// A size from a script or the network, brought inside the limits. Anything
/// that is not a number becomes the smallest size.
pub fn clamp_size(width: f64, height: f64) -> (f32, f32) {
    let clean = |v: f64, hi: f32| if v.is_finite() { (v as f32).clamp(MIN_SIZE, hi) } else { MIN_SIZE };
    (clean(width, MAX_WIDTH), clean(height, MAX_HEIGHT))
}

/// Health from a script or the network, from 1 to [`MAX_HEALTH`].
pub fn clamp_health(hp: i64) -> i32 {
    hp.clamp(1, MAX_HEALTH as i64) as i32
}

/// Current health: 0 (dead) is allowed, up to the largest maximum.
pub fn clamp_current(hp: i64) -> i32 {
    hp.clamp(0, MAX_HEALTH as i64) as i32
}

/// Whether a body is past the old limits.
pub fn is_big(width: f32, height: f32) -> bool {
    width > ORDINARY_WIDTH || height > ORDINARY_HEIGHT
}

/// The tallest ledge a body of this size steps onto without jumping, in
/// blocks: 0.6 for ordinary ones, growing with the width (a quarter of it) up to 6.
pub fn step_up(width: f64) -> f64 {
    let w = if width.is_finite() { width } else { 0.0 };
    (w * 0.25).clamp(0.6, 6.0)
}

/// How far past the usual 96 blocks (and 48 for creatures in unloaded
/// country) a creature stays alive and drawn: its largest size less half an
/// ordinary body's. 0 for ordinary creatures.
pub fn keep_extra(width: f32, height: f32) -> f32 {
    let big = if width > height { width } else { height };
    if big.is_finite() && big > ORDINARY_HEIGHT {
        big - ORDINARY_HEIGHT * 0.5
    } else {
        0.0
    }
}

/// A creature's drawn bounds, as a radius around its feet for the frustum
/// test: the largest of its height and width (so it does not pop out of view
/// while only its middle is behind the camera).
pub fn cull_radius(width: f32, height: f32) -> f32 {
    let r = if width > height { width } else { height };
    if r.is_finite() {
        r.max(4.0)
    } else {
        4.0
    }
}

/// The size of the nameplate or health bar for a creature of this drawn
/// height: 1 up to an ordinary body, then growing (at most 8 times).
pub fn plate_scale(height: f32) -> f32 {
    if height.is_finite() && height > ORDINARY_HEIGHT {
        (height / ORDINARY_HEIGHT).min(8.0)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_kept_inside_the_limits() {
        assert_eq!(clamp_size(1.0, 2.0), (1.0, 2.0));
        assert_eq!(clamp_size(1000.0, 1000.0), (MAX_WIDTH, MAX_HEIGHT));
        assert_eq!(clamp_size(-3.0, 0.0), (MIN_SIZE, MIN_SIZE));
        assert_eq!(clamp_size(f64::NAN, f64::INFINITY), (MIN_SIZE, MIN_SIZE));
        assert_eq!(clamp_size(32.0, 64.0), (32.0, 64.0));
    }

    #[test]
    fn health_is_kept_inside_the_limits() {
        assert_eq!(clamp_health(0), 1);
        assert_eq!(clamp_health(-5), 1);
        assert_eq!(clamp_health(20), 20);
        assert_eq!(clamp_health(i64::MAX), MAX_HEALTH);
        assert_eq!(clamp_current(-1), 0);
        assert_eq!(clamp_current(5_000_000), MAX_HEALTH);
    }

    #[test]
    fn ordinary_creatures_are_unchanged() {
        assert!(!is_big(3.0, 6.0));
        assert!(is_big(3.1, 1.0) && is_big(1.0, 6.1));
        assert_eq!(step_up(0.6), 0.6);
        assert_eq!(step_up(1.0), 0.6);
        assert_eq!(keep_extra(3.0, 6.0), 0.0);
        assert_eq!(plate_scale(1.8), 1.0);
        assert_eq!(cull_radius(0.5, 0.5), 4.0);
    }

    #[test]
    fn big_creatures_get_more_room() {
        assert_eq!(step_up(16.0), 4.0);
        assert_eq!(step_up(32.0), 6.0);
        assert_eq!(step_up(f64::NAN), 0.6);
        assert!(keep_extra(5.0, 64.0) > 60.0);
        assert_eq!(cull_radius(30.0, 64.0), 64.0);
        assert_eq!(plate_scale(64.0), 8.0);
        assert!(plate_scale(12.0) > 1.9 && plate_scale(12.0) < 2.1);
    }
}
