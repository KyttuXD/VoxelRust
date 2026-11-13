//! Bows and crossbows, drawn in the OCD style: flat fills, a dark outline,
//! no grain. A bow is shown at rest (string taut) and in three stages of
//! being drawn, with an arrow nocked: the pictures a bow in the hand and in
//! the hotbar changes between (see `game::bow`).
//!
//! Paint styles: `bow`, `crossbow` (at rest) and `bow_pull0..2`,
//! `crossbow_pull0..2` (drawn back a little, half way, fully).
//!
//! The picture is diagonal like the game's other tools: the stave bulges to
//! the top right, the string is pulled to the bottom left.

type Px = [u8; 4];

const CLEAR: Px = [0, 0, 0, 0];
const OUTLINE: Px = [26, 26, 26, 255];
const STRING: Px = [236, 236, 230, 255];
const SHAFT: Px = [150, 112, 70, 255];
const HEAD: Px = [190, 192, 200, 255];
const FEATHER: Px = [236, 236, 240, 255];

/// What is at a pixel of the picture, before the outline.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    None,
    Stave,
    Stock,
    String,
    Shaft,
    Head,
    Feather,
}

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

/// How far (in pixels along the diagonal) the nock is pulled back from the
/// string's middle at each stage.
fn pull_of(stage: Option<i32>) -> i32 {
    match stage {
        None => 0,
        Some(k) => 2 + 2 * k.clamp(0, 2),
    }
}

fn abs(v: i32) -> i32 {
    if v < 0 { -v } else { v }
}

/// Whether the pixel is on the straight line from `a` to `b` (about one pixel thick).
fn on_line(p: (i32, i32), a: (i32, i32), b: (i32, i32)) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    // Within the segment's span.
    let (lo_x, hi_x) = (a.0.min(b.0), a.0.max(b.0));
    let (lo_y, hi_y) = (a.1.min(b.1), a.1.max(b.1));
    if p.0 < lo_x || p.0 > hi_x || p.1 < lo_y || p.1 > hi_y {
        return false;
    }
    // The cross product is twice the triangle's area: small means near the line.
    let cross = (p.0 - a.0) * dy - (p.1 - a.1) * dx;
    let long = abs(dx).max(abs(dy)).max(1);
    abs(cross) * 2 <= long
}

fn part(stage: Option<i32>, crossbow: bool, x: i32, y: i32) -> Part {
    if !(0..16).contains(&x) || !(0..16).contains(&y) {
        return Part::None;
    }
    // The ends of the stave and the middle of the string at rest.
    let (a, b) = ((3, 2), (13, 12));
    let pull = pull_of(stage);
    let nock = (8 - pull, 7 + pull);

    // The arrow (a bolt in a crossbow) lies along x + y == 15, nock to point.
    if stage.is_some() {
        let on_shaft = x + y == 15 && x >= nock.0 && x <= 14;
        let head = (x + y == 15 && x >= 13) || [(12, 2), (13, 1), (14, 2), (13, 3)].contains(&(x, y));
        if head {
            return Part::Head;
        }
        if on_shaft {
            return if x <= nock.0 + 1 { Part::Feather } else { Part::Shaft };
        }
        // The feathers' vanes, either side of the shaft at the nock.
        if (x + y == 14 || x + y == 16) && x >= nock.0 && x <= nock.0 + 1 {
            return Part::Feather;
        }
    }
    // The string: from one end of the stave to the nock and on to the other.
    if on_line((x, y), a, nock) || on_line((x, y), nock, b) {
        return Part::String;
    }
    // The stave, an arc round (3, 12).
    let (dx, dy) = (x - 3, y - 12);
    let d2 = dx * dx + dy * dy;
    if (100..=130).contains(&d2) && x >= 3 && y <= 12 {
        return Part::Stave;
    }
    // A crossbow's stock runs back from the stave along the line of the bolt.
    if crossbow && (x + y == 14 || x + y == 16) && (3..=10).contains(&x) {
        return Part::Stock;
    }
    if crossbow && (2..=4).contains(&x) && (13..=14).contains(&y) && (15..=17).contains(&(x + y)) {
        return Part::Stock;
    }
    Part::None
}

/// One pixel of the picture in the style `style` (see the top of this file),
/// in the colour `rgb` for the wood; `None` if it is not one of ours.
pub fn painted(style: &str, rgb: [u8; 3], x: i32, y: i32) -> Option<Px> {
    let (stage, crossbow) = stage_of(style)?;
    let wood = [rgb[0], rgb[1], rgb[2], 255];
    let dark = [
        (rgb[0] as u32 * 62 / 100) as u8,
        (rgb[1] as u32 * 62 / 100) as u8,
        (rgb[2] as u32 * 62 / 100) as u8,
        255,
    ];
    let here = part(stage, crossbow, x, y);
    Some(match here {
        Part::Stave => wood,
        Part::Stock => dark,
        Part::String => STRING,
        Part::Shaft => SHAFT,
        Part::Head => HEAD,
        Part::Feather => FEATHER,
        Part::None => {
            // The dark outline: a pixel beside the picture's own.
            let beside = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)];
            if beside.iter().any(|&(bx, by)| part(stage, crossbow, bx, by) != Part::None) {
                OUTLINE
            } else {
                CLEAR
            }
        }
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
