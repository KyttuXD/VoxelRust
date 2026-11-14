//! The client side of multiplayer.

use crate::proto::{content_checksum, Inbox, Msg, Next, VERSION};
use crate::sys::net::TcpStream;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Another player, as last heard of.
pub struct Remote {
    pub name: String,
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// The item in their hand: index plus one, 0 for none.
    pub held: u16,
    /// Walk animation, driven by how far they move between updates.
    pub phase: f32,
    pub stride: f32,
    /// Blocks a second they covered lately (from their position changes: tells a run from a walk).
    pub pace: f32,
    /// False until their first position arrives.
    pub seen: bool,
    /// Their pose byte (see `content::pose::unpack`), the posture they are
    /// easing into, and the emote they are playing (a code, 0 = none) and for how long.
    pub pose: u8,
    pub body: crate::content::pose::Smoother,
    pub emote: u16,
    pub emote_t: f32,
    /// The status effects on them: id, level, seconds left when it was sent.
    pub effects: Vec<(String, u8, u32)>,
    /// Their health, hunger, armour and ping, as last said (None until the first message; see `net/sync`).
    pub vitals: Option<crate::net::sync::vitals::Vitals>,
}

pub enum Event {
    Block(i32, i32, i32, u16),
    /// A line for the chat log.
    Chat(String),
    Time(f32),
    /// What this player may do (see `proto::PERM_*`).
    Perms(u8),
    /// A piece of the server's content pack; the flag marks the last one.
    Pack(bool, Vec<u8>),
    /// The mods the server offers for download.
    ModList(Vec<crate::proto::ModInfo>),
    /// A piece of one mod's bundle: its place in the list, whether it is the last piece, the bytes.
    ModPart(u16, bool, Vec<u8>),
    /// A player's skin image arrived, and whether their arms are slim.
    Skin(u16, Vec<u8>, bool),
    /// Shared state from another player: who, channel, key, data.
    State(u16, u8, String, Vec<u8>),
    /// The connection is gone.
    Lost,
    /// Another player joined (their name), for `join` scripts.
    Joined(String),
    /// Another player left.
    Left(String),
    /// Another player started (code above 0) or stopped (0) an emote: who and which.
    Emote(u16, u16),
}

pub struct Welcome {
    pub seed: u32,
    pub time: f32,
    pub kind: String,
}

pub struct Client {
    stream: TcpStream,
    inbox: Inbox,
    pub id: u16,
    pub players: BTreeMap<u16, Remote>,
    send_timer: f32,
    /// Time since a position was last sent at all.
    refresh_timer: f32,
    last_sent: ([f64; 3], f32, f32, u16, u8),
    /// The pose byte to send with the next position (set by the session).
    pub pose: u8,
    pub alive: bool,
    /// Voice frames that have arrived and not yet been played: who, sequence
    /// number, loudness (0..=255), the ADPCM frame. Taken by `client::voice`;
    /// kept short so a client that ignores them costs nothing.
    pub voice_in: Vec<(u16, u16, u8, Vec<u8>)>,
    /// Which program the server is (from [`Msg::Info`]; None until it says,
    /// and for a server from before it was sent).
    pub server_info: Option<crate::net::version::Info>,
    /// What big skins the server takes (largest side, most bytes), from
    /// [`Msg::SkinCaps`]; None for a server that does not say, which is sent
    /// 64 x 64 skins only.
    pub skin_caps: Option<(u16, u32)>,
    /// Big skins of other players that are on their way.
    skin_in: crate::net::skinxfer::Transfers,
    /// Seconds before our own skin may be sent again (the server takes one in five).
    skin_cool: f32,
    /// Seconds since connecting (the server's [`Msg::SkinCaps`] may still be on its way).
    age: f32,
    /// What this player's account is (see `net/accounts/client.rs`).
    pub acct: crate::net::accounts::client::ClientAccount,
    /// Our own ping in milliseconds as the server measured it (0 until it has; see `net/sync`).
    pub own_ping: u16,
    /// The player the server named to run the creatures (None: the lowest number does).
    pub runner: Option<u16>,
}

/// Most voice frames held waiting (about a second of one talker).
const MAX_VOICE_WAITING: usize = 64;

/// Split `host:port`; the port may be left out.
pub fn parse_address(addr: &str) -> Option<(String, u16)> {
    let addr = addr.trim();
    if addr.is_empty() {
        return None;
    }
    // An IPv6 address is written in brackets, `[::1]` or `[::1]:25590`.
    if let Some(rest) = addr.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        if host.is_empty() || !host.contains(':') || host.contains(|c: char| c.is_whitespace() || c == '[') {
            return None;
        }
        return match after {
            "" => Some((String::from(host), crate::proto::DEFAULT_PORT)),
            _ => Some((String::from(host), port_of(after.strip_prefix(':')?)?)),
        };
    }
    match addr.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && !h.contains(':') => Some((String::from(h), port_of(p)?)),
        Some(_) => None,
        None => Some((String::from(addr), crate::proto::DEFAULT_PORT)),
    }
}

/// A port number, 1 to 65535, digits only (no sign, no 0).
fn port_of(text: &str) -> Option<u16> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<u16>().ok().filter(|&p| p != 0)
}

impl Client {
    /// Connect and say hello. Waits (a few seconds at most) for the answer. A name that
    /// has an account is refused here (use [`Client::connect_auth`] with its PIN).
    pub fn connect(host: &str, port: u16, name: &str) -> Result<(Client, Welcome), String> {
        Client::connect_auth(host, port, name, &crate::net::accounts::client::JoinAuth::default()).map_err(String::from)
    }

