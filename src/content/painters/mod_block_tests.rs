//! Tests that the painter styles the woods mod uses keep the house rules
//! (docs/TEXTURING.md, "VoxelRust house style" and "Mod blocks"): at most eight
//! colours, pure transparency (only glass is half see-through), the one-pixel
//! frame on the block faces that have one, and the glint at (1, 1) and the
//! shadow at (14, 14) on the faces whose rule says so. The woods mod is
//! `VoxelRust-Mods/woods/pack.txt`; without it (a build without the mods
//! folder) the tests have nothing to read and pass.

use super::tests::{colours, has_frame, picture};
use super::*;
use alloc::string::String;
use alloc::vec::Vec;

/// Styles whose faces carry the glint and the shadow (the rule of `sealed`).
const CORNERED_STYLES: &[&str] = &["concrete", "wool", "terracotta", "cardboard", "screen"];

/// Styles that are glass: the only ones with half see-through pixels.
const GLASS_STYLES: &[&str] = &["glass", "glassdoor"];

/// Every `texture <id> paint <style> ...` of the woods mod: the style and the first colour if it is a plain one.
fn woods_paints() -> Vec<(String, Option<[u8; 3]>)> {
    let mut out = Vec::new();
    for (path, bytes) in crate::mods::SHIPPED {
        if !path.ends_with("woods/pack.txt") {
            continue;
        }
        for line in core::str::from_utf8(bytes).unwrap_or("").lines() {
            let w: Vec<&str> = line.split_whitespace().collect();
            if w.len() >= 4 && w[0] == "texture" && w[2] == "paint" {
                let n: Vec<u8> = w[4..].iter().take(3).filter_map(|t| t.parse().ok()).collect();
                out.push((String::from(w[3]), if n.len() == 3 { Some([n[0], n[1], n[2]]) } else { None }));
            }
        }
    }
    out
}

fn brightness(px: &[u8], x: usize, y: usize) -> u32 {
    let o = (y * 16 + x) * 4;
    px[o] as u32 + px[o + 1] as u32 + px[o + 2] as u32
}

#[test]
fn the_woods_mods_styles_keep_the_colour_limit_and_pure_transparency() {
    for (style, own) in woods_paints() {
        let mut rgbs = alloc::vec![[150, 110, 70], [40, 200, 90], [235, 235, 240], [20, 24, 30]];
        rgbs.extend(own);
        for rgb in rgbs {
            let px = picture(|x, y| paint(&style, rgb, x, y));
            assert!(colours(&px) <= 8, "{style} in {rgb:?} has {} colours", colours(&px));
            if !GLASS_STYLES.contains(&style.as_str()) {
                assert!(px.chunks(4).all(|p| p[3] == 0 || p[3] == 255), "{style} in {rgb:?} has half see-through pixels");
            }
        }
    }
}

#[test]
fn the_woods_mods_block_faces_have_the_frame() {
    for (style, own) in woods_paints() {
        if !FRAMED_STYLES.contains(&style.as_str()) {
            continue;
        }
        for rgb in [[150, 110, 70], [200, 200, 210]].into_iter().chain(own) {
            assert!(has_frame(&picture(|x, y| paint(&style, rgb, x, y))), "{style} in {rgb:?} has no frame");
        }
    }
}

#[test]
fn faces_with_the_house_corners_have_the_glint_and_the_shadow() {
    // The styles the woods mod uses and the other flat block faces that follow the rule.
    for style in CORNERED_STYLES {
        for rgb in [[150, 110, 70], [196, 160, 110], [90, 120, 200]] {
            let px = picture(|x, y| paint(style, rgb, x, y));
            assert!(brightness(&px, 1, 1) > brightness(&px, 2, 1), "{style} in {rgb:?}: no glint at (1, 1)");
            assert_eq!(px[(14 * 16 + 14) * 4..(14 * 16 + 14) * 4 + 4], px[0..4], "{style} in {rgb:?}: no shadow at (14, 14)");
        }
    }
}

#[test]
fn every_tree_wood_style_has_the_core_woods_ground_and_cut_outs() {
    // A log's side, cut end and planks are solid pictures (no cut-outs), the leaves have them and keep two to four colours.
    for style in ["bark", "rings", "planks", "ocd_bark", "ocd_rings", "ocd_planks", "palm_bark", "palm_rings", "palm_planks"] {
        let px = picture(|x, y| paint(style, [150, 110, 70], x, y));
        assert!(px.chunks(4).all(|p| p[3] == 255), "{style} is solid");
        assert!(colours(&px) <= 6, "{style}: {} colours", colours(&px));
    }
    for style in ["leaves", "ocd_leaves", "palm_fronds"] {
        let px = picture(|x, y| paint(style, [150, 110, 70], x, y));
        assert!(px.chunks(4).any(|p| p[3] == 0), "{style} has cut-outs");
        assert!(colours(&px) <= 4, "{style}: {} colours", colours(&px));
    }
}
