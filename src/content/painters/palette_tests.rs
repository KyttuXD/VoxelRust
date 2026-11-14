//! Tests of palettes in the painters: the old `r g b` syntax draws what it
//! always drew, and the new palette forms draw what the docs say.

use super::*;
use crate::block::{Registry, TileSrc, BASE_DATA};
use crate::palette::{Gradient, Palette};
use crate::textures::described_pixels;
use alloc::vec::Vec;

fn at(style: &str, rgb: [u8; 3], x: i32, y: i32) -> Px {
    paint(style, rgb, x, y).unwrap()
}

fn pal_at(style: &str, pal: &Palette, x: i32, y: i32) -> Px {
    paint_pal(style, pal, x, y).unwrap()
}

fn colour_count(f: impl Fn(i32, i32) -> Px) -> usize {
    let mut seen: Vec<[u8; 3]> = Vec::new();
    for y in 0..16 {
        for x in 0..16 {
            let p = f(x, y);
            if p[3] > 0 && !seen.contains(&[p[0], p[1], p[2]]) {
                seen.push([p[0], p[1], p[2]]);
            }
        }
    }
    seen.len()
}

/// A registry of the base data and some extra lines.
fn with(extra: &str) -> Registry {
    Registry::from_sources(&[("base.txt", BASE_DATA), ("extra.txt", extra)], &mut |_| None)
}

fn tile<'a>(r: &'a Registry, id: &str) -> &'a crate::block::TileDef {
    r.tiles.iter().find(|t| t.id == id).unwrap_or_else(|| panic!("no texture {id}"))
}

#[test]
fn old_rgb_pictures_are_what_they_were() {
    // Tones as the house style makes them: light about a fifth to warm white, dark 73% leaning cool.
    let c = [200, 60, 60];
    let (l, d) = ([213, 102, 91, 255], [138, 43, 45, 255]);
    // A pickaxe: lit top row, middle row, shaded ends, a wooden haft (lit column, shaded column),
    // an ink outline that is lighter on its upper left edge (the rim light).
    // (The tools are painted from VoxelRust's own silhouettes: see tool_shapes.)
    assert_eq!(at("pick", c, 7, 2), l);
    assert_eq!(at("pick", c, 8, 3), [200, 60, 60, 255]);
    assert_eq!(at("pick", c, 3, 3), l);
    assert_eq!(at("pick", c, 13, 3), d);
    assert_eq!(at("pick", c, 7, 8), [166, 118, 70, 255]);
    assert_eq!(at("pick", c, 8, 8), [118, 80, 54, 255]);
    assert_eq!(at("pick", c, 9, 8), [24, 22, 34, 255]);
    assert_eq!(at("pick", c, 4, 1), [60, 54, 80, 255]);
    assert_eq!(at("pick", c, 8, 1), [24, 22, 34, 255]);
    assert_eq!(at("pick", c, 0, 0), CLEAR);
    // The haft is bound with the dark tone of the head.
    assert_eq!(at("pick", c, 7, 7), d);
    // A sword: lit left edge of the blade, a highlight (the fuller) beside it, mid on the right.
    assert_eq!(at("sword", c, 8, 5), [200, 60, 60, 255]);
    assert_eq!(at("sword", c, 7, 5), [223, 135, 117, 255]);
    assert_eq!(at("sword", c, 3, 10), d);
    // A block: frame at 92% of the fill (leaning cool), the glint, the shadow in the corner, four pores.
    assert_eq!(at("concrete", [150, 110, 70], 0, 0), [131, 101, 67, 255]);
    assert_eq!(at("concrete", [150, 110, 70], 5, 5), [150, 110, 70, 255]);
    assert_eq!(at("concrete", [150, 110, 70], 14, 14), [131, 101, 67, 255]);
    assert_eq!(at("concrete", [150, 110, 70], 1, 1), [164, 128, 86, 255]);
    // An ore: stone with a frame, nuggets of three pixels with a lighter pixel first, a crack of darker stone.
    assert_eq!(at("ore", [40, 40, 46], 0, 0), [80, 87, 101, 255]);
    assert_eq!(at("ore", [40, 40, 46], 7, 7), [116, 120, 132, 255]);
    assert_eq!(at("ore", [40, 40, 46], 3, 2), [91, 87, 79, 255]);
    assert_eq!(at("ore", [40, 40, 46], 4, 2), [40, 40, 46, 255]);
    assert_eq!(at("ore", [40, 40, 46], 7, 5), [92, 100, 116, 255]);
    // A gem is still lit upper left, shaded lower right.
    assert_eq!(at("gem", c, 6, 6), l);
    assert_eq!(at("gem", c, 9, 9), d);
    // ... with a pale sparkle on the lit facet.
    assert_eq!(at("gem", c, 5, 6), px(pale(lift(c))));
}

