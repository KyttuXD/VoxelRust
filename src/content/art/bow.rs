//! Bows and crossbows, drawn in the OCD style: flat fills, a dark outline,
//! no grain. A bow is shown at rest (string taut) and in three stages of
//! being drawn, with an arrow nocked: the pictures a bow in the hand and in
//! the hotbar changes between (see `game::bow`).
//!
//! Paint styles: `bow`, `crossbow` (at rest) and `bow_pull0..2`,
//! `crossbow_pull0..2` (drawn back a little, half way, fully).
//!
//! The pictures stand upright (the OCD style), in VoxelRust's own shapes.

type Px = [u8; 4];

const CLEAR: Px = [0, 0, 0, 0];
const OUTLINE: Px = [24, 22, 34, 255];
/// The lighter outline on the lit (upper and left) edges: the house rim light.
const RIM: Px = [60, 54, 80, 255];
const STRING: Px = [236, 236, 230, 255];
const SHAFT: Px = [150, 112, 70, 255];
const HEAD: Px = [190, 192, 200, 255];

/// Which way the picture of a style is drawn: `None` is at rest, `Some(k)`
/// the stage of drawing (0..=2). The second is whether it is a crossbow.
fn stage_of(style: &str) -> Option<(Option<i32>, bool)> {
    Some(match style {
        "bow" => (None, false),
        "crossbow" => (None, true),
        "bow_pull0" => (Some(0), false),
        "bow_pull1" => (Some(1), false),
        "bow_pull2" => (Some(2), false),
        "crossbow_pull0" => (Some(0), true),
        "crossbow_pull1" => (Some(1), true),
        "crossbow_pull2" => (Some(2), true),
        _ => return None,
    })
}

/// One pixel of the picture in the style `style` (see the top of this file),
/// in the colour `rgb` for the wood; `None` if it is not one of ours.
pub fn painted(style: &str, rgb: [u8; 3], x: i32, y: i32) -> Option<Px> {
    stage_of(style)?;
    // The picture is VoxelRust's own silhouette (painters/tool_shapes.rs): the
    // stave and stock in the wood colour, the string and arrow parts pale.
    let rows = crate::painters::tool_shapes::shape(style)?;
    if !(0..16).contains(&x) || !(0..16).contains(&y) {
        return Some(CLEAR);
    }
    let wood = [rgb[0], rgb[1], rgb[2], 255];
    let dark = [(rgb[0] as u32 * 62 / 100) as u8, (rgb[1] as u32 * 62 / 100) as u8, (rgb[2] as u32 * 62 / 100) as u8, 255];
    Some(match rows[y as usize].as_bytes()[x as usize] {
        b'o' => {
            // Rim light: an outline pixel with the picture on its right or below and none on its left and above.
            let solid = |dx: i32, dy: i32| {
                let (cx, cy) = (x + dx, y + dy);
                (0..16).contains(&cx) && (0..16).contains(&cy) && rows[cy as usize].as_bytes()[cx as usize] != b'.'
            };
            if (solid(1, 0) || solid(0, 1)) && !solid(-1, 0) && !solid(0, -1) {
                RIM
            } else {
                OUTLINE
            }
        }
        b'h' => wood,
        b'H' => dark,
        b'L' | b'W' => STRING,
        b'M' => HEAD,
        b'D' | b'B' => SHAFT,
        _ => CLEAR,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(style: &str) -> usize {
        (0..16).flat_map(|y| (0..16).map(move |x| (x, y))).filter(|&(x, y)| painted(style, [150, 110, 70], x, y).unwrap()[3] > 0).count()
    }

    #[test]
    fn every_style_draws_something_and_only_ours_are_drawn() {
        for s in ["bow", "crossbow", "bow_pull0", "bow_pull1", "bow_pull2", "crossbow_pull0", "crossbow_pull1", "crossbow_pull2"] {
            assert!(count(s) > 20, "{s} draws a picture");
        }
        assert!(painted("wool", [1, 2, 3], 4, 4).is_none());
    }

    #[test]
    fn a_drawn_bow_differs_from_the_one_at_rest_and_each_stage_from_the_last() {
        let pic = |s: &str| -> alloc::vec::Vec<[u8; 4]> { (0..256).map(|i| painted(s, [150, 110, 70], i % 16, i / 16).unwrap()).collect() };
        assert_ne!(pic("bow"), pic("bow_pull0"));
        assert_ne!(pic("bow_pull0"), pic("bow_pull1"));
        assert_ne!(pic("bow_pull1"), pic("bow_pull2"));
        assert_ne!(pic("crossbow_pull0"), pic("crossbow_pull2"));
    }

    #[test]
    fn only_flat_colours_and_a_dark_outline_are_used() {
        let mut colours = alloc::vec::Vec::new();
        for y in 0..16 {
            for x in 0..16 {
                let p = painted("bow_pull2", [150, 110, 70], x, y).unwrap();
                if p[3] > 0 && !colours.contains(&p) {
                    colours.push(p);
                }
            }
        }
        assert!(colours.len() <= 8, "{colours:?}");
        assert!(colours.contains(&OUTLINE));
    }
}
