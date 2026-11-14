//! What scripts may do with effects: give, take away and ask about them,
//! for the player or for the creature the player is looking at. The host
//! functions `apply_effect`, `remove_effect`, `has_effect` and
//! `effect_level` (script/host.rs) call these.
//!
//! A target is `"player"` or `"target"` (the creature in the crosshair, up
//! to 8 blocks away). Anything else is no one.

use super::apply::{self, lookup};
use super::{MAX_LEVEL, MAX_SECS};
use crate::math::forward;
use crate::session::Session;

/// Who an effect is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Who {
    Player,
    /// The creature in the crosshair.
    Target,
}

pub fn who(word: &str) -> Option<Who> {
    match word {
        "player" | "me" | "self" => Some(Who::Player),
        "target" | "looked_at" | "mob" => Some(Who::Target),
        _ => None,
    }
}

/// The creature the player is looking at.
fn target(g: &Session) -> Option<usize> {
    let dir = forward(g.player.yaw, g.player.pitch);
    g.mobs.hit_test(g.player.eye(), dir, 8.0).map(|(i, _)| i)
}

/// Seconds and level a script asked for, held to what is allowed. Zero (or
/// less) seconds mean the effect's own time.
pub fn clamp_args(secs: f64, level: i64) -> (f32, i32) {
    let secs = if secs > 0.0 { if secs > MAX_SECS as f64 { MAX_SECS } else { secs as f32 } } else { 0.0 };
    (secs, level.clamp(1, MAX_LEVEL as i64) as i32)
}

/// Give an effect. False when there is no such effect or no such target, or
/// a mod refused it.
pub fn give(g: &mut Session, target_word: &str, id: &str, secs: f64, level: i64) -> bool {
    let (secs, level) = clamp_args(secs, level);
    match who(target_word) {
        Some(Who::Player) => apply::give(g, id, secs, level).is_ok(),
        Some(Who::Target) => {
            let Some(d) = lookup(&id.to_ascii_lowercase()) else { return false };
            let Some(i) = target(g) else { return false };
            let secs = if secs > 0.0 { secs } else { d.secs };
            match g.mobs.list.get_mut(i) {
                Some(m) => {
                    m.fx.give(d, secs, level);
                    true
                }
                None => false,
            }
        }
        None => false,
    }
}

/// Take an effect away (all of them for an empty id). Whether any was there.
pub fn remove(g: &mut Session, target_word: &str, id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    match who(target_word) {
        Some(Who::Player) => {
            if id.is_empty() || id == "all" {
                let any = !g.fx.list.is_empty();
                apply::clear_all(g);
                any
            } else {
                match lookup(&id) {
                    Some(d) => apply::clear(g, &d.id),
                    None => false,
                }
            }
        }
        Some(Who::Target) => {
            let Some(i) = target(g) else { return false };
            let Some(m) = g.mobs.list.get_mut(i) else { return false };
            if id.is_empty() || id == "all" {
                let any = !m.fx.list.is_empty();
                m.fx.clear_all();
                any
            } else {
                match lookup(&id) {
                    Some(d) => m.fx.clear(&d.id),
                    None => false,
                }
            }
        }
        None => false,
    }
}

/// The level of an effect on someone, 0 if it is not active.
pub fn level(g: &Session, target_word: &str, id: &str) -> i64 {
    let id = id.to_ascii_lowercase();
    let id = lookup(&id).map_or(id, |d| d.id.clone());
    match who(target_word) {
        Some(Who::Player) => g.fx.level(&id) as i64,
        Some(Who::Target) => target(g).and_then(|i| g.mobs.list.get(i)).map_or(0, |m| m.fx.level(&id) as i64),
        None => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_named_by_a_word() {
        assert_eq!(who("player"), Some(Who::Player));
        assert_eq!(who("target"), Some(Who::Target));
        assert_eq!(who("everyone"), None);
        assert_eq!(who(""), None);
    }

    #[test]
    fn what_a_script_asks_for_is_held_to_range() {
        assert_eq!(clamp_args(10.0, 2), (10.0, 2));
        assert_eq!(clamp_args(0.0, 0), (0.0, 1));
        assert_eq!(clamp_args(-5.0, -5), (0.0, 1));
        assert_eq!(clamp_args(1e12, 1_000_000), (MAX_SECS, MAX_LEVEL));
        assert_eq!(clamp_args(f64::NAN, 1).0, 0.0);
    }
}
