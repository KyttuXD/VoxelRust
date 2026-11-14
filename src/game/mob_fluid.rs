//! What water and lava do to creatures, beyond what the body physics of
//! `game/player.rs` already does for everything that swims (floating,
//! being carried by a current, climbing out over a shore).
//!
//! - Lava sets a creature alight (the `burning` effect, kept up while it is
//!   in the lava) and it burns until put out or dead; water puts it out
//!   (`game/entity.rs` passes `in_water` only for water, not lava).
//! - Creatures that live in water (`swim`) are carried by the current too
//!   (their bodies fly through the water, so the body physics skips it), and
//!   suffocate out of water by the same air meter that drowns land creatures
//!   under it: ten seconds, then blows.
//! - Land creatures drown in water already (`game/effects/air.rs`, called
//!   from `game/entity.rs`).

use crate::block::{self, Behavior, Block};
use crate::entity::Mob;
use crate::math::floord;

/// Seconds of burning each moment in lava adds (it is topped up every frame).
const BURN_SECS: f32 = 6.0;
/// How hard a current pushes a swimming creature, blocks a second squared.
const SWIM_DRIFT: f32 = 4.0;

/// What a body at `pos` stands in: (in water, in lava).
pub fn wet(get: &dyn Fn(i32, i32, i32) -> Block, pos: [f64; 3]) -> (bool, bool) {
    let b = get(floord(pos[0]), floord(pos[1] + 0.2), floord(pos[2]));
    let f = block::fluid_of(b);
    (matches!(f, Some((false, _))), matches!(f, Some((true, _))))
}

/// Whether a body of this `height` at `pos` is in lava anywhere from its
/// feet to its middle.
pub fn in_lava(get: &dyn Fn(i32, i32, i32) -> Block, pos: [f64; 3], height: f64) -> bool {
    [0.1, height * 0.5].iter().any(|dy| matches!(block::fluid_of(get(floord(pos[0]), floord(pos[1] + dy), floord(pos[2]))), Some((true, _))))
}

/// One step for one creature (after its effects and drowning have been done).
pub fn step(m: &mut Mob, behavior: Behavior, get: &dyn Fn(i32, i32, i32) -> Block, dt: f32) {
    if in_lava(get, m.body.pos, m.body.height) {
        if let Some(burning) = crate::game::effects::apply::lookup("burning") {
            if !m.fx.refresh("burning", BURN_SECS, 1) {
                m.fx.give(burning, BURN_SECS, 1);
            }
        }
    }
    if behavior == Behavior::Swim {
        let (in_water, _) = wet(get, m.body.pos);
        if in_water {
            // Carried by the current, though its body flies through the water.
            if m.body.flying {
                let (px, pz) = crate::game::current::push_at(get, m.body.pos);
                m.body.vel[0] += px * SWIM_DRIFT * dt;
                m.body.vel[2] += pz * SWIM_DRIFT * dt;
            }
        }
        // Out of the water it cannot breathe.
        let hurt = crate::game::effects::apply::tick_mob_air(&m.fx, &mut m.air, dt, !in_water);
        if hurt > 0 {
            m.health -= hurt;
            m.hurt = 0.35;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{reg, AIR};

    fn world(liquid: Block) -> impl Fn(i32, i32, i32) -> Block {
        move |x, y, _z| {
            if y < 10 {
                reg().wk.stone
            } else if y < 12 && x < 5 {
                liquid
            } else {
                AIR
            }
        }
    }

    #[test]
    fn a_creature_in_lava_is_known_by_its_feet_or_middle() {
        let w = world(reg().wk.lava);
        assert!(in_lava(&w, [2.5, 10.0, 0.5], 1.0));
        assert!(!in_lava(&w, [8.5, 10.0, 0.5], 1.0));
        // Standing on lava's surface with the feet above it is not in it.
        assert!(!in_lava(&w, [2.5, 12.5, 0.5], 0.5));
        let water = world(reg().wk.water);
        assert!(!in_lava(&water, [2.5, 10.0, 0.5], 1.0));
    }

    #[test]
    fn wet_tells_water_from_lava() {
        let water = world(reg().wk.water);
        assert_eq!(wet(&water, [2.5, 10.5, 0.5]), (true, false));
        let lava = world(reg().wk.lava);
        assert_eq!(wet(&lava, [2.5, 10.5, 0.5]), (false, true));
        assert_eq!(wet(&lava, [8.5, 10.5, 0.5]), (false, false));
    }

    #[test]
    fn burning_exists_for_lava_to_give() {
        assert!(crate::game::effects::apply::lookup("burning").is_some());
    }
}
