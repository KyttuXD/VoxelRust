//! What the server keeps for player-versus-player: its switches and clock, and
//! per player the gear they reported, when they last hit, their spawn
//! protection and who hurt them last.

use super::rules::Rules;
use super::wire::Gear;
use alloc::collections::BTreeMap;
use alloc::string::String;

/// Seconds after a hit in which, if the victim dies, the one who hit them gets the kill.
pub const KILL_WINDOW: f64 = 15.0;

/// Most killers whose kills are counted.
pub const MAX_KILLERS: usize = 4096;

/// What the server keeps for all.
#[derive(Default)]
pub struct Pvp {
    pub rules: Rules,
    /// The switch or the protection was set by a command (and is kept with the
    /// permissions, which then win over `settings.txt`).
    pub by_command: bool,
    /// Seconds since the server started.
    pub now: f64,
    /// Kills of players who have made one (lower-case name).
    pub kills: BTreeMap<String, u32>,
}

/// What the server keeps for one player.
#[derive(Clone, Debug)]
pub struct ConnPvp {
    /// Their game said it can be fought (it sent its gear).
    pub capable: bool,
    /// Their armour as reported and clamped, and whether they are alive.
    pub gear: Gear,
    /// When their last hit that counted was (server seconds); None for never.
    pub last_hit: Option<f64>,
    /// Nobody can hurt them before this time (server seconds).
    pub protected_until: f64,
    /// Who hurt them last, and when.
    pub hurt_by: Option<(String, f64)>,
}

impl Default for ConnPvp {
    fn default() -> ConnPvp {
        ConnPvp { capable: false, gear: Gear::default(), last_hit: None, protected_until: 0.0, hurt_by: None }
    }
}

impl ConnPvp {
    /// A player who has just joined, at server time `now`.
    pub fn joined(now: f64, protect: f32) -> ConnPvp {
        ConnPvp { protected_until: now + protect as f64, ..ConnPvp::default() }
    }

    /// Seconds since their last counted hit (a big number when none).
    pub fn since_last(&self, now: f64) -> f64 {
        self.last_hit.map_or(1.0e9, |t| now - t)
    }

    /// Seconds of protection left (0 or less when none).
    pub fn protected_left(&self, now: f64) -> f64 {
        self.protected_until - now
    }

    /// Take a gear message: the armour is clamped. Returns what happened to
    /// their life: Some(true) when they have just come back, Some(false) when
    /// they have just died, None when it did not change.
    pub fn take_gear(&mut self, g: Gear, now: f64, protect: f32) -> Option<bool> {
        let was = self.gear.alive;
        self.capable = true;
        self.gear = super::check::clamp_gear(g);
        match (was, self.gear.alive) {
            (false, true) => {
                self.protected_until = now + protect as f64;
                self.hurt_by = None;
                Some(true)
            }
            (true, false) => Some(false),
            _ => None,
        }
    }

    /// Who gets the kill if they die now: the last to hurt them, if lately.
    pub fn killer(&self, now: f64) -> Option<&str> {
        self.hurt_by.as_ref().filter(|h| now - h.1 <= KILL_WINDOW && now >= h.1).map(|h| h.0.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_player_is_protected_for_the_set_time_and_cannot_be_fought_until_their_game_says_so() {
        let p = ConnPvp::joined(100.0, 5.0);
        assert!(!p.capable);
        assert!(p.protected_left(102.0) > 0.0);
        assert!(p.protected_left(105.0) <= 0.0);
        assert!(p.since_last(100.0) > 1.0e6);
        // No protection at all when set to 0.
        assert!(ConnPvp::joined(100.0, 0.0).protected_left(100.0) <= 0.0);
    }

    #[test]
    fn dying_and_coming_back_are_noticed_and_coming_back_protects() {
        let mut p = ConnPvp::joined(0.0, 5.0);
        assert_eq!(p.take_gear(Gear { defense: 4, toughness: 0, alive: true }, 10.0, 5.0), None);
        assert!(p.capable);
        p.hurt_by = Some((String::from("Bob"), 12.0));
        assert_eq!(p.killer(13.0), Some("Bob"));
        assert_eq!(p.take_gear(Gear { defense: 4, toughness: 0, alive: false }, 14.0, 5.0), Some(false));
        assert_eq!(p.take_gear(Gear { defense: 4, toughness: 0, alive: false }, 15.0, 5.0), None);
        assert_eq!(p.take_gear(Gear { defense: 4, toughness: 0, alive: true }, 20.0, 5.0), Some(true));
        assert!(p.protected_left(22.0) > 0.0 && p.protected_left(25.5) <= 0.0);
        assert_eq!(p.killer(20.0), None);
    }

    #[test]
    fn a_hit_long_ago_gets_no_kill_and_gear_is_clamped() {
        let mut p = ConnPvp::default();
        p.hurt_by = Some((String::from("Bob"), 1.0));
        assert_eq!(p.killer(1.0 + KILL_WINDOW + 1.0), None);
        p.take_gear(Gear { defense: 200, toughness: 100, alive: true }, 0.0, 0.0);
        assert_eq!((p.gear.defense, p.gear.toughness), (20, 8));
    }
}
