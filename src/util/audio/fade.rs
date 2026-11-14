//! Volume fades for sounds a mod controls (`fade_sound`) and for music
//! (fade in, fade out, crossfade).
//!
//! A [`Fade`] moves a number from where it is to a target over a time. The
//! curve is either smooth (ease in and out: for a volume that is going
//! somewhere) or equal-power (for two tracks that cross: one rising and one
//! falling along it have squares that add to 1, so a crossfade does not dip).

use crate::math::{cosf, sinf};

/// How the value moves between the two ends.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Curve {
    /// Ease in and out (smoothstep).
    Smooth,
    /// Quarter sine and cosine: two fades with opposite directions (0 to 1
    /// and 1 to 0) have squares that add to 1 at every moment.
    Equal,
}

#[derive(Clone, Copy, Debug)]
pub struct Fade {
    from: f32,
    to: f32,
    secs: f32,
    t: f32,
    curve: Curve,
}

impl Fade {
    /// A value that stays where it is.
    pub fn hold(v: f32) -> Fade {
        Fade { from: v, to: v, secs: 0.0, t: 0.0, curve: Curve::Smooth }
    }

    /// A value going from `from` to `to` in `secs` (a time of 0 or less
    /// arrives at once).
    pub fn new(from: f32, to: f32, secs: f32, curve: Curve) -> Fade {
        let secs = if secs.is_finite() && secs > 0.0 { secs.min(3600.0) } else { 0.0 };
        Fade { from: clean(from), to: clean(to), secs, t: 0.0, curve }
    }

    /// Where the value is now.
    pub fn value(&self) -> f32 {
        if self.secs <= 0.0 || self.t >= self.secs {
            return self.to;
        }
        let x = (self.t / self.secs).clamp(0.0, 1.0);
        let p = match self.curve {
            Curve::Smooth => x * x * (3.0 - 2.0 * x),
            Curve::Equal => {
                if self.to >= self.from {
                    sinf(x * core::f32::consts::FRAC_PI_2)
                } else {
                    1.0 - cosf(x * core::f32::consts::FRAC_PI_2)
                }
            }
        };
        self.from + (self.to - self.from) * p
    }

    /// Where it is going.
    pub fn target(&self) -> f32 {
        self.to
    }

    /// Whether it has arrived.
    pub fn done(&self) -> bool {
        self.secs <= 0.0 || self.t >= self.secs
    }

    /// Go on by `dt` seconds; the new value.
    pub fn tick(&mut self, dt: f32) -> f32 {
        if dt.is_finite() && dt > 0.0 {
            self.t += dt;
        }
        self.value()
    }

    /// Head for a new target from where the value is now.
    pub fn retarget(&mut self, to: f32, secs: f32, curve: Curve) {
        *self = Fade::new(self.value(), to, secs, curve);
    }
}

fn clean(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fade_goes_from_one_end_to_the_other_and_stays() {
        let mut f = Fade::new(0.0, 1.0, 2.0, Curve::Smooth);
        assert_eq!(f.value(), 0.0);
        assert!(!f.done());
        let mut last = 0.0;
        // 0.01 added 200 times in f32 falls just short of 2.0: one step more.
        for _ in 0..201 {
            let v = f.tick(0.01);
            assert!(v >= last - 1e-6 && v <= 1.0, "{v}");
            last = v;
        }
        assert_eq!(f.value(), 1.0);
        assert!(f.done());
        assert_eq!(f.tick(5.0), 1.0);
    }

    #[test]
    fn halfway_is_halfway_on_the_smooth_curve() {
        let mut f = Fade::new(0.0, 2.0, 1.0, Curve::Smooth);
        let v = f.tick(0.5);
        assert!((v - 1.0).abs() < 1e-4, "{v}");
    }

    #[test]
    fn zero_time_or_bad_numbers_arrive_at_once() {
        assert_eq!(Fade::new(0.0, 0.7, 0.0, Curve::Smooth).value(), 0.7);
        assert_eq!(Fade::new(0.0, 0.7, -3.0, Curve::Smooth).value(), 0.7);
        assert_eq!(Fade::new(0.0, 0.7, f32::NAN, Curve::Equal).value(), 0.7);
        assert_eq!(Fade::new(f32::NAN, f32::INFINITY, 1.0, Curve::Smooth).value(), 0.0);
        let mut f = Fade::hold(0.4);
        assert_eq!(f.tick(1.0), 0.4);
        assert!(f.done());
        // A bad step does not move it.
        let mut g = Fade::new(0.0, 1.0, 1.0, Curve::Smooth);
        assert_eq!(g.tick(f32::NAN), 0.0);
        assert_eq!(g.tick(-1.0), 0.0);
    }

    #[test]
    fn a_pair_of_equal_power_fades_keeps_the_power_constant() {
        let mut up = Fade::new(0.0, 1.0, 3.0, Curve::Equal);
        let mut down = Fade::new(1.0, 0.0, 3.0, Curve::Equal);
        for _ in 0..301 {
            let (a, b) = (up.tick(0.01), down.tick(0.01));
            let power = a * a + b * b;
            assert!((power - 1.0).abs() < 0.01, "{a} {b} {power}");
        }
        assert_eq!((up.value(), down.value()), (1.0, 0.0));
    }

    #[test]
    fn a_retarget_starts_where_the_value_is() {
        let mut f = Fade::new(0.0, 1.0, 1.0, Curve::Smooth);
        let mid = f.tick(0.5);
        f.retarget(0.0, 1.0, Curve::Smooth);
        assert!((f.value() - mid).abs() < 1e-5);
        f.tick(2.0);
        assert_eq!(f.value(), 0.0);
        assert_eq!(f.target(), 0.0);
    }
}
