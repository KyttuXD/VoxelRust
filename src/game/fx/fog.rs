//! A fog a mod asks for (`set_fog`): darkness that closes in, a mist, a
//! colour over the distance.
//!
//! The engine sets the fog before it draws the world, and a mod's own drawing
//! comes after, so a mod cannot blend with it by drawing. Instead it says what
//! it wants and the renderer folds that into the fog of the next world pass:
//! the start, the end and the colour are mixed with the dimension's and the
//! sky's by `mix` (the call's `density`, 0 to 1). The override lives
//! [`HOLD`] seconds after the last call, so a mod that stops asking (or is
//! switched off) gives the fog back; one that asks every frame keeps it.

/// Seconds an override stays after the last call that asked for it.
pub const HOLD: f32 = 0.5;
/// Farthest a fog's start or end may be set, blocks.
pub const MAX_DISTANCE: f32 = 512.0;

/// The fog to mix in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blend {
    pub start: f32,
    pub end: f32,
    /// Colour, 0 to 1 each.
    pub rgb: [f32; 3],
    /// How much of this fog there is against the usual one, 0 to 1.
    pub mix: f32,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

impl Blend {
    /// The fog colour with this mixed in.
    pub fn colour(&self, base: [f32; 3]) -> [f32; 3] {
        [lerp(base[0], self.rgb[0], self.mix), lerp(base[1], self.rgb[1], self.mix), lerp(base[2], self.rgb[2], self.mix)]
    }

    /// The fog's start and end with this mixed in (never an end before the start).
    pub fn range(&self, start: f32, end: f32) -> (f32, f32) {
        let s = lerp(start, self.start, self.mix).max(0.0);
        let e = lerp(end, self.end, self.mix).max(s + 0.5);
        (s, e)
    }
}

/// The override a mod holds, and for how much longer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FogHold {
    cur: Option<Blend>,
    left: f32,
}

impl FogHold {
    /// Ask for a fog: `start` and `end` in blocks (0 to 512, the end at least a
    /// block past the start), the colour 0 to 255 and `density` 0 to 1. A
    /// density of 0 takes the override away. `false` for a number that is not finite.
    pub fn set(&mut self, start: f64, end: f64, rgb: [u8; 3], density: f64) -> bool {
        if !(start.is_finite() && end.is_finite() && density.is_finite()) {
            return false;
        }
        let mix = density.clamp(0.0, 1.0) as f32;
        if mix <= 0.0 {
            *self = FogHold::default();
            return true;
        }
        let s = (start as f32).clamp(0.0, MAX_DISTANCE);
        let e = (end as f32).clamp(s + 1.0, MAX_DISTANCE + 1.0);
        self.cur = Some(Blend { start: s, end: e, rgb: [rgb[0] as f32 / 255.0, rgb[1] as f32 / 255.0, rgb[2] as f32 / 255.0], mix });
        self.left = HOLD;
        true
    }

    /// Let `dt` seconds pass; the fog goes when it was not asked for again in time.
    pub fn tick(&mut self, dt: f32) {
        if self.cur.is_none() {
            return;
        }
        if dt.is_finite() && dt > 0.0 {
            self.left -= dt;
        }
        if self.left <= 0.0 {
            *self = FogHold::default();
        }
    }

    /// The fog now, if one is held.
    pub fn get(&self) -> Option<Blend> {
        self.cur
    }

    /// Seconds before it goes without another call.
    pub fn left(&self) -> f32 {
        if self.cur.is_some() {
            self.left
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fog_is_held_then_let_go() {
        let mut f = FogHold::default();
        assert_eq!(f.get(), None);
        assert!(f.set(2.0, 30.0, [10, 20, 30], 0.8));
        let b = f.get().unwrap();
        assert_eq!((b.start, b.end, b.mix), (2.0, 30.0, 0.8));
        f.tick(0.3);
        assert!(f.get().is_some());
        // Asked again: held another half second.
        assert!(f.set(2.0, 30.0, [10, 20, 30], 0.8));
        f.tick(0.4);
        assert!(f.get().is_some());
        f.tick(0.2);
        assert_eq!(f.get(), None, "half a second after the last call it is gone");
        assert_eq!(f.left(), 0.0);
        // A mod that asks every frame keeps it.
        for _ in 0..600 {
            f.set(0.0, 20.0, [0, 0, 0], 1.0);
            f.tick(1.0 / 60.0);
        }
        assert!(f.get().is_some());
        // A frame of nonsense time neither ends nor extends it.
        let before = f.left();
        f.tick(f32::NAN);
        f.tick(-5.0);
        assert_eq!(f.left(), before);
    }

    #[test]
    fn numbers_are_bounded_and_nonsense_is_refused() {
        let mut f = FogHold::default();
        assert!(!f.set(f64::NAN, 10.0, [0; 3], 1.0));
        assert!(!f.set(0.0, f64::INFINITY, [0; 3], 1.0));
        assert!(!f.set(0.0, 10.0, [0; 3], f64::NAN));
        assert_eq!(f.get(), None);
        assert!(f.set(-50.0, 99999.0, [255; 3], 7.0));
        let b = f.get().unwrap();
        assert_eq!((b.start, b.end, b.mix), (0.0, MAX_DISTANCE + 1.0, 1.0));
        // An end before the start is pushed a block past it.
        assert!(f.set(100.0, 10.0, [0; 3], 0.5));
        let b = f.get().unwrap();
        assert_eq!((b.start, b.end), (100.0, 101.0));
        // A density of nothing gives the fog back at once.
        assert!(f.set(1.0, 5.0, [0; 3], 0.0));
        assert_eq!(f.get(), None);
    }

    #[test]
    fn the_blend_mixes_colour_and_range_by_its_density() {
        let b = Blend { start: 0.0, end: 20.0, rgb: [0.0, 0.0, 0.0], mix: 0.5 };
        assert_eq!(b.colour([0.8, 0.6, 0.4]), [0.4, 0.3, 0.2]);
        assert_eq!(b.range(40.0, 100.0), (20.0, 60.0));
        let full = Blend { mix: 1.0, ..b };
        assert_eq!(full.colour([1.0, 1.0, 1.0]), [0.0, 0.0, 0.0]);
        assert_eq!(full.range(40.0, 100.0), (0.0, 20.0));
        let none = Blend { mix: 0.0, ..b };
        assert_eq!(none.range(40.0, 100.0), (40.0, 100.0));
        assert_eq!(none.colour([0.2, 0.3, 0.4]), [0.2, 0.3, 0.4]);
        // Never an end at or before the start.
        let odd = Blend { start: 10.0, end: 10.5, rgb: [0.0; 3], mix: 1.0 };
        let (s, e) = odd.range(0.0, 0.1);
        assert!(e > s);
    }
}
