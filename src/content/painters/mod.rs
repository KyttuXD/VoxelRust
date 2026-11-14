//! The painters: small procedures that draw the game's own 16 x 16 pictures
//! in the OCD style (docs/TEXTURING.md) with VoxelRust's own look, the
//! "house style" (see `house.rs` and the section of that name in
//! docs/TEXTURING.md). No noise, no random numbers: every picture is flat
//! fills of two to eight colours. A block has a one-pixel frame in a darker
//! tone of its fill, a glint at (1, 1) and a shadow at (14, 14); an item has
//! a cool near-black outline with a lighter rim on its lit edges; plants and
//! see-through things have neither.
//!
//! Each subject has its own file. A file offers `builtin(name, x, y)` for the
//! fixed pictures (`texture <id> builtin <name>`) and `paint(style, pal, x, y)`
//! for the patterns that take colours (`texture <id> paint <style> r g b`, or
//! with a palette: several colours, roles and gradients, see
//! `content/palette.rs`); both return `None` for a name that is not theirs.
//! A plain `r g b` is a palette of one colour and makes the old tones by the
//! old ratios, so old lines draw exactly what they always drew. The lists of names below
//! are the registry: a test checks that each name is painted by some file.

pub mod benches;
pub mod cardboard_and_screens;
pub mod blocks;
pub mod explosives;
pub mod food;
pub mod gear;
pub mod glass;
pub mod house;
pub mod liquids;
pub mod ocd_wood;
pub mod ores;
pub mod palm_wood;
pub mod plants;
pub mod sky;
pub mod stone;
pub mod tool_shapes;
pub mod tools;
pub mod wood;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod palette_tests;

pub use crate::content::palette::Palette;
pub use house::*;

/// One RGBA pixel.
pub type Px = [u8; 4];

/// Nothing: fully see-through.
pub const CLEAR: Px = [0, 0, 0, 0];

/// The outline of items: an ink with a hint of blue (VoxelRust's house
/// colour for outlines; the pack's is a neutral near-black).
pub const OUTLINE: [u8; 3] = [24, 22, 34];

/// The rim light on the lit (upper and left) edges of an item's outline.
pub const RIM: [u8; 3] = [60, 54, 80];

/// Frame tone of a block as a percentage of its fill (the pack's stone is
/// `#7e7e7e` inside and `#5c5c5c` round it).
pub const DARK: i32 = 73;

/// Every `builtin` name.
pub const BUILTINS: &[&str] = &[
    "stone", "dirt", "grass_top", "grass_side", "sand", "gravel", "water", "lava", "log_side", "log_top", "leaves", "planks", "cobble", "brick", "glass", "coal_ore", "iron_ore", "copper_ore", "crystal_ore", "snow",
    "snow_side", "ice", "sandstone", "cactus_side", "cactus_top", "lamp", "flower_red", "flower_yellow", "tall_grass", "pine_side", "pine_leaves", "deepstone", "clay", "stone_bricks", "sun", "moon", "wand", "ruby_ore",
];

/// Every `paint` style.
pub const STYLES: &[&str] = &[
    "concrete", "wool", "terracotta", "glass", "rock", "polished", "bricks", "tiles", "planks", "bark", "rings", "leaves", "blossom", "stick", "pick", "axe", "shovel", "hoe", "sword", "gem", "meat", "fruit", "fishfood",
    "rail", "machine", "hull", "wings", "chest", "chest_front", "chest_side", "chest_top", "workbench", "mushroom", "cluster", "icicle", "magma", "vine", "web", "glassdoor", "bow", "crossbow", "arrow", "ingot", "door", "farmland", "crop1", "crop2", "crop3",
    "crop4", "seeds", "sheaf", "loaf", "pizza", "wedge", "root", "tuber", "jar", "egg", "sack", "cookie", "pie", "bowl", "ore", "ore_vein", "tnt_side", "tnt_top", "tnt_bottom", "powder",
    "flint_steel", "bench5", "bench7", "bench_side", "ocd_bark", "ocd_rings", "ocd_planks", "ocd_leaves", "palm_bark", "palm_rings", "palm_planks", "palm_fronds", "cardboard", "screen",
];

