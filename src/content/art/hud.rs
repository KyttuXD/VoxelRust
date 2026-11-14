//! The hearts and effect icons of the HUD, drawn in the OCD style: flat
//! fills, a dark outline, no grain.
//!
//! Paint styles (the texture ids are the same):
//! - hearts, 9 x 8 pixels in the middle of the tile: `heart_empty`,
//!   `heart_full`, `heart_half`, and `heart_<look>_full` / `heart_<look>_half`
//!   for the looks `poison` (green), `wither` (dark), `frozen` (blue),
//!   `absorb` (yellow) and `hardcore` (deep red). A half heart is lit on its
//!   left and empty on its right, so it is drawn alone.
//! - icons of effects, 16 x 16: a coloured badge with a light picture on it:
//!   `effect_poison`, `effect_burning`, `effect_regeneration`,
//!   `effect_slowness`, `effect_speed`, `effect_hunger`, `effect_weakness`,
//!   `effect_strength`, `effect_wither`, `effect_absorption`, `effect_frozen`,
//!   `effect_saturation`, `effect_night_vision`, `effect_jump_boost`; and
//!   `effect_flame`, the flame shown beside the hearts of someone on fire.

type Px = [u8; 4];

const CLEAR: Px = [0, 0, 0, 0];
const OUTLINE: Px = [24, 22, 34, 255];
const GLYPH: Px = [246, 246, 240, 255];

/// The heart: `O` outline, `F` fill, `H` highlight. 9 wide, 8 tall.
const HEART: [&str; 8] = [".OOO.OOO.", "OHFFOFFFO", "OHFFFFFFO", "OFFFFFFFO", ".OFFFFFO.", "..OFFFO..", "...OFO...", "....O...."];
/// Where the heart sits in the tile.
const HEART_AT: (i32, i32) = (3, 4);

struct Look {
    fill: Px,
    light: Px,
}

