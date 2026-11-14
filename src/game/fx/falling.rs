//! Blocks in flight (`spawn_falling_block`): flying debris a mod throws.
//!
//! A [`Falling`] is a small cube (half a block wide) with a block's picture.
//! It moves under gravity, bounces a little off walls and ceilings, and when
//! it comes down on something solid it lands in the cell it rests in: if
//! that cell is free (air) it becomes the block, otherwise (or where the
//! world may not be changed, or the place is not loaded) it breaks into
//! dust. At most [`MAX_LIVE`] fly at once; one that flies for [`MAX_AGE`]
//! seconds, falls out of the world, or has no loaded ground under it for as
//! long, is gone without a trace.
//!
//! [`step`] and [`landing`] are pure, so they are tested without a world.

use crate::block::{self, Block, AIR};
use crate::math::floord;
use crate::session::Session;
use alloc::vec::Vec;

/// Most falling blocks at once.
pub const MAX_LIVE: usize = 64;
/// Seconds a block may fly before it is dropped from the list.
pub const MAX_AGE: f32 = 30.0;
/// Half the width of the little cube.
pub const HALF: f64 = 0.25;
/// Blocks a second squared.
pub const GRAVITY: f32 = 24.0;
/// Fastest it may move, blocks a second, in each direction.
pub const MAX_SPEED: f32 = 60.0;
/// The farthest a step may move it between looks at the world.
const STEP: f32 = 0.4;
/// Most looks a frame (so a fast one cannot cost much).
const MAX_SUBSTEPS: usize = 16;
/// The height it is dropped at when it falls below the world.
const FLOOR: f64 = -16.0;

/// A block flying or falling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Falling {
    pub block: Block,
    /// The middle of the cube.
    pub pos: [f64; 3],
    pub vel: [f32; 3],
    /// Seconds it has been flying.
    pub age: f32,
}

/// What a step came to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fate {
    Flying,
    /// Came to rest on something solid, in this cell.
    Landed([i32; 3]),
    /// Out of the world or out of time.
    Lost,
}

/// What becomes of a block that has landed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Landing {
    /// It becomes the block it was.
    Place,
    /// It falls apart.
    Break,
}

/// What a landing comes to: the block is put in the cell if the cell is free
/// (air), the world may be changed, and the cell is inside the world's
/// height; else it breaks.
pub fn landing(cell_is_air: bool, may_change: bool, cell_y: i32) -> Landing {
    if cell_is_air && may_change && (1..crate::chunk::CH).contains(&cell_y) {
        Landing::Place
    } else {
        Landing::Break
    }
}

/// Whether a cube of this half-width at `p` overlaps a solid cell.
fn blocked(p: [f64; 3], solid: &dyn Fn(i32, i32, i32) -> bool) -> bool {
    let lo = [floord(p[0] - HALF), floord(p[1] - HALF), floord(p[2] - HALF)];
    let hi = [floord(p[0] + HALF), floord(p[1] + HALF), floord(p[2] + HALF)];
    for x in lo[0]..=hi[0] {
        for y in lo[1]..=hi[1] {
            for z in lo[2]..=hi[2] {
                if solid(x, y, z) {
                    return true;
                }
            }
        }
    }
    false
}

/// Move a block on by `dt` seconds. `solid` says whether the cell at x, y, z
/// stops it. Gravity pulls, each direction is tried on its own so it slides
/// along walls, and the distance is cut into steps no longer than 0.4 of a
/// block so a fast one cannot go through a thin wall.
pub fn step(f: &mut Falling, dt: f32, solid: &dyn Fn(i32, i32, i32) -> bool) -> Fate {
    if !f.pos.iter().all(|v| v.is_finite()) || !f.vel.iter().all(|v| v.is_finite()) {
        return Fate::Lost;
    }
    let dt = dt.clamp(0.0, 0.25);
    f.age += dt;
    if f.age > MAX_AGE || f.pos[1] < FLOOR {
        return Fate::Lost;
    }
    f.vel[1] -= GRAVITY * dt;
    for v in f.vel.iter_mut() {
        *v = v.clamp(-MAX_SPEED, MAX_SPEED);
    }
    let fastest = f.vel.iter().fold(0.0f32, |m, v| if *v > m { *v } else if -*v > m { -*v } else { m });
    let n = {
        let want = (fastest * dt / STEP) as usize + 1;
        if want > MAX_SUBSTEPS {
            MAX_SUBSTEPS
        } else {
            want
        }
    };
    let h = dt / n as f32;
    for _ in 0..n {
        for axis in [0usize, 2, 1] {
            let move_by = (f.vel[axis] * h) as f64;
            if move_by == 0.0 {
                continue;
            }
            let mut to = f.pos;
            to[axis] += move_by;
            if !blocked(to, solid) {
                f.pos = to;
                continue;
            }
            if axis == 1 && move_by < 0.0 {
                // Down onto something: rest on top of it, and land.
                let below = floord(to[1] - HALF);
                f.pos[1] = below as f64 + 1.0 + HALF;
                f.vel = [0.0; 3];
                return Fate::Landed([floord(f.pos[0]), floord(f.pos[1]), floord(f.pos[2])]);
            }
            // A wall or a ceiling: bounce off a little.
            f.vel[axis] = -f.vel[axis] * 0.3;
        }
    }
    Fate::Flying
}

