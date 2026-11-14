//! Tests of the painters: every name is painted, every picture is in the
//! OCD style (at most eight flat colours), and block faces have their frame.

use super::*;
use crate::block::{reg, Registry, TileDef, TileSrc, BASE_DATA};
use crate::textures::{described_pixels, TILE};
use alloc::string::String;
use alloc::vec::Vec;

/// The game with every shipped mod loaded, as the player may have it.
fn everything() -> Registry {
    let mut sources: Vec<(&str, &str)> = alloc::vec![("base.txt", BASE_DATA)];
    for (path, bytes) in crate::mods::SHIPPED {
        if path.ends_with("/pack.txt") {
            sources.push((path, core::str::from_utf8(bytes).unwrap()));
        }
    }
    Registry::from_sources(&sources, &mut |path| crate::mods::SHIPPED.iter().find(|f| f.0 == path).map(|f| f.1.to_vec()))
}

/// How many different opaque-or-not colours (ignoring alpha) a picture has.
fn colours(px: &[u8]) -> usize {
    let mut seen: Vec<[u8; 3]> = Vec::new();
    for p in px.chunks(4) {
        if p[3] > 0 && !seen.contains(&[p[0], p[1], p[2]]) {
            seen.push([p[0], p[1], p[2]]);
        }
    }
    seen.len()
}

fn picture(f: impl Fn(i32, i32) -> Option<Px>) -> Vec<u8> {
    let mut out = Vec::new();
    for y in 0..16 {
        for x in 0..16 {
            out.extend_from_slice(&f(x, y).expect("painted"));
        }
    }
    out
}

/// A one-pixel frame: every border pixel is the same solid colour, darker than the pixel inside the corner.
fn has_frame(px: &[u8]) -> bool {
    let at = |x: usize, y: usize| &px[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4];
    let f = at(0, 0);
    let sum = |p: &[u8]| p[0] as u32 + p[1] as u32 + p[2] as u32;
    for i in 0..16 {
        for (x, y) in [(i, 0), (i, 15), (0, i), (15, i)] {
            if at(x, y) != f {
                return false;
            }
        }
    }
    f[3] == 255 && sum(f) < sum(at(1, 1))
}

#[test]
fn every_name_in_the_lists_is_painted() {
    for name in BUILTINS {
        assert!(builtin(name, 3, 4).is_some(), "no builtin painter for {name}");
    }
    for style in STYLES {
        assert!(paint(style, [150, 110, 70], 3, 4).is_some(), "no painter for style {style}");
    }
    assert!(builtin("no_such_picture", 0, 0).is_none());
    assert!(paint("no_such_style", [1, 2, 3], 0, 0).is_none());
}

#[test]
fn every_name_the_data_uses_is_in_the_lists() {
    for r in [reg(), &everything()] {
        for t in &r.tiles {
            match &t.src {
                TileSrc::Builtin(n) => assert!(BUILTINS.contains(&n.as_str()), "{} uses unlisted painter {n}", t.id),
                TileSrc::Paint { style, .. } => assert!(STYLES.contains(&style.as_str()) || crate::content::art::bow::painted(style, [1, 2, 3], 0, 0).is_some() || crate::content::art::hud::knows(style), "{} uses unlisted style {style}", t.id),
                _ => {}
            }
        }
    }
}

#[test]
fn every_texture_has_at_most_eight_colours() {
    for r in [reg(), &everything()] {
        for (i, t) in r.tiles.iter().enumerate() {
            let px = described_pixels(i, t);
            assert!(colours(&px) <= 8, "{} has {} colours", t.id, colours(&px));
        }
    }
    // Whatever colour a style is asked for in.
    for style in STYLES {
        for rgb in [[255, 255, 255], [0, 0, 0], [150, 110, 70], [40, 200, 90], [250, 20, 20]] {
            let px = picture(|x, y| paint(style, rgb, x, y));
            assert!(colours(&px) <= 8, "{style} in {rgb:?} has {} colours", colours(&px));
        }
    }
}

#[test]
fn block_faces_have_the_frame() {
    for name in FRAMED_BUILTINS {
        assert!(has_frame(&picture(|x, y| builtin(name, x, y))), "{name} has no frame");
    }
    for style in FRAMED_STYLES {
        for rgb in [[150, 110, 70], [40, 200, 90], [200, 200, 210]] {
            assert!(has_frame(&picture(|x, y| paint(style, rgb, x, y))), "{style} in {rgb:?} has no frame");
        }
    }
    // A plain colour with grain set is a block face too.
    let t = TileDef { id: String::from("t"), src: TileSrc::Flat { rgb: [176, 180, 190], noise: 6 } };
    assert!(has_frame(&described_pixels(0, &t)));
}

#[test]
fn pictures_have_no_noise() {
    // The same picture twice, and a flat region really is flat: no painter looks at random numbers.
    for name in BUILTINS {
        assert_eq!(picture(|x, y| builtin(name, x, y)), picture(|x, y| builtin(name, x, y)));
    }
    // Stone: fill, frame, the glint and the chips; dirt: the same with its flecks.
    let stone = picture(|x, y| builtin("stone", x, y));
    assert_eq!(colours(&stone), 4);
    let dirt = picture(|x, y| builtin("dirt", x, y));
    assert_eq!(colours(&dirt), 4);
}

/// The sum of the channels of the pixel at `(x, y)` of a 16 x 16 picture.
fn bright(px: &[u8], x: usize, y: usize) -> u32 {
    let o = (y * 16 + x) * 4;
    px[o] as u32 + px[o + 1] as u32 + px[o + 2] as u32
}

