//! The colour a block shows from a distance: the average of the solid
//! pixels of its top picture, found from the block registry so that blocks
//! of mods and packs look right too. Distant terrain (`lod.rs`,
//! `far_surface.rs`) takes every colour from here, so a far column has the
//! colour of the block that appears when you walk up to it.

use crate::block::{def, reg, Block, Kind};
use alloc::vec::Vec;

/// Water, seen from afar.
pub const WATER: [u8; 3] = [44, 104, 198];
/// Lava, seen from afar.
pub const LAVA: [u8; 3] = [226, 98, 24];
/// What a block with no picture looks like.
pub const GREY: [u8; 3] = [128, 128, 128];

/// The average colour of a picture's solid pixels (RGBA bytes), or `None`
/// when it has none.
pub fn average_rgba(px: &[u8]) -> Option<[u8; 3]> {
    let (mut sum, mut n) = ([0u64; 3], 0u64);
    for p in px.chunks_exact(4) {
        if p[3] > 128 {
            sum[0] += p[0] as u64;
            sum[1] += p[1] as u64;
            sum[2] += p[2] as u64;
            n += 1;
        }
    }
    if n == 0 {
        None
    } else {
        Some([(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8])
    }
}

/// The average colour of one block, worked out from its top picture.
pub fn of_block(b: Block) -> [u8; 3] {
    let r = reg();
    let d = match r.blocks.get(b as usize) {
        Some(d) => d,
        None => return GREY,
    };
    if d.kind == Kind::Liquid {
        return if d.x.fluid.map_or(false, |f| f.0) { LAVA } else { WATER };
    }
    let tile = d.tex[0] as usize;
    match r.tiles.get(tile) {
        Some(t) => average_rgba(&crate::textures::described_pixels(tile, t)).unwrap_or(GREY),
        None => GREY,
    }
}

/// A table of block colours filled in as blocks turn up (a tile uses a
/// handful of different blocks, so this stays tiny and costs nothing
/// until it is asked).
pub struct BlockColours {
    cols: Vec<Option<[u8; 3]>>,
}

impl BlockColours {
    pub fn new() -> BlockColours {
        BlockColours { cols: alloc::vec![None; reg().blocks.len()] }
    }

    pub fn colour(&mut self, b: Block) -> [u8; 3] {
        if let Some(Some(c)) = self.cols.get(b as usize) {
            return *c;
        }
        let c = of_block(b);
        if let Some(slot) = self.cols.get_mut(b as usize) {
            *slot = Some(c);
        }
        c
    }
}

impl Default for BlockColours {
    fn default() -> Self {
        BlockColours::new()
    }
}

/// Leaves and needles: left out of the ground when trees are thinned.
pub fn is_foliage(b: Block) -> bool {
    let d = def(b);
    d.kind == Kind::Cutout && (d.id.contains("leaves") || d.id.contains("leaf") || d.id.contains("needle"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_average_is_taken_over_solid_pixels_only() {
        let px = [10u8, 20, 30, 255, 30, 40, 50, 255, 200, 200, 200, 0];
        assert_eq!(average_rgba(&px), Some([20, 30, 40]));
        assert_eq!(average_rgba(&[1, 2, 3, 0]), None);
        assert_eq!(average_rgba(&[]), None);
    }

    #[test]
    fn blocks_have_their_own_colours_and_the_table_agrees() {
        let r = reg();
        let mut t = BlockColours::new();
        assert_eq!(t.colour(r.wk.water), WATER);
        assert_eq!(t.colour(r.wk.stone), of_block(r.wk.stone));
        assert_eq!(t.colour(r.wk.stone), of_block(r.wk.stone));
        assert_ne!(of_block(r.wk.grass), of_block(r.wk.sand));
        // Out of range: grey, no panic.
        assert_eq!(of_block(Block::MAX), GREY);
    }
}