fn look_of(name: &str) -> Option<Look> {
    let p = |fill: [u8; 3], light: [u8; 3]| Look { fill: [fill[0], fill[1], fill[2], 255], light: [light[0], light[1], light[2], 255] };
    Some(match name {
        "" => p([226, 44, 52], [255, 150, 150]),
        "poison" => p([96, 170, 46], [172, 226, 110]),
        "wither" => p([62, 62, 70], [120, 120, 132]),
        "frozen" => p([108, 198, 236], [226, 248, 255]),
        "absorb" => p([240, 190, 40], [255, 236, 142]),
        "hardcore" => p([170, 20, 44], [255, 120, 140]),
        _ => return None,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fill {
    Empty,
    Full,
    Half,
}

fn heart_px(look: &Look, fill: Fill, x: i32, y: i32) -> Px {
    let (hx, hy) = (x - HEART_AT.0, y - HEART_AT.1);
    if !(0..9).contains(&hx) || !(0..8).contains(&hy) {
        return CLEAR;
    }
    let lit = match fill {
        Fill::Empty => false,
        Fill::Full => true,
        // The left half of the heart, with its middle.
        Fill::Half => hx <= 4,
    };
    match HEART[hy as usize].as_bytes()[hx as usize] {
        b'O' => OUTLINE,
        b'F' => {
            if lit {
                look.fill
            } else {
                [74, 44, 48, 255]
            }
        }
        b'H' => {
            if lit {
                look.light
            } else {
                [74, 44, 48, 255]
            }
        }
        _ => CLEAR,
    }
}

/// A heart style: `heart_full`, `heart_poison_half`, `heart_empty`...
fn heart(style: &str, x: i32, y: i32) -> Option<Px> {
    let rest = style.strip_prefix("heart_")?;
    let (look, kind) = match rest.rsplit_once('_') {
        Some((l, k)) => (l, k),
        None => ("", rest),
    };
    let fill = match kind {
        "empty" => Fill::Empty,
        "full" => Fill::Full,
        "half" => Fill::Half,
        _ => return None,
    };
    // Only the plain heart has an empty one.
    if fill == Fill::Empty && !look.is_empty() {
        return None;
    }
    Some(heart_px(&look_of(look)?, fill, x, y))
}

// ------------------------------------------------------------------- icons

/// An 8 x 8 picture, `#` where the light glyph is. Holes show the badge.
type Glyph = [&'static str; 8];

const DROP: Glyph = ["...##...", "...##...", "..####..", "..####..", ".######.", ".######.", ".######.", "..####.."];
const CROSS: Glyph = ["...##...", "...##...", "...##...", "########", "########", "...##...", "...##...", "...##..."];
const DOWN: Glyph = ["...##...", "...##...", "...##...", "...##...", "########", ".######.", "..####..", "...##..."];
const UP: Glyph = ["...##...", "..####..", ".######.", "########", "...##...", "...##...", "...##...", "...##..."];
const CHEVRONS: Glyph = [".#..#...", "..#..#..", "...#..#.", "....#..#", "....#..#", "...#..#.", "..#..#..", ".#..#..."];
const DRUMSTICK: Glyph = [".####...", "######..", "######..", ".####...", "..###...", "...###..", "....###.", ".....##."];
const SWORD: Glyph = ["......##", ".....###", "....###.", ".#.###..", "..###...", "..##....", ".#.#....", "#......."];
const SWORD_CRACKED: Glyph = ["......##", ".....##.", "........", ".#..#...", "..###...", "..##....", ".#.#....", "#......."];
const SKULL: Glyph = [".######.", "########", "##.##.##", "##.##.##", "########", ".######.", "..#..#..", "..####.."];
const SHIELD: Glyph = ["########", "########", "########", "########", ".######.", ".######.", "..####..", "...##..."];
const SNOWFLAKE: Glyph = ["...##...", "#..##..#", ".#.##.#.", "..####..", "########", "..####..", ".#.##.#.", "#..##..#"];
const APPLE: Glyph = [".....#..", "....#...", ".##.##..", "########", "########", "########", ".######.", "..#..#.."];
const EYE: Glyph = ["........", "..####..", ".######.", "###..###", "###..###", ".######.", "..####..", "........"];
const FLAME_SMALL: Glyph = ["....#...", "...##...", "..###.#.", ".######.", ".######.", ".######.", "..####..", "...##..."];

/// The badge colour and glyph of an effect's icon.
fn icon_of(name: &str) -> Option<([u8; 3], Glyph)> {
    Some(match name {
        "poison" => ([88, 150, 44], DROP),
        "burning" => ([220, 104, 28], FLAME_SMALL),
        "regeneration" => ([212, 66, 98], CROSS),
        "slowness" => ([96, 116, 150], DOWN),
        "speed" => ([60, 172, 214], CHEVRONS),
        "hunger" => ([138, 116, 58], DRUMSTICK),
        "weakness" => ([124, 124, 136], SWORD_CRACKED),
        "strength" => ([156, 36, 36], SWORD),
        "wither" => ([46, 46, 54], SKULL),
        "absorption" => ([214, 164, 30], SHIELD),
        "frozen" => ([104, 184, 226], SNOWFLAKE),
        "saturation" => ([214, 124, 56], APPLE),
        "night_vision" => ([40, 40, 118], EYE),
        "jump_boost" => ([92, 188, 96], UP),
        _ => return None,
    })
}

/// Inside the badge: the 12 x 12 square from (2, 2).
fn in_badge(x: i32, y: i32) -> bool {
    (2..=13).contains(&x) && (2..=13).contains(&y)
}

/// The ring round it, without its corners.
fn on_ring(x: i32, y: i32) -> bool {
    if !(1..=14).contains(&x) || !(1..=14).contains(&y) || in_badge(x, y) {
        return false;
    }
    let corner = (x == 1 || x == 14) && (y == 1 || y == 14);
    !corner
}

fn icon(name: &str, x: i32, y: i32) -> Option<Px> {
    let (rgb, glyph) = icon_of(name)?;
    if on_ring(x, y) {
        return Some(OUTLINE);
    }
    if !in_badge(x, y) {
        return Some(CLEAR);
    }
    // The glyph, 8 x 8, in the middle.
    let (gx, gy) = (x - 4, y - 4);
    if (0..8).contains(&gx) && (0..8).contains(&gy) && glyph[gy as usize].as_bytes()[gx as usize] == b'#' {
        return Some(GLYPH);
    }
    Some([rgb[0], rgb[1], rgb[2], 255])
}

/// The flame beside the hearts: the rows of it, as the span of pixels.
const FLAME: [(i32, i32); 10] = [(4, 4), (4, 5), (3, 5), (3, 6), (2, 6), (2, 7), (1, 7), (1, 7), (2, 6), (3, 5)];
const FLAME_AT: (i32, i32) = (4, 3);

fn flame_part(x: i32, y: i32) -> u8 {
    let (fx, fy) = (x - FLAME_AT.0, y - FLAME_AT.1);
    if !(0..10).contains(&fy) {
        return 0;
    }
    let (lo, hi) = FLAME[fy as usize];
    if fx < lo || fx > hi {
        return 0;
    }
    // The hot core: yellow, low in the middle.
    if (6..=8).contains(&fy) && (3..=5).contains(&fx) {
        2
    } else {
        1
    }
}

fn flame(x: i32, y: i32) -> Px {
    match flame_part(x, y) {
        2 => [255, 214, 70, 255],
        1 => [238, 116, 24, 255],
        _ => {
            let beside = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)];
            if beside.iter().any(|&(bx, by)| flame_part(bx, by) != 0) {
                OUTLINE
            } else {
                CLEAR
            }
        }
    }
}

/// One pixel of the picture in the style `style` (see the top of this
/// file); `None` if it is not one of ours. The colour a data line gives is
/// not used: these pictures have their own.
pub fn painted(style: &str, x: i32, y: i32) -> Option<Px> {
    if let Some(rest) = style.strip_prefix("effect_") {
        if rest == "flame" {
            return Some(flame(x, y));
        }
        return icon(rest, x, y);
    }
    heart(style, x, y)
}

/// Whether `style` is one of this file's.
pub fn knows(style: &str) -> bool {
    painted(style, 0, 0).is_some()
}

/// Every style, for tests and for the generator's checks.
pub const STYLES: &[&str] = &[
    "heart_empty",
    "heart_full",
    "heart_half",
    "heart_poison_full",
    "heart_poison_half",
    "heart_wither_full",
    "heart_wither_half",
    "heart_frozen_full",
    "heart_frozen_half",
    "heart_absorb_full",
    "heart_absorb_half",
    "heart_hardcore_full",
    "heart_hardcore_half",
    "effect_poison",
    "effect_burning",
    "effect_regeneration",
    "effect_slowness",
    "effect_speed",
    "effect_hunger",
    "effect_weakness",
    "effect_strength",
    "effect_wither",
    "effect_absorption",
    "effect_frozen",
    "effect_saturation",
    "effect_night_vision",
    "effect_jump_boost",
    "effect_flame",
];

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn picture(style: &str) -> Vec<Px> {
        (0..256).map(|i| painted(style, i % 16, i / 16).unwrap()).collect()
    }

    fn colours(style: &str) -> Vec<Px> {
        let mut seen: Vec<Px> = Vec::new();
        for p in picture(style) {
            if p[3] > 0 && !seen.contains(&p) {
                seen.push(p);
            }
        }
        seen
    }

    #[test]
    fn every_style_draws_a_picture_in_few_flat_colours_with_a_dark_outline() {
        for s in STYLES {
            let c = colours(s);
            assert!(c.len() >= 2 && c.len() <= 8, "{s}: {} colours", c.len());
            assert!(c.contains(&OUTLINE), "{s} has no dark outline");
            assert!(picture(s).iter().filter(|p| p[3] > 0).count() > 20, "{s} is nearly empty");
            // Pixels are fully there or not at all.
            assert!(picture(s).iter().all(|p| p[3] == 0 || p[3] == 255), "{s} has soft edges");
            assert!(knows(s));
        }
    }

    #[test]
    fn only_our_own_styles_are_drawn() {
        for s in ["wool", "heart", "heart_", "heart_purple_full", "heart_poison_empty", "heart_full_full", "effect_", "effect_nonsense", "bow"] {
            assert!(painted(s, 5, 5).is_none(), "{s}");
        }
    }

    #[test]
    fn hearts_are_red_green_dark_blue_and_yellow() {
        let fill = |s: &str| picture(s)[(7 * 16 + 7) as usize];
        assert_eq!(fill("heart_full"), [226, 44, 52, 255]);
        assert!(fill("heart_poison_full")[1] > fill("heart_poison_full")[0], "poison is green");
        let w = fill("heart_wither_full");
        assert!(w[0] < 90 && w[1] < 90 && w[2] < 90, "wither is dark");
        let f = fill("heart_frozen_full");
        assert!(f[2] > f[0], "frozen is blue");
        let a = fill("heart_absorb_full");
        assert!(a[0] > 200 && a[1] > 150 && a[2] < 100, "absorption is yellow");
    }

    #[test]
    fn a_half_heart_is_lit_on_the_left_only_and_an_empty_one_not_at_all() {
        let full = picture("heart_full");
        let half = picture("heart_half");
        let empty = picture("heart_empty");
        let at = |p: &Vec<Px>, x: i32, y: i32| p[(y * 16 + x) as usize];
        // Row 6 of the picture is the heart's row 2: left of the middle and right of it.
        assert_eq!(at(&half, 5, 6), at(&full, 5, 6));
        assert_ne!(at(&half, 9, 6), at(&full, 9, 6));
        assert_eq!(at(&half, 9, 6), at(&empty, 9, 6));
        assert_ne!(at(&empty, 5, 6), at(&full, 5, 6));
        // They all have the same outline.
        let shape = |p: &Vec<Px>| p.iter().map(|px| px[3]).collect::<Vec<u8>>();
        assert_eq!(shape(&full), shape(&half));
        assert_eq!(shape(&full), shape(&empty));
    }

    #[test]
    fn effect_icons_are_all_different() {
        let icons: Vec<Vec<Px>> = STYLES.iter().filter(|s| s.starts_with("effect_")).map(|s| picture(s)).collect();
        for i in 0..icons.len() {
            for j in i + 1..icons.len() {
                assert_ne!(icons[i], icons[j], "{} and {} are the same picture", i, j);
            }
        }
        // Icons stay inside their tile with a clear corner.
        assert_eq!(painted("effect_poison", 0, 0), Some(CLEAR));
        assert_eq!(painted("effect_poison", 15, 15), Some(CLEAR));
    }

    #[test]
    fn the_same_picture_twice_has_no_noise() {
        for s in STYLES {
            assert_eq!(picture(s), picture(s), "{s}");
        }
    }
}
