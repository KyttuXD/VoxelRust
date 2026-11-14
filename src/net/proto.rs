//! The multiplayer wire protocol: small binary messages over TCP.
//!
//! Every message is `[length: u16][type: u8][body]`, all little-endian;
//! `length` counts the type byte and the body. Both sides generate terrain
//! from the same seed, so only what players change is sent.

use alloc::string::String;
use alloc::vec::Vec;

/// The protocol version: both sides must speak the same one (the build name
/// and the release number do not matter for compatibility, see
/// `net::version`). Version 12 added [`Msg::Info`]. Version 13 gave the
/// creature snapshot a size, bone overrides and a state text per creature
/// (`game/creature_wire.rs`) and added the mods' message channel
/// ([`channel::MODMSG`], `net/modmsg.rs`). Version 14 added big (HD) skins in
/// pieces ([`Msg::SkinCaps`], [`Msg::SkinBegin`], [`Msg::SkinPart`],
/// `net/skinxfer.rs`) and the accounts ([`Msg::AuthChallenge`] and the
/// messages around it, `net/accounts`).
pub const VERSION: u16 = 14;
/// The oldest protocol a server still lets in: version 13 clients know every
/// message a 14 server sends them (it sends them the 64 x 64 version of a
/// big skin, as a plain [`Msg::Skin`], and none of the account messages: a
/// name with an account is refused to them, see `net::accounts::OLDEST_ACCEPTED`).
pub const MIN_VERSION: u16 = 13;

/// What a player may do on a server (the flags of [`Msg::Perms`]).
pub const PERM_BUILD: u8 = 1;
pub const PERM_GAMEMODE: u8 = 2;
pub const PERM_COMMANDS: u8 = 4;
/// Not a permission: the player is in creative mode (survival otherwise).
pub const PERM_CREATIVE: u8 = 8;
/// May talk over proximity voice chat (allowed by default).
pub const PERM_VOICE: u8 = 16;
/// May fight other players, and be fought (allowed by default; see `net/pvp`).
pub const PERM_PVP: u8 = 32;
/// Largest voice frame, in bytes (a 20 ms frame is about 163).
pub const MAX_VOICE_FRAME: usize = 200;
/// [`Msg::VoiceState`] flags: this client wants to hear others / may talk.
pub const VOICE_HEAR: u8 = 1;
pub const VOICE_TALK: u8 = 2;
pub const DEFAULT_PORT: u16 = 25590;
/// Most bytes of pack text in one message.
pub const MAX_PACK_PART: usize = 40_000;
/// Largest skin image, in bytes of PNG, however it is sent (in one
/// [`Msg::Skin`] or in pieces).
pub const MAX_SKIN: usize = 1 << 20;
/// Largest skin in one plain [`Msg::Skin`] (what clients of protocol 13 take
/// too); bigger ones go in pieces.
pub const MAX_SKIN_MSG: usize = 24_000;
/// Most bytes of a skin in one [`Msg::SkinPart`].
pub const MAX_SKIN_PART: usize = 40_000;
/// Most bytes of shared state in one message. A message body may be
/// [`MAX_BODY`] long in all, and the header of a state message (id, channel,
/// key of up to 64 bytes) takes about 75 of those: more than this would be
/// refused by the receiver, which drops the connection.
pub const MAX_STATE: usize = 59_000;
/// Channels of [`Msg::State`].
pub mod channel {
    /// Every creature, from the player who runs them.
    pub const MOBS: u8 = 1;
    /// "I hit creature N for D", to the player who runs them.
    pub const HIT: u8 = 2;
    /// What is in the chest at `key` ("x y z"); kept.
    pub const CHEST: u8 = 3;
    /// Messages of mods' scripts to the same mod's scripts elsewhere
    /// (`net_send` and `on_net`, see script/ext/net.rs); the key is `mod\x1fkey`.
    pub const SCRIPT: u8 = 4;
    /// Messages of mods to the same mod on other machines (`send_message` and
    /// `on_message`, see net/modmsg.rs): the key is `mod\x1fchannel`, the data
    /// the receiver's number (two bytes, 0xFFFF for everyone) and the text.
    /// Checked by the server; never kept.
    pub const MODMSG: u8 = 5;
    /// From the server to a joining player: where they were when they left (`x y z yaw pitch`).
    pub const PLAYER: u8 = 6;
    /// Players' health, hunger, worn armour, ping and death notices, the
    /// server's questions that measure a ping, and who runs the creatures
    /// (`net/sync`); never kept. A game that does not know the channel ignores it.
    pub const VITALS: u8 = 7;
    /// Player against player (`net/pvp`, no protocol change: older games ignore these).
    /// "I hit player N": from the attacker to the server only.
    pub const PVP_HIT: u8 = 8;
    /// "You were hit": from the server (id 0) to the one who was hit.
    pub const PVP_HURT: u8 = 9;
    /// "Player N was hit by M": from the server (id 0) to those nearby, for the flash.
    pub const PVP_SEEN: u8 = 10;
    /// "My armour, and whether I am alive": from a client to the server only; a
    /// game that sends it can be fought.
    pub const PVP_GEAR: u8 = 11;
    /// From here up: the mods' own code, one channel each (see
    /// `Session::send_ext`).
    pub const NATIVE: u8 = 32;
}
/// Most status effects in one [`Msg::Effects`].
pub const MAX_EFFECTS: usize = 16;
/// Longest message body accepted.
pub const MAX_BODY: usize = 60_000;

