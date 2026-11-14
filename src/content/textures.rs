//! Textures. The game ships no image files: the built-in textures are
//! drawn by the painters in `painters/` (OCD style, flat colours) at
//! start-up, and packs and players can add pictures of their own.

use crate::block::{Registry, TileDef, TileSrc};
use alloc::vec::Vec;

/// Tile edge in pixels.
pub const TILE: usize = 16;
/// The atlas holds 16 x 16 tiles.
pub const ATLAS: usize = 512;
/// Tiles along one edge of the atlas, and how many it holds.
pub const PER_ROW: usize = ATLAS / TILE;
pub const MAX_TILES: usize = PER_ROW * PER_ROW;

type Px = [u8; 4];

/// One pixel of the water texture at a moment in its eight-step cycle (see
/// `painters/liquids.rs`).
pub use crate::painters::liquids::{lava_px, water_frame, water_px};

/// One pixel of a built-in texture; `None` if there is no such painter.
/// The painters are in `src/content/painters/`, one file for each subject.
pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    crate::painters::builtin(name, x, y)
}

/// One pixel of a built-in pattern drawn in the colour `rgb`; `None` if
/// there is no such style. These let data files make whole families of
/// blocks (sixteen colours of wool, five kinds of wood) from one line each.
pub fn painted(style: &str, rgb: [u8; 3], x: i32, y: i32) -> Option<Px> {
    // Bows and crossbows, at rest and drawn, are in the OCD style (content/art/bow.rs).
    if let Some(p) = crate::content::art::bow::painted(style, rgb, x, y) {
        return Some(p);
    }
    // The HUD's hearts and effect icons (content/art/hud.rs).
    if let Some(p) = crate::content::art::hud::painted(style, x, y) {
        return Some(p);
    }
    crate::painters::paint(style, rgb, x, y)
}

/// [`painted`] with a palette (`texture <id> paint <style> palette ...`): the
/// same patterns with several colours, roles and gradients.
pub fn painted_pal(style: &str, pal: &crate::palette::Palette, x: i32, y: i32) -> Option<Px> {
    // Bows and crossbows take the one colour of their wood (at this pixel, if there is a gradient).
    if let Some(p) = crate::content::art::bow::painted(style, pal.at(x, y), x, y) {
        return Some(p);
    }
    crate::painters::paint_pal(style, pal, x, y)
}

/// A plain colour as a block face: with a `noise` setting (the old grain,
/// now ignored) it gets the one-pixel darker frame of the OCD style; with
/// none it is a flat fill all over.
fn flat_px(rgb: [u8; 3], noise: u8, x: i32, y: i32) -> Px {
    if noise > 0 {
        crate::painters::framed(x, y, rgb, crate::painters::DARK)
    } else {
        crate::painters::px(rgb)
    }
}


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

/// Put `new` in `slot` for readers to find. What `slot` held before is kept
/// in `retired` until the next swap, and what `retired` held is freed: a
/// reader that took a reference from `slot` has all of a rebuild to finish
/// with it. (These used to be leaked for good, one list per atlas rebuild.)
fn swap_retiring<T>(slot: &core::sync::atomic::AtomicPtr<T>, retired: &core::sync::atomic::AtomicPtr<T>, new: *mut T) {
    use core::sync::atomic::Ordering::AcqRel;
    let old = slot.swap(new, AcqRel);
    let older = retired.swap(old, AcqRel);
    if !older.is_null() {
        // SAFETY: made by `Box::into_raw` below, replaced two swaps ago.
        unsafe { drop(alloc::boxed::Box::from_raw(older)) };
    }
}

