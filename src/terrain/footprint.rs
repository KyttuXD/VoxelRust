//! How much memory the loaded world takes (an estimate, for the memory
//! budget: see `util/budget`).

use crate::chunk::VOLUME;
use crate::world::World;

/// Bytes of one chunk's blocks.
pub const CHUNK_BYTES: usize = VOLUME * core::mem::size_of::<crate::block::Block>();

/// What a map entry costs besides what it holds.
const ENTRY: usize = 96;

impl World {
    /// Bytes held by the block data of the loaded chunks and what hangs on them.
    pub fn chunk_bytes(&self) -> usize {
        let mut total = 0usize;
        for c in self.chunks.values() {
            total += ENTRY + c.props.capacity() * core::mem::size_of::<([u8; 3], crate::block::Block)>();
            if c.data.is_some() {
                total += CHUNK_BYTES;
            }
        }
        total += self.far.len() * ENTRY;
        for list in self.overlay.values() {
            total += ENTRY + list.capacity() * core::mem::size_of::<(i32, i32, i32, crate::block::Block)>();
        }
        total
    }

    /// How many chunks have their block data in memory.
    pub fn chunks_in_memory(&self) -> usize {
        self.chunks.values().filter(|c| c.data.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunk_is_128_kilobytes_and_the_world_counts_the_loaded_ones() {
        assert_eq!(CHUNK_BYTES, 128 * 1024);
        let w = World::new(5);
        assert_eq!(w.chunk_bytes(), 0);
        assert_eq!(w.chunks_in_memory(), 0);
    }
}