/// Styles whose picture is a block face: they have the one-pixel frame
/// (checked by a test). The rest are items, plants and glass.
pub const FRAMED_STYLES: &[&str] = &["concrete", "wool", "terracotta", "rock", "polished", "chest", "workbench", "magma", "farmland", "door", "ore", "ore_vein", "tnt_side", "tnt_top", "tnt_bottom", "bench5", "bench7", "bench_side", "ocd_bark", "ocd_planks", "palm_bark", "palm_planks", "cardboard", "screen"];

/// Fixed pictures with the frame, likewise.
pub const FRAMED_BUILTINS: &[&str] = &[
    "stone", "dirt", "grass_top", "sand", "gravel", "coal_ore", "iron_ore", "copper_ore", "crystal_ore", "ruby_ore", "snow", "ice", "sandstone", "lamp", "deepstone", "clay", "cactus_top", "cobble",
];

/// One pixel of a fixed picture; `None` if no painter has that name.
pub fn builtin(name: &str, x: i32, y: i32) -> Option<Px> {
    blocks::builtin(name, x, y)
        .or_else(|| stone::builtin(name, x, y))
        .or_else(|| wood::builtin(name, x, y))
        .or_else(|| ores::builtin(name, x, y))
        .or_else(|| plants::builtin(name, x, y))
        .or_else(|| glass::builtin(name, x, y))
        .or_else(|| liquids::builtin(name, x, y))
        .or_else(|| sky::builtin(name, x, y))
        .or_else(|| tools::builtin(name, x, y))
}

/// One pixel of a pattern in the colour `rgb`; `None` if no painter has that style.
pub fn paint(style: &str, rgb: [u8; 3], x: i32, y: i32) -> Option<Px> {
    paint_pal(style, &Palette::from_rgb(rgb), x, y)
}

/// One pixel of a pattern in the colours of a palette; `None` if no painter has that style.
pub fn paint_pal(style: &str, pal: &Palette, x: i32, y: i32) -> Option<Px> {
    blocks::paint(style, pal, x, y)
        .or_else(|| stone::paint(style, pal, x, y))
        .or_else(|| wood::paint(style, pal, x, y))
        .or_else(|| ocd_wood::paint(style, pal, x, y))
        .or_else(|| palm_wood::paint(style, pal, x, y))
        .or_else(|| ores::paint(style, pal, x, y))
        .or_else(|| plants::paint(style, pal, x, y))
        .or_else(|| glass::paint(style, pal, x, y))
        .or_else(|| tools::paint(style, pal, x, y))
        .or_else(|| food::paint(style, pal, x, y))
        .or_else(|| gear::paint(style, pal, x, y))
        .or_else(|| explosives::paint(style, pal, x, y))
        .or_else(|| benches::paint(style, pal, x, y))
        .or_else(|| cardboard_and_screens::paint(style, pal, x, y))
}

// ---- Colour helpers.

/// An opaque pixel.
#[inline]
pub fn px(c: [u8; 3]) -> Px {
    [c[0], c[1], c[2], 255]
}

/// `c` with every channel scaled to `pct` percent. Shade is cool in the house
/// style: when `pct` darkens, red goes down a little more and blue a little
/// less, so shadows lean to blue-violet and never to grey.
#[inline]
pub fn scale(c: [u8; 3], pct: i32) -> [u8; 3] {
    let (kr, kb) = if pct < 100 { (95, 105) } else { (100, 100) };
    [(c[0] as i32 * pct * kr / 10000).clamp(0, 255) as u8, (c[1] as i32 * pct / 100).clamp(0, 255) as u8, (c[2] as i32 * pct * kb / 10000).clamp(0, 255) as u8]
}

/// The frame tone of `c`.
#[inline]
pub fn dark(c: [u8; 3]) -> [u8; 3] {
    scale(c, DARK)
}

/// A lighter tone of `c`: about a fifth of the way to white, warm (red moves
/// further than green, green further than blue): light is warm in the house style.
#[inline]
pub fn lift(c: [u8; 3]) -> [u8; 3] {
    [(c[0] as i32 + (255 - c[0] as i32) * 24 / 100) as u8, (c[1] as i32 + (255 - c[1] as i32) * 22 / 100) as u8, (c[2] as i32 + (255 - c[2] as i32) * 16 / 100) as u8]
}

