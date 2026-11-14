//! The rules behind the creature hooks (`on_creature_death`,
//! `on_creature_removed`, `on_creature_sunburn`) and the melee reach of
//! creatures, as small pure functions the creature step and the session call.
//!
//! The dispatching itself is in `game/hooks.rs`; what is kept between the
//! creature step and the session (who dealt the last blow, creatures that
//! left, creatures waiting for a sunburn answer) lives in [`Mobs`](crate::entity::Mobs).

/// Why a creature left the world (`on_creature_removed`'s last argument).
pub mod reason {
    /// Its health ran out.
    pub const KILLED: &str = "killed";
    /// Gone for another reason: too far from the player, fallen out of the
    /// world, caught by the morning, a hostile one in peaceful play, blown up.
    pub const DESPAWNED: &str = "despawned";
    /// Left behind where the world is no longer loaded.
    pub const UNLOADED: &str = "unloaded";
    /// A script took it away (`mob_remove`).
    pub const SCRIPTED: &str = "scripted";
    pub const ALL: [&str; 4] = [KILLED, DESPAWNED, UNLOADED, SCRIPTED];
}

/// Most entries each of the creature lists the session reads keeps.
pub const MOST_PENDING: usize = 64;

/// What the creature step does about a night creature in daylight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    /// It burns (is taken away) now.
    Burn,
    /// A script said no a short while ago: it stays.
    Spare,
    /// Ask the scripts first; it stays until they have answered.
    Ask,
}

/// How long, in seconds, a script's "no" to a creature's burning holds before
/// it is asked again.
pub const SPARE_SECS: f32 = 20.0;

/// The decision for a creature whose `sun_veto` is given (above 0: spared for
/// that many more seconds; below 0: the scripts said it burns; 0: not asked),
/// when scripts are listening (`asking`) or not.
pub fn sun_gate(sun_veto: f32, asking: bool) -> Gate {
    if sun_veto > 0.0 {
        Gate::Spare
    } else if sun_veto < 0.0 || !asking {
        Gate::Burn
    } else {
        Gate::Ask
    }
}

/// The value `sun_veto` takes after the scripts answered: spared for a while, or burning.
pub fn sun_answer(allowed: bool) -> f32 {
    if allowed {
        -1.0
    } else {
        SPARE_SECS
    }
}

/// A creature's melee reach in blocks, measured between it and the player
/// along the ground: what the pack said (`reach`), else its half width plus a
/// block (a plain 0.6 wide creature reaches 1.3, as it always did; a creature
/// 3 wide reaches 2.5). Never less than 1.3 or more than 16.
pub fn reach_of(half_width: f32, said: f32) -> f32 {
    let r = if said > 0.0 && said.is_finite() { said } else { half_width + 1.0 };
    let r = if r.is_finite() { r } else { 1.3 };
    r.clamp(1.3, 16.0)
}

/// The distance a creature chasing the player stops closing in at: most of
/// its reach, so that it does bite.
pub fn close_in_to(reach: f32) -> f32 {
    reach * 0.7
}

/// The tallest the player's feet may be above a creature's feet and still be
/// bitten: its height, but at least what it always was.
pub fn bite_height(height: f32) -> f32 {
    if height > 1.6 && height.is_finite() {
        height
    } else {
        1.6
    }
}

/// The cause named for a death nobody dealt the last blow of.
pub fn natural_cause(in_lava: bool, drowning: bool) -> &'static str {
    if in_lava {
        "lava"
    } else if drowning {
        "drown"
    } else {
        "other"
    }
}

/// The player's name for `killer`, as a script sees it.
pub const PLAYER_KILLER: &str = "player";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_creature_reaches_as_far_as_it_always_did() {
        assert!((reach_of(0.3, 0.0) - 1.3).abs() < 1e-6);
    }

    #[test]
    fn a_big_creature_reaches_further_from_its_size() {
        // 3 blocks wide.
        assert!((reach_of(1.5, 0.0) - 2.5).abs() < 1e-6);
        // It can reach what its own body keeps it from standing next to.
        assert!(reach_of(1.5, 0.0) > 1.5 + 0.3);
        assert!(reach_of(4.0, 0.0) > reach_of(1.5, 0.0));
    }

    #[test]
    fn the_pack_may_say_how_far() {
        assert!((reach_of(0.3, 4.0) - 4.0).abs() < 1e-6);
        // Too short is lifted, too long is cut.
        assert!((reach_of(0.3, 0.5) - 1.3).abs() < 1e-6);
        assert!((reach_of(0.3, 99.0) - 16.0).abs() < 1e-6);
    }

    #[test]
    fn nonsense_sizes_give_a_sane_reach() {
        for w in [f32::NAN, f32::INFINITY, -3.0, 0.0] {
            let r = reach_of(w, 0.0);
            assert!(r.is_finite() && (1.3..=16.0).contains(&r), "{} -> {}", w, r);
        }
        assert!((reach_of(0.3, f32::NAN) - 1.3).abs() < 1e-6);
    }

    #[test]
    fn it_stops_closing_in_inside_its_reach() {
        for hw in [0.3, 0.8, 1.5, 3.0] {
            let reach = reach_of(hw, 0.0);
            assert!(close_in_to(reach) < reach);
        }
        // For a plain creature this is the 0.9 it used.
        assert!((close_in_to(1.3) - 0.91).abs() < 1e-5);
    }

    #[test]
    fn tall_creatures_may_bite_higher_up() {
        assert_eq!(bite_height(1.0), 1.6);
        assert_eq!(bite_height(f32::NAN), 1.6);
        assert_eq!(bite_height(4.0), 4.0);
    }

    #[test]
    fn the_sun_gate_waits_for_the_scripts_only_when_they_listen() {
        assert_eq!(sun_gate(0.0, false), Gate::Burn);
        assert_eq!(sun_gate(0.0, true), Gate::Ask);
        assert_eq!(sun_gate(5.0, true), Gate::Spare);
        assert_eq!(sun_gate(5.0, false), Gate::Spare);
        assert_eq!(sun_gate(-1.0, true), Gate::Burn);
    }

    #[test]
    fn the_answers_become_gate_states() {
        assert_eq!(sun_gate(sun_answer(true), true), Gate::Burn);
        assert_eq!(sun_gate(sun_answer(false), true), Gate::Spare);
    }

    #[test]
    fn deaths_without_a_killer_are_named() {
        assert_eq!(natural_cause(true, true), "lava");
        assert_eq!(natural_cause(false, true), "drown");
        assert_eq!(natural_cause(false, false), "other");
    }

    #[test]
    fn the_reasons_are_the_four_the_docs_name() {
        assert_eq!(reason::ALL, ["killed", "despawned", "unloaded", "scripted"]);
    }
}