/// Make a block fly: the caller has checked the numbers. `false` when
/// [`MAX_LIVE`] are already in flight.
pub fn launch(list: &mut Vec<Falling>, b: Block, pos: [f64; 3], vel: [f32; 3]) -> bool {
    if list.len() >= MAX_LIVE || b == AIR {
        return false;
    }
    list.push(Falling { block: b, pos, vel, age: 0.0 });
    true
}

/// A grey-brown for the dust of a block that broke.
fn dust_colour(b: Block) -> [u8; 3] {
    let _ = b;
    [150, 132, 112]
}

/// Move every falling block of the session on by `dt` and settle the ones
/// that landed.
pub fn tick(g: &mut Session, dt: f32) {
    let mut list = core::mem::take(&mut super::state(g).falling);
    if list.is_empty() {
        super::state(g).falling = list;
        return;
    }
    let may = super::may_change(g);
    let mut i = 0;
    while i < list.len() {
        let (cx, cz) = (floord(list[i].pos[0]), floord(list[i].pos[2]));
        // Where the land is not loaded it waits (and in time is dropped).
        let fate = if g.world.is_loaded(cx, cz) {
            let world = &g.world;
            step(&mut list[i], dt, &|x, y, z| block::is_solid(world.get(x, y, z)))
        } else {
            list[i].age += dt.clamp(0.0, 0.25);
            if list[i].age > MAX_AGE {
                Fate::Lost
            } else {
                Fate::Flying
            }
        };
        match fate {
            Fate::Flying => i += 1,
            Fate::Lost => {
                list.swap_remove(i);
            }
            Fate::Landed(cell) => {
                let f = list.swap_remove(i);
                let free = g.world.get(cell[0], cell[1], cell[2]) == AIR;
                let placed = landing(free, may, cell[1]) == Landing::Place && g.world.set_block(cell[0], cell[1], cell[2], f.block);
                if !placed {
                    // Falls apart into dust.
                    let col = dust_colour(f.block);
                    for _ in 0..4 {
                        let (a, b) = (g.primed.unit() - 0.5, g.primed.unit() - 0.5);
                        g.primed.spark(f.pos, [a * 3.0, 1.5, b * 3.0], 0.5, col);
                    }
                }
            }
        }
    }
    // Blocks launched while this ran (none can be, but be safe) stay.
    let s = super::state(g);
    list.append(&mut s.falling);
    s.falling = list;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world with a solid floor at y <= 9 and nothing else.
    fn floor(_x: i32, y: i32, _z: i32) -> bool {
        y <= 9
    }

    fn fall(b: Block, pos: [f64; 3], vel: [f32; 3]) -> Falling {
        Falling { block: b, pos, vel, age: 0.0 }
    }

    fn run(f: &mut Falling, solid: &dyn Fn(i32, i32, i32) -> bool, seconds: f32) -> Fate {
        let mut t = 0.0;
        while t < seconds {
            match step(f, 1.0 / 60.0, solid) {
                Fate::Flying => {}
                other => return other,
            }
            t += 1.0 / 60.0;
        }
        Fate::Flying
    }

    #[test]
    fn a_dropped_block_lands_on_the_floor_in_the_cell_above_it() {
        let mut f = fall(5, [0.5, 20.5, 0.5], [0.0; 3]);
        let fate = run(&mut f, &floor, 5.0);
        assert_eq!(fate, Fate::Landed([0, 10, 0]));
        assert!((f.pos[1] - 10.25).abs() < 1e-9, "{}", f.pos[1]);
        assert_eq!(f.vel, [0.0; 3]);
    }

    #[test]
    fn a_thrown_block_lands_away_from_where_it_was_thrown() {
        let mut f = fall(5, [0.5, 12.5, 0.5], [6.0, 4.0, 0.0]);
        match run(&mut f, &floor, 5.0) {
            Fate::Landed(c) => assert!(c[0] > 2 && c[1] == 10, "{:?}", c),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn a_fast_block_does_not_go_through_a_thin_floor() {
        // A floor one block thick at y = 9.
        let thin = |_x: i32, y: i32, _z: i32| y == 9;
        let mut f = fall(5, [0.5, 40.5, 0.5], [0.0, -60.0, 0.0]);
        assert_eq!(run(&mut f, &thin, 5.0), Fate::Landed([0, 10, 0]));
    }

    #[test]
    fn a_wall_turns_it_back_and_a_ceiling_too() {
        // A wall at x >= 3, nothing else, and no floor for a while.
        let wall = |x: i32, _y: i32, _z: i32| x >= 3;
        let mut f = fall(5, [1.5, 50.5, 0.5], [20.0, 0.0, 0.0]);
        for _ in 0..30 {
            assert_eq!(step(&mut f, 1.0 / 60.0, &wall), Fate::Flying);
        }
        assert!(f.pos[0] < 3.0 - HALF + 1e-6, "inside the wall at {}", f.pos[0]);
        assert!(f.vel[0] <= 0.0, "still pushing into the wall");
        let ceiling = |_x: i32, y: i32, _z: i32| y >= 60;
        let mut f = fall(5, [0.5, 58.5, 0.5], [0.0, 30.0, 0.0]);
        for _ in 0..30 {
            assert_eq!(step(&mut f, 1.0 / 60.0, &ceiling), Fate::Flying);
        }
        assert!(f.pos[1] < 60.0 - HALF + 1e-6);
    }

    #[test]
    fn falling_out_of_the_world_or_flying_too_long_loses_it() {
        let mut f = fall(5, [0.5, -17.0, 0.5], [0.0; 3]);
        assert_eq!(step(&mut f, 0.016, &|_, _, _| false), Fate::Lost);
        let mut f = fall(5, [0.5, 100.5, 0.5], [0.0; 3]);
        f.age = MAX_AGE;
        assert_eq!(step(&mut f, 0.1, &|_, _, _| false), Fate::Lost);
        let mut f = fall(5, [f64::NAN, 100.5, 0.5], [0.0; 3]);
        assert_eq!(step(&mut f, 0.016, &|_, _, _| false), Fate::Lost);
        let mut f = fall(5, [0.5, 100.5, 0.5], [f32::INFINITY, 0.0, 0.0]);
        assert_eq!(step(&mut f, 0.016, &|_, _, _| false), Fate::Lost);
    }

    #[test]
    fn speed_is_held_and_a_long_frame_is_cut_short() {
        let mut f = fall(5, [0.5, 500.5, 0.5], [0.0, -1000.0, 0.0]);
        step(&mut f, 100.0, &|_, _, _| false);
        assert!(f.vel[1] >= -MAX_SPEED);
        // 0.25 s at 60 blocks a second is 15 blocks at most.
        assert!(f.pos[1] > 500.5 - 16.0, "moved to {}", f.pos[1]);
    }

    #[test]
    fn landing_needs_a_free_cell_and_the_right_to_build() {
        assert_eq!(landing(true, true, 10), Landing::Place);
        assert_eq!(landing(false, true, 10), Landing::Break);
        assert_eq!(landing(true, false, 10), Landing::Break);
        assert_eq!(landing(true, true, 0), Landing::Break);
        assert_eq!(landing(true, true, -3), Landing::Break);
        assert_eq!(landing(true, true, crate::chunk::CH), Landing::Break);
        assert_eq!(landing(true, true, crate::chunk::CH - 1), Landing::Place);
    }

    #[test]
    fn at_most_sixty_four_fly_and_air_is_not_a_block() {
        let mut list = Vec::new();
        for _ in 0..MAX_LIVE {
            assert!(launch(&mut list, 3, [0.0; 3], [0.0; 3]));
        }
        assert!(!launch(&mut list, 3, [0.0; 3], [0.0; 3]));
        assert_eq!(list.len(), MAX_LIVE);
        let mut empty = Vec::new();
        assert!(!launch(&mut empty, AIR, [0.0; 3], [0.0; 3]));
        assert!(empty.is_empty());
    }
}
