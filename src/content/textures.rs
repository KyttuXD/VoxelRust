//! Textures. The game ships no image files: the built-in textures are
//! painted by small procedures at start-up, and packs can add their own as
//! flat colours or 16x16 TGA files.

use crate::block::{Registry, TileDef, TileSrc};
use crate::noise::hash2;
use alloc::vec::Vec;

/// Tile edge in pixels.
pub const TILE: usize = 16;
/// The atlas holds 16 x 16 tiles.
pub const ATLAS: usize = 512;
/// Tiles along one edge of the atlas, and how many it holds.
pub const PER_ROW: usize = ATLAS / TILE;
pub const MAX_TILES: usize = PER_ROW * PER_ROW;

type Px = [u8; 4];

#[inline]
fn h(x: i32, y: i32, salt: u32) -> u32 {
    hash2(salt.wrapping_mul(0x9E37_79B1) ^ 0x51ED_270B, x, y)
}

/// `base` with every channel moved by the same random amount in `-amt..=amt`.
fn grain(base: [u8; 3], amt: i32, x: i32, y: i32, salt: u32) -> Px {
    let d = if amt > 0 { (h(x, y, salt) % (2 * amt as u32 + 1)) as i32 - amt } else { 0 };
    shade(base, d)
}

fn shade(base: [u8; 3], d: i32) -> Px {
    let c = |v: u8| (v as i32 + d).clamp(0, 255) as u8;
    [c(base[0]), c(base[1]), c(base[2]), 255]
}

fn stone(x: i32, y: i32) -> Px {
    let mut p = grain([126, 130, 138], 9, x, y, 1);
    if h(x / 3, y / 2, 2) % 5 == 0 {
        p = shade([p[0], p[1], p[2]], -14);
    }
    p
}

fn dirt(x: i32, y: i32) -> Px {
    let mut p = grain([118, 84, 56], 13, x, y, 3);
    if h(x, y, 4) % 23 == 0 {
        p = shade([p[0], p[1], p[2]], 26);
    }
    p
}

fn grass(x: i32, y: i32) -> Px {
    grain([74, 150, 70], 14, x, y, 5)
}

fn ore(x: i32, y: i32, speck: [u8; 3], salt: u32) -> Px {
    let (bx, by) = (x / 2, y / 2);
    if h(bx, by, salt) % 5 == 0 && (1..7).contains(&bx) && (1..7).contains(&by) {
        grain(speck, 12, x, y, salt + 1)
    } else {
        stone(x, y)
    }
}

fn bricks(x: i32, y: i32, w: i32, hgt: i32, body: Px, mortar: Px) -> Px {
    let row = y / hgt;
    let off = if row % 2 == 0 { 0 } else { w / 2 };
    if y % hgt == hgt - 1 || (x + off) % w == w - 1 {
        mortar
    } else {
        body
    }
}

/// One pixel of the water texture at a moment in its eight-step cycle:
/// bands of lighter ripples that travel across the tile, so rivers and seas
/// look as if they move. The pattern wraps, so tiles join without a seam.
pub fn water_px(x: i32, y: i32, phase: i32) -> Px {
    let wobble = (h(x / 4, 0, 10) % 3) as i32;
    let ripple = (y + wobble + phase).rem_euclid(8) == 0;
    // A second, fainter set drifting the other way gives a restless look.
    let glint = (x * 2 + y - phase * 2).rem_euclid(16) == 0 && h(x / 2, y / 2, 12) % 3 == 0;
    let c = if ripple { [74, 146, 226] } else if glint { [96, 168, 236] } else { [44, 104, 198] };
    let p = grain(c, 5, x, (y + phase / 2).rem_euclid(16), 11);
    [p[0], p[1], p[2], 168]
}

/// Lava: molten orange, with darker crust in drifting blobs and a few hot spots.
pub fn lava_px(x: i32, y: i32) -> Px {
    let crust = h(x / 3, y / 3, 14) % 4 == 0;
    let c = if crust {
        [92, 30, 16]
    } else if h(x, y, 15) % 5 == 0 {
        [255, 196, 64]
    } else {
        [226, 88, 24]
    };
    let p = grain(c, 4, x, y, 16);
    [p[0], p[1], p[2], 255]
}

/// A whole frame of [`water_px`], 16 x 16 RGBA.
pub fn water_frame(phase: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(TILE * TILE * 4);
    for y in 0..TILE as i32 {
        for x in 0..TILE as i32 {
            out.extend_from_slice(&water_px(x, y, phase));
        }
    }
    out
}

fn disc(x: i32, y: i32, r2: i32) -> bool {
    let (dx, dy) = (2 * x - 15, 2 * y - 15);
    dx * dx + dy * dy <= r2 * 4
}

