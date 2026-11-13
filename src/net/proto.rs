//! The multiplayer wire protocol: small binary messages over TCP.
//!
//! Every message is `[length: u16][type: u8][body]`, all little-endian;
//! `length` counts the type byte and the body. Both sides generate terrain
//! from the same seed, so only what players change is sent.

use alloc::string::String;
use alloc::vec::Vec;

pub const VERSION: u16 = 7;

/// What a player may do on a server (the flags of [`Msg::Perms`]).
pub const PERM_BUILD: u8 = 1;
pub const PERM_GAMEMODE: u8 = 2;
pub const PERM_COMMANDS: u8 = 4;
/// Not a permission: the player is in creative mode (survival otherwise).
pub const PERM_CREATIVE: u8 = 8;
pub const DEFAULT_PORT: u16 = 25590;
/// Most bytes of pack text in one message.
pub const MAX_PACK_PART: usize = 40_000;
/// Largest skin image, in bytes of PNG.
pub const MAX_SKIN: usize = 24_000;
/// Most bytes of shared state in one message.
pub const MAX_STATE: usize = 60_000;
/// Channels of [`Msg::State`].
pub mod channel {
    /// Every creature, from the player who runs them.
    pub const MOBS: u8 = 1;
    /// "I hit creature N for D", to the player who runs them.
    pub const HIT: u8 = 2;
    /// What is in the chest at `key` ("x y z"); kept.
    pub const CHEST: u8 = 3;
    /// From here up: the mods' own code, one channel each (see
    /// `Session::send_ext`).
    pub const NATIVE: u8 = 32;
}
/// Longest message body accepted.
pub const MAX_BODY: usize = 60_000;

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    // Client to server.
    /// First message from a client. `content` is a checksum of the block
    /// state names: both sides must be running the same data files.
    Hello { version: u16, content: u32, name: String },
    /// Where I am (also relayed to everyone else, with my id).
    /// `held` is the item in hand as its index plus one (0 = empty hand).
    Move { id: u16, pos: [f64; 3], yaw: f32, pitch: f32, held: u16 },
    /// A block changed (sent by whoever changed it, relayed to the rest).
    SetBlock { x: i32, y: i32, z: i32, block: u16 },
    Chat { id: u16, text: String },
    /// A player's skin: a small PNG, and whether their arms are slim
    /// (sent by its owner, relayed with their id).
    Skin { id: u16, png: Vec<u8>, slim: bool },
    // Server to client.
    Welcome { id: u16, seed: u32, time: f32, kind: String },
    /// The server will not let you in.
    Refuse { reason: String },
    Join { id: u16, name: String },
    Leave { id: u16 },
    /// Many block changes at once (the world's history, sent on joining).
    Edits { list: Vec<(i32, i32, i32, u16)> },
    Time { time: f32 },
    /// A piece of the server's content pack (text); `last` ends it. Every
    /// player gets this on joining, so all use the same blocks and items.
    Pack { last: bool, data: Vec<u8> },
    /// What you may do here (`PERM_` flags), sent on joining and whenever it changes.
    Perms { flags: u8 },
    /// Shared state of the game, passed between players through the
    /// server (sent by a client, relayed with its id): creatures, chests,
    /// what is in a chest. `channel` says which; `key` names the thing;
    /// with `keep` the server remembers the latest for players who join later.
    State { id: u16, channel: u8, keep: bool, key: String, data: Vec<u8> },
}

struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        let b = &s.as_bytes()[..s.len().min(200)];
        self.u8(b.len() as u8);
        self.0.extend_from_slice(b);
    }
}

struct R<'a>(&'a [u8]);

impl R<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(self.u32()? as i32)
    }
    fn f32(&mut self) -> Option<f32> {
        let v = f32::from_bits(self.u32()?);
        if v.is_finite() {
            Some(v)
        } else {
            None
        }
    }
    fn f64(&mut self) -> Option<f64> {
        let v = f64::from_le_bytes(self.take(8)?.try_into().ok()?);
        if v.is_finite() {
            Some(v)
        } else {
            None
        }
    }
    fn str(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        let b = self.take(n)?;
        // Printable ASCII only: names and chat are shown with the built-in font.
        Some(b.iter().filter(|c| (32..127).contains(*c)).map(|&c| c as char).collect())
    }
}

