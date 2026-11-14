//! The sync of shared games over real sockets on this machine: a server in a
//! background thread (whose clock can be sped up to make seconds pass), and
//! clients that connect to it.

use super::vitals::Vitals;
use crate::netplay::{Client, Event};
use crate::proto::channel::{PLAYER, VITALS};
use crate::server::{Config, Server};
use alloc::string::String;
use alloc::vec::Vec;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

struct Running {
    port: u16,
    stop: Arc<AtomicBool>,
    /// The step of the server's clock each tick, as the bits of an f32.
    dt: Arc<AtomicU32>,
    handle: std::thread::JoinHandle<Server>,
}

impl Running {
    fn start(max_players: usize) -> Running {
        let cfg = Config { port: 0, seed: 7, kind: String::from("normal"), max_players, content: crate::proto::content_checksum(), states: crate::block::reg().blocks.len() as u16, pack: Vec::new(), mods: Vec::new() };
        let mut server = Server::bind(cfg).unwrap();
        let port = server.port();
        let stop = Arc::new(AtomicBool::new(false));
        let dt = Arc::new(AtomicU32::new(0.005f32.to_bits()));
        let (stop2, dt2) = (stop.clone(), dt.clone());
        let handle = std::thread::spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                server.tick(f32::from_bits(dt2.load(Ordering::Relaxed)), &mut |_| None);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            server
        });
        Running { port, stop, dt, handle }
    }

    fn join(&self, name: &str) -> Client {
        Client::connect("127.0.0.1", self.port, name).unwrap().0
    }

    /// Let `secs` of the server's time pass in a moment (then back to real speed).
    fn fast_forward(&self, c: &mut [&mut Client], ms: u64) {
        self.dt.store(0.5f32.to_bits(), Ordering::Relaxed);
        for _ in 0..ms / 5 {
            for client in c.iter_mut() {
                client.poll(0.005);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        self.dt.store(0.005f32.to_bits(), Ordering::Relaxed);
    }

    fn finish(self) -> Server {
        self.stop.store(true, Ordering::Relaxed);
        self.handle.join().unwrap()
    }
}

fn pump(c: &mut Client, ms: u64) -> Vec<Event> {
    let mut all = Vec::new();
    for _ in 0..ms / 5 {
        all.extend(c.poll(0.005));
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    all
}

fn chat_lines(events: &[Event]) -> Vec<String> {
    events.iter().filter_map(|e| if let Event::Chat(l) = e { Some(l.clone()) } else { None }).collect()
}

fn named<'a>(c: &'a Client, name: &str) -> Option<&'a crate::netplay::Remote> {
    c.players.values().find(|p| p.name == name)
}

const WELL: Vitals = Vitals { hp: 13, max: 40, food: 17, worn: [3, 0, 9, 1], ping: 0 };

#[test]
fn health_hunger_and_armour_reach_the_others_and_newcomers_and_nonsense_does_not() {
    let s = Running::start(8);
    let (mut a, mut b) = (s.join("Ann"), s.join("Bob"));
    pump(&mut a, 60);
    pump(&mut b, 60);
    a.send_vitals(&WELL);
    pump(&mut b, 100);
    assert_eq!(named(&b, "Ann").unwrap().vitals, Some(WELL));
    // Nothing is told back to the one who sent it.
    pump(&mut a, 50);
    assert!(named(&a, "Bob").unwrap().vitals.is_none());
    // A message with numbers out of range is not taken (and does not cut the connection).
    a.send_state(VITALS, false, "me", &[99, 20, 5, 0, 0, 0, 0, 0, 0, 0, 0]);
    a.send_state(VITALS, false, "me", &[1, 2, 3]);
    pump(&mut b, 100);
    assert_eq!(named(&b, "Ann").unwrap().vitals, Some(WELL));
    assert!(a.alive);
    // A newcomer is told at once.
    let mut c = s.join("Cy");
    pump(&mut c, 120);
    assert_eq!(named(&c, "Ann").unwrap().vitals, Some(WELL));
    assert!(named(&c, "Bob").unwrap().vitals.is_none(), "Bob has said nothing yet");
    s.finish();
}