    /// Connect, say hello and, if the server asks for a PIN, answer with `auth`. When there is no PIN
    /// yet (or it is wrong) the error says what to ask the player; join again with it.
    pub fn connect_auth(host: &str, port: u16, name: &str, auth: &crate::net::accounts::client::JoinAuth) -> Result<(Client, Welcome), crate::net::accounts::client::ConnectError> {
        use crate::net::accounts::client::{respond, Challenge, ConnectError};
        let failed = |s: &str| ConnectError::Failed(String::from(s));
        let mut stream = TcpStream::connect(host, port, 6000).map_err(|e| ConnectError::Failed(alloc::format!("COULD NOT CONNECT ({:?})", e).to_ascii_uppercase()))?;
        stream.set_nodelay(true);
        let hello = Msg::Hello { version: VERSION, content: content_checksum(), name: String::from(name) };
        stream.send_all(&hello.encode(), 4000).map_err(|_| failed("THE SERVER HUNG UP"))?;
        let mut inbox = Inbox::default();
        let mut buf = [0u8; 4096];
        let mut answered = false;
        for _ in 0..120 {
            match inbox.next() {
                Next::Msg(Msg::Welcome { id, seed, time, kind }) => {
                    let mut c = Client { stream, inbox, id, players: BTreeMap::new(), send_timer: 0.0, refresh_timer: 0.0, last_sent: ([0.0; 3], 0.0, 0.0, 0, 0), pose: 0, alive: true, voice_in: Vec::new(), server_info: None, skin_caps: None, skin_in: Default::default(), skin_cool: 0.0, age: 0.0, acct: Default::default(), own_ping: 0, runner: None };
                    c.acct.name = crate::settings::clean_name(name, 16).to_ascii_lowercase();
                    c.acct.at = auth.at.clone();
                    c.acct.remember = auth.remember;
                    // A PIN that has just worked is kept if the player asked for that.
                    if let (true, true, Some(pin), Some(at)) = (answered, auth.remember, auth.pin.as_deref(), auth.at.as_ref()) {
                        crate::net::accounts::remember::save(&at.base, &at.address, &c.acct.name, pin);
                    }
                    return Ok((c, Welcome { seed, time: crate::net::limits::clean_time(time), kind }));
                }
                Next::Msg(Msg::Refuse { reason }) => return Err(ConnectError::Failed(crate::net::version::explain_refusal(&reason))),
                Next::Msg(Msg::AuthChallenge { op, flags, rounds, tries, salt, new_salt, nonce }) if !answered => {
                    let challenge = Challenge { op, flags, rounds, tries, salt, new_salt, nonce };
                    let answer = respond(auth, name, &challenge)?;
                    stream.send_all(&answer.encode(), 4000).map_err(|_| failed("THE SERVER HUNG UP"))?;
                    answered = true;
                }
                Next::Msg(Msg::AuthResult { ok: false, tries, text }) => {
                    // A PIN that was kept and is wrong now is forgotten (it was changed somewhere else).
                    if let (true, Some(at)) = (tries > 0 && !auth.register, auth.at.as_ref()) {
                        crate::net::accounts::remember::forget(&at.base, &at.address, name);
                    }
                    if auth.register {
                        return Err(ConnectError::Failed(text.to_ascii_uppercase()));
                    }
                    return Err(ConnectError::NeedPin { register: false, tries, note: text.to_ascii_uppercase(), weak_ok: false });
                }
                Next::Msg(_) | Next::Bad => return Err(failed("THAT IS NOT A VOXELRUST SERVER")),
                Next::Wait => {}
            }
            match stream.recv(&mut buf, 100) {
                Ok(n) => inbox.push(&buf[..n]),
                Err(_) => return Err(failed("THE SERVER HUNG UP")),
            }
        }
        Err(failed("THE SERVER DID NOT ANSWER"))
    }

    pub(crate) fn send(&mut self, msg: &Msg) {
        if self.alive && self.stream.send_all(&msg.encode(), 2000).is_err() {
            self.alive = false;
        }
    }

