//! Primed TNT: a block that has been lit and burns its fuse before it
//! explodes (see `game::explosion`).
//!
//! Any block with `tag tnt <power>` is TNT. It is lit by lava or a block
//! tagged `tag fire` beside it ([`react`]), by an explosion (with a short
//! random fuse, so a stack goes off as a chain), or by right clicking it with
//! the flint and steel ([`ignite_cell`]). Lighting it takes the block out of
//! the world and puts a [`PrimedBlock`] in `Session::primed`: it falls if
//! nothing holds it, flashes white as its fuse runs down, and explodes when
//! the fuse is out. The client draws the list (`client::hand::primed`).
//!
//! The list is not saved: a world saved while TNT burns loses it.

use crate::block::{self, reg, Block, AIR};
use crate::explosion::{self, ExplosionSource};
use crate::math::floord;
use crate::session::Session;
use alloc::string::String;
use alloc::vec::Vec;

/// Seconds a lit block burns.
pub const FUSE: f32 = 4.0;
/// The power of a block whose `tag tnt` gives none.
pub const DEFAULT_POWER: f32 = 2.5;
/// Most lit blocks at once; past that, more are not lit.
pub const MAX_PRIMED: usize = 400;
/// Most sparks and debris in the air at once.
pub const MAX_SPARKS: usize = 320;
/// The item that lights TNT.
pub const IGNITER: &str = "flint_and_steel";

/// A lit block.
#[derive(Clone, Copy, Debug)]
pub struct PrimedBlock {
    /// The block it was (for its picture).
    pub block: Block,
    /// The middle of the block.
    pub pos: [f64; 3],
    /// Falling speed.
    pub vy: f32,
    /// Seconds to go.
    pub fuse: f32,
    pub power: f32,
}

/// A spark or a bit of debris flung by an explosion, or smoke from a fuse.
#[derive(Clone, Copy, Debug)]
pub struct Spark {
    pub pos: [f64; 3],
    pub vel: [f32; 3],
    pub life: f32,
    pub colour: [u8; 3],
}

/// What the session keeps about explosions: the lit blocks, the sparks, and
/// a random-number state of its own.
#[derive(Default)]
pub struct Primed {
    pub list: Vec<PrimedBlock>,
    pub sparks: Vec<Spark>,
    rng: u32,
}

impl Primed {
    /// A random number (24 bits).
    pub fn roll(&mut self) -> u32 {
        if self.rng == 0 {
            self.rng = 0x2545_F491;
        }
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.rng >> 8
    }

    /// A random number from 0 to 1.
    pub fn unit(&mut self) -> f32 {
        (self.roll() & 0xFFFF) as f32 / 65535.0
    }

    /// Add a spark (the oldest goes when there are too many).
    pub fn spark(&mut self, pos: [f64; 3], vel: [f32; 3], life: f32, colour: [u8; 3]) {
        if self.sparks.len() >= MAX_SPARKS {
            self.sparks.swap_remove(0);
        }
        self.sparks.push(Spark { pos, vel, life, colour });
    }
}

// ---- Pure rules.

/// The power of a block that is TNT, `None` for anything else.
pub fn power_of(b: Block) -> Option<f32> {
    if b == AIR {
        return None;
    }
    block::def(b).x.tag("tnt").map(|p| if p > 0.0 { p.min(explosion::MAX_POWER) } else { DEFAULT_POWER })
}

/// Whether a block sets TNT beside it alight: lava, or a block with `tag fire`.
pub fn is_hot(b: Block) -> bool {
    if b == AIR {
        return false;
    }
    matches!(reg().fluid[b as usize], Some((true, _))) || block::def(b).x.tag("fire").is_some()
}

/// Whether an item lights TNT.
pub fn is_igniter(item_id: &str) -> bool {
    item_id == IGNITER
}

/// The fuse of a block lit by a neighbouring explosion, from a random
/// number: half a second to a second and a half, so a stack goes off in a
/// ripple rather than all at once.
pub fn chain_fuse(roll: u32) -> f32 {
    0.5 + (roll % 1000) as f32 / 1000.0
}

/// Whether a block with this long still to burn is drawn white. It flashes
/// every half second, and twice as often in the last second.
pub fn flash_on(fuse: f32) -> bool {
    let fuse = if fuse < 0.0 { 0.0 } else { fuse };
    let step = if fuse < 1.0 { 0.125 } else { 0.25 };
    ((fuse / step) as i32) & 1 == 0
}

/// How big a block with this long to burn is drawn (1 = a whole block): it
/// swells a little in the last second.
pub fn fuse_scale(fuse: f32) -> f32 {
    if fuse >= 1.0 {
        1.0
    } else {
        1.0 + (1.0 - if fuse < 0.0 { 0.0 } else { fuse }) * 0.12
    }
}

