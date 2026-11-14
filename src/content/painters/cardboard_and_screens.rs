//! Two plain blocks that were a flat colour with a grain, which reads as a
//! placeholder beside the OCD pack's pictures (the pack has neither): a
//! cardboard box face and a switched-off screen.
//!
//! - **`cardboard`**: the frame and the house corners, the fill, a flute (a
//!   darker line) on every third row, and a strip of lighter packing tape
//!   down the middle two columns.
//! - **`screen`**: a dark glass in a lighter bezel with a pale glare across
//!   the upper left, and one lit pixel in the lower right corner of the bezel.
//!
//! Three to five flat colours each, no noise.

use super::*;

fn cardboard(x: i32, y: i32, rgb: [u8; 3], pal: &Palette) -> Px {
    if edge(x, y) || (x, y) == (14, 14) || (x, y) == (1, 1) {
        return sealed_pal(x, y, pal, rgb, DARK);
    }
    if matches!(x, 7 | 8) {
        px(lift(rgb))
    } else if y % 3 == 2 {
        px(scale(rgb, 88))
    } else {
        px(rgb)
    }
}

fn screen(x: i32, y: i32, rgb: [u8; 3], pal: &Palette) -> Px {
    if edge(x, y) {
        return px(pal.shade(rgb, 60));
    }
    // The lit pixel: a power light in the bezel's lower right corner.
    if (x, y) == (13, 14) {
        return px([120, 224, 150]);
    }
    let glass = rect(x, y, 2, 2, 12, 11);
    if !glass {
        px(lift(rgb))
    } else if x + y == 6 || x + y == 7 {
        px(pale(rgb))
    } else {
        px(rgb)
    }
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        "cardboard" => cardboard(x, y, rgb, pal),
        "screen" => screen(x, y, rgb, pal),
        _ => return None,
    })
}