#[test]
fn a_death_is_announced_once_and_a_live_player_is_not_called_dead() {
    let s = Running::start(8);
    let (mut a, mut b) = (s.join("Ann"), s.join("Bob"));
    pump(&mut a, 60);
    pump(&mut b, 60);
    a.send_vitals(&WELL);
    a.send_died("fall");
    a.send_died("fall");
    let eb = pump(&mut b, 150);
    let ea = pump(&mut a, 50);
    assert_eq!(chat_lines(&eb).iter().filter(|l| l.contains("ANN FELL FROM A HIGH PLACE")).count(), 1, "{:?}", chat_lines(&eb));
    assert_eq!(chat_lines(&ea).iter().filter(|l| l.contains("FELL FROM")).count(), 1);
    // Others see them at no health until they say they are well again.
    assert_eq!(named(&b, "Ann").unwrap().vitals.map(|v| v.hp), Some(0));
    a.send_vitals(&Vitals { hp: 40, ..WELL });
    pump(&mut b, 100);
    assert_eq!(named(&b, "Ann").unwrap().vitals.map(|v| v.hp), Some(40));
    s.finish();
}

#[test]
fn the_last_lines_of_chat_are_told_to_a_newcomer() {
    let s = Running::start(8);
    let mut a = s.join("Ann");
    pump(&mut a, 60);
    a.send_chat("hello there");
    a.send_chat("anyone for mining?");
    pump(&mut a, 100);
    let mut b = s.join("Bob");
    let eb = pump(&mut b, 150);
    let lines = chat_lines(&eb);
    let at = lines.iter().position(|l| l.contains("RECENT CHAT")).expect("a header");
    assert!(lines[at + 1].contains("<ANN> HELLO THERE") && lines[at + 2].contains("ANYONE FOR MINING"), "{lines:?}");
    // Nothing yet: a server nobody has spoken on says nothing.
    s.finish();
    let s = Running::start(8);
    let mut c = s.join("Cy");
    assert!(!chat_lines(&pump(&mut c, 100)).iter().any(|l| l.contains("RECENT CHAT")));
    s.finish();
}

#[test]
fn health_comes_back_with_the_player_unless_they_died() {
    let s = Running::start(8);
    let mut watcher = s.join("Ref");
    let mut d = s.join("Dana");
    pump(&mut d, 60);
    d.send_move(1.0, [33.5, 71.0, -12.25], 2.0, -0.25, 0);
    d.send_vitals(&Vitals { hp: 7, max: 20, food: 12, worn: [0; 4], ping: 0 });
    pump(&mut d, 60);
    drop(d);
    pump(&mut watcher, 100);
    let mut d2 = s.join("dana");
    let events = pump(&mut d2, 200);
    let health = events.iter().find_map(|e| match e {
        Event::State(_, PLAYER, key, data) if key == "health" => Some(String::from_utf8_lossy(data).into_owned()),
        _ => None,
    });
    assert_eq!(health.as_deref(), Some("7 20 12"));
    assert!(events.iter().any(|e| matches!(e, Event::State(_, PLAYER, key, _) if key == "you")));
    // A player who dies and leaves while dead starts again at the spawn: no place, no health.
    d2.send_move(1.0, [40.5, 71.0, 3.0], 0.0, 0.0, 0);
    d2.send_vitals(&Vitals { hp: 0, max: 20, food: 12, worn: [0; 4], ping: 0 });
    d2.send_died("drown");
    pump(&mut d2, 80);
    drop(d2);
    pump(&mut watcher, 100);
    let mut d3 = s.join("Dana");
    let events = pump(&mut d3, 200);
    assert!(!events.iter().any(|e| matches!(e, Event::State(_, PLAYER, ..))), "a dead player is told nothing of where or how they were");
    // It is kept in the spots file with the place.
    d3.send_move(1.0, [1.5, 70.0, 2.5], 0.0, 0.0, 0);
    d3.send_vitals(&Vitals { hp: 9, max: 30, food: 4, worn: [0; 4], ping: 0 });
    pump(&mut d3, 80);
    drop(d3);
    pump(&mut watcher, 100);
    let server = s.finish();
    let text = server.spots_text();
    assert!(text.contains("dana 1.5 70 2.5 0 0 9 30 4"), "{text}");
    let mut again = Server::bind(Config { port: 0, seed: 1, kind: String::new(), max_players: 1, content: 0, states: 1, pack: Vec::new(), mods: Vec::new() }).unwrap();
    again.load_spots(&text);
    assert_eq!(again.spots_text(), text);
    // Files from before health was kept still load.
    again.load_spots("old 1 2 3 4 5\nbad 1 2 3 4 5 6 7\nbroken 1 2 3 4 5 x 7 8\n");
    assert!(again.spots.contains_key("old") && !again.spots.contains_key("bad"));
}