/// A falling step for something whose middle is at height `y`: its new
/// height and speed. `solid_at` says whether the block in a cell is solid.
pub fn fall_step(y: f64, vy: f32, dt: f32, solid_at: &dyn Fn(i32) -> bool) -> (f64, f32) {
    let vy = vy - 18.0 * dt;
    let ny = y + (vy * dt) as f64;
    let below = floord(ny - 0.5);
    if solid_at(below) {
        (below as f64 + 1.5, 0.0)
    } else {
        (ny, vy)
    }
}

// ---- Lighting.

const SIDES: [[i32; 3]; 6] = [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]];

/// Light the TNT block at `cell` with `fuse` seconds to burn. The mods' own
/// code may refuse (`Native::on_prime`). Returns whether it was lit.
pub fn prime_at(g: &mut Session, cell: [i32; 3], fuse: f32) -> bool {
    let b = g.world.get(cell[0], cell[1], cell[2]);
    let power = match power_of(b) {
        Some(p) => p,
        None => return false,
    };
    if g.primed.list.len() >= MAX_PRIMED {
        return false;
    }
    for (_, n) in crate::native::active() {
        if let Some(prime) = n.on_prime {
            if !prime(g, cell) {
                return false;
            }
        }
    }
    // A hook may have changed what is there.
    if g.world.get(cell[0], cell[1], cell[2]) != b {
        return false;
    }
    g.world.set_block(cell[0], cell[1], cell[2], AIR);
    let pos = [cell[0] as f64 + 0.5, cell[1] as f64 + 0.5, cell[2] as f64 + 0.5];
    g.primed.list.push(PrimedBlock { block: b, pos, vy: 0.0, fuse: fuse.max(0.05), power });
    g.sound_queue.push(String::from("hiss"));
    g.fire("tntprimed", &block::def(b).id);
    true
}

/// The flint and steel was used on `cell`: light it if it is TNT.
pub fn ignite_cell(g: &mut Session, cell: [i32; 3]) -> bool {
    prime_at(g, cell, FUSE)
}

/// The block at `c` just changed: TNT beside lava or fire catches, and lava
/// or fire that has just appeared lights the TNT beside it.
pub fn react(g: &mut Session, c: [i32; 3]) {
    let b = g.world.get(c[0], c[1], c[2]);
    if b == AIR {
        return;
    }
    if power_of(b).is_some() {
        let lit = SIDES.iter().any(|d| is_hot(g.world.get(c[0] + d[0], c[1] + d[1], c[2] + d[2])));
        if lit {
            prime_at(g, c, FUSE);
        }
    } else if is_hot(b) {
        for d in SIDES {
            let n = [c[0] + d[0], c[1] + d[1], c[2] + d[2]];
            if power_of(g.world.get(n[0], n[1], n[2])).is_some() {
                prime_at(g, n, FUSE);
            }
        }
    }
}

// ---- Every frame.

