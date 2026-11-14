//! Stone and things built of it: stone, deep stone, cobblestone, bricks,
//! stone bricks, tiles, rock and polished stone. The house stone is a cool
//! slate (a hint of blue in every grey), chipped in pairs; its walls step
//! each row on by a quarter instead of a half.

use super::*;

/// The colour of stone (ores sit in it: see `ores.rs` and `palette.rs`).
pub const STONE: [u8; 3] = [116, 120, 132];
/// The colour of deep stone.
pub const DEEPSTONE: [u8; 3] = [52, 54, 72];

/// The seeds of the cobblestones: every pixel belongs to its nearest seed and
/// the seams run where two seeds are about equally near.
const SEEDS: [(i32, i32); 5] = [(4, 3), (11, 4), (7, 9), (2, 12), (12, 12)];

/// A wall of bricks `w` x `hgt` pixels, each row shifted on by `step` pixels
/// from the one above (a quarter of a brick: the stair of the house style),
/// with mortar along the top and the left of each brick. `w` divides 16 and
/// `step * (16 / hgt)` is a multiple of `w`, so the tile joins up. Returns
/// `(is_mortar, second_tone, glint)`: whether the pixel is mortar, whether
/// its brick is one of the lighter ones (every other brick) and whether it is
/// the lit pixel in the brick's top left corner.
fn brick_course(x: i32, y: i32, w: i32, hgt: i32, step: i32) -> (bool, bool, bool) {
    let row = y / hgt;
    let off = (row * step) % w;
    let col = (x + off) / w;
    let (bx, by) = ((x + off) % w, y % hgt);
    ((bx == 0 || by == 0), (col + row) % 2 == 1, bx == 1 && by == 1)
}

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "stone" => sealed_grain(x, y, STONE, DARK, if at_any(x, y, &CHIPS) { Some(scale(STONE, 88)) } else { None }),
        "deepstone" => sealed_grain(x, y, DEEPSTONE, 70, if at_any(x, y, &CHIPS) { Some(scale(DEEPSTONE, 84)) } else { None }),
        "cobble" => {
            // Five rounded stones with irregular seams, a lit pixel on each.
            let fill = [118, 120, 130];
            let mut best = (i32::MAX, 0usize);
            let mut next = i32::MAX;
            for (i, &(sx, sy)) in SEEDS.iter().enumerate() {
                let d = (x - sx) * (x - sx) + (y - sy) * (y - sy);
                if d < best.0 {
                    next = best.0;
                    best = (d, i);
                } else if d < next {
                    next = d;
                }
            }
            let (sx, sy) = SEEDS[best.1];
            if edge(x, y) || next - best.0 <= 6 {
                px(scale(fill, DARK))
            } else if (x, y) == (sx - 1, sy - 1) {
                px(lift(fill))
            } else {
                px(fill)
            }
        }
        "brick" => {
            // Red bricks, eight wide and four tall, every row stepped on by two; a lit corner on each.
            let (mortar, second, lit) = brick_course(x, y, 8, 4, 2);
            px(if mortar {
                [208, 198, 182]
            } else if lit {
                [204, 112, 84]
            } else if second {
                [166, 70, 60]
            } else {
                [150, 62, 56]
            })
        }
        "stone_bricks" => {
            // Big squared blocks with a bevel: a lit upper left edge, a seam round the rest.
            let (mortar, _, _) = brick_course(x, y, 8, 8, 4);
            let (bx, by) = ((x + (y / 8) * 4) % 8, y % 8);
            px(if mortar {
                scale(STONE, 66)
            } else if bx == 1 || by == 1 {
                lift(STONE)
            } else {
                STONE
            })
        }
        _ => return None,
    })
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        "rock" => {
            // Stone in any colour: frame, fill and chips in pairs.
            sealed_pal_grain(x, y, pal, rgb, DARK, if at_any(x, y, &CHIPS) { Some(scale(rgb, 88)) } else { None })
        }
        "polished" => {
            // Smooth stone: a frame and a bar of shine in the top left, longer than a glint.
            if edge(x, y) {
                px(pal.shade(rgb, 82))
            } else if (x, y) == (2, 2) || (x, y) == (3, 2) || (x, y) == (2, 3) || (x, y) == (4, 2) {
                px(pal.light_of(rgb))
            } else if (x, y) == (14, 14) {
                px(pal.shade(rgb, 82))
            } else {
                px(rgb)
            }
        }
        "bricks" => {
            // Light mortar, bricks in the colour and a lighter second tone in turn, each with a lit corner.
            let (mortar, second, lit) = brick_course(x, y, 8, 4, 2);
            px(if mortar {
                pal.accent().unwrap_or_else(|| pale(rgb))
            } else if lit {
                glint(if second { pal.light_of(rgb) } else { rgb })
            } else if second {
                pal.light_of(rgb)
            } else {
                rgb
            })
        }
        "tiles" => {
            // Four square tiles with grout between and a glint on each.
            if x % 8 == 7 || y % 8 == 7 {
                px(pal.shade(rgb, 72))
            } else if x % 8 == 1 && y % 8 == 1 {
                px(glint(rgb))
            } else {
                px(rgb)
            }
        }
        _ => return None,
    })
}