/// The files in the textures folder when the atlas was last built.
static OVERRIDE_FILES: core::sync::atomic::AtomicPtr<Vec<alloc::string::String>> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
static OVERRIDE_FILES_OLD: core::sync::atomic::AtomicPtr<Vec<alloc::string::String>> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// Look in the textures folder again (when the atlas is rebuilt).
pub fn rescan_overrides() {
    let Some(dir) = override_dir() else { return };
    let files: Vec<alloc::string::String> = crate::sys::list_dir(dir);
    swap_retiring(&OVERRIDE_FILES, &OVERRIDE_FILES_OLD, alloc::boxed::Box::into_raw(alloc::boxed::Box::new(files)));
}

fn override_files() -> &'static [alloc::string::String] {
    let p = OVERRIDE_FILES.load(core::sync::atomic::Ordering::Acquire);
    // SAFETY: from a boxed list, freed only two rebuilds after it was replaced.
    if p.is_null() { &[] } else { unsafe { &*p } }
}

/// Texture packs: folders of PNG pictures named as Minecraft-style packs name
/// them (`assets/<namespace>/textures/block/stone.png` and `.../item/...`, or
/// just a folder of pictures). A texture the game has an id for is taken from
/// a pack when the pack has a picture it matches (see [`pack_names`]).
struct PackIndex {
    /// file stem -> full path; later packs come later and win.
    files: alloc::collections::BTreeMap<alloc::string::String, alloc::string::String>,
}

static PACKS: core::sync::atomic::AtomicPtr<PackIndex> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
static PACKS_OLD: core::sync::atomic::AtomicPtr<PackIndex> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// Use these packs (folders), in order, and index their pictures. Returns how many pictures were found.
pub fn set_texture_packs(paths: &[alloc::string::String]) -> usize {
    // The packs' pictures of creatures and players are looked for too.
    crate::pack_entities::set_packs(paths);
    let mut files = alloc::collections::BTreeMap::new();
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
                    // (A later folder's picture of the same name replaces the earlier one.)
                    files.insert(alloc::string::String::from(stem), alloc::format!("{}{}", d, f));
                }
            }
        }
    }
    let n = files.len();
    let p = if paths.is_empty() { core::ptr::null_mut() } else { alloc::boxed::Box::into_raw(alloc::boxed::Box::new(PackIndex { files })) };
    swap_retiring(&PACKS, &PACKS_OLD, p);
    n
}

fn packs() -> Option<&'static PackIndex> {
    let p = PACKS.load(core::sync::atomic::Ordering::Acquire);
    // SAFETY: from a boxed index, freed only two swaps after it was replaced.
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
        let Some(path) = index.files.get(&name) else { continue };
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
    tile_pixels_with(i, t, &mut Vec::new())
}

/// The pack's iron tools as read so far (kind -> picture), so that a pack with big pictures is
/// not decoded again for every metal a tool is made in.
type ToolPictures = Vec<(&'static str, Option<Vec<u8>>)>;

/// [`tile_pixels`] with a memory of the pack's tool pictures kept between calls.
fn tile_pixels_with(i: usize, t: &TileDef, tools: &mut ToolPictures) -> Vec<u8> {
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
        if let Some(px) = pack_tool(style, *rgb, tools) {
            return px;
        }
    }
    // A palette of one flat colour is the same thing; one with several colours is the game's own drawing.
    if let TileSrc::Palette { style, pal } = &t.src {
        if pal.is_plain() {
            if let Some(px) = pack_tool(style, pal.base(), tools) {
                return px;
            }
        }
    }
    described_pixels(i, t)
}

