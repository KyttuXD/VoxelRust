//! Stone and things built of it: stone, deep stone, cobblestone, bricks,
//! stone bricks, tiles, rock and polished stone.

use super::*;

const STONE: [u8; 3] = [126, 126, 126];

/// A wall of bricks `w` x `hgt` pixels, every other row shifted by half a
/// brick, with mortar along the bottom and right of each brick. Returns
/// `(is_mortar, second_tone)`: whether the pixel is mortar and whether its
/// brick is one of the lighter ones (every other brick).
fn brick_course(x: i32, y: i32, w: i32, hgt: i32) -> (bool, bool) {
    let row = y / hgt;
    let off = if row % 2 == 0 { 0 } else { w / 2 };
    let col = (x + off) / w;
    let mortar = y % hgt == hgt - 1 || (x + off) % w == w - 1;
    (mortar, (col + row) % 2 == 1)
}

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "stone" => framed(x, y, STONE, DARK),
        "deepstone" => framed(x, y, [58, 56, 66], 70),
        "cobble" => {
            // Four big stones: a frame, a cross between them, a light corner on each.
            let fill = [126, 126, 128];
            if edge(x, y) || x == 7 || y == 7 {
                px(scale(fill, DARK))
            } else if (x == 1 || x == 8) && (y == 1 || y == 8) {
                px(lift(fill))
            } else {
                px(fill)
            }
        }
        "brick" => {
            let (mortar, _) = brick_course(x, y, 8, 4);
            px(if mortar { [202, 196, 184] } else { [160, 74, 58] })
        }
        "stone_bricks" => {
            let (mortar, _) = brick_course(x, y, 8, 8);
            px(if mortar { scale(STONE, 70) } else { STONE })
        }
        _ => return None,
    })
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        "rock" => {
            // Stone in any colour: frame, fill and two flatter patches.
            if edge(x, y) {
                px(pal.shade(rgb, DARK))
            } else if rect(x, y, 3, 10, 2, 2) || rect(x, y, 10, 3, 2, 2) {
                px(scale(rgb, 88))
            } else {
                px(rgb)
            }
        }
        "polished" => {
            // Smooth stone: a frame and a little shine in the top left.
            if edge(x, y) {
                px(pal.shade(rgb, 82))
            } else if (x == 2 && y == 2) || (x == 3 && y == 2) || (x == 2 && y == 3) {
                px(pal.light_of(rgb))
            } else {
                px(rgb)
            }
        }
        "bricks" => {
            // Light mortar, bricks in the colour and a lighter second tone in turn.
            let (mortar, second) = brick_course(x, y, 8, 4);
            px(if mortar {
                pal.accent().unwrap_or_else(|| pale(rgb))
            } else if second {
                pal.light_of(rgb)
            } else {
                rgb
            })
        }
        "tiles" => {
            // Four square tiles with grout between.
            if x % 8 == 7 || y % 8 == 7 {
                px(pal.shade(rgb, 72))
            } else {
                px(rgb)
            }
        }
        _ => return None,
    })
}