/// One mod a server offers for download (see [`Msg::ModList`] and
/// `net::modsync`).
#[derive(Clone, Debug, PartialEq)]
pub struct ModInfo {
    pub name: String,
    /// What its pack.txt says (`version 1.2`); empty if it says nothing.
    pub version: String,
    /// First 8 bytes of the SHA-256 of the mod's bundle.
    pub sum: u64,
    /// Bytes of the bundle that will be sent, and how many files are in it.
    pub size: u32,
    pub files: u16,
}

/// Most mods in one [`Msg::ModList`].
pub const MAX_MOD_LIST: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    // Client to server.
    /// First message from a client. `content` is a checksum of the block
    /// state names: both sides must be running the same data files.
    Hello { version: u16, content: u32, name: String },
    /// Where I am (also relayed to everyone else, with my id).
    /// `held` is the item in hand as its index plus one (0 = empty hand).
    /// `pose` is the body's posture in the low four bits (sitting, lying,
    /// crouching, swimming...) and what the arms are doing above (see
    /// `content::pose::pack`).
    Move { id: u16, pos: [f64; 3], yaw: f32, pitch: f32, held: u16, pose: u8 },
    /// I started playing an emote (its place in the content's emote list
    /// plus one), or stopped (0). Relayed to the others with my id.
    Emote { id: u16, emote: u16 },
    /// The status effects on a player (poison, burning, mods' own...), sent
    /// by them whenever the set changes (an effect starts, ends or changes
    /// level) and relayed with their id; also told to players who join
    /// later. Each is its id, level and the whole seconds left. The server
    /// keeps it only to relay: everyone's effects are their own game's.
    Effects { id: u16, list: Vec<(String, u8, u32)> },
    /// A block changed (sent by whoever changed it, relayed to the rest).
    SetBlock { x: i32, y: i32, z: i32, block: u16 },
    Chat { id: u16, text: String },
    /// A player's skin: a small PNG, and whether their arms are slim
    /// (sent by its owner, relayed with their id).
    Skin { id: u16, png: Vec<u8>, slim: bool },
    /// From the server, right after [`Msg::Info`], to a client of protocol 14
    /// or later: big skins are welcome, up to this many pixels on a side and
    /// this many bytes of PNG. A client that never gets it sends skins of
    /// 64 x 64 only.
    SkinCaps { max_side: u16, max_bytes: u32 },
    /// A skin that does not fit in one message is coming: `size` bytes of PNG
    /// of a picture `side` x `side` pixels, in [`Msg::SkinPart`]s that follow
    /// at once (one transfer at a time per player). Sent by its owner, relayed
    /// with their id.
    SkinBegin { id: u16, size: u32, side: u16, slim: bool },
    /// The next piece of the skin that [`Msg::SkinBegin`] announced; `last`
    /// ends it.
    SkinPart { id: u16, last: bool, data: Vec<u8> },
    /// Voice chat: one 20 ms frame of ADPCM speech (see `client::voice::codec`).
    /// Sent by a talker (`id` ignored, the server knows who it is) and relayed
    /// by the server only to players in range, with `gain` (0..=255) already
    /// worked out from the distance.
    Voice { id: u16, seq: u16, gain: u8, frame: Vec<u8> },
    /// Voice chat switches of this client (`VOICE_` flags): whether it wants
    /// to hear others and whether it may talk (its microphone is on). Players
    /// start with both off, so a client that never sends this costs nothing.
    VoiceState { flags: u8 },
    /// "Send me these mods" (names from the [`Msg::ModList`]); answered with
    /// [`Msg::ModPart`]s, one mod after the other in the order asked.
    ModsWant { names: Vec<String> },
    /// The answer to a login [`Msg::AuthChallenge`]: the HMAC of the nonce and the name under
    /// the key the PIN makes (`net/accounts/keys.rs`). The PIN itself is never sent.
    AuthLogin { proof: [u8; 32] },
    /// The answer to a register challenge: the key the new PIN makes with the salt the server
    /// chose (the server keeps it as the account's verifier).
    AuthRegister { key: [u8; 32] },
    /// The answer to a change challenge: the proof for the old PIN and the key of the new one.
    AuthChange { proof: [u8; 32], key: [u8; 32] },
    /// "Give me a challenge" for `op` (`accounts::state::op`), from a player already in the game
    /// (`/register`, `/pin`).
    AuthAsk { op: u8 },
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
    /// The mods the server offers, sent on joining (after the pack, even
    /// when there are none).
    ModList { mods: Vec<ModInfo> },
    /// A piece of one mod's bundle (`index` is its place in the list);
    /// `last` ends it.
    ModPart { index: u16, last: bool, data: Vec<u8> },
    /// Shared state of the game, passed between players through the
    /// server (sent by a client, relayed with its id): creatures, chests,
    /// what is in a chest. `channel` says which; `key` names the thing;
    /// with `keep` the server remembers the latest for players who join later.
    State { id: u16, channel: u8, keep: bool, key: String, data: Vec<u8> },
    /// A challenge for a login, a registration or a PIN change (`op`, see
    /// `accounts::state::op`). `salt` is the account's, `new_salt` is for a new PIN's key,
    /// `nonce` is fresh for this challenge, `rounds` how often the PIN is hashed, `tries` how
    /// many wrong tries are left, `flags` as in `accounts::state::flag`.
    AuthChallenge { op: u8, flags: u8, rounds: u32, tries: u8, salt: [u8; 16], new_salt: [u8; 16], nonce: [u8; 16] },
    /// How an account step went (`ok`), and what to tell the player. `tries` is how many
    /// wrong tries are left after a wrong PIN.
    AuthResult { ok: bool, tries: u8, text: String },
    /// What the server's accounts say about this player, sent after [`Msg::Welcome`] and again
    /// when it changes: the server's mode (0 no accounts, 1 optional, 2 required) and
    /// `accounts::state::flag` bits.
    Account { mode: u8, flags: u8 },
    /// Which program the server is: its build name, release number (0 when
    /// built by hand) and commit. Sent right after [`Msg::Welcome`]; only
    /// for showing to the player (the protocol version decides whether a
    /// client may join).
    Info { build: String, release: u32, commit: String },
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
        self.str_n(s, 200);
    }
    /// A string cut to `max` bytes (at most 200).
    fn str_n(&mut self, s: &str, max: usize) {
        let b = &s.as_bytes()[..s.len().min(max.min(200))];
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
    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
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
    /// A world coordinate: finite and within what a block position can be (a
    /// huge one would turn into infinity in the maths that follows).
    fn coord(&mut self) -> Option<f64> {
        let v = self.f64()?;
        if v > -2.2e9 && v < 2.2e9 {
            Some(v)
        } else {
            None
        }
    }
    /// A state key: printable ASCII and the unit separator `\x1f` (the
    /// script channel's keys are `mod\x1fkey`, and `str` would drop it).
    fn key(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        let b = self.take(n)?;
        Some(b.iter().filter(|c| (32..127).contains(*c) || **c == 0x1f).map(|&c| c as char).collect())
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
            Msg::Move { id, pos, yaw, pitch, held, pose } => {
                w.u8(2);
                w.u16(*id);
                for p in pos {
                    w.f64(*p);
                }
                w.f32(*yaw);
                w.f32(*pitch);
                w.u16(*held);
                w.u8(*pose);
            }
            Msg::Emote { id, emote } => {
                w.u8(20);
                w.u16(*id);
                w.u16(*emote);
            }
            Msg::Effects { id, list } => {
                w.u8(21);
                w.u16(*id);
                let n = list.len().min(MAX_EFFECTS);
                w.u8(n as u8);
                for (name, level, secs) in &list[..n] {
                    w.str(name);
                    w.u8(*level);
                    w.u32(*secs);
                }
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
                let n = png.len().min(MAX_SKIN_MSG);
                w.u16(n as u16);
                w.0.extend_from_slice(&png[..n]);
                w.u8(*slim as u8);
            }
            Msg::SkinCaps { max_side, max_bytes } => {
                w.u8(23);
                w.u16(*max_side);
                w.u32(*max_bytes);
            }
            Msg::SkinBegin { id, size, side, slim } => {
                w.u8(24);
                w.u16(*id);
                w.u32(*size);
                w.u16(*side);
                w.u8(*slim as u8);
            }
            Msg::SkinPart { id, last, data } => {
                w.u8(25);
                w.u16(*id);
                w.u8(*last as u8);
                let n = data.len().min(MAX_SKIN_PART);
                w.u16(n as u16);
                w.0.extend_from_slice(&data[..n]);
            }
            Msg::State { id, channel, keep, key, data } => {
                w.u8(6);
                w.u16(*id);
                w.u8(*channel);
                w.u8(*keep as u8);
                // (The receiver refuses keys over 64 bytes.)
                w.str_n(key, 64);
                let n = data.len().min(MAX_STATE);
                w.u16(n as u16);
                w.0.extend_from_slice(&data[..n]);
            }
            Msg::Voice { id, seq, gain, frame } => {
                w.u8(7);
                w.u16(*id);
                w.u16(*seq);
                w.u8(*gain);
                let n = frame.len().min(MAX_VOICE_FRAME);
                w.u8(n as u8);
                w.0.extend_from_slice(&frame[..n]);
            }
            Msg::VoiceState { flags } => {
                w.u8(8);
                w.u8(*flags);
            }
            Msg::ModsWant { names } => {
                w.u8(9);
                let n = names.len().min(MAX_MOD_LIST);
                w.u8(n as u8);
                for name in &names[..n] {
                    w.str(name);
                }
            }
            Msg::ModList { mods } => {
                w.u8(18);
                let n = mods.len().min(MAX_MOD_LIST);
                w.u8(n as u8);
                for m in &mods[..n] {
                    w.str(&m.name);
                    w.str(&m.version);
                    w.0.extend_from_slice(&m.sum.to_le_bytes());
                    w.u32(m.size);
                    w.u16(m.files);
                }
            }
            Msg::ModPart { index, last, data } => {
                w.u8(19);
                w.u16(*index);
                w.u8(*last as u8);
                let n = data.len().min(MAX_PACK_PART);
                w.u16(n as u16);
                w.0.extend_from_slice(&data[..n]);
            }
            Msg::AuthLogin { proof } => {
                w.u8(40);
                w.0.extend_from_slice(proof);
            }
            Msg::AuthRegister { key } => {
                w.u8(41);
                w.0.extend_from_slice(key);
            }
            Msg::AuthChange { proof, key } => {
                w.u8(42);
                w.0.extend_from_slice(proof);
                w.0.extend_from_slice(key);
            }
            Msg::AuthAsk { op } => {
                w.u8(43);
                w.u8(*op);
            }
            Msg::AuthChallenge { op, flags, rounds, tries, salt, new_salt, nonce } => {
                w.u8(44);
                w.u8(*op);
                w.u8(*flags);
                w.u32(*rounds);
                w.u8(*tries);
                w.0.extend_from_slice(salt);
                w.0.extend_from_slice(new_salt);
                w.0.extend_from_slice(nonce);
            }
            Msg::AuthResult { ok, tries, text } => {
                w.u8(45);
                w.u8(*ok as u8);
                w.u8(*tries);
                w.str(text);
            }
            Msg::Account { mode, flags } => {
                w.u8(46);
                w.u8(*mode);
                w.u8(*flags);
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
            Msg::Info { build, release, commit } => {
                w.u8(22);
                w.str(build);
                w.u32(*release);
                w.str(commit);
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
            2 => Msg::Move { id: r.u16()?, pos: [r.coord()?, r.coord()?, r.coord()?], yaw: r.f32()?, pitch: r.f32()?, held: r.u16()?, pose: r.u8()? },
            3 => Msg::SetBlock { x: r.i32()?, y: r.i32()?, z: r.i32()?, block: r.u16()? },
            20 => Msg::Emote { id: r.u16()?, emote: r.u16()? },
            21 => {
                let id = r.u16()?;
                let n = r.u8()? as usize;
                if n > MAX_EFFECTS {
                    return None;
                }
                let mut list = Vec::with_capacity(n);
                for _ in 0..n {
                    list.push((r.str()?, r.u8()?, r.u32()?));
                }
                Msg::Effects { id, list }
            }
            4 => Msg::Chat { id: r.u16()?, text: r.str()? },
            5 => {
                let id = r.u16()?;
                let n = r.u16()? as usize;
                if n > MAX_SKIN_MSG {
                    return None;
                }
                let png = r.take(n)?.to_vec();
                Msg::Skin { id, png, slim: r.u8()? != 0 }
            }
            23 => Msg::SkinCaps { max_side: r.u16()?, max_bytes: r.u32()? },
            24 => Msg::SkinBegin { id: r.u16()?, size: r.u32()?, side: r.u16()?, slim: r.u8()? != 0 },
            25 => {
                let (id, last) = (r.u16()?, r.u8()? != 0);
                let n = r.u16()? as usize;
                if n > MAX_SKIN_PART {
                    return None;
                }
                Msg::SkinPart { id, last, data: r.take(n)?.to_vec() }
            }
            6 => {
                let (id, channel, keep, key) = (r.u16()?, r.u8()?, r.u8()? != 0, r.key()?);
                let n = r.u16()? as usize;
                if n > MAX_STATE || key.len() > 64 {
                    return None;
                }
                Msg::State { id, channel, keep, key, data: r.take(n)?.to_vec() }
            }
            7 => {
                let (id, seq, gain) = (r.u16()?, r.u16()?, r.u8()?);
                let n = r.u8()? as usize;
                if n > MAX_VOICE_FRAME {
                    return None;
                }
                Msg::Voice { id, seq, gain, frame: r.take(n)?.to_vec() }
            }
            8 => Msg::VoiceState { flags: r.u8()? },
            9 => {
                let n = r.u8()? as usize;
                if n > MAX_MOD_LIST {
                    return None;
                }
                let mut names = Vec::with_capacity(n);
                for _ in 0..n {
                    names.push(r.str()?);
                }
                Msg::ModsWant { names }
            }
            18 => {
                let n = r.u8()? as usize;
                if n > MAX_MOD_LIST {
                    return None;
                }
                let mut mods = Vec::with_capacity(n);
                for _ in 0..n {
                    let (name, version) = (r.str()?, r.str()?);
                    let sum = u64::from_le_bytes(r.take(8)?.try_into().ok()?);
                    mods.push(ModInfo { name, version, sum, size: r.u32()?, files: r.u16()? });
                }
                Msg::ModList { mods }
            }
            19 => {
                let (index, last) = (r.u16()?, r.u8()? != 0);
                let n = r.u16()? as usize;
                if n > MAX_PACK_PART {
                    return None;
                }
                Msg::ModPart { index, last, data: r.take(n)?.to_vec() }
            }
            22 => Msg::Info { build: r.str()?, release: r.u32()?, commit: r.str()? },
            40 => Msg::AuthLogin { proof: r.array()? },
            41 => Msg::AuthRegister { key: r.array()? },
            42 => Msg::AuthChange { proof: r.array()?, key: r.array()? },
            43 => Msg::AuthAsk { op: r.u8()? },
            44 => Msg::AuthChallenge { op: r.u8()?, flags: r.u8()?, rounds: r.u32()?, tries: r.u8()?, salt: r.array()?, new_salt: r.array()?, nonce: r.array()? },
            45 => Msg::AuthResult { ok: r.u8()? != 0, tries: r.u8()?, text: r.str()? },
            46 => Msg::Account { mode: r.u8()?, flags: r.u8()? },
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
    let mut text = crate::save::current_palette().join("\n");
    // The sizes of the HD pictures are content too (two ends with the same names and other
    // sizes disagree). Without HD tiles this adds nothing: the checksum is what it always was.
    text.push_str(&crate::hd::source::checksum_text(&crate::block::reg().tiles));
    crate::png::crc32(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_round_trips() {
        let all = [
            Msg::Hello { version: VERSION, content: 0xDEAD_BEEF, name: String::from("Kyttu_1") },
            Msg::Move { id: 7, pos: [1.5, -64.25, 1e6], yaw: -3.1, pitch: 0.5, held: 9, pose: 0x23 },
            Msg::Emote { id: 3, emote: 12 },
            Msg::Emote { id: 3, emote: 0 },
            Msg::Effects { id: 3, list: alloc::vec![(String::from("poison"), 2, 17), (String::from("night_vision"), 255, 86_400)] },
            Msg::Effects { id: 0, list: Vec::new() },
            Msg::SetBlock { x: -100_000, y: 127, z: 5, block: 4095 },
            Msg::Chat { id: 2, text: String::from("hello there") },
            Msg::Skin { id: 4, png: alloc::vec![1, 2, 3, 250], slim: true },
            Msg::SkinCaps { max_side: 4096, max_bytes: MAX_SKIN as u32 },
            Msg::SkinBegin { id: 4, size: 123_456, side: 1024, slim: false },
            Msg::SkinPart { id: 4, last: false, data: alloc::vec![9; MAX_SKIN_PART] },
            Msg::SkinPart { id: 4, last: true, data: alloc::vec![1, 2, 3] },
            Msg::State { id: 4, channel: 3, keep: true, key: String::from("1 2 3"), data: b"chest".to_vec() },
            // The script channel's key has a separator in it, which must get through.
            Msg::State { id: 4, channel: channel::SCRIPT, keep: false, key: String::from("mymod\u{1f}score"), data: b"7".to_vec() },
            // A mod's message: a receiver number in front of the text.
            Msg::State { id: 4, channel: channel::MODMSG, keep: false, key: String::from("storm\u{1f}phase"), data: crate::modmsg::pack(0xFFFF, "rage 2").unwrap() },
            Msg::State { id: 4, channel: channel::MODMSG, keep: false, key: String::from("storm\u{1f}phase"), data: crate::modmsg::pack(3, "").unwrap() },
            // The creature snapshot with a size, bones and a state after the ten fields.
            Msg::State { id: 1, channel: channel::MOBS, keep: false, key: String::new(), data: b"7 boomer 0 1.00 64.00 2.00 0.50 0 900 0|S12.00,40.00|M5000|O150,-30|B2:100,0,0,0,50,0,150;5:0,0,-300,0,0,0,100|Trage%7C2\n".to_vec() },
            Msg::Info { build: String::from("linux-server-x86_64"), release: 321, commit: String::from("5d7c83d") },
            Msg::Welcome { id: 3, seed: 42, time: 0.25, kind: String::from("somewhere 46.8 -121.7 30 75") },
            Msg::Refuse { reason: String::from("server full") },
            Msg::Join { id: 9, name: String::from("Bob") },
            Msg::Leave { id: 9 },
            Msg::Edits { list: alloc::vec![(1, 2, 3, 4), (-5, 6, -7, 800)] },
            Msg::Time { time: 0.99 },
            Msg::Perms { flags: PERM_BUILD | PERM_CREATIVE },
            Msg::Pack { last: true, data: b"block x\n tex stone\n".to_vec() },
            Msg::Voice { id: 5, seq: 65_535, gain: 200, frame: alloc::vec![9; 163] },
            Msg::Voice { id: 0, seq: 0, gain: 0, frame: Vec::new() },
            Msg::VoiceState { flags: VOICE_HEAR | VOICE_TALK },
            Msg::ModsWant { names: alloc::vec![String::from("earth"), String::from("my-mod")] },
            Msg::ModsWant { names: Vec::new() },
            Msg::ModList { mods: alloc::vec![ModInfo { name: String::from("earth"), version: String::from("1.2"), sum: 0xDEAD_BEEF_0123_4567, size: 123_456, files: 40 }, ModInfo { name: String::from("x"), version: String::new(), sum: 0, size: 0, files: 0 }] },
            Msg::ModList { mods: Vec::new() },
            Msg::ModPart { index: 3, last: false, data: alloc::vec![7; 1000] },
            Msg::ModPart { index: 0, last: true, data: Vec::new() },
            Msg::AuthLogin { proof: [7; 32] },
            Msg::AuthRegister { key: [8; 32] },
            Msg::AuthChange { proof: [1; 32], key: [2; 32] },
            Msg::AuthAsk { op: 2 },
            Msg::AuthChallenge { op: 1, flags: 5, rounds: 20_000, tries: 3, salt: [1; 16], new_salt: [2; 16], nonce: [3; 16] },
            Msg::AuthResult { ok: false, tries: 2, text: String::from("wrong PIN") },
            Msg::AuthResult { ok: true, tries: 0, text: String::new() },
            Msg::Account { mode: 1, flags: 3 },
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
    fn the_biggest_state_message_is_not_over_the_body_limit() {
        // A key as long as allowed and as much data as allowed must still be a message
        // the receiver takes (it used to be 60 000 bytes of data, which it dropped).
        let key = "k".repeat(64);
        let m = Msg::State { id: 1, channel: 3, keep: true, key, data: alloc::vec![9; MAX_STATE + 5_000] };
        let bytes = m.encode();
        assert!(bytes.len() - 2 <= MAX_BODY, "{}", bytes.len());
        let mut i = Inbox::default();
        i.push(&bytes);
        match i.next() {
            Next::Msg(Msg::State { data, .. }) => assert_eq!(data.len(), MAX_STATE),
            other => panic!("{other:?}"),
        }
        // A skin piece fits too, and one that is too long for its message is cut.
        let mut i = Inbox::default();
        i.push(&Msg::SkinPart { id: 1, last: false, data: alloc::vec![1; MAX_SKIN_PART + 500] }.encode());
        match i.next() {
            Next::Msg(Msg::SkinPart { data, .. }) => assert_eq!(data.len(), MAX_SKIN_PART),
            other => panic!("{other:?}"),
        }
        // A pack piece and a mod piece fit too.
        for m in [Msg::Pack { last: false, data: alloc::vec![1; MAX_PACK_PART * 2] }, Msg::ModPart { index: 1, last: false, data: alloc::vec![1; MAX_PACK_PART * 2] }] {
            let mut i = Inbox::default();
            i.push(&m.encode());
            assert!(matches!(i.next(), Next::Msg(_)));
        }
    }

    #[test]
    fn a_list_of_too_many_effects_is_refused_and_a_long_one_is_cut() {
        // Sending more than the limit sends the first ones only.
        let many: Vec<(String, u8, u32)> = (0..40).map(|i| (alloc::format!("e{i}"), 1, 5)).collect();
        let mut inbox = Inbox::default();
        inbox.push(&Msg::Effects { id: 1, list: many.clone() }.encode());
        assert_eq!(inbox.next(), Next::Msg(Msg::Effects { id: 1, list: many[..MAX_EFFECTS].to_vec() }));
        // A message that claims more than the limit is junk.
        let mut body = alloc::vec![21u8, 1, 0, (MAX_EFFECTS + 1) as u8];
        for _ in 0..=MAX_EFFECTS {
            body.extend_from_slice(&[1, b'a', 1, 5, 0, 0, 0]);
        }
        let mut bytes = (body.len() as u16).to_le_bytes().to_vec();
        bytes.extend_from_slice(&body);
        let mut i = Inbox::default();
        i.push(&bytes);
        assert_eq!(i.next(), Next::Bad);
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
        // A position no block could be at is not this protocol either.
        let mut far = Msg::Move { id: 1, pos: [0.0; 3], yaw: 0.0, pitch: 0.0, held: 0, pose: 0 }.encode();
        far[5..13].copy_from_slice(&1.0e300f64.to_le_bytes());
        let mut i = Inbox::default();
        i.push(&far);
        assert_eq!(i.next(), Next::Bad);
        let mut nan = Msg::Time { time: 0.0 }.encode();
        nan[3..7].copy_from_slice(&f32::NAN.to_le_bytes());
        let mut i = Inbox::default();
        i.push(&nan);
        assert_eq!(i.next(), Next::Bad);
        // A voice frame longer than allowed is not this protocol.
        let mut big = Msg::Voice { id: 1, seq: 1, gain: 1, frame: alloc::vec![1; 10] }.encode();
        big[8] = 201;
        let mut i = Inbox::default();
        i.push(&big);
        assert_eq!(i.next(), Next::Bad);
        // Names are cleaned of anything that is not printable ASCII.
        let mut hello = Msg::Join { id: 1, name: String::from("a\u{7}b") }.encode();
        hello[6] = 200;
        let mut i = Inbox::default();
        i.push(&hello);
        assert_eq!(i.next(), Next::Msg(Msg::Join { id: 1, name: String::from("b") }));
    }
}
