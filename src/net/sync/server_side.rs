//! The server's half of the sync (see `net/sync/mod.rs`): players' health and
//! death notices, pings, who runs the creatures, a guest who comes back
//! before the old connection has timed out, and the health players had when
//! they left.
//!
//! This file is declared inside `net/server.rs` (a child module), so that it can reach the
//! server's own fields.

use super::*;
use crate::net::sync::state::{PING_EVERY, PING_LOST};
use crate::net::sync::vitals::{self, key, Vitals};
use crate::proto::channel::VITALS;

/// A connection that has said nothing for this many seconds, whose player
/// comes again from the same address under the same name, is taken to be the
/// old connection of that player (a live game says something every two seconds).
pub(super) const STALE_AFTER: f32 = 2.5;

/// What a player's place and health were as they left: the key they are remembered by
/// (their account or their name), where they stood, and how they were.
pub(super) struct Notes {
    pub key: String,
    pub spot: Option<[f64; 5]>,
    pub health: Option<[u8; 3]>,
}

impl Server {
    /// Send a message of the sync channel from the server to connection `i`.
    fn vitals_to(&mut self, i: usize, k: &str, data: Vec<u8>) {
        let msg = Msg::State { id: 0, channel: VITALS, keep: false, key: String::from(k), data };
        if self.conns[i].stream.send_all(&msg.encode(), SEND_TIMEOUT).is_err() {
            self.conns[i].gone = true;
        }
    }

    /// Per tick: the clock, and a question to each player who knows the channel, now and then, to measure their ping.
    pub(super) fn sync_tick(&mut self, dt: f32) {
        self.sync.clock += dt as f64;
        let now = self.sync.clock;
        let mut ask: Vec<(usize, u16)> = Vec::new();
        for (k, c) in self.conns.iter_mut().enumerate() {
            if !c.joined || c.gone || !c.sync.speaks {
                continue;
            }
            c.sync.ping_timer += dt;
            if matches!(c.sync.ping_sent, Some((_, at)) if now - at > PING_LOST) {
                c.sync.ping_sent = None;
            }
            if c.sync.ping_sent.is_none() && c.sync.ping_timer >= PING_EVERY {
                c.sync.ping_timer = 0.0;
                c.sync.ping_seq = c.sync.ping_seq.wrapping_add(1);
                c.sync.ping_sent = Some((c.sync.ping_seq, now));
                ask.push((k, c.sync.ping_seq));
            }
        }
        for (k, seq) in ask {
            self.vitals_to(k, key::ASK, seq.to_le_bytes().to_vec());
        }
    }

    /// A message of the sync channel from connection `i`.
    pub(super) fn on_vitals(&mut self, i: usize, k: &str, data: &[u8], log: &mut Vec<String>) {
        let id = self.conns[i].id;
        match k {
            key::ME => {
                // (Out-of-range numbers are not taken at all.)
                let Some(mut v) = Vitals::decode(data) else { return };
                let c = &mut self.conns[i];
                c.sync.speaks = true;
                if v.hp > 0 {
                    c.sync.dead = false;
                }
                v.ping = c.sync.ping_ms;
                c.sync.vitals = Some(v);
                self.broadcast(&Msg::State { id, channel: VITALS, keep: false, key: String::from(key::ME), data: v.encode(true) }, id);
            }
            // "I died": announced once, until they say they are well again, and not more than once in two seconds.
            key::DIED => {
                let now = self.sync.clock;
                let c = &mut self.conns[i];
                if c.sync.dead || matches!(c.sync.died_at, Some(at) if now - at < 2.0) {
                    return;
                }
                c.sync.dead = true;
                c.sync.died_at = Some(now);
                c.sync.speaks = true;
                let mut v = c.sync.vitals.unwrap_or(Vitals { hp: 0, max: 20, food: 20, worn: [0; vitals::SLOTS], ping: 0 });
                v.hp = 0;
                c.sync.vitals = Some(v);
                let cause = vitals::clean_cause(data);
                let line = vitals::death_line(&c.name, &cause);
                self.broadcast(&Msg::State { id, channel: VITALS, keep: false, key: String::from(key::ME), data: v.encode(true) }, id);
                // A death by a player's blow is announced by `net/pvp` ("A was slain by B"), not twice.
                if cause != "pvp" {
                    log.push(line.clone());
                    self.sync.history.push(&line);
                    self.broadcast(&Msg::Chat { id: 0, text: line }, u16::MAX);
                }
            }
            key::PONG => {
                let now = self.sync.clock;
                let c = &mut self.conns[i];
                if let (Some(seq), Some((want, at))) = (vitals::two_bytes(data), c.sync.ping_sent) {
                    if seq == want {
                        let ms = (((now - at) * 1000.0) as u32).clamp(1, vitals::MOST_PING as u32) as u16;
                        c.sync.ping_ms = if c.sync.ping_ms == 0 { ms } else { ((c.sync.ping_ms as u32 * 3 + ms as u32) / 4).max(1) as u16 };
                        c.sync.ping_sent = None;
                        let told = c.sync.ping_ms.to_le_bytes().to_vec();
                        self.vitals_to(i, key::PING, told);
                    }
                }
            }
            _ => {}
        }
    }

