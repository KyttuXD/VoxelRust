//! A chunk on the wire: the blocks run-length encoded exactly as the save
//! files do (`save::encode`, with the server's state numbers: both sides use
//! the same content, so they mean the same blocks), deflated, and cut into
//! pieces that fit a message.

use crate::block::Block;
use crate::chunk::{ChunkData, VOLUME};
use crate::proto::{MAX_CHUNK_PART, MAX_CHUNK_PARTS};
use alloc::vec::Vec;

/// The most bytes the run-length encoding of a chunk can be (every block
/// differs from the one before: a count and a state each).
pub const MAX_RLE: usize = VOLUME * 4;

/// The compressed bytes of a chunk (a zlib stream of its run-length encoding).
pub fn compress(c: &ChunkData) -> Vec<u8> {
    crate::image::deflate::zlib(&crate::save::encode(c))
}

/// The chunk in compressed bytes cut into the pieces of [`crate::proto::Msg::ChunkData`];
/// `None` if it would take more pieces than may be sent (noise of all different blocks).
pub fn pieces(whole: &[u8]) -> Option<Vec<Vec<u8>>> {
    if whole.is_empty() || whole.len() > MAX_CHUNK_PART * MAX_CHUNK_PARTS {
        return None;
    }
    Some(whole.chunks(MAX_CHUNK_PART).map(|p| p.to_vec()).collect())
}

/// Compress a chunk and cut it into pieces.
pub fn pack(c: &ChunkData) -> Option<Vec<Vec<u8>>> {
    pieces(&compress(c))
}

/// A chunk from its compressed bytes; `None` if they are damaged or say
/// something that is not a chunk. State numbers the game does not know become air.
pub fn unpack(whole: &[u8]) -> Option<ChunkData> {
    // A zlib header, the deflate blocks and four bytes of checksum.
    if whole.len() < 7 || whole[0] & 0x0F != 8 || (u32::from(whole[0]) << 8 | u32::from(whole[1])) % 31 != 0 {
        return None;
    }
    let rle = crate::png::inflate_raw(&whole[2..], MAX_RLE)?;
    let n = crate::block::reg().blocks.len().min(Block::MAX as usize + 1);
    let identity: Vec<Block> = (0..n).map(|i| i as Block).collect();
    crate::save::decode(&rle, &identity)
}

/// Put the edits of a chunk on top of it: (x in the chunk, y, z in the chunk, state).
pub fn apply_edits(c: &mut ChunkData, edits: &[(i32, i32, i32, Block)]) {
    for &(x, y, z, b) in edits {
        if (0..crate::chunk::CW).contains(&x) && (0..crate::chunk::CH).contains(&y) && (0..crate::chunk::CW).contains(&z) {
            c.set(x, y, z, b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ChunkData {
        let mut c = ChunkData::new();
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..40 {
                    c.set(x, y, z, if y < 30 { 1 } else { 2 });
                }
                c.set(x, 40, z, ((x + z) % 3) as Block);
            }
        }
        c
    }

    #[test]
    fn a_chunk_survives_the_trip_and_is_small() {
        let c = sample();
        let whole = compress(&c);
        assert!(whole.len() < 4000, "{} bytes", whole.len());
        assert_eq!(unpack(&whole).as_ref(), Some(&c));
        let parts = pieces(&whole).unwrap();
        assert_eq!(parts.concat(), whole);
        assert!(parts.iter().all(|p| p.len() <= MAX_CHUNK_PART));
    }

    #[test]
    fn edits_are_part_of_the_chunk_that_is_sent() {
        let mut c = sample();
        apply_edits(&mut c, &[(3, 41, 4, 2), (0, 0, 0, 0), (99, 1, 1, 2), (1, 999, 1, 2), (1, -1, 1, 2)]);
        assert_eq!(c.get(3, 41, 4), 2);
        assert_eq!(c.get(0, 0, 0), 0);
        let back = unpack(&compress(&c)).unwrap();
        assert_eq!(back.get(3, 41, 4), 2);
        assert_eq!(back, c);
    }

    #[test]
    fn noise_and_damage_are_refused_not_trusted() {
        assert!(unpack(&[]).is_none());
        assert!(unpack(b"not a zlib stream at all").is_none());
        let mut whole = compress(&sample());
        let cut = whole.len() / 2;
        whole.truncate(cut);
        assert!(unpack(&whole).is_none());
        // Runs that add up to more than a chunk.
        let mut rle = Vec::new();
        for _ in 0..3 {
            rle.extend_from_slice(&60_000u16.to_le_bytes());
            rle.extend_from_slice(&1u16.to_le_bytes());
        }
        assert!(unpack(&crate::image::deflate::zlib(&rle)).is_none());
        // Too many pieces.
        assert!(pieces(&alloc::vec![1u8; MAX_CHUNK_PART * MAX_CHUNK_PARTS + 1]).is_none());
        assert!(pieces(&[]).is_none());
    }

    #[test]
    fn a_big_answer_is_cut_into_several_pieces() {
        let whole: Vec<u8> = (0..MAX_CHUNK_PART * 2 + 5).map(|i| i as u8).collect();
        let parts = pieces(&whole).unwrap();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[2].len(), 5);
    }
}
