//! Things that grow: grass, leaves, flowers, tall grass, cactus, crops,
//! mushrooms and vines. Leaves and plants are drawn in the green they
//! should have in the game; cut-outs are fully see-through, never half.

use super::*;

const GRASS: [u8; 3] = [84, 176, 58];

/// How far the green reaches down the side of a grass block, column by column.
const GDRIP: [i32; 16] = [3, 4, 3, 5, 4, 3, 4, 4, 5, 3, 4, 3, 5, 4, 3, 4];

/// A leaf block: two tones in clumps three wide and two tall, and a scatter
/// of see-through slits (two pixels wide, one tall, where `(x / 2) * 3 + y * 5`
/// is a multiple of 7). Clumps of the second tone are `other` (the darker
/// green, or blossom pink).
pub fn leaves(x: i32, y: i32, tone: [u8; 3], other: [u8; 3]) -> Px {
    if ((x / 2) * 3 + y * 5) % 7 == 0 {
        CLEAR
    } else if (x / 3 + y / 2) % 2 == 1 {
        px(other)
    } else {
        px(tone)
    }
}

/// A flower: four petals round a brown middle (a pale pixel on the upper
/// left one) on a stem with two leaves.
fn flower(x: i32, y: i32, petal: [u8; 3]) -> Px {
    if rect(x, y, 7, 4, 2, 2) {
        px([70, 48, 30])
    } else if (x, y) == (6, 3) {
        px(pale(petal))
    } else if rect(x, y, 7, 2, 2, 2) || rect(x, y, 7, 6, 2, 2) || rect(x, y, 5, 4, 2, 2) || rect(x, y, 9, 4, 2, 2) || (x, y) == (6, 3) || (x, y) == (9, 3) || (x, y) == (6, 6) || (x, y) == (9, 6) {
        px(petal)
    } else if (x == 7 || x == 8) && y > 7 {
        px(if x == 7 { [58, 136, 62] } else { [40, 104, 52] })
    } else if (x, y) == (6, 11) || (x, y) == (5, 10) || (x, y) == (9, 12) || (x, y) == (10, 11) {
        px([84, 164, 80])
    } else {
        CLEAR
    }
}

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        // The top: tufts, each a short diagonal of darker green.
        "grass_top" => sealed_grain(x, y, GRASS, DARK, if at_any(x, y, &[(4, 5), (5, 6), (11, 4), (10, 5), (7, 11), (8, 12)]) { Some(scale(GRASS, 86)) } else { None }),
        "grass_side" => {
            // A green band over flecked dirt, with a hanging, uneven lower edge.
            if y < GDRIP[x as usize & 15] {
                sealed(x, y, GRASS, DARK)
            } else {
                blocks::dirt_px(x, y)
            }
        }
        "leaves" => leaves(x, y, [62, 152, 60], [42, 110, 52]),
        "pine_leaves" => leaves(x, y, [34, 106, 82], [24, 76, 62]),
        "flower_red" => flower(x, y, [214, 52, 60]),
        "flower_yellow" => flower(x, y, [240, 204, 48]),
        "tall_grass" => {
            // Five blades, two pixels wide, the left pixel lit.
            let g = [80, 168, 70];
            for (x0, top) in [(1, 8), (4, 5), (7, 3), (10, 7), (13, 4)] {
                // A blade tapers to a one pixel tip on its right.
                if y >= top && (x == x0 + 1 || (x == x0 && y > top)) {
                    return Some(px(if y >= 14 {
                        dark(g)
                    } else if x == x0 {
                        lift(g)
                    } else {
                        g
                    }));
                }
            }
            CLEAR
        }
        "cactus_side" => {
            let fill = [48, 146, 74];
            if [(2, 4), (8, 2), (12, 7), (3, 11), (8, 12), (13, 13)].contains(&(x, y)) {
                px([240, 232, 190]) // spines
            } else if x == 0 || x == 15 || ((x == 5 || x == 10) && y % 4 != 3) {
                px(dark(fill))
            } else {
                px(fill)
            }
        }
        "cactus_top" => {
            if edge(x, y) {
                px(dark([62, 150, 84]))
            } else if (2 * x - 15).abs() + (2 * y - 15).abs() <= 9 {
                px([104, 178, 112])
            } else {
                px([62, 150, 84])
            }
        }
        _ => return None,
    })
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        "leaves" => leaves(x, y, rgb, pal.dark_of(rgb)),
        "blossom" => leaves(x, y, rgb, [255, 240, 246]),
        "crop1" | "crop2" | "crop3" | "crop4" => {
            // Three plants that get taller; the last stage carries the crop's own colour on top.
            let stage = (style.as_bytes()[4] - b'0') as i32;
            let green = [64 + stage as u8 * 12, 150 - stage as u8 * 6, 52];
            let top = 15 - stage * 3;
            for (x0, extra) in [(1, 1), (6, 0), (11, 2)] {
                if (x == x0 || x == x0 + 1) && y >= top + extra {
                    let c = if stage == 4 && y < top + extra + 4 { rgb } else { green };
                    return Some(px(if x == x0 { lift(c) } else { c }));
                }
            }
            CLEAR
        }
        "mushroom" => {
            // A domed cap in the given colour with two pale spots on a pale stem.
            let (dx, dy) = (2 * x - 15, 2 * y - 13);
            if (4..9).contains(&y) && dx * dx + dy * dy * 3 <= 150 {
                if y == 8 {
                    px(dark(rgb))
                } else if [(5, 5), (6, 5), (10, 6), (11, 6)].contains(&(x, y)) {
                    px(lift(rgb))
                } else {
                    px(rgb)
                }
            } else if (9..16).contains(&y) && (x == 7 || x == 8) {
                px(if x == 7 { [228, 220, 200] } else { [200, 193, 176] })
            } else {
                CLEAR
            }
        }
        "vine" => {
            // Strands hanging down with small leaves and a few bright berries.
            for (sx, len) in [(2, 12), (7, 15), (12, 10)] {
                if x == sx && y < len {
                    return Some(px(if [(2, 8), (7, 5), (12, 6)].contains(&(x, y)) { [250, 214, 110] } else { dark(rgb) }));
                }
            }
            if [3, 8, 11].contains(&x) && y % 4 == 1 && y < 10 {
                px(rgb)
            } else {
                CLEAR
            }
        }
        _ => return None,
    })
}
