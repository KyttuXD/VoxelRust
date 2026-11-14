//! The palm tree's wood and fronds, the core wood's drawing (`wood.rs`) with
//! what makes a palm a palm added on top, so it stands beside the oak, the
//! maple and the birch and does not look like another game's:
//!
//! - **`palm_bark`**: the core bark, and across it the ring scars of a palm
//!   trunk: two rows (5 and 11) where the bars are a darker tone, the ground
//!   between the bars left as it is.
//! - **`palm_rings`**: the core cut end with a fibre dot of the ring tone at
//!   each corner of the outer ring.
//! - **`palm_planks`**: the core planks.
//! - **`palm_fronds`**: leaves as a frond: a two pixel rib down the middle,
//!   the leaflets drooping outward in bands, each with a lit top row, a fill
//!   row and a shaded foot; clear where there is no leaflet (never half see-through).
//!
//! Two to five flat colours each, no noise.

use super::*;
use crate::palette::Role;

/// The bark of a palm: the core bark with ring scars.
fn palm_bark(x: i32, y: i32, bar: [u8; 3], base: Option<[u8; 3]>) -> Px {
    if matches!(x % 4, 1 | 2) && matches!(y, 5 | 11) {
        return px(scale(bar, 80));
    }
    wood::bark(x, y, bar, base)
}

/// The cut end of a palm: the core rings with fibre dots.
fn palm_rings(x: i32, y: i32, rgb: [u8; 3], rim: Option<[u8; 3]>) -> Px {
    if [(3, 3), (12, 3), (3, 12), (12, 12)].contains(&(x, y)) {
        return px(scale(rgb, 86));
    }
    wood::rings(x, y, rgb, rim)
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
        "palm_bark" => palm_bark(x, y, rgb, pal.role(Role::Dark)),
        "palm_rings" => palm_rings(x, y, rgb, pal.role(Role::Dark)),
        "palm_planks" => wood::planks(x, y, rgb, pal.role(Role::Dark)),
        "palm_fronds" => fronds(x, y, rgb, pal.dark_of(rgb), pal.light_of(rgb)),
        _ => return None,
    })
}
