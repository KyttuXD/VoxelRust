//! Textures. The game ships no image files: the built-in textures are
//! painted by small procedures at start-up, and packs can add their own as
//! flat colours or 16x16 TGA files.

use crate::block::{Registry, TileSrc};
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
        "vehicle" => {
            // A generic "thing with wheels" picture for vehicle items.
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

/// Paint every registered texture into one 512 x 512 RGBA atlas.
pub fn build_atlas(r: &Registry) -> Vec<u8> {
    let mut a = alloc::vec![0u8; ATLAS * ATLAS * 4];
    for (i, t) in r.tiles.iter().enumerate() {
        let (tx, ty) = (i % PER_ROW, i / PER_ROW);
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
                let o = ((ty * TILE + y) * ATLAS + tx * TILE + x) * 4;
                a[o..o + 4].copy_from_slice(&p);
            }
        }
    }
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
}
