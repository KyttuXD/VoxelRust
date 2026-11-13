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
    /// World kind as text (`normal`, or `earth <lat> <lon> <scale> <vscale>`).
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
}

struct Conn {
    stream: TcpStream,
    inbox: Inbox,
    id: u16,
    name: String,
    joined: bool,
    /// Where this player last said they were, for telling newcomers.
    last_move: Option<([f64; 3], f32, f32)>,
    skin: Vec<u8>,
    /// Ticks since anything arrived; idle strangers are dropped.
    quiet: u32,
    gone: bool,
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
}

const SEND_TIMEOUT: u32 = 3000;

impl Server {
    pub fn bind(cfg: Config) -> Result<Server, NetError> {
        let listener = TcpListener::bind(cfg.port)?;
        Ok(Server { listener, conns: Vec::new(), edits: BTreeMap::new(), cfg, next_id: 1, time: 0.12, time_timer: 0.0, dirty: false })
    }

    pub fn port(&self) -> u16 {
        self.listener.local_port()
    }

    pub fn players(&self) -> usize {
        self.conns.iter().filter(|c| c.joined && !c.gone).count()
    }

    fn broadcast(&mut self, msg: &Msg, except: u16) {
        let bytes = msg.encode();
        for c in self.conns.iter_mut().filter(|c| c.joined && !c.gone && c.id != except) {
            if c.stream.send_all(&bytes, SEND_TIMEOUT).is_err() {
                c.gone = true;
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
        for _ in 0..8 {
            match self.listener.accept(0) {
                Ok(Some(mut stream)) => {
                    stream.set_nodelay(true);
                    self.conns.push(Conn { stream, inbox: Inbox::default(), id: 0, name: String::new(), joined: false, last_move: None, skin: Vec::new(), quiet: 0, gone: false });
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
                        // Tell the newcomer about the world and who is here.
                        let mut out = Msg::Welcome { id, seed: self.cfg.seed, time: self.time, kind: self.cfg.kind.clone() }.encode();
                        // The content pack first (even when empty), so the
                        // edits that follow are read with the right blocks.
                        let pack = &self.cfg.pack[..];
                        let parts: Vec<&[u8]> = if pack.is_empty() { alloc::vec![&pack[..]] } else { pack.chunks(crate::proto::MAX_PACK_PART).collect() };
                        for (k, part) in parts.iter().enumerate() {
                            out.extend(Msg::Pack { last: k + 1 == parts.len(), data: part.to_vec() }.encode());
                        }
                        for other in self.conns.iter().filter(|c| c.joined && !c.gone) {
                            out.extend(Msg::Join { id: other.id, name: other.name.clone() }.encode());
                            if !other.skin.is_empty() {
                                out.extend(Msg::Skin { id: other.id, png: other.skin.clone() }.encode());
                            }
                            if let Some((pos, yaw, pitch)) = other.last_move {
                                out.extend(Msg::Move { id: other.id, pos, yaw, pitch }.encode());
                            }
                        }
                        let all: Vec<(i32, i32, i32, u16)> = self.edits.iter().map(|(k, &b)| (k.0, k.1, k.2, b)).collect();
                        for part in all.chunks(4000) {
                            out.extend(Msg::Edits { list: part.to_vec() }.encode());
                        }
                        if self.conns[i].stream.send_all(&out, 4_000).is_err() {
                            self.conns[i].gone = true;
                            break;
                        }
                        self.conns[i].id = id;
                        self.conns[i].name = name.clone();
                        self.conns[i].joined = true;
                        log.push(alloc::format!("{} joined ({} online)", name, self.players()));
                        self.broadcast(&Msg::Join { id, name }, id);
                    }
                    Msg::Move { pos, yaw, pitch, .. } if joined => {
                        self.conns[i].last_move = Some((pos, yaw, pitch));
                        self.broadcast(&Msg::Move { id, pos, yaw, pitch }, id);
                    }
                    Msg::SetBlock { x, y, z, block } if joined => {
                        if (0..CH).contains(&y) && block < self.cfg.states {
                            self.edits.insert((x, y, z), block);
                            self.dirty = true;
                            self.broadcast(&Msg::SetBlock { x, y, z, block }, id);
                        }
                    }
                    Msg::Skin { png, .. } if joined => {
                        self.conns[i].skin = png.clone();
                        self.broadcast(&Msg::Skin { id, png }, id);
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
