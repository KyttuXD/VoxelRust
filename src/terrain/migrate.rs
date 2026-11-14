//! Bringing saved worlds up to date. Only the chunks a player changed are saved
//! (the rest is made again from the seed, so terrain improvements reach them by
//! themselves); what a saved chunk holds is brought forward here when its region
//! is read, once, and the region file then carries the new data version.
//!
//! Add a step for each new `DATA_VERSION` (and bump it in `save.rs`):
//! - 1: lava that touches water sets to obsidian or cobblestone, as lava does
//!   now when it meets water (older worlds had lava lying against water).
//!
//! What flows is not fixed here but at load time: the cells of water and lava in
//! a chunk read from disk are looked at again by the liquid rules (see
//! [`flowing_cells`]), so unstable or stuck flows settle as the current rules say.

use crate::block::{self, reg, Block, Kind};
use crate::chunk::{ChunkData, CH, CW};

/// Bring a chunk saved at data version `from` up to the current one.
pub fn chunk(c: &mut ChunkData, from: u32) {
    if from < 1 {
        harden_lava_beside_water(c);
    }
}

fn at(c: &ChunkData, x: i32, y: i32, z: i32) -> Option<Block> {
    if (0..CW).contains(&x) && (0..CW).contains(&z) && (0..CH).contains(&y) {
        Some(c.blocks[ChunkData::idx(x, y, z)])
    } else {
        None
    }
}

/// Version 1: lava touching water (inside the chunk) becomes stone.
fn harden_lava_beside_water(c: &mut ChunkData) {
    let r = reg();
    let (obsidian, cobble) = (r.block("obsidian").unwrap_or(r.wk.stone), r.block("cobble").unwrap_or(r.wk.stone));
    let mut set: alloc::vec::Vec<(usize, Block)> = alloc::vec::Vec::new();
    for x in 0..CW {
        for z in 0..CW {
            for y in 0..CH {
                let b = c.blocks[ChunkData::idx(x, y, z)];
                let Some((true, level)) = block::fluid_of(b) else { continue };
                let wet = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]]
                    .iter()
                    .any(|d| at(c, x + d[0], y + d[1], z + d[2]).map_or(false, block::is_water));
                if wet {
                    set.push((ChunkData::idx(x, y, z), if level == 8 { obsidian } else { cobble }));
                }
            }
        }
    }
    for (i, b) in set {
        c.blocks[i] = b;
    }
}

/// The liquid cells of a chunk (as local `[x, y, z]`) that could move: those with
/// air or a plant beside or below them, at most `limit`. Quiet pools and oceans, which
/// are liquid all round, are left out.
pub fn flowing_cells(c: &ChunkData, limit: usize) -> alloc::vec::Vec<[i32; 3]> {
    let r = reg();
    let open = |b: Option<Block>| b.map_or(false, |b| b == 0 || r.kind[b as usize] == Kind::Cross);
    let mut out = alloc::vec::Vec::new();
    'all: for x in 0..CW {
        for z in 0..CW {
            for y in 0..CH {
                let b = c.blocks[ChunkData::idx(x, y, z)];
                if b == 0 || block::fluid_of(b).is_none() {
                    continue;
                }
                if [[1, 0, 0], [-1, 0, 0], [0, 0, 1], [0, 0, -1], [0, -1, 0]].iter().any(|d| open(at(c, x + d[0], y + d[1], z + d[2]))) {
                    out.push([x, y, z]);
                    if out.len() >= limit {
                        break 'all;
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lava_beside_water_is_hardened_once_and_only_in_old_worlds() {
        let r = reg();
        let mut c = ChunkData::default();
        c.blocks[ChunkData::idx(4, 100, 4)] = r.wk.lava;
        c.blocks[ChunkData::idx(5, 100, 4)] = r.wk.water;
        c.blocks[ChunkData::idx(10, 100, 10)] = r.wk.lava;
        let mut now = c.clone();
        chunk(&mut now, 1);
        assert_eq!(now, c, "a current world is left alone");
        chunk(&mut c, 0);
        assert!(block::fluid_of(c.blocks[ChunkData::idx(4, 100, 4)]).is_none(), "lava against water stayed lava");
        assert_eq!(c.blocks[ChunkData::idx(5, 100, 4)], r.wk.water);
        assert_eq!(c.blocks[ChunkData::idx(10, 100, 10)], r.wk.lava, "lone lava was changed");
    }

    #[test]
    fn only_liquid_that_can_move_is_listed() {
        let r = reg();
        let mut c = ChunkData::default();
        for y in 89..95 {
            for x in 0..CW {
                for z in 0..CW {
                    c.blocks[ChunkData::idx(x, y, z)] = if y == 89 { r.wk.stone } else { r.wk.water };
                }
            }
        }
        // A sealed lake: only the top layer has air above, which is not beside or below.
        assert!(flowing_cells(&c, 100).is_empty());
        c.blocks[ChunkData::idx(3, 92, 3)] = 0;
        assert!(!flowing_cells(&c, 100).is_empty());
        assert!(flowing_cells(&c, 2).len() <= 2);
    }
}
