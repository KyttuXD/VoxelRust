//! The multiplayer server. It runs inside the game program
//! (`voxelrust --server`), so anything that can run the game can host,
//! and it needs no window or graphics.
//!
//! The server owns the list of block changes and relays positions, edits
//! and chat. Terrain itself is never sent: every client generates it from
//! the seed, then applies the changes.
//!
//! It also answers one extra request, used by machines too old to make
//! HTTPS connections themselves: `TILE <elev|map> <z> <x> <y>` on a fresh
//! connection returns that map tile.

use crate::chunk::CH;
use crate::proto::{Inbox, Msg, Next, VERSION};
use crate::sys::net::{NetError, TcpListener, TcpStream};
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// The server's half of accounts (PIN logins, the administrators' commands): a child module
/// so that it reaches the fields of the server and its connections.
#[path = "accounts/server_side.rs"]
mod accounts_flow;
use self::accounts_flow::Gate;
/// The server's half of player versus player (see `net/pvp`).
#[path = "pvp/server_side.rs"]
mod pvp_flow;

/// The server's half of the sync of shared games (health, pings, who runs the creatures, a
/// guest who comes back too soon): also a child module, for the same reason.
#[path = "sync/server_side.rs"]
mod sync_flow;

pub struct Config {
    pub port: u16,
    pub seed: u32,
    /// World kind as text (`normal`, `flat`, or what a mod's terrain calls itself).
    pub kind: String,
    pub max_players: usize,
    /// Checksum of block state names that clients must match.
    pub content: u32,
    /// Number of block states, for validating edits.
    pub states: u16,
    /// The server's content packs and the files they use (see
    /// [`crate::save::bundle`]), sent to every
    /// player so that all of them use the same blocks and items.
    pub pack: Vec<u8>,
    /// The mods players may download (see [`crate::net::modsync::serve`]).
    pub mods: Vec<crate::net::modsync::serve::ServedMod>,
}

struct Conn {
    stream: TcpStream,
    inbox: Inbox,
    id: u16,
    name: String,
    joined: bool,
    /// Where this player last said they were, for telling newcomers.
    last_move: Option<([f64; 3], f32, f32, u16, u8)>,
    /// This player's skin as the server keeps it (see `net::skinslot`).
    skin: crate::net::skinslot::SkinSlot,
    /// Skins of other players waiting to be sent to this one, a few pieces a tick.
    skin_out: crate::net::skinqueue::SkinQueue,
    /// The protocol version their game said it speaks.
    proto: u16,
    /// What this player's voice chat is set to (`VOICE_` flags); 0 until they say.
    voice_flags: u8,
    voice_bucket: crate::net_voice::Bucket,
    /// Ticks since anything arrived; idle strangers are dropped.
    quiet: u32,
    /// Seconds since anything arrived; players who say nothing for a
    /// minute (a client sends its position every couple of seconds) are
    /// dropped, so a dead connection does not stay a player for ever.
    idle: f32,
    /// What this connection may still send (see `net::limits`).
    throttle: crate::net::limits::Throttle,
    gone: bool,
    /// Mod bundles waiting to be sent: (place in the server's list, bytes
    /// already sent), a few pieces per tick. (Only a place is kept: the
    /// pieces are cut when sent, so a player asking for every mod does not
    /// cost the server a second copy of them all.)
    mod_out: alloc::collections::VecDeque<(usize, usize)>,
    /// This player has already asked for mods.
    mods_asked: bool,
    /// The status effects they last said they have, for telling newcomers.
    effects: Vec<(String, u8, u32)>,
    /// Which protocol they speak, whether they are logged in to an account, and the login
    /// challenge they owe an answer to (see `net/accounts`).
    acct: crate::net::accounts::state::ConnAuth,
    /// Their health, death, emote, ping and place in the order of arrival (see `net/sync`).
    sync: crate::net::sync::state::ConnSync,
    /// Player versus player: their gear, last hit, spawn protection (see `net/pvp`).
    pvp: crate::net::pvp::state::ConnPvp,
}

pub struct Server {
    listener: TcpListener,
    conns: Vec<Conn>,
    /// Every block changed by players: position -> state.
    pub edits: BTreeMap<(i32, i32, i32), u16>,
    pub cfg: Config,
    next_id: u16,
    pub time: f32,
    time_timer: f32,
    /// Edits have changed since the last save.
    pub dirty: bool,
    /// The latest of each piece of kept state, for players who join later.
    kept: BTreeMap<(u8, String), Vec<u8>>,
    /// Bytes of data in `kept`.
    kept_bytes: usize,
    /// Names of the operators: players who may use the server's commands.
    /// The first player ever to join becomes one.
    pub ops: Vec<String>,
    /// Names that may not join.
    pub banned: Vec<String>,
    /// `ops` or `banned` changed since they were last written out.
    pub ops_dirty: bool,
    /// Where each player was when they left (by lower-case name): x, y, z, yaw, pitch.
    /// They come back to it (sent right after the welcome).
    pub spots: BTreeMap<String, [f64; 5]>,
    pub spots_dirty: bool,
    /// What players may do who have no entry of their own.
    pub default_perms: u8,
    /// Players with permissions of their own (lower-case name, `PERM_` flags).
    pub perms: Vec<(String, u8)>,
    /// Players in creative mode (lower-case names); everyone else is in survival.
    pub creative: Vec<String>,
    /// Voice chat: who is muted and how far a voice carries (see [`crate::net_voice`]).
    pub voice: crate::net_voice::Rules,
    /// Accounts: the settings, the accounts, the limits on logins (see `net/accounts`).
    pub acct: crate::net::accounts::state::AccountState,
    /// The chat history, remembered health and the like (see `net/sync`).
    sync: crate::net::sync::state::ServerSync,
    /// Player versus player: the switches, the clock and the kills (see `net/pvp`).
    pub pvp: crate::net::pvp::state::Pvp,
}

const SEND_TIMEOUT: u32 = 3000;
/// Mark on a permission line of the permissions file: written since fighting between players had a flag.
const FILE_HAS_PVP: u8 = 64;

/// Most block changes the server keeps (a hostile player may not grow the
/// list, and the memory behind it, for ever; changes to blocks already in it
/// are still taken).
pub const MAX_EDITS: usize = 600_000;
/// Most bytes of kept shared state (chests and the like) in all.
pub const MAX_KEPT_BYTES: usize = 8 << 20;
/// Connections that have not said hello yet, at most.
const MAX_STRANGERS: usize = 24;
/// Seconds without a word from a player before they are dropped.
const IDLE_SECONDS: f32 = 60.0;

