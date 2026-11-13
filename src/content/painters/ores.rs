//! Ores (stone with four blobs of the ore's colour) and crystal clusters.

use super::*;

const STONE: [u8; 3] = [126, 126, 126];

/// Stone with a frame and four 2 x 2 blobs of `ore`, each with a lighter
/// pixel on top.
fn ore(x: i32, y: i32, ore: [u8; 3]) -> Px {
    for (bx, by) in [(3, 3), (9, 4), (4, 9), (10, 10)] {
        if rect(x, y, bx, by, 2, 2) {
            return px(if x == bx && y == by { lift(ore) } else { ore });
        }
    }
    framed(x, y, STONE, DARK)
}

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "coal_ore" => ore(x, y, [40, 40, 46]),
        "iron_ore" => ore(x, y, [210, 160, 125]),
        "copper_ore" => ore(x, y, [86, 176, 150]),
        "crystal_ore" => ore(x, y, [150, 236, 250]),
        "ruby_ore" => ore(x, y, [232, 66, 122]),
        _ => return None,
    })
}

pub fn paint(style: &str, rgb: [u8; 3], x: i32, y: i32) -> Option<Px> {
    Some(match style {
        "cluster" => {
            // Three crystal spikes rising from the floor: lit on the left, shaded on the right.
            for (cx, top, half) in [(8, 2, 3), (4, 7, 2), (12, 6, 2)] {
                if y >= top && (x - cx).abs() * (16 - top) <= half * (y - top) {
                    return Some(px(if x < cx {
                        lift(rgb)
                    } else if x > cx {
                        dark(rgb)
                    } else {
                        rgb
                    }));
                }
            }
            CLEAR
        }
        _ => return None,
    })
}
