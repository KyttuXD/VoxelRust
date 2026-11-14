//! What the server keeps for the sync of shared games: per connection (their
//! health, whether they are dead, their emote, their ping, when they came) and
//! as a whole (the chat history, the health of players who left, a clock,
//! who changed which block lately, who runs the creatures).

use super::history::Chatlog;
use super::vitals::Vitals;
use alloc::collections::BTreeMap;

/// Seconds between the questions that measure a player's ping.
pub const PING_EVERY: f32 = 5.0;
/// A question not answered in this many seconds is forgotten (and asked again).
pub const PING_LOST: f64 = 20.0;
/// Two different players changing the same block within this many seconds
/// are told how it ended (both get the last one).
pub const CONFLICT_SECONDS: f64 = 1.5;
/// Most blocks remembered as changed lately.
pub const MAX_RECENT: usize = 256;
/// Most players whose health is remembered (as many as their places).
pub const MAX_REMEMBERED: usize = 4096;

/// Per connection.
#[derive(Default)]
pub struct ConnSync {
    /// Their health as they last said (checked).
    pub vitals: Option<Vitals>,
    /// They said they died and have not said they are alive since.
    pub dead: bool,
    /// When (server clock) their last death was announced.
    pub died_at: Option<f64>,
    /// The emote they last started (0 none); forgotten when they move off.
    pub emote: u16,
    /// Their ping in milliseconds (smoothed); 0 until measured.
    pub ping_ms: u16,
    pub ping_seq: u16,
    /// The question asked and when (server clock).
    pub ping_sent: Option<(u16, f64)>,
    pub ping_timer: f32,
    /// They have sent their health once: they know this channel (older games do not, and are not asked).
    pub speaks: bool,
    /// Their place in the order of those who came in (1 is the first ever of this run).
    pub order: u64,
}

/// For the whole server.
#[derive(Default)]
pub struct ServerSync {
    pub history: Chatlog,
    /// Health, maximum and hunger of players as they left (by the key of their place).
    pub health: BTreeMap<alloc::string::String, [u8; 3]>,
    /// Seconds since the server started (for pings and conflicts).
    pub clock: f64,
    /// The last changes of blocks: who, and when.
    pub recent: BTreeMap<(i32, i32, i32), (u16, f64)>,
    /// How many have come in.
    pub joined: u64,
    /// Who the players were last told runs the creatures (0 nobody).
    pub runner: u16,
}

impl ServerSync {
    /// A change of a block by `who` at `at` (server clock): true if another
    /// player changed it a moment ago, so that both must be told how it ended.
    pub fn block_conflict(&mut self, cell: (i32, i32, i32), who: u16) -> bool {
        let clash = matches!(self.recent.get(&cell), Some(&(other, at)) if other != who && self.clock - at < CONFLICT_SECONDS);
        if self.recent.len() >= MAX_RECENT && !self.recent.contains_key(&cell) {
            let now = self.clock;
            self.recent.retain(|_, v| now - v.1 < CONFLICT_SECONDS);
        }
        if self.recent.len() < MAX_RECENT || self.recent.contains_key(&cell) {
            self.recent.insert(cell, (who, self.clock));
        }
        clash
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_players_changing_one_block_at_once_clash_but_one_player_or_a_later_change_does_not() {
        let mut s = ServerSync::default();
        assert!(!s.block_conflict((1, 2, 3), 5));
        assert!(!s.block_conflict((1, 2, 3), 5), "the same player again");
        assert!(s.block_conflict((1, 2, 3), 6), "another a moment later");
        s.clock = 10.0;
        assert!(!s.block_conflict((1, 2, 3), 5), "long after");
        assert!(!s.block_conflict((9, 9, 9), 5));
    }

    #[test]
    fn the_list_of_recent_changes_does_not_grow_for_ever() {
        let mut s = ServerSync::default();
        for i in 0..1000 {
            s.block_conflict((i, 0, 0), 1);
        }
        assert!(s.recent.len() <= MAX_RECENT);
    }
}
