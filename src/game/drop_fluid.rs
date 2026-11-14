//! Dropped items in the air, in water and in lava.
//!
//! In the air an item falls and stops at what is solid. In water it floats
//! up to the surface, bobs there, and drifts with the current (the same push
//! the player and the creatures feel). In lava it is gone at once, with a
//! sizzle.

use crate::block::{self, Block};
use crate::game::current;
use crate::math::floord;

/// What became of an item this step.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fate {
    /// Still there.
    Lies,
    /// Burnt up in lava.
    Burnt,
}

/// Gravity in the air, blocks a second squared.
const FALL: f32 = 18.0;
/// How strongly the surface pulls a floating item, and how much it slows.
const BUOY_SPRING: f32 = 14.0;
const WATER_DRAG: f32 = 4.0;
/// How far under the surface an item rides.
const RIDES: f64 = 0.15;
/// How hard the current carries it.
const DRIFT: f32 = 3.0;

/// Move an item at `pos` with speed `vel` for `dt` seconds.
pub fn step(get: &dyn Fn(i32, i32, i32) -> Block, pos: &mut [f64; 3], vel: &mut [f32; 3], dt: f32) -> Fate {
    let cell = [floord(pos[0]), floord(pos[1]), floord(pos[2])];
    let here = get(cell[0], cell[1], cell[2]);
    match block::fluid_of(here) {
        Some((true, _)) => return Fate::Burnt,
        Some((false, _)) => {
            // Up to the surface, riding just under it.
            let top = current::surface_at(get, cell).unwrap_or(cell[1] as f64 + 1.0);
            let want = (top - RIDES - pos[1]) as f32;
            vel[1] += (want * BUOY_SPRING - vel[1] * WATER_DRAG) * dt;
            // Along with the current, slowing in the water.
            let (px, pz) = current::current(get, cell);
            for (a, push) in [(0, px), (2, pz)] {
                vel[a] += (push * DRIFT - vel[a]) * (WATER_DRAG * dt).min(1.0);
            }
        }
        None => {
            vel[1] -= FALL * dt;
            // Resting on the surface of water it floated up to: no leaping out.
        }
    }
    let next = [pos[0] + (vel[0] * dt) as f64, pos[1] + (vel[1] * dt) as f64, pos[2] + (vel[2] * dt) as f64];
    let hit = get(floord(next[0]), floord(next[1]), floord(next[2]));
    if block::is_solid(hit) {
        *vel = [0.0; 3];
    } else {
        *pos = next;
        // Dropping into lava from above.
        if matches!(block::fluid_of(hit), Some((true, _))) {
            return Fate::Burnt;
        }
    }
    Fate::Lies
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{reg, AIR};

    /// Stone below y = 10, a pool of `liquid` at y = 10 and 11 where x < 5.
    fn pool(liquid: Block) -> impl Fn(i32, i32, i32) -> Block {
        move |x, y, _z| {
            if y < 10 {
                reg().wk.stone
            } else if y <= 11 && x < 5 {
                liquid
            } else {
                AIR
            }
        }
    }

    fn run(get: &dyn Fn(i32, i32, i32) -> Block, mut pos: [f64; 3], mut vel: [f32; 3], seconds: f32) -> ([f64; 3], [f32; 3], Fate) {
        for _ in 0..(seconds * 60.0) as i32 {
            if step(get, &mut pos, &mut vel, 1.0 / 60.0) == Fate::Burnt {
                return (pos, vel, Fate::Burnt);
            }
        }
        (pos, vel, Fate::Lies)
    }

    #[test]
    fn an_item_falls_and_lies_on_the_ground() {
        let w = pool(AIR);
        let (pos, _, fate) = run(&w, [8.5, 14.0, 0.5], [0.0; 3], 3.0);
        assert_eq!(fate, Fate::Lies);
        assert!(pos[1] >= 10.0 && pos[1] < 10.3, "y = {}", pos[1]);
    }

    #[test]
    fn an_item_in_water_floats_to_the_surface() {
        let w = pool(reg().wk.water);
        let (pos, _, fate) = run(&w, [2.5, 10.2, 0.5], [0.0; 3], 4.0);
        assert_eq!(fate, Fate::Lies);
        // The surface is at 11.875; the item rides just under it.
        assert!(pos[1] > 11.4 && pos[1] < 11.95, "y = {}", pos[1]);
    }

    #[test]
    fn an_item_dropped_into_water_does_not_sink_to_the_bottom() {
        let w = pool(reg().wk.water);
        let (pos, _, _) = run(&w, [2.5, 14.0, 0.5], [0.0, -2.0, 0.0], 5.0);
        assert!(pos[1] > 11.0, "y = {}", pos[1]);
    }

    #[test]
    fn lava_burns_an_item_up() {
        let w = pool(reg().wk.lava);
        let (_, _, fate) = run(&w, [2.5, 14.0, 0.5], [0.0; 3], 2.0);
        assert_eq!(fate, Fate::Burnt);
        // Even one lying where the lava arrives.
        let (_, _, fate) = run(&w, [2.5, 10.5, 0.5], [0.0; 3], 0.1);
        assert_eq!(fate, Fate::Burnt);
    }

    #[test]
    fn running_water_carries_an_item_downstream() {
        let r = reg();
        // A strip of flowing water getting shallower toward +x.
        let get = move |x: i32, y: i32, z: i32| -> Block {
            if y < 10 {
                r.wk.stone
            } else if y == 10 && z == 0 && (0..7).contains(&x) {
                r.flow[0][(7 - x) as usize]
            } else {
                AIR
            }
        };
        let (pos, _, _) = run(&get, [1.5, 10.2, 0.5], [0.0; 3], 1.5);
        assert!(pos[0] > 2.5, "x = {}", pos[0]);
    }
}
