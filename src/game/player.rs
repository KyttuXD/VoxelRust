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
/// A body whose box holds more cells than this tests only the slab it moves through.
const BIG_BOX: f64 = 1200.0;
const EPS: f64 = 1e-4;
// Close to Minecraft's numbers, in blocks and seconds: walking 4.317, sprinting
// 5.612, a jump about 1.25 blocks high in 0.56 s, gravity 32, falling at most 78.
const WALK: f32 = 4.317;
const SPRINT: f32 = 5.612;
const GRAVITY: f32 = 32.0;
const TERMINAL: f32 = 78.4;
const JUMP: f32 = 9.0;
/// What a jump from a sprint adds to the speed along the way you run, blocks a second.
const SPRINT_JUMP_BOOST: f32 = 2.0;

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
    /// In lava rather than water (`in_water` is true then too).
    pub in_lava: bool,
    /// Half the body's width and its full height, in blocks.
    pub half_width: f64,
    pub height: f64,
    /// Multiplies walking speed (creatures are slower than players).
    pub speed_scale: f32,
    /// Multiplies the speed of a jump (a jump boost effect).
    pub jump_scale: f32,
    /// Upward speed an effect lifts the body at (levitation); 0 for none.
    pub lift: f32,
    /// How far down in a crouch (0..1): holding the descend key on the
    /// ground. Lowers the eyes and slows the walk (content/pose/crouch.rs).
    pub crouch: f32,
    /// Set while holding on to something climbable: the vertical speed
    /// (no gravity, no jumping). The session sets it each frame.
    pub climb: Option<f32>,
    /// The tallest ledge it steps onto without jumping (0.6; more for the
    /// big creatures of `creature_size`).
    pub step_up: f64,
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
            in_lava: false,
            half_width: width * 0.5,
            height,
            speed_scale: 1.0,
            jump_scale: 1.0,
            lift: 0.0,
            crouch: 0.0,
            climb: None,
            step_up: STEP_UP,
        }
    }

    /// A creature's body: as [`Player::with_size`], and one past the old size
    /// limits steps up in proportion to its width.
    pub fn with_creature_size(pos: [f64; 3], width: f64, height: f64) -> Player {
        let mut p = Player::with_size(pos, width, height);
        p.resize(width, height);
        p
    }

    /// Change the box (a script resized the creature). Where it stands stays.
    pub fn resize(&mut self, width: f64, height: f64) {
        self.half_width = width * 0.5;
        self.height = height;
        self.step_up = if crate::creature_size::is_big(width as f32, height as f32) { crate::creature_size::step_up(width).max(STEP_UP) } else { STEP_UP };
    }

    pub fn eye(&self) -> [f64; 3] {
        [self.pos[0], self.pos[1] + EYE - crate::content::pose::crouch::EYE_DROP * self.crouch as f64, self.pos[2]]
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
        self.sanitize();
        for _ in 0..(crate::chunk::CH + 2) {
            let (mn, mx) = self.bounds(self.pos);
            match touching(get, mn, mx, 1) {
                Some((_, top)) => self.pos[1] = top + EPS,
                None => break,
            }
        }
    }

    /// Put right a body whose numbers went wrong (NaN or infinity from a bad
    /// hook, a hostile server or a damaged save): such a position would make
    /// every cell lookup nonsense and never recover.
    pub fn sanitize(&mut self) {
        for v in self.vel.iter_mut() {
            if !v.is_finite() {
                *v = 0.0;
            }
        }
        for (a, p) in self.pos.iter_mut().enumerate() {
            if !p.is_finite() {
                *p = if a == 1 { 150.0 } else { 0.0 };
            }
        }
    }

    pub fn update(&mut self, dt: f32, inp: &Input, get: &dyn Fn(i32, i32, i32) -> Block) {
        self.sanitize();
        let dt = if dt.is_finite() && dt > 0.0 { dt } else { 0.0 };
        let feet = get(floord(self.pos[0]), floord(self.pos[1] + 0.4), floord(self.pos[2]));
        // Water and lava both hold you up, and swimming is the same in either.
        self.in_water = crate::block::fluid_of(feet).is_some();
        self.in_lava = matches!(crate::block::fluid_of(feet), Some((true, _)));
        let crouching = inp.descend && self.on_ground && !self.flying && !self.in_water;
        self.crouch = crate::content::pose::crouch::approach(self.crouch, crouching, dt);

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
            let speed = (if inp.sprint { 21.8 } else { 10.9 }) * self.speed_scale;
            let k = (10.0 * dt).min(1.0);
            let wy = (inp.jump as i32 - inp.descend as i32) as f32;
            self.vel[0] += (wx * speed - self.vel[0]) * k;
            self.vel[2] += (wz * speed - self.vel[2]) * k;
            self.vel[1] += (wy * speed - self.vel[1]) * k;
        } else if self.in_water {
            // Lava is thick: half the pace.
            let speed = (if inp.sprint { 3.4 } else { 2.4 }) * self.speed_scale * if self.in_lava { 0.5 } else { 1.0 };
            let k = (5.0 * dt).min(1.0);
            // Running water carries you along, toward where it is lower.
            let (px, pz) = crate::game::current::push_at(get, self.pos);
            self.vel[0] += px * 6.0 * dt;
            self.vel[2] += pz * 6.0 * dt;
            self.vel[0] += (wx * speed - self.vel[0]) * k;
            self.vel[2] += (wz * speed - self.vel[2]) * k;
            // Held up by the liquid, by how much of the body is under (game/buoyancy.rs).
            let under = crate::game::current::submerged(get, self.pos, self.height);
            self.vel[1] = crate::game::buoyancy::vertical(self.vel[1], dt, under, self.in_lava, inp.jump, inp.descend);
        } else {
            // A crouch walks slowly, and does not sprint.
            let pace = if self.crouch > 0.0 { WALK * (1.0 - (1.0 - crate::content::pose::crouch::SPEED) * self.crouch) } else if inp.sprint { SPRINT } else { WALK };
            let speed = pace * self.speed_scale;
            // On the ground the pace is taken up quickly. In the air what speed there is is kept
            // (faster than the pace, from a sprinting jump, it is lost only slowly).
            let in_air_fast = !self.on_ground && sqrtf(self.vel[0] * self.vel[0] + self.vel[2] * self.vel[2]) > speed * 1.02;
            let k = ((if self.on_ground { 12.0 } else if in_air_fast { 0.4 } else { 2.0 }) * dt).min(1.0);
            self.vel[0] += (wx * speed - self.vel[0]) * k;
            self.vel[2] += (wz * speed - self.vel[2]) * k;
            if let Some(up) = self.climb {
                self.vel[1] = up;
            } else {
                self.vel[1] = (self.vel[1] - GRAVITY * dt).max(-TERMINAL);
                if inp.jump && self.on_ground {
                    self.vel[1] = JUMP * self.jump_scale;
                    // A sprinting jump springs forward: the speed it leaves the ground with is more
                    // than the sprint (about 7.5 blocks a second), as in Minecraft.
                    let dl = sqrtf(wx * wx + wz * wz);
                    if inp.sprint && self.crouch <= 0.0 && dl > 0.1 {
                        self.vel[0] += wx / dl * SPRINT_JUMP_BOOST * self.speed_scale;
                        self.vel[2] += wz / dl * SPRINT_JUMP_BOOST * self.speed_scale;
                    }
                }
            }
        }

        // Levitation: lifted off the ground, whatever else holds.
        if self.lift > 0.0 && !self.flying {
            self.vel[1] = crate::game::effects::body::levitate(self.vel[1], self.lift, dt);
            self.climb = None;
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
                    if !self.slide(1, self.step_up, get) && !self.slide(a, d, get) {
                        self.slide(1, -self.step_up - 0.01, get);
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
        let old = self.pos;
        self.pos[a] += d;
        let (mut mn, mut mx) = self.bounds(self.pos);
        // A huge body (a giant creature) tests only the slab it just moved
        // through, not its whole box: the box may hold tens of thousands of
        // cells. (Ordinary bodies test the whole box, as they always did.)
        if self.half_width * self.half_width * 4.0 * self.height > BIG_BOX {
            let (omn, omx) = self.bounds(old);
            if d > 0.0 {
                mn[a] = mn[a].max(omx[a]);
            } else {
                mx[a] = mx[a].min(omn[a]);
            }
        }
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
    fn a_giant_lands_and_steps_up_a_ledge_its_width_allows_but_a_small_body_does_not() {
        // A floor at y 10 and, from z = -3 on, a ledge three blocks higher.
        let get = |_x: i32, y: i32, z: i32| -> Block {
            let top = if z <= -3 { 13 } else { 10 };
            if y < top {
                reg().wk.stone
            } else {
                0
            }
        };
        let forward = Input { forward: 1.0, ..Default::default() };
        let mut giant = Player::with_creature_size([0.5, 12.0, 20.5], 16.0, 40.0);
        assert_eq!(giant.step_up, 4.0);
        run(&mut giant, Input::default(), 2.0, &get);
        assert!(giant.on_ground && (giant.pos[1] - 10.0).abs() < 1e-2, "y = {}", giant.pos[1]);
        run(&mut giant, forward, 6.0, &get);
        assert!((giant.pos[1] - 13.0).abs() < 0.05, "it climbed to y = {}", giant.pos[1]);
        assert!(giant.pos[2] < 0.0);
        // The same ledge stops an ordinary body (which keeps the old 0.6).
        let mut small = Player::with_creature_size([0.5, 12.0, 20.5], 0.6, 1.8);
        assert_eq!(small.step_up, 0.6);
        run(&mut small, Input::default(), 2.0, &get);
        run(&mut small, forward, 6.0, &get);
        assert!((small.pos[1] - 10.0).abs() < 0.05 && small.pos[2] > -2.9, "{:?}", small.pos);
        // Resizing changes how high it steps.
        small.resize(20.0, 50.0);
        assert_eq!(small.step_up, 5.0);
        small.resize(1.0, 2.0);
        assert_eq!(small.step_up, 0.6);
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
    fn a_body_with_nan_numbers_is_put_right() {
        let w = world(&[]);
        let mut p = Player::new([f64::NAN, f64::INFINITY, 0.5]);
        p.vel = [f32::NAN, f32::NEG_INFINITY, 1.0];
        p.update(f32::NAN, &Input::default(), &w);
        assert!(p.pos.iter().all(|v| v.is_finite()), "{:?}", p.pos);
        assert!(p.vel.iter().all(|v| v.is_finite()), "{:?}", p.vel);
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
    fn crouching_lowers_the_eyes_and_slows_the_walk() {
        let w = world(&[]);
        let mut walker = Player::new([0.5, 10.0, 0.5]);
        let mut crouched = Player::new([0.5, 10.0, 0.5]);
        // (The world is flat stone below y = 10.)
        run(&mut walker, Input::default(), 0.5, &w);
        run(&mut crouched, Input::default(), 0.5, &w);
        let standing_eye = crouched.eye()[1];
        run(&mut walker, Input { forward: 1.0, ..Default::default() }, 1.0, &w);
        run(&mut crouched, Input { forward: 1.0, descend: true, ..Default::default() }, 1.0, &w);
        assert!(crouched.crouch > 0.99);
        assert!(crouched.eye()[1] < standing_eye - 0.2, "eyes {} vs {}", crouched.eye()[1], standing_eye);
        let (a, b) = ((0.5 - walker.pos[2]).abs(), (0.5 - crouched.pos[2]).abs());
        assert!(b < a * 0.6 && b > 0.5, "walked {a} blocks, crouched {b}");
        // Let go and the eyes come back up.
        run(&mut crouched, Input::default(), 0.5, &w);
        assert!(crouched.crouch < 0.01);
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

        // Water holds a swimmer up: it floats with its head out, not on the floor.
        let pool = world(&[([0, 10, 0], "water"), ([0, 11, 0], "water")]);
        let mut p = Player::new([0.5, 12.5, 0.5]);
        run(&mut p, Input::default(), 4.0, &pool);
        assert!(p.pos[1] > 10.2 && p.pos[1] < 10.8, "floats at {}", p.pos[1]);
        assert!(p.in_water && !p.in_lava);
        // Holding the descend key dives to the bottom.
        run(&mut p, Input { descend: true, ..Default::default() }, 3.0, &pool);
        assert!((p.pos[1] - 10.0).abs() < 1e-2, "dived to {}", p.pos[1]);
    }

    #[test]
    fn walking_speed_does_not_depend_on_the_frame_rate() {
        let w = world(&[]);
        let mut went = alloc::vec::Vec::new();
        for fps in [20.0f32, 30.0, 60.0, 144.0, 500.0] {
            let mut p = Player::new([2.5, 121.0, 2.5]);
            p.yaw = 0.0;
            run(&mut p, Input::default(), 1.0, &w);
            let start = p.pos;
            let inp = Input { forward: 1.0, ..Default::default() };
            let steps = (3.0 * fps) as i32;
            for _ in 0..steps {
                p.update(1.0 / fps, &inp, &w);
            }
            let d = ((p.pos[0] - start[0]).powi(2) + (p.pos[2] - start[2]).powi(2)).sqrt();
            went.push(d / 3.0);
        }
        for v in &went {
            assert!(*v > 3.6 && *v < 4.4, "walked {v} blocks a second: {went:?}");
        }
    }

    #[test]
    fn a_sprinting_jump_goes_farther_than_a_walking_one_and_keeps_its_speed_in_the_air() {
        let w = world(&[]);
        let hop = |sprint: bool| {
            let mut p = Player::new([2.5, 121.0, 2.5]);
            run(&mut p, Input::default(), 1.0, &w);
            // Run up to speed first.
            let go = Input { forward: 1.0, sprint, ..Default::default() };
            for _ in 0..120 {
                p.update(1.0 / 60.0, &go, &w);
            }
            let start = p.pos;
            let jump = Input { forward: 1.0, sprint, jump: true, ..Default::default() };
            let mut peak = 0.0f32;
            let mut left = false;
            for _ in 0..90 {
                p.update(1.0 / 60.0, &jump, &w);
                let hs = (p.vel[0] * p.vel[0] + p.vel[2] * p.vel[2]).sqrt();
                if !p.on_ground {
                    left = true;
                    peak = peak.max(hs);
                } else if left {
                    break;
                }
            }
            (((p.pos[0] - start[0]).powi(2) + (p.pos[2] - start[2]).powi(2)).sqrt(), peak)
        };
        let (walk_d, walk_v) = hop(false);
        let (run_d, run_v) = hop(true);
        assert!(run_v > 6.8 && run_v < 8.6, "a sprinting jump leaves at {run_v}");
        assert!(run_d > walk_d * 1.4, "a sprint jump went {run_d}, a walking one {walk_d} (speeds {run_v}, {walk_v})");
        assert!(run_d > 3.4, "{run_d}");
    }
}
