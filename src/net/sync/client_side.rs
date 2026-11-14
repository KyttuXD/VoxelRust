//! The client's half of the sync: telling the server our health, our death
//! and our armour, and keeping what the server says of the others (their
//! health, our own ping, who runs the creatures).

use super::vitals::{key, two_bytes, Vitals};
use crate::netplay::Client;
use crate::proto::channel::VITALS;

impl Client {
    /// Tell the server (and through it the others) how we are.
    pub fn send_vitals(&mut self, v: &Vitals) {
        let data = v.held_to_range().encode(false);
        self.send_state(VITALS, false, key::ME, &data);
    }

    /// Tell the server we died of `cause` (a creature's id or `fall`, `drown`...).
    pub fn send_died(&mut self, cause: &str) {
        let cause = super::vitals::clean_cause(cause.as_bytes());
        self.send_state(VITALS, false, key::DIED, cause.as_bytes());
    }

    /// A message of the sync channel arrived from `from` (0 is the server).
    /// Everything is checked; what is wrong is dropped.
    pub(crate) fn on_vitals_state(&mut self, from: u16, k: &str, data: &[u8]) {
        match k {
            key::ME if from != 0 && from != self.id => {
                if let (Some(p), Some(v)) = (self.players.get_mut(&from), Vitals::decode(data)) {
                    p.vitals = Some(v);
                }
            }
            // The server measures our ping: answer at once with the same number.
            key::ASK if from == 0 && data.len() == 2 => self.send_state(VITALS, false, key::PONG, data),
            key::PING if from == 0 => {
                if let Some(ms) = two_bytes(data) {
                    self.own_ping = ms.min(super::vitals::MOST_PING);
                }
            }
            key::RUNNER if from == 0 => {
                if let Some(who) = two_bytes(data) {
                    self.runner = if who == 0 { None } else { Some(who) };
                }
            }
            _ => {}
        }
    }

    /// Whether this player is the one who runs the creatures: the one the
    /// server named (it goes by who came first), or, from a server that names
    /// none (or one who is not here), the one with the lowest number.
    pub fn is_the_runner(&self) -> bool {
        match self.runner {
            Some(r) if r == self.id => true,
            Some(r) if self.players.contains_key(&r) => false,
            _ => self.players.keys().all(|&other| self.id < other),
        }
    }
}