impl Msg {
    /// The whole message, ready to send.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = W(alloc::vec![0, 0]);
        match self {
            Msg::Hello { version, content, name } => {
                w.u8(1);
                w.u16(*version);
                w.u32(*content);
                w.str(name);
            }
            Msg::Move { id, pos, yaw, pitch, held } => {
                w.u8(2);
                w.u16(*id);
                for p in pos {
                    w.f64(*p);
                }
                w.f32(*yaw);
                w.f32(*pitch);
                w.u16(*held);
            }
            Msg::SetBlock { x, y, z, block } => {
                w.u8(3);
                w.i32(*x);
                w.i32(*y);
                w.i32(*z);
                w.u16(*block);
            }
            Msg::Chat { id, text } => {
                w.u8(4);
                w.u16(*id);
                w.str(text);
            }
            Msg::Skin { id, png, slim } => {
                w.u8(5);
                w.u16(*id);
                let n = png.len().min(MAX_SKIN);
                w.u16(n as u16);
                w.0.extend_from_slice(&png[..n]);
                w.u8(*slim as u8);
            }
            Msg::State { id, channel, keep, key, data } => {
                w.u8(6);
                w.u16(*id);
                w.u8(*channel);
                w.u8(*keep as u8);
                w.str(key);
                let n = data.len().min(MAX_STATE);
                w.u16(n as u16);
                w.0.extend_from_slice(&data[..n]);
            }
            Msg::Welcome { id, seed, time, kind } => {
                w.u8(10);
                w.u16(*id);
                w.u32(*seed);
                w.f32(*time);
                w.str(kind);
            }
            Msg::Refuse { reason } => {
                w.u8(11);
                w.str(reason);
            }
            Msg::Join { id, name } => {
                w.u8(12);
                w.u16(*id);
                w.str(name);
            }
            Msg::Leave { id } => {
                w.u8(13);
                w.u16(*id);
            }
            Msg::Edits { list } => {
                w.u8(14);
                let n = list.len().min(4000);
                w.u16(n as u16);
                for &(x, y, z, b) in &list[..n] {
                    w.i32(x);
                    w.i32(y);
                    w.i32(z);
                    w.u16(b);
                }
            }
            Msg::Time { time } => {
                w.u8(15);
                w.f32(*time);
            }
            Msg::Perms { flags } => {
                w.u8(17);
                w.u8(*flags);
            }
            Msg::Pack { last, data } => {
                w.u8(16);
                w.u8(*last as u8);
                let n = data.len().min(MAX_PACK_PART);
                w.u16(n as u16);
                w.0.extend_from_slice(&data[..n]);
            }
        }
        let len = (w.0.len() - 2) as u16;
        w.0[..2].copy_from_slice(&len.to_le_bytes());
        w.0
    }

    fn decode_body(kind: u8, body: &[u8]) -> Option<Msg> {
        let mut r = R(body);
        let m = match kind {
            1 => Msg::Hello { version: r.u16()?, content: r.u32()?, name: r.str()? },
            2 => Msg::Move { id: r.u16()?, pos: [r.f64()?, r.f64()?, r.f64()?], yaw: r.f32()?, pitch: r.f32()?, held: r.u16()? },
            3 => Msg::SetBlock { x: r.i32()?, y: r.i32()?, z: r.i32()?, block: r.u16()? },
            4 => Msg::Chat { id: r.u16()?, text: r.str()? },
            5 => {
                let id = r.u16()?;
                let n = r.u16()? as usize;
                if n > MAX_SKIN {
                    return None;
                }
                let png = r.take(n)?.to_vec();
                Msg::Skin { id, png, slim: r.u8()? != 0 }
            }
            6 => {
                let (id, channel, keep, key) = (r.u16()?, r.u8()?, r.u8()? != 0, r.str()?);
                let n = r.u16()? as usize;
                if n > MAX_STATE || key.len() > 64 {
                    return None;
                }
                Msg::State { id, channel, keep, key, data: r.take(n)?.to_vec() }
            }
            10 => Msg::Welcome { id: r.u16()?, seed: r.u32()?, time: r.f32()?, kind: r.str()? },
            11 => Msg::Refuse { reason: r.str()? },
            12 => Msg::Join { id: r.u16()?, name: r.str()? },
            13 => Msg::Leave { id: r.u16()? },
            14 => {
                let n = r.u16()? as usize;
                let mut list = Vec::with_capacity(n.min(4000));
                for _ in 0..n {
                    list.push((r.i32()?, r.i32()?, r.i32()?, r.u16()?));
                }
                Msg::Edits { list }
            }
            15 => Msg::Time { time: r.f32()? },
            17 => Msg::Perms { flags: r.u8()? },
            16 => {
                let last = r.u8()? != 0;
                let n = r.u16()? as usize;
                if n > MAX_PACK_PART {
                    return None;
                }
                Msg::Pack { last, data: r.take(n)?.to_vec() }
            }
            _ => return None,
        };
        if r.0.is_empty() {
            Some(m)
        } else {
            None
        }
    }
}