    /// Read what has arrived and report what the game needs to act on.
    pub fn poll(&mut self, dt: f32) -> Vec<Event> {
        let mut events = Vec::new();
        if !self.alive {
            return events;
        }
        if dt > 0.0 && dt < 3600.0 {
            self.skin_cool = (self.skin_cool - dt).max(0.0);
            self.age += dt;
            self.skin_in.tick(dt);
        }
        let mut buf = [0u8; 8192];
        for _ in 0..32 {
            match self.stream.recv(&mut buf, 0) {
                Ok(0) => break,
                Ok(n) => self.inbox.push(&buf[..n]),
                Err(_) => {
                    self.alive = false;
                    break;
                }
            }
        }
        loop {
            match self.inbox.next() {
                Next::Wait => break,
                Next::Bad => {
                    self.alive = false;
                    break;
                }
                Next::Msg(m) => match m {
                    Msg::Join { id, name } => {
                        events.push(Event::Chat(alloc::format!("{} JOINED", name.to_ascii_uppercase())));
                        events.push(Event::Joined(name.clone()));
                        self.players.insert(id, Remote { name, pos: [0.0; 3], yaw: 0.0, pitch: 0.0, held: 0, phase: 0.0, stride: 0.0, pace: 0.0, seen: false, pose: 0, body: Default::default(), emote: 0, emote_t: 0.0, effects: Vec::new(), vitals: None });
                    }
                    Msg::Leave { id } => {
                        self.skin_in.forget(id);
                        if let Some(p) = self.players.remove(&id) {
                            events.push(Event::Chat(alloc::format!("{} LEFT", p.name.to_ascii_uppercase())));
                            events.push(Event::Left(p.name.clone()));
                        }
                    }
                    Msg::Move { id, pos, yaw, pitch, held, pose } => {
                        if let Some(p) = self.players.get_mut(&id) {
                            p.held = held;
                            p.pose = pose;
                            let (dx, dz) = ((pos[0] - p.pos[0]) as f32, (pos[2] - p.pos[2]) as f32);
                            let step = crate::math::sqrtf(dx * dx + dz * dz);
                            if p.seen {
                                p.phase += step * 3.2;
                                p.stride = (step * 2.5).min(1.0);
                                // (positions come ten times a second)
                                p.pace += (step * 10.0 - p.pace) * 0.5;
                            }
                            p.pos = pos;
                            p.yaw = yaw;
                            p.pitch = pitch;
                            p.seen = true;
                        }
                    }
                    Msg::Emote { id, emote } => {
                        if let Some(p) = self.players.get_mut(&id) {
                            p.emote = emote;
                            p.emote_t = 0.0;
                            events.push(Event::Emote(id, emote));
                        }
                    }
                    // Someone's effects: kept on their record (nothing for the game to do).
                    Msg::Effects { id, list } => {
                        if let Some(p) = self.players.get_mut(&id) {
                            p.effects = crate::game::effects::sync::clean(list);
                        }
                    }
                    Msg::SetBlock { x, y, z, block } => events.push(Event::Block(x, y, z, block)),
                    Msg::Edits { list } => events.extend(list.into_iter().map(|(x, y, z, b)| Event::Block(x, y, z, b))),
                    Msg::Chat { id, text } => {
                        if id == 0 {
                            // From the server itself (an answer to a command).
                            if !text.is_empty() {
                                events.push(Event::Chat(alloc::format!("&E{}", text).to_ascii_uppercase()));
                            }
                        } else {
                            let who = self.players.get(&id).map_or("?", |p| p.name.as_str());
                            events.push(Event::Chat(alloc::format!("<{}> {}", who.to_ascii_uppercase(), text)));
                        }
                    }
                    Msg::Time { time } => events.push(Event::Time(crate::net::limits::clean_time(time))),
                    Msg::Info { build, release, commit } => {
                        let info = crate::net::version::Info { build, release, commit };
                        events.push(Event::Chat(alloc::format!("SERVER: {}", crate::net::version::describe(&info).to_ascii_uppercase())));
                        if let Some(line) = crate::net::version::compare(&crate::net::version::local(), &info) {
                            events.push(Event::Chat(line));
                        }
                        self.server_info = Some(info);
                    }
                    Msg::Perms { flags } => events.push(Event::Perms(flags)),
                    // Accounts: a challenge for /register or /pin, how it went, what the server says of us.
                    m @ (Msg::AuthChallenge { .. } | Msg::AuthResult { .. } | Msg::Account { .. }) => self.on_account_message(m, &mut events),
                    // (A skin's header must say a size a skin may have: anything else is not decoded at all.)
                    Msg::Skin { id, png, slim } if crate::net::limits::skin_header_ok(&png) => events.push(Event::Skin(id, png, slim)),
                    Msg::SkinCaps { max_side, max_bytes } => self.skin_caps = Some((max_side, max_bytes)),
                    // A big skin in pieces: only from players we know of, and each piece is checked on arrival.
                    Msg::SkinBegin { id, size, side, slim } => {
                        if self.players.contains_key(&id) {
                            let _ = self.skin_in.begin(id, size, side, slim);
                        }
                    }
                    Msg::SkinPart { id, last, data } => {
                        if let Ok(Some((png, slim))) = self.skin_in.part(id, &data, last) {
                            events.push(Event::Skin(id, png, slim));
                        }
                    }
                    // Health, ping and the like are kept here; the game does not see them as events.
                    Msg::State { id, channel: crate::proto::channel::VITALS, key, data, .. } => self.on_vitals_state(id, &key, &data),
                    Msg::State { id, channel, key, data, .. } => events.push(Event::State(id, channel, key, data)),
                    Msg::Pack { last, data } => events.push(Event::Pack(last, data)),
                    Msg::ModList { mods } => events.push(Event::ModList(mods)),
                    Msg::ModPart { index, last, data } => events.push(Event::ModPart(index, last, data)),
                    Msg::Voice { id, seq, gain, frame } => {
                        if self.voice_in.len() >= MAX_VOICE_WAITING {
                            self.voice_in.remove(0);
                        }
                        self.voice_in.push((id, seq, gain, frame));
                    }
                    _ => {}
                },
            }
        }
        // Players who stop moving stop walking.
        for p in self.players.values_mut() {
            p.stride *= 1.0 - (dt * 3.0).min(1.0);
            p.pace *= 1.0 - (dt * 0.5).min(1.0);
            p.body.update(crate::content::pose::unpack(p.pose).0, dt);
            // An emote runs on while they stand still, and stops when they move off.
            if p.emote != 0 {
                p.emote_t += dt;
                if p.stride > 0.3 {
                    p.emote = 0;
                }
            }
        }
        if !self.alive {
            events.push(Event::Lost);
        }
        events
    }

    /// Report where we are: ten times a second while moving, and every
    /// couple of seconds regardless.
    pub fn send_move(&mut self, dt: f32, pos: [f64; 3], yaw: f32, pitch: f32, held: u16) {
        self.send_timer += dt;
        self.refresh_timer += dt;
        let changed = self.last_sent != (pos, yaw, pitch, held, self.pose);
        if self.send_timer < 0.1 || (!changed && self.refresh_timer < 2.0) {
            return;
        }
        self.send_timer = 0.0;
        self.refresh_timer = 0.0;
        self.last_sent = (pos, yaw, pitch, held, self.pose);
        let id = self.id;
        let pose = self.pose;
        self.send(&Msg::Move { id, pos, yaw, pitch, held, pose });
    }

    /// Tell the others which emote this player is playing (0 = stopped).
    pub fn send_emote(&mut self, emote: u16) {
        let id = self.id;
        self.send(&Msg::Emote { id, emote });
    }

    /// Tell the others which status effects are on this player (see
    /// `game::effects::sync`).
    pub fn send_effects(&mut self, list: &[(String, u8, u32)]) {
        let id = self.id;
        self.send(&Msg::Effects { id, list: list.to_vec() });
    }

    pub fn send_block(&mut self, x: i32, y: i32, z: i32, block: u16) {
        self.send(&Msg::SetBlock { x, y, z, block });
    }

    /// Tell the others our skin. `hd` is the PNG at `side` pixels (the same
    /// as `small` for a 64 x 64 skin) and `small` the 64 x 64 version. A big
    /// skin goes in pieces if the server said it takes them (and is small
    /// enough for it); otherwise the 64 x 64 one goes. False (nothing sent)
    /// if the last skin went less than five seconds ago: ask again later.
    pub fn send_skins(&mut self, hd: &[u8], side: usize, small: &[u8], slim: bool) -> bool {
        if self.skin_cool > 0.0 {
            return false;
        }
        // A big skin waits a moment for the server to say whether it takes one (it says so right
        // after saying hello; a server that never does is sent the 64 x 64 version).
        if side > 64 && self.skin_caps.is_none() && self.age < 1.5 {
            return false;
        }
        self.skin_cool = crate::net::skinxfer::COOLDOWN;
        let id = self.id;
        let takes = |caps: Option<(u16, u32)>| caps.map_or(false, |(max_side, max_bytes)| side <= max_side as usize && hd.len() <= max_bytes as usize);
        if side > 64 && takes(self.skin_caps) && hd.len() <= crate::proto::MAX_SKIN {
            for m in crate::net::skinxfer::messages(id, hd, side, slim) {
                self.send(&m);
            }
        } else if small.len() <= crate::proto::MAX_SKIN_MSG {
            self.send(&Msg::Skin { id, png: small.to_vec(), slim });
        }
        true
    }