/// One pixel of a built-in texture; `None` if there is no such painter.
pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    const CLEAR: Px = [0, 0, 0, 0];
    Some(match name {
        "stone" => stone(x, y),
        "dirt" => dirt(x, y),
        "grass_top" => grass(x, y),
        "grass_side" => {
            if y < 3 + (h(x, 0, 6) % 3) as i32 {
                grass(x, y)
            } else {
                dirt(x, y)
            }
        }
        "sand" => grain([220, 204, 152], 8, x, y, 7),
        "gravel" => {
            let p = grain([128, 124, 122], 16, x / 2, (y + x / 2) / 2, 8);
            grain([p[0], p[1], p[2]], 8, x, y, 9)
        }
        "water" => water_px(x, y, 0),
        "lava" => lava_px(x, y),
        "log_side" | "pine_side" => {
            let base = if name == "log_side" { [106, 80, 48] } else { [76, 54, 38] };
            let stripe = (h(x, 0, 12) % 19) as i32 - 9;
            let p = shade(base, stripe);
            grain([p[0], p[1], p[2]], 5, x, y, 13)
        }
        "log_top" => {
            let d = core::cmp::max((2 * x - 15).abs(), (2 * y - 15).abs()) / 2;
            if d >= 7 {
                grain([96, 72, 44], 6, x, y, 14)
            } else if d % 2 == 0 {
                grain([190, 152, 96], 5, x, y, 15)
            } else {
                grain([164, 126, 76], 5, x, y, 15)
            }
        }
        "leaves" | "pine_leaves" => {
            let (base, holes) = if name == "leaves" { ([48, 124, 54], 6) } else { ([30, 92, 66], 9) };
            if h(x, y, 16) % holes == 0 {
                CLEAR
            } else {
                grain(base, 20, x, y, 17)
            }
        }
        "planks" => {
            if y % 4 == 3 {
                grain([128, 96, 56], 5, x, y, 18)
            } else if (x + (y / 4) * 5) % 16 == 0 {
                grain([140, 106, 62], 4, x, y, 18)
            } else {
                let p = shade([180, 140, 86], (h(0, y / 4, 19) % 13) as i32 - 6);
                grain([p[0], p[1], p[2]], 5, x, y, 20)
            }
        }
        "cobble" => {
            let (cx, cy) = ((x + (y / 4 % 2) * 2) / 4, y / 4);
            let edge = (x + (y / 4 % 2) * 2) % 4 == 0 || y % 4 == 0;
            let tone = (h(cx, cy, 21) % 37) as i32 - 18;
            let p = shade([124, 124, 128], if edge { -34 } else { tone });
            grain([p[0], p[1], p[2]], 6, x, y, 22)
        }
        "brick" => bricks(x, y, 8, 4, grain([160, 74, 58], 9, x, y, 23), grain([202, 196, 184], 5, x, y, 24)),
        "stone_bricks" => {
            let body = grain([134, 136, 142], 6, x, y, 25);
            bricks(x, y, 16, 8, body, grain([88, 90, 96], 4, x, y, 26))
        }
        "glass" => {
            if x == 0 || y == 0 || x == 15 || y == 15 {
                [206, 232, 238, 255]
            } else if (x + y == 6 || x + y == 7) && x > 1 && y > 1 {
                [236, 248, 250, 255]
            } else {
                CLEAR
            }
        }
        "coal_ore" => ore(x, y, [38, 38, 44], 30),
        "iron_ore" => ore(x, y, [204, 158, 120], 32),
        "copper_ore" => ore(x, y, [84, 176, 150], 34),
        "crystal_ore" => ore(x, y, [150, 236, 250], 36),
        "ruby_ore" => ore(x, y, [232, 66, 122], 58),
        "snow" => grain([238, 243, 250], 5, x, y, 38),
        "snow_side" => {
            if y < 4 + (h(x, 0, 39) % 2) as i32 {
                grain([238, 243, 250], 5, x, y, 38)
            } else {
                dirt(x, y)
            }
        }
        "ice" => {
            let streak = (x + y * 3) % 11 == 0;
            grain(if streak { [196, 226, 248] } else { [150, 194, 236] }, 5, x, y, 40)
        }
        "sandstone" => {
            let band = (h(0, y / 3, 41) % 17) as i32 - 8;
            let p = shade([212, 192, 138], band);
            grain([p[0], p[1], p[2]], 4, x, y, 42)
        }
        "cactus_side" => {
            if x % 4 == 1 {
                if y % 5 == 2 {
                    [224, 228, 190, 255]
                } else {
                    grain([42, 112, 58], 5, x, y, 43)
                }
            } else {
                grain([62, 144, 74], 7, x, y, 44)
            }
        }
        "cactus_top" => {
            if disc(x, y, 5) {
                grain([96, 170, 96], 6, x, y, 45)
            } else {
                grain([62, 144, 74], 7, x, y, 44)
            }
        }
        "lamp" => {
            if x == 0 || y == 0 || x == 15 || y == 15 {
                grain([120, 88, 48], 6, x, y, 46)
            } else if disc(x, y, 4) {
                [255, 252, 224, 255]
            } else {
                grain([250, 214, 120], 8, x, y, 47)
            }
        }
        "flower_red" | "flower_yellow" => {
            let petal = if name == "flower_red" { [214, 52, 60] } else { [240, 204, 48] };
            let (dx, dy) = (2 * x - 15, 2 * y - 9);
            let d = dx * dx + dy * dy;
            if d <= 4 {
                [70, 48, 30, 255]
            } else if d <= 40 {
                grain(petal, 10, x, y, 48)
            } else if (x == 7 || x == 8) && y > 6 {
                grain([52, 128, 52], 8, x, y, 49)
            } else if y > 10 && (x == 6 || x == 9) && y < 13 {
                grain([52, 128, 52], 8, x, y, 49)
            } else {
                CLEAR
            }
        }
        "tall_grass" => {
            let height = 5 + (h(x, 0, 50) % 9) as i32;
            if x % 2 == (h(x / 2, 0, 51) % 2) as i32 && y >= 16 - height {
                grain([84, 160, 72], 16, x, y, 52)
            } else {
                CLEAR
            }
        }
        "deepstone" => {
            let crack = h(x / 2, y, 53) % 9 == 0;
            grain(if crack { [28, 26, 34] } else { [54, 52, 62] }, 7, x, y, 54)
        }
        "clay" => grain([156, 162, 176], 5, x, y, 55),
        "sun" => {
            if disc(x, y, 6) {
                [255, 246, 196, 255]
            } else if disc(x, y, 8) {
                [255, 224, 140, 120]
            } else {
                CLEAR
            }
        }
        "moon" => {
            if disc(x, y, 6) {
                let crater = h(x / 2, y / 2, 56) % 6 == 0;
                if crater {
                    [180, 188, 206, 255]
                } else {
                    [224, 230, 242, 255]
                }
            } else {
                CLEAR
            }
        }
        "wand" => {
            // A diagonal rod with a bright tip.
            let d = x + y - 15;
            if d.abs() <= 1 && (3..13).contains(&x) {
                grain([120, 84, 50], 8, x, y, 57)
            } else if (11..15).contains(&x) && (1..5).contains(&y) {
                [140, 232, 250, 255]
            } else {
                CLEAR
            }
        }
        _ => return None,
    })
}