#[test]
fn a_one_colour_palette_equals_the_old_function_everywhere() {
    for style in STYLES {
        for rgb in [[150, 110, 70], [40, 200, 90], [250, 20, 20]] {
            let p = Palette::from_rgb(rgb);
            for y in 0..16 {
                for x in 0..16 {
                    assert_eq!(paint(style, rgb, x, y), paint_pal(style, &p, x, y), "{style} {rgb:?} at {x},{y}");
                }
            }
        }
    }
    // The builtin ores are the ore style.
    for (name, rgb) in [("coal_ore", [38, 38, 52]), ("iron_ore", [214, 150, 118]), ("ruby_ore", [236, 62, 116])] {
        for y in 0..16 {
            for x in 0..16 {
                assert_eq!(builtin(name, x, y), paint("ore", rgb, x, y), "{name}");
            }
        }
    }
}

#[test]
fn a_gradient_spreads_over_the_head_of_a_tool() {
    let fire = Palette::from_list(&[[200, 40, 20], [250, 200, 30]], Gradient::Vertical);
    // The pick's head is rows 2 to 5; the upper rows are the first colour, the lower ones the second.
    assert_eq!(pal_at("pick", &fire, 7, 2)[..3], [200, 40, 20]);
    assert_eq!(pal_at("pick", &fire, 13, 4)[..3], [250, 200, 30]);
    // The haft stays wood and the outline stays dark.
    assert_eq!(pal_at("pick", &fire, 8, 10), [118, 80, 54, 255]);
    assert_eq!(pal_at("pick", &fire, 9, 1), [24, 22, 34, 255]);
    // An axe's head has both colours too.
    let top = pal_at("axe", &fire, 10, 3);
    let bottom = pal_at("axe", &fire, 10, 6);
    assert_ne!(top, bottom);
    // The picture stays in the OCD range of colours.
    for style in ["pick", "axe", "shovel", "hoe", "sword"] {
        let three = Palette::from_list(&[[200, 40, 20], [250, 120, 30], [250, 220, 60]], Gradient::Vertical);
        assert!(colour_count(|x, y| pal_at(style, &three, x, y)) <= 8, "{style}");
    }
}

#[test]
fn tool_palettes_give_handle_fuller_and_outline() {
    let mut p = Palette::from_list(&[[120, 200, 240]], Gradient::None);
    p.more.push(Palette::from_list(&[[200, 170, 120], [150, 110, 70]], Gradient::None));
    assert_eq!(pal_at("axe", &p, 8, 10), [150, 110, 70, 255]);
    // A sword with an accent has a fuller down column 7.
    let sword = parse(&["colors", "#78c8f0", "accent=#ffffff", "outline=#101040"]);
    assert_eq!(pal_at("sword", &sword, 7, 5), [255, 255, 255, 255]);
    assert_eq!(pal_at("sword", &sword, 8, 5)[..3], [120, 200, 240]);
    assert_eq!(pal_at("sword", &sword, 7, 0), [16, 16, 64, 255]);
    assert!(colour_count(|x, y| pal_at("sword", &sword, x, y)) <= 8);
}

fn parse(words: &[&str]) -> Palette {
    crate::palette::parse_spec(words, &Vec::new()).unwrap()
}