    /// What to remember of the player on a connection as they leave. (A static function so that it
    /// works on a connection already taken out of the list.)
    pub(super) fn leave_notes(c: &Conn) -> Notes {
        let key = c.acct.account.clone().unwrap_or_else(|| c.name.clone());
        let spot = c.last_move.map(|(pos, yaw, pitch, ..)| [pos[0], pos[1], pos[2], yaw as f64, pitch as f64]);
        let health = c.sync.vitals.map(|v| [v.hp, v.max, v.food]);
        Notes { key, spot, health }
    }

    pub(super) fn apply_notes(&mut self, n: Notes) {
        if let Some(s) = n.spot {
            self.remember_spot(&n.key, s);
        }
        if let Some(h) = n.health {
            self.remember_health(&n.key, h);
        }
    }

    /// Note the health of a player (lower-case key; at most [`MAX_REMEMBERED`] players).
    pub(super) fn remember_health(&mut self, name: &str, h: [u8; 3]) {
        let key = name.to_ascii_lowercase();
        if h[1] >= 1 && h[1] <= vitals::MOST_HEALTH && h[0] <= h[1] && h[2] <= vitals::MOST_FOOD && (self.sync.health.len() < crate::net::sync::state::MAX_REMEMBERED || self.sync.health.contains_key(&key)) {
            if self.sync.health.insert(key, h) != Some(h) {
                self.spots_dirty = true;
            }
        }
    }

    /// The connection of a guest who has come again before their old connection timed out: the
    /// same name, from the same address, and the old one quiet for [`STALE_AFTER`] seconds.
    pub(super) fn stale_twin(&self, i: usize, name: &str) -> Option<usize> {
        let ip = self.conns[i].acct.ip?;
        self.conns.iter().enumerate().position(|(k, c)| k != i && c.joined && !c.gone && c.acct.account.is_none() && c.name.eq_ignore_ascii_case(name) && c.acct.ip == Some(ip) && c.idle >= STALE_AFTER)
    }

    /// Whether a player of this name is here.
    pub(super) fn name_is_here(&self, name: &str) -> bool {
        self.conns.iter().any(|c| c.joined && !c.gone && c.name.eq_ignore_ascii_case(name))
    }

    /// Whoever came first of those here runs the creatures for everybody.
    fn runner_index(&self) -> Option<usize> {
        self.conns.iter().enumerate().filter(|(_, c)| c.joined && !c.gone).min_by_key(|(_, c)| c.sync.order).map(|(k, _)| k)
    }

    /// Tell everybody who runs the creatures when that changes (the first to come, or the next
    /// when they leave).
    pub(super) fn announce_runner(&mut self) {
        let id = self.runner_index().map_or(0, |k| self.conns[k].id);
        if id != self.sync.runner {
            self.sync.runner = id;
            self.broadcast(&Msg::State { id: 0, channel: VITALS, keep: false, key: String::from(key::RUNNER), data: id.to_le_bytes().to_vec() }, u16::MAX);
        }
    }

    /// What a newcomer is told of the sync: who runs the creatures, and the last lines of chat.
    pub(super) fn sync_intro(&self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.sync.runner != 0 {
            out.extend(Msg::State { id: 0, channel: VITALS, keep: false, key: String::from(key::RUNNER), data: self.sync.runner.to_le_bytes().to_vec() }.encode());
        }
        if !self.sync.history.is_empty() {
            out.extend(Msg::Chat { id: 0, text: String::from("recent chat:") }.encode());
            for line in self.sync.history.lines() {
                out.extend(Msg::Chat { id: 0, text: line.clone() }.encode());
            }
        }
        out
    }

