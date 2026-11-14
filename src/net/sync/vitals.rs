//! The health message of a player: how much health they have (in half
//! hearts) out of how much, their hunger, the four armour pieces they wear
//! and, from the server, their ping. Checked on both ends: nothing outside
//! its ranges is taken.

use alloc::string::String;
use alloc::vec::Vec;

/// Most health (and most maximum health): the meter shows five rows of hearts.
pub const MOST_HEALTH: u8 = 100;
/// Most hunger (as `Session::hunger`).
pub const MOST_FOOD: u8 = 20;
/// Armour slots (head, chest, legs, feet).
pub const SLOTS: usize = 4;
/// A ping is shown up to this many milliseconds.
pub const MOST_PING: u16 = 9999;
/// Longest cause of a death.
pub const MAX_CAUSE: usize = 24;

/// Keys of the state channel [`crate::proto::channel::VITALS`].
pub mod key {
    /// A player's health: from them to the server, and from the server to the others (with their id).
    pub const ME: &str = "me";
    /// "I died": the cause as text; from the dying player to the server.
    pub const DIED: &str = "died";
    /// The server asks for an answer, to measure the ping (two bytes: a number).
    pub const ASK: &str = "ask";
    /// The answer to [`ASK`] with the same two bytes.
    pub const PONG: &str = "pong";
    /// The server tells a player their own ping (two bytes, milliseconds).
    pub const PING: &str = "ping";
    /// The server says which player runs the creatures (two bytes: their number).
    pub const RUNNER: &str = "runner";
}

/// One player's health as shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Vitals {
    /// Half hearts left (0 is dead).
    pub hp: u8,
    /// Most half hearts (1 to 100).
    pub max: u8,
    /// Hunger, 0 to 20.
    pub food: u8,
    /// What is worn on head, chest, legs and feet: the item's place in the
    /// content's list plus one, 0 for nothing.
    pub worn: [u16; SLOTS],
    /// Milliseconds the server measured (0 until it has; only set by the server).
    pub ping: u16,
}

/// Bytes of a message from a player (no ping) and of one from the server.
pub const LEN_FROM_PLAYER: usize = 3 + 2 * SLOTS;
pub const LEN_FROM_SERVER: usize = LEN_FROM_PLAYER + 2;

impl Vitals {
    /// Whether every number is in its range.
    pub fn is_sound(&self) -> bool {
        self.max >= 1 && self.max <= MOST_HEALTH && self.hp <= self.max && self.food <= MOST_FOOD && self.ping <= MOST_PING
    }

    /// The same, held to what the ranges allow (for what we make ourselves).
    pub fn held_to_range(mut self) -> Vitals {
        self.max = self.max.clamp(1, MOST_HEALTH);
        self.hp = self.hp.min(self.max);
        self.food = self.food.min(MOST_FOOD);
        self.ping = self.ping.min(MOST_PING);
        self
    }

    /// The message body; the server's form carries the ping too.
    pub fn encode(&self, with_ping: bool) -> Vec<u8> {
        let mut v = alloc::vec![self.hp, self.max, self.food];
        for w in self.worn {
            v.extend_from_slice(&w.to_le_bytes());
        }
        if with_ping {
            v.extend_from_slice(&self.ping.to_le_bytes());
        }
        v
    }

    /// Read a message body of either form; None if it is not one or a number is out of range.
    pub fn decode(data: &[u8]) -> Option<Vitals> {
        if data.len() != LEN_FROM_PLAYER && data.len() != LEN_FROM_SERVER {
            return None;
        }
        let mut worn = [0u16; SLOTS];
        for (k, w) in worn.iter_mut().enumerate() {
            *w = u16::from_le_bytes([data[3 + 2 * k], data[4 + 2 * k]]);
        }
        let ping = if data.len() == LEN_FROM_SERVER { u16::from_le_bytes([data[LEN_FROM_PLAYER], data[LEN_FROM_PLAYER + 1]]) } else { 0 };
        let v = Vitals { hp: data[0], max: data[1], food: data[2], worn, ping };
        if v.is_sound() {
            Some(v)
        } else {
            None
        }
    }
}

