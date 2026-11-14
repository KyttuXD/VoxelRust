//! Water and lava. Water has eight animation frames: a flat blue with
//! lighter one-pixel ripples that travel down the tile.

use super::*;
use alloc::vec::Vec;

/// One pixel of the water texture at a moment in its eight-step cycle. The
/// pattern wraps both ways, so tiles join without a seam.
pub fn water_px(x: i32, y: i32, phase: i32) -> Px {
    const WOBBLE: [i32; 4] = [0, 1, 2, 1];
    let ripple = (y + WOBBLE[(x / 4 & 3) as usize] + phase).rem_euclid(8) == 0;
    let c = if ripple { [86, 160, 240] } else { [40, 106, 206] };
    [c[0], c[1], c[2], 168]
}

/// Lava: molten orange with a few dark crust slabs and hot yellow spots.
pub fn lava_px(x: i32, y: i32) -> Px {
    // Crust slabs with a notch each, and hot spots in short bars.
    let crust = rect(x, y, 1, 2, 4, 2) || rect(x, y, 2, 4, 1, 1) || rect(x, y, 9, 1, 5, 2) || rect(x, y, 12, 3, 1, 1) || rect(x, y, 5, 8, 4, 2) || rect(x, y, 6, 10, 1, 1) || rect(x, y, 11, 11, 3, 2) || rect(x, y, 2, 12, 3, 1);
    let hot = rect(x, y, 3, 6, 2, 1) || rect(x, y, 11, 5, 1, 2) || rect(x, y, 7, 13, 2, 1) || rect(x, y, 13, 8, 1, 1);
    px(if crust {
        [86, 28, 34]
    } else if hot {
        [255, 204, 72]
    } else {
        [232, 94, 28]
    })
}

/// A whole frame of [`water_px`], 16 x 16 RGBA.
pub fn water_frame(phase: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16 {
        for x in 0..16 {
            out.extend_from_slice(&water_px(x, y, phase));
        }
    }
    out
}

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "water" => water_px(x, y, 0),
        "lava" => lava_px(x, y),
        _ => return None,
    })
}