impl Server {
    pub fn bind(cfg: Config) -> Result<Server, NetError> {
        let listener = TcpListener::bind(cfg.port)?;
        Ok(Server { listener, conns: Vec::new(), edits: BTreeMap::new(), cfg, next_id: 1, time: 0.12, time_timer: 0.0, dirty: false,
            kept: BTreeMap::new(), kept_bytes: 0, ops: Vec::new(), banned: Vec::new(), ops_dirty: false, spots: BTreeMap::new(), spots_dirty: false,
            default_perms: crate::proto::PERM_BUILD | crate::proto::PERM_VOICE | crate::proto::PERM_PVP, perms: Vec::new(), creative: Vec::new(), voice: Default::default(), acct: Default::default(), pvp: Default::default(), sync: Default::default() })
    }

    /// A player number no one here has: never 0 (the server) or 65535 (what
    /// "everyone" is told with), and not one still in use after the counter
    /// has gone all the way round.
    fn fresh_id(&mut self) -> u16 {
        loop {
            let id = self.next_id;
            self.next_id = if id >= u16::MAX - 1 { 1 } else { id + 1 };
            if id != 0 && id != u16::MAX && !self.conns.iter().any(|c| c.id == id && c.joined) {
                return id;
            }
        }
    }

    /// Remember (or, with no data, forget) a piece of shared state for players
    /// who join later, within [`MAX_KEPT_BYTES`] and 8192 pieces.
    fn keep_state(&mut self, channel: u8, key: &str, data: &[u8]) {
        let k = (channel, String::from(key));
        let old = self.kept.get(&k).map_or(0, |d| d.len());
        if data.is_empty() {
            if self.kept.remove(&k).is_some() {
                self.kept_bytes -= old;
            }
        } else if (old > 0 || self.kept.len() < 8192) && self.kept_bytes - old + data.len() <= MAX_KEPT_BYTES {
            self.kept_bytes = self.kept_bytes - old + data.len();
            self.kept.insert(k, data.to_vec());
        }
    }

    /// Whether the connection `i` is the player who runs the creatures for
    /// everybody: the one who came first of those here (the clients are told
    /// who that is, see `net/sync`).
    fn runs_the_world(&self, i: usize) -> bool {
        let me = self.conns[i].sync.order;
        self.conns.iter().filter(|c| c.joined && !c.gone).all(|c| c.sync.order >= me)
    }

    pub fn port(&self) -> u16 {
        self.listener.local_port()
    }

    pub fn players(&self) -> usize {
        self.conns.iter().filter(|c| c.joined && !c.gone).count()
    }

    /// What `who` may do, and whether they are in creative mode: `PERM_` flags.
    /// Operators may do everything.
    pub fn flags_for(&self, who: &str) -> u8 {
        use crate::proto::*;
        let lower = who.to_ascii_lowercase();
        let mut f = if self.ops.iter().any(|o| o.eq_ignore_ascii_case(who)) || self.is_admin(who) {
            PERM_BUILD | PERM_GAMEMODE | PERM_COMMANDS | PERM_VOICE | PERM_PVP
        } else {
            self.perms.iter().find(|p| p.0 == lower).map_or(self.default_perms, |p| p.1)
        };
        if self.creative.contains(&lower) {
            f |= PERM_CREATIVE;
        }
        // Fighting is off for everybody when the server says so.
        if !self.pvp.rules.on {
            f &= !PERM_PVP;
        }
        f
    }

    /// Tell every player (or just connection `only`) what they may do.
    fn send_perms(&mut self, only: Option<usize>) {
        for k in 0..self.conns.len() {
            if only.map_or(false, |o| o != k) || !self.conns[k].joined || self.conns[k].gone {
                continue;
            }
            let bytes = Msg::Perms { flags: self.flags_for(&self.conns[k].name) }.encode();
            if self.conns[k].stream.send_all(&bytes, SEND_TIMEOUT).is_err() {
                self.conns[k].gone = true;
            }
        }
    }

    /// The permission lines kept with the operators.
    pub fn perm_text(&self) -> String {
        // Flag 128 marks lines written since voice chat existed: older files
        // have no voice bit, and voice is allowed unless a line says otherwise.
        let mut t = alloc::format!("default {}\n", self.default_perms | 128 | FILE_HAS_PVP);
        for (n, f) in &self.perms {
            t.push_str(&alloc::format!("perm {} {}\n", f | 128 | FILE_HAS_PVP, n));
        }
        for n in &self.creative {
            t.push_str(&alloc::format!("creative {}\n", n));
        }
        t.push_str(&self.pvp_text());
        t.push_str(&self.voice.text());
        t
    }

    /// Flags as written to the permissions file: files from before voice chat
    /// (no 128 mark) allow voice.
    fn read_flags(f: u8) -> u8 {
        let kept = f & (crate::proto::PERM_BUILD | crate::proto::PERM_GAMEMODE | crate::proto::PERM_COMMANDS | crate::proto::PERM_VOICE | crate::proto::PERM_PVP);
        // (Files from before player versus player have no 64 mark: fighting is allowed.)
        let kept = if f & FILE_HAS_PVP == 0 { kept | crate::proto::PERM_PVP } else { kept };
        if f & 128 == 0 { kept | crate::proto::PERM_VOICE } else { kept }
    }

    /// Read one line written by `perm_text`; false if it is not one.
    pub fn load_perm_line(&mut self, line: &str) -> bool {
        match line.split_once(' ') {
            Some(("default", f)) => self.default_perms = Self::read_flags(f.trim().parse().unwrap_or(crate::proto::PERM_BUILD)),
            Some(("perm", rest)) => {
                if let Some((f, n)) = rest.split_once(' ') {
                    self.perms.push((n.to_ascii_lowercase(), Self::read_flags(f.parse::<u8>().unwrap_or(0))));
                }
            }
            Some(("creative", n)) => self.creative.push(n.to_ascii_lowercase()),
            Some(("pvp", v)) => return self.pvp_load_line(v),
            _ => return self.voice.load_line(line),
        }
        true
    }

    /// Apply `+build -commands`-style words to `flags`. None if a word is not one.
    fn change_flags(mut flags: u8, words: &[&str]) -> Option<u8> {
        use crate::proto::*;
        for w in words {
            let (on, name) = match (w.strip_prefix('+'), w.strip_prefix('-')) {
                (Some(n), _) => (true, n),
                (_, Some(n)) => (false, n),
                _ => return None,
            };
            let bit = match name {
                "build" => PERM_BUILD,
                "gamemode" => PERM_GAMEMODE,
                "commands" => PERM_COMMANDS,
                "voice" => PERM_VOICE,
                "pvp" => PERM_PVP,
                _ => return None,
            };
            flags = if on { flags | bit } else { flags & !bit };
        }
        Some(flags)
    }

    fn describe(flags: u8) -> String {
        use crate::proto::*;
        let mut v: Vec<&str> = Vec::new();
        for (bit, n) in [(PERM_BUILD, "build"), (PERM_GAMEMODE, "gamemode"), (PERM_COMMANDS, "commands"), (PERM_VOICE, "voice"), (PERM_PVP, "pvp")] {
            if flags & bit != 0 {
                v.push(n);
            }
        }
        if v.is_empty() { String::from("nothing") } else { v.join(" ") }
    }

