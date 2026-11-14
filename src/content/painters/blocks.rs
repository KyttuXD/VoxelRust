//! Plain building blocks: earth, sand, snow, clay, lamps, wool, concrete,
//! terracotta, farmland, magma, chests and workbenches. Flat fills with a
//! one-pixel darker frame.

use super::*;

const DIRT: [u8; 3] = [132, 94, 66];
const SNOW: [u8; 3] = [255, 255, 255];

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "dirt" => framed(x, y, DIRT, DARK),
        "sand" => framed(x, y, [219, 211, 159], 82),
        "clay" => framed(x, y, [159, 164, 177], 77),
        "snow" => framed(x, y, SNOW, 90),
        "snow_side" => {
            // Snow on top, a jagged lower edge, dirt below.
            let snowy = y < 4 || (y == 4 && x % 4 < 2);
            if snowy {
                framed(x, y, SNOW, 90)
            } else {
                framed(x, y, DIRT, DARK)
            }
        }
        "gravel" => {
            let base = [122, 115, 115];
            let pebble = rect(x, y, 2, 3, 3, 2) || rect(x, y, 9, 1, 3, 2) || rect(x, y, 5, 10, 3, 2) || rect(x, y, 11, 8, 3, 2);
            if edge(x, y) {
                px(scale(base, DARK))
            } else if pebble {
                px(scale(base, 78))
            } else {
                px(base)
            }
        }
        "sandstone" => {
            let fill = [237, 231, 184];
            if edge(x, y) {
                px(scale(fill, 86))
            } else if y == 5 || y == 10 {
                px(scale(fill, 93))
            } else {
                px(fill)
            }
        }
        "lamp" => {
            if edge(x, y) {
                px([120, 88, 48])
            } else if disc(x, y, 4) {
                px([255, 252, 224])
            } else {
                px([250, 214, 120])
            }
        }
        _ => return None,
    })
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        "concrete" => framed_pal(x, y, pal, rgb, 92),
        "wool" => framed_pal(x, y, pal, rgb, 85),
        "terracotta" => framed_pal(x, y, pal, rgb, DARK),
        "farmland" => {
            // Furrows running one way, darker than plain dirt.
            if edge(x, y) || y % 4 == 3 {
                px(pal.shade(rgb, 70))
            } else {
                px(rgb)
            }
        }
        "magma" => {
            // A dark crust with four glowing blobs, the middle of each lighter.
            let crust = [62, 42, 44];
            for (bx, by) in [(3, 3), (10, 4), (5, 9), (11, 11)] {
                if rect(x, y, bx, by, 2, 2) {
                    return Some(px(if x == bx && y == by { pal.light_of(rgb) } else { rgb }));
                }
            }
            px(if edge(x, y) { scale(crust, 70) } else { crust })
        }
        "chest" => {
            if edge(x, y) || y == 5 {
                px(pal.shade(rgb, 60))
            } else if rect(x, y, 7, 4, 2, 4) {
                px(pal.accent().unwrap_or([214, 196, 120])) // the latch
            } else if y < 5 {
                px(rgb)
            } else {
                px(scale(rgb, 88))
            }
        }
        // The faces of a chest block (data/base.txt: a 14 x 14 footprint, a 7 high
        // body and a 3 high lid, so rows 6..15 of the picture): `chest_front` is
        // the latch side, `chest_side` the other three, `chest_top` the lid and
        // the underside. Each draws its own dark outline; the cells round it
        // are never seen.
        "chest_front" | "chest_side" => {
            if !((1..15).contains(&x) && (6..16).contains(&y)) {
                return Some(CLEAR);
            }
            if x == 1 || x == 14 || y == 6 || y == 15 || y == 9 {
                px(pal.shade(rgb, 60))
            } else if style == "chest_front" && rect(x, y, 7, 7, 2, 5) {
                px(pal.accent().unwrap_or([214, 196, 120])) // the latch, across the lid's seam
            } else if y < 9 {
                px(rgb)
            } else {
                px(scale(rgb, 88))
            }
        }
        "chest_top" => {
            if !((1..15).contains(&x) && (1..15).contains(&y)) {
                return Some(CLEAR);
            }
            let ring = ((3..=12).contains(&y) && (x == 3 || x == 12)) || ((3..=12).contains(&x) && (y == 3 || y == 12));
            if x == 1 || x == 14 || y == 1 || y == 14 {
                px(pal.shade(rgb, 60))
            } else if ring {
                px(scale(rgb, 90))
            } else {
                px(rgb)
            }
        }
        "workbench" => {
            if x % 5 == 0 || y % 5 == 0 {
                px(pal.shade(rgb, 60))
            } else {
                px(rgb)
            }
        }
        _ => return None,
    })
}