/// One pixel of a built-in pattern drawn in the colour `rgb`; `None` if
/// there is no such style. These let data files make whole families of
/// blocks (sixteen colours of wool, five kinds of wood) from one line each.
pub fn painted(style: &str, rgb: [u8; 3], x: i32, y: i32) -> Option<Px> {
    let salt = 200 + (rgb[0] as u32) * 3 + (rgb[1] as u32) * 5 + (rgb[2] as u32) * 7;
    let dark = |c: [u8; 3], pct: i32| -> [u8; 3] { [0, 1, 2].map(|i| (c[i] as i32 * pct / 100).clamp(0, 255) as u8) };
    // Bows and crossbows, at rest and drawn, are in the OCD style (content/art/bow.rs).
    if let Some(p) = crate::content::art::bow::painted(style, rgb, x, y) {
        return Some(p);
    }
    Some(match style {
        "concrete" => grain(rgb, 4, x, y, salt),
        "wool" => {
            let weave = if (x + y) % 4 == 0 {
                9
            } else if (x - y).rem_euclid(4) == 0 {
                -9
            } else {
                0
            };
            let p = shade(rgb, weave);
            grain([p[0], p[1], p[2]], 5, x, y, salt)
        }
        "terracotta" => {
            let band = (h(0, y / 3, salt) % 13) as i32 - 6;
            let p = shade(rgb, band);
            grain([p[0], p[1], p[2]], 3, x, y, salt + 1)
        }
        "glass" => {
            if x == 0 || y == 0 || x == 15 || y == 15 {
                let p = shade(rgb, 40);
                [p[0], p[1], p[2], 235]
            } else if (x + y == 6 || x + y == 7) && x > 1 && y > 1 {
                let p = shade(rgb, 70);
                [p[0], p[1], p[2], 170]
            } else {
                [rgb[0], rgb[1], rgb[2], 105]
            }
        }
        "rock" => {
            let mut p = grain(rgb, 9, x, y, salt);
            if h(x / 3, y / 2, salt + 1) % 5 == 0 {
                p = shade([p[0], p[1], p[2]], -14);
            }
            p
        }
        "polished" => {
            if x == 0 || y == 0 {
                grain(dark(rgb, 82), 3, x, y, salt)
            } else {
                grain(rgb, 4, x, y, salt)
            }
        }
        "bricks" => bricks(x, y, 8, 4, grain(rgb, 8, x, y, salt), grain(dark(rgb, 70), 4, x, y, salt + 1)),
        "tiles" => bricks(x, y, 8, 8, grain(rgb, 5, x, y, salt), grain(dark(rgb, 72), 3, x, y, salt + 1)),
        "planks" => {
            if y % 4 == 3 {
                grain(dark(rgb, 70), 5, x, y, salt)
            } else if (x + (y / 4) * 5) % 16 == 0 {
                grain(dark(rgb, 78), 4, x, y, salt)
            } else {
                let p = shade(rgb, (h(0, y / 4, salt + 1) % 13) as i32 - 6);
                grain([p[0], p[1], p[2]], 5, x, y, salt + 2)
            }
        }
        "bark" => {
            let p = shade(rgb, (h(x, 0, salt) % 19) as i32 - 9);
            let mut p = grain([p[0], p[1], p[2]], 5, x, y, salt + 1);
            // Pale-barked trees get dark flecks.
            if rgb[0] > 180 && h(x / 3, y / 2, salt + 2) % 7 == 0 {
                p = shade([p[0], p[1], p[2]], -90);
            }
            p
        }
        "rings" => {
            let d = core::cmp::max((2 * x - 15).abs(), (2 * y - 15).abs()) / 2;
            if d >= 7 {
                grain(dark(rgb, 55), 6, x, y, salt)
            } else if d % 2 == 0 {
                grain(rgb, 5, x, y, salt)
            } else {
                grain(dark(rgb, 86), 5, x, y, salt)
            }
        }
        "leaves" | "blossom" => {
            let roll = h(x, y, salt);
            if roll % 6 == 0 {
                [0, 0, 0, 0]
            } else if style == "blossom" && roll % 5 == 1 {
                grain([255, 240, 246], 8, x, y, salt)
            } else {
                grain(rgb, 20, x, y, salt + 1)
            }
        }
        // ---- Item pictures: a shape in the given colour on a clear ground.
        "stick" | "pick" | "axe" | "shovel" | "hoe" | "sword" => {
            let handle = (x + y - 15).abs() <= 1 && (2..12).contains(&x);
            let wood = grain([122, 88, 52], 8, x, y, salt);
            let metal = |x: i32, y: i32| grain(rgb, 10, x, y, salt + 3);
            match style {
                "stick" if handle => wood,
                "pick" => {
                    // A curved head across the top-right end of the handle.
                    let (dx, dy) = (x - 6, y - 9);
                    let d2 = dx * dx + dy * dy;
                    if (40..=64).contains(&d2) && x >= 6 && y <= 9 {
                        metal(x, y)
                    } else if handle {
                        wood
                    } else {
                        CLEAR_PX
                    }
                }
                "axe" => {
                    if (9..14).contains(&x) && (1..7).contains(&y) && x + y >= 11 && !(x == 13 && y == 1) {
                        metal(x, y)
                    } else if handle {
                        wood
                    } else {
                        CLEAR_PX
                    }
                }
                "hoe" => {
                    // A flat blade bent back from the end of the handle.
                    if (8..14).contains(&x) && (2..4).contains(&y) || (8..10).contains(&x) && (4..7).contains(&y) {
                        metal(x, y)
                    } else if handle {
                        wood
                    } else {
                        CLEAR_PX
                    }
                }
                "sword" => {
                    // A long blade from the guard to the top-right corner.
                    let blade = (x + y - 15).abs() <= 1 && (6..15).contains(&x);
                    let guard = (x - y + 5).abs() <= 3 && (x + y - 15).abs() <= 1 + (x == 5 || y == 10) as i32 && (3..8).contains(&x);
                    if blade {
                        let p = metal(x, y);
                        if x + y == 14 { shade([p[0], p[1], p[2]], 30) } else { p }
                    } else if (x - 4).abs() <= 2 && (y - 11).abs() <= 2 && (x - y + 7).abs() <= 3 && (x + y - 15).abs() <= 2 {
                        let _ = guard;
                        grain(dark(rgb, 60), 6, x, y, salt)
                    } else if (x + y - 15).abs() <= 1 && (1..4).contains(&x) {
                        wood
                    } else {
                        CLEAR_PX
                    }
                }
                "shovel" => {
                    let (dx, dy) = (2 * x - 23, 2 * y - 7);
                    if dx * dx + dy * dy <= 30 {
                        metal(x, y)
                    } else if handle {
                        wood
                    } else {
                        CLEAR_PX
                    }
                }
                _ => CLEAR_PX,
            }
        }
        "gem" => {
            let d = (2 * x - 15).abs() + (2 * y - 15).abs();
            if d <= 11 {
                let p = shade(rgb, if x + y < 14 { 46 } else { -18 });
                grain([p[0], p[1], p[2]], 4, x, y, salt)
            } else if d <= 13 {
                let p = dark(rgb, 55);
                [p[0], p[1], p[2], 255]
            } else {
                CLEAR_PX
            }
        }
        "meat" => {
            let (dx, dy) = (2 * x - 13, 2 * y - 17);
            if dx * dx * 2 + dy * dy * 3 <= 150 {
                grain(rgb, 10, x, y, salt)
            } else if (11..15).contains(&x) && (2..6).contains(&y) && (x - y - 9).abs() <= 1 {
                [236, 230, 214, 255] // the bone
            } else {
                CLEAR_PX
            }
        }
        "fruit" => {
            let (dx, dy) = (2 * x - 15, 2 * y - 19);
            if dx * dx + dy * dy <= 90 {
                let p = shade(rgb, if dx < -2 && dy < 0 { 40 } else { 0 });
                grain([p[0], p[1], p[2]], 6, x, y, salt)
            } else if (x == 7 || x == 8) && (2..5).contains(&y) {
                [96, 66, 36, 255]
            } else if (9..12).contains(&x) && (2..4).contains(&y) {
                [72, 150, 60, 255]
            } else {
                CLEAR_PX
            }
        }
        "fishfood" => {
            let (dx, dy) = (2 * x - 13, 2 * y - 15);
            if dx * dx + dy * dy * 4 <= 110 {
                if x == 4 && y == 7 {
                    [20, 20, 24, 255]
                } else {
                    grain(rgb, 10, x, y, salt)
                }
            } else if (12..15).contains(&x) && (y - 7).abs() <= x - 11 {
                grain(dark(rgb, 75), 8, x, y, salt)
            } else {
                CLEAR_PX
            }
        }
        "rail" => {
            // Two rails across wooden sleepers, on a clear ground.
            if x == 3 || x == 12 {
                grain(rgb, 8, x, y, salt)
            } else if y % 4 == 1 && (1..15).contains(&x) {
                grain([112, 82, 50], 8, x, y, salt)
            } else {
                CLEAR_PX
            }
        }
        "machine" => {
            // A generic "thing with wheels" picture.
            let body = (5..11).contains(&y) && (1..15).contains(&x);
            let cabin = (2..5).contains(&y) && (5..11).contains(&x);
            let wheel = |cx: i32| (2 * x - cx).pow(2) + (2 * y - 23).pow(2) <= 14;
            if wheel(8) || wheel(23) {
                [30, 30, 34, 255]
            } else if body || cabin {
                grain(rgb, 8, x, y, salt)
            } else {
                CLEAR_PX
            }
        }
        "hull" => {
            let d = (2 * x - 15).abs();
            if (7..12).contains(&y) && d <= 15 - (y - 7) * 2 {
                grain(rgb, 8, x, y, salt)
            } else if x == 7 && (1..7).contains(&y) {
                [96, 70, 40, 255]
            } else if (8..13).contains(&x) && (1..6).contains(&y) && x - 8 <= 5 - y {
                [236, 232, 220, 255]
            } else {
                CLEAR_PX
            }
        }
        "wings" => {
            let fuselage = (6..10).contains(&y) && (1..15).contains(&x);
            let wing = (6..10).contains(&x) && (1..15).contains(&y);
            let tail = (12..15).contains(&x) && (4..12).contains(&y);
            if fuselage || wing || tail {
                grain(rgb, 8, x, y, salt)
            } else {
                CLEAR_PX
            }
        }
        // ---- Furniture faces.
        "chest" => {
            if x == 0 || y == 0 || x == 15 || y == 15 || y == 5 {
                grain(dark(rgb, 55), 4, x, y, salt)
            } else if (7..9).contains(&x) && (4..8).contains(&y) {
                [214, 196, 120, 255] // the latch
            } else {
                let p = shade(rgb, (h(0, y / 4, salt + 1) % 11) as i32 - 5);
                grain([p[0], p[1], p[2]], 5, x, y, salt + 2)
            }
        }
        "workbench" => {
            if x % 5 == 0 || y % 5 == 0 {
                grain(dark(rgb, 60), 4, x, y, salt)
            } else {
                grain(rgb, 6, x, y, salt)
            }
        }
        // ---- Things that grow in caves.
        "mushroom" => {
            // A domed cap in the given colour on a pale stem.
            let (dx, dy) = (2 * x - 15, 2 * y - 13);
            if (4..9).contains(&y) && dx * dx + dy * dy * 3 <= 150 {
                if h(x / 2, y / 2, salt) % 5 == 0 { grain(shade(rgb, 60)[..3].try_into().unwrap_or(rgb), 4, x, y, salt) } else { grain(rgb, 8, x, y, salt) }
            } else if (9..16).contains(&y) && (7..9).contains(&x) {
                grain([228, 220, 200], 6, x, y, salt)
            } else {
                CLEAR_PX
            }
        }
        "cluster" => {
            // Three crystal spikes rising from the floor.
            let spike = |cx: i32, top: i32, half: i32| y >= top && (x - cx).abs() * (16 - top) <= half * (y - top);
            if spike(8, 2, 3) || spike(4, 7, 2) || spike(12, 6, 2) {
                let p = shade(rgb, if (x + y) % 3 == 0 { 50 } else { -10 });
                grain([p[0], p[1], p[2]], 6, x, y, salt)
            } else {
                CLEAR_PX
            }
        }
        "icicle" => {
            // Spikes hanging from the ceiling.
            let spike = |cx: i32, len: i32, half: i32| y < len && (x - cx).abs() * len <= half * (len - y);
            if spike(4, 11, 2) || spike(9, 15, 3) || spike(13, 8, 2) {
                let p = shade(rgb, if x % 3 == 0 { 30 } else { 0 });
                [p[0], p[1], p[2], 215]
            } else {
                CLEAR_PX
            }
        }
        "magma" => {
            // Dark crust split by glowing seams in the given colour.
            let seam = (h(x / 4, y / 4, salt) % 4 == 0 && (x % 4 == 1 || y % 4 == 2)) || (x + y * 2 + (h(x / 5, 0, salt) % 3) as i32) % 11 == 0;
            if seam {
                grain(rgb, 16, x, y, salt)
            } else {
                grain([52, 38, 40], 8, x, y, salt + 1)
            }
        }
        "vine" => {
            // Strands hanging down, with a few bright berries.
            let strand = (x == 3 || x == 8 || x == 12) && y < 9 + (h(x, 0, salt) % 7) as i32;
            if strand && h(x, y, salt + 2) % 6 == 0 {
                [250, 214, 110, 255]
            } else if strand || ((x == 4 || x == 9 || x == 11) && y % 4 == 1 && y < 10) {
                grain(rgb, 14, x, y, salt)
            } else {
                CLEAR_PX
            }
        }
        "web" => {
            // Spokes and rings of thread.
            let (dx, dy) = (x - 8, y - 8);
            let spoke = dx == 0 || dy == 0 || dx == dy || dx == -dy;
            let ring = matches!(dx.abs().max(dy.abs()), 3 | 6) && (dx.abs() + dy.abs()) % 2 == 0;
            if spoke || ring {
                [rgb[0], rgb[1], rgb[2], 220]
            } else {
                CLEAR_PX
            }
        }
        "glassdoor" => {
            // A framed door whose upper half is panes of glass.
            let frame = x == 0 || x == 15 || y == 0 || y == 15 || y == 8 || x == 7 || x == 8;
            if x == 12 && (y == 10 || y == 11) {
                [222, 200, 110, 255]
            } else if frame {
                grain(dark(rgb, 62), 4, x, y, salt)
            } else if y < 8 {
                // Clear, with a glint.
                if (x + y) % 7 == 0 { [220, 240, 250, 255] } else { CLEAR_PX }
            } else {
                grain(rgb, 6, x, y, salt + 2)
            }
        }
        "bow" | "crossbow" => {
            // A curved stave and its string; the crossbow adds a stock.
            let (dx, dy) = (x - 3, y - 12);
            let d2 = dx * dx + dy * dy;
            let stave = (100..=130).contains(&d2) && x >= 3 && y <= 12;
            let string = x + y == 15 && (3..=12).contains(&x);
            let stock = style == "crossbow" && (x - y).abs() <= 1 && (3..14).contains(&x);
            if stock {
                grain(dark(rgb, 70), 6, x, y, salt)
            } else if stave {
                grain(rgb, 8, x, y, salt)
            } else if string {
                [236, 236, 230, 255]
            } else {
                CLEAR_PX
            }
        }
        "arrow" => {
            // A shaft from bottom-left to a head at top-right, fletched.
            let shaft = x + y == 15 && (2..14).contains(&x);
            let head = (12..15).contains(&x) && (1..4).contains(&y) && x + y >= 14;
            let feather = (1..4).contains(&x) && (11..15).contains(&y) && (x + y - 15).abs() <= 2;
            if head {
                grain(rgb, 8, x, y, salt)
            } else if shaft {
                grain([150, 112, 70], 6, x, y, salt)
            } else if feather {
                [236, 236, 240, 255]
            } else {
                CLEAR_PX
            }
        }
        "ingot" => {
            // A bar seen from a corner: lit top, shaded front.
            if (5..9).contains(&y) && (2 + (8 - y)..12 + (8 - y)).contains(&x) {
                let p = shade(rgb, 34);
                grain([p[0], p[1], p[2]], 4, x, y, salt)
            } else if (9..12).contains(&y) && (2..12).contains(&x) {
                grain(dark(rgb, 78), 5, x, y, salt)
            } else if (9..12).contains(&y) && (12..12 + (12 - y)).contains(&x) {
                grain(dark(rgb, 60), 4, x, y, salt)
            } else {
                CLEAR_PX
            }
        }
        "door" => {
            // A framed door of two panels with a small handle.
            let frame = x == 0 || x == 15 || y == 0 || y == 15 || y == 8;
            let panel = (3..13).contains(&x) && ((3..6).contains(&y) || (11..13).contains(&y));
            if x == 12 && y == 9 || x == 12 && y == 10 {
                [222, 200, 110, 255]
            } else if frame {
                grain(dark(rgb, 62), 4, x, y, salt)
            } else if panel {
                grain(dark(rgb, 82), 5, x, y, salt + 1)
            } else {
                grain(rgb, 6, x, y, salt + 2)
            }
        }
        "farmland" => {
            // Furrows running one way, darker and damper than plain dirt.
            if y % 4 == 0 {
                grain(dark(rgb, 62), 6, x, y, salt)
            } else if y % 4 == 2 {
                grain(shade(rgb, 12)[..3].try_into().unwrap_or(rgb), 8, x, y, salt + 1)
            } else {
                grain(rgb, 9, x, y, salt + 2)
            }
        }
        "crop1" | "crop2" | "crop3" | "crop4" => {
            // Rows of stalks that get taller; the last stage carries the
            // crop's own colour on top.
            let stage = (style.as_bytes()[4] - b'0') as i32;
            let top = 16 - stage * 4 + (h(x, 0, salt) % 3) as i32 - 1;
            let stalk = x % 3 != 2 && (h(x / 3, 1, salt) % 5 != 0);
            if !stalk || y < top {
                CLEAR_PX
            } else if stage == 4 && y < top + 4 {
                grain(rgb, 14, x, y, salt)
            } else {
                let green = [64 + stage as u8 * 12, 150 - stage as u8 * 6, 52];
                grain(green, 14, x, y, salt + 3)
            }
        }
        "seeds" => {
            // A scatter of small grains.
            let cell = h(x / 3, y / 3, salt) % 4 == 0;
            if cell && (4..13).contains(&x) && (5..13).contains(&y) && x % 3 != 2 && y % 3 != 2 {
                grain(rgb, 16, x, y, salt)
            } else {
                CLEAR_PX
            }
        }
        "sheaf" => {
            // A tied bundle: stalks fanning out above and below the band.
            let spread = (y - 8).abs() / 3 + 2;
            if (x - 8).abs() <= spread && (1..15).contains(&y) {
                if y == 8 || y == 9 {
                    grain(dark(rgb, 55), 4, x, y, salt)
                } else if y < 5 {
                    grain(shade(rgb, 26)[..3].try_into().unwrap_or(rgb), 12, x, y, salt)
                } else {
                    grain(rgb, 14, x + (h(x, 0, salt) % 2) as i32, y, salt)
                }
            } else {
                CLEAR_PX
            }
        }
        "loaf" => {
            // A rounded loaf with scored top.
            let (dx, dy) = (2 * x - 15, 2 * y - 17);
            if dx * dx + dy * dy * 3 <= 190 {
                if y <= 6 && (x + 1) % 4 == 0 && (4..12).contains(&x) {
                    grain(shade(rgb, 40)[..3].try_into().unwrap_or(rgb), 4, x, y, salt)
                } else {
                    let p = shade(rgb, if y > 9 { -24 } else { 0 });
                    grain([p[0], p[1], p[2]], 8, x, y, salt)
                }
            } else {
                CLEAR_PX
            }
        }
        "pizza" => {
            // A round pie: crust, sauce in the given colour, cheese, toppings.
            let (dx, dy) = (2 * x - 15, 2 * y - 15);
            let d2 = dx * dx + dy * dy;
            if d2 > 210 {
                CLEAR_PX
            } else if d2 > 150 {
                grain([214, 164, 96], 10, x, y, salt)
            } else if h(x / 2, y / 2, salt) % 6 == 0 {
                grain(dark(rgb, 70), 10, x, y, salt)
            } else if h(x, y, salt + 9) % 4 == 0 {
                grain(rgb, 10, x, y, salt)
            } else {
                grain([246, 216, 120], 12, x, y, salt)
            }
        }
        "wedge" => {
            // A wedge with holes (cheese), or a slice of anything.
            if (4..14).contains(&y) && x >= 1 && x <= 14 && (x - 1) * 10 >= (13 - y) * 13 {
                if h(x / 2, y / 2, salt) % 7 == 0 && y > 6 {
                    grain(dark(rgb, 78), 4, x, y, salt)
                } else {
                    let p = shade(rgb, if y < 6 { 22 } else { 0 });
                    grain([p[0], p[1], p[2]], 6, x, y, salt)
                }
            } else {
                CLEAR_PX
            }
        }
        "root" => {
            // A tapering root with a tuft of leaves (carrot, beet).
            let w = (y - 3) / 3;
            let cx = 12 - y * 2 / 3;
            if (4..15).contains(&y) && (x - cx).abs() <= 3 - w.min(3) {
                grain(rgb, 12, x, y, salt)
            } else if (1..5).contains(&y) && (x - 11).abs() <= 2 && (x + y) % 2 == 0 {
                [70, 156, 60, 255]
            } else {
                CLEAR_PX
            }
        }
        "tuber" => {
            // A lumpy oval (potato), with a few eyes.
            let (dx, dy) = (2 * x - 15, 2 * y - 16);
            if dx * dx * 2 + dy * dy * 3 <= 330 + (h(x / 4, y / 4, salt) % 60) as i32 {
                if h(x, y, salt + 5) % 13 == 0 {
                    grain(dark(rgb, 70), 4, x, y, salt)
                } else {
                    grain(rgb, 9, x, y, salt)
                }
            } else {
                CLEAR_PX
            }
        }
        "jar" => {
            // A pail or bottle of something, the contents in the given colour.
            let body = (4..12).contains(&x) && (5..15).contains(&y);
            if body && (x == 4 || x == 11 || y == 14) {
                [150, 154, 164, 255]
            } else if body && y >= 7 {
                grain(rgb, 4, x, y, salt)
            } else if body || (y == 3 || y == 4) && (5..11).contains(&x) && (x == 5 || x == 10 || y == 3) {
                [196, 200, 210, if body { 90 } else { 255 }]
            } else {
                CLEAR_PX
            }
        }
        "egg" => {
            let (dx, dy) = (2 * x - 15, 2 * y - 17);
            let wide = if dy < 0 { 4 } else { 3 };
            if dx * dx * wide + dy * dy * 2 <= 260 {
                let p = shade(rgb, if dx < 0 && dy < 0 { 16 } else if dy > 6 { -22 } else { 0 });
                grain([p[0], p[1], p[2]], 3, x, y, salt)
            } else {
                CLEAR_PX
            }
        }
        "sack" => {
            // A tied sack (flour, sugar), a band of the given colour on it.
            let (dx, dy) = (2 * x - 15, 2 * y - 19);
            if dx * dx + dy * dy <= 120 {
                if (9..11).contains(&y) {
                    grain(rgb, 6, x, y, salt)
                } else {
                    grain([222, 210, 184], 8, x, y, salt)
                }
            } else if (6..10).contains(&x) && (1..4).contains(&y) {
                grain([200, 186, 156], 6, x, y, salt)
            } else {
                CLEAR_PX
            }
        }
        "cookie" => {
            let (dx, dy) = (2 * x - 15, 2 * y - 15);
            if dx * dx + dy * dy <= 150 {
                if h(x / 2, y / 2, salt) % 5 == 0 {
                    [70, 44, 30, 255]
                } else {
                    grain(rgb, 8, x, y, salt)
                }
            } else {
                CLEAR_PX
            }
        }
        "pie" => {
            // Seen from the side: a filled crust in a tin.
            if (9..13).contains(&y) && (x - 8).abs() <= 7 - (y - 9) {
                grain([206, 160, 100], 8, x, y, salt)
            } else if (5..9).contains(&y) && (2 * x - 15).abs() <= 13 - (8 - y) * 3 {
                if y == 5 || (x + y) % 3 == 0 { grain([222, 178, 112], 8, x, y, salt) } else { grain(rgb, 10, x, y, salt) }
            } else {
                CLEAR_PX
            }
        }
        "bowl" => {
            // A bowl of stew or soup.
            let (dx, dy) = (2 * x - 15, 2 * y - 14);
            if (7..9).contains(&y) && (x - 8).abs() <= 6 {
                grain(rgb, 12, x, y, salt)
            } else if y >= 9 && dx * dx + dy * dy * 4 <= 200 {
                grain([150, 108, 70], 6, x, y, salt)
            } else {
                CLEAR_PX
            }
        }
        _ => return None,
    })
}

