//! Land from the server over real sockets on the local machine: a server with a
//! chunk source (a plain function, as the real one is the world generator),
//! clients of protocol 15 that ask, and raw connections that speak older
//! protocols or break the rules.

use super::client_state::Streamer;
use super::codec;
use super::gen::tests::land;
use crate::chunk::ChunkData;
use crate::netplay::{Client, Event};
use crate::proto::{Inbox, Msg, Next, CHUNKS_SERVED, VERSION};
use crate::server::{Config, Server};
use crate::sys::net::TcpStream;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const SEED: u32 = 4242;

/// A server on a free port in a background thread; with `serve` it makes land with [`land`],
/// and it starts out knowing `edits`.
fn start(serve: bool, edits: &[((i32, i32, i32), u16)]) -> (u16, Arc<AtomicBool>, std::thread::JoinHandle<Server>) {
    let cfg = Config { port: 0, seed: SEED, kind: String::from("somewhere 46.8 -121.7 30 75"), max_players: 8, content: crate::proto::content_checksum(), states: crate::block::reg().blocks.len() as u16, pack: Vec::new(), mods: Vec::new() };
    let mut server = Server::bind(cfg).unwrap();
    if serve {
        server.use_chunk_source(land, false);
    }
    for &(k, b) in edits {
        server.edits.insert(k, b);
    }
    let port = server.port();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let handle = std::thread::spawn(move || {
        while !stop2.load(Ordering::Relaxed) {
            server.tick(0.005, &mut |_| None);
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

/// A connection that is not a `Client`: it says hello as protocol `version` and reads what comes.
struct Raw {
    stream: TcpStream,
    inbox: Inbox,
}

impl Raw {
    fn new(port: u16) -> Raw {
        Raw { stream: TcpStream::connect("127.0.0.1", port, 2000).unwrap(), inbox: Inbox::default() }
    }

    fn hello(port: u16, version: u16, name: &str) -> Raw {
        let mut r = Raw::new(port);
        r.send(&Msg::Hello { version, content: 0, name: String::from(name) }.encode());
        r
    }

    fn send(&mut self, bytes: &[u8]) {
        let _ = self.stream.send_all(bytes, 2000);
    }

    /// Everything that arrives in about `ms` milliseconds.
    fn read(&mut self, ms: u32) -> Vec<Msg> {
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        for _ in 0..ms / 20 {
            if let Ok(n) = self.stream.recv(&mut buf, 20) {
                self.inbox.push(&buf[..n]);
            }
            while let Next::Msg(m) = self.inbox.next() {
                out.push(m);
            }
        }
        out
    }
}

fn expected(cx: i32, cz: i32, edits: &[(i32, i32, i32, u16)]) -> ChunkData {
    let mut c = land(SEED, cx, cz);
    codec::apply_edits(&mut c, edits);
    c
}

#[test]
fn a_client_that_cannot_make_the_land_is_sent_it_with_the_servers_edits_on_it() {
    let (port, stop, handle) = start(true, &[((3, 40, 4), 7), ((-1, 41, 33), 2)]);
    let (mut c, welcome) = Client::connect("127.0.0.1", port, "Ann").unwrap();
    let mut streamer = Streamer::new(&welcome.kind);
    for e in pump(&mut c, 200) {
        if let Event::ChunkCaps(flags, limit) = e {
            assert_eq!((flags, limit), (CHUNKS_SERVED, 30_000));
            streamer.on_caps(flags, limit);
        }
    }
    assert_eq!(streamer.mode, super::client_state::Mode::Streaming);
    for key in [(0, 0), (-1, 2), (5, 5)] {
        streamer.want(key);
    }
    for (cx, cz) in streamer.plan(0.1, (0, 0)) {
        c.send_chunk_request(cx, cz);
    }
    let mut got: BTreeMap<(i32, i32), ChunkData> = BTreeMap::new();
    for e in pump(&mut c, 300) {
        if let Event::Chunk(cx, cz, part, parts, data) = e {
            if let Some(whole) = streamer.on_part(cx, cz, part, parts, data) {
                got.insert((cx, cz), codec::unpack(&whole).expect("a chunk"));
            }
        }
    }
    assert_eq!(got.len(), 3);
    assert_eq!(got[&(0, 0)], expected(0, 0, &[(3, 40, 4, 7)]));
    assert_eq!(got[&(-1, 2)], expected(-1, 2, &[(15, 41, 1, 2)]));
    assert_eq!(got[&(5, 5)], expected(5, 5, &[]));
    assert_eq!(streamer.open(), 0);

    // A block changed by a player is on the chunk the next time it is asked for (what the
    // server kept of it is thrown away).
    c.send_block(1, 41, 1, 2);
    pump(&mut c, 60);
    c.send_chunk_request(0, 0);
    let mut again = None;
    for e in pump(&mut c, 300) {
        if let Event::Chunk(0, 0, 0, 1, data) = e {
            again = codec::unpack(&data);
        }
    }
    assert_eq!(again.expect("answered"), expected(0, 0, &[(3, 40, 4, 7), (1, 41, 1, 2)]));
    stop.store(true, Ordering::Relaxed);
    let server = handle.join().unwrap();
    assert!(server.serves_chunks());
}

#[test]
fn a_server_that_makes_no_land_says_so_and_ignores_requests() {
    let (port, stop, handle) = start(false, &[]);
    let (mut c, _) = Client::connect("127.0.0.1", port, "Ann").unwrap();
    let events = pump(&mut c, 200);
    assert!(events.iter().any(|e| matches!(e, Event::ChunkCaps(0, _))));
    c.send_chunk_request(0, 0);
    let after = pump(&mut c, 200);
    assert!(!after.iter().any(|e| matches!(e, Event::Chunk(..))));
    assert!(c.alive, "asking is no crime");
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

#[test]
fn clients_of_protocol_13_and_14_are_never_sent_land_or_a_word_about_it() {
    let (port, stop, handle) = start(true, &[]);
    for version in [13u16, 14] {
        let mut old = Raw::hello(port, version, &alloc::format!("Old{version}"));
        let before = old.read(300);
        assert!(before.iter().any(|m| matches!(m, Msg::Welcome { .. })), "joined as protocol {version}");
        assert!(!before.iter().any(|m| matches!(m, Msg::ChunkCaps { .. } | Msg::ChunkData { .. })), "nothing new for protocol {version}");
        // One that asks anyway is dropped, and gets no land.
        old.send(&Msg::ChunkRequest { cx: 0, cz: 0 }.encode());
        let after = old.read(300);
        assert!(!after.iter().any(|m| matches!(m, Msg::ChunkData { .. })));
    }
    // A client of today is told.
    let mut new = Raw::hello(port, VERSION, "New");
    assert!(new.read(300).iter().any(|m| matches!(m, Msg::ChunkCaps { flags, .. } if *flags == CHUNKS_SERVED)));
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

#[test]
fn a_connection_that_has_not_joined_gets_no_land() {
    let (port, stop, handle) = start(true, &[]);
    let mut stranger = Raw::new(port);
    stranger.send(&Msg::ChunkRequest { cx: 0, cz: 0 }.encode());
    assert!(!stranger.read(300).iter().any(|m| matches!(m, Msg::ChunkData { .. })));
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

#[test]
fn sixteen_at_once_and_only_land_within_thirty_thousand_blocks() {
    let (port, stop, handle) = start(true, &[]);
    let mut raw = Raw::hello(port, VERSION, "Fast");
    raw.read(300);
    // Sixty requests in one go: the server takes sixteen and ignores the rest.
    let mut burst = Vec::new();
    for i in 0..60 {
        burst.extend(Msg::ChunkRequest { cx: i, cz: 0 }.encode());
    }
    raw.send(&burst);
    let answers = raw.read(600);
    let mut got: Vec<i32> = answers.iter().filter_map(|m| if let Msg::ChunkData { cx, .. } = m { Some(*cx) } else { None }).collect();
    got.sort();
    assert_eq!(got.len(), 16, "{got:?}");
    assert_eq!(got, (0..16).collect::<Vec<i32>>(), "the first sixteen");
    // Their places are free again: more can be asked for. Land too far from the origin is not served.
    let mut more = Vec::new();
    more.extend(Msg::ChunkRequest { cx: 100_000, cz: 0 }.encode());
    more.extend(Msg::ChunkRequest { cx: 0, cz: -2000 }.encode());
    more.extend(Msg::ChunkRequest { cx: i32::MIN, cz: i32::MAX }.encode());
    more.extend(Msg::ChunkRequest { cx: 20, cz: 0 }.encode());
    raw.send(&more);
    let answers = raw.read(400);
    let got: Vec<(i32, i32)> = answers.iter().filter_map(|m| if let Msg::ChunkData { cx, cz, .. } = m { Some((*cx, *cz)) } else { None }).collect();
    assert_eq!(got, [(20, 0)]);
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}

#[test]
fn a_chunk_that_does_not_fit_in_one_message_comes_in_pieces_that_join_up() {
    // Land of all different blocks cannot be squeezed: it needs several pieces.
    fn noise(_seed: u32, cx: i32, cz: i32) -> ChunkData {
        let mut c = ChunkData::new();
        let mut s = (cx as u32).wrapping_mul(7919) ^ (cz as u32).wrapping_mul(104_729) ^ 0x1234_5678;
        let n = crate::block::reg().blocks.len() as u32;
        for b in c.blocks.iter_mut() {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            // (Blocks that are read back at rest, such as an open chest, are not used.)
            *b = crate::block::at_rest((s % n) as u16);
        }
        c
    }
    let cfg = Config { port: 0, seed: SEED, kind: String::from("somewhere"), max_players: 4, content: 0, states: 100, pack: Vec::new(), mods: Vec::new() };
    let mut server = Server::bind(cfg).unwrap();
    server.use_chunk_source(noise, false);
    let port = server.port();
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let handle = std::thread::spawn(move || {
        while !stop2.load(Ordering::Relaxed) {
            server.tick(0.005, &mut |_| None);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    });
    let (mut c, welcome) = Client::connect("127.0.0.1", port, "Ann").unwrap();
    let mut streamer = Streamer::new(&welcome.kind);
    for e in pump(&mut c, 200) {
        if let Event::ChunkCaps(flags, limit) = e {
            streamer.on_caps(flags, limit);
        }
    }
    streamer.want((2, 3));
    for (cx, cz) in streamer.plan(0.1, (0, 0)) {
        c.send_chunk_request(cx, cz);
    }
    let mut pieces = 0;
    let mut whole = None;
    for e in pump(&mut c, 800) {
        if let Event::Chunk(cx, cz, part, parts, data) = e {
            pieces += 1;
            assert!(data.len() <= crate::proto::MAX_CHUNK_PART);
            assert!(parts > 1 || part == 0);
            whole = streamer.on_part(cx, cz, part, parts, data).or(whole);
        }
    }
    assert!(pieces > 1, "{pieces} piece");
    assert_eq!(codec::unpack(&whole.expect("all the pieces came")).unwrap(), noise(0, 2, 3));
    stop.store(true, Ordering::Relaxed);
    handle.join().unwrap();
}
