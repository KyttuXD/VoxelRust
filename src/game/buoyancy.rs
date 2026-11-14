//! Floating: the up and down of a body in water or lava.
//!
//! A body is held up by the liquid in proportion to how much of it is under
//! the surface. Water carries a swimmer just enough that it floats with
//! about four fifths of the body under (the head out), and rises toward
//! that from below and sinks back to it from above, so it bobs a little
//! when it breaks the surface. Lava is thicker: it holds up more and drags
//! harder. Holding the descend key dives; holding jump swims up.

/// What pulls a body down in a liquid, blocks a second squared.
const WEIGHT: f32 = 7.0;
/// What holds a fully submerged body up in water, and in lava.
const LIFT_WATER: f32 = 8.75;
const LIFT_LAVA: f32 = 11.5;
/// Slowing, per second, of the vertical speed.
const DRAG_WATER: f32 = 2.5;
const DRAG_LAVA: f32 = 4.5;
/// Extra downward push while the descend key is held.
const DIVE: f32 = 14.0;
/// Swimming up: the push and the fastest rise it makes.
const STROKE: f32 = 22.0;
const FASTEST_RISE: f32 = 3.6;

/// The vertical speed after `dt` seconds in a liquid, for a body that is
/// `submerged` (0..1) under its surface.
pub fn vertical(vy: f32, dt: f32, submerged: f32, lava: bool, jump: bool, descend: bool) -> f32 {
    let f = if submerged < 0.0 {
        0.0
    } else if submerged > 1.0 {
        1.0
    } else {
        submerged
    };
    let (lift, drag) = if lava { (LIFT_LAVA, DRAG_LAVA) } else { (LIFT_WATER, DRAG_WATER) };
    let mut v = vy + (lift * f - WEIGHT) * dt;
    if descend {
        v -= DIVE * dt;
    }
    v *= 1.0 - (drag * dt).min(1.0);
    if jump {
        v = (v + STROKE * dt).min(FASTEST_RISE);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(mut v: f32, mut y: f32, surface: f32, seconds: f32, jump: bool, descend: bool) -> (f32, f32) {
        for _ in 0..(seconds * 60.0) as i32 {
            let f = ((surface - y) / 1.8).clamp(0.0, 1.0);
            v = vertical(v, 1.0 / 60.0, f, false, jump, descend);
            y += v / 60.0;
        }
        (y, v)
    }

    #[test]
    fn a_body_settles_with_its_head_out() {
        // From a few blocks down in a pool, with the surface at 20.
        let (y, v) = run(0.0, 16.0, 20.0, 12.0, false, false);
        let under = (20.0 - y) / 1.8;
        assert!((0.7..0.9).contains(&under), "under = {under}");
        assert!(v.abs() < 0.05, "still moving at {v}");
    }

    #[test]
    fn it_bobs_when_it_breaks_the_surface() {
        // Thrown up through the surface it comes back down: not a rest.
        let (_, v) = run(3.0, 18.0, 20.0, 0.3, false, false);
        let (y2, _) = run(v, 21.5, 20.0, 1.5, false, false);
        assert!(y2 < 21.5, "it did not come back");
    }

    #[test]
    fn descending_dives_and_jumping_swims_up() {
        let (y, _) = run(0.0, 18.0, 20.0, 2.0, false, true);
        assert!(y < 17.0, "y = {y}");
        let (y, v) = run(0.0, 10.0, 20.0, 1.0, true, false);
        assert!(y > 12.0 && v <= FASTEST_RISE + 1e-4);
    }

    #[test]
    fn lava_holds_up_more_and_drags_more() {
        let (a, b) = (vertical(0.0, 0.1, 1.0, false, false, false), vertical(0.0, 0.1, 1.0, true, false, false));
        assert!(b > a);
        // Out of the liquid altogether, a body just sinks.
        assert!(vertical(0.0, 0.1, 0.0, false, false, false) < 0.0);
    }
}
