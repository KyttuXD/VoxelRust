//! Explosions a mod sets off: the flags it may choose, the limits, and the
//! fires a blast can leave. The blast itself is `game/explosion.rs`
//! (`explode_with`), the same code TNT uses, so the chain reaction of TNT
//! (lit blocks with short fuses, at most `primed::MAX_PRIMED`), the damage
//! and the `before_explode` hooks of other mods all apply.

use crate::block::{self, reg, AIR};
use crate::explosion::{self, ExplosionSource, MAX_POWER};
use crate::session::Session;
use alloc::format;
use alloc::string::String;

/// The number a script's explosions carry as their source (`mod<N>`).
pub const SOURCE: u32 = 0x5C21;
/// Most explosions the scripts may set off in one frame (the rest do nothing).
pub const MAX_PER_FRAME: u32 = 16;
/// Most fires one blast leaves behind.
pub const MAX_FIRES: usize = 24;

/// Bits of the number form of the flags.
pub const FIRE: i64 = 1;
pub const NO_BLOCKS: i64 = 2;
pub const NO_DAMAGE: i64 = 4;
pub const SILENT: i64 = 8;

/// What an explosion is asked to leave out or add.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flags {
    /// Leave fire where the ground allows.
    pub fire: bool,
    /// Break no blocks (and light no TNT).
    pub no_blocks: bool,
    /// Hurt and throw nobody.
    pub no_damage: bool,
    /// No sound, no shaking of the view, no flash.
    pub silent: bool,
}

impl Flags {
    /// An ordinary explosion.
    pub const NONE: Flags = Flags { fire: false, no_blocks: false, no_damage: false, silent: false };

    /// From the number form: 1 fire, 2 no_blocks, 4 no_damage, 8 silent.
    /// Other bits are a mistake.
    pub fn from_bits(bits: i64) -> Result<Flags, String> {
        if bits < 0 || bits > FIRE | NO_BLOCKS | NO_DAMAGE | SILENT {
            return Err(format!("explosion flags {} are not made of 1 (fire), 2 (no_blocks), 4 (no_damage) and 8 (silent)", bits));
        }
        Ok(Flags { fire: bits & FIRE != 0, no_blocks: bits & NO_BLOCKS != 0, no_damage: bits & NO_DAMAGE != 0, silent: bits & SILENT != 0 })
    }

    /// From names separated by commas, spaces or `|`: `"fire, silent"`. An
    /// empty text is no flags.
    pub fn parse(text: &str) -> Result<Flags, String> {
        let mut f = Flags::NONE;
        for word in text.split(|c: char| c == ',' || c == '|' || c == '+' || c.is_whitespace()).filter(|w| !w.is_empty()) {
            match word.to_ascii_lowercase().as_str() {
                "fire" => f.fire = true,
                "no_blocks" => f.no_blocks = true,
                "no_damage" => f.no_damage = true,
                "silent" => f.silent = true,
                "none" => {}
                other => return Err(format!("unknown explosion flag `{}` (known: fire, no_blocks, no_damage, silent)", other)),
            }
        }
        Ok(f)
    }

    /// The number form.
    pub fn bits(&self) -> i64 {
        (if self.fire { FIRE } else { 0 }) | (if self.no_blocks { NO_BLOCKS } else { 0 }) | (if self.no_damage { NO_DAMAGE } else { 0 }) | (if self.silent { SILENT } else { 0 })
    }
}

/// The power an explosion is made with: `None` for nothing to do (not a
/// number, or not above 0), else held to [`MAX_POWER`].
pub fn clamp_power(power: f64) -> Option<f32> {
    if !power.is_finite() || power <= 0.0 {
        return None;
    }
    let p = if power > MAX_POWER as f64 { MAX_POWER } else { power as f32 };
    // Smaller than a whisper does nothing worth a frame.
    if p < 0.05 {
        None
    } else {
        Some(p)
    }
}

/// Whether one more explosion may go off this frame, given how many have.
pub fn may_go_off(so_far: u32) -> bool {
    so_far < MAX_PER_FRAME
}

/// Set an explosion off for a mod: refused (0 blocks) where the player may
/// not change the world, past the per-frame count, or for a power that is
/// nothing. Returns the number of blocks destroyed.
pub fn run(g: &mut Session, at: [f64; 3], power: f64, flags: Flags) -> usize {
    let Some(p) = clamp_power(power) else { return 0 };
    if !super::may_change(g) || !at.iter().all(|v| v.is_finite()) {
        return 0;
    }
    let s = super::state(g);
    if !may_go_off(s.blasts) {
        return 0;
    }
    s.blasts += 1;
    explosion::explode_with(g, at, p, ExplosionSource::Mod(SOURCE), flags)
}