    /// A command typed by the player `who`. Anyone may ask /list and
    /// /help; the rest are for operators. Returns what to tell them.
    pub fn command(&mut self, who: &str, line: &str) -> String {
        // /gm c, /gmc and the like (older games send them as typed).
        let first = line.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        let spelled;
        let line = if matches!(first.as_str(), "/gm" | "/gmc" | "/gms" | "/gamemode") {
            spelled = crate::game::aliases::expand(&line.to_ascii_lowercase(), true);
            spelled.as_str()
        } else {
            line
        };
        let w: Vec<&str> = line.split_whitespace().collect();
        let arg = |k: usize| w.get(k).copied().unwrap_or("");
        let is_op = self.ops.iter().any(|o| o.eq_ignore_ascii_case(who)) || self.is_admin(who);
        let online = |s: &Server, name: &str| s.conns.iter().position(|c| c.joined && !c.gone && c.name.eq_ignore_ascii_case(name));
        match arg(0).to_ascii_lowercase().as_str() {
            "/version" => {
                let me = crate::net::version::local();
                alloc::format!("server: {} protocol {}", crate::net::version::describe(&me), VERSION)
            }
            "/list" => {
                let names: Vec<String> = self.conns.iter().filter(|c| c.joined && !c.gone).map(|c| if self.ops.iter().any(|o| o.eq_ignore_ascii_case(&c.name)) || self.is_admin(&c.name) { alloc::format!("{}*", c.name) } else { c.name.clone() }).collect();
                alloc::format!("online ({}): {}   (* = operator or administrator)", names.len(), names.join(", "))
            }
            "/help" if arg(1) == "accounts" => String::from("accounts: /register PIN PIN (protect your name; 4 or 6 digits)  /pin OLD NEW  /forgetpin (the game, not the server).  administrators: /resetpin NAME [PIN]  /accounts"),
            "/pvp" | "/kills" => self.pvp_command(who, &w, is_op).unwrap_or_default(),
            "/register" | "/pin" | "/login" => self.plain_account_command(),
            "/resetpin" | "/accounts" => self.admin_command(who, &w),
            "/help" => String::from("server: /list /version /gamemode creative|survival (if allowed)   operators: /op /deop /kick /ban /pardon NAME  /say TEXT  /time 0..1  /perm NAME [+build -gamemode +commands +voice +pvp]  /default [+build ...]  /mute NAME  /unmute NAME  /voicerange BLOCKS  /gamemode NAME creative|survival  /pvp on|off.  anyone: /pvp  /kills [NAME]"),
            "/gamemode" if arg(2).is_empty() || !is_op => {
                // Your own game mode, if you may choose it.
                let mode = if arg(2).is_empty() { arg(1) } else { arg(2) };
                if !is_op && self.flags_for(who) & crate::proto::PERM_GAMEMODE == 0 {
                    return String::from("you may not change your game mode here");
                }
                if arg(2).is_empty() {
                    self.set_creative(who, mode)
                } else {
                    String::from("only operators may change someone else's game mode")
                }
            }
            _ if !is_op => String::from("only operators may do that"),
            "/gamemode" => match online(self, arg(1)) {
                Some(k) => {
                    let name = self.conns[k].name.clone();
                    self.set_creative(&name, arg(2))
                }
                None => String::from("nobody of that name is here"),
            },
            "/perm" if !arg(1).is_empty() => {
                let lower = arg(1).to_ascii_lowercase();
                let now = self.perms.iter().find(|p| p.0 == lower).map_or(self.default_perms, |p| p.1);
                if w.len() == 2 {
                    return alloc::format!("{} may: {}", arg(1), Self::describe(now));
                }
                match Self::change_flags(now, &w[2..]) {
                    Some(f) => {
                        self.perms.retain(|p| p.0 != lower);
                        self.perms.push((lower, f));
                        self.ops_dirty = true;
                        self.send_perms(None);
                        alloc::format!("{} may now: {}", arg(1), Self::describe(f))
                    }
                    None => String::from("use /perm NAME +build -build +gamemode -gamemode +commands -commands +voice -voice +pvp -pvp"),
                }
            }
            "/mute" | "/unmute" if !arg(1).is_empty() => {
                let mute = arg(0).eq_ignore_ascii_case("/mute");
                let changed = self.voice.set_muted(arg(1), mute);
                self.ops_dirty |= changed;
                match (mute, changed) {
                    (true, true) => alloc::format!("{} may not talk on voice chat", arg(1)),
                    (true, false) => alloc::format!("{} is already muted", arg(1)),
                    (false, true) => alloc::format!("{} may talk on voice chat again", arg(1)),
                    (false, false) => alloc::format!("{} was not muted", arg(1)),
                }
            }
            "/voicerange" => {
                if w.len() == 1 {
                    return alloc::format!("voices carry {} blocks", self.voice.range);
                }
                if self.voice.set_range(arg(1)) {
                    self.ops_dirty = true;
                    alloc::format!("voices now carry {} blocks", self.voice.range)
                } else {
                    alloc::format!("use /voicerange with a number of blocks from 2 to {}", crate::net_voice::MAX_RANGE)
                }
            }
            "/default" => {
                if w.len() == 1 {
                    return alloc::format!("players may by default: {}", Self::describe(self.default_perms));
                }
                match Self::change_flags(self.default_perms, &w[1..]) {
                    Some(f) => {
                        self.default_perms = f;
                        self.ops_dirty = true;
                        self.send_perms(None);
                        alloc::format!("players may now by default: {}", Self::describe(f))
                    }
                    None => String::from("use /default +build -build +gamemode -gamemode +commands -commands +voice -voice +pvp -pvp"),
                }
            }
            "/op" if !arg(1).is_empty() => {
                if !self.ops.iter().any(|o| o.eq_ignore_ascii_case(arg(1))) {
                    self.ops.push(String::from(arg(1)));
                    self.ops_dirty = true;
                }
                alloc::format!("{} is an operator", arg(1))
            }
            "/deop" if !arg(1).is_empty() => {
                // The last operator cannot be removed: someone must hold the keys.
                if self.ops.len() <= 1 && self.ops.iter().any(|o| o.eq_ignore_ascii_case(arg(1))) {
                    return String::from("that is the only operator");
                }
                self.ops.retain(|o| !o.eq_ignore_ascii_case(arg(1)));
                self.ops_dirty = true;
                alloc::format!("{} is no longer an operator", arg(1))
            }
            "/kick" | "/ban" => match online(self, arg(1)) {
                Some(k) => {
                    let bytes = Msg::Refuse { reason: String::from("an operator removed you from the server") }.encode();
                    let _ = self.conns[k].stream.send_all(&bytes, SEND_TIMEOUT);
                    self.conns[k].gone = true;
                    if arg(0).eq_ignore_ascii_case("/ban") {
                        self.banned.push(String::from(arg(1)));
                        self.ops_dirty = true;
                    }
                    alloc::format!("{} was removed", arg(1))
                }
                None if arg(0).eq_ignore_ascii_case("/ban") && !arg(1).is_empty() => {
                    self.banned.push(String::from(arg(1)));
                    self.ops_dirty = true;
                    alloc::format!("{} may not join", arg(1))
                }
                None => String::from("nobody of that name is here"),
            },
            "/pardon" => {
                self.banned.retain(|b| !b.eq_ignore_ascii_case(arg(1)));
                self.ops_dirty = true;
                alloc::format!("{} may join again", arg(1))
            }
            "/say" => {
                let text = alloc::format!("[server] {}", w[1.min(w.len())..].join(" "));
                self.broadcast(&Msg::Chat { id: 0, text: text.clone() }, u16::MAX);
                String::new()
            }
            "/time" => match arg(1).parse::<f32>() {
                Ok(t) if (0.0..1.0).contains(&t) => {
                    self.time = t;
                    self.broadcast(&Msg::Time { time: t }, u16::MAX);
                    String::from("time set")
                }
                _ => String::from("use /time with a number from 0 to 1 (0.25 is noon)"),
            },
            _ => String::from("no such server command; try /help"),
        }
    }

