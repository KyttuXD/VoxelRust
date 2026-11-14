//! The VoxelRust house style: what makes the game's own pictures its own
//! while they stay in the OCD style (flat fills, few colours, a frame round
//! blocks, an outline round items). The rules, in short (the long form is in
//! docs/TEXTURING.md, "VoxelRust house style"):
//!
//! - **Cool shadows, warm lights.** `scale` (darker) leans to blue-violet,
//!   `lift`, `pale` and `glint` lean to warm white. Colours are never greyed.
//! - **The corners.** Every block face has a glint at (1, 1), just inside
//!   the frame's top left corner, and a shadow at (14, 14): the frame's
//!   colour comes in one pixel diagonally. A block is recognisably ours by
//!   those two pixels alone.
//! - **A grain of its own per material class**: stone is chipped in pairs,
//!   earth is flecked diagonally, sand is rippled, clay is layered, cloth is
//!   stitched, wood has its own boards and dashed bark, ore is nuggets of
//!   three joined by a fracture line, metal and gems are faceted with a
//!   sparkle pixel.
//! - **Items** have an ink outline with a hint of blue, lighter on the lit
//!   (upper and left) edges: the rim light. Tools keep the OCD proportions and
//!   stand upright, with a bound haft and heads of their own shapes
//!   (`tool_shapes.rs`).
//!
//! Everything here is a pure function of the pixel's place: no noise, no
//! random numbers.

use super::*;

/// A block face's pixel: the frame (`pct` percent of the cell's colour) round
/// the edge, the glint at (1, 1), the shadow at (14, 14), the cell's own
/// colour elsewhere. `cell` is the colour of the material at this pixel, so a
/// face of two materials (grass above dirt) frames each in its own tone.
#[inline]
pub fn sealed(x: i32, y: i32, cell: [u8; 3], pct: i32) -> Px {
    sealed_grain(x, y, cell, pct, None)
}

/// [`sealed`] with a grain: `Some(c)` paints the inside of the face in `c`
/// (the grain of the material, a pixel or so at a time) except on the two
/// signature pixels.
#[inline]
pub fn sealed_grain(x: i32, y: i32, cell: [u8; 3], pct: i32, grain: Option<[u8; 3]>) -> Px {
    sealed_tone(x, y, cell, scale(cell, pct), grain)
}

/// [`sealed`] with a palette: the frame is the palette's dark colour if it has one.
#[inline]
pub fn sealed_pal(x: i32, y: i32, pal: &Palette, cell: [u8; 3], pct: i32) -> Px {
    sealed_tone(x, y, cell, pal.shade(cell, pct), None)
}

/// [`sealed_pal`] with a grain; the grain colour is given by the painter.
#[inline]
pub fn sealed_pal_grain(x: i32, y: i32, pal: &Palette, cell: [u8; 3], pct: i32, grain: Option<[u8; 3]>) -> Px {
    sealed_tone(x, y, cell, pal.shade(cell, pct), grain)
}

fn sealed_tone(x: i32, y: i32, cell: [u8; 3], frame: [u8; 3], grain: Option<[u8; 3]>) -> Px {
    if edge(x, y) || (x == 14 && y == 14) {
        px(frame)
    } else if x == 1 && y == 1 {
        px(glint(cell))
    } else if let Some(g) = grain {
        px(g)
    } else {
        px(cell)
    }
}

/// Does the pixel `(x, y)` lie in the list of places?
#[inline]
pub fn at_any(x: i32, y: i32, places: &[(i32, i32)]) -> bool {
    places.contains(&(x, y))
}

/// Stone's grain: chips in pairs, a horizontal pair, a vertical pair, a
/// horizontal pair and a lone one, never in the corners.
pub const CHIPS: [(i32, i32); 7] = [(4, 4), (5, 4), (10, 6), (10, 7), (5, 11), (6, 11), (12, 11)];

/// Earth's grain: flecks in short diagonals.
pub const FLECKS: [(i32, i32); 6] = [(3, 3), (4, 4), (10, 5), (11, 6), (6, 11), (7, 12)];

/// Sand's grain: two little crescents, the ripples of a dune.
pub const RIPPLES: [(i32, i32); 8] = [(3, 5), (4, 4), (5, 4), (6, 5), (9, 10), (10, 9), (11, 9), (12, 10)];

/// Cloth's stitches: short stitches in rows, every other row shifted two.
#[inline]
pub fn stitch(x: i32, y: i32) -> bool {
    y % 2 == 1 && x % 4 == 1 + 2 * ((y / 2) % 2)
}

/// Clay's layers: three flat dashes of different lengths.
#[inline]
pub fn layer(x: i32, y: i32) -> bool {
    rect(x, y, 3, 4, 5, 1) || rect(x, y, 8, 9, 5, 1) || rect(x, y, 3, 12, 4, 1)
}

/// A flat metal or gem face's sparkle: one pixel of the palest tone, always
/// two pixels in from the lit corner of the shape.
#[inline]
pub fn sparkle(c: [u8; 3]) -> [u8; 3] {
    pale(c)
}
