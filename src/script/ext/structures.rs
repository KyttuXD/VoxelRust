//! Placing structures as the world is generated: a hash that gives the same
//! number for the same chunk every time (so a structure starts in the same
//! chunks in every copy of a world), and the world's seed. The hook that
//! says a chunk was just generated is `on_chunk_generated`
//! (`game/world_hooks.rs`); the blocks go in with `set_block`, at most
//! [`MAX_WRITES`] in one call of that hook.

use super::args::{coord, int, want};
use super::Ctx;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;

/// Most blocks `set_block` writes in one call of `on_chunk_generated` (after
/// that it answers `false`).
pub const MAX_WRITES: u32 = 4096;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("rand_chunk(cx: i64, cz: i64, salt: i64) -> i64", "a whole number from 0 to 2147483647 that depends only on the world's seed, the chunk (cx, cz: block position divided by 16, rounded down) and the salt: the same every time it is asked, in every copy of the world, so a structure can start in the same chunks again (`rand_chunk(cx, cz, 7) % 100 < 3`)"),
    ("world_seed() -> i64", "the world's seed"),
];

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The number [`rand_chunk`](FUNCTIONS) gives: 31 bits, the same for the same
/// seed, chunk and salt, whatever the platform, and well spread for
/// neighbouring chunks and salts.
pub fn chunk_hash(seed: u32, cx: i32, cz: i32, salt: i64) -> u32 {
    let mut h = mix(seed as u64 ^ 0x9E37_79B9_7F4A_7C15);
    for v in [cx as i64 as u64, cz as i64 as u64, salt as u64] {
        h = mix(h ^ v.wrapping_add(0x9E37_79B9_7F4A_7C15));
    }
    (h >> 33) as u32
}

pub fn call(g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "rand_chunk" => {
            want(name, args, 3)?;
            let (cx, cz, salt) = (coord(name, args, 0)?, coord(name, args, 1)?, int(name, args, 2)?);
            Ok(Value::Int(chunk_hash(g.world.seed, cx, cz, salt) as i64))
        }
        "world_seed" => {
            want(name, args, 0)?;
            Ok(Value::Int(g.world.seed as i64))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_chunk_gives_the_same_number() {
        for (cx, cz) in [(0, 0), (-5, 17), (1_000_000, -1_000_000), (i32::MIN, i32::MAX)] {
            assert_eq!(chunk_hash(42, cx, cz, 3), chunk_hash(42, cx, cz, 3));
        }
    }

    #[test]
    fn it_fits_in_31_bits() {
        for i in 0..2000 {
            assert!(chunk_hash(7, i, -i, i as i64) <= 0x7FFF_FFFF);
        }
    }

    #[test]
    fn seed_chunk_and_salt_each_change_it() {
        let base = chunk_hash(1, 2, 3, 4);
        assert_ne!(base, chunk_hash(2, 2, 3, 4));
        assert_ne!(base, chunk_hash(1, 3, 3, 4));
        assert_ne!(base, chunk_hash(1, 2, 4, 4));
        assert_ne!(base, chunk_hash(1, 2, 3, 5));
        // Swapping the coordinates is another chunk.
        assert_ne!(chunk_hash(1, 2, 3, 4), chunk_hash(1, 3, 2, 4));
    }

    #[test]
    fn about_the_asked_share_of_chunks_start_a_structure() {
        let mut hits = 0;
        for cx in -50..50 {
            for cz in -50..50 {
                if chunk_hash(9, cx, cz, 11) % 100 < 10 {
                    hits += 1;
                }
            }
        }
        // 10 000 chunks at 10 percent: well within 8 to 12 percent.
        assert!((800..1200).contains(&hits), "{} of 10000", hits);
    }
}