    fn set_creative(&mut self, name: &str, mode: &str) -> String {
        let lower = name.to_ascii_lowercase();
        match mode {
            "creative" => {
                if !self.creative.contains(&lower) {
                    self.creative.push(lower);
                }
            }
            "survival" => self.creative.retain(|c| *c != lower),
            _ => return String::from("use /gamemode creative or /gamemode survival"),
        }
        self.ops_dirty = true;
        self.send_perms(None);
        alloc::format!("{} is now in {} mode", name, mode)
    }

    /// Say something to every player, as the server (a chat line from nobody).
    pub fn announce(&mut self, text: &str) {
        self.broadcast(&Msg::Chat { id: 0, text: String::from(text) }, u16::MAX);
    }

    /// Send a message to one player only (nothing if they are not here).
    fn send_to(&mut self, to: u16, msg: &Msg) {
        let bytes = msg.encode();
        for c in self.conns.iter_mut().filter(|c| c.joined && !c.gone && c.id == to) {
            if c.stream.send_all(&bytes, SEND_TIMEOUT).is_err() {
                c.gone = true;
            }
        }
    }

    /// Pass on the skin of the player at `from` to everyone else, the big
    /// original to clients that take it and the 64 x 64 one to the others.
    fn relay_skin(&mut self, from: usize) {
        let id = self.conns[from].id;
        let new: crate::net::skinqueue::Batch = alloc::sync::Arc::new(self.conns[from].skin.messages_for(id, crate::net::skinslot::HD_PROTOCOL));
        let old: crate::net::skinqueue::Batch = alloc::sync::Arc::new(self.conns[from].skin.messages_for(id, 0));
        // The pieces are not sent here: they wait in each connection's queue and
        // go a few to a tick (a big skin must not hold up the server).
        for (k, c) in self.conns.iter_mut().enumerate() {
            if k == from || !c.joined || c.gone {
                continue;
            }
            let batch = if c.proto >= crate::net::skinslot::HD_PROTOCOL { &new } else { &old };
            if !c.skin_out.push(batch.clone()) {
                // Far behind with skins already: let it go rather than grow without end.
                c.gone = true;
            }
        }
    }

    fn broadcast(&mut self, msg: &Msg, except: u16) {
        let bytes = msg.encode();
        for c in self.conns.iter_mut().filter(|c| c.joined && !c.gone && c.id != except) {
            if c.stream.send_all(&bytes, SEND_TIMEOUT).is_err() {
                c.gone = true;
            }
        }
    }

    /// A talker's voice frame: pass it, with a loudness worked out from the
    /// distance, to every player in range who has voice on. Frames from
    /// players who may not talk (no permission, muted, mic off, too many)
    /// are dropped without a word. Nothing is kept.
    fn relay_voice(&mut self, from: usize, seq: u16, frame: &[u8]) {
        let talker = &mut self.conns[from];
        if talker.voice_flags & crate::proto::VOICE_TALK == 0 || !talker.voice_bucket.allow() || !crate::net_voice::frame_ok(frame) {
            return;
        }
        let (id, name, at) = (talker.id, talker.name.clone(), talker.last_move.map(|m| m.0));
        let Some(at) = at else { return };
        if self.voice.is_muted(&name) || self.flags_for(&name) & crate::proto::PERM_VOICE == 0 {
            return;
        }
        let listeners: Vec<Option<([f64; 3], bool)>> = self
            .conns
            .iter()
            .enumerate()
            .map(|(k, c)| {
                if k == from || !c.joined || c.gone {
                    return None;
                }
                c.last_move.map(|m| (m.0, c.voice_flags & crate::proto::VOICE_HEAR != 0))
            })
            .collect();
        let gains = crate::net_voice::audience(at, &listeners, self.voice.range);
        for (k, g) in gains.into_iter().enumerate() {
            if let Some(gain) = g {
                let bytes = Msg::Voice { id, seq, gain, frame: frame.to_vec() }.encode();
                // A short wait: a listener who cannot keep up is dropped like any other.
                if self.conns[k].stream.send_all(&bytes, 500).is_err() {
                    self.conns[k].gone = true;
                }
            }
        }
    }

