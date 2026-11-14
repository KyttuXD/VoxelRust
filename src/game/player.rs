//! Player movement and collision.
//!
//! The player is a box 0.6 wide and 1.8 tall. Blocks collide with their real
//! shapes, so slabs and stairs can be walked up: a ledge of up to 0.6 blocks
//! is stepped onto without jumping.

use crate::block::{def, Block, Kind};
use crate::math::{cosf, floord, sinf, sqrtf};

pub const HALF_WIDTH: f64 = 0.3;
pub const HEIGHT: f64 = 1.8;
pub const EYE: f64 = 1.62;
const STEP_UP: f64 = 0.6;
const EPS: f64 = 1e-4;

#[derive(Clone, Copy, Default)]
pub struct Input {
    /// -1..=1: backward / forward.
    pub forward: f32,
    /// -1..=1: left / right.
    pub strafe: f32,
    pub jump: bool,
    pub descend: bool,
    pub sprint: bool,
}

#[derive(Clone)]
pub struct Player {
    /// Centre of the feet.
    pub pos: [f64; 3],
    pub vel: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub flying: bool,
    pub in_water: bool,
    /// Half the body's width and its full height, in blocks.
    pub half_width: f64,
    pub height: f64,
    /// Multiplies walking speed (creatures are slower than players).
    pub speed_scale: f32,
    /// Multiplies the speed of a jump (a jump boost effect).
    pub jump_scale: f32,
}

impl Player {
    pub fn new(pos: [f64; 3]) -> Player {
        Player::with_size(pos, HALF_WIDTH * 2.0, HEIGHT)
    }

    /// A body of another size (creatures).
    pub fn with_size(pos: [f64; 3], width: f64, height: f64) -> Player {
        Player {
            pos,
            vel: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            on_ground: false,
            flying: false,
            in_water: false,
            half_width: width * 0.5,
            height,
            speed_scale: 1.0,
            jump_scale: 1.0,
        }
    }

    pub fn eye(&self) -> [f64; 3] {
        [self.pos[0], self.pos[1] + EYE, self.pos[2]]
    }

    fn bounds(&self, pos: [f64; 3]) -> ([f64; 3], [f64; 3]) {
        (
            [pos[0] - self.half_width, pos[1], pos[2] - self.half_width],
            [pos[0] + self.half_width, pos[1] + self.height, pos[2] + self.half_width],
        )
    }

    /// The body's box in the world: (min corner, max corner).
    pub fn aabb(&self) -> ([f64; 3], [f64; 3]) {
        self.bounds(self.pos)
    }

    /// Whether a block placed in `cell` would overlap the player.
    pub fn overlaps_cell(&self, cell: [i32; 3]) -> bool {
        let (mn, mx) = self.bounds(self.pos);
        (0..3).all(|a| mx[a] > cell[a] as f64 && mn[a] < cell[a] as f64 + 1.0)
    }

    /// If the player is inside something solid (spawned in a tree, or the
    /// terrain changed), rise until free.
    pub fn unstick(&mut self, get: &dyn Fn(i32, i32, i32) -> Block) {
        for _ in 0..(crate::chunk::CH + 2) {
            let (mn, mx) = self.bounds(self.pos);
            match touching(get, mn, mx, 1) {
                Some((_, top)) => self.pos[1] = top + EPS,
                None => break,
            }
        }
    }

