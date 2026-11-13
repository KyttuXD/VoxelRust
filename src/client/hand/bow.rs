//! A bow in the first-person hand: the picture changes with the draw (see
//! `game::bow`), and the pose comes upright while the arm pulls back (the
//! arm's motion is `swing::draw_motion`).

use super::Held;

/// The picture a bow is shown with when it is drawn `draw` (0..1) of the way:
/// one of its three pulling pictures, or `None` (at rest).
pub(super) fn drawn_picture(held: Held, draw: f32) -> Option<u16> {
    let Held::Bow(base) = held else { return None };
    crate::game::bow::pull_frame(draw).map(|f| crate::game::bow::pulling_tile(base, f))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bow_at_rest_keeps_its_picture_and_other_things_never_change() {
        assert_eq!(drawn_picture(Held::Bow(3), 0.0), None);
        assert_eq!(drawn_picture(Held::Tool(3), 0.8), None);
    }
}
