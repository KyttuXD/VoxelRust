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
    skin: Vec<u8>,
    skin_slim: bool,
    /// What this player's voice chat is set to (`VOICE_` flags); 0 until they say.
    voice_flags: u8,
    voice_bucket: crate::net_voice::Bucket,
    /// Ticks since anything arrived; idle strangers are dropped.
    quiet: u32,
    gone: bool,
    /// Mod bundle pieces (encoded messages) waiting to be sent, a few per tick.
    mod_out: alloc::collections::VecDeque<Vec<u8>>,
    /// This player has already asked for mods.
    mods_asked: bool,
    /// The status effects they last said they have, for telling newcomers.
    effects: Vec<(String, u8, u32)>,
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
    /// Names of the operators: players who may use the server's commands.
    /// The first player ever to join becomes one.
    pub ops: Vec<String>,
    /// Names that may not join.
    pub banned: Vec<String>,
    /// `ops` or `banned` changed since they were last written out.
    pub ops_dirty: bool,
    /// What players may do who have no entry of their own.
    pub default_perms: u8,
    /// Players with permissions of their own (lower-case name, `PERM_` flags).
    pub perms: Vec<(String, u8)>,
    /// Players in creative mode (lower-case names); everyone else is in survival.
    pub creative: Vec<String>,
    /// Voice chat: who is muted and how far a voice carries (see [`crate::net_voice`]).
    pub voice: crate::net_voice::Rules,
}

const SEND_TIMEOUT: u32 = 3000;