/// A cause of death as it may be sent: lower-case letters, digits, `_` and
/// spaces, at most [`MAX_CAUSE`] long (a creature's id or the name of a kind of harm).
pub fn clean_cause(raw: &[u8]) -> String {
    raw.iter().filter(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || **b == b'_' || **b == b' ').take(MAX_CAUSE).map(|&b| b as char).collect()
}

/// The line the server says when `name` died of `cause`.
pub fn death_line(name: &str, cause: &str) -> String {
    match cause {
        "" | "other" | "mob" => alloc::format!("{} died", name),
        "fall" => alloc::format!("{} fell from a high place", name),
        "drown" => alloc::format!("{} drowned", name),
        "hunger" => alloc::format!("{} starved", name),
        "explosion" => alloc::format!("{} blew up", name),
        "block" => alloc::format!("{} was hurt by a block", name),
        c => alloc::format!("{} was killed by {}", name, c.replace('_', " ")),
    }
}

/// A number of two bytes (a ping, a player number, a question's number).
pub fn two_bytes(data: &[u8]) -> Option<u16> {
    match data {
        [a, b] => Some(u16::from_le_bytes([*a, *b])),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_goes_through_in_both_forms() {
        let v = Vitals { hp: 13, max: 40, food: 17, worn: [3, 0, 900, 65535], ping: 0 };
        let a = v.encode(false);
        assert_eq!(a.len(), LEN_FROM_PLAYER);
        assert_eq!(Vitals::decode(&a), Some(v));
        let b = Vitals { ping: 87, ..v }.encode(true);
        assert_eq!(b.len(), LEN_FROM_SERVER);
        assert_eq!(Vitals::decode(&b).unwrap().ping, 87);
    }

    #[test]
    fn numbers_out_of_range_and_wrong_sizes_are_not_taken() {
        let ok = Vitals { hp: 20, max: 20, food: 20, worn: [0; 4], ping: 0 };
        assert!(Vitals::decode(&ok.encode(false)).is_some());
        for bad in [Vitals { hp: 21, ..ok }, Vitals { max: 0, hp: 0, ..ok }, Vitals { max: 101, hp: 5, ..ok }, Vitals { food: 21, ..ok }] {
            assert_eq!(Vitals::decode(&bad.encode(false)), None, "{bad:?}");
        }
        assert_eq!(Vitals::decode(&Vitals { ping: 10_000, ..ok }.encode(true)), None);
        assert_eq!(Vitals::decode(&[]), None);
        assert_eq!(Vitals::decode(&[1; 12]), None);
        assert_eq!(Vitals::decode(&[1; 14]), None);
        // Dead is allowed, at any maximum.
        assert!(Vitals::decode(&Vitals { hp: 0, ..ok }.encode(false)).is_some());
        assert_eq!(Vitals { hp: 200, max: 0, food: 99, worn: [0; 4], ping: 60_000 }.held_to_range(), Vitals { hp: 1, max: 1, food: 20, worn: [0; 4], ping: 9999 });
    }

    #[test]
    fn a_death_cause_is_plain_and_short_and_the_line_reads_well() {
        assert_eq!(clean_cause(b"fall\x00\n<script>"), "fallscript");
        assert_eq!(clean_cause(&[b'a'; 60]).len(), MAX_CAUSE);
        assert_eq!(death_line("Ann", "fall"), "Ann fell from a high place");
        assert_eq!(death_line("Ann", "giant_spider"), "Ann was killed by giant spider");
        assert_eq!(death_line("Ann", ""), "Ann died");
        assert_eq!(two_bytes(&[1, 2]), Some(513));
        assert_eq!(two_bytes(&[1]), None);
    }
}