    /// What a newcomer is told of another player's sync state: health, then emote.
    pub(super) fn player_intro(other: &Conn) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(v) = other.sync.vitals {
            out.extend(Msg::State { id: other.id, channel: VITALS, keep: false, key: String::from(key::ME), data: v.encode(true) }.encode());
        }
        if other.sync.emote != 0 {
            out.extend(Msg::Emote { id: other.id, emote: other.sync.emote }.encode());
        }
        out
    }

    /// What a returning player is told of the health they had: nothing when they died (they come
    /// back at the world's spawn, at full health, like after a respawn) or when nothing is known.
    /// Returns the bytes to send and whether their place should be left out.
    pub(super) fn health_intro(&self, key_of_place: &str) -> (Vec<u8>, bool) {
        match self.sync.health.get(&key_of_place.to_ascii_lowercase()) {
            Some(h) if h[0] > 0 => (Msg::State { id: 0, channel: crate::proto::channel::PLAYER, keep: false, key: String::from("health"), data: alloc::format!("{} {} {}", h[0], h[1], h[2]).into_bytes() }.encode(), false),
            Some(_) => (Vec::new(), true),
            None => (Vec::new(), false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> Server {
        Server::bind(Config { port: 0, seed: 1, kind: String::new(), max_players: 4, content: 0, states: 1, pack: Vec::new(), mods: Vec::new() }).unwrap()
    }

    fn conn_of(s: &Server, id: u16, name: &str, order: u64) -> Conn {
        let stream = crate::sys::net::TcpStream::connect("127.0.0.1", s.port(), 2000).unwrap();
        let mut c = Conn { stream, inbox: Inbox::default(), id, name: String::from(name), joined: true, last_move: None, skin: Default::default(), skin_out: Default::default(), out: Default::default(), proto: 0, voice_flags: 0, voice_bucket: Default::default(), quiet: 0, idle: 0.0, throttle: Default::default(), gone: false, mod_out: Default::default(), mods_asked: false, effects: Vec::new(), acct: Default::default(), sync: Default::default(), pvp: Default::default(), chunks: Default::default() };
        c.sync.order = order;
        c
    }

    #[test]
    fn the_player_who_came_first_runs_the_creatures_whatever_the_numbers() {
        let mut s = server();
        // Number 9 came first, then 3 (the counter went round).
        let (a, b) = (conn_of(&s, 9, "Ann", 1), conn_of(&s, 3, "Bob", 2));
        s.conns.push(a);
        s.conns.push(b);
        assert!(s.runs_the_world(0) && !s.runs_the_world(1));
        s.announce_runner();
        assert_eq!(s.sync.runner, 9);
        // When Ann is gone, Bob does.
        s.conns[0].gone = true;
        assert!(s.runs_the_world(1));
        s.announce_runner();
        assert_eq!(s.sync.runner, 3);
        s.conns[1].gone = true;
        s.announce_runner();
        assert_eq!(s.sync.runner, 0);
    }

    #[test]
    fn a_guest_who_comes_again_replaces_a_quiet_old_connection_only() {
        let mut s = server();
        let mut old = conn_of(&s, 1, "Bob", 1);
        old.acct.ip = Some([127, 0, 0, 1]);
        let mut new = conn_of(&s, 0, "Bob", 0);
        new.joined = false;
        new.acct.ip = Some([127, 0, 0, 1]);
        s.conns.push(old);
        s.conns.push(new);
        assert_eq!(s.stale_twin(1, "bob"), None, "the old one spoke a moment ago");
        s.conns[0].idle = STALE_AFTER + 0.1;
        assert_eq!(s.stale_twin(1, "bob"), Some(0));
        assert_eq!(s.stale_twin(1, "Cy"), None, "another name");
        s.conns[1].acct.ip = Some([10, 0, 0, 2]);
        assert_eq!(s.stale_twin(1, "bob"), None, "another address");
    }

    #[test]
    fn health_is_remembered_by_name_checked_and_a_dead_player_is_not_sent_back_to_their_place() {
        let mut s = server();
        s.remember_health("Dana", [7, 20, 12]);
        s.remember_health("Eve", [30, 20, 5]);
        s.remember_health("Fay", [0, 20, 20]);
        assert_eq!(s.sync.health.get("dana"), Some(&[7, 20, 12]));
        assert!(!s.sync.health.contains_key("eve"), "more health than the most");
        let (bytes, skip) = s.health_intro("DANA");
        assert!(!bytes.is_empty() && !skip);
        let (bytes, skip) = s.health_intro("fay");
        assert!(bytes.is_empty() && skip, "dead: respawn at full health, at the spawn");
        let (bytes, skip) = s.health_intro("nobody");
        assert!(bytes.is_empty() && !skip);
    }
}