/// Move the lit blocks and the sparks on by `dt`, and explode whatever has
/// burnt through.
pub fn tick(g: &mut Session, dt: f32) {
    if g.primed.list.is_empty() && g.primed.sparks.is_empty() {
        return;
    }
    let dt = dt.clamp(0.0, 0.25);
    let mut due: Vec<([f64; 3], f32)> = Vec::new();
    let mut i = 0;
    while i < g.primed.list.len() {
        let p = &mut g.primed.list[i];
        p.fuse -= dt;
        let (cx, cz) = (floord(p.pos[0]), floord(p.pos[2]));
        if g.world.is_loaded(cx, cz) {
            let world = &g.world;
            let (y, vy) = fall_step(p.pos[1], p.vy, dt, &|below| block::is_solid(world.get(cx, below, cz)));
            p.pos[1] = y;
            p.vy = vy;
        }
        if p.fuse <= 0.0 {
            due.push((p.pos, p.power));
            g.primed.list.swap_remove(i);
        } else if p.pos[1] < -16.0 {
            g.primed.list.swap_remove(i);
        } else {
            i += 1;
        }
    }
    // Smoke and sparks off the fuses.
    for k in 0..g.primed.list.len().min(48) {
        if g.primed.unit() < dt * 10.0 {
            let pos = g.primed.list[k].pos;
            let (a, b) = (g.primed.unit() - 0.5, g.primed.unit() - 0.5);
            let colour = if g.primed.unit() < 0.5 { [250, 190, 70] } else { [120, 120, 124] };
            g.primed.spark([pos[0], pos[1] + 0.6, pos[2]], [a * 1.2, 2.0, b * 1.2], 0.5, colour);
        }
    }
    // The sparks themselves.
    let mut s = 0;
    while s < g.primed.sparks.len() {
        let sp = &mut g.primed.sparks[s];
        sp.life -= dt;
        sp.vel[1] -= 14.0 * dt;
        for a in 0..3 {
            sp.pos[a] += (sp.vel[a] * dt) as f64;
        }
        if sp.life <= 0.0 {
            g.primed.sparks.swap_remove(s);
        } else {
            s += 1;
        }
    }
    for (pos, power) in due {
        explosion::explode(g, pos, power, ExplosionSource::Tnt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_fuse_is_between_half_a_second_and_a_second_and_a_half() {
        for roll in [0u32, 1, 499, 500, 999, 1000, 123_456, 16_777_215] {
            let f = chain_fuse(roll);
            assert!((0.5..1.5).contains(&f), "{roll} gave {f}");
        }
        assert_ne!(chain_fuse(10), chain_fuse(700));
    }

    #[test]
    fn a_block_flashes_on_and_off_and_faster_at_the_end() {
        // Slow at first: a quarter second on, a quarter second off.
        assert!(flash_on(4.0));
        assert!(!flash_on(3.9));
        assert!(flash_on(3.6));
        assert!(!flash_on(3.4));
        // Quicker in the last second: an eighth each.
        assert!(!flash_on(0.99));
        assert!(flash_on(0.85));
        assert!(!flash_on(0.7));
        // Never panics on odd values.
        let _ = flash_on(-1.0);
        let _ = flash_on(1.0e9);
    }

    #[test]
    fn a_block_over_a_short_fuse_flashes_both_ways() {
        let mut on = 0;
        let mut off = 0;
        let mut f = FUSE;
        while f > 0.0 {
            if flash_on(f) {
                on += 1;
            } else {
                off += 1;
            }
            f -= 0.01;
        }
        assert!(on > 100 && off > 100, "{on} on, {off} off");
    }

    #[test]
    fn the_block_swells_only_at_the_end() {
        assert_eq!(fuse_scale(4.0), 1.0);
        assert_eq!(fuse_scale(1.0), 1.0);
        assert!(fuse_scale(0.5) > 1.0);
        assert!(fuse_scale(0.0) > fuse_scale(0.5));
        assert_eq!(fuse_scale(-3.0), fuse_scale(0.0));
    }

    #[test]
    fn a_lit_block_falls_until_something_holds_it() {
        // Ground is the cell at height 10 and below: a block resting on it has its middle at 11.5.
        let ground = |cell: i32| cell <= 10;
        let (mut y, mut vy) = (14.5, 0.0);
        for _ in 0..200 {
            let (ny, nvy) = fall_step(y, vy, 0.05, &ground);
            y = ny;
            vy = nvy;
        }
        assert_eq!(y, 11.5);
        assert_eq!(vy, 0.0);
        // Already resting, it stays put.
        let (y2, vy2) = fall_step(11.5, 0.0, 0.05, &ground);
        assert_eq!((y2, vy2), (11.5, 0.0));
        // In open air it speeds up.
        let (_, v1) = fall_step(50.0, 0.0, 0.1, &|_| false);
        let (_, v2) = fall_step(50.0, v1, 0.1, &|_| false);
        assert!(v2 < v1 && v1 < 0.0);
    }

    #[test]
    fn the_fuse_burns_out_in_four_seconds_at_sixty_frames_a_second() {
        let mut fuse = FUSE;
        let mut frames = 0;
        while fuse > 0.0 {
            fuse -= 1.0 / 60.0;
            frames += 1;
        }
        assert!((239..=241).contains(&frames), "{frames}");
    }

    #[test]
    fn only_the_flint_and_steel_lights_tnt() {
        assert!(is_igniter("flint_and_steel"));
        assert!(!is_igniter("stick"));
        assert!(!is_igniter(""));
    }

    #[test]
    fn the_base_game_has_tnt_and_what_lights_it() {
        if crate::native::skip_without_mods() {
            return;
        }
        let r = reg();
        let tnt = r.blocks.iter().position(|b| b.id == "tnt").expect("a tnt block") as Block;
        assert_eq!(power_of(tnt), Some(2.5));
        assert!(power_of(r.wk.stone).is_none());
        assert!(power_of(AIR).is_none());
        assert!(is_hot(r.wk.lava));
        assert!(!is_hot(r.wk.water));
        assert!(!is_hot(tnt));
        let camp = r.blocks.iter().position(|b| b.id == "campfire").expect("a campfire") as Block;
        assert!(is_hot(camp));
        // The items.
        for id in ["gunpowder", "flint_and_steel", "tnt"] {
            assert!(r.items.iter().any(|i| i.id == id), "no item {id}");
        }
        assert!(r.grid_recipes.iter().any(|rc| r.items[rc.out.0 as usize].id == "tnt"), "no recipe for tnt");
    }

    #[test]
    fn sparks_are_capped() {
        let mut p = Primed::default();
        for _ in 0..(MAX_SPARKS + 50) {
            p.spark([0.0; 3], [0.0; 3], 1.0, [255, 255, 255]);
        }
        assert_eq!(p.sparks.len(), MAX_SPARKS);
        let (a, b) = (p.unit(), p.unit());
        assert!((0.0..=1.0).contains(&a) && (0.0..=1.0).contains(&b) && a != b);
    }
}