#[test]
fn a_guest_who_comes_back_replaces_their_quiet_old_connection_but_not_a_live_one() {
    let s = Running::start(2);
    let mut a = s.join("Ann");
    let mut b = s.join("Bob");
    pump(&mut a, 60);
    pump(&mut b, 60);
    // Ann's connection is live: another Ann from this address is not her, and the server is full.
    assert!(Client::connect("127.0.0.1", s.port, "Ann").is_err(), "full");
    // Her game stops saying anything (a lost network) while the server's clock runs on.
    s.fast_forward(&mut [&mut b], 100);
    let mut a2 = s.join("Ann");
    let ea2 = pump(&mut a2, 150);
    assert!(!ea2.iter().any(|e| matches!(e, Event::Lost)));
    // Bob sees one Ann, with her own name, and the old connection was let go.
    let eb = pump(&mut b, 100);
    assert_eq!(b.players.values().filter(|p| p.name == "Ann").count(), 1, "{:?}", b.players.values().map(|p| p.name.clone()).collect::<Vec<_>>());
    assert!(!b.players.values().any(|p| p.name == "Ann2"));
    assert!(chat_lines(&eb).iter().any(|l| l.contains("ANN LEFT")));
    let ea = pump(&mut a, 100);
    assert!(ea.iter().any(|e| matches!(e, Event::Lost)) || !a.alive);
    s.finish();
}

#[test]
fn when_the_runner_leaves_the_next_to_have_come_runs_the_creatures() {
    let s = Running::start(8);
    let mut a = s.join("Ann");
    let mut b = s.join("Bob");
    let mut c = s.join("Cy");
    for client in [&mut a, &mut b, &mut c] {
        pump(client, 80);
    }
    assert!(a.runs_the_world() && !b.runs_the_world() && !c.runs_the_world());
    assert_eq!(b.runner, Some(a.id));
    drop(a);
    pump(&mut b, 150);
    pump(&mut c, 50);
    assert!(b.runs_the_world() && !c.runs_the_world());
    assert_eq!(c.runner, Some(b.id));
    // A newcomer is told who it is at once.
    let mut d = s.join("Dee");
    pump(&mut d, 120);
    assert_eq!(d.runner, Some(b.id));
    assert!(!d.runs_the_world());
    s.finish();
}

#[test]
fn two_players_changing_one_block_at_once_end_with_the_same_one() {
    let s = Running::start(8);
    let (mut a, mut b) = (s.join("Ann"), s.join("Bob"));
    pump(&mut a, 60);
    pump(&mut b, 60);
    a.send_block(5, 70, 5, 1);
    pump(&mut a, 30);
    b.send_block(5, 70, 5, 2);
    let eb = pump(&mut b, 120);
    let ea = pump(&mut a, 120);
    let last = |events: &[Event]| events.iter().filter_map(|e| if let Event::Block(5, 70, 5, k) = e { Some(*k) } else { None }).last();
    assert_eq!(last(&ea), Some(2), "Ann ends with what Bob placed after her");
    assert_eq!(last(&eb), Some(2), "and so does Bob, though he heard Ann's first");
    // Later changes by one player alone are not echoed to them.
    std::thread::sleep(std::time::Duration::from_millis(50));
    a.send_block(6, 70, 6, 1);
    assert!(!pump(&mut a, 100).iter().any(|e| matches!(e, Event::Block(6, 70, 6, _))));
    s.finish();
}

#[test]
fn a_ping_is_measured_for_players_who_know_the_channel_and_shown_to_the_others() {
    let s = Running::start(8);
    let (mut a, mut b) = (s.join("Ann"), s.join("Bob"));
    pump(&mut a, 60);
    pump(&mut b, 60);
    a.send_vitals(&WELL);
    pump(&mut a, 40);
    // Seconds pass on the server: it asks, Ann's game answers as it polls.
    s.fast_forward(&mut [&mut a, &mut b], 150);
    assert!(a.own_ping > 0, "the server told her her ping");
    a.send_vitals(&Vitals { hp: 12, ..WELL });
    pump(&mut b, 100);
    let seen = named(&b, "Ann").unwrap().vitals.unwrap();
    assert_eq!(seen.hp, 12);
    assert!(seen.ping > 0, "{seen:?}");
    // Bob never sent his health: the server did not ask him anything.
    assert_eq!(b.own_ping, 0);
    s.finish();
}

#[test]
fn an_emote_being_played_is_shown_to_a_newcomer() {
    let s = Running::start(8);
    let mut a = s.join("Ann");
    pump(&mut a, 60);
    a.send_emote(3);
    pump(&mut a, 60);
    let mut b = s.join("Bob");
    pump(&mut b, 150);
    assert_eq!(named(&b, "Ann").unwrap().emote, 3);
    s.finish();
}