/// Bytes received so far, from which whole messages are taken.
#[derive(Default)]
pub struct Inbox {
    buf: Vec<u8>,
}

/// What [`Inbox::next`] found.
#[derive(Debug, PartialEq)]
pub enum Next {
    Msg(Msg),
    /// Not enough bytes yet.
    Wait,
    /// The stream is not speaking this protocol: drop the connection.
    Bad,
}

impl Inbox {
    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    pub fn next(&mut self) -> Next {
        if self.buf.len() < 3 {
            return Next::Wait;
        }
        let len = u16::from_le_bytes([self.buf[0], self.buf[1]]) as usize;
        if len == 0 || len > MAX_BODY {
            return Next::Bad;
        }
        if self.buf.len() < 2 + len {
            return Next::Wait;
        }
        let msg = Msg::decode_body(self.buf[2], &self.buf[3..2 + len]);
        self.buf.drain(..2 + len);
        match msg {
            Some(m) => Next::Msg(m),
            None => Next::Bad,
        }
    }

    /// The raw bytes waiting (used to recognise a relay request line).
    pub fn peek(&self) -> &[u8] {
        &self.buf
    }
}

/// Checksum of the block state names, to check both sides have the same content.
pub fn content_checksum() -> u32 {
    crate::png::crc32(crate::save::current_palette().join("\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_round_trips() {
        let all = [
            Msg::Hello { version: VERSION, content: 0xDEAD_BEEF, name: String::from("Kyttu_1") },
            Msg::Move { id: 7, pos: [1.5, -64.25, 1e6], yaw: -3.1, pitch: 0.5, held: 9 },
            Msg::SetBlock { x: -100_000, y: 127, z: 5, block: 4095 },
            Msg::Chat { id: 2, text: String::from("hello there") },
            Msg::Skin { id: 4, png: alloc::vec![1, 2, 3, 250], slim: true },
            Msg::State { id: 4, channel: 3, keep: true, key: String::from("1 2 3"), data: b"chest".to_vec() },
            Msg::Welcome { id: 3, seed: 42, time: 0.25, kind: String::from("somewhere 46.8 -121.7 30 75") },
            Msg::Refuse { reason: String::from("server full") },
            Msg::Join { id: 9, name: String::from("Bob") },
            Msg::Leave { id: 9 },
            Msg::Edits { list: alloc::vec![(1, 2, 3, 4), (-5, 6, -7, 800)] },
            Msg::Time { time: 0.99 },
            Msg::Perms { flags: PERM_BUILD | PERM_CREATIVE },
            Msg::Pack { last: true, data: b"block x\n tex stone\n".to_vec() },
        ];
        let mut inbox = Inbox::default();
        for m in &all {
            inbox.push(&m.encode());
        }
        for m in &all {
            assert_eq!(inbox.next(), Next::Msg(m.clone()));
        }
        assert_eq!(inbox.next(), Next::Wait);
    }

    #[test]
    fn messages_survive_being_split_and_junk_is_rejected() {
        let bytes = Msg::Chat { id: 1, text: String::from("split me") }.encode();
        let mut inbox = Inbox::default();
        for b in &bytes[..bytes.len() - 1] {
            inbox.push(&[*b]);
            assert_eq!(inbox.next(), Next::Wait);
        }
        inbox.push(&bytes[bytes.len() - 1..]);
        assert!(matches!(inbox.next(), Next::Msg(Msg::Chat { .. })));

        // Unknown type, truncated body, trailing bytes, zero length, NaN.
        for bad in [&[1u8, 0, 99][..], &[2, 0, 13, 1][..], &[4, 0, 13, 1, 0, 9][..], &[0, 0, 1][..]] {
            let mut i = Inbox::default();
            i.push(bad);
            assert_eq!(i.next(), Next::Bad, "{bad:?}");
        }
        let mut nan = Msg::Time { time: 0.0 }.encode();
        nan[3..7].copy_from_slice(&f32::NAN.to_le_bytes());
        let mut i = Inbox::default();
        i.push(&nan);
        assert_eq!(i.next(), Next::Bad);
        // Names are cleaned of anything that is not printable ASCII.
        let mut hello = Msg::Join { id: 1, name: String::from("a\u{7}b") }.encode();
        hello[6] = 200;
        let mut i = Inbox::default();
        i.push(&hello);
        assert_eq!(i.next(), Next::Msg(Msg::Join { id: 1, name: String::from("b") }));
    }
}
