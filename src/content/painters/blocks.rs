//! Plain building blocks: earth, sand, snow, clay, lamps, wool, concrete,
//! terracotta, farmland, magma, chests and workbenches. Flat fills with a
//! one-pixel darker frame, the glint and the shadow in the corners and the
//! grain of each material (house.rs).

use super::*;

pub const DIRT: [u8; 3] = [138, 96, 62];
const SNOW: [u8; 3] = [248, 251, 255];
const SAND: [u8; 3] = [226, 204, 146];

/// Dirt: flecked in short diagonals.
pub fn dirt_px(x: i32, y: i32) -> Px {
    sealed_grain(x, y, DIRT, DARK, if at_any(x, y, &FLECKS) { Some(scale(DIRT, 84)) } else { None })
}

/// How far the snow reaches down the side of a snowy block, column by column: a dripping edge.
const DRIP: [i32; 16] = [4, 4, 5, 6, 4, 4, 5, 4, 3, 5, 6, 5, 4, 4, 5, 4];

pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    Some(match name {
        "dirt" => dirt_px(x, y),
        "sand" => sealed_grain(x, y, SAND, 82, if at_any(x, y, &RIPPLES) { Some(scale(SAND, 91)) } else { None }),
        "clay" => sealed_grain(x, y, [150, 166, 188], 77, if layer(x, y) { Some(scale([150, 166, 188], 90)) } else { None }),
        "snow" => sealed_grain(x, y, SNOW, 90, if at_any(x, y, &[(4, 9), (10, 4), (11, 12), (6, 3)]) { Some([212, 228, 250]) } else { None }),
        "snow_side" => {
            // Snow on top with a dripping lower edge, flecked dirt below.
            if y < DRIP[x as usize & 15] {
                sealed(x, y, SNOW, 90)
            } else {
                dirt_px(x, y)
            }
        }
        "gravel" => {
            // Pebbles: a flat stone with a bump on top, four of them.
            let base = [110, 108, 118];
            let pebble = |bx: i32, by: i32| rect(x, y, bx, by, 3, 1) || (x == bx + 1 && y == by - 1);
            let lit = |bx: i32, by: i32| x == bx + 1 && y == by - 1;
            let at = [(3, 5), (9, 3), (5, 11), (11, 9)];
            if at.iter().any(|&(bx, by)| lit(bx, by)) {
                px(scale(base, 92))
            } else if at.iter().any(|&(bx, by)| pebble(bx, by)) {
                px(scale(base, 76))
            } else {
                sealed(x, y, base, DARK)
            }
        }
        "sandstone" => {
            // Wavy strata: each band steps down by one halfway across.
            let fill = [232, 214, 160];
            let band = (y == 5 && x < 8) || (y == 6 && x >= 8) || (y == 10 && x < 8) || (y == 11 && x >= 8);
            sealed_grain(x, y, fill, 86, if band { Some(scale(fill, 92)) } else { None })
        }
        "lamp" => {
            // An amber frame round a glowing diamond with a brighter core.
            let (dx, dy) = ((2 * x - 15).abs(), (2 * y - 15).abs());
            if edge(x, y) {
                px([124, 80, 40])
            } else if dx + dy <= 5 {
                px([255, 250, 226])
            } else if dx + dy <= 17 {
                px([252, 222, 128])
            } else {
                px([234, 178, 84])
            }
        }
        _ => return None,
    })
}

pub fn paint(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    let rgb = pal.at(x, y);
    Some(match style {
        // Concrete: smooth, with four pores.
        "concrete" => sealed_pal_grain(x, y, pal, rgb, 92, if at_any(x, y, &[(4, 3), (11, 8), (6, 12), (12, 4)]) { Some(scale(rgb, 88)) } else { None }),
        // Wool: rows of stitches.
        "wool" => sealed_pal_grain(x, y, pal, rgb, 85, if stitch(x, y) { Some(scale(rgb, 91)) } else { None }),
        // Terracotta: two short kiln lines.
        "terracotta" => sealed_pal_grain(x, y, pal, rgb, DARK, if (y == 5 || y == 10) && (3..=12).contains(&x) { Some(scale(rgb, 90)) } else { None }),
        "farmland" => {
            // Furrows running slantwise, darker than plain dirt.
            if edge(x, y) || (x + y) % 5 == 4 {
                px(pal.shade(rgb, 70))
            } else {
                px(rgb)
            }
        }
        "magma" => {
            // A dark crust with four glowing blobs of three joined pixels, the first of each lighter.
            let crust = [60, 40, 48];
            for blob in [[(3, 3), (4, 3), (4, 4)], [(10, 4), (10, 5), (11, 5)], [(5, 10), (6, 10), (5, 11)], [(10, 11), (11, 11), (11, 12)]] {
                if let Some(i) = blob.iter().position(|&p| p == (x, y)) {
                    return Some(px(if i == 0 { pal.light_of(rgb) } else { rgb }));
                }
            }
            px(if edge(x, y) { scale(crust, 70) } else { crust })
        }
        "chest" => {
            if edge(x, y) || y == 5 {
                px(pal.shade(rgb, 60))
            } else if rect(x, y, 7, 4, 2, 4) {
                px(pal.accent().unwrap_or([214, 196, 120])) // the latch
            } else if at_any(x, y, &[(2, 2), (13, 2)]) {
                px(pal.accent().unwrap_or([214, 196, 120])) // the rivets of the lid
            } else if y < 5 {
                px(rgb)
            } else {
                px(scale(rgb, 88))
            }
        }
        // The faces of a chest block (data/base.txt: a 14 x 14 footprint, a 7 high
        // body and a 3 high lid, so rows 6..15 of the picture): `chest_front` is
        // the latch side, `chest_side` the other three, `chest_top` the lid and
        // the underside. Each draws its own dark outline; the cells round it
        // are never seen.
        "chest_front" | "chest_side" => {
            if !((1..15).contains(&x) && (6..16).contains(&y)) {
                return Some(CLEAR);
            }
            if x == 1 || x == 14 || y == 6 || y == 15 || y == 9 {
                px(pal.shade(rgb, 60))
            } else if style == "chest_front" && rect(x, y, 7, 7, 2, 5) {
                px(pal.accent().unwrap_or([214, 196, 120])) // the latch, across the lid's seam
            } else if at_any(x, y, &[(3, 7), (12, 7)]) {
                px(pal.accent().unwrap_or([214, 196, 120])) // the rivets of the lid
            } else if y < 9 {
                px(rgb)
            } else {
                px(scale(rgb, 88))
            }
        }
        "chest_top" => {
            if !((1..15).contains(&x) && (1..15).contains(&y)) {
                return Some(CLEAR);
            }
            let ring = ((3..=12).contains(&y) && (x == 3 || x == 12)) || ((3..=12).contains(&x) && (y == 3 || y == 12));
            if x == 1 || x == 14 || y == 1 || y == 14 {
                px(pal.shade(rgb, 60))
            } else if ring {
                px(scale(rgb, 90))
            } else {
                px(rgb)
            }
        }
        "workbench" => {
            // A grid of nine cells, each with a glint in its top left corner.
            if x % 5 == 0 || y % 5 == 0 {
                px(pal.shade(rgb, 60))
            } else if x % 5 == 1 && y % 5 == 1 {
                px(glint(rgb))
            } else {
                px(rgb)
            }
        }
        _ => return None,
    })
}
