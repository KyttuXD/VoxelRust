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
    /// False until their first position arrives.
    pub seen: bool,
}

pub enum Event {
    Block(i32, i32, i32, u16),
    /// A line for the chat log.
    Chat(String),
    Time(f32),
    /// A piece of the server's content pack; the flag marks the last one.
    Pack(bool, Vec<u8>),
    /// A player's skin image arrived, and whether their arms are slim.
    Skin(u16, Vec<u8>, bool),
    /// Shared state from another player: who, channel, key, data.
    State(u16, u8, String, Vec<u8>),
    /// The connection is gone.
    Lost,
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
    last_sent: ([f64; 3], f32, f32, u16),
    pub alive: bool,
}

/// Split `host:port`; the port may be left out.
pub fn parse_address(addr: &str) -> Option<(String, u16)> {
    let addr = addr.trim();
    if addr.is_empty() {
        return None;
    }
    match addr.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && !h.contains(':') => Some((String::from(h), p.parse().ok()?)),
        Some(_) => None,
        None => Some((String::from(addr), crate::proto::DEFAULT_PORT)),
    }
}

impl Client {
    /// Connect and say hello. Waits (a few seconds at most) for the answer.
    pub fn connect(host: &str, port: u16, name: &str) -> Result<(Client, Welcome), String> {
        let mut stream = TcpStream::connect(host, port, 6000).map_err(|e| alloc::format!("COULD NOT CONNECT ({:?})", e).to_ascii_uppercase())?;
        stream.set_nodelay(true);
        let hello = Msg::Hello { version: VERSION, content: content_checksum(), name: String::from(name) };
        stream.send_all(&hello.encode(), 4000).map_err(|_| String::from("THE SERVER HUNG UP"))?;
        let mut inbox = Inbox::default();
        let mut buf = [0u8; 4096];
        for _ in 0..80 {
            match inbox.next() {
                Next::Msg(Msg::Welcome { id, seed, time, kind }) => {
                    let c = Client { stream, inbox, id, players: BTreeMap::new(), send_timer: 0.0, refresh_timer: 0.0, last_sent: ([0.0; 3], 0.0, 0.0, 0), alive: true };
                    return Ok((c, Welcome { seed, time, kind }));
                }
                Next::Msg(Msg::Refuse { reason }) => return Err(reason.to_ascii_uppercase()),
                Next::Msg(_) | Next::Bad => return Err(String::from("THAT IS NOT A VOXELRUST SERVER")),
                Next::Wait => {}
            }
            match stream.recv(&mut buf, 100) {
                Ok(n) => inbox.push(&buf[..n]),
                Err(_) => return Err(String::from("THE SERVER HUNG UP")),
            }
        }
        Err(String::from("THE SERVER DID NOT ANSWER"))
    }

    fn send(&mut self, msg: &Msg) {
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
                        self.players.insert(id, Remote { name, pos: [0.0; 3], yaw: 0.0, pitch: 0.0, held: 0, phase: 0.0, stride: 0.0, seen: false });
                    }
                    Msg::Leave { id } => {
                        if let Some(p) = self.players.remove(&id) {
                            events.push(Event::Chat(alloc::format!("{} LEFT", p.name.to_ascii_uppercase())));
                        }
                    }
                    Msg::Move { id, pos, yaw, pitch, held } => {
                        if let Some(p) = self.players.get_mut(&id) {
                            p.held = held;
                            let (dx, dz) = ((pos[0] - p.pos[0]) as f32, (pos[2] - p.pos[2]) as f32);
                            let step = crate::math::sqrtf(dx * dx + dz * dz);
                            if p.seen {
                                p.phase += step * 3.2;
                                p.stride = (step * 2.5).min(1.0);
                            }
                            p.pos = pos;
                            p.yaw = yaw;
                            p.pitch = pitch;
                            p.seen = true;
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
                            events.push(Event::Chat(alloc::format!("<{}> {}", who, text).to_ascii_uppercase()));
                        }
                    }
                    Msg::Time { time } => events.push(Event::Time(time)),
                    Msg::Skin { id, png, slim } => events.push(Event::Skin(id, png, slim)),
                    Msg::State { id, channel, key, data, .. } => events.push(Event::State(id, channel, key, data)),
                    Msg::Pack { last, data } => events.push(Event::Pack(last, data)),
                    _ => {}
                },
            }
        }
        // Players who stop moving stop walking.
        for p in self.players.values_mut() {
            p.stride *= 1.0 - (dt * 3.0).min(1.0);
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
        let changed = self.last_sent != (pos, yaw, pitch, held);
        if self.send_timer < 0.1 || (!changed && self.refresh_timer < 2.0) {
            return;
        }
        self.send_timer = 0.0;
        self.refresh_timer = 0.0;
        self.last_sent = (pos, yaw, pitch, held);
        let id = self.id;
        self.send(&Msg::Move { id, pos, yaw, pitch, held });
    }

    pub fn send_block(&mut self, x: i32, y: i32, z: i32, block: u16) {
        self.send(&Msg::SetBlock { x, y, z, block });
    }

    pub fn send_skin(&mut self, png: &[u8], slim: bool) {
        let id = self.id;
        self.send(&Msg::Skin { id, png: png.to_vec(), slim });
    }

    /// Share a piece of state with the other players (see [`Msg::State`]).
    pub fn send_state(&mut self, channel: u8, keep: bool, key: &str, data: &[u8]) {
        let id = self.id;
        self.send(&Msg::State { id, channel, keep, key: String::from(key), data: data.to_vec() });
    }

    /// Whether this player is the one who runs the creatures and the like
    /// for everybody: the one with the lowest number of those present.
    pub fn runs_the_world(&self) -> bool {
        self.players.keys().all(|&other| self.id < other)
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
        assert!(eb.iter().any(|e| matches!(e, Event::Chat(t) if t == "<ANN> HELLO BOB")));

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
        let mut again = Server::bind(Config { port: 0, seed: 1, kind: String::new(), max_players: 1, content: 0, states: 1, pack: Vec::new() }).unwrap();
        again.load_text(&text);
        assert_eq!(again.edits, server.edits);
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
}
