//! The maple's wood, in the same drawing as the core oak, pine and birch
//! (`wood.rs`, `plants.rs`) so that every tree of the game reads as one family.
//! The style names `ocd_bark`, `ocd_rings`, `ocd_planks` and `ocd_leaves` are
//! older than this file (they drew the OCD pack's shapes, which did not match
//! the core wood) and stay because the woods mod's `pack.txt` names them.
//!
//! - **`ocd_bark`**: the core bark, bars in dashes on a dark ground.
//! - **`ocd_rings`**: the core cut end, round, a darker pith.
//! - **`ocd_planks`**: the core planks, three upright boards, joints at
//!   different heights, a nail head at each end.
//! - **`ocd_leaves`**: the core leaf clumps (two tones, slits two wide, all
//!   cut out and never half see-through) and one lit pixel at the corner of
//!   every second light clump, the glint of the house style, in the lit tone of the
//!   colour: a maple's red-orange leaves catch the light.
//!
//! Two to four flat colours each, no noise. The palm (`palm_wood.rs`) builds on
//! the same pieces.

use super::*;
use crate::palette::Role;

/// A leaf block of the maple: the core leaf clumps, and the lit pixel in the
/// top left corner of every second clump of the first tone.
pub fn maple_leaves(x: i32, y: i32, tone: [u8; 3], other: [u8; 3], lit: [u8; 3]) -> Px {
    let p = plants::leaves(x, y, tone, other);
    if p[3] != 0 && (x / 3 + y / 2) % 4 == 0 && x % 3 == 0 && y % 2 == 0 {
        px(lit)
    } else {
        p
    }
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        "ocd_bark" => wood::bark(x, y, rgb, pal.role(Role::Dark)),
        "ocd_rings" => wood::rings(x, y, rgb, pal.role(Role::Dark)),
        "ocd_planks" => wood::planks(x, y, rgb, pal.role(Role::Dark)),
        "ocd_leaves" => maple_leaves(x, y, rgb, pal.dark_of(rgb), pal.light_of(rgb)),
        _ => return None,
    })
}
