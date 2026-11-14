//! Mods' messages in a shared game (`send_message` and `on_message`, see
//! docs/SCRIPTING.md): which machine's mod code tells which other machines
//! what. They ride [`crate::proto::Msg::State`] on [`crate::proto::channel::MODMSG`]:
//!
//! * the key is `mod\x1fchannel` (the mod's name, then the channel the mod
//!   chose; printable ASCII, 64 bytes together);
//! * the data is the receiver's player number (two bytes, little-endian;
//!   [`EVERYONE`] for all the others) and then the text (at most
//!   [`MAX_TEXT`] bytes of UTF-8 without control characters).
//!
//! The server checks every one of them before passing it on (a modified
//! client cannot send the others junk, or more than [`PER_SECOND`] messages a
//! second), and the receiving game checks again. Plain functions, tested
//! without sockets.

use alloc::string::String;
use alloc::vec::Vec;

/// Longest text of a message, in bytes.
pub const MAX_TEXT: usize = 512;
/// Most messages a player may send in a second (a few more may come in a burst).
pub const PER_SECOND: f32 = 8.0;
/// Longest key (`mod\x1fchannel`): the same limit as every state key.
pub const MAX_KEY: usize = 64;
/// The receiver number of a message for everyone but the sender.
pub const EVERYONE: u16 = 0xFFFF;

/// Whether `part` is a name that may be part of a key: printable ASCII, not
/// empty, and no unit separator.
fn name_ok(part: &str) -> bool {
    !part.is_empty() && part.bytes().all(|b| (32..127).contains(&b))
}

/// The key for `channel` of the mod `mod_name`, if both are fit for one.
pub fn make_key(mod_name: &str, channel: &str) -> Option<String> {
    if !name_ok(mod_name) || !name_ok(channel) || mod_name.len() + 1 + channel.len() > MAX_KEY {
        return None;
    }
    Some(alloc::format!("{}\u{1f}{}", mod_name, channel))
}

/// The mod and channel of a key, if it is sound.
pub fn split_key(key: &str) -> Option<(&str, &str)> {
    let (m, c) = key.split_once('\u{1f}')?;
    if key.len() > MAX_KEY || !name_ok(m) || !name_ok(c) {
        return None;
    }
    Some((m, c))
}

/// Whether `text` may be sent: valid (it is a `&str`), within the limit, and
/// free of control characters other than newline and tab.
pub fn text_ok(text: &str) -> bool {
    text.len() <= MAX_TEXT && text.chars().all(|c| !c.is_control() || c == '\n' || c == '\t')
}

/// The data of a message to `to` (a player number, or [`EVERYONE`]). `None`
/// if the text is not fit to send.
pub fn pack(to: u16, text: &str) -> Option<Vec<u8>> {
    if !text_ok(text) {
        return None;
    }
    let mut v = Vec::with_capacity(2 + text.len());
    v.extend_from_slice(&to.to_le_bytes());
    v.extend_from_slice(text.as_bytes());
    Some(v)
}

/// The receiver and the text of a message's data, if it is sound.
pub fn unpack(data: &[u8]) -> Option<(u16, &str)> {
    if data.len() < 2 || data.len() > 2 + MAX_TEXT {
        return None;
    }
    let to = u16::from_le_bytes([data[0], data[1]]);
    let text = core::str::from_utf8(&data[2..]).ok()?;
    if !text_ok(text) {
        return None;
    }
    Some((to, text))
}

/// What the server asks of a [`crate::proto::Msg::State`] on the mods' channel:
/// a sound key and sound data.
pub fn valid(key: &str, data: &[u8]) -> bool {
    split_key(key).is_some() && unpack(data).is_some()
}

/// How fast one player may send: [`PER_SECOND`] a second, in bursts of as many.
#[derive(Clone, Debug)]
pub struct Bucket {
    tokens: f32,
    last: f32,
}

impl Default for Bucket {
    fn default() -> Bucket {
        Bucket { tokens: PER_SECOND, last: 0.0 }
    }
}

impl Bucket {
    /// Take one message's worth at time `now` (seconds, any clock that does
    /// not run backwards: a time that does adds nothing). `false`: too many.
    pub fn allow(&mut self, now: f32) -> bool {
        if now.is_finite() {
            let dt = now - self.last;
            if dt > 0.0 {
                self.tokens = (self.tokens + dt * PER_SECOND).min(PER_SECOND);
                self.last = now;
            }
        }
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_carry_the_mod_and_the_channel() {
        let k = make_key("storm", "phase").unwrap();
        assert_eq!(split_key(&k), Some(("storm", "phase")));
        assert!(make_key("", "x").is_none() && make_key("m", "").is_none());
        assert!(make_key("m", "a\u{1f}b").is_none(), "no separator in a name");
        assert!(make_key("m", "caf\u{e9}").is_none(), "printable ASCII only");
        assert!(make_key("m", &"c".repeat(62)).is_some());
        assert!(make_key("m", &"c".repeat(63)).is_none(), "64 bytes with the mod's name");
        assert!(split_key("nothing here").is_none() && split_key("\u{1f}x").is_none() && split_key("x\u{1f}").is_none());
        assert!(split_key(&alloc::format!("m\u{1f}{}", "c".repeat(100))).is_none());
    }

    #[test]
    fn text_has_a_size_limit_and_no_control_characters() {
        assert!(text_ok(&"x".repeat(MAX_TEXT)));
        assert!(!text_ok(&"x".repeat(MAX_TEXT + 1)));
        assert!(text_ok("two\nlines\tand tab") && text_ok("") && text_ok("caf\u{e9}"));
        assert!(!text_ok("bell\u{7}") && !text_ok("nul\u{0}") && !text_ok("esc\u{1b}[0m"));
        // The limit is in bytes, not letters.
        assert!(!text_ok(&"\u{e9}".repeat(MAX_TEXT / 2 + 1)));
    }

    #[test]
    fn data_round_trips_and_hostile_data_is_refused() {
        let d = pack(7, "hello").unwrap();
        assert_eq!(unpack(&d), Some((7, "hello")));
        assert_eq!(unpack(&pack(EVERYONE, "").unwrap()), Some((EVERYONE, "")));
        assert!(pack(1, &"x".repeat(MAX_TEXT + 1)).is_none());
        assert!(unpack(&[]).is_none() && unpack(&[1]).is_none());
        assert!(unpack(&[1, 0, 0xFF, 0xFE]).is_none(), "not UTF-8");
        let mut big = alloc::vec![1u8, 0];
        big.extend(core::iter::repeat(b'x').take(MAX_TEXT + 1));
        assert!(unpack(&big).is_none());
        assert!(unpack(&[1, 0, 7]).is_none(), "a control character");
        assert!(valid("m\u{1f}c", &d) && !valid("m", &d) && !valid("m\u{1f}c", &[0]));
    }

    #[test]
    fn eight_a_second_and_a_burst_of_eight() {
        let mut b = Bucket::default();
        let burst = (0..50).filter(|_| b.allow(0.0)).count();
        assert_eq!(burst, 8);
        assert!(!b.allow(0.05), "less than a token has come back");
        // Half a second brings four more.
        let more = (0..20).filter(|_| b.allow(0.55)).count();
        assert_eq!(more, 4);
        // A long quiet gives the burst back and no more.
        let later = (0..50).filter(|_| b.allow(100.0)).count();
        assert_eq!(later, 8);
        // A clock that runs backwards, or is not a number, gives nothing.
        let mut c = Bucket::default();
        for _ in 0..8 {
            c.allow(10.0);
        }
        assert!(!c.allow(5.0) && !c.allow(f32::NAN));
    }
}