const CLEAR_PX: Px = [0, 0, 0, 0];

/// Decode an uncompressed true-colour TGA of exactly 16 x 16 pixels (24 or
/// 32 bits) into RGBA, top row first.
pub fn decode_tga(d: &[u8]) -> Option<Vec<u8>> {
    if d.len() < 18 || d[1] != 0 || d[2] != 2 {
        return None;
    }
    let id_len = d[0] as usize;
    let (w, hgt) = (u16::from_le_bytes([d[12], d[13]]) as usize, u16::from_le_bytes([d[14], d[15]]) as usize);
    let bpp = d[16] as usize / 8;
    if w != TILE || hgt != TILE || (bpp != 3 && bpp != 4) {
        return None;
    }
    let top_down = d[17] & 0x20 != 0;
    let data = d.get(18 + id_len..18 + id_len + w * hgt * bpp)?;
    let mut out = alloc::vec![0u8; w * hgt * 4];
    for row in 0..hgt {
        let src_row = if top_down { row } else { hgt - 1 - row };
        for x in 0..w {
            let s = (src_row * w + x) * bpp;
            let o = (row * w + x) * 4;
            out[o] = data[s + 2];
            out[o + 1] = data[s + 1];
            out[o + 2] = data[s];
            out[o + 3] = if bpp == 4 { data[s + 3] } else { 255 };
        }
    }
    Some(out)
}

