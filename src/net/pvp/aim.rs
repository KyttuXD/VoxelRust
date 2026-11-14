//! The attacker's side: which other player the crosshair is on.

/// Half the width and the height of a player's body (the same box the
/// server measures a hit against).
const HALF: f64 = 0.3;
const HEIGHT: f64 = 1.8;

/// How far along the ray (`origin`, unit `dir`) it enters the box of a body
/// standing at `feet`, if it does within `reach`.
pub fn ray_body(origin: [f64; 3], dir: [f64; 3], reach: f32, feet: [f64; 3]) -> Option<f32> {
    let lo = [feet[0] - HALF, feet[1], feet[2] - HALF];
    let hi = [feet[0] + HALF, feet[1] + HEIGHT, feet[2] + HALF];
    let (mut t0, mut t1) = (0.0f64, reach as f64);
    for a in 0..3 {
        if dir[a] > -1e-9 && dir[a] < 1e-9 {
            if origin[a] < lo[a] || origin[a] > hi[a] {
                return None;
            }
            continue;
        }
        let (mut a0, mut a1) = ((lo[a] - origin[a]) / dir[a], (hi[a] - origin[a]) / dir[a]);
        if a0 > a1 {
            core::mem::swap(&mut a0, &mut a1);
        }
        t0 = t0.max(a0);
        t1 = t1.min(a1);
    }
    if t0 <= t1 {
        Some(t0 as f32)
    } else {
        None
    }
}

/// The nearest of `bodies` (number, feet) the ray hits, and how far along it.
pub fn hit_test(origin: [f64; 3], dir: [f64; 3], reach: f32, bodies: impl Iterator<Item = (u16, [f64; 3])>) -> Option<(u16, f32)> {
    let mut best: Option<(u16, f32)> = None;
    for (id, feet) in bodies {
        if let Some(t) = ray_body(origin, dir, reach, feet) {
            if best.map_or(true, |b| t < b.1) {
                best = Some((id, t));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ray_finds_the_body_in_front_and_the_nearer_of_two() {
        let eye = [0.0, 65.62, 0.0];
        let ahead = [1.0, 0.0, 0.0];
        let t = ray_body(eye, ahead, 4.5, [3.0, 64.0, 0.0]).unwrap();
        assert!((t - 2.7).abs() < 1e-4);
        let bodies = [(1u16, [4.0, 64.0, 0.0]), (2, [3.0, 64.0, 0.1]), (3, [-3.0, 64.0, 0.0])];
        assert_eq!(hit_test(eye, ahead, 4.5, bodies.iter().copied()).map(|h| h.0), Some(2));
    }

    #[test]
    fn it_misses_beside_above_behind_and_beyond_the_reach() {
        let eye = [0.0, 65.62, 0.0];
        let ahead = [1.0, 0.0, 0.0];
        assert_eq!(ray_body(eye, ahead, 4.5, [3.0, 64.0, 1.0]), None); // beside
        assert_eq!(ray_body(eye, ahead, 4.5, [3.0, 67.0, 0.0]), None); // above
        assert_eq!(ray_body(eye, ahead, 4.5, [-3.0, 64.0, 0.0]), None); // behind
        assert_eq!(ray_body(eye, ahead, 4.5, [6.0, 64.0, 0.0]), None); // too far
        assert!(hit_test(eye, ahead, 4.5, core::iter::empty()).is_none());
    }

    #[test]
    fn looking_down_at_someone_below_hits_their_head() {
        let eye = [0.0, 70.0, 0.0];
        let down = [0.0, -1.0, 0.0];
        let t = ray_body(eye, down, 5.0, [0.1, 66.0, 0.0]).unwrap();
        assert!((t - 2.2).abs() < 1e-4);
    }
}
