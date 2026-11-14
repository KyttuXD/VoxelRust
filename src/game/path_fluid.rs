//! The world as walking creatures see it for path finding: like
//! `block::is_solid`, but a walker will not wade into lava or water deep
//! enough to cover its head (it walks round, or stays where it is). Water
//! one block deep is walked through.

use crate::block;
use crate::path::Ground;
use crate::world::World;

/// A [`Ground`] over the world that steers walkers clear of deep water and lava.
pub struct Walker<'a>(pub &'a World);

/// Whether a cell is one a walker keeps out of, given what is in the cell
/// and the one above it.
pub fn dangerous(here: block::Block, above: block::Block) -> bool {
    match block::fluid_of(here) {
        Some((true, _)) => true,
        Some((false, _)) => block::fluid_of(above).is_some(),
        None => matches!(block::fluid_of(above), Some((true, _))),
    }
}

impl Ground for Walker<'_> {
    fn solid(&self, x: i32, y: i32, z: i32) -> bool {
        block::is_solid(self.0.get(x, y, z))
    }

    fn avoid(&self, x: i32, y: i32, z: i32) -> bool {
        dangerous(self.0.get(x, y, z), self.0.get(x, y + 1, z))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{reg, AIR};
    use crate::path;

    /// A flat stone floor at y < 10 where `liquid(x, y, z)` says what stands above it.
    struct Flat<F: Fn(i32, i32, i32) -> block::Block>(F);
    impl<F: Fn(i32, i32, i32) -> block::Block> Ground for Flat<F> {
        fn solid(&self, x: i32, y: i32, z: i32) -> bool {
            y < 10 || block::is_solid((self.0)(x, y, z))
        }
        fn avoid(&self, x: i32, y: i32, z: i32) -> bool {
            dangerous((self.0)(x, y, z), (self.0)(x, y + 1, z))
        }
    }

    #[test]
    fn deep_water_and_lava_are_dangerous_but_a_puddle_is_not() {
        let r = reg();
        assert!(dangerous(r.wk.lava, AIR));
        assert!(dangerous(r.flow[1][2], AIR));
        assert!(dangerous(r.wk.water, r.wk.water));
        assert!(!dangerous(r.wk.water, AIR));
        assert!(!dangerous(AIR, AIR));
        // Air under lava: a head in it.
        assert!(dangerous(AIR, r.wk.lava));
    }

    #[test]
    fn a_walker_goes_round_a_pool_not_through_it() {
        let water = reg().wk.water;
        // A pool two deep at x = 3, z from -5 to 5 (a long wall of water), open at z = 6.
        let g = Flat(move |x, y, z| if x == 3 && (-5..=5).contains(&z) && (10..12).contains(&y) { water } else { AIR });
        let path = path::find_path(&g, [0, 10, 0], [6, 10, 0], 2000).expect("a way round");
        assert!(path.iter().all(|c| !(c[0] == 3 && (-5..=5).contains(&c[2]))), "walked through the water: {path:?}");
        assert_eq!(*path.last().unwrap(), [6, 10, 0]);
    }

    #[test]
    fn a_shallow_stream_is_waded() {
        let water = reg().wk.water;
        let g = Flat(move |x, y, _z| if x == 3 && y == 10 { water } else { AIR });
        let path = path::find_path(&g, [0, 10, 0], [6, 10, 0], 2000).unwrap();
        assert!(path.iter().any(|c| c[0] == 3), "went round a stream that was only a block deep");
    }
}