/// Where pictures the player has edited are looked for (`<id>.png`, `.jpg`, `.gif`, `.bmp` or `.tga`),
/// set once by the game when it starts.
static OVERRIDE_DIR: core::sync::atomic::AtomicPtr<alloc::string::String> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

pub fn set_override_dir(dir: &str) {
    let p = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(alloc::string::String::from(dir)));
    OVERRIDE_DIR.store(p, core::sync::atomic::Ordering::Release);
    rescan_overrides();
}

fn override_dir() -> Option<&'static str> {
    let p = OVERRIDE_DIR.load(core::sync::atomic::Ordering::Acquire);
    // SAFETY: set once from a leaked box and never freed.
    if p.is_null() { None } else { Some(unsafe { &*p }.as_str()) }
}

/// The files in the textures folder when the atlas was last built (the old
/// lists are kept for good: a few names each time packs are reloaded).
static OVERRIDE_FILES: core::sync::atomic::AtomicPtr<Vec<alloc::string::String>> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// Look in the textures folder again (when the atlas is rebuilt).
pub fn rescan_overrides() {
    let Some(dir) = override_dir() else { return };
    let files: Vec<alloc::string::String> = crate::sys::list_dir(dir);
    OVERRIDE_FILES.store(alloc::boxed::Box::into_raw(alloc::boxed::Box::new(files)), core::sync::atomic::Ordering::Release);
}

