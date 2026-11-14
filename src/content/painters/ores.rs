//! Ores (stone with four blobs of the ore's colour), vein ores and crystal clusters.
//!
//! An ore is drawn from palettes: the host stone's colour (`host` of the
//! palette, stone if none) and the ore's palette; a palette with `more`
//! palettes is a multi-gem ore whose blobs take the gems in turn. In a palette
//! the base is the blob, the highlight (or light) colour its top left pixel,
//! and the dark colour, if there is one, its bottom right pixel.

use super::*;
use crate::palette::Role;

const STONE: [u8; 3] = [126, 126, 126];

/// The four blobs: their top left pixels.
const BLOBS: [(i32, i32); 4] = [(3, 3), (9, 4), (4, 9), (10, 10)];

/// The lit pixel of a gem: its highlight, else its light colour, else the old lift.
fn lit(gem: &Palette, c: [u8; 3]) -> [u8; 3] {
    gem.role(Role::Highlight).unwrap_or_else(|| gem.light_of(c))
}

/// Stone with a frame and four 2 x 2 blobs, each with a lighter pixel on top
/// (and a darker one below if the palette has a dark colour).
fn ore_px(x: i32, y: i32, pal: &Palette) -> Px {
    let host = pal.host.unwrap_or(STONE);
    for (i, &(bx, by)) in BLOBS.iter().enumerate() {
        if rect(x, y, bx, by, 2, 2) {
            let gem = pal.nth(i);
            let c = gem.at(bx, by);
            return px(if x == bx && y == by {
                lit(gem, c)
            } else if x == bx + 1 && y == by + 1 && !gem.is_gradient() {
                gem.role(Role::Dark).unwrap_or(c)
            } else {
                c
            });
        }
    }
    framed(x, y, host, DARK)
}

/// Stone crossed by two diagonal bands: the upper left one in the first
/// palette, the lower right one in the second (or the first again), each a
/// lighter lit row beside its base row.
fn vein_px(x: i32, y: i32, pal: &Palette) -> Px {
    let host = pal.host.unwrap_or(STONE);
    if edge(x, y) {
        return px(scale(host, DARK));
    }
    let d = x + y;
    for (k, lo) in [(0usize, 9), (1usize, 17)] {
        if d == lo || d == lo + 1 {
            let gem = pal.nth(k);
            let c = gem.at(x, y);
            return px(if d == lo { lit(gem, c) } else { c });
        }
    }
    px(host)
}

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    let plain = |c: [u8; 3]| ore_px(x, y, &Palette::from_rgb(c));
    Some(match name {
        "coal_ore" => plain([40, 40, 46]),
        "iron_ore" => plain([210, 160, 125]),
        "copper_ore" => plain([86, 176, 150]),
        "crystal_ore" => plain([150, 236, 250]),
        "ruby_ore" => plain([232, 66, 122]),
        _ => return None,
    })
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        "ore" => ore_px(x, y, pal),
        "ore_vein" => vein_px(x, y, pal),
        "cluster" => {
            // Three crystal spikes rising from the floor: lit on the left, shaded on the right.
            for (cx, top, half) in [(8, 2, 3), (4, 7, 2), (12, 6, 2)] {
                if y >= top && (x - cx).abs() * (16 - top) <= half * (y - top) {
                    return Some(px(if x < cx {
                        pal.light_of(rgb)
                    } else if x > cx {
                        pal.dark_of(rgb)
                    } else {
                        rgb
                    }));
                }
            }
            CLEAR
        }
        _ => return None,
    })
}
