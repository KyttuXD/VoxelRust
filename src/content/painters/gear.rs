//! Gems, ingots and the little pictures of vehicles (also used for rails,
//! boats and planes in item lists). Three tones of the given colour and a
//! dark outline; rails are left without one, as they lie on the ground.

use super::*;

const TYRE: [u8; 3] = [54, 54, 70];
const HUB: [u8; 3] = [154, 156, 172];
const MAST: [u8; 3] = [102, 68, 44];
const SAIL: [u8; 3] = [240, 234, 220];
const SLEEPER: [u8; 3] = [158, 108, 64];
const SLEEPER_DARK: [u8; 3] = [116, 78, 52];

fn when(on: bool, c: [u8; 3]) -> Option<[u8; 3]> {
    if on {
        Some(c)
    } else {
        None
    }
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    let (l, m, d) = (pal.light_of(rgb), rgb, pal.dark_of(rgb));
    Some(match style {
        "gem" => outlined_with(x, y, pal.outline(), &|x, y| {
            // A cut stone: lit upper left, shaded lower right, a pale sparkle on the lit facet.
            if (2 * x - 15).abs() + (2 * y - 15).abs() <= 11 {
                Some(if (x, y) == (5, 6) {
                    sparkle(l)
                } else if x + y <= 13 {
                    l
                } else if x + y >= 17 {
                    d
                } else {
                    m
                })
            } else {
                None
            }
        }),
        "ingot" => outlined_with(x, y, pal.outline(), &|x, y| {
            // A bar seen from a corner: lit top with a sparkle, front with a stamped notch, shaded end.
            if (5..9).contains(&y) && (2 + (8 - y)..12 + (8 - y)).contains(&x) {
                Some(if (x, y) == (6, 6) { sparkle(l) } else { l })
            } else if (9..12).contains(&y) && (2..12).contains(&x) {
                Some(if (x, y) == (4, 10) || (x, y) == (5, 10) { d } else { m })
            } else if (9..12).contains(&y) && (12..12 + (12 - y)).contains(&x) {
                Some(d)
            } else {
                None
            }
        }),
        "rail" => {
            // Two rails across wooden sleepers, on a clear ground.
            if x == 2 || x == 12 {
                px(l)
            } else if x == 3 || x == 13 {
                px(d)
            } else if (4..12).contains(&x) && y % 4 == 1 {
                px(SLEEPER)
            } else if (4..12).contains(&x) && y % 4 == 2 {
                px(SLEEPER_DARK)
            } else {
                CLEAR
            }
        }
        "machine" => outlined_with(x, y, pal.outline(), &|x, y| {
            // A generic "thing with wheels": body, cabin with a window, two wheels.
            for cx in [4, 11] {
                if (x == cx && (y == 11 || y == 13)) || ((x - cx).abs() <= 1 && y == 12) {
                    return Some(if x == cx && y == 12 { HUB } else { TYRE });
                }
            }
            if rect(x, y, 6, 3, 4, 1) {
                Some([200, 228, 240])
            } else if rect(x, y, 5, 2, 6, 3) {
                Some(m)
            } else if rect(x, y, 1, 5, 14, 6) {
                Some(if y == 5 {
                    l
                } else if y == 10 {
                    d
                } else {
                    m
                })
            } else {
                None
            }
        }),
        "hull" => outlined_with(x, y, pal.outline(), &|x, y| {
            // A boat: a hull narrowing downwards, a mast and a sail.
            if (7..12).contains(&y) && (2 * x - 15).abs() <= 13 - (y - 7) * 2 {
                Some(if y == 7 {
                    l
                } else if y == 11 {
                    d
                } else {
                    m
                })
            } else if x == 7 && (1..7).contains(&y) {
                Some(MAST)
            } else {
                when((8..13).contains(&x) && (1..6).contains(&y) && x - 8 <= 5 - y, SAIL)
            }
        }),
        "wings" => outlined_with(x, y, pal.outline(), &|x, y| {
            // A plane from above: fuselage, wings and tail.
            let fuselage = rect(x, y, 1, 6, 14, 4);
            let wing = rect(x, y, 6, 1, 4, 14);
            let tail = rect(x, y, 12, 4, 3, 8);
            if !(fuselage || wing || tail) {
                None
            } else if (fuselage && y == 6 && !wing) || (wing && x == 6 && !fuselage) || (tail && y == 4 && !fuselage) {
                Some(l)
            } else if (fuselage && y == 9 && !wing) || (wing && x == 9 && !fuselage) || (tail && y == 11) {
                Some(d)
            } else {
                Some(m)
            }
        }),
        _ => return None,
    })
}
