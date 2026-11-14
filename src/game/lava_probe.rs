//! How much lava is near, for the sound of it (the lava bed and the pops of
//! `util/audio/ambience.rs`). Part of `ambience_probe.rs`, which runs it a
//! few times a second.
//!
//! Blocks are looked at on a grid three apart out to twelve blocks, five
//! levels around the player's feet; each lava found counts more the closer it
//! is, so the sound swells as you walk up to a lake and a lone cell far off is
//! only a whisper. About 400 lookups.

use crate::block::{fluid_of, Block, AIR};
use crate::math::{clampf, sqrtf};

/// Farthest lava that is heard, blocks.
pub const REACH: f32 = 13.0;
/// Grid spacing and how far out it goes.
const STEP: i32 = 3;
const OUT: i32 = 12;

/// 0 to 1: how much lava there is around `at` (the feet), by `get`.
pub fn lava_near(get: &dyn Fn(i32, i32, i32) -> Block, at: [i32; 3]) -> f32 {
    let mut sum = 0.0f32;
    let mut dx = -OUT;
    while dx <= OUT {
        let mut dz = -OUT;
        while dz <= OUT {
            for dy in -3..=1i32 {
                let b = get(at[0] + dx, at[1] + dy, at[2] + dz);
                if b == AIR {
                    continue;
                }
                if matches!(fluid_of(b), Some((true, _))) {
                    let d = sqrtf((dx * dx + dy * dy + dz * dz) as f32);
                    sum += clampf(1.0 - d / REACH, 0.0, 1.0);
                }
            }
            dz += STEP;
        }
        dx += STEP;
    }
    clampf(sum / 2.5, 0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::reg;

    fn lava_at(cells: &'static [(i32, i32, i32)]) -> impl Fn(i32, i32, i32) -> Block {
        let lava = reg().wk.lava;
        move |x, y, z| if cells.contains(&(x, y, z)) { lava } else { AIR }
    }

    #[test]
    fn no_lava_is_silence() {
        assert_eq!(lava_near(&lava_at(&[]), [0, 60, 0]), 0.0);
    }

    #[test]
    fn lava_within_twelve_blocks_is_heard() {
        // On the grid: twelve blocks out, level with the feet.
        assert!(lava_near(&lava_at(&[(12, 60, 0)]), [0, 60, 0]) > 0.0);
        assert!(lava_near(&lava_at(&[(0, 59, -9)]), [0, 60, 0]) > 0.0);
    }

    #[test]
    fn lava_beyond_the_reach_is_not() {
        assert_eq!(lava_near(&lava_at(&[(24, 60, 0)]), [0, 60, 0]), 0.0);
        assert_eq!(lava_near(&lava_at(&[(0, 40, 0)]), [0, 60, 0]), 0.0);
    }

    #[test]
    fn closer_and_more_lava_is_louder() {
        let near = lava_near(&lava_at(&[(3, 59, 0)]), [0, 60, 0]);
        let far = lava_near(&lava_at(&[(9, 59, 0)]), [0, 60, 0]);
        assert!(near > far && far > 0.0);
        let more = lava_near(&lava_at(&[(3, 59, 0), (3, 59, 3), (0, 59, 3), (0, 59, 0)]), [0, 60, 0]);
        assert!(more > near && more <= 1.0);
    }
}
