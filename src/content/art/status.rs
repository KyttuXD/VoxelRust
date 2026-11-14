//! The icons of the newer status effects and the bubbles of the air meter,
//! in the OCD style (flat fills, a dark outline, no grain). The older
//! effects' icons and the hearts are in `hud.rs`, which this file borrows
//! the badge from.
//!
//! Paint styles (the texture ids are the same):
//! - `effect_decay`, `effect_haste`, `effect_mining_fatigue`,
//!   `effect_levitation`, `effect_blindness`, `effect_water_breathing`:
//!   16 x 16 badges with a light picture on them.
//! - `hud_bubble`: an air bubble, 9 x 9 in the middle of its tile.
//! - `hud_bubble_pop`: the same, burst (drawn blinking when the air is out).

use super::hud::{badge, Glyph, CLEAR, OUTLINE};

type Px = [u8; 4];

const BROKEN_HEART: Glyph = [".##..##.", "####.###", "###.####", "####.###", ".###.##.", "..##.#..", "...#.#..", "....#..."];
const PICK: Glyph = [".######.", "##....##", "#..##...", "...##...", "...##...", "...##...", "...##...", "...##..."];
const PICK_BROKEN: Glyph = [".######.", "##....##", "#..##...", "...##...", "........", "...##...", "...##...", "........"];
const LIFT: Glyph = ["...##...", "..####..", ".##..##.", "........", "...##...", "..####..", ".##..##.", "........"];
const EYE_SHUT: Glyph = ["........", ".######.", "##....##", "........", "#.#..#.#", "..#..#..", "........", "........"];
const BUBBLES: Glyph = ["...##...", "..####..", "..####..", "...##...", ".##...#.", "####.###", "####..#.", ".##....."];

/// The badge colour and glyph of an effect's icon.
fn icon_of(name: &str) -> Option<([u8; 3], Glyph)> {
    Some(match name {
        "decay" => ([78, 64, 50], BROKEN_HEART),
        "haste" => ([226, 170, 44], PICK),
        "mining_fatigue" => ([112, 104, 92], PICK_BROKEN),
        "levitation" => ([160, 134, 214], LIFT),
        "blindness" => ([58, 58, 66], EYE_SHUT),
        "water_breathing" => ([48, 116, 210], BUBBLES),
        _ => return None,
    })
}

/// The air bubble: `O` outline, `B` body, `H` shine.
const BUBBLE: [&str; 9] = ["..OOOOO..", ".OBBBBBO.", "OBHHBBBBO", "OBHBBBBBO", "OBBBBBBBO", "OBBBBBBBO", "OBBBBBBBO", ".OBBBBBO.", "..OOOOO.."];
/// The burst bubble: `O` outline pieces, `H` the spray.
const BUBBLE_POP: [&str; 9] = ["O...O...O", ".O..O..O.", "..O...O..", "....H....", "OO.HHH.OO", "....H....", "..O...O..", ".O..O..O.", "O...O...O"];
/// Where the bubble sits in its tile.
const BUBBLE_AT: i32 = 3;

fn bubble(rows: &[&str; 9], x: i32, y: i32) -> Px {
    let (bx, by) = (x - BUBBLE_AT, y - BUBBLE_AT);
    if !(0..9).contains(&bx) || !(0..9).contains(&by) {
        return CLEAR;
    }
    match rows[by as usize].as_bytes()[bx as usize] {
        b'O' => OUTLINE,
        b'B' => [96, 170, 236, 255],
        b'H' => [214, 238, 255, 255],
        _ => CLEAR,
    }
}

/// One pixel of the picture in the style `style`; `None` if it is not one of
/// this file's.
pub fn painted(style: &str, x: i32, y: i32) -> Option<Px> {
    match style {
        "hud_bubble" => Some(bubble(&BUBBLE, x, y)),
        "hud_bubble_pop" => Some(bubble(&BUBBLE_POP, x, y)),
        _ => {
            let (rgb, glyph) = icon_of(style.strip_prefix("effect_")?)?;
            Some(badge(rgb, &glyph, x, y))
        }
    }
}

/// Every style, for tests and for the generator's checks.
pub const STYLES: &[&str] = &["effect_decay", "effect_haste", "effect_mining_fatigue", "effect_levitation", "effect_blindness", "effect_water_breathing", "hud_bubble", "hud_bubble_pop"];

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
    fn every_picture_is_a_few_flat_colours_with_a_dark_outline() {
        for s in STYLES {
            let c = colours(s);
            assert!(c.len() >= 2 && c.len() <= 8, "{s}: {} colours", c.len());
            assert!(c.contains(&OUTLINE), "{s} has no dark outline");
            assert!(picture(s).iter().filter(|p| p[3] > 0).count() > 20, "{s} is nearly empty");
            assert!(picture(s).iter().all(|p| p[3] == 0 || p[3] == 255), "{s} has soft edges");
            assert_eq!(picture(s), picture(s), "{s} is not steady");
        }
    }

    #[test]
    fn the_new_icons_differ_from_each_other_and_from_the_old_ones() {
        let new: Vec<Vec<Px>> = STYLES.iter().map(|s| picture(s)).collect();
        for i in 0..new.len() {
            for j in i + 1..new.len() {
                assert_ne!(new[i], new[j], "{} and {} are the same picture", STYLES[i], STYLES[j]);
            }
        }
        for old in crate::content::art::hud::STYLES.iter().filter(|s| s.starts_with("effect_")) {
            let o: Vec<Px> = (0..256).map(|i| crate::content::art::hud::painted(old, i % 16, i / 16).unwrap()).collect();
            assert!(!new.contains(&o), "{old} is drawn twice");
        }
        // Icons keep a clear corner.
        assert_eq!(painted("effect_haste", 0, 0), Some(CLEAR));
        assert_eq!(painted("effect_haste", 15, 15), Some(CLEAR));
    }

    #[test]
    fn the_bubble_is_blue_and_nine_by_nine_in_the_middle() {
        let p = picture("hud_bubble");
        let at = |x: i32, y: i32| p[(y * 16 + x) as usize];
        assert_eq!(at(3 + 4, 3 + 4), [96, 170, 236, 255]);
        assert_eq!(at(3 + 2, 3), OUTLINE);
        assert_eq!(at(2, 7), CLEAR);
        assert_eq!(at(12, 7), CLEAR);
        // The burst one has the same extent but is mostly empty.
        let q = picture("hud_bubble_pop");
        assert!(q.iter().filter(|p| p[3] > 0).count() < p.iter().filter(|p| p[3] > 0).count());
    }

    #[test]
    fn only_our_own_styles_are_drawn() {
        for s in ["effect_poison", "effect_", "effect_nonsense", "hud_bubbles", "heart_full", "wool"] {
            assert!(painted(s, 5, 5).is_none(), "{s}");
        }
    }
}