/// Which cells of a ball catch fire: air with something solid under it,
/// picked by a hash of the cell so the same blast makes the same fires.
/// Pure: `air` and `solid` say what a cell holds.
pub fn fire_cells(cells: &[[i32; 3]], seed: u32, air: &dyn Fn([i32; 3]) -> bool, solid: &dyn Fn([i32; 3]) -> bool) -> alloc::vec::Vec<[i32; 3]> {
    let mut out = alloc::vec::Vec::new();
    for c in cells {
        if out.len() >= MAX_FIRES {
            break;
        }
        if !air(*c) || c[1] < 1 || !solid([c[0], c[1] - 1, c[2]]) {
            continue;
        }
        let h = (c[0] as u32).wrapping_mul(0x9E37_79B1) ^ (c[1] as u32).wrapping_mul(0x85EB_CA77) ^ (c[2] as u32).wrapping_mul(0xC2B2_AE3D) ^ seed;
        let h = (h ^ (h >> 15)).wrapping_mul(0x2C1B_3C6D);
        // About one cell in seven.
        if (h >> 8) % 7 == 0 {
            out.push(*c);
        }
    }
    out
}

/// Leave fires in a blast's ball (the `fire` flag): the block `fire`, where
/// the game has one.
pub fn set_fires(g: &mut Session, at: [f64; 3], radius: f32, seed: u32) {
    let Some(fire) = reg().block("fire") else { return };
    let cells = explosion::blast_cells(at, radius, seed);
    let world = &g.world;
    let picked = fire_cells(&cells, seed, &|c| world.get(c[0], c[1], c[2]) == AIR, &|c| block::is_solid(world.get(c[0], c[1], c[2])));
    for c in picked {
        g.world.set_block(c[0], c[1], c[2], fire);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_come_from_numbers_and_names() {
        assert_eq!(Flags::from_bits(0).unwrap(), Flags::NONE);
        let all = Flags::from_bits(15).unwrap();
        assert!(all.fire && all.no_blocks && all.no_damage && all.silent);
        assert_eq!(Flags::from_bits(1 | 8).unwrap(), Flags { fire: true, silent: true, ..Flags::NONE });
        assert!(Flags::from_bits(16).is_err());
        assert!(Flags::from_bits(-1).is_err());
        assert_eq!(Flags::parse("").unwrap(), Flags::NONE);
        assert_eq!(Flags::parse("fire").unwrap(), Flags { fire: true, ..Flags::NONE });
        assert_eq!(Flags::parse("fire, no_blocks | silent").unwrap(), Flags { fire: true, no_blocks: true, no_damage: false, silent: true });
        assert_eq!(Flags::parse("FIRE no_damage").unwrap().bits(), FIRE | NO_DAMAGE);
        assert!(Flags::parse("fire, boom").is_err());
        for bits in 0..16 {
            assert_eq!(Flags::from_bits(bits).unwrap().bits(), bits);
        }
    }

    #[test]
    fn power_is_held_to_the_games_limit() {
        assert_eq!(clamp_power(2.5), Some(2.5));
        assert_eq!(clamp_power(100.0), Some(MAX_POWER));
        assert_eq!(clamp_power(f64::INFINITY), None);
        assert_eq!(clamp_power(f64::NAN), None);
        assert_eq!(clamp_power(0.0), None);
        assert_eq!(clamp_power(-3.0), None);
        assert_eq!(clamp_power(0.001), None);
    }

    #[test]
    fn only_so_many_go_off_in_a_frame() {
        assert!(may_go_off(0));
        assert!(may_go_off(MAX_PER_FRAME - 1));
        assert!(!may_go_off(MAX_PER_FRAME));
        assert!(!may_go_off(u32::MAX));
    }

    #[test]
    fn fires_stand_on_ground_in_the_open_and_are_few() {
        // A flat floor at y = 10: air above it, solid at and below.
        let air = |c: [i32; 3]| c[1] > 10;
        let solid = |c: [i32; 3]| c[1] <= 10;
        let mut cells = alloc::vec::Vec::new();
        for x in -10..10 {
            for y in 5..16 {
                for z in -10..10 {
                    cells.push([x, y, z]);
                }
            }
        }
        let fires = fire_cells(&cells, 99, &air, &solid);
        assert!(!fires.is_empty() && fires.len() <= MAX_FIRES, "{}", fires.len());
        for f in &fires {
            assert_eq!(f[1], 11, "a fire must stand on the floor, not float: {:?}", f);
        }
        // The same blast makes the same fires.
        assert_eq!(fires, fire_cells(&cells, 99, &air, &solid));
        // With no ground there are none.
        assert!(fire_cells(&cells, 99, &|_| true, &|_| false).is_empty());
        // With no room there are none.
        assert!(fire_cells(&cells, 99, &|_| false, &|_| true).is_empty());
    }
}