/// A pale tone of `c`: about half way to white, warm.
#[inline]
pub fn pale(c: [u8; 3]) -> [u8; 3] {
    [(c[0] as i32 + (255 - c[0] as i32) * 55 / 100) as u8, (c[1] as i32 + (255 - c[1] as i32) * 50 / 100) as u8, (c[2] as i32 + (255 - c[2] as i32) * 44 / 100) as u8]
}

/// The glint of a block: a gentle step towards warm white, the pixel at (1, 1) of every block face.
#[inline]
pub fn glint(c: [u8; 3]) -> [u8; 3] {
    [(c[0] as i32 + (255 - c[0] as i32) * 14 / 100) as u8, (c[1] as i32 + (255 - c[1] as i32) * 13 / 100) as u8, (c[2] as i32 + (255 - c[2] as i32) * 9 / 100) as u8]
}

// ---- Shape helpers.

/// On the one-pixel border of the tile.
#[inline]
pub fn edge(x: i32, y: i32) -> bool {
    x == 0 || y == 0 || x == 15 || y == 15
}

/// A flat fill with a one-pixel frame in `pct` percent of it.
#[inline]
pub fn framed(x: i32, y: i32, fill: [u8; 3], pct: i32) -> Px {
    px(if edge(x, y) { scale(fill, pct) } else { fill })
}

/// Is `(x, y)` in the `w` x `h` rectangle whose top left is `(x0, y0)`?
#[inline]
pub fn rect(x: i32, y: i32, x0: i32, y0: i32, w: i32, h: i32) -> bool {
    x >= x0 && y >= y0 && x < x0 + w && y < y0 + h
}

/// Inside a disc round the middle of the tile; `r2` is the squared radius
/// in half pixels (`disc(x, y, 6)` is a disc about five pixels across).
#[inline]
pub fn disc(x: i32, y: i32, r2: i32) -> bool {
    let (dx, dy) = (2 * x - 15, 2 * y - 15);
    dx * dx + dy * dy <= r2 * 4
}

/// Inside an ellipse whose middle is `(cx, cy)` in half pixels (15 is the
/// tile's middle); `a2` and `b2` are the squared half-axes in half pixels.
#[inline]
pub fn ellipse(x: i32, y: i32, cx: i32, cy: i32, a2: i32, b2: i32) -> bool {
    let (dx, dy) = (2 * x - cx, 2 * y - cy);
    dx * dx * b2 + dy * dy * a2 <= a2 * b2
}

/// A picture with a dark outline: the pixel of the shape `f` at `(x, y)`, or
/// the outline if a neighbour of it is in the shape, or nothing. `f` is
/// asked about places outside the tile too and must answer by geometry alone.
pub fn outlined(x: i32, y: i32, f: &dyn Fn(i32, i32) -> Option<[u8; 3]>) -> Px {
    outlined_with(x, y, OUTLINE, f)
}

/// [`outlined`] with an outline of another colour (a palette's `outline` role).
pub fn outlined_with(x: i32, y: i32, outline: [u8; 3], f: &dyn Fn(i32, i32) -> Option<[u8; 3]>) -> Px {
    outlined_px_with(x, y, outline, &|x, y| f(x, y).map(px))
}

/// [`outlined`] for shapes that bring their own alpha.
pub fn outlined_px(x: i32, y: i32, f: &dyn Fn(i32, i32) -> Option<Px>) -> Px {
    outlined_px_with(x, y, OUTLINE, f)
}

/// [`outlined_px`] with an outline of another colour.
pub fn outlined_px_with(x: i32, y: i32, outline: [u8; 3], f: &dyn Fn(i32, i32) -> Option<Px>) -> Px {
    if let Some(p) = f(x, y) {
        return p;
    }
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        if f(x + dx, y + dy).is_some() {
            // The house rim light: the house outline is lighter where the shape
            // is lit (its upper and left edges). A palette's own outline stays flat.
            let lit = outline == OUTLINE && (f(x + 1, y).is_some() || f(x, y + 1).is_some()) && f(x - 1, y).is_none() && f(x, y - 1).is_none();
            return px(if lit { RIM } else { outline });
        }
    }
    CLEAR
}

/// A flat fill with a one-pixel frame: the palette's dark colour if it has
/// one, else `pct` percent of the fill.
#[inline]
pub fn framed_pal(x: i32, y: i32, pal: &Palette, fill: [u8; 3], pct: i32) -> Px {
    px(if edge(x, y) { pal.shade(fill, pct) } else { fill })
}
