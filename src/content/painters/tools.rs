//! Tools, weapons and sticks. All are drawn upright: the handle straight down
//! the middle, the head or blade at the top, a dark outline all round.
//! `paint <style> r g b` gives the colour of the metal (or wood) of the head:
//! its lit and shaded tones come from it by fixed ratios, so every material
//! is the same drawing recoloured. With a palette the head can have a dark
//! and a light colour of its own, or a gradient over the head (fire is red to
//! yellow); the second palette of `palette a+b` is the handle (wood types);
//! a sword's `accent` is a fuller line down the blade; `outline` replaces the
//! near-black outline.

use super::*;

const WOOD: [u8; 3] = [160, 122, 74];
const WOOD_DARK: [u8; 3] = [116, 86, 52];

/// A two-pixel handle in columns 7 and 8 from row `y0` to row `y1`, in the
/// colours `wood` (lit column, shaded column).
fn handle_in(x: i32, y: i32, y0: i32, y1: i32, wood: ([u8; 3], [u8; 3])) -> Option<[u8; 3]> {
    if y < y0 || y > y1 {
        None
    } else if x == 7 {
        Some(wood.0)
    } else if x == 8 {
        Some(wood.1)
    } else {
        None
    }
}

/// The usual wooden handle.
fn handle(x: i32, y: i32, y0: i32, y1: i32) -> Option<[u8; 3]> {
    handle_in(x, y, y0, y1, (WOOD, WOOD_DARK))
}

/// The box a gradient is spread over: the head of each tool, as
/// `(x0, y0, width, height)`.
fn head_box(style: &str) -> (i32, i32, i32, i32) {
    match style {
        "pick" => (2, 2, 12, 3),
        "axe" => (3, 2, 6, 5),
        "shovel" => (5, 1, 6, 6),
        "hoe" => (3, 2, 7, 4),
        "sword" => (6, 1, 4, 9),
        _ => (0, 0, 16, 16),
    }
}

/// `c` if `on`, else nothing.
fn when(on: bool, c: [u8; 3]) -> Option<[u8; 3]> {
    if on {
        Some(c)
    } else {
        None
    }
}

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "wand" => outlined(x, y, &|x, y| when(rect(x, y, 7, 2, 2, 4), [140, 232, 250]).or_else(|| handle(x, y, 6, 14))),
        _ => return None,
    })
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let (hx, hy, hw, hh) = head_box(style);
    let rgb = pal.at_in(x, y, hx, hy, hw, hh);
    let (l, m, d) = (pal.light_of(rgb), rgb, pal.dark_of(rgb));
    let wood = pal.handle_or((WOOD, WOOD_DARK));
    let handle = |x: i32, y: i32, y0: i32, y1: i32| handle_in(x, y, y0, y1, wood);
    let outlined = |x: i32, y: i32, f: &dyn Fn(i32, i32) -> Option<[u8; 3]>| outlined_with(x, y, pal.outline(), f);
    Some(match style {
        "stick" => outlined(x, y, &|x, y| handle(x, y, 2, 14)),
        "pick" => outlined(x, y, &|x, y| {
            // A bar across the top with its ends bent down.
            let head = match y {
                2 => when((4..=11).contains(&x), l),
                3 => when((2..=13).contains(&x), m),
                4 => when(matches!(x, 2 | 3 | 12 | 13), d),
                _ => None,
            };
            head.or_else(|| handle(x, y, 4, 14))
        }),
        "axe" => outlined(x, y, &|x, y| {
            // A blade to the left of the top of the handle.
            let head = match y {
                2 => when((4..=8).contains(&x), l),
                3 | 4 => when((3..=8).contains(&x), if x == 3 { l } else { m }),
                5 => when((3..=7).contains(&x), d),
                6 => when((4..=6).contains(&x), d),
                _ => None,
            };
            head.or_else(|| handle(x, y, 3, 14))
        }),
        "shovel" => outlined(x, y, &|x, y| {
            // A rounded blade on the end of the handle.
            let head = match y {
                1 => when((6..=9).contains(&x), l),
                2..=5 => when((5..=10).contains(&x), if x == 5 { l } else if x == 10 { d } else { m }),
                6 => when((6..=9).contains(&x), d),
                _ => None,
            };
            head.or_else(|| handle(x, y, 6, 14))
        }),
        "hoe" => outlined(x, y, &|x, y| {
            // A flat blade across the top with a hook down at the left end.
            let head = match y {
                2 => when((3..=9).contains(&x), l),
                3 => when((3..=9).contains(&x), m),
                4 | 5 => when(x == 3 || x == 4, d),
                _ => None,
            };
            head.or_else(|| handle(x, y, 4, 14))
        }),
        "sword" => outlined(x, y, &|x, y| {
            // A broad blade, a cross guard, a short grip and a pommel.
            match y {
                1 => when(x == 7 || x == 8, if x == 7 { l } else { m }),
                2..=9 => when(
                    (6..=9).contains(&x),
                    match pal.accent() {
                        // A fuller: a line of the second colour down the middle of the blade.
                        Some(f) if x == 7 && (3..=8).contains(&y) => f,
                        _ => {
                            if x == 6 {
                                l
                            } else if x == 9 {
                                d
                            } else {
                                m
                            }
                        }
                    },
                ),
                10 => when((4..=11).contains(&x), d),
                11..=14 => handle(x, y, 11, 14),
                _ => None,
            }
        }),
        "bow" => outlined(x, y, &|x, y| {
            // A curved stave on the left and a straight string.
            let stave = match y {
                2 => when(x == 10 || x == 11, l),
                3 => when(x == 8 || x == 9, l),
                4 => when(x == 7, l),
                5 => when(x == 6, l),
                6 | 9 => when(x == 5, m),
                7 | 8 => when(x == 4 || x == 5, WOOD_DARK),
                10 => when(x == 6, d),
                11 => when(x == 7, d),
                12 => when(x == 8 || x == 9, d),
                13 => when(x == 10 || x == 11, d),
                _ => None,
            };
            stave.or_else(|| when(x == 12 && (3..=12).contains(&y), [236, 236, 230]))
        }),
        "crossbow" => outlined(x, y, &|x, y| {
            // A stock down the middle, the arms across the top and a taut string.
            let arms = match y {
                2 => when(x == 2 || x == 13, l),
                3 => when((3..=12).contains(&x), m),
                4 => when(x == 3 || x == 12, d),
                _ => None,
            };
            arms.or_else(|| when(y == 5 && (4..=11).contains(&x) && x != 7 && x != 8, [236, 236, 230])).or_else(|| when((7..=8).contains(&x) && (4..=14).contains(&y), if x == 7 { d } else { scale(rgb, 55) }))
        }),
        "arrow" => outlined(x, y, &|x, y| {
            // A shaft with a head at the top and white feathers at the foot.
            let head = match y {
                1 => when(x == 7, l),
                2 | 3 => when((6..=8).contains(&x), if x == 6 { l } else { m }),
                _ => None,
            };
            head.or_else(|| when(x == 7 && (4..=13).contains(&y), WOOD)).or_else(|| when((x == 6 || x == 8) && (12..=14).contains(&y), [236, 236, 240]))
        }),
        _ => return None,
    })
}
