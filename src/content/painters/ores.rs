//! Ores (stone with four nuggets of the ore's colour), vein ores and crystal clusters.
//!
//! The house ore language: a nugget is three joined pixels (a lit one, a
//! base one and a dark one) and a thin fracture line of darker stone runs
//! from the first nugget towards the last, as if the ore had split the
//! rock. A vein is a wavy band, not a straight diagonal.
//!
//! An ore is drawn from palettes: the host stone's colour (`host` of the
//! palette, stone if none) and the ore's palette; a palette with `more`
//! palettes is a multi-gem ore whose nuggets take the gems in turn. In a
//! palette the base is the nugget, the highlight (or light) colour its first
//! pixel, and the dark colour, if there is one, its last pixel.

use super::*;
use crate::palette::Role;

const STONE: [u8; 3] = stone::STONE;

/// The four nuggets: three pixels each, the lit one first, the dark one last.
const NUGGETS: [[(i32, i32); 3]; 4] = [[(3, 2), (4, 2), (4, 3)], [(10, 4), (10, 5), (11, 5)], [(5, 9), (4, 10), (5, 10)], [(9, 11), (10, 11), (10, 12)]];

/// The fracture line between the nuggets, in a darker tone of the host.
const CRACK: [(i32, i32); 6] = [(6, 4), (7, 5), (8, 6), (8, 7), (7, 8), (7, 9)];

/// The lit pixel of a gem: its highlight, else its light colour, else the old lift.
fn lit(gem: &Palette, c: [u8; 3]) -> [u8; 3] {
    gem.role(Role::Highlight).unwrap_or_else(|| gem.light_of(c))
}

/// Stone with a frame, four nuggets of three pixels, the first lit and the
/// last dark (if the palette has a dark colour) and the fracture line.
fn ore_px(x: i32, y: i32, pal: &Palette) -> Px {
    let host = pal.host.unwrap_or(STONE);
    for (i, nugget) in NUGGETS.iter().enumerate() {
        if let Some(k) = nugget.iter().position(|&p| p == (x, y)) {
            let gem = pal.nth(i);
            let c = gem.at(x, y);
            return px(if k == 0 {
                lit(gem, c)
            } else if k == 2 && !gem.is_gradient() {
                gem.role(Role::Dark).unwrap_or(c)
            } else {
                c
            });
        }
    }
    if at_any(x, y, &CRACK) {
        return px(scale(host, 84));
    }
    framed(x, y, host, DARK)
}

/// Where the band `k` of a vein runs in column `x`: its lit row. The first
/// band is high on the left and dips in the middle; the second mirrors it.
fn vein_row(k: usize, x: i32) -> i32 {
    const WAVE: [i32; 8] = [0, 0, 1, 1, 2, 2, 1, 1];
    if k == 0 {
        3 + WAVE[(x & 7) as usize]
    } else {
        11 - WAVE[((x + 3) & 7) as usize]
    }
}

/// Stone crossed by two wavy bands: the upper one in the first palette, the
/// lower one in the second (or the first again), each a lighter lit row
/// above its base row.
fn vein_px(x: i32, y: i32, pal: &Palette) -> Px {
    let host = pal.host.unwrap_or(STONE);
    if edge(x, y) {
        return px(scale(host, DARK));
    }
    for k in 0..2usize {
        let row = vein_row(k, x);
        if y == row || y == row + 1 {
            let gem = pal.nth(k);
            let c = gem.at(x, y);
            return px(if y == row { lit(gem, c) } else { c });
        }
    }
    px(host)
}

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    let plain = |c: [u8; 3]| ore_px(x, y, &Palette::from_rgb(c));
    Some(match name {
        "coal_ore" => plain([38, 38, 52]),
        "iron_ore" => plain([214, 150, 118]),
        "copper_ore" => plain([70, 184, 160]),
        "crystal_ore" => plain([140, 226, 255]),
        "ruby_ore" => plain([236, 62, 116]),
        _ => return None,
    })
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        "ore" => ore_px(x, y, pal),
        "ore_vein" => vein_px(x, y, pal),
        "cluster" => {
            // Three crystal spikes rising from the floor: lit on the left, shaded on the
            // right, a pale pixel just under each tip.
            for (cx, top, half) in [(7, 1, 3), (3, 7, 2), (12, 5, 2)] {
                if y >= top && (x - cx).abs() * (16 - top) <= half * (y - top) {
                    return Some(px(if x == cx && y == top + 1 {
                        pale(rgb)
                    } else if x < cx {
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
