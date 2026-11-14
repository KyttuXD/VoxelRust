//! Wood the way the OCD pack draws its trees, for the woods the pack has no
//! picture of (maple, and the base of the palm in `palm_wood.rs`): the same
//! shapes as the pack's oak, spruce and birch, in the wood's own colours, so
//! they stand beside the pack's trees and do not stand out.
//!
//! - **Bark** (`ocd_bark`): a darker frame, and four lighter bars two pixels
//!   wide with a dark pixel between them; a bar is broken by one dark pixel
//!   (a knot), at a different height on each pair of bars.
//! - **The cut end** (`ocd_rings`): a dark frame, round rings in two tones
//!   and a two pixel pith. Round, where the pack's are square.
//! - **Planks** (`ocd_planks`): four horizontal slats two pixels tall with a
//!   dark seam two pixels tall between them, as the pack's, and one joint
//!   (a seam across the slat) in each slat, at a different place in each.
//! - **Leaves** (`ocd_leaves`): four squares with a dark rim and a lit pixel
//!   in the upper left corner, and a see-through cross between them, as the
//!   pack's leaves (the cross is fully clear: leaves are never half see-through).
//!
//! Two or three flat colours each, no noise.

use super::*;

/// The frame tone of a wood: the palette's dark colour, else 63% of the lit tone (the pack's oak is 62-63%).
pub fn frame_tone(pal: &Palette, lit: [u8; 3]) -> [u8; 3] {
    pal.shade(lit, 63)
}

/// Bark: bars on a dark ground. `knots` breaks each bar with a dark pixel.
pub fn bark_px(x: i32, y: i32, bar: [u8; 3], gap: [u8; 3], knots: bool) -> Px {
    if edge(x, y) || !matches!(x % 4, 1 | 2) {
        return px(gap);
    }
    // The first and third pair of bars break high, the second and fourth low.
    let knot_y = if (x / 4) % 2 == 0 { 6 } else { 11 };
    if knots && y == knot_y {
        px(gap)
    } else {
        px(bar)
    }
}

/// The cut end of a log: the frame, an outer ring, an inner ring and a pith, all round.
/// `light` is the outer ring and pith, `mid` the ring between; `fibres` puts four
/// dots of the mid tone on the outer ring (a palm's fibres).
pub fn rings_px(x: i32, y: i32, light: [u8; 3], mid: [u8; 3], frame: [u8; 3], fibres: bool) -> Px {
    let (dx, dy) = (2 * x - 15, 2 * y - 15);
    let d2 = dx * dx + dy * dy;
    if edge(x, y) || d2 > 196 {
        px(frame)
    } else if fibres && [(3, 3), (12, 3), (3, 12), (12, 12)].contains(&(x, y)) {
        px(mid)
    } else if d2 > 121 {
        px(light)
    } else if d2 > 64 {
        px(mid)
    } else if d2 > 36 {
        px(light)
    } else if d2 > 9 {
        px(mid)
    } else {
        px(light)
    }
}

/// Planks: slats on a seam colour. `joints` is the column of the joint in the first to fourth slat.
pub fn planks_px(x: i32, y: i32, slat: [u8; 3], seam: [u8; 3], joints: [i32; 4]) -> Px {
    if edge(x, y) || !matches!(y % 4, 1 | 2) {
        return px(seam);
    }
    if x == joints[(y / 4) as usize & 3] {
        px(seam)
    } else {
        px(slat)
    }
}

/// Leaves: four squares, each with a rim, a lit corner pixel and a fill; the cross between them is clear.
pub fn leaves_px(x: i32, y: i32, fill: [u8; 3], rim: [u8; 3], lit: [u8; 3]) -> Px {
    if matches!(x, 7 | 8) || matches!(y, 7 | 8) {
        return CLEAR;
    }
    // Place inside the square, 0..=6 each way.
    let (u, v) = (if x < 7 { x } else { x - 9 }, if y < 7 { y } else { y - 9 });
    if u == 0 || u == 6 || v == 0 || v == 6 {
        px(rim)
    } else if u == 1 && v == 1 {
        px(lit)
    } else {
        px(fill)
    }
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        "ocd_bark" => bark_px(x, y, rgb, frame_tone(pal, rgb), true),
        "ocd_rings" => rings_px(x, y, rgb, scale(rgb, 84), frame_tone(pal, rgb), false),
        "ocd_planks" => planks_px(x, y, rgb, pal.shade(rgb, 76), [9, 4, 11, 6]),
        "ocd_leaves" => leaves_px(x, y, rgb, pal.dark_of(rgb), pal.light_of(rgb)),
        _ => return None,
    })
}