    pub fn update(&mut self, dt: f32, inp: &Input, get: &dyn Fn(i32, i32, i32) -> Block) {
        let feet = get(floord(self.pos[0]), floord(self.pos[1] + 0.4), floord(self.pos[2]));
        // Water and lava both hold you up, and swimming is the same in either.
        self.in_water = crate::block::fluid_of(feet).is_some();

        // The direction the player wants to go, on the ground plane.
        let (s, c) = (sinf(self.yaw), cosf(self.yaw));
        let mut wx = -s * inp.forward + c * inp.strafe;
        let mut wz = -c * inp.forward - s * inp.strafe;
        let len = sqrtf(wx * wx + wz * wz);
        if len > 1.0 {
            wx /= len;
            wz /= len;
        }

        if self.flying {
            let speed = (if inp.sprint { 24.0 } else { 11.0 }) * self.speed_scale;
            let k = (10.0 * dt).min(1.0);
            let wy = (inp.jump as i32 - inp.descend as i32) as f32;
            self.vel[0] += (wx * speed - self.vel[0]) * k;
            self.vel[2] += (wz * speed - self.vel[2]) * k;
            self.vel[1] += (wy * speed - self.vel[1]) * k;
        } else if self.in_water {
            let speed = (if inp.sprint { 3.4 } else { 2.4 }) * self.speed_scale;
            let k = (5.0 * dt).min(1.0);
            self.vel[0] += (wx * speed - self.vel[0]) * k;
            self.vel[2] += (wz * speed - self.vel[2]) * k;
            self.vel[1] -= 7.0 * dt;
            self.vel[1] *= 1.0 - (2.5 * dt).min(1.0);
            if inp.jump {
                self.vel[1] = (self.vel[1] + 22.0 * dt).min(3.6);
            }
        } else {
            let speed = (if inp.sprint { 6.2 } else { 4.4 }) * self.speed_scale;
            let k = ((if self.on_ground { 14.0 } else { 3.5 }) * dt).min(1.0);
            self.vel[0] += (wx * speed - self.vel[0]) * k;
            self.vel[2] += (wz * speed - self.vel[2]) * k;
            self.vel[1] = (self.vel[1] - 30.0 * dt).max(-60.0);
            if inp.jump && self.on_ground {
                self.vel[1] = 8.8 * self.jump_scale;
            }
        }

        // Move in small steps so nothing is skipped at high speed.
        let longest = (0..3).map(|a| (self.vel[a] * dt).abs_()).fold(0.0f32, f32::max);
        let steps = ((longest / 0.35) as i32 + 1).min(32);
        let sdt = (dt / steps as f32) as f64;
        let was_on_ground = self.on_ground;
        self.on_ground = false;
        for _ in 0..steps {
            // Vertical first, so standing on the ground is known before the
            // horizontal move decides whether it may step up.
            let dy = self.vel[1] as f64 * sdt;
            if self.slide(1, dy, get) {
                if dy < 0.0 {
                    self.on_ground = true;
                }
                self.vel[1] = 0.0;
            }
            for a in [0, 2] {
                let d = self.vel[a] as f64 * sdt;
                if d == 0.0 {
                    continue;
                }
                let before = self.pos;
                if !self.slide(a, d, get) {
                    continue;
                }
                let blocked = self.pos;
                // Try again from one step higher.
                // Swimmers climb out over a shore just as walkers step up to a ledge.
                let grounded = self.on_ground || was_on_ground || self.in_water;
                let mut stepped = false;
                if grounded && !self.flying {
                    self.pos = before;
                    if !self.slide(1, STEP_UP, get) && !self.slide(a, d, get) {
                        self.slide(1, -STEP_UP - 0.01, get);
                        stepped = self.pos[1] > before[1] + 0.01;
                        if stepped {
                            self.on_ground = true;
                        }
                    }
                }
                if !stepped {
                    self.pos = blocked;
                    self.vel[a] = 0.0;
                }
            }
        }
    }

    /// Move `d` along axis `a`, stopping flush against whatever is in the
    /// way. Returns whether something was.
    fn slide(&mut self, a: usize, d: f64, get: &dyn Fn(i32, i32, i32) -> Block) -> bool {
        if d == 0.0 {
            return false;
        }
        self.pos[a] += d;
        let (mn, mx) = self.bounds(self.pos);
        match touching(get, mn, mx, a) {
            None => false,
            Some((lowest, highest)) => {
                let (off_lo, off_hi) = if a == 1 { (0.0, self.height) } else { (self.half_width, self.half_width) };
                self.pos[a] = if d > 0.0 { lowest - off_hi - EPS } else { highest + off_lo + EPS };
                true
            }
        }
    }
}