impl Server {
    pub fn bind(cfg: Config) -> Result<Server, NetError> {
        let listener = TcpListener::bind(cfg.port)?;
        Ok(Server { listener, conns: Vec::new(), edits: BTreeMap::new(), cfg, next_id: 1, time: 0.12, time_timer: 0.0, dirty: false,
            kept: BTreeMap::new(), ops: Vec::new(), banned: Vec::new(), ops_dirty: false,
            default_perms: crate::proto::PERM_BUILD | crate::proto::PERM_VOICE, perms: Vec::new(), creative: Vec::new(), voice: Default::default() })
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
        let mut f = if self.ops.iter().any(|o| o.eq_ignore_ascii_case(who)) {
            PERM_BUILD | PERM_GAMEMODE | PERM_COMMANDS | PERM_VOICE
        } else {
            self.perms.iter().find(|p| p.0 == lower).map_or(self.default_perms, |p| p.1)
        };
        if self.creative.contains(&lower) {
            f |= PERM_CREATIVE;
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
        let mut t = alloc::format!("default {}\n", self.default_perms | 128);
        for (n, f) in &self.perms {
            t.push_str(&alloc::format!("perm {} {}\n", f | 128, n));
        }
        for n in &self.creative {
            t.push_str(&alloc::format!("creative {}\n", n));
        }
        t.push_str(&self.voice.text());
        t
    }

    /// Flags as written to the permissions file: files from before voice chat
    /// (no 128 mark) allow voice.
    fn read_flags(f: u8) -> u8 {
        let kept = f & (crate::proto::PERM_BUILD | crate::proto::PERM_GAMEMODE | crate::proto::PERM_COMMANDS | crate::proto::PERM_VOICE);
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
                _ => return None,
            };
            flags = if on { flags | bit } else { flags & !bit };
        }
        Some(flags)
    }

    fn describe(flags: u8) -> String {
        use crate::proto::*;
        let mut v: Vec<&str> = Vec::new();
        for (bit, n) in [(PERM_BUILD, "build"), (PERM_GAMEMODE, "gamemode"), (PERM_COMMANDS, "commands"), (PERM_VOICE, "voice")] {
            if flags & bit != 0 {
                v.push(n);
            }
        }
        if v.is_empty() { String::from("nothing") } else { v.join(" ") }
    }

    /// A command typed by the player `who`. Anyone may ask /list and
    /// /help; the rest are for operators. Returns what to tell them.
    pub fn command(&mut self, who: &str, line: &str) -> String {
        let w: Vec<&str> = line.split_whitespace().collect();
        let arg = |k: usize| w.get(k).copied().unwrap_or("");
        let is_op = self.ops.iter().any(|o| o.eq_ignore_ascii_case(who));
        let online = |s: &Server, name: &str| s.conns.iter().position(|c| c.joined && !c.gone && c.name.eq_ignore_ascii_case(name));
        match arg(0).to_ascii_lowercase().as_str() {
            "/list" => {
                let names: Vec<String> = self.conns.iter().filter(|c| c.joined && !c.gone).map(|c| if self.ops.iter().any(|o| o.eq_ignore_ascii_case(&c.name)) { alloc::format!("{}*", c.name) } else { c.name.clone() }).collect();
                alloc::format!("online ({}): {}   (* = operator)", names.len(), names.join(", "))
            }
            "/help" => String::from("server: /list /gamemode creative|survival (if allowed)   operators: /op /deop /kick /ban /pardon NAME  /say TEXT  /time 0..1  /perm NAME [+build -gamemode +commands +voice]  /default [+build ...]  /mute NAME  /unmute NAME  /voicerange BLOCKS  /gamemode NAME creative|survival"),
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
                    None => String::from("use /perm NAME +build -build +gamemode -gamemode +commands -commands +voice -voice"),
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
                    None => String::from("use /default +build -build +gamemode -gamemode +commands -commands +voice -voice"),
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
        }
        for _ in 0..8 {
            match self.listener.accept(0) {
                Ok(Some(mut stream)) => {
                    stream.set_nodelay(true);
                    self.conns.push(Conn { stream, inbox: Inbox::default(), id: 0, name: String::new(), joined: false, last_move: None, skin: Vec::new(), skin_slim: false, voice_flags: 0, voice_bucket: Default::default(), quiet: 0, gone: false, mod_out: Default::default(), mods_asked: false, effects: Vec::new() });
                }
                _ => break,
            }
        }
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
            if !c.joined {
                // A tile relay request is a plain text line, not a message.
                let peek = c.inbox.peek();
                if peek.starts_with(b"TILE ") || peek.starts_with(b"GEO ") {
                    if let Some(end) = peek.iter().position(|&b| b == b'\n') {
                        let line = String::from_utf8_lossy(&peek[..end]).into_owned();
                        let data = tile(&line).unwrap_or_default();
                        let _ = c.stream.send_all(&(data.len() as u32).to_le_bytes(), SEND_TIMEOUT);
                        let _ = c.stream.send_all(&data, 20_000);
                        log.push(alloc::format!("relay: {} -> {} bytes", line, data.len()));
                        c.gone = true;
                    } else if peek.len() > 64 {
                        c.gone = true;
                    }
                    continue;
                }
                if c.quiet > 4000 {
                    c.gone = true; // connected but never said hello
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
                match msg {
                    Msg::Hello { version, content, name } if !joined => {
                        // Content need not match: the server sends its own.
                        let _ = content;
                        let refuse = if version != VERSION {
                            Some("this server runs a different version of the game")
                        } else if self.banned.iter().any(|b| b.eq_ignore_ascii_case(&name)) {
                            Some("you may not join this server")
                        } else if self.players() >= self.cfg.max_players {
                            Some("the server is full")
                        } else {
                            None
                        };
                        if let Some(reason) = refuse {
                            let _ = self.conns[i].stream.send_all(&Msg::Refuse { reason: String::from(reason) }.encode(), SEND_TIMEOUT);
                            self.conns[i].gone = true;
                            break;
                        }
                        let mut name = crate::settings::clean_name(&name, 16);
                        if name.is_empty() {
                            name = String::from("Player");
                        }
                        // Two players never share a name.
                        let taken = |n: &str, conns: &[Conn]| conns.iter().any(|c| c.joined && !c.gone && c.name == n);
                        let mut n = 2;
                        let base = name.clone();
                        while taken(&name, &self.conns) {
                            name = alloc::format!("{}{}", base, n);
                            n += 1;
                        }
                        let id = self.next_id;
                        self.next_id = self.next_id.wrapping_add(1).max(1);
                        // Whoever is first through the door holds the keys.
                        if self.ops.is_empty() {
                            self.ops.push(name.clone());
                            self.ops_dirty = true;
                        }
                        // Tell the newcomer about the world and who is here.
                        let mut out = Msg::Welcome { id, seed: self.cfg.seed, time: self.time, kind: self.cfg.kind.clone() }.encode();
                        // The content pack first (even when empty), so the
                        // edits that follow are read with the right blocks.
                        let pack = &self.cfg.pack[..];
                        let parts: Vec<&[u8]> = if pack.is_empty() { alloc::vec![&pack[..]] } else { pack.chunks(crate::proto::MAX_PACK_PART).collect() };
                        for (k, part) in parts.iter().enumerate() {
                            out.extend(Msg::Pack { last: k + 1 == parts.len(), data: part.to_vec() }.encode());
                        }
                        // Then what mods can be downloaded (even if none).
                        out.extend(Msg::ModList { mods: self.cfg.mods.iter().map(|m| m.info()).collect() }.encode());
                        for other in self.conns.iter().filter(|c| c.joined && !c.gone) {
                            out.extend(Msg::Join { id: other.id, name: other.name.clone() }.encode());
                            if !other.skin.is_empty() {
                                out.extend(Msg::Skin { id: other.id, png: other.skin.clone(), slim: other.skin_slim }.encode());
                            }
                            if let Some((pos, yaw, pitch, held, pose)) = other.last_move {
                                out.extend(Msg::Move { id: other.id, pos, yaw, pitch, held, pose }.encode());
                            }
                            if !other.effects.is_empty() {
                                out.extend(Msg::Effects { id: other.id, list: other.effects.clone() }.encode());
                            }
                        }
                        let all: Vec<(i32, i32, i32, u16)> = self.edits.iter().map(|(k, &b)| (k.0, k.1, k.2, b)).collect();
                        for part in all.chunks(4000) {
                            out.extend(Msg::Edits { list: part.to_vec() }.encode());
                        }
                        for ((channel, key), data) in &self.kept {
                            out.extend(Msg::State { id: 0, channel: *channel, keep: true, key: key.clone(), data: data.clone() }.encode());
                        }
                        if self.conns[i].stream.send_all(&out, 4_000).is_err() {
                            self.conns[i].gone = true;
                            break;
                        }
                        self.conns[i].id = id;
                        self.conns[i].name = name.clone();
                        self.conns[i].joined = true;
                        self.send_perms(Some(i));
                        log.push(alloc::format!("{} joined ({} online)", name, self.players()));
                        self.broadcast(&Msg::Join { id, name }, id);
                    }
                    Msg::Move { pos, yaw, pitch, held, pose, .. } if joined => {
                        self.conns[i].last_move = Some((pos, yaw, pitch, held, pose));
                        self.broadcast(&Msg::Move { id, pos, yaw, pitch, held, pose }, id);
                    }
                    // An emote the player plays (or stops): everyone else sees it.
                    Msg::Emote { emote, .. } if joined => self.broadcast(&Msg::Emote { id, emote }, id),
                    // Their status effects: kept for newcomers and shown to everyone else.
                    Msg::Effects { list, .. } if joined => {
                        let list = crate::game::effects::sync::clean(list);
                        self.conns[i].effects = list.clone();
                        self.broadcast(&Msg::Effects { id, list }, id);
                    }
                    Msg::SetBlock { x, y, z, block } if joined => {
                        let may = self.flags_for(&self.conns[i].name) & crate::proto::PERM_BUILD != 0;
                        if may && (0..CH).contains(&y) && block < self.cfg.states {
                            self.edits.insert((x, y, z), block);
                            self.dirty = true;
                            self.broadcast(&Msg::SetBlock { x, y, z, block }, id);
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
                                let data = &self.cfg.mods[index].data;
                                let parts: Vec<&[u8]> = if data.is_empty() { alloc::vec![&data[..]] } else { data.chunks(crate::proto::MAX_PACK_PART).collect() };
                                for (k, part) in parts.iter().enumerate() {
                                    queued.push_back(Msg::ModPart { index: index as u16, last: k + 1 == parts.len(), data: part.to_vec() }.encode());
                                }
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
                        if keep {
                            if data.is_empty() {
                                self.kept.remove(&(channel, key.clone()));
                            } else if self.kept.len() < 8192 || self.kept.contains_key(&(channel, key.clone())) {
                                self.kept.insert((channel, key.clone()), data.clone());
                            }
                        }
                        self.broadcast(&Msg::State { id, channel, keep, key, data }, id);
                    }
                    Msg::Skin { png, slim, .. } if joined => {
                        self.conns[i].skin = png.clone();
                        self.conns[i].skin_slim = slim;
                        self.broadcast(&Msg::Skin { id, png, slim }, id);
                    }
                    Msg::Chat { text, .. } if joined && text.starts_with('/') => {
                        let name = self.conns[i].name.clone();
                        log.push(alloc::format!("{} ran {}", name, text));
                        let reply = self.command(&name, &text);
                        let bytes = Msg::Chat { id: 0, text: reply }.encode();
                        if self.conns[i].stream.send_all(&bytes, SEND_TIMEOUT).is_err() {
                            self.conns[i].gone = true;
                        }
                    }
                    Msg::Chat { text, .. } if joined => {
                        let text: String = text.chars().take(120).collect();
                        log.push(alloc::format!("<{}> {}", self.conns[i].name, text));
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
        // A few pieces of mods for whoever is downloading them.
        for c in self.conns.iter_mut().filter(|c| !c.gone) {
            for _ in 0..12 {
                match c.mod_out.pop_front() {
                    Some(bytes) => {
                        if c.stream.send_all(&bytes, SEND_TIMEOUT).is_err() {
                            c.gone = true;
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
        // Say goodbye to those who left.
        let mut k = 0;
        while k < self.conns.len() {
            if self.conns[k].gone {
                let c = self.conns.swap_remove(k);
                if c.joined {
                    log.push(alloc::format!("{} left ({} online)", c.name, self.players()));
                    self.broadcast(&Msg::Leave { id: c.id }, c.id);
                }
            } else {
                k += 1;
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_gate_building_game_mode_and_commands() {
        use crate::proto::*;
        let mut s = Server::bind(Config { port: 0, seed: 1, kind: String::from("normal"), max_players: 4, content: 0, states: 10, pack: Vec::new(), mods: Vec::new() }).unwrap();
        s.ops.push(String::from("Ann"));
        // By default players build, and nothing else; operators do everything.
        assert_eq!(s.flags_for("Bob"), PERM_BUILD | PERM_VOICE);
        assert_eq!(s.flags_for("ann"), PERM_BUILD | PERM_GAMEMODE | PERM_COMMANDS | PERM_VOICE);
        // Bob may not choose his game mode until he is allowed to.
        assert_eq!(s.command("Bob", "/gamemode creative"), "you may not change your game mode here");
        assert_eq!(s.command("Bob", "/perm Bob +gamemode"), "only operators may do that");
        assert_eq!(s.command("Ann", "/perm Bob +gamemode -build"), "Bob may now: gamemode voice");
        assert_eq!(s.flags_for("bob"), PERM_GAMEMODE | PERM_VOICE);
        assert_eq!(s.command("Bob", "/gamemode creative"), "Bob is now in creative mode");
        assert_eq!(s.flags_for("Bob"), PERM_GAMEMODE | PERM_VOICE | PERM_CREATIVE);
        assert_eq!(s.command("Bob", "/gamemode Ann survival"), "only operators may change someone else's game mode");
        // The default for everyone else.
        assert_eq!(s.command("Ann", "/default -build"), "players may now by default: voice");
        assert_eq!(s.flags_for("Cy"), PERM_VOICE);
        assert_eq!(s.command("Ann", "/default +build +commands"), "players may now by default: build commands voice");
        assert_eq!(s.command("Ann", "/perm Bob +fly"), "use /perm NAME +build -build +gamemode -gamemode +commands -commands +voice -voice");
        // It is all kept.
        let text = s.perm_text();
        let mut t = Server::bind(Config { port: 0, seed: 1, kind: String::from("normal"), max_players: 4, content: 0, states: 10, pack: Vec::new(), mods: Vec::new() }).unwrap();
        for line in text.lines() {
            assert!(t.load_perm_line(line), "{line}");
        }
        assert_eq!(t.default_perms, PERM_BUILD | PERM_COMMANDS | PERM_VOICE);
        assert_eq!(t.flags_for("bob"), PERM_GAMEMODE | PERM_VOICE | PERM_CREATIVE);
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
        assert_eq!(s.command("Ann", "/perm Bob -voice"), "Bob may now: build");
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
        assert_eq!(old.flags_for("Zed"), PERM_BUILD | PERM_VOICE);
        assert_eq!(old.flags_for("Eve"), PERM_BUILD | PERM_VOICE);
    }
}