fn override_files() -> &'static [alloc::string::String] {
    let p = OVERRIDE_FILES.load(core::sync::atomic::Ordering::Acquire);
    // SAFETY: from a leaked box, never freed.
    if p.is_null() { &[] } else { unsafe { &*p } }
}

/// Texture packs: folders of PNG pictures named as Minecraft-style packs name
/// them (`assets/<namespace>/textures/block/stone.png` and `.../item/...`, or
/// just a folder of pictures). A texture the game has an id for is taken from
/// a pack when the pack has a picture it matches (see [`pack_names`]).
struct PackIndex {
    /// `(file stem, full path)`; later packs come later and win.
    files: Vec<(alloc::string::String, alloc::string::String)>,
}

static PACKS: core::sync::atomic::AtomicPtr<PackIndex> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// Use these packs (folders), in order, and index their pictures. Returns how many pictures were found.
pub fn set_texture_packs(paths: &[alloc::string::String]) -> usize {
    let mut files = Vec::new();
    for root in paths {
        let root = if root.ends_with('/') { root.clone() } else { alloc::format!("{}/", root) };
        // Every folder a pack might keep its pictures in.
        let mut dirs: Vec<alloc::string::String> = Vec::new();
        for sub in ["", "textures/", "block/", "item/", "blocks/", "items/"] {
            dirs.push(alloc::format!("{}{}", root, sub));
        }
        for ns in crate::sys::list_dir(&alloc::format!("{}assets/", root)) {
            for sub in ["textures/block/", "textures/item/", "textures/blocks/", "textures/items/"] {
                dirs.push(alloc::format!("{}assets/{}/{}", root, ns, sub));
            }
        }
        for d in dirs {
            for f in crate::sys::list_dir(&d) {
                if let Some(stem) = f.strip_suffix(".png") {
                    let name = alloc::string::String::from(stem);
                    let path = alloc::format!("{}{}", d, f);
                    files.retain(|e: &(alloc::string::String, alloc::string::String)| e.0 != name);
                    files.push((name, path));
                }
            }
        }
    }
    let n = files.len();
    let p = if paths.is_empty() { core::ptr::null_mut() } else { alloc::boxed::Box::into_raw(alloc::boxed::Box::new(PackIndex { files })) };
    PACKS.store(p, core::sync::atomic::Ordering::Release);
    n
}

fn packs() -> Option<&'static PackIndex> {
    let p = PACKS.load(core::sync::atomic::Ordering::Acquire);
    // SAFETY: from a leaked box, never freed.
    if p.is_null() { None } else { Some(unsafe { &*p }) }
}

/// The names a pack might call the texture `id`, best first.
pub fn pack_names(id: &str) -> Vec<alloc::string::String> {
    use alloc::string::{String, ToString};
    let mut v: Vec<String> = Vec::new();
    let mut add = |n: &str| {
        if !v.iter().any(|x| x == n) {
            v.push(n.to_string());
        }
    };
    add(id);
    // A longbow is drawn back in the pack's bow pictures.
    if let Some(n) = id.strip_prefix("longbow_pulling_").or_else(|| id.strip_prefix("magic_bow_pulling_")) {
        add(&alloc::format!("bow_pulling_{}", n));
    }
    // Wood: `<wood>_side` and `_top` are the log's sides and ends; our pine is the spruce.
    fn wood(w: &str) -> &str {
        match w {
            "pine" => "spruce",
            "blossom" => "cherry",
            w => w,
        }
    }
    if let Some(w) = id.strip_suffix("_side") {
        add(&alloc::format!("{}_log", wood(w)));
    }
    if let Some(w) = id.strip_suffix("_top") {
        add(&alloc::format!("{}_log_top", wood(w)));
    }
    for tail in ["_planks", "_leaves", "_door", "_chest"] {
        if let Some(w) = id.strip_suffix(tail) {
            add(&alloc::format!("{}{}", wood(w), tail));
        }
    }
    let fixed: &[(&str, &str)] = &[
        ("grass_top", "grass_block_top"),
        ("grass_side", "grass_block_side"),
        ("log_side", "oak_log"),
        ("log_top", "oak_log_top"),
        ("leaves", "oak_leaves"),
        ("planks", "oak_planks"),
        ("cobble", "cobblestone"),
        ("brick", "bricks"),
        ("snow_side", "grass_block_snow"),
        ("lamp", "redstone_lamp_on"),
        ("flower_red", "poppy"),
        ("flower_yellow", "dandelion"),
        ("tall_grass", "short_grass"),
        ("deepstone", "deepslate"),
        ("water", "water_still"),
        ("lava", "lava_still"),
        ("crystal_ore", "diamond_ore"),
        ("workbench_top", "crafting_table_top"),
        ("marble", "calcite"),
        ("slate", "deepslate"),
        ("basalt", "basalt_side"),
        ("moss", "moss_block"),
        ("mushroom", "red_mushroom"),
        ("glow_mushroom", "brown_mushroom"),
        ("farmland_top", "farmland"),
        ("raw_meat", "beef"),
        ("cooked_meat", "cooked_beef"),
        ("raw_fish", "cod"),
        ("cooked_fish", "cooked_cod"),
        ("berries", "sweet_berries"),
        ("crystal", "diamond"),
        ("copper_ingot", "copper_ingot"),
        ("icicle", "pointed_dripstone"),
        ("bread_crust", "bread"),
        ("ceramic", "white_terracotta"),
        ("steel", "iron_block"),
        ("books", "bookshelf"),
        ("pizza", "pumpkin_pie"),
        ("jack_face", "carved_pumpkin"),
        ("pumpkin_skin", "pumpkin_side"),
        ("straw", "hay_block_side"),
        ("pear", "apple"),
        ("cherry", "sweet_berries"),
        ("orange", "orange_dye"),
        ("milk", "milk_bucket"),
        ("flour", "sugar"),
        ("dough", "bread"),
        ("stew", "mushroom_stew"),
        ("cheese", "cake"),
        ("honeydew", "honeycomb"),
        ("crossbow", "crossbow_standby"),
        ("longbow", "bow"),
        ("magic_bow", "bow"),
    ];
    for (ours, theirs) in fixed {
        if id == *ours {
            add(theirs);
        }
    }
    // Coloured blocks: our `<colour>_glass` is their stained glass; `_boards` and chests are plain.
    if let Some(c) = id.strip_suffix("_glass") {
        add(&alloc::format!("{}_stained_glass", c));
    }
    if let Some(c) = id.strip_suffix("_boards") {
        add(&alloc::format!("{}_terracotta", c));
    }
    // Crops in stages: wheat_1..4 is wheat_stage0..3 (the last two share the ripe picture).
    for (ours, theirs, max) in [("wheat", "wheat_stage", 7), ("carrot", "carrots_stage", 3), ("potato", "potatoes_stage", 3), ("beet", "beetroots_stage", 3)] {
        if let Some(n) = id.strip_prefix(ours).and_then(|r| r.strip_prefix('_')).and_then(|r| r.parse::<usize>().ok()) {
            let stage = if max == 7 { [0, 2, 4, 7][(n.clamp(1, 4)) - 1] } else { n.clamp(1, 4) - 1 };
            add(&alloc::format!("{}{}", theirs, stage));
        }
    }
    // Seeds and plain crops.
    if let Some(c) = id.strip_suffix("_seeds") {
        add(&alloc::format!("{}_seeds", match c { "carrot" => "pumpkin", "potato" => "beetroot", c => c }));
    }
    // Tools: our wood is their wooden, our crystal their diamond, and a pick is a pickaxe.
    for (kind, theirs) in [("_pick", "_pickaxe"), ("_axe", "_axe"), ("_shovel", "_shovel"), ("_hoe", "_hoe"), ("_sword", "_sword")] {
        if let Some(m) = id.strip_suffix(kind) {
            let m = match m {
                "wood" => "wooden",
                "crystal" => "diamond",
                m => m,
            };
            add(&alloc::format!("{}{}", m, theirs));
        }
    }
    v
}