#[test]
fn block_faces_carry_the_house_corners() {
    // A glint at (1, 1) lighter than the fill beside it, the frame's colour again at (14, 14).
    for name in ["stone", "deepstone", "dirt", "sand", "clay", "gravel", "sandstone"] {
        let px = picture(|x, y| builtin(name, x, y));
        assert!(bright(&px, 1, 1) > bright(&px, 2, 2), "{name}: no glint at (1, 1)");
        assert_eq!(px[(14 * 16 + 14) * 4..(14 * 16 + 14) * 4 + 4], px[0..4], "{name}: no shadow at (14, 14)");
    }
    for style in ["concrete", "wool", "terracotta", "rock"] {
        let px = picture(|x, y| paint(style, [150, 110, 70], x, y));
        assert!(bright(&px, 1, 1) > bright(&px, 2, 2), "{style}: no glint at (1, 1)");
        assert_eq!(px[(14 * 16 + 14) * 4..(14 * 16 + 14) * 4 + 4], px[0..4], "{style}: no shadow at (14, 14)");
    }
}

#[test]
fn shades_lean_cool_and_lights_lean_warm() {
    // Darker: red falls further than blue. Lighter: red rises further than blue.
    let c = [160, 160, 160];
    let (d, l) = (scale(c, 70), lift(c));
    assert!(d[0] < d[1] && d[1] < d[2], "{d:?}");
    assert!(l[0] > l[1] && l[1] > l[2], "{l:?}");
}

#[test]
fn items_have_the_house_outline_with_a_rim_light_on_the_lit_edge() {
    for style in ["pick", "axe", "shovel", "hoe", "sword", "gem", "ingot", "meat", "fruit"] {
        let px = picture(|x, y| paint(style, [200, 60, 60], x, y));
        let has = |c: [u8; 3]| px.chunks(4).any(|p| p[3] == 255 && p[..3] == c);
        assert!(has(OUTLINE), "{style}: no outline");
        assert!(has(RIM), "{style}: no rim light");
    }
}

#[test]
fn the_tool_shapes_are_sixteen_by_sixteen_of_known_letters() {
    use crate::painters::tool_shapes::shape;
    for name in ["stick", "pick", "axe", "shovel", "hoe", "sword", "arrow", "bow", "bow_pull0", "bow_pull1", "bow_pull2", "crossbow", "crossbow_pull0", "crossbow_pull1", "crossbow_pull2"] {
        let rows = shape(name).unwrap_or_else(|| panic!("no shape {name}"));
        for r in rows.iter() {
            assert_eq!(r.len(), 16, "{name}: {r}");
            assert!(r.bytes().all(|c| b".ohHLMDBW".contains(&c)), "{name}: {r}");
        }
    }
    // Tools stand on a two pixel haft in columns 7 and 8.
    for name in ["stick", "pick", "axe", "shovel", "hoe", "sword"] {
        let rows = shape(name).unwrap();
        assert!(rows[12].as_bytes()[7] != b'.' && rows[12].as_bytes()[8] != b'.', "{name}");
    }
}

#[test]
fn see_through_pixels_keep_their_old_alpha() {
    let alpha = |name: &str| picture(|x, y| builtin(name, x, y)).chunks(4).map(|p| p[3]).collect::<Vec<u8>>();
    // Leaves: holes or solid, nothing in between.
    for n in ["leaves", "pine_leaves"] {
        let a = alpha(n);
        assert!(a.iter().all(|v| *v == 0 || *v == 255), "{n}");
        assert!(a.iter().any(|v| *v == 0) && a.iter().any(|v| *v == 255), "{n}");
    }
    // Glass: a solid frame and a clear middle.
    let g = alpha("glass");
    assert_eq!((g[0], g[8 * 16 + 8]), (255, 0));
    // Stained glass: frame and fill each have their one alpha.
    let s = picture(|x, y| paint("glass", [200, 40, 40], x, y));
    assert_eq!((s[3], s[(8 * 16 + 8) * 4 + 3]), (235, 105));
    // Water is see-through, ice is solid, lava is solid.
    assert!(alpha("water").iter().all(|v| *v == 168));
    assert!(alpha("ice").iter().all(|v| *v == 255));
    assert!(alpha("lava").iter().all(|v| *v == 255));
}

#[test]
fn tools_are_upright_with_the_head_on_top() {
    // The metal is above the middle and the handle runs down the middle columns.
    for style in ["pick", "axe", "shovel", "hoe", "sword"] {
        let px = picture(|x, y| paint(style, [200, 60, 60], x, y));
        let solid = |x: usize, y: usize| px[(y * 16 + x) * 4 + 3] == 255;
        assert!((6..=9).any(|x| solid(x, 14)), "{style}: no handle at the foot");
        assert!(!solid(0, 8) && !solid(15, 8) && !solid(0, 15), "{style}: not upright");
        // The widest row is in the top half.
        let width = |y: usize| (0..16).filter(|x| solid(*x, y)).count();
        let (mut best, mut at) = (0, 0);
        for y in 0..16 {
            if width(y) > best {
                best = width(y);
                at = y;
            }
        }
        assert!(at < 8 || style == "sword", "{style}: head not on top (row {at})");
    }
}

#[test]
fn water_frames_wrap_and_move() {
    let a = crate::textures::water_frame(0);
    let b = crate::textures::water_frame(1);
    assert_ne!(a, b);
    assert_eq!(a, crate::textures::water_frame(8));
    assert_eq!(a.len(), TILE * TILE * 4);
    assert!(colours(&a) <= 2);
}
