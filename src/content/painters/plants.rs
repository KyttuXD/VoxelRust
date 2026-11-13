//! Things that grow: grass, leaves, flowers, tall grass, cactus, crops,
//! mushrooms and vines. Leaves and plants are drawn in the green they
//! should have in the game; cut-outs are fully see-through, never half.

use super::*;

const GRASS: [u8; 3] = [87, 182, 50];
const DIRT: [u8; 3] = [132, 94, 66];

/// The 2 x 2 cells of a leaf block that are holes (cell x, cell y).
const HOLES: [(i32, i32); 8] = [(1, 0), (5, 1), (3, 2), (7, 3), (0, 4), (4, 5), (2, 6), (6, 7)];

/// A leaf block: two tones in a loose 2 x 2 check and some see-through cells.
/// Cells of the second tone are `other` (the darker green, or blossom pink).
fn leaves(x: i32, y: i32, tone: [u8; 3], other: [u8; 3]) -> Px {
    let (cx, cy) = (x / 2, y / 2);
    if HOLES.contains(&(cx, cy)) {
        CLEAR
    } else if (cx + cy) % 2 == 1 {
        px(other)
    } else {
        px(tone)
    }
}

/// A flower: a head of petals round a brown middle on a stem with two leaves.
fn flower(x: i32, y: i32, petal: [u8; 3]) -> Px {
    let (dx, dy) = (2 * x - 15, 2 * y - 9);
    let d = dx * dx + dy * dy;
    if d <= 4 {
        px([70, 48, 30])
    } else if d <= 40 {
        px(petal)
    } else if (x == 7 || x == 8) && y > 6 {
        px([52, 128, 52])
    } else if (x == 6 || x == 9) && (y == 11 || y == 12) {
        px([84, 160, 72])
    } else {
        CLEAR
    }
}

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "grass_top" => framed(x, y, GRASS, DARK),
        "grass_side" => {
            // A green band over dirt, with a jagged lower edge.
            let green = y < 4 || (y == 4 && matches!(x % 4, 1 | 2));
            framed(x, y, if green { GRASS } else { DIRT }, DARK)
        }
        "leaves" => leaves(x, y, [64, 148, 48], [47, 108, 35]),
        "pine_leaves" => leaves(x, y, [38, 108, 74], [28, 79, 54]),
        "flower_red" => flower(x, y, [214, 52, 60]),
        "flower_yellow" => flower(x, y, [240, 204, 48]),
        "tall_grass" => {
            // Five blades, two pixels wide, the left pixel lit.
            let g = [84, 160, 72];
            for (x0, top) in [(1, 8), (4, 5), (7, 3), (10, 7), (13, 4)] {
                if (x == x0 || x == x0 + 1) && y >= top {
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
            let fill = [46, 140, 60];
            if [(3, 4), (8, 9), (12, 3), (3, 12), (13, 12), (7, 2)].contains(&(x, y)) {
                px([224, 228, 190]) // spines
            } else if x == 0 || x == 15 || x == 5 || x == 10 {
                px(dark(fill))
            } else {
                px(fill)
            }
        }
        "cactus_top" => {
            if edge(x, y) {
                px(dark([62, 144, 74]))
            } else if disc(x, y, 5) {
                px([96, 170, 96])
            } else {
                px([62, 144, 74])
            }
        }
        _ => return None,
    })
}

pub fn paint(style: &str, rgb: [u8; 3], x: i32, y: i32) -> Option<Px> {
    Some(match style {
        "leaves" => leaves(x, y, rgb, dark(rgb)),
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
            // A domed cap in the given colour on a pale stem.
            let (dx, dy) = (2 * x - 15, 2 * y - 13);
            if (4..9).contains(&y) && dx * dx + dy * dy * 3 <= 150 {
                if y == 8 {
                    px(dark(rgb))
                } else if [(6, 5), (10, 6), (8, 7)].contains(&(x, y)) {
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
            for (sx, len) in [(3, 11), (8, 15), (12, 9)] {
                if x == sx && y < len {
                    return Some(px(if [(3, 8), (8, 5), (12, 6)].contains(&(x, y)) { [250, 214, 110] } else { dark(rgb) }));
                }
            }
            if [4, 9, 11].contains(&x) && y % 4 == 1 && y < 10 {
                px(rgb)
            } else {
                CLEAR
            }
        }
        _ => return None,
    })
}
