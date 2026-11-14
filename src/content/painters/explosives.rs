//! TNT, gunpowder and the flint and steel, in the OCD style: a red block
//! with a white label band and a dark frame, a heap of grey dust, and a
//! steel striker with a chip of flint on an upright handle.

use super::*;

/// The label band of the TNT's sides and the lettering on it.
const LABEL: [u8; 3] = [226, 224, 216];
const INK: [u8; 3] = [64, 44, 40];
/// The chip of flint, and its lit corner.
const FLINT: [u8; 3] = [70, 74, 88];
const FLINT_LIT: [u8; 3] = [132, 138, 154];
/// The striker's handle.
const HANDLE: [u8; 3] = [118, 84, 52];

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    let (l, m, d) = (pal.light_of(rgb), rgb, pal.dark_of(rgb));
    Some(match style {
        // The side: red, with a white band across the middle (rows 5 to 10)
        // and dark marks on it for lettering. The frame stays unbroken.
        "tnt_side" => {
            if edge(x, y) {
                px(d)
            } else if rect(x, y, 1, 5, 14, 6) {
                let mark = (y == 7 || y == 8) && (3..13).contains(&x) && (x % 4 == 2 || x % 4 == 3);
                px(if mark { INK } else { LABEL })
            } else {
                px(m)
            }
        }
        // The top: red, with the stick of the fuse in the middle.
        "tnt_top" => {
            if edge(x, y) {
                px(d)
            } else if rect(x, y, 7, 7, 2, 2) {
                px(INK)
            } else if rect(x, y, 6, 6, 4, 4) {
                px(d)
            } else {
                px(m)
            }
        }
        // The bottom: red, with a darker cross.
        "tnt_bottom" => {
            if edge(x, y) || (7..=8).contains(&x) || (7..=8).contains(&y) {
                px(d)
            } else {
                px(m)
            }
        }
        // A heap of dust, lit from the upper left.
        "powder" => outlined(x, y, &|x, y| {
            if !(5..=12).contains(&y) {
                return None;
            }
            let half = (y - 4).min(6) * 2 - 1;
            if (2 * x - 15).abs() > half {
                return None;
            }
            Some(if y >= 11 {
                d
            } else if x + y <= 12 {
                l
            } else {
                m
            })
        }),
        // Upright: a chip of flint on top, a steel bar across, a handle below.
        "flint_steel" => outlined(x, y, &|x, y| {
            if rect(x, y, 6, 1, 4, 4) && !(y == 1 && (x == 6 || x == 9)) {
                Some(if x == 7 && y == 2 { FLINT_LIT } else { FLINT })
            } else if rect(x, y, 4, 5, 8, 3) {
                Some(if y == 5 {
                    l
                } else if y == 7 {
                    d
                } else {
                    m
                })
            } else if rect(x, y, 7, 8, 2, 7) {
                Some(HANDLE)
            } else {
                None
            }
        }),
        _ => return None,
    })
}