    /// Do one round of work: accept, read, relay. `tile` fetches a map tile
    /// for a relay request line. Returns lines for the server's log.
    pub fn tick(&mut self, dt: f32, tile: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Vec<String> {
        let mut log = Vec::new();
        self.pvp.now += dt as f64;
        self.time += dt / 1200.0;
        if self.time >= 1.0 {
            self.time -= 1.0;
        }
        self.time_timer += dt;
        if self.time_timer > 5.0 {
            self.time_timer = 0.0;
            let t = self.time;
            self.broadcast(&Msg::Time { time: t }, 0);
        }
        for c in self.conns.iter_mut() {
            c.voice_bucket.refill(dt);
            c.throttle.refill(dt);
            c.skin.tick(dt);
        }
        self.account_tick(dt);
        self.sync_tick(dt);
        for _ in 0..8 {
            match self.listener.accept(0) {
                Ok(Some(mut stream)) => {
                    // Too many strangers at the door (or in all): the newcomer is
                    // let go at once (dropping the stream closes it), so a flood of
                    // silent connections cannot use up the machine's sockets and memory.
                    let strangers = self.conns.iter().filter(|c| !c.joined && !c.gone).count();
                    if strangers >= MAX_STRANGERS || self.conns.len() >= self.cfg.max_players.saturating_add(MAX_STRANGERS) {
                        continue;
                    }
                    stream.set_nodelay(true);
                    self.conns.push(Conn { stream, inbox: Inbox::default(), id: 0, name: String::new(), joined: false, last_move: None, skin: crate::net::skinslot::SkinSlot::deferred(), skin_out: Default::default(), proto: 0, voice_flags: 0, voice_bucket: Default::default(), quiet: 0, idle: 0.0, throttle: Default::default(), gone: false, mod_out: Default::default(), mods_asked: false, effects: Vec::new(), acct: Default::default(), sync: Default::default(), pvp: Default::default() });
                }
                _ => break,
            }
        }
        let mut relays = 0;
        let mut buf = [0u8; 4096];
        for i in 0..self.conns.len() {
            // Read whatever has arrived.
            let mut got_any = false;
            for _ in 0..16 {
                match self.conns[i].stream.recv(&mut buf, 0) {
                    Ok(0) => break,
                    Ok(n) => {
                        self.conns[i].inbox.push(&buf[..n]);
                        got_any = true;
                    }
                    Err(_) => {
                        self.conns[i].gone = true;
                        break;
                    }
                }
            }
            let c = &mut self.conns[i];
            c.quiet = if got_any { 0 } else { c.quiet + 1 };
            c.idle = if got_any { 0.0 } else { c.idle + dt };
            if c.idle > IDLE_SECONDS || (!c.joined && c.quiet > 4000 && c.acct.pending.is_none()) {
                c.gone = true; // said nothing for ages (or never said hello)
                continue;
            }
            if !c.joined {
                // A tile relay request is a plain text line, not a message.
                let peek = c.inbox.peek();
                if peek.starts_with(b"TILE ") || peek.starts_with(b"GEO ") {
                    // A few a tick: answering one may mean fetching the tile.
                    if relays >= 2 && peek.contains(&b'\n') {
                        continue;
                    }
                    if let Some(end) = peek.iter().position(|&b| b == b'\n').filter(|&e| e <= 64) {
                        relays += 1;
                        let line = String::from_utf8_lossy(&peek[..end]).into_owned();
                        let data = tile(&line).unwrap_or_default();
                        let _ = c.stream.send_all(&(data.len() as u32).to_le_bytes(), SEND_TIMEOUT);
                        let _ = c.stream.send_all(&data, 20_000);
                        log.push(alloc::format!("relay: {} -> {} bytes", line, data.len()));
                        c.gone = true;
                    } else if peek.len() > 64 || peek.contains(&b'\n') {
                        c.gone = true;
                    }
                    continue;
                }
            }
            loop {
                let msg = match self.conns[i].inbox.next() {
                    Next::Msg(m) => m,
                    Next::Wait => break,
                    Next::Bad => {
                        self.conns[i].gone = true;
                        break;
                    }
                };
                if self.conns[i].gone {
                    break;
                }
                let (id, joined) = (self.conns[i].id, self.conns[i].joined);
                // Each kind of message has an allowance; what is over it is
                // dropped, and a connection that keeps sending far too much is cut.
                if joined {
                    use crate::net::limits::Class;
                    let class = match &msg {
                        Msg::Move { .. } | Msg::Emote { .. } | Msg::Effects { .. } | Msg::VoiceState { .. } => Some(Class::Motion),
                        Msg::SetBlock { .. } => Some(Class::Block),
                        Msg::Chat { .. } => Some(Class::Chat),
                        Msg::State { channel: crate::proto::channel::MODMSG, .. } => Some(Class::Mod),
                        Msg::State { .. } => Some(Class::State),
                        Msg::Skin { .. } | Msg::SkinBegin { .. } => Some(Class::Skin),
                        Msg::SkinPart { .. } => Some(Class::SkinPart),
                        _ => None,
                    };
                    if let Some(class) = class {
                        if !self.conns[i].throttle.allow(class) {
                            if self.conns[i].throttle.abusive() {
                                log.push(alloc::format!("{} was sending far too much and was dropped", self.conns[i].name));
                                self.conns[i].gone = true;
                                break;
                            }
                            continue;
                        }
                    }
                }
                match msg {
                    Msg::Hello { version, content, name } if !joined && self.conns[i].acct.pending.is_none() => {
                        // Content need not match: the server sends its own.
                        let _ = content;
                        // The name as the server will know it: bans and the like are
                        // checked against this, not against what was typed.
                        let mut name = crate::settings::clean_name(&name, 16);
                        if name.is_empty() {
                            name = String::from("Player");
                        }
                        let refuse = if !crate::net::accounts::version_accepted(version) {
                            // Both versions, and who must update (the build name does not matter).
                            Some(crate::net::version::mismatch_reason(version, VERSION, &crate::net::version::local()))
                        } else if self.banned.iter().any(|b| b.eq_ignore_ascii_case(&name)) {
                            Some(String::from("you may not join this server"))
                        } else if self.players() >= self.cfg.max_players && !self.name_is_here(&name) {
                            // (A player of the same name may be their own old connection, which
                            // makes way when they are let in: `admit` looks again.)
                            Some(String::from("the server is full"))
                        } else {
                            // A name with an account is logged in to first (see net/accounts).
                            match self.hello_gate(i, version, &name) {
                                Ok(Gate::Wait) => continue,
                                Ok(Gate::Guest) => None,
                                Err(reason) => Some(reason),
                            }
                        };
                        if let Some(reason) = refuse {
                            let _ = self.conns[i].stream.send_all(&Msg::Refuse { reason }.encode(), SEND_TIMEOUT);
                            self.conns[i].gone = true;
                            break;
                        }
                        if !self.admit(i, name, &mut log) {
                            break;
                        }
                    }
                    // A login, a registration or a PIN change (see net/accounts).
                    Msg::AuthLogin { .. } | Msg::AuthRegister { .. } | Msg::AuthChange { .. } | Msg::AuthAsk { .. } => {
                        if !self.auth_message(i, msg, &mut log) {
                            break;
                        }
                    }
                    Msg::Move { pos, yaw, pitch, held, pose, .. } if joined => {
                        // An emote ends when they walk off it.
                        if let Some((old, ..)) = self.conns[i].last_move {
                            let (dx, dz) = (old[0] - pos[0], old[2] - pos[2]);
                            if dx * dx + dz * dz > 0.25 {
                                self.conns[i].sync.emote = 0;
                            }
                        }
                        self.conns[i].last_move = Some((pos, yaw, pitch, held, pose));
                        self.broadcast(&Msg::Move { id, pos, yaw, pitch, held, pose }, id);
                    }
                    // An emote the player plays (or stops): everyone else sees it.
                    Msg::Emote { emote, .. } if joined => {
                        self.conns[i].sync.emote = emote;
                        self.broadcast(&Msg::Emote { id, emote }, id);
                    }
                    // Their status effects: kept for newcomers and shown to everyone else.
                    Msg::Effects { list, .. } if joined => {
                        let list = crate::game::effects::sync::clean(list);
                        self.conns[i].effects = list.clone();
                        self.broadcast(&Msg::Effects { id, list }, id);
                    }
                    Msg::SetBlock { x, y, z, block } if joined => {
                        let may = self.flags_for(&self.conns[i].name) & crate::proto::PERM_BUILD != 0;
                        // (New places stop being taken once the list is full.)
                        let room = self.edits.len() < MAX_EDITS || self.edits.contains_key(&(x, y, z));
                        if may && room && (0..CH).contains(&y) && block < self.cfg.states {
                            self.edits.insert((x, y, z), block);
                            self.dirty = true;
                            self.broadcast(&Msg::SetBlock { x, y, z, block }, id);
                            // Two players changing one block at once would each be left with the
                            // other's: the one who came last (the server's copy) is told to both.
                            if self.sync.block_conflict((x, y, z), id) {
                                self.send_to(id, &Msg::SetBlock { x, y, z, block });
                            }
                        }
                    }
                    Msg::ModsWant { names } if joined && !self.conns[i].mods_asked => {
                        // Queue the bundles asked for, in the order asked: a few
                        // pieces are sent each tick so play goes on meanwhile.
                        self.conns[i].mods_asked = true;
                        let mut queued = alloc::collections::VecDeque::new();
                        let mut sent: Vec<&str> = Vec::new();
                        for want in &names {
                            let found = self.cfg.mods.iter().position(|m| m.name == *want);
                            if let (Some(index), false) = (found, sent.contains(&want.as_str())) {
                                sent.push(want);
                                queued.push_back((index, 0usize));
                            }
                        }
                        if !queued.is_empty() {
                            log.push(alloc::format!("{} downloads {} mod{}", self.conns[i].name, sent.len(), if sent.len() == 1 { "" } else { "s" }));
                        }
                        self.conns[i].mod_out = queued;
                    }
                    Msg::VoiceState { flags } if joined => {
                        self.conns[i].voice_flags = flags & (crate::proto::VOICE_HEAR | crate::proto::VOICE_TALK);
                    }
                    Msg::Voice { seq, frame, .. } if joined => self.relay_voice(i, seq, &frame),
                    Msg::State { channel, keep, key, data, .. } if joined => {
                        use crate::proto::channel as ch;
                        if channel == ch::VITALS {
                            // Health, a death, the answer to a ping (see net/sync).
                            self.on_vitals(i, &key, &data, &mut log);
                            continue;
                        }
                        if channel == ch::MODMSG {
                            // A mod's message: only a sound one is passed on, never kept, to the
                            // one player it is for (not back to the sender) or to all the others.
                            let to = crate::modmsg::unpack(&data).map(|(to, _)| to).filter(|_| crate::modmsg::split_key(&key).is_some());
                            if let Some(to) = to {
                                let msg = Msg::State { id, channel, keep: false, key, data };
                                if to == crate::modmsg::EVERYONE {
                                    self.broadcast(&msg, id);
                                } else if to != id {
                                    self.send_to(to, &msg);
                                }
                            }
                            continue;
                        }
                        // Fights: a hit and the gear are the server's to read, and the news of a hit
                        // is the server's alone to give (never passed on from a player).
                        match channel {
                            ch::PVP_HIT => self.pvp_hit(i, &data),
                            ch::PVP_GEAR => self.pvp_gear(i, &data, &mut log),
                            _ => {}
                        }
                        if (ch::PVP_HIT..=ch::PVP_GEAR).contains(&channel) || !crate::net::relay_rules::player_may_send(channel) {
                            continue;
                        }
                        let flags = self.flags_for(&self.conns[i].name);
                        // The creatures are the business of the one player who runs
                        // them; what is in a chest is for those who may build.
                        let allowed = match channel {
                            ch::MOBS => self.runs_the_world(i),
                            ch::CHEST => flags & crate::proto::PERM_BUILD != 0,
                            _ => true,
                        };
                        if allowed {
                            if keep {
                                self.keep_state(channel, &key, &data);
                            }
                            self.broadcast(&Msg::State { id, channel, keep, key, data }, id);
                        }
                    }
                    // Skins are taken only if their header and size are a skin's, at most
                    // once in five seconds, and relayed in the form each client knows.
                    Msg::Skin { png, slim, .. } if joined => {
                        if self.conns[i].skin.on_whole(png, slim) {
                            self.relay_skin(i);
                        }
                    }
                    Msg::SkinBegin { size, side, slim, .. } if joined => {
                        let _ = self.conns[i].skin.on_begin(size, side, slim);
                    }
                    Msg::SkinPart { data, last, .. } if joined => {
                        if self.conns[i].skin.on_part(&data, last) {
                            self.relay_skin(i);
                        }
                    }
                    Msg::Chat { text, .. } if joined && text.starts_with('/') => {
                        let name = self.conns[i].name.clone();
                        log.push(alloc::format!("{} ran {}", name, accounts_flow::loggable(&text)));
                        let reply = self.command(&name, &text);
                        let bytes = Msg::Chat { id: 0, text: reply }.encode();
                        if self.conns[i].stream.send_all(&bytes, SEND_TIMEOUT).is_err() {
                            self.conns[i].gone = true;
                        }
                    }
                    Msg::Chat { text, .. } if joined => {
                        let text: String = text.chars().take(120).collect();
                        log.push(alloc::format!("<{}> {}", self.conns[i].name, text));
                        self.sync.history.push(&alloc::format!("<{}> {}", self.conns[i].name, text));
                        self.broadcast(&Msg::Chat { id, text }, id);
                    }
                    // Anything else from a client is not allowed.
                    _ => {
                        self.conns[i].gone = true;
                        break;
                    }
                }
            }
        }
        // Big skins that came in are unpacked by the worker, not here; the ones it has
        // finished are passed on.
        for c in self.conns.iter_mut().filter(|c| !c.gone) {
            if let Some(pending) = c.skin.take_job() {
                let _ = crate::net::skinwork::submit(crate::net::skinwork::Job { id: c.id, pending });
            }
        }
        for d in crate::net::skinwork::take_done() {
            if let Some(k) = self.conns.iter().position(|c| c.id == d.id && c.joined && !c.gone) {
                if self.conns[k].skin.finish(d.pending, d.legacy) {
                    self.relay_skin(k);
                }
            }
        }
        // A few pieces of skins for whoever is waiting for them.
        for c in self.conns.iter_mut().filter(|c| !c.gone && !c.skin_out.is_empty()) {
            let mut batch = Vec::new();
            c.skin_out.take(crate::net::skinqueue::PER_TICK, &mut batch);
            for bytes in batch {
                if c.stream.send_all(&bytes, SEND_TIMEOUT).is_err() {
                    c.gone = true;
                    c.skin_out.clear();
                    break;
                }
            }
        }
        // A few pieces of mods for whoever is downloading them.
        for c in self.conns.iter_mut().filter(|c| !c.gone) {
            for _ in 0..12 {
                let Some(&(index, at)) = c.mod_out.front() else { break };
                let data = &self.cfg.mods[index].data;
                let end = at.saturating_add(crate::proto::MAX_PACK_PART).min(data.len());
                let last = end >= data.len();
                let bytes = Msg::ModPart { index: index as u16, last, data: data[at.min(end)..end].to_vec() }.encode();
                if last {
                    c.mod_out.pop_front();
                } else if let Some(front) = c.mod_out.front_mut() {
                    front.1 = end;
                }
                if c.stream.send_all(&bytes, SEND_TIMEOUT).is_err() {
                    c.gone = true;
                    break;
                }
            }
        }
        // Say goodbye to those who left.
        let mut k = 0;
        while k < self.conns.len() {
            if self.conns[k].gone {
                let c = self.conns.swap_remove(k);
                if c.joined {
                    // (An account's place and health are kept under its name, a guest's under theirs.)
                    let notes = Self::leave_notes(&c);
                    self.apply_notes(notes);
                    log.push(alloc::format!("{} left ({} online)", c.name, self.players()));
                    self.broadcast(&Msg::Leave { id: c.id }, c.id);
                }
            } else {
                k += 1;
            }
        }
        self.announce_runner();
        log
    }

    /// The edits as text, one `x y z state-name` per line.
    pub fn save_text(&self) -> String {
        let r = crate::block::reg();
        let mut s = String::new();
        for (k, &b) in &self.edits {
            s.push_str(&alloc::format!("{} {} {} {}\n", k.0, k.1, k.2, r.state_name(b)));
        }
        s
    }

    /// Note where a player stands as they leave (only sane numbers; at most 4096 players).
    fn remember_spot(&mut self, name: &str, spot: [f64; 5]) {
        let ok = spot.iter().all(|v| v.is_finite()) && spot[..3].iter().all(|v| *v > -3.0e7 && *v < 3.0e7) && spot[1] > -64.0 && spot[1] < 4096.0;
        let key = name.to_ascii_lowercase();
        if ok && (self.spots.len() < 4096 || self.spots.contains_key(&key)) {
            self.spots.insert(key, spot);
            self.spots_dirty = true;
        }
    }

    /// Note where everyone who is here stands now (done every few seconds and as the server stops,
    /// so a restart or a crash costs little).
    pub fn snapshot_spots(&mut self) {
        let here: Vec<sync_flow::Notes> = self.conns.iter().filter(|c| c.joined && !c.gone).map(Self::leave_notes).collect();
        for n in here {
            self.apply_notes(n);
        }
    }

    /// The remembered places, one `name x y z yaw pitch` line each.
    pub fn spots_text(&self) -> String {
        self.spots
            .iter()
            .map(|(n, s)| {
                // (Their health follows when it is known: health, most health, hunger.)
                let health = self.sync.health.get(n).map_or(String::new(), |h| alloc::format!(" {} {} {}", h[0], h[1], h[2]));
                alloc::format!("{} {} {} {} {} {}{}\n", n, s[0], s[1], s[2], s[3], s[4], health)
            })
            .collect()
    }

    /// Read what [`Server::spots_text`] wrote.
    pub fn load_spots(&mut self, text: &str) {
        for line in text.lines() {
            let w: Vec<&str> = line.split(' ').collect();
            if w.len() == 6 || w.len() == 9 {
                let n: Vec<Option<f64>> = w[1..6].iter().map(|v| v.parse::<f64>().ok()).collect();
                if let [Some(a), Some(b), Some(c), Some(d), Some(e)] = n[..] {
                    self.remember_spot(w[0], [a, b, c, d, e]);
                    if w.len() == 9 {
                        let h: Vec<Option<u8>> = w[6..9].iter().map(|v| v.parse::<u8>().ok()).collect();
                        if let [Some(a), Some(b), Some(c)] = h[..] {
                            self.remember_health(w[0], [a, b, c]);
                        }
                    }
                }
            }
        }
        self.spots_dirty = false;
    }

    pub fn load_text(&mut self, text: &str) {
        let r = crate::block::reg();
        for line in text.lines() {
            let w: Vec<&str> = line.split(' ').collect();
            if let [x, y, z, name] = w[..] {
                if let (Ok(x), Ok(y), Ok(z), Some(b)) = (x.parse(), y.parse(), z.parse(), r.state_by_name(name)) {
                    self.edits.insert((x, y, z), b);
                }
            }
        }
    }
}

/// A remembered place as the text the client reads: `x y z yaw pitch`.
fn spot_text(s: &[f64; 5]) -> String {
    alloc::format!("{} {} {} {} {}", s[0], s[1], s[2], s[3], s[4])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_gate_building_game_mode_and_commands() {
        use crate::proto::*;
        let mut s = Server::bind(Config { port: 0, seed: 1, kind: String::from("normal"), max_players: 4, content: 0, states: 10, pack: Vec::new(), mods: Vec::new() }).unwrap();
        s.ops.push(String::from("Ann"));
        // By default players build, and nothing else; operators do everything.
        assert_eq!(s.flags_for("Bob"), PERM_BUILD | PERM_VOICE | PERM_PVP);
        assert_eq!(s.flags_for("ann"), PERM_BUILD | PERM_GAMEMODE | PERM_COMMANDS | PERM_VOICE | PERM_PVP);
        // Bob may not choose his game mode until he is allowed to.
        assert_eq!(s.command("Bob", "/gamemode creative"), "you may not change your game mode here");
        assert_eq!(s.command("Bob", "/perm Bob +gamemode"), "only operators may do that");
        assert_eq!(s.command("Ann", "/perm Bob +gamemode -build"), "Bob may now: gamemode voice pvp");
        assert_eq!(s.flags_for("bob"), PERM_GAMEMODE | PERM_VOICE | PERM_PVP);
        assert_eq!(s.command("Bob", "/gamemode creative"), "Bob is now in creative mode");
        assert_eq!(s.command("Bob", "/gm s"), "Bob is now in survival mode");
        assert_eq!(s.command("Bob", "/gmc"), "Bob is now in creative mode");
        assert_eq!(s.flags_for("Bob"), PERM_GAMEMODE | PERM_VOICE | PERM_PVP | PERM_CREATIVE);
        assert_eq!(s.command("Bob", "/gamemode Ann survival"), "only operators may change someone else's game mode");
        // The default for everyone else.
        assert_eq!(s.command("Ann", "/default -build"), "players may now by default: voice pvp");
        assert_eq!(s.flags_for("Cy"), PERM_VOICE | PERM_PVP);
        assert_eq!(s.command("Ann", "/default +build +commands"), "players may now by default: build commands voice pvp");
        assert_eq!(s.command("Ann", "/perm Bob +fly"), "use /perm NAME +build -build +gamemode -gamemode +commands -commands +voice -voice +pvp -pvp");
        // It is all kept.
        let text = s.perm_text();
        let mut t = Server::bind(Config { port: 0, seed: 1, kind: String::from("normal"), max_players: 4, content: 0, states: 10, pack: Vec::new(), mods: Vec::new() }).unwrap();
        for line in text.lines() {
            assert!(t.load_perm_line(line), "{line}");
        }
        assert_eq!(t.default_perms, PERM_BUILD | PERM_COMMANDS | PERM_VOICE | PERM_PVP);
        assert_eq!(t.flags_for("bob"), PERM_GAMEMODE | PERM_VOICE | PERM_PVP | PERM_CREATIVE);
    }

    #[test]
    fn operators_and_their_commands() {
        let cfg = Config { port: 0, seed: 1, kind: String::new(), max_players: 4, content: 0, states: 1, pack: Vec::new(), mods: Vec::new() };
        let mut s = Server::bind(cfg).unwrap();
        s.ops.push(String::from("Ann"));
        // Anyone may look; only operators may act.
        assert!(s.command("Bob", "/list").starts_with("online (0)"));
        assert_eq!(s.command("Bob", "/op Bob"), "only operators may do that");
        assert_eq!(s.command("ann", "/op Bob"), "Bob is an operator");
        assert!(s.ops_dirty && s.ops.len() == 2);
        assert_eq!(s.command("Bob", "/deop Ann"), "Ann is no longer an operator");
        // The last operator stays.
        assert_eq!(s.command("Bob", "/deop Bob"), "that is the only operator");
        assert_eq!(s.command("Bob", "/ban Mallory"), "Mallory may not join");
        assert!(s.banned.iter().any(|b| b == "Mallory"));
        assert_eq!(s.command("Bob", "/pardon mallory"), "mallory may join again");
        assert!(s.banned.is_empty());
        assert_eq!(s.command("Bob", "/time 0.25"), "time set");
        assert_eq!(s.time, 0.25);
        assert!(s.command("Bob", "/time later").starts_with("use /time"));
        assert!(s.command("Bob", "/dance").starts_with("no such server command"));
    }

    #[test]
    fn voice_permission_muting_and_range_are_commands_and_are_kept() {
        use crate::proto::*;
        let cfg = || Config { port: 0, seed: 1, kind: String::new(), max_players: 4, content: 0, states: 1, pack: Vec::new(), mods: Vec::new() };
        let mut s = Server::bind(cfg()).unwrap();
        s.ops.push(String::from("Ann"));
        assert_eq!(s.command("Bob", "/mute Cy"), "only operators may do that");
        assert_eq!(s.command("Bob", "/voicerange 5"), "only operators may do that");
        assert_eq!(s.command("Ann", "/mute Cy"), "Cy may not talk on voice chat");
        assert_eq!(s.command("Ann", "/mute cy"), "cy is already muted");
        assert!(s.voice.is_muted("CY") && !s.voice.is_muted("Bob"));
        assert_eq!(s.command("Ann", "/unmute Bob"), "Bob was not muted");
        assert_eq!(s.command("Ann", "/voicerange 40"), "voices now carry 40 blocks");
        assert!(s.command("Ann", "/voicerange far").starts_with("use /voicerange"));
        assert_eq!(s.command("Ann", "/voicerange"), "voices carry 40 blocks");
        assert_eq!(s.command("Ann", "/perm Bob -voice"), "Bob may now: build pvp");
        assert_eq!(s.flags_for("Bob") & PERM_VOICE, 0);
        // All of it is kept, and files from before voice chat still allow voice.
        let mut t = Server::bind(cfg()).unwrap();
        for line in s.perm_text().lines() {
            assert!(t.load_perm_line(line), "{line}");
        }
        assert!(t.voice.is_muted("cy") && t.voice.range == 40.0);
        assert_eq!(t.flags_for("Bob") & PERM_VOICE, 0);
        assert_eq!(t.flags_for("Dee") & PERM_VOICE, PERM_VOICE);
        let mut old = Server::bind(cfg()).unwrap();
        assert!(old.load_perm_line("default 1"));
        assert!(old.load_perm_line("perm 1 eve"));
        assert_eq!(old.flags_for("Zed"), PERM_BUILD | PERM_VOICE | PERM_PVP);
        assert_eq!(old.flags_for("Eve"), PERM_BUILD | PERM_VOICE | PERM_PVP);
    }

    fn plain_config() -> Config {
        Config { port: 0, seed: 1, kind: String::new(), max_players: 4, content: 0, states: 1, pack: Vec::new(), mods: Vec::new() }
    }

    #[test]
    fn player_numbers_skip_zero_and_everybody_and_are_never_shared() {
        let mut s = Server::bind(plain_config()).unwrap();
        s.next_id = 65_533;
        let ids: Vec<u16> = (0..5).map(|_| s.fresh_id()).collect();
        assert_eq!(ids, [65_533, 65_534, 1, 2, 3], "65535 is what 'everyone' is, 0 is the server");
        // One still in use after the counter went round is not given out again.
        let mut s = Server::bind(plain_config()).unwrap();
        let stream = crate::sys::net::TcpStream::connect("127.0.0.1", s.port(), 2000).unwrap();
        s.conns.push(Conn { stream, inbox: Inbox::default(), id: 7, name: String::from("Ann"), joined: true, last_move: None, skin: Default::default(), skin_out: Default::default(), proto: 0, voice_flags: 0, voice_bucket: Default::default(), quiet: 0, idle: 0.0, throttle: Default::default(), gone: false, mod_out: Default::default(), mods_asked: false, effects: Vec::new(), acct: Default::default(), sync: Default::default(), pvp: Default::default() });
        s.next_id = 6;
        assert_eq!((s.fresh_id(), s.fresh_id()), (6, 8));
    }

    #[test]
    fn kept_state_is_bounded_by_bytes_and_count_and_can_be_replaced_or_removed() {
        let mut s = Server::bind(plain_config()).unwrap();
        let big = alloc::vec![1u8; 3 << 20];
        s.keep_state(3, "a", &big);
        s.keep_state(3, "b", &big);
        assert_eq!(s.kept_bytes, 6 << 20);
        s.keep_state(3, "c", &big);
        assert_eq!(s.kept.len(), 2, "the third would pass the limit");
        // Replacing a piece counts only the difference.
        s.keep_state(3, "a", &alloc::vec![2u8; 4 << 20]);
        assert_eq!(s.kept_bytes, 7 << 20);
        s.keep_state(3, "a", &[]);
        s.keep_state(3, "b", &[]);
        s.keep_state(3, "nothing", &[]);
        assert_eq!((s.kept.len(), s.kept_bytes), (0, 0));
        for i in 0..9000 {
            s.keep_state(3, &alloc::format!("{i}"), b"x");
        }
        assert_eq!(s.kept.len(), 8192);
    }
}
