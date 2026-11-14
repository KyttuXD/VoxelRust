//! Which blocks the terrain puts on the ground of a column: the top block
//! and the filling under it. The real chunk generator (`worldgen::generate`)
//! and the distant terrain (`far_surface.rs`) both ask here, so a far column
//! has the block that appears when you walk up to it.

use crate::block::{Block, WellKnown};
use crate::noise::hash2;
use crate::worldgen::{source, Biome, Column, SEA};

/// The top block and the block under it for the column at `(x, z)`, by the
/// terrain's own rules: sea bed, beaches, sand over sandstone in deserts,
/// badlands bands by height, rock and snow on mountains, snowy grass in the
/// cold, grass and dirt elsewhere.
pub fn surface_blocks(wk: &WellKnown, seed: u32, x: i32, z: i32, col: &Column) -> (Block, Block) {
    let h = col.height;
    if h < SEA {
        let bed = match hash2(seed ^ 0xB1, x >> 2, z >> 2) % 8 {
            0 => wk.clay,
            1 | 2 => wk.gravel,
            _ => wk.sand,
        };
        return (bed, bed);
    }
    // Invented shores are sandy; real ones say so themselves (the
    // map marks sand, and it arrives here as the Desert biome).
    let beach = source().is_none() && h <= SEA + 1 && col.biome != Biome::Tundra && col.biome != Biome::Taiga;
    if beach {
        return (wk.sand, wk.sand);
    }
    match col.biome {
        Biome::Desert => (wk.sand, wk.sandstone),
        Biome::Badlands => {
            let band = wk.bands[(h.rem_euclid(8) / 2) as usize];
            (band, band)
        }
        Biome::Tundra | Biome::Taiga => (wk.snowy_grass, wk.dirt),
        Biome::Mountain => {
            if h > SEA + 54 {
                (wk.snow, wk.stone)
            } else {
                (wk.stone, wk.stone)
            }
        }
        _ => (wk.grass, wk.dirt),
    }
}

/// The block at height `y` of a column as the terrain makes it, leaving
/// out caves, ores, trees and everything built: air above the ground,
/// water up to the sea, the top block, the filling, the coloured strata of
/// badlands, and stone below.
pub fn natural_block(wk: &WellKnown, col: &Column, top: Block, fill: Block, y: i32) -> Block {
    let h = col.height;
    if y > h {
        if col.river && h > SEA && y == h {
            return wk.water;
        }
        if y <= SEA {
            let frozen = y == SEA && matches!(col.biome, Biome::Tundra | Biome::Taiga);
            return if frozen { wk.ice } else { wk.water };
        }
        return crate::block::AIR;
    }
    if y == h {
        return if col.river && h > SEA { wk.water } else { top };
    }
    if y <= 0 {
        return wk.deepstone;
    }
    if col.biome == Biome::Badlands && y > SEA && y >= h - 20 {
        return wk.bands[(y.rem_euclid(8) / 2) as usize];
    }
    if y >= h - 3 {
        return fill;
    }
    wk.stone
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::reg;
    use crate::worldgen;

    #[test]
    fn badlands_bands_follow_the_height_and_deserts_have_sand_on_sandstone() {
        let wk = &reg().wk;
        let mut col = Column { height: 70, biome: Biome::Badlands, river: false, paved: false, building: 0, road: false, farm: false };
        let mut seen = alloc::vec::Vec::new();
        for h in 60..68 {
            col.height = h;
            let (t, f) = surface_blocks(wk, 1, 5, 5, &col);
            assert_eq!(t, f);
            assert_eq!(t, wk.bands[(h % 8 / 2) as usize]);
            if !seen.contains(&t) {
                seen.push(t);
            }
        }
        assert!(!seen.is_empty());
        col.biome = Biome::Desert;
        col.height = 70;
        assert_eq!(surface_blocks(wk, 1, 5, 5, &col), (wk.sand, wk.sandstone));
        // Under the sea: a bed; the natural column has water over it.
        col.height = SEA - 5;
        let (bed, _) = surface_blocks(wk, 1, 5, 5, &col);
        assert_eq!(natural_block(wk, &col, bed, bed, SEA - 5), bed);
        assert_eq!(natural_block(wk, &col, bed, bed, SEA - 4), wk.water);
        assert_eq!(natural_block(wk, &col, bed, bed, SEA + 1), crate::block::AIR);
    }

    #[test]
    fn the_generator_uses_these_rules() {
        // A real chunk's ground blocks are the ones the rules name (where no cave, build or river interferes).
        let wk = &reg().wk;
        let seed = 21;
        let mut same = 0;
        let mut total = 0;
        for cx in -3..3 {
            let data = worldgen::generate(seed, cx * 5, cx * 3);
            for lx in (0..16).step_by(5) {
                for lz in (0..16).step_by(5) {
                    let (x, z) = (cx * 5 * 16 + lx, cx * 3 * 16 + lz);
                    let col = worldgen::column(seed, x, z);
                    if col.height < SEA || col.river {
                        continue;
                    }
                    let (top, _) = surface_blocks(wk, seed, x, z, &col);
                    total += 1;
                    if data.get(lx, col.height, lz) == top {
                        same += 1;
                    }
                }
            }
        }
        assert!(total == 0 || same * 10 >= total * 8, "{same} of {total}");
    }
}
