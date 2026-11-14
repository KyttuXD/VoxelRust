//! Player versus player through a real server and real clients. The server's
//! clock (the one cooldowns and spawn protection use) is set by the test, so
//! nothing here waits for time to pass.

use super::wire::{Gear, Hit, Hurt, Seen};
use crate::netplay::{Client, Event};
use crate::proto::{channel, content_checksum};
use crate::server::{Config, Server};
use alloc::string::String;
use alloc::vec::Vec;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

struct Arena {
    port: u16,
    stop: Arc<AtomicBool>,
    /// The server's clock in milliseconds.
    clock: Arc<AtomicU64>,
    handle: std::thread::JoinHandle<Server>,
}

impl Arena {
    fn start(setup: impl FnOnce(&mut Server)) -> Arena {
        let cfg = Config { port: 0, seed: 5, kind: String::from("normal"), max_players: 8, content: content_checksum(), states: crate::block::reg().blocks.len() as u16, pack: Vec::new(), mods: Vec::new() };
        let mut server = Server::bind(cfg).unwrap();
        setup(&mut server);
        let port = server.port();
        let stop = Arc::new(AtomicBool::new(false));
        let clock = Arc::new(AtomicU64::new(0));
        let (stop2, clock2) = (stop.clone(), clock.clone());
        let handle = std::thread::spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                server.pvp.now = clock2.load(Ordering::Relaxed) as f64 / 1000.0;
                server.tick(0.0, &mut |_| None);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            server
        });
        Arena { port, stop, clock, handle }
    }

    fn at(&self, secs: f64) {
        self.clock.store((secs * 1000.0) as u64, Ordering::Relaxed);
    }

    fn join(&self, name: &str, pos: [f64; 3]) -> Client {
        let (mut c, _) = Client::connect("127.0.0.1", self.port, name).unwrap();
        pump(&mut c, 60);
        c.send_move(1.0, pos, 0.0, 0.0, 0);
        c.send_state(channel::PVP_GEAR, false, "", &Gear::default().pack());
        c
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

fn hurts(events: &[Event]) -> Vec<Hurt> {
    events.iter().filter_map(|e| if let Event::State(0, channel::PVP_HURT, _, d) = e { Hurt::unpack(d) } else { None }).collect()
}

fn seen(events: &[Event]) -> Vec<Seen> {
    events.iter().filter_map(|e| if let Event::State(0, channel::PVP_SEEN, _, d) = e { Seen::unpack(d) } else { None }).collect()
}

fn chats(events: &[Event]) -> Vec<String> {
    events.iter().filter_map(|e| if let Event::Chat(t) = e { Some(t.clone()) } else { None }).collect()
}

fn swing(c: &mut Client, target: u16, damage: u8) {
    c.send_state(channel::PVP_HIT, false, "", &Hit { target, item: 0, damage }.pack());
}

/// Ann (an operator, the first in), Bob two blocks east of her, Cy a little further; everyone settled.
fn trio(a: &Arena) -> (Client, Client, Client) {
    let mut ann = a.join("Ann", [0.0, 64.0, 0.0]);
    let mut bob = a.join("Bob", [2.0, 64.0, 0.0]);
    let mut cy = a.join("Cy", [3.0, 64.0, 2.0]);
    for c in [&mut ann, &mut bob, &mut cy] {
        pump(c, 120);
    }
    (ann, bob, cy)
}

fn no_protection(s: &mut Server) {
    s.pvp.rules.protect = 0.0;
}

#[test]
fn a_hit_in_range_hurts_the_target_and_the_others_see_it() {
    let a = Arena::start(no_protection);
    let (mut ann, mut bob, mut cy) = trio(&a);
    a.at(100.0);
    swing(&mut ann, bob.id, 2);
    let (eb, ea, ec) = (pump(&mut bob, 120), pump(&mut ann, 40), pump(&mut cy, 40));
    let h = hurts(&eb);
    assert_eq!(h.len(), 1, "{:?}", chats(&eb));
    assert_eq!((h[0].attacker, h[0].damage), (ann.id, 2));
    // Thrown east, away from Ann.
    assert_eq!(h[0].dir, [100, 0]);
    // The target is told once and is not sent the flash; the others are.
    assert!(seen(&eb).is_empty());
    assert_eq!(seen(&ea), [Seen { victim: bob.id, attacker: ann.id }]);
    assert_eq!(seen(&ec), [Seen { victim: bob.id, attacker: ann.id }]);
    assert!(hurts(&ea).is_empty() && hurts(&ec).is_empty());
    a.finish();
}

#[test]
fn a_second_hit_before_the_cooldown_is_ignored() {
    let a = Arena::start(no_protection);
    let (mut ann, mut bob, _cy) = trio(&a);
    a.at(100.0);
    swing(&mut ann, bob.id, 1);
    assert_eq!(hurts(&pump(&mut bob, 100)).len(), 1);
    swing(&mut ann, bob.id, 1);
    assert!(hurts(&pump(&mut bob, 100)).is_empty());
    a.at(100.1);
    swing(&mut ann, bob.id, 1);
    assert!(hurts(&pump(&mut bob, 100)).is_empty());
    a.at(100.4);
    swing(&mut ann, bob.id, 1);
    assert_eq!(hurts(&pump(&mut bob, 100)).len(), 1);
    a.finish();
}

#[test]
fn nobody_can_be_hurt_in_their_first_seconds_after_joining_or_respawning() {
    let a = Arena::start(|_| {});
    let (mut ann, mut bob, _cy) = trio(&a);
    a.at(2.0);
    swing(&mut ann, bob.id, 1);
    assert!(hurts(&pump(&mut bob, 100)).is_empty());
    a.at(6.0);
    swing(&mut ann, bob.id, 1);
    assert_eq!(hurts(&pump(&mut bob, 100)).len(), 1);
    // Bob dies, and comes back: protected again for five seconds.
    bob.send_state(channel::PVP_GEAR, false, "", &Gear { defense: 0, toughness: 0, alive: false }.pack());
    pump(&mut bob, 60);
    a.at(10.0);
    bob.send_state(channel::PVP_GEAR, false, "", &Gear::default().pack());
    pump(&mut bob, 60);
    a.at(12.0);
    swing(&mut ann, bob.id, 1);
    assert!(hurts(&pump(&mut bob, 100)).is_empty());
    a.at(15.5);
    swing(&mut ann, bob.id, 1);
    assert_eq!(hurts(&pump(&mut bob, 100)).len(), 1);
    a.finish();
}

#[test]
fn far_away_players_dead_players_creative_players_and_old_games_cannot_be_hit() {
    let a = Arena::start(no_protection);
    let (mut ann, mut bob, mut cy) = trio(&a);
    // Out of reach: Cy walks off.
    cy.send_move(1.0, [30.0, 64.0, 0.0], 0.0, 0.0, 0);
    pump(&mut cy, 60);
    a.at(100.0);
    swing(&mut ann, cy.id, 1);
    assert!(hurts(&pump(&mut cy, 100)).is_empty());
    // A game that never sent its gear (an old one) takes no damage.
    let (mut old, _) = Client::connect("127.0.0.1", a.port, "Old").unwrap();
    pump(&mut old, 60);
    old.send_move(1.0, [0.0, 64.0, 1.0], 0.0, 0.0, 0);
    pump(&mut old, 60);
    a.at(101.0);
    swing(&mut ann, old.id, 1);
    assert!(hurts(&pump(&mut old, 100)).is_empty());
    // Creative mode: immune, and hits nothing.
    ann.send_chat("/gamemode Bob creative");
    pump(&mut ann, 80);
    pump(&mut bob, 80);
    a.at(102.0);
    swing(&mut ann, bob.id, 1);
    assert!(hurts(&pump(&mut bob, 100)).is_empty());
    swing(&mut bob, ann.id, 1);
    assert!(hurts(&pump(&mut ann, 100)).is_empty());
    ann.send_chat("/gamemode Bob survival");
    pump(&mut ann, 80);
    pump(&mut bob, 80);
    a.at(103.0);
    swing(&mut ann, bob.id, 1);
    assert_eq!(hurts(&pump(&mut bob, 100)).len(), 1);
    // Someone who is down is left alone.
    bob.send_state(channel::PVP_GEAR, false, "", &Gear { defense: 0, toughness: 0, alive: false }.pack());
    pump(&mut bob, 60);
    a.at(104.0);
    swing(&mut ann, bob.id, 1);
    assert!(hurts(&pump(&mut bob, 100)).is_empty());
    // And a number that is nobody's, or one's own, does nothing at all.
    swing(&mut ann, 999, 1);
    let own = ann.id;
    swing(&mut ann, own, 1);
    assert!(hurts(&pump(&mut ann, 100)).is_empty());
    a.finish();
}

#[test]
fn the_switch_and_the_permission_flag_turn_fighting_off_and_back_on() {
    let a = Arena::start(no_protection);
    let (mut ann, mut bob, _cy) = trio(&a);
    // Only operators change the switch; everybody may ask.
    bob.send_chat("/pvp off");
    let e = pump(&mut bob, 100);
    assert!(chats(&e).iter().any(|l| l.contains("ONLY OPERATORS")), "{:?}", chats(&e));
    ann.send_chat("/pvp off");
    pump(&mut ann, 80);
    let e = pump(&mut bob, 80);
    assert!(e.iter().any(|e| matches!(e, Event::Perms(f) if *f & crate::proto::PERM_PVP == 0)));
    a.at(100.0);
    swing(&mut ann, bob.id, 1);
    assert!(hurts(&pump(&mut bob, 100)).is_empty());
    bob.send_chat("/pvp");
    assert!(chats(&pump(&mut bob, 100)).iter().any(|l| l.contains("PVP IS OFF")));
    ann.send_chat("/pvp on");
    pump(&mut ann, 80);
    pump(&mut bob, 80);
    a.at(101.0);
    swing(&mut ann, bob.id, 1);
    assert_eq!(hurts(&pump(&mut bob, 100)).len(), 1);
    // A player ops forbid it to can neither hit nor be hit.
    ann.send_chat("/perm Bob -pvp");
    pump(&mut ann, 80);
    a.at(102.0);
    swing(&mut ann, bob.id, 1);
    assert!(hurts(&pump(&mut bob, 100)).is_empty());
    swing(&mut bob, ann.id, 1);
    assert!(hurts(&pump(&mut ann, 100)).is_empty());
    // The switch set by a command is kept with the permissions.
    ann.send_chat("/pvp off");
    pump(&mut ann, 80);
    let server = a.finish();
    assert!(server.perm_text().contains("pvp off"));
    assert!(!server.pvp.rules.on);
}

#[test]
fn armour_is_taken_off_the_blow_by_what_the_server_believes() {
    let a = Arena::start(no_protection);
    let (mut ann, mut bob, _cy) = trio(&a);
    // Bob says he wears 255 points of armour: the server believes 20 and 8.
    bob.send_state(channel::PVP_GEAR, false, "", &Gear { defense: 255, toughness: 255, alive: true }.pack());
    pump(&mut bob, 60);
    a.at(100.0);
    // A hit that claims 200 is held to what a fist may do (and then armour takes its share).
    swing(&mut ann, bob.id, 200);
    let h = hurts(&pump(&mut bob, 120));
    assert_eq!(h.len(), 1);
    let blow = super::check::blow(1, 200);
    assert_eq!(h[0].damage as i32, super::check::after_armour(blow, Gear { defense: 20, toughness: 8, alive: true }));
    assert!((h[0].damage as i32) < blow);
    a.finish();
}

#[test]
fn a_death_soon_after_a_hit_is_a_kill_that_everybody_is_told_of() {
    let a = Arena::start(no_protection);
    let (mut ann, mut bob, mut cy) = trio(&a);
    a.at(100.0);
    swing(&mut ann, bob.id, 5);
    pump(&mut bob, 80);
    a.at(101.0);
    bob.send_state(channel::PVP_GEAR, false, "", &Gear { defense: 0, toughness: 0, alive: false }.pack());
    let ec = pump(&mut cy, 150);
    assert!(chats(&ec).iter().any(|l| l.contains("BOB WAS SLAIN BY ANN")), "{:?}", chats(&ec));
    ann.send_chat("/kills");
    assert!(chats(&pump(&mut ann, 100)).iter().any(|l| l.contains("1 KILL")));
    // Dying with nobody to blame (a fall) is nobody's kill.
    a.at(200.0);
    bob.send_state(channel::PVP_GEAR, false, "", &Gear::default().pack());
    pump(&mut bob, 60);
    a.at(210.0);
    bob.send_state(channel::PVP_GEAR, false, "", &Gear { defense: 0, toughness: 0, alive: false }.pack());
    let ec = pump(&mut cy, 100);
    assert!(!chats(&ec).iter().any(|l| l.contains("SLAIN")), "{:?}", chats(&ec));
    a.finish();
}

#[test]
fn a_player_cannot_forge_the_servers_news() {
    let a = Arena::start(no_protection);
    let (_ann, mut bob, mut cy) = trio(&a);
    cy.send_state(channel::PVP_HURT, false, "", &Hurt { attacker: 1, damage: 40, dir: [0, 0] }.pack());
    cy.send_state(channel::PVP_SEEN, false, "", &Seen { victim: bob.id, attacker: 1 }.pack());
    let e = pump(&mut bob, 150);
    assert!(hurts(&e).is_empty() && seen(&e).is_empty());
    a.finish();
}