/// The tint a grey texture of the packs gets (they are coloured by the biome in their own game).
fn tint_of(name: &str) -> Option<[u8; 3]> {
    Some(match name {
        "grass_block_top" | "short_grass" | "tall_grass_top" | "tall_grass_bottom" | "fern" | "large_fern_top" | "large_fern_bottom" | "sugar_cane" | "grass" => [145, 189, 89],
        "oak_leaves" | "jungle_leaves" | "acacia_leaves" | "dark_oak_leaves" | "mangrove_leaves" | "vine" => [72, 181, 24],
        "spruce_leaves" => [97, 153, 97],
        "birch_leaves" => [128, 167, 85],
        "water_still" | "water_flow" => [63, 118, 228],
        "lily_pad" => [32, 128, 48],
        _ => return None,
    })
}

/// A pack's picture for texture `id`, 16 x 16 RGBA, if a pack has one.
pub fn pack_pixels(id: &str) -> Option<Vec<u8>> {
    let index = packs()?;
    for name in pack_names(id) {
        let Some((_, path)) = index.files.iter().find(|f| f.0 == name) else { continue };
        let Some(mut img) = crate::sys::read_file(path).and_then(|b| crate::image::decode(&b)) else { continue };
        // An animated picture is a tall strip of frames: the first one.
        if img.h > img.w && img.w > 0 && img.h % img.w == 0 {
            img.rgba.truncate(img.w * img.w * 4);
            img.h = img.w;
        }
        let mut px = if img.w == TILE && img.h == TILE { img.rgba } else { crate::image::fit(&img, TILE) };
        if let Some(t) = tint_of(&name) {
            for p in px.chunks_mut(4) {
                p[0] = (p[0] as u32 * t[0] as u32 / 255) as u8;
                p[1] = (p[1] as u32 * t[1] as u32 / 255) as u8;
                p[2] = (p[2] as u32 * t[2] as u32 / 255) as u8;
            }
        }
        tweak_colours(&mut px);
        return Some(px);
    }
    None
}

/// The 16 x 16 RGBA picture of texture number `i`, top row first: the file
/// the player has put in the textures folder if there is one, else what the
/// data describes.
pub fn tile_pixels(i: usize, t: &TileDef) -> Vec<u8> {
    if let Some(dir) = override_dir() {
        for ext in ["png", "jpg", "jpeg", "gif", "bmp", "tga"] {
            let name = alloc::format!("{}.{}", t.id, ext);
            if !override_files().iter().any(|f| *f == name) {
                continue;
            }
            if let Some(img) = crate::sys::read_file(&alloc::format!("{}{}", dir, name)).and_then(|b| crate::image::decode(&b)) {
                // Any size: fitted to a tile.
                return if img.w == TILE && img.h == TILE { img.rgba } else { crate::image::fit(&img, TILE) };
            }
        }
    }
    if let Some(px) = pack_pixels(&t.id) {
        return px;
    }
    // A tool the pack has no picture of (ruby, tin, emberite...): the pack's iron
    // one in that material's colour, so it is in the pack's style too.
    if let TileSrc::Paint { style, rgb } = &t.src {
        if let Some(px) = pack_tool(style, *rgb) {
            return px;
        }
    }
    described_pixels(i, t)
}

/// The pack's iron `kind` (pick, axe, shovel, hoe, sword) with its metal
/// recoloured to `rgb`; its handle and outline stay.
fn pack_tool(kind: &str, rgb: [u8; 3]) -> Option<Vec<u8>> {
    let theirs = match kind {
        "pick" => "iron_pickaxe",
        "axe" => "iron_axe",
        "shovel" => "iron_shovel",
        "hoe" => "iron_hoe",
        "sword" => "iron_sword",
        _ => return None,
    };
    let mut px = pack_pixels(theirs)?;
    for p in px.chunks_mut(4) {
        if p[3] == 0 {
            continue;
        }
        let (r, g, b) = (p[0] as i32, p[1] as i32, p[2] as i32);
        let hi = r.max(g).max(b);
        let lo = r.min(g).min(b);
        // Metal: grey (little colour) and not dark like the outline.
        if hi - lo < 28 && hi > 70 {
            let lum = (r + g + b) as f32 / 3.0 / 255.0;
            let k = (lum / 0.8).min(1.25);
            p[0] = (rgb[0] as f32 * k).min(255.0) as u8;
            p[1] = (rgb[1] as f32 * k).min(255.0) as u8;
            p[2] = (rgb[2] as f32 * k).min(255.0) as u8;
        }
    }
    Some(px)
}

/// How much the pack's colours are changed: hue turned by this many degrees,
/// saturation and brightness scaled by these percentages. Set from the settings.
static TWEAK: [core::sync::atomic::AtomicI32; 3] = [core::sync::atomic::AtomicI32::new(6), core::sync::atomic::AtomicI32::new(108), core::sync::atomic::AtomicI32::new(97)];

pub fn set_pack_tweak(hue: i32, saturation: i32, brightness: i32) {
    use core::sync::atomic::Ordering::Release;
    TWEAK[0].store(hue, Release);
    TWEAK[1].store(saturation, Release);
    TWEAK[2].store(brightness, Release);
}

/// Change the colours of a pack's picture a little (see [`set_pack_tweak`]).
fn tweak_colours(px: &mut [u8]) {
    use core::sync::atomic::Ordering::Acquire;
    let (hue, sat, val) = (TWEAK[0].load(Acquire), TWEAK[1].load(Acquire), TWEAK[2].load(Acquire));
    if hue == 0 && sat == 100 && val == 100 {
        return;
    }
    let (dh, ks, kv) = (hue as f32 / 60.0, sat as f32 / 100.0, val as f32 / 100.0);
    for p in px.chunks_mut(4) {
        let (r, g, b) = (p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0);
        let mx = r.max(g).max(b);
        let mn = r.min(g).min(b);
        let d = mx - mn;
        let mut h = if d <= 0.0 {
            0.0
        } else if mx == r {
            (g - b) / d
        } else if mx == g {
            2.0 + (b - r) / d
        } else {
            4.0 + (r - g) / d
        };
        h += dh;
        h -= 6.0 * crate::math::floorf(h / 6.0);
        let s = if mx <= 0.0 { 0.0 } else { (d / mx * ks).min(1.0) };
        let v = (mx * kv).min(1.0);
        // Back to red, green, blue.
        let c = v * s;
        // The standard hsv to rgb: channel n takes v - c * max(0, min(k, 4 - k, 1)), k = (n + h) mod 6.
        let chan = |n: f32| -> u8 {
            let kk = {
                let t = n + h;
                t - 6.0 * crate::math::floorf(t / 6.0)
            };
            let m = kk.min(4.0 - kk).min(1.0).max(0.0);
            ((v - c * m) * 255.0 + 0.5).clamp(0.0, 255.0) as u8
        };
        p[0] = chan(5.0);
        p[1] = chan(3.0);
        p[2] = chan(1.0);
    }
}