/// The pack's iron `kind` (pick, axe, shovel, hoe, sword) with its metal
/// recoloured to `rgb`; its handle and outline stay.
fn pack_tool(kind: &str, rgb: [u8; 3], tools: &mut ToolPictures) -> Option<Vec<u8>> {
    let theirs = match kind {
        "pick" => "iron_pickaxe",
        "axe" => "iron_axe",
        "shovel" => "iron_shovel",
        "hoe" => "iron_hoe",
        "sword" => "iron_sword",
        _ => return None,
    };
    if !tools.iter().any(|t| t.0 == theirs) {
        tools.push((theirs, pack_pixels(theirs)));
    }
    let mut px = tools.iter().find(|t| t.0 == theirs)?.1.clone()?;
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
static TWEAK: [core::sync::atomic::AtomicI32; 3] = [core::sync::atomic::AtomicI32::new(0), core::sync::atomic::AtomicI32::new(100), core::sync::atomic::AtomicI32::new(100)];

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
pub fn described_pixels(_i: usize, t: &TileDef) -> Vec<u8> {
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
                TileSrc::Flat { rgb, noise } => flat_px(*rgb, *noise, xi, yi),
                TileSrc::Paint { style, rgb } => painted(style, *rgb, xi, yi).unwrap_or([230, 40, 220, 255]),
                TileSrc::Palette { style, pal } => painted_pal(style, pal, xi, yi).unwrap_or([230, 40, 220, 255]),
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
static ALPHA_OLD: core::sync::atomic::AtomicPtr<Vec<u8>> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// The alpha of texture `tile`, 16 x 16, top row first (`None` before the atlas exists).
pub fn tile_alpha(tile: usize) -> Option<&'static [u8]> {
    let p = ALPHA.load(core::sync::atomic::Ordering::Acquire);
    if p.is_null() {
        return None;
    }
    // SAFETY: from a boxed list, freed only two atlas builds after it was replaced.
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
    let mut tools: ToolPictures = Vec::new();
    for (i, t) in r.tiles.iter().enumerate() {
        let (tx, ty) = (i % PER_ROW, i / PER_ROW);
        let px = tile_pixels_with(i, t, &mut tools);
        mask.extend(px.chunks(4).map(|p| p[3]));
        for y in 0..TILE {
            let o = ((ty * TILE + y) * ATLAS + tx * TILE) * 4;
            a[o..o + TILE * 4].copy_from_slice(&px[y * TILE * 4..(y + 1) * TILE * 4]);
        }
    }
    swap_retiring(&ALPHA, &ALPHA_OLD, alloc::boxed::Box::into_raw(alloc::boxed::Box::new(mask)));
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
    fn replaced_lists_are_freed_two_swaps_later() {
        use core::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};
        static DROPS: AtomicUsize = AtomicUsize::new(0);
        struct D;
        impl Drop for D {
            fn drop(&mut self) {
                DROPS.fetch_add(1, Ordering::SeqCst);
            }
        }
        let (slot, old): (AtomicPtr<D>, AtomicPtr<D>) = (AtomicPtr::new(core::ptr::null_mut()), AtomicPtr::new(core::ptr::null_mut()));
        for round in 1..=5usize {
            swap_retiring(&slot, &old, alloc::boxed::Box::into_raw(alloc::boxed::Box::new(D)));
            // The current one and the one before it are alive; everything older is gone.
            assert_eq!(DROPS.load(Ordering::SeqCst), round.saturating_sub(2), "round {round}");
        }
        for p in [&slot, &old] {
            let p = p.swap(core::ptr::null_mut(), Ordering::SeqCst);
            // SAFETY: made above with `Box::into_raw`.
            unsafe { drop(alloc::boxed::Box::from_raw(p)) };
        }
    }

    #[test]
    fn the_registry_finds_textures_and_models_by_id_as_a_search_would() {
        let r = reg();
        for (i, t) in r.tiles.iter().enumerate().step_by(7) {
            assert_eq!(r.tile(&t.id), r.tiles.iter().position(|x| x.id == t.id), "{} ({i})", t.id);
        }
        for m in &r.models {
            assert_eq!(r.model(&m.id), r.models.iter().position(|x| x.id == m.id), "{}", m.id);
        }
        assert_eq!(r.tile("no such texture"), None);
        assert!(r.tile("white").is_some());
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
        set_pack_tweak(6, 108, 97);
        tweak_colours(&mut px);
        set_pack_tweak(0, 100, 100);
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
