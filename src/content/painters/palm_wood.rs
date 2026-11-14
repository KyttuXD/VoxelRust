//! The palm tree's wood and fronds, drawn like the OCD pack's trees (see
//! `ocd_wood.rs` for the shared shapes) because the pack has no palm.
//!
//! - **`palm_bark`**: the pack's bark bars, and across them the ring scars
//!   of a palm trunk: two rows of a darker tone (at rows 5 and 11) that cut
//!   the bars and leave the ground between them.
//! - **`palm_rings`**: round rings with a fibre dot at each corner of the ring.
//! - **`palm_planks`**: the pack's slats with the joints in other places than
//!   the maple's.
//! - **`palm_fronds`**: leaves as a frond: a rib down the middle with the
//!   leaflets drooping outward from it in bands, each leaflet with a lit top
//!   row, a fill row and a shaded foot; clear where there is no leaflet.
//!
//! Two to four flat colours each, no noise.

use super::ocd_wood::{bark_px, frame_tone, planks_px, rings_px};
use super::*;

/// The bark of a palm: bars with ring scars.
fn palm_bark(x: i32, y: i32, bar: [u8; 3], gap: [u8; 3]) -> Px {
    if !edge(x, y) && matches!(x % 4, 1 | 2) && matches!(y, 5 | 11) {
        return px(scale(bar, 80));
    }
    bark_px(x, y, bar, gap, false)
}

/// A frond: the rib in the middle two columns, the leaflets in bands that
/// sag three rows deep further out, shaded at the foot.
fn fronds(x: i32, y: i32, fill: [u8; 3], shade: [u8; 3], lit: [u8; 3]) -> Px {
    let across = (2 * x - 15).abs();
    if across == 1 {
        return px(shade);
    }
    let reach = (across - 1) / 2;
    match (y - (reach + 1) / 2).rem_euclid(4) {
        0 => px(lit),
        1 => px(fill),
        2 => px(shade),
        _ => CLEAR,
    }
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        "palm_bark" => palm_bark(x, y, rgb, frame_tone(pal, rgb)),
        "palm_rings" => rings_px(x, y, rgb, scale(rgb, 84), frame_tone(pal, rgb), true),
        "palm_planks" => planks_px(x, y, rgb, pal.shade(rgb, 76), [3, 10, 6, 12]),
        "palm_fronds" => fronds(x, y, rgb, pal.dark_of(rgb), pal.light_of(rgb)),
        _ => return None,
    })
}
