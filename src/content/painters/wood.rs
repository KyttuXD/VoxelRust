//! Wood: logs, bark, cut ends, planks and doors.

use super::*;
use crate::palette::Role;

/// Planks: lighter slats, four rows of them, with a gap of the darker base
/// tone between rows and a staggered joint in each slat.
fn planks(x: i32, y: i32, slat: [u8; 3], gap: Option<[u8; 3]>) -> Px {
    let gap = gap.unwrap_or_else(|| scale(slat, 77));
    let joint = x == 5 + (y / 4 % 2) * 8;
    px(if y % 4 == 3 || joint { gap } else { slat })
}

/// Bark: dark with lighter vertical bars. Pale barks get a few dark flecks.
fn bark(x: i32, y: i32, bar: [u8; 3], base: Option<[u8; 3]>) -> Px {
    let base = base.unwrap_or_else(|| scale(bar, 66));
    if bar[0] > 180 && (rect(x, y, 3, 3, 2, 1) || rect(x, y, 10, 6, 2, 1) || rect(x, y, 5, 11, 2, 1) || rect(x, y, 12, 13, 2, 1)) {
        return px(scale(bar, 25));
    }
    px(if matches!(x % 4, 1 | 2) { bar } else { base })
}

/// The cut end of a log: a dark rim, then rings in two tones.
fn rings(x: i32, y: i32, rgb: [u8; 3], rim: Option<[u8; 3]>) -> Px {
    let d = core::cmp::max((2 * x - 15).abs(), (2 * y - 15).abs()) / 2;
    px(if d >= 7 {
        rim.unwrap_or_else(|| scale(rgb, 55))
    } else if d % 2 == 0 {
        rgb
    } else {
        scale(rgb, 86)
    })
}

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "log_side" => bark(x, y, [125, 96, 58], None),
        "pine_side" => bark(x, y, [100, 74, 54], None),
        "log_top" => rings(x, y, [180, 146, 92], None),
        "planks" => planks(x, y, [172, 140, 87], None),
        _ => return None,
    })
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        "planks" => planks(x, y, rgb, pal.role(Role::Dark)),
        "bark" => bark(x, y, rgb, pal.role(Role::Dark)),
        "rings" => rings(x, y, rgb, pal.role(Role::Dark)),
        "door" => {
            // A framed door of two panels with a small handle.
            if x == 12 && (y == 9 || y == 10) {
                px(pal.accent().unwrap_or([222, 200, 110]))
            } else if edge(x, y) || y == 8 {
                px(pal.shade(rgb, 65))
            } else if rect(x, y, 3, 3, 10, 3) || rect(x, y, 3, 10, 10, 3) {
                px(scale(rgb, 86))
            } else {
                px(rgb)
            }
        }
        _ => return None,
    })
}
