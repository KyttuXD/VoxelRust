//! A chunk: a 16 x 256 x 16 column of blocks.

use crate::block::Block;
use alloc::vec::Vec;

/// Chunk width and depth in blocks.
pub const CW: i32 = 16;
/// World height in blocks.
pub const CH: i32 = 256;
/// The height worlds had before they were made taller; chunks saved then
/// are that high and are stood in the bottom of a new one when read.
pub const OLD_CH: i32 = 128;
pub const VOLUME: usize = (CW * CW * CH) as usize;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ChunkData {
    /// Indexed `(x * 16 + z) * CH + y`, so each vertical column is contiguous.
    pub blocks: Vec<Block>,
}

impl ChunkData {
    pub fn new() -> ChunkData {
        ChunkData { blocks: alloc::vec![0; VOLUME] }
    }

    #[inline]
    pub fn idx(x: i32, y: i32, z: i32) -> usize {
        ((x * CW + z) * CH + y) as usize
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32, z: i32) -> Block {
        self.blocks[Self::idx(x, y, z)]
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, z: i32, b: Block) {
        self.blocks[Self::idx(x, y, z)] = b;
    }
}

impl Default for ChunkData {
    fn default() -> Self {
        ChunkData::new()
    }
}

/// Chunk coordinate containing block coordinate `v`.
#[inline]
pub fn chunk_of(v: i32) -> i32 {
    v >> 4
}