#[test]
fn ores_take_host_gems_and_veins() {
    let ruby = parse(&["#e2406e", "#a82454", "#ff9cb8"]);
    // Nugget: highlight (the light colour) first, dark last, base between.
    assert_eq!(pal_at("ore", &ruby, 3, 2), [255, 156, 184, 255]);
    assert_eq!(pal_at("ore", &ruby, 4, 3), [168, 36, 84, 255]);
    assert_eq!(pal_at("ore", &ruby, 4, 2), [226, 64, 110, 255]);
    assert_eq!(pal_at("ore", &ruby, 7, 7), [116, 120, 132, 255]);
    // A deep host.
    let mut deep = ruby.clone();
    deep.host = Some(crate::painters::stone::DEEPSTONE);
    assert_eq!(pal_at("ore", &deep, 7, 7), [52, 54, 72, 255]);
    assert_eq!(pal_at("ore", &deep, 0, 0), [36, 39, 55, 255]);
    // A multi ore: the second nugget is the second gem.
    let mut multi = ruby.clone();
    multi.more.push(parse(&["#40c070"]));
    assert_eq!(pal_at("ore", &multi, 10, 5)[..3], [64, 192, 112]);
    assert_eq!(pal_at("ore", &multi, 4, 2)[..3], [226, 64, 110]);
    // The frame is complete, the colours few.
    for pal in [&ruby, &deep, &multi] {
        assert!(colour_count(|x, y| pal_at("ore", pal, x, y)) <= 8);
        assert!(colour_count(|x, y| pal_at("ore_vein", pal, x, y)) <= 8);
    }
    // A vein is a wavy band across the stone that is not on the frame: a lit row over a base row.
    assert_eq!(pal_at("ore_vein", &ruby, 5, 6), [226, 64, 110, 255]);
    assert_eq!(pal_at("ore_vein", &ruby, 5, 5), [255, 156, 184, 255]);
    assert_eq!(pal_at("ore_vein", &ruby, 4, 5), [255, 156, 184, 255]);
    assert_eq!(pal_at("ore_vein", &ruby, 0, 9), [80, 87, 101, 255]);
}

#[test]
fn the_data_lines_make_palette_textures() {
    let r = with(
        "palette jade #3aa66a #1e6a3e #8fe0b0\n\
         palette lava_grad #d83a1c #f2a21c gradient vertical\n\
         palette birch #d8caa0 #a89a70\n\
         texture t_old paint pick 200 60 60\n\
         texture t_one paint pick #c83c3c # a comment\n\
         texture t_jade paint gem palette jade\n\
         texture t_ore paint ore palette jade host deepstone\n\
         texture t_fire paint axe palette lava_grad+birch\n\
         texture t_inline paint sword colors #112233 #223344 gradient diagonal\n\
         texture t_bad paint pick palette nope\n\
         texture t_bad2 paint pick #12345\n",
    );
    assert!(matches!(tile(&r, "t_old").src, TileSrc::Paint { rgb: [200, 60, 60], .. }));
    assert!(matches!(tile(&r, "t_one").src, TileSrc::Paint { rgb: [200, 60, 60], .. }));
    assert!(matches!(tile(&r, "t_jade").src, TileSrc::Palette { .. }));
    match &tile(&r, "t_ore").src {
        TileSrc::Palette { pal, .. } => assert_eq!(pal.host, Some(crate::painters::stone::DEEPSTONE)),
        _ => panic!("palette"),
    }
    match &tile(&r, "t_fire").src {
        TileSrc::Palette { pal, .. } => assert!(pal.is_gradient() && pal.more.len() == 1),
        _ => panic!("palette"),
    }
    match &tile(&r, "t_inline").src {
        TileSrc::Palette { pal, .. } => assert_eq!((pal.gradient, pal.len()), (Gradient::Diagonal, 2)),
        _ => panic!("palette"),
    }
    assert!(r.tiles.iter().all(|t| t.id != "t_bad" && t.id != "t_bad2"), "bad lines are skipped");
    assert!(r.warnings.iter().any(|w| w.contains("nope")), "{:?}", r.warnings);
    // Each draws something within the colour limit.
    for id in ["t_old", "t_one", "t_jade", "t_ore", "t_fire", "t_inline"] {
        let px = described_pixels(0, tile(&r, id));
        assert!(px.chunks(4).any(|p| p[3] > 0), "{id}");
    }
}

#[test]
fn the_shipped_ruby_ore_is_a_palette_ore() {
    let r = with("");
    assert!(matches!(tile(&r, "ruby_ore").src, TileSrc::Palette { .. }));
    let px = described_pixels(0, tile(&r, "ruby_ore"));
    assert!(colour_count(|x, y| {
        let o = ((y * 16 + x) * 4) as usize;
        [px[o], px[o + 1], px[o + 2], px[o + 3]]
    }) <= 8);
}