/// The picture the data describes, whatever files there are.
pub fn described_pixels(i: usize, t: &TileDef) -> Vec<u8> {
    let mut out = alloc::vec![0u8; TILE * TILE * 4];
    for y in 0..TILE {
        for x in 0..TILE {
            let (xi, yi) = (x as i32, y as i32);
            let p: Px = match &t.src {
                TileSrc::Builtin(name) => match builtin(name, xi, yi) {
                    Some(p) => p,
                    // Unknown painter: a loud checkerboard.
                    None => {
                        if (x / 4 + y / 4) % 2 == 0 {
                            [230, 40, 220, 255]
                        } else {
                            [20, 20, 20, 255]
                        }
                    }
                },
                TileSrc::Flat { rgb, noise } => grain(*rgb, *noise as i32, xi, yi, 100 + i as u32),
                TileSrc::Paint { style, rgb } => painted(style, *rgb, xi, yi).unwrap_or([230, 40, 220, 255]),
                TileSrc::Pixels(px) => {
                    let o = (y * TILE + x) * 4;
                    [px[o], px[o + 1], px[o + 2], px[o + 3]]
                }
            };
            let o = (y * TILE + x) * 4;
            out[o..o + 4].copy_from_slice(&p);
        }
    }
    out
}

/// The see-through-ness of every pixel of every texture as last built into
/// the atlas, 256 bytes a texture: what held items are cut out by.
static ALPHA: core::sync::atomic::AtomicPtr<Vec<u8>> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// The alpha of texture `tile`, 16 x 16, top row first (`None` before the atlas exists).
pub fn tile_alpha(tile: usize) -> Option<&'static [u8]> {
    let p = ALPHA.load(core::sync::atomic::Ordering::Acquire);
    if p.is_null() {
        return None;
    }
    // SAFETY: from a leaked box, never freed.
    unsafe { &*p }.get(tile * TILE * TILE..(tile + 1) * TILE * TILE)
}

/// The atlas of what the data describes, ignoring the player's own files (for exporting).
pub fn build_atlas_described(r: &Registry) -> Vec<u8> {
    let mut a = alloc::vec![0u8; ATLAS * ATLAS * 4];
    for (i, t) in r.tiles.iter().enumerate() {
        let (tx, ty) = (i % PER_ROW, i / PER_ROW);
        let px = described_pixels(i, t);
        for y in 0..TILE {
            let o = ((ty * TILE + y) * ATLAS + tx * TILE) * 4;
            a[o..o + TILE * 4].copy_from_slice(&px[y * TILE * 4..(y + 1) * TILE * 4]);
        }
    }
    a
}

/// Paint every registered texture into one 512 x 512 RGBA atlas.
pub fn build_atlas(r: &Registry) -> Vec<u8> {
    rescan_overrides();
    let mut a = alloc::vec![0u8; ATLAS * ATLAS * 4];
    let mut mask: Vec<u8> = Vec::with_capacity(r.tiles.len() * TILE * TILE);
    for (i, t) in r.tiles.iter().enumerate() {
        let (tx, ty) = (i % PER_ROW, i / PER_ROW);
        let px = tile_pixels(i, t);
        mask.extend(px.chunks(4).map(|p| p[3]));
        for y in 0..TILE {
            let o = ((ty * TILE + y) * ATLAS + tx * TILE) * 4;
            a[o..o + TILE * 4].copy_from_slice(&px[y * TILE * 4..(y + 1) * TILE * 4]);
        }
    }
    ALPHA.store(alloc::boxed::Box::into_raw(alloc::boxed::Box::new(mask)), core::sync::atomic::Ordering::Release);
    a
}

/// Halve an RGBA image. Colours are weighted by alpha and alpha keeps its
/// maximum, so cut-out textures (leaves) do not fade away in the distance.
/// Tiles are power-of-two aligned, so no colour crosses a tile edge.
pub fn half_size(src: &[u8], w: usize) -> Vec<u8> {
    let nw = w / 2;
    let mut out = alloc::vec![0u8; nw * nw * 4];
    for y in 0..nw {
        for x in 0..nw {
            let (mut r, mut g, mut b, mut wsum, mut amax) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let o = ((y * 2 + dy) * w + x * 2 + dx) * 4;
                let a = src[o + 3] as u32;
                r += src[o] as u32 * a;
                g += src[o + 1] as u32 * a;
                b += src[o + 2] as u32 * a;
                wsum += a;
                amax = amax.max(a);
            }
            let o = (y * nw + x) * 4;
            if wsum > 0 {
                out[o] = (r / wsum) as u8;
                out[o + 1] = (g / wsum) as u8;
                out[o + 2] = (b / wsum) as u8;
            }
            out[o + 3] = amax as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::reg;

    #[test]
    fn every_base_texture_has_a_painter() {
        for t in &reg().tiles {
            if let TileSrc::Builtin(name) = &t.src {
                assert!(builtin(name, 3, 4).is_some(), "no painter for {name}");
            }
        }
    }

    #[test]
    fn atlas_and_mips_have_the_right_size() {
        let a = build_atlas(reg());
        assert_eq!(a.len(), ATLAS * ATLAS * 4);
        // Tile 0 (grass_top) is opaque.
        assert_eq!(a[3], 255);
        assert_eq!(half_size(&a, ATLAS).len(), 256 * 256 * 4);
    }

    #[test]
    fn tga_round_trip() {
        let mut f = alloc::vec![0u8; 18];
        f[2] = 2;
        f[12] = 16;
        f[14] = 16;
        f[16] = 32;
        f[17] = 0x20;
        for i in 0..256u32 {
            f.extend_from_slice(&[i as u8, 2, 3, 255]); // B G R A
        }
        let px = decode_tga(&f).unwrap();
        assert_eq!(&px[0..4], &[3, 2, 0, 255]);
        assert_eq!(&px[4..8], &[3, 2, 1, 255]);
        assert!(decode_tga(&f[..100]).is_none());
    }

    #[test]
    fn every_texture_survives_being_written_as_a_png_and_read_back() {
        let r = crate::block::reg();
        for (i, t) in r.tiles.iter().enumerate().step_by(37) {
            let px = described_pixels(i, t);
            let img = crate::png::decode(&crate::png::encode(TILE, TILE, &px)).expect("png");
            assert_eq!((img.w, img.h), (TILE, TILE), "{}", t.id);
            assert_eq!(img.rgba, px, "{}", t.id);
        }
    }

    #[test]
    fn pack_names_know_our_names_for_things() {
        let has = |id: &str, want: &str| assert!(pack_names(id).iter().any(|n| n == want), "{id}: {:?}", pack_names(id));
        has("stone", "stone");
        has("grass_top", "grass_block_top");
        has("pine_side", "spruce_log");
        has("birch_top", "birch_log_top");
        has("wood_pick", "wooden_pickaxe");
        has("crystal_axe", "diamond_axe");
        has("red_glass", "red_stained_glass");
        has("wheat_3", "wheat_stage4");
        has("carrot_4", "carrots_stage3");
        has("raw_meat", "beef");
    }

    #[test]
    fn the_pack_colour_change_is_small_and_keeps_alpha() {
        let mut px = [126u8, 126, 126, 255, 129, 94, 63, 128];
        let before = px;
        tweak_colours(&mut px);
        // A grey stays grey (a little darker); a brown moves a little; alpha is untouched.
        assert!(px[0] == px[1] && px[1] == px[2] && px[0] < before[0] && before[0] - px[0] < 8, "{px:?}");
        assert!(px[4] != before[4] || px[5] != before[5] || px[6] != before[6]);
        assert!((px[4] as i32 - before[4] as i32).abs() < 30 && (px[5] as i32 - before[5] as i32).abs() < 30);
        assert_eq!((px[3], px[7]), (255, 128));
    }

    #[test]
    fn ruby_tools_are_not_matched_to_other_materials() {
        for id in ["ruby_pick", "ruby_axe", "ruby_sword", "ruby", "ruby_ore"] {
            assert!(pack_names(id).iter().all(|n| !n.contains("netherite") && !n.contains("emerald") && !n.contains("redstone")), "{id}");
        }
    }
}