    /// Share a piece of state with the other players (see [`Msg::State`]).
    pub fn send_state(&mut self, channel: u8, keep: bool, key: &str, data: &[u8]) {
        let id = self.id;
        self.send(&Msg::State { id, channel, keep, key: String::from(key), data: data.to_vec() });
    }

    /// Whether this player is the one who runs the creatures and the like
    /// for everybody: the one the server named (the first to have come of
    /// those present), else the one with the lowest number.
    pub fn runs_the_world(&self) -> bool {
        self.is_the_runner()
    }

    /// Tell the server whether we want to hear voice chat and whether we
    /// may talk (`proto::VOICE_HEAR`, `VOICE_TALK`).
    pub fn send_voice_state(&mut self, flags: u8) {
        self.send(&Msg::VoiceState { flags });
    }

    /// Send one 20 ms frame of our voice; the server passes it on to whoever is near.
    pub fn send_voice(&mut self, seq: u16, frame: &[u8]) {
        let id = self.id;
        self.send(&Msg::Voice { id, seq, gain: 255, frame: frame.to_vec() });
    }

    /// Ask for these mods from the server's list (see [`Msg::ModsWant`]).
    pub fn send_mods_want(&mut self, names: &[String]) {
        self.send(&Msg::ModsWant { names: names.to_vec() });
    }

