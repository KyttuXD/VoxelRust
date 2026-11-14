//! Wood: logs, bark, cut ends, planks and doors. In the house style planks
//! are upright boards (not slats) with a nail at each end and a joint at a
//! different height in each board, bark is in dashes, and the cut end of a
//! log is round, with a pith.

use super::*;
use crate::palette::Role;

/// Planks: three upright boards (five, four and four wide, a seam of the
/// darker tone after each), a joint across each board at its own height, a
/// short grain dash and a light nail head at the top and the foot.
pub fn planks(x: i32, y: i32, slat: [u8; 3], gap: Option<[u8; 3]>) -> Px {
    let gap = gap.unwrap_or_else(|| scale(slat, 77));
    let seam = x == 5 || x == 10 || x == 15;
    let joint = (y == 11 && x < 5) || (y == 8 && (6..10).contains(&x)) || (y == 4 && (11..15).contains(&x));
    if seam || joint {
        px(gap)
    } else if (matches!(x, 2 | 7 | 12)) && (y == 1 || y == 14) {
        px(lift(slat))
    } else if (x == 3 && (5..7).contains(&y)) || (x == 8 && (12..14).contains(&y)) || (x == 13 && (9..11).contains(&y)) {
        px(scale(slat, 91))
    } else {
        px(slat)
    }
}

/// Bark: dark with lighter vertical bars, broken into dashes. Pale barks get a few dark flecks.
pub fn bark(x: i32, y: i32, bar: [u8; 3], base: Option<[u8; 3]>) -> Px {
    let base = base.unwrap_or_else(|| scale(bar, 66));
    if bar[0] > 180 && (rect(x, y, 3, 3, 2, 1) || rect(x, y, 10, 6, 2, 1) || rect(x, y, 5, 11, 2, 1) || rect(x, y, 12, 13, 2, 1)) {
        return px(scale(bar, 25));
    }
    px(if matches!(x % 4, 1 | 2) && (y + (x / 4) * 3) % 8 != 0 { bar } else { base })
}

/// The cut end of a log: round, a dark rim in the corners and along the
/// sides, rings in two tones and a darker pith in the middle.
pub fn rings(x: i32, y: i32, rgb: [u8; 3], rim: Option<[u8; 3]>) -> Px {
    let (dx, dy) = (2 * x - 15, 2 * y - 15);
    let d2 = dx * dx + dy * dy;
    px(if d2 > 196 {
        rim.unwrap_or_else(|| scale(rgb, 55))
    } else if d2 <= 2 {
        scale(rgb, 70)
    } else if d2 > 50 && d2 <= 110 {
        scale(rgb, 86)
    } else {
        rgb
    })
}

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "log_side" => bark(x, y, [134, 92, 54], None),
        "pine_side" => bark(x, y, [96, 70, 62], None),
        "log_top" => rings(x, y, [192, 146, 86], None),
        "planks" => planks(x, y, [186, 142, 82], None),
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
            } else if rect(x, y, 3, 2, 4, 5) || rect(x, y, 9, 2, 4, 5) || rect(x, y, 3, 10, 10, 3) {
                px(scale(rgb, 86))
            } else {
                px(rgb)
            }
        }
        _ => return None,
    })
}