trait AbsF {
    fn abs_(self) -> f32;
}
impl AbsF for f32 {
    fn abs_(self) -> f32 {
        if self < 0.0 {
            -self
        } else {
            self
        }
    }
}

/// Solid boxes overlapping the box `mn..mx`: the lowest start and highest end
/// of them along axis `a`, or `None` if the space is free.
fn touching(get: &dyn Fn(i32, i32, i32) -> Block, mn: [f64; 3], mx: [f64; 3], a: usize) -> Option<(f64, f64)> {
    let mut found: Option<(f64, f64)> = None;
    let mut shape = alloc::vec::Vec::new();
    for x in floord(mn[0])..=floord(mx[0]) {
        for y in floord(mn[1])..=floord(mx[1]) {
            for z in floord(mn[2])..=floord(mx[2]) {
                let b = get(x, y, z);
                if b == 0 {
                    continue;
                }
                let d = def(b);
                if !d.solid {
                    continue;
                }
                let cell = [x as f64, y as f64, z as f64];
                let mut test = |lo: [f64; 3], hi: [f64; 3]| {
                    if (0..3).all(|i| mx[i] > lo[i] && mn[i] < hi[i]) {
                        found = Some(match found {
                            None => (lo[a], hi[a]),
                            Some((l, h)) => (l.min(lo[a]), h.max(hi[a])),
                        });
                    }
                };
                if d.kind == Kind::Boxes {
                    let mask = crate::block::connect_mask(b, &|dx, dz| get(x + dx, y, z + dz));
                    crate::block::shape(b, mask, &mut shape);
                    for bx in &shape {
                        let lo = [0, 1, 2].map(|i| cell[i] + bx.min[i] as f64 / 16.0);
                        let hi = [0, 1, 2].map(|i| cell[i] + bx.max[i] as f64 / 16.0);
                        test(lo, hi);
                    }
                } else {
                    test(cell, [cell[0] + 1.0, cell[1] + 1.0, cell[2] + 1.0]);
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::reg;

    /// A flat floor at y < 10 with a few extra blocks.
    fn world(extra: &'static [([i32; 3], &'static str)]) -> impl Fn(i32, i32, i32) -> Block {
        move |x, y, z| {
            for (p, id) in extra {
                if *p == [x, y, z] {
                    return reg().block(id).unwrap();
                }
            }
            if y < 10 {
                reg().wk.stone
            } else {
                0
            }
        }
    }

    fn run(p: &mut Player, inp: Input, seconds: f32, get: &dyn Fn(i32, i32, i32) -> Block) {
        let n = (seconds * 60.0) as i32;
        for _ in 0..n {
            p.update(1.0 / 60.0, &inp, get);
        }
    }

    #[test]
    fn falls_and_lands_on_the_floor() {
        let w = world(&[]);
        let mut p = Player::new([0.5, 14.0, 0.5]);
        run(&mut p, Input::default(), 2.0, &w);
        assert!(p.on_ground);
        assert!((p.pos[1] - 10.0).abs() < 1e-3, "y = {}", p.pos[1]);
    }

    #[test]
    fn walls_stop_the_player_and_jumping_clears_one_block() {
        // Yaw 0 looks toward -Z. A two-high wall at z = -3.
        let wall = world(&[([0, 10, -3], "stone"), ([0, 11, -3], "stone")]);
        let mut p = Player::new([0.5, 10.0, 0.5]);
        let fwd = Input { forward: 1.0, ..Default::default() };
        run(&mut p, fwd, 3.0, &wall);
        assert!((p.pos[2] - (-2.0 + HALF_WIDTH)).abs() < 1e-2, "z = {}", p.pos[2]);

        // A single block is too high to step onto, but can be jumped.
        let block = world(&[([0, 10, -3], "stone")]);
        let mut p = Player::new([0.5, 10.0, 0.5]);
        run(&mut p, fwd, 3.0, &block);
        assert!(p.pos[2] > -2.0);
        run(&mut p, Input { forward: 1.0, jump: true, ..Default::default() }, 1.0, &block);
        assert!(p.pos[2] < -2.5, "z = {}", p.pos[2]);
    }

    #[test]
    fn slabs_and_stairs_are_walked_up_without_jumping() {
        // A slab, then a full block behind it: half a block per step.
        let w = world(&[([0, 10, -2], "stone_slab"), ([0, 10, -3], "stone"), ([0, 10, -4], "stone")]);
        let mut p = Player::new([0.5, 10.0, 0.5]);
        run(&mut p, Input { forward: 1.0, ..Default::default() }, 0.8, &w);
        assert!(p.pos[2] < -2.5, "z = {}", p.pos[2]);
        assert!((p.pos[1] - 11.0).abs() < 1e-2, "y = {}", p.pos[1]);

        // Stairs facing the walker (state 0 rises toward -Z).
        let w = world(&[([0, 10, -2], "stone_stairs"), ([0, 10, -3], "stone")]);
        let mut p = Player::new([0.5, 10.0, 0.5]);
        run(&mut p, Input { forward: 1.0, ..Default::default() }, 0.8, &w);
        assert!((p.pos[1] - 11.0).abs() < 1e-2, "stairs: y = {} z = {}", p.pos[1], p.pos[2]);
    }

    #[test]
    fn standing_still_inside_solid_space_does_not_teleport() {
        // Everything solid (as unloaded chunks are): a zero-length step must
        // leave the player where they are instead of ejecting them upward.
        let solid = |_: i32, _: i32, _: i32| reg().wk.stone;
        let mut p = Player::new([0.5, 50.0, 0.5]);
        p.update(0.0, &Input::default(), &solid);
        assert_eq!(p.pos, [0.5, 50.0, 0.5]);
    }

    #[test]
    fn a_swimmer_climbs_out_onto_a_shore_one_block_up() {
        // A pool at x < 10, water at y = 10; land beyond it at x >= 10, one
        // block higher, with a player swimming toward it.
        let stone = reg().wk.stone;
        let water = reg().wk.water;
        let w = move |x: i32, y: i32, _z: i32| -> Block {
            if y < 10 {
                stone
            } else if y == 10 && x < 10 {
                water
            } else if y == 10 && x >= 10 {
                stone
            } else {
                0
            }
        };
        let mut p = Player::new([8.5, 10.0, 5.5]);
        p.yaw = -crate::math::PI * 0.5; // facing +x
        // Swim toward the shore, paddling up now and then.
        let out = Input { forward: 1.0, jump: true, ..Default::default() };
        run(&mut p, out, 4.0, &w);
        assert!(p.pos[0] > 10.0, "still in the pool at x = {}", p.pos[0]);
        assert!(p.pos[1] >= 11.0 - 1e-3, "not up on the shore: y = {}", p.pos[1]);
    }

    #[test]
    fn flying_ignores_gravity_and_water_is_not_solid() {
        let w = world(&[]);
        let mut p = Player::new([0.5, 20.0, 0.5]);
        p.flying = true;
        run(&mut p, Input::default(), 1.0, &w);
        assert!((p.pos[1] - 20.0).abs() < 1e-3);
        run(&mut p, Input { jump: true, ..Default::default() }, 1.0, &w);
        assert!(p.pos[1] > 25.0);

        let pool = world(&[([0, 10, 0], "water"), ([0, 11, 0], "water")]);
        let mut p = Player::new([0.5, 12.5, 0.5]);
        run(&mut p, Input::default(), 3.0, &pool);
        assert!((p.pos[1] - 10.0).abs() < 1e-2, "sank to {}", p.pos[1]);
        assert!(p.in_water);
    }
}