    pub fn send_chat(&mut self, text: &str) {
        let id = self.id;
        self.send(&Msg::Chat { id, text: String::from(text) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::{Config, Server};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[test]
    fn addresses_parse() {
        assert_eq!(parse_address("example.org:1234"), Some((String::from("example.org"), 1234)));
        assert_eq!(parse_address(" 10.0.0.5 "), Some((String::from("10.0.0.5"), crate::proto::DEFAULT_PORT)));
        assert_eq!(parse_address("host:notaport"), None);
        assert_eq!(parse_address(""), None);
        assert_eq!(parse_address(":5"), None);
        // IPv6 in brackets, with and without a port; a bare one is ambiguous.
        assert_eq!(parse_address("[::1]:7000"), Some((String::from("::1"), 7000)));
        assert_eq!(parse_address("[2001:db8::5]"), Some((String::from("2001:db8::5"), crate::proto::DEFAULT_PORT)));
        assert_eq!(parse_address("::1"), None);
        assert_eq!(parse_address("[::1"), None);
        assert_eq!(parse_address("[]:5"), None);
        assert_eq!(parse_address("[::1]x"), None);
        // Port 0, a sign, and one too big are no ports.
        assert_eq!(parse_address("host:0"), None);
        assert_eq!(parse_address("host:+80"), None);
        assert_eq!(parse_address("host:65536"), None);
        assert_eq!(parse_address("host:65535"), Some((String::from("host"), 65535)));
    }

    /// A mod of one big file (so that it comes in several pieces) and a pack.
    fn test_mod() -> crate::net::modsync::serve::ServedMod {
        let files = alloc::vec![(String::from("pack.txt"), b"version 1.5\nblock m\n".to_vec()), (String::from("textures/big.png"), (0..100_000u32).map(|i| (i % 251) as u8).collect())];
        let data = crate::net::modsync::bundle::encode(&files).unwrap();
        crate::net::modsync::serve::ServedMod { name: String::from("testmod"), version: String::from("1.5"), sum: crate::net::modsync::bundle::sum_of(&data), files: 2, data }
    }

    /// Run a server on a free port in a background thread.
    fn start_server(max_players: usize) -> (u16, Arc<AtomicBool>, std::thread::JoinHandle<Server>) {
        let cfg = Config {
            port: 0,
            seed: 4242,
            kind: String::from("normal"),
            max_players,
            content: content_checksum(),
            states: crate::block::reg().blocks.len() as u16,
            pack: b"block server_special\n tex stone\n".to_vec(),
            mods: alloc::vec![test_mod()],
        };
        let mut server = Server::bind(cfg).unwrap();
        let port = server.port();
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let handle = std::thread::spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                server.tick(0.005, &mut |line| if line == "TILE map 1 0 0" { Some(alloc::vec![7u8; 300]) } else { None });
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            server
        });
        (port, stop, handle)
    }

    fn pump(c: &mut Client, ms: u64) -> Vec<Event> {
        let mut all = Vec::new();
        for _ in 0..ms / 5 {
            all.extend(c.poll(0.005));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        all
    }

    #[test]
    fn mods_are_announced_and_sent_only_when_asked() {
        let (port, stop, handle) = start_server(8);
        let (mut c, _) = Client::connect("127.0.0.1", port, "Mia").unwrap();
        let events = pump(&mut c, 120);
        let list = events.iter().find_map(|e| if let Event::ModList(l) = e { Some(l.clone()) } else { None }).expect("no announcement");
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].name.as_str(), list[0].version.as_str()), ("testmod", "1.5"));
        // Nothing is sent unasked.
        assert!(!events.iter().any(|e| matches!(e, Event::ModPart(..))));
        c.send_mods_want(&[String::from("nosuchmod"), String::from("testmod"), String::from("testmod")]);
        let mut got: Vec<u8> = Vec::new();
        let mut ended = 0;
        for _ in 0..40 {
            for e in pump(&mut c, 50) {
                if let Event::ModPart(index, last, data) = e {
                    assert_eq!(index, 0);
                    got.extend_from_slice(&data);
                    ended += last as usize;
                }
            }
            if ended > 0 {
                break;
            }
        }
        assert_eq!(ended, 1, "the mod is sent once, whole");
        assert_eq!(got.len(), list[0].size as usize);
        assert_eq!(crate::net::modsync::bundle::sum_of(&got), list[0].sum);
        assert_eq!(crate::net::modsync::bundle::decode(&got).unwrap().len(), 2);
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn a_player_without_permission_cannot_change_the_world() {
        use crate::proto::*;
        let (port, stop, handle) = start_server(8);
        let (mut ann, _) = Client::connect("127.0.0.1", port, "Ann").unwrap();
        let ea = pump(&mut ann, 80);
        // The first player is the operator and may do everything.
        assert!(ea.iter().any(|e| matches!(e, Event::Perms(f) if *f == PERM_BUILD | PERM_GAMEMODE | PERM_COMMANDS | PERM_VOICE)), "no such event");
        let (mut bob, _) = Client::connect("127.0.0.1", port, "Bob").unwrap();
        let eb = pump(&mut bob, 100);
        // Everyone else may build, and nothing more, and starts in survival.
        assert!(eb.iter().any(|e| matches!(e, Event::Perms(f) if *f == PERM_BUILD | PERM_VOICE)), "no such event");
        pump(&mut ann, 40);
        ann.send_chat("/default -build");
        pump(&mut ann, 60);
        let eb = pump(&mut bob, 100);
        assert!(eb.iter().any(|e| matches!(e, Event::Perms(f) if *f == PERM_VOICE)), "no such event");
        // Bob's edit goes nowhere...
        let stone = crate::block::reg().wk.stone;
        bob.send_block(2, 70, 2, stone);
        pump(&mut bob, 60);
        let ea = pump(&mut ann, 100);
        assert!(!ea.iter().any(|e| matches!(e, Event::Block(2, 70, 2, _))), "no such event");
        // ... until Ann says he may.
        ann.send_chat("/perm Bob +build");
        pump(&mut ann, 60);
        let eb = pump(&mut bob, 100);
        assert!(eb.iter().any(|e| matches!(e, Event::Perms(f) if *f == PERM_BUILD | PERM_VOICE)), "no such event");
        bob.send_block(2, 70, 2, stone);
        pump(&mut bob, 60);
        let ea = pump(&mut ann, 100);
        assert!(ea.iter().any(|e| matches!(e, Event::Block(2, 70, 2, _))), "no such event");
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn poses_and_emotes_reach_the_other_players() {
        use crate::content::pose::{pack, unpack, Kind, SWING};
        let (port, stop, handle) = start_server(8);
        let (mut a, _) = Client::connect("127.0.0.1", port, "Ann").unwrap();
        let (mut b, _) = Client::connect("127.0.0.1", port, "Bob").unwrap();
        pump(&mut a, 80);
        pump(&mut b, 80);
        // Ann sits, swinging an arm.
        a.pose = pack(Kind::Sit, SWING);
        a.send_move(1.0, [3.5, 70.0, 9.5], 0.25, 0.0, 0);
        pump(&mut a, 40);
        pump(&mut b, 100);
        let seen = b.players.values().find(|p| p.name == "Ann").expect("Ann is there");
        assert_eq!(unpack(seen.pose), (Kind::Sit, SWING));
        // A change of pose alone is sent, even though she did not move.
        a.pose = pack(Kind::Stand, 0);
        a.send_move(1.0, [3.5, 70.0, 9.5], 0.25, 0.0, 0);
        pump(&mut b, 100);
        assert_eq!(unpack(b.players.values().find(|p| p.name == "Ann").unwrap().pose).0, Kind::Stand);
        // Her emote is relayed with her id, and stopping it too.
        a.send_emote(2);
        pump(&mut a, 40);
        let eb = pump(&mut b, 100);
        assert!(eb.iter().any(|e| matches!(e, Event::Emote(_, 2))), "no emote event");
        assert_eq!(b.players.values().find(|p| p.name == "Ann").unwrap().emote, 2);
        a.send_emote(0);
        pump(&mut b, 100);
        assert_eq!(b.players.values().find(|p| p.name == "Ann").unwrap().emote, 0);
        // The sender does not hear herself.
        let ea = pump(&mut a, 40);
        assert!(!ea.iter().any(|e| matches!(e, Event::Emote(..))));
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn effects_are_relayed_with_the_senders_id_and_told_to_those_who_join_later() {
        let (port, stop, handle) = start_server(8);
        let (mut a, _) = Client::connect("127.0.0.1", port, "Ann").unwrap();
        let (mut b, _) = Client::connect("127.0.0.1", port, "Bob").unwrap();
        pump(&mut a, 40);
        pump(&mut b, 100);
        a.send_effects(&[(String::from("poison"), 2, 9), (String::from("Bad Id"), 1, 1)]);
        pump(&mut a, 40);
        pump(&mut b, 100);
        // Only ids that could be typed are kept.
        assert_eq!(b.players.values().find(|p| p.name == "Ann").unwrap().effects, [(String::from("poison"), 2, 9)]);
        // The sender does not hear herself, and a newcomer is told.
        assert!(a.players.values().all(|p| p.effects.is_empty()));
        let (mut c, _) = Client::connect("127.0.0.1", port, "Cy").unwrap();
        pump(&mut c, 100);
        assert_eq!(c.players.values().find(|p| p.name == "Ann").unwrap().effects, [(String::from("poison"), 2, 9)]);
        // Clearing them is told too.
        a.send_effects(&[]);
        pump(&mut b, 100);
        assert!(b.players.values().find(|p| p.name == "Ann").unwrap().effects.is_empty());
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn two_players_see_each_other_build_and_chat() {
        let (port, stop, handle) = start_server(8);
        let (mut a, wa) = Client::connect("127.0.0.1", port, "Ann").unwrap();
        assert_eq!((wa.seed, wa.kind.as_str()), (4242, "normal"));
        // Ann builds before Bob arrives: he must still get that block.
        let stone = crate::block::reg().wk.stone;
        a.send_block(1, 70, -3, stone);
        a.send_block(1, 999, -3, stone); // out of the world: ignored
        a.send_block(1, 71, -3, 60_000); // no such block: ignored
        pump(&mut a, 60);
        let (mut b, _) = Client::connect("127.0.0.1", port, "Ann").unwrap();
        let eb = pump(&mut b, 120);
        let ea = pump(&mut a, 60);
        // Bob is handed the server's content pack, whole.
        let pack: Vec<u8> = eb.iter().filter_map(|e| if let Event::Pack(_, d) = e { Some(d.clone()) } else { None }).flatten().collect();
        assert_eq!(pack, b"block server_special\n tex stone\n");
        assert!(eb.iter().any(|e| matches!(e, Event::Pack(true, _))));
        // Bob sees Ann already there and gets the world's history.
        assert_eq!(b.players.len(), 1);
        assert_eq!(b.players.values().next().unwrap().name, "Ann");
        let blocks: Vec<_> = eb.iter().filter_map(|e| if let Event::Block(x, y, z, bl) = e { Some((*x, *y, *z, *bl)) } else { None }).collect();
        assert_eq!(blocks, [(1, 70, -3, stone)]);
        // Ann is told about the newcomer, who was renamed to avoid a clash.
        assert_eq!(a.players.values().next().unwrap().name, "Ann2");
        assert!(ea.iter().any(|e| matches!(e, Event::Chat(t) if t == "ANN2 JOINED")));

        // Someone joining later is told where a player who is standing
        // still already is.
        let (mut c, _) = Client::connect("127.0.0.1", port, "Cat").unwrap();
        a.send_move(1.0, [3.5, 70.0, 9.5], 0.25, 0.0, 0);
        pump(&mut a, 60);
        drop(c);
        pump(&mut a, 60);
        c = Client::connect("127.0.0.1", port, "Cat").unwrap().0;
        pump(&mut c, 120);
        let ann = c.players.values().find(|p| p.name == "Ann").expect("Ann missing");
        assert!(ann.seen && ann.pos == [3.5, 70.0, 9.5], "newcomer does not know where Ann is: {:?}", ann.pos);
        drop(c);
        pump(&mut a, 100);
        pump(&mut b, 100);

        // Movement, a live edit, and chat all cross over.
        a.send_move(1.0, [10.5, 64.0, -2.25], 1.0, -0.5, 0);
        a.send_block(5, 60, 5, 0);
        a.send_chat("hello bob");
        pump(&mut a, 40);
        let eb = pump(&mut b, 120);
        let ann = b.players.values().next().unwrap();
        assert_eq!((ann.pos, ann.yaw), ([10.5, 64.0, -2.25], 1.0));
        assert!(eb.iter().any(|e| matches!(e, Event::Block(5, 60, 5, 0))));
        assert!(eb.iter().any(|e| matches!(e, Event::Chat(t) if t == "<ANN> hello bob")));

        // Leaving is announced.
        drop(b);
        let ea = pump(&mut a, 150);
        assert!(ea.iter().any(|e| matches!(e, Event::Chat(t) if t == "ANN2 LEFT")), "no leave message");
        assert!(a.players.is_empty());

        stop.store(true, Ordering::Relaxed);
        let server = handle.join().unwrap();
        // Both edits are remembered and survive a save and load.
        assert_eq!(server.edits.len(), 2);
        let text = server.save_text();
        assert!(text.contains("1 70 -3 stone"), "{text}");
        let mut again = Server::bind(Config { port: 0, seed: 1, kind: String::new(), max_players: 1, content: 0, states: 1, pack: Vec::new(), mods: Vec::new() }).unwrap();
        again.load_text(&text);
        assert_eq!(again.edits, server.edits);
    }

    #[test]
    fn a_player_comes_back_where_they_left_off() {
        let (port, stop, handle) = start_server(4);
        let (mut r, _) = Client::connect("127.0.0.1", port, "Ref").unwrap();
        let (mut a, _) = Client::connect("127.0.0.1", port, "Dana").unwrap();
        pump(&mut a, 60);
        a.send_move(1.0, [33.5, 71.0, -12.25], 2.0, -0.25, 0);
        pump(&mut a, 60);
        drop(a);
        pump(&mut r, 100);
        // The server notes it as they leave; a different spelling of the name is the same player.
        let (mut b, _) = Client::connect("127.0.0.1", port, "dana").unwrap();
        let events = pump(&mut b, 200);
        let got = events.iter().find_map(|e| match e {
            Event::State(_, crate::proto::channel::PLAYER, key, data) if key == "you" => Some(String::from_utf8_lossy(data).into_owned()),
            _ => None,
        });
        assert_eq!(got.as_deref(), Some("33.5 71 -12.25 2 -0.25"), "{} events, none with the place", events.len());
        // A newcomer with no history is told nothing.
        let (mut c, _) = Client::connect("127.0.0.1", port, "Eve").unwrap();
        let events = pump(&mut c, 100);
        assert!(!events.iter().any(|e| matches!(e, Event::State(_, crate::proto::channel::PLAYER, ..))));
        stop.store(true, Ordering::Relaxed);
        let server = handle.join().unwrap();
        assert!(server.spots_text().starts_with("dana 33.5 71 -12.25 2 -0.25"));
        let mut again = Server::bind(Config { port: 0, seed: 1, kind: String::new(), max_players: 1, content: 0, states: 1, pack: Vec::new(), mods: Vec::new() }).unwrap();
        again.load_spots(&server.spots_text());
        assert_eq!(again.spots, server.spots);
    }

    #[test]
    fn full_servers_and_wrong_content_are_refused_and_tiles_are_relayed() {
        let (port, stop, handle) = start_server(1);
        let (_a, _) = Client::connect("127.0.0.1", port, "One").unwrap();
        let err = Client::connect("127.0.0.1", port, "Two").err().unwrap();
        assert_eq!(err, "THE SERVER IS FULL");
        // A relay request on a fresh connection.
        let mut s = TcpStream::connect("127.0.0.1", port, 2000).unwrap();
        s.send_all(b"TILE map 1 0 0\n", 2000).unwrap();
        let mut len = [0u8; 4];
        s.recv_exact(&mut len, 3000).unwrap();
        assert_eq!(u32::from_le_bytes(len), 300);
        let mut data = [0u8; 300];
        s.recv_exact(&mut data, 3000).unwrap();
        assert!(data.iter().all(|&b| b == 7));
        // Nobody answering at all.
        assert!(Client::connect("127.0.0.1", 1, "X").is_err());
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn voice_reaches_only_those_near_who_listen() {
        use crate::proto::*;
        let (port, stop, handle) = start_server(8);
        let (mut ann, _) = Client::connect("127.0.0.1", port, "Ann").unwrap();
        let (mut bob, _) = Client::connect("127.0.0.1", port, "Bob").unwrap();
        let (mut cy, _) = Client::connect("127.0.0.1", port, "Cy").unwrap();
        let (mut dee, _) = Client::connect("127.0.0.1", port, "Dee").unwrap();
        ann.send_move(1.0, [0.0, 64.0, 0.0], 0.0, 0.0, 0);
        bob.send_move(1.0, [5.0, 64.0, 0.0], 0.0, 0.0, 0);
        cy.send_move(1.0, [200.0, 64.0, 0.0], 0.0, 0.0, 0);
        dee.send_move(1.0, [6.0, 64.0, 0.0], 0.0, 0.0, 0);
        ann.send_voice_state(VOICE_HEAR | VOICE_TALK);
        bob.send_voice_state(VOICE_HEAR);
        cy.send_voice_state(VOICE_HEAR);
        // Dee is near but has voice off (never said otherwise).
        for c in [&mut ann, &mut bob, &mut cy, &mut dee] {
            pump(c, 80);
        }
        let frame = alloc::vec![0u8, 0, 10, 1, 2, 3, 4, 5];
        ann.send_voice(1, &frame);
        for c in [&mut ann, &mut bob, &mut cy, &mut dee] {
            pump(c, 80);
        }
        assert_eq!(bob.voice_in.len(), 1, "a listener nearby hears");
        let (id, seq, gain, got) = bob.voice_in[0].clone();
        assert_eq!((id, seq, got), (ann.id, 1, frame.clone()));
        assert!(gain > 0);
        assert!(cy.voice_in.is_empty(), "too far");
        assert!(dee.voice_in.is_empty(), "voice off");
        assert!(ann.voice_in.is_empty(), "never to the sender");
        // Someone without the mic switch on cannot talk.
        bob.send_voice(1, &frame);
        pump(&mut bob, 40);
        pump(&mut ann, 80);
        assert!(ann.voice_in.is_empty());
        // Muted players are not passed on; /unmute lets them through again.
        ann.send_chat("/mute Ann");
        pump(&mut ann, 80);
        ann.send_voice(2, &frame);
        pump(&mut ann, 40);
        pump(&mut bob, 80);
        assert_eq!(bob.voice_in.len(), 1, "muted: nothing new arrived");
        ann.send_chat("/unmute Ann");
        pump(&mut ann, 80);
        ann.send_voice(3, &frame);
        pump(&mut ann, 40);
        pump(&mut bob, 80);
        assert_eq!(bob.voice_in.len(), 2);
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn a_client_of_another_protocol_is_told_both_versions_and_who_must_update() {
        let (port, stop, handle) = start_server(4);
        let mut s = TcpStream::connect("127.0.0.1", port, 2000).unwrap();
        s.send_all(&Msg::Hello { version: 3, content: 0, name: String::from("Old") }.encode(), 2000).unwrap();
        let mut inbox = Inbox::default();
        let mut buf = [0u8; 512];
        let mut reason = None;
        for _ in 0..40 {
            if let Ok(n) = s.recv(&mut buf, 100) {
                inbox.push(&buf[..n]);
            }
            if let Next::Msg(Msg::Refuse { reason: r }) = inbox.next() {
                reason = Some(r);
                break;
            }
        }
        let reason = reason.expect("no refusal");
        assert!(reason.contains("your game is too old") && reason.contains("protocol 3") && reason.contains(&alloc::format!("speaks {}", VERSION)), "{reason}");
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn the_server_says_which_program_it_is_and_answers_version() {
        let (port, stop, handle) = start_server(4);
        let (mut a, _) = Client::connect("127.0.0.1", port, "Ann").unwrap();
        let events = pump(&mut a, 120);
        let info = a.server_info.clone().expect("no server info");
        assert_eq!(info, crate::net::version::local());
        assert!(events.iter().any(|e| matches!(e, Event::Chat(l) if l.starts_with("SERVER: "))));
        a.send_chat("/version");
        let events = pump(&mut a, 120);
        assert!(events.iter().any(|e| matches!(e, Event::Chat(l) if l.contains("PROTOCOL"))), "no answer");
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn a_flood_of_chat_is_cut_short_and_only_the_runner_may_send_creatures() {
        use crate::proto::channel;
        let (port, stop, handle) = start_server(4);
        let (mut a, _) = Client::connect("127.0.0.1", port, "Ann").unwrap();
        let (mut b, _) = Client::connect("127.0.0.1", port, "Bob").unwrap();
        pump(&mut a, 60);
        pump(&mut b, 100);
        for i in 0..60 {
            a.send_chat(&alloc::format!("spam {i}"));
        }
        let heard = pump(&mut b, 200).iter().filter(|e| matches!(e, Event::Chat(l) if l.contains("spam"))).count();
        assert!((1..=12).contains(&heard), "{heard} lines got through");
        // Bob is not the one who runs the creatures: his snapshot goes nowhere.
        b.send_state(channel::MOBS, false, "all", b"x");
        pump(&mut b, 40);
        let ea = pump(&mut a, 120);
        assert!(!ea.iter().any(|e| matches!(e, Event::State(_, channel::MOBS, ..))));
        a.send_state(channel::MOBS, false, "all", b"y");
        pump(&mut a, 40);
        let eb = pump(&mut b, 120);
        assert!(eb.iter().any(|e| matches!(e, Event::State(_, channel::MOBS, ..))));
        // A mod's message key keeps its separator on the way.
        a.send_state(channel::SCRIPT, false, "mymod\u{1f}score", b"7");
        pump(&mut a, 40);
        let eb = pump(&mut b, 120);
        assert!(eb.iter().any(|e| matches!(e, Event::State(_, channel::SCRIPT, k, _) if k == "mymod\u{1f}score")));
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn mod_messages_reach_the_others_or_one_player_and_are_checked_and_rationed() {
        use crate::modmsg::{pack, EVERYONE};
        use crate::proto::channel::MODMSG;
        let (port, stop, handle) = start_server(4);
        let (mut a, _) = Client::connect("127.0.0.1", port, "Ann").unwrap();
        let (mut b, _) = Client::connect("127.0.0.1", port, "Bob").unwrap();
        let (mut c, _) = Client::connect("127.0.0.1", port, "Cy").unwrap();
        pump(&mut a, 60);
        pump(&mut b, 60);
        pump(&mut c, 100);
        let count = |events: &[Event]| events.iter().filter(|e| matches!(e, Event::State(_, MODMSG, ..))).count();
        // To everyone else: Bob and Cy hear it, Ann does not hear herself.
        a.send_state(MODMSG, false, "storm\u{1f}phase", &pack(EVERYONE, "two").unwrap());
        pump(&mut a, 40);
        let (eb, ec, ea) = (pump(&mut b, 120), pump(&mut c, 120), pump(&mut a, 20));
        assert_eq!((count(&eb), count(&ec), count(&ea)), (1, 1, 0));
        assert!(eb.iter().any(|e| matches!(e, Event::State(id, MODMSG, k, d) if *id == a.id && k == "storm\u{1f}phase" && d.ends_with(b"two"))));
        // To one player.
        a.send_state(MODMSG, false, "storm\u{1f}phase", &pack(b.id, "psst").unwrap());
        pump(&mut a, 40);
        let (eb, ec) = (pump(&mut b, 120), pump(&mut c, 120));
        assert_eq!((count(&eb), count(&ec)), (1, 0));
        // Bad keys and text never leave the server.
        a.send_state(MODMSG, false, "nokey", &pack(EVERYONE, "x").unwrap());
        a.send_state(MODMSG, false, "storm\u{1f}phase", &[0xFF, 0xFF, 0x07]);
        a.send_state(MODMSG, false, "storm\u{1f}phase", &[0xFF]);
        let mut big = alloc::vec![0xFFu8, 0xFF];
        big.extend(core::iter::repeat(b'x').take(600));
        a.send_state(MODMSG, false, "storm\u{1f}phase", &big);
        pump(&mut a, 40);
        assert_eq!((count(&pump(&mut b, 120)), count(&pump(&mut c, 120))), (0, 0));
        // More than eight a second is cut.
        for i in 0..40 {
            a.send_state(MODMSG, false, "storm\u{1f}spam", &pack(EVERYONE, &alloc::format!("{i}")).unwrap());
        }
        pump(&mut a, 40);
        let heard = count(&pump(&mut b, 200));
        assert!((1..=10).contains(&heard), "{heard} messages got through");
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    /// A skin of `side` pixels, noisy enough to need several pieces on the wire.
    fn noisy_skin(side: usize) -> Vec<u8> {
        let mut x = 77u32;
        let px: Vec<u8> = (0..side * side * 4)
            .map(|i| {
                x = x.wrapping_mul(1664525).wrapping_add(1013904223);
                if i % 4 == 3 {
                    255
                } else {
                    (x >> 24) as u8
                }
            })
            .collect();
        crate::png::encode(side, side, &px)
    }

    #[test]
    fn a_big_skin_reaches_new_clients_whole_and_old_ones_as_64_by_64() {
        let (port, stop, handle) = start_server(8);
        let (mut a, _) = Client::connect("127.0.0.1", port, "Ann").unwrap();
        let (mut b, _) = Client::connect("127.0.0.1", port, "Bob").unwrap();
        pump(&mut a, 100);
        pump(&mut b, 100);
        assert!(a.skin_caps.is_some(), "the server says it takes big skins");
        let hd = noisy_skin(128);
        assert!(hd.len() > crate::proto::MAX_SKIN_PART);
        let small = crate::skinxfer::legacy_version(&hd, 128).unwrap();
        assert!(a.send_skins(&hd, 128, &small, true));
        // Not again within five seconds.
        assert!(!a.send_skins(&hd, 128, &small, true));
        pump(&mut a, 60);
        let eb = pump(&mut b, 250);
        let got = eb.iter().find_map(|e| if let Event::Skin(_, png, slim) = e { Some((png.clone(), *slim)) } else { None }).expect("Bob gets Ann's skin");
        assert_eq!(got, (hd.clone(), true));
        // A client of protocol 13 who joins now is sent the 64 x 64 one, and no message it does not know.
        let mut raw = TcpStream::connect("127.0.0.1", port, 2000).unwrap();
        raw.send_all(&Msg::Hello { version: crate::proto::MIN_VERSION, content: 0, name: String::from("Old") }.encode(), 2000).unwrap();
        let mut inbox = Inbox::default();
        let mut buf = [0u8; 8192];
        let mut skin = None;
        let mut unknown = false;
        for _ in 0..60 {
            if let Ok(n) = raw.recv(&mut buf, 20) {
                inbox.push(&buf[..n]);
            }
            loop {
                match inbox.next() {
                    Next::Msg(Msg::Skin { png, .. }) => skin = Some(png),
                    Next::Msg(Msg::SkinBegin { .. } | Msg::SkinPart { .. } | Msg::SkinCaps { .. }) => unknown = true,
                    Next::Msg(_) => {}
                    Next::Wait | Next::Bad => break,
                }
            }
            if skin.is_some() {
                break;
            }
        }
        assert!(!unknown, "an old client is never sent messages from protocol 14");
        assert_eq!(skin.as_deref(), Some(&small[..]));
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }

    #[test]
    fn a_skin_that_is_not_one_is_not_relayed() {
        let (port, stop, handle) = start_server(8);
        let (mut a, _) = Client::connect("127.0.0.1", port, "Ann").unwrap();
        let (mut b, _) = Client::connect("127.0.0.1", port, "Bob").unwrap();
        pump(&mut a, 80);
        pump(&mut b, 80);
        // A picture of 100 x 100 and a header that says a size no skin has.
        let odd = crate::png::encode(100, 100, &[9u8; 100 * 100 * 4]);
        a.send(&Msg::Skin { id: a.id, png: odd, slim: false });
        pump(&mut a, 40);
        assert!(!pump(&mut b, 120).iter().any(|e| matches!(e, Event::Skin(..))));
        // Pieces with no announcement, and one announced too big.
        let id = a.id;
        a.send(&Msg::SkinPart { id, last: true, data: alloc::vec![1; 500] });
        a.send(&Msg::SkinBegin { id, size: crate::proto::MAX_SKIN as u32 + 1, side: 128, slim: false });
        pump(&mut a, 40);
        assert!(!pump(&mut b, 120).iter().any(|e| matches!(e, Event::Skin(..))));
        // A proper one still goes through afterwards.
        let ok = crate::png::encode(64, 64, &[200u8; 64 * 64 * 4]);
        assert!(a.send_skins(&ok, 64, &ok, false));
        pump(&mut a, 40);
        assert!(pump(&mut b, 120).iter().any(|e| matches!(e, Event::Skin(..))));
        stop.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }
}
