//! End-to-end tests of accounts: a real server on localhost, in a thread, and real clients.
//! (The pieces have tests of their own in their files: keys, PINs, lockouts, the file...)

use super::client::{ConnectError, JoinAuth};
use super::config::Require;
use super::keys;
use super::state::Bucket;
use super::store::{Account, KIND_PIN};
use super::strikes::Strikes;
use crate::netplay::{Client, Event, Welcome};
use crate::proto::{Inbox, Msg, Next, PERM_BUILD, PERM_COMMANDS, PERM_GAMEMODE, PERM_PVP, PERM_VOICE};
use crate::server::{Config, Server};
use alloc::string::String;
use alloc::vec::Vec;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// An account whose PIN is `pin` (a thousand rounds, to keep the tests quick).
fn add_account(s: &mut Server, name: &str, pin: &str) {
    let salt = [name.len() as u8 + 40; 16];
    let key = keys::derive_key(pin, &salt, 1000);
    assert!(s.acct.accounts.put(Account { name: String::from(name), kind: String::from(KIND_PIN), salt, key, rounds: 1000, created: 1, strikes: Strikes::default() }));
}

struct Harness {
    port: u16,
    stop: Arc<AtomicBool>,
    skew: Arc<AtomicU64>,
    handle: std::thread::JoinHandle<Server>,
}

impl Harness {
    fn start(setup: impl FnOnce(&mut Server)) -> Harness {
        Harness::start_with(0.005, setup)
    }

    /// `dt` is the time each round of the server's loop takes as far as the server knows
    /// (a bigger one makes its timeouts come sooner in real time).
    fn start_with(dt: f32, setup: impl FnOnce(&mut Server)) -> Harness {
        let cfg = Config { port: 0, seed: 7, kind: String::from("normal"), max_players: 8, content: crate::proto::content_checksum(), states: crate::block::reg().blocks.len() as u16, pack: Vec::new(), mods: Vec::new() };
        let mut server = Server::bind(cfg).unwrap();
        server.acct.cfg.rounds = 1000;
        setup(&mut server);
        let port = server.port();
        let stop = Arc::new(AtomicBool::new(false));
        let skew = Arc::new(AtomicU64::new(0));
        let (stop2, skew2) = (stop.clone(), skew.clone());
        let handle = std::thread::spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                server.acct.skew = skew2.load(Ordering::Relaxed);
                server.tick(dt, &mut |_| None);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            server
        });
        Harness { port, stop, skew, handle }
    }

    /// Let `secs` seconds pass for the server's clock (the lockouts).
    fn later(&self, secs: u64) {
        self.skew.fetch_add(secs, Ordering::Relaxed);
    }

    fn finish(self) -> Server {
        self.stop.store(true, Ordering::Relaxed);
        self.handle.join().unwrap()
    }

    fn join(&self, name: &str, pin: Option<&str>) -> Result<(Client, Welcome), ConnectError> {
        Client::connect_auth("127.0.0.1", self.port, name, &JoinAuth { pin: pin.map(String::from), ..Default::default() })
    }

    fn register(&self, name: &str, pin: &str) -> Result<(Client, Welcome), ConnectError> {
        Client::connect_auth("127.0.0.1", self.port, name, &JoinAuth { pin: Some(String::from(pin)), register: true, ..Default::default() })
    }

    /// What a client of protocol `version` is sent after saying hello as `name`, in `wait_ms`.
    fn raw(&self, version: u16, name: &str, wait_ms: u64) -> Vec<Msg> {
        let mut s = crate::sys::net::TcpStream::connect("127.0.0.1", self.port, 2000).unwrap();
        s.send_all(&Msg::Hello { version, content: 0, name: String::from(name) }.encode(), 2000).unwrap();
        let (mut inbox, mut out, mut buf) = (Inbox::default(), Vec::new(), [0u8; 4096]);
        let end = std::time::Instant::now() + std::time::Duration::from_millis(wait_ms);
        while std::time::Instant::now() < end {
            let closed = match s.recv(&mut buf, 50) {
                Ok(n) => {
                    inbox.push(&buf[..n]);
                    false
                }
                Err(_) => true,
            };
            while let Next::Msg(m) = inbox.next() {
                out.push(m);
            }
            if closed {
                break;
            }
        }
        out
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

fn chats(events: &[Event]) -> Vec<String> {
    events.iter().filter_map(|e| if let Event::Chat(l) = e { Some(l.clone()) } else { None }).collect()
}

fn note_of(e: Result<(Client, Welcome), ConnectError>) -> String {
    match e {
        Err(ConnectError::NeedPin { note, .. }) => note,
        Err(ConnectError::Failed(t)) => t,
        Ok(_) => String::from("JOINED"),
    }
}

#[test]
fn the_right_pin_logs_in_and_the_wrong_one_does_not() {
    let h = Harness::start(|s| add_account(s, "ann", "4821"));
    // No PIN: the game is told to ask for one (and nothing is counted against anybody).
    assert!(matches!(h.join("Ann", None), Err(ConnectError::NeedPin { register: false, tries: 5, .. })));
    let (mut c, _) = h.join("Ann", Some("4821")).ok().expect("the right PIN");
    let events = pump(&mut c, 300);
    assert!(chats(&events).iter().any(|l| l.contains("LOGGED IN AS ANN")), "{:?}", chats(&events));
    assert!(c.acct.logged_in());
    let note = note_of(h.join("Ann", Some("4822")));
    assert_eq!(note, "WRONG PIN: 4 TRIES LEFT");
    let note = note_of(h.join("ann", Some("0000")));
    assert_eq!(note, "WRONG PIN: 3 TRIES LEFT");
    drop(c);
    let server = h.finish();
    // The right PIN forgave the account; the wrong ones were counted against it afterwards.
    let a = server.acct.accounts.get("ann").unwrap();
    assert_eq!(a.strikes.fails, 2);
    assert!(!server.acct.accounts.to_text().contains("4821"));
}

#[test]
fn five_wrong_tries_lock_the_account_and_each_lockout_is_twice_as_long() {
    let h = Harness::start(|s| add_account(s, "ann", "4821"));
    for left in (1..=4).rev() {
        assert_eq!(note_of(h.join("ann", Some("1357"))), alloc::format!("WRONG PIN: {left} {} LEFT", if left == 1 { "TRY" } else { "TRIES" }));
    }
    let fifth = note_of(h.join("ann", Some("1357")));
    assert!(fifth.contains("TOO MANY WRONG PINS: LOCKED UNTIL") && fifth.contains("60 SECONDS"), "{fifth}");
    // Locked: not even the right PIN gets in, and the player is told before being asked anything.
    assert!(note_of(h.join("ann", Some("4821"))).contains("LOCKED UNTIL"));
    assert!(note_of(h.join("ann", None)).contains("LOCKED UNTIL"));
    // A minute later it is open again, and the next lockout is two minutes.
    h.later(61);
    for _ in 0..4 {
        assert!(note_of(h.join("ann", Some("1357"))).starts_with("WRONG PIN"));
    }
    let second = note_of(h.join("ann", Some("1357")));
    assert!(second.contains("LOCKED UNTIL") && second.contains("2 MINUTES"), "{second}");
    // And the right PIN works once that is over.
    h.later(121);
    assert!(h.join("ann", Some("4821")).is_ok());
    h.finish();
}

#[test]
fn wrong_tries_are_counted_per_address_too() {
    let h = Harness::start(|s| {
        add_account(s, "ann", "4821");
        add_account(s, "bob", "9362");
    });
    // Three wrong tries on one account and two on another: this address has had five.
    for _ in 0..3 {
        assert!(note_of(h.join("ann", Some("1357"))).starts_with("WRONG PIN"));
    }
    assert!(note_of(h.join("bob", Some("1357"))).starts_with("WRONG PIN"));
    assert!(note_of(h.join("bob", Some("1357"))).contains("LOCKED UNTIL"));
    // Every account is out of reach from here, with the right PIN too; a name without an account is not.
    assert!(note_of(h.join("ann", Some("4821"))).contains("LOCKED UNTIL"));
    assert!(h.join("carol", None).is_ok());
    h.later(61);
    assert!(h.join("ann", Some("4821")).is_ok());
    h.finish();
}

#[test]
fn logins_are_limited_for_the_whole_server() {
    let h = Harness::start(|s| {
        add_account(s, "ann", "4821");
        s.acct.global = Bucket::empty();
    });
    let note = note_of(h.join("ann", Some("4821")));
    assert!(note.contains("BUSY"), "{note}");
    // Nothing was counted against the account.
    let server = h.finish();
    assert_eq!(server.acct.accounts.get("ann").unwrap().strikes.fails, 0);
}

#[test]
fn a_name_with_an_account_cannot_be_used_without_logging_in_and_guests_keep_off_it() {
    let h = Harness::start(|s| add_account(s, "ann", "4821"));
    let (mut owner, _) = h.join("Ann", Some("4821")).ok().unwrap();
    // A guest who is called the same thing (the server only knows the owner): a login is asked for.
    assert!(matches!(h.join("ANN", None), Err(ConnectError::NeedPin { .. })));
    // Another guest of a name that is free joins as a guest, and cannot be made to become "ann2" by accident.
    let (_guest, _) = h.join("bob", None).ok().unwrap();
    // The owner logging in again replaces the old connection.
    let (mut again, _) = h.join("Ann", Some("4821")).ok().unwrap();
    let events = pump(&mut owner, 400);
    assert!(!owner.alive || events.iter().any(|e| matches!(e, Event::Lost)));
    assert!(pump(&mut again, 100).iter().all(|e| !matches!(e, Event::Lost)));
    h.finish();
}

#[test]
fn an_older_game_joins_as_a_guest_but_not_as_a_name_with_an_account() {
    let h = Harness::start(|s| {
        add_account(s, "ann", "4821");
        s.acct.cfg.admins = alloc::vec![String::from("kyttu")];
    });
    // A guest of protocol 13 is let in and is sent nothing that protocol does not know.
    let msgs = h.raw(13, "oldtimer", 600);
    assert!(msgs.iter().any(|m| matches!(m, Msg::Welcome { .. })), "{msgs:?}");
    assert!(msgs.iter().all(|m| !matches!(m, Msg::Account { .. } | Msg::AuthChallenge { .. } | Msg::AuthResult { .. })), "{msgs:?}");
    // A name with an account is refused with the way out.
    let msgs = h.raw(13, "Ann", 600);
    assert!(matches!(&msgs[..], [Msg::Refuse { reason }] if reason == "this name has an account: update your game to log in"), "{msgs:?}");
    // And so is a reserved one.
    let msgs = h.raw(13, "Kyttu", 600);
    assert!(matches!(&msgs[..], [Msg::Refuse { reason }] if reason == "this name is reserved"), "{msgs:?}");
    // A game from before that is too old for the server still gets the version message.
    let msgs = h.raw(12, "ancient", 600);
    assert!(matches!(&msgs[..], [Msg::Refuse { reason }] if reason.contains("too old")), "{msgs:?}");
    h.finish();
}

#[test]
fn when_accounts_are_required_an_older_game_is_told_to_update_and_a_new_name_registers() {
    let h = Harness::start(|s| s.acct.cfg.require = Require::Yes);
    let msgs = h.raw(13, "oldtimer", 600);
    assert!(matches!(&msgs[..], [Msg::Refuse { reason }] if reason.contains("needs accounts: update your game")), "{msgs:?}");
    // A current game is asked to choose a PIN.
    assert!(matches!(h.join("newbie", None), Err(ConnectError::NeedPin { register: true, .. })));
    // A trivial PIN is not even sent.
    assert!(note_of(h.register("newbie", "1234")).contains("TOO EASY"));
    let (c, _) = h.register("newbie", "4821").ok().expect("registering");
    drop(c);
    let (_c, _) = h.join("newbie", Some("4821")).ok().expect("logging in after");
    assert!(matches!(h.join("newbie", None), Err(ConnectError::NeedPin { register: false, .. })));
    let server = h.finish();
    assert!(server.acct.accounts.get("newbie").is_some());
    assert!(!server.acct.accounts.to_text().contains("4821"));
}

#[test]
fn a_guest_registers_in_the_game_and_changes_the_pin_with_chat_commands() {
    // (A quick clock: the server lets a player ask for a challenge only once a second.)
    let h = Harness::start_with(0.05, |_| {});
    let (mut c, _) = h.join("Bob", None).ok().unwrap();
    let events = pump(&mut c, 200);
    assert!(chats(&events).iter().any(|l| l.contains("THIS SERVER HAS ACCOUNTS")), "{:?}", chats(&events));
    // Mistakes are caught in the game, before anything is sent.
    assert!(c.account_command("/register 1234 1234").contains("TOO EASY"));
    assert!(c.account_command("/register 4821 4822").contains("NOT THE SAME"));
    assert!(c.account_command("/register 48").contains("4 OR 6"));
    assert!(c.account_command("/pin 4821 7390").contains("NO ACCOUNT"));
    assert_eq!(c.account_command("/register 4821 4821"), "MAKING YOUR ACCOUNT...");
    let events = pump(&mut c, 600);
    assert!(chats(&events).iter().any(|l| l.contains("PROTECTED BY YOUR PIN")), "{:?}", chats(&events));
    assert!(c.acct.logged_in());
    // Now the name needs the PIN.
    assert!(matches!(h.join("bob", None), Err(ConnectError::NeedPin { .. })));
    assert!(c.account_command("/register 9362").contains("ALREADY"));
    assert_eq!(c.account_command("/pin 4821 7390"), "CHANGING YOUR PIN...");
    let events = pump(&mut c, 600);
    assert!(chats(&events).iter().any(|l| l.contains("PIN IS CHANGED")), "{:?}", chats(&events));
    assert!(note_of(h.join("bob", Some("4821"))).starts_with("WRONG PIN"));
    // A wrong old PIN changes nothing.
    assert_eq!(c.account_command("/pin 1111 5802"), "CHANGING YOUR PIN...");
    let events = pump(&mut c, 600);
    assert!(chats(&events).iter().any(|l| l.contains("WRONG PIN")), "{:?}", chats(&events));
    // (Logging in again replaces this connection.)
    assert!(h.join("bob", Some("7390")).is_ok());
    drop(c);
    let server = h.finish();
    let text = server.acct.accounts.to_text();
    assert!(!text.contains("4821") && !text.contains("7390") && !text.contains("5802"));
}

#[test]
fn an_older_server_without_accounts_just_says_so() {
    // (A client that never heard an Account message: what a protocol 13 server gives.)
    let h = Harness::start(|_| {});
    let (mut c, _) = h.join("Cy", None).ok().unwrap();
    pump(&mut c, 200);
    c.acct.info = None;
    assert!(c.account_command("/register 4821 4821").contains("DOES NOT HAVE ACCOUNTS"));
    h.finish();
}

#[test]
fn nobody_can_register_a_reserved_name_and_the_administrator_logs_in_with_every_permission() {
    let h = Harness::start(|s| {
        s.acct.cfg.admins = alloc::vec![String::from("kyttu")];
    });
    // Joining as the administrator's name before the account exists: refused, and registering is no way round.
    assert!(note_of(h.join("Kyttu", None)).contains("RESERVED"));
    assert!(note_of(h.register("KYTTU", "4821")).contains("RESERVED"));
    h.finish();

    // The owner makes the account on the machine (--set-pin), and logs in.
    let file = super::set_pin::apply("", "Kyttu", "482915", &super::config::Config::from_text("admins=KYTTU\n", None), &mut super::rng::Rng::from_seed(b"x"), 1).unwrap().0;
    let h = Harness::start(move |s| {
        s.acct.cfg.admins = alloc::vec![String::from("kyttu")];
        s.acct.accounts.load_text(&file);
    });
    // Somebody else comes first: they are not made an operator (the server names its administrators).
    let (mut guest, _) = h.join("Bob", None).ok().unwrap();
    let events = pump(&mut guest, 200);
    let perms: Vec<u8> = events.iter().filter_map(|e| if let Event::Perms(f) = e { Some(*f) } else { None }).collect();
    assert_eq!(perms.last(), Some(&(PERM_BUILD | PERM_VOICE | PERM_PVP)));
    // The administrator: all of it, a star in the list, and the commands.
    let (mut admin, _) = h.join("kyttu", Some("482915")).ok().expect("administrator login");
    let events = pump(&mut admin, 300);
    let perms: Vec<u8> = events.iter().filter_map(|e| if let Event::Perms(f) = e { Some(*f) } else { None }).collect();
    assert_eq!(perms.last(), Some(&(PERM_BUILD | PERM_GAMEMODE | PERM_COMMANDS | PERM_VOICE | PERM_PVP)));
    assert!(chats(&events).iter().any(|l| l.contains("ADMINISTRATOR")), "{:?}", chats(&events));
    admin.send_chat("/list");
    let events = pump(&mut admin, 300);
    assert!(chats(&events).iter().any(|l| l.contains("KYTTU*") && !l.contains("BOB*")), "{:?}", chats(&events));
    // /accounts and /resetpin are the administrators' alone.
    guest.send_chat("/accounts");
    assert!(chats(&pump(&mut guest, 300)).iter().any(|l| l.contains("ONLY OPERATORS") || l.contains("ONLY ADMINISTRATORS")));
    admin.send_chat("/accounts");
    assert!(chats(&pump(&mut admin, 300)).iter().any(|l| l.contains("ACCOUNTS (1): KYTTU*")));
    // Reset the PIN of an account: a new PIN is given, and works.
    // (On a server where accounts are optional a guest makes one with /register in the game.)
    let (mut ann, _) = h.join("Ann", None).ok().unwrap();
    pump(&mut ann, 200);
    assert_eq!(ann.account_command("/register 4821 4821"), "MAKING YOUR ACCOUNT...");
    assert!(chats(&pump(&mut ann, 600)).iter().any(|l| l.contains("PROTECTED BY YOUR PIN")));
    admin.send_chat("/resetpin ann");
    let reply = chats(&pump(&mut admin, 400)).into_iter().find(|l| l.contains("THE PIN OF ANN IS NOW")).expect("a reset PIN");
    let new_pin: String = reply.split("IS NOW ").nth(1).unwrap().chars().take(6).collect();
    assert!(new_pin.chars().all(|c| c.is_ascii_digit()), "{reply}");
    assert!(note_of(h.join("ann", Some("4821"))).starts_with("WRONG PIN"));
    assert!(h.join("ann", Some(&new_pin)).is_ok());
    // An administrator's own PIN is out of /resetpin's reach (also for themselves).
    admin.send_chat("/resetpin kyttu 937156");
    assert!(chats(&pump(&mut admin, 400)).iter().any(|l| l.contains("CANNOT BE RESET FROM THE GAME")));
    // The administrator can ban and kick like any operator.
    admin.send_chat("/ban bob");
    assert!(chats(&pump(&mut admin, 300)).iter().any(|l| l.contains("BOB WAS REMOVED")));
    let server = h.finish();
    assert!(!server.ops.iter().any(|o| o.eq_ignore_ascii_case("bob")));
}

#[test]
fn the_pin_never_appears_in_the_servers_log_or_goes_to_other_players() {
    let h = Harness::start(|_| {});
    let (mut a, _) = h.join("Ann", None).ok().unwrap();
    let (mut b, _) = h.join("Bob", None).ok().unwrap();
    pump(&mut a, 100);
    pump(&mut b, 100);
    // An older game would send the command as chat: the server turns it down and nobody else sees it.
    a.send_chat("/register 4821 4821");
    let to_a = chats(&pump(&mut a, 300));
    assert!(to_a.iter().any(|l| l.contains("ACCOUNTS NEED A NEWER GAME")), "{to_a:?}");
    let to_b = chats(&pump(&mut b, 100));
    assert!(to_b.iter().all(|l| !l.contains("4821")), "{to_b:?}");
    h.finish();
}

#[test]
fn nobody_hears_about_accounts_from_the_joining_of_others() {
    let h = Harness::start(|s| add_account(s, "ann", "4821"));
    let (mut guest, _) = h.join("Bob", None).ok().unwrap();
    pump(&mut guest, 100);
    let (_ann, _) = h.join("ann", Some("4821")).ok().unwrap();
    let events = pump(&mut guest, 300);
    // The guest hears "ANN JOINED", like for any player, and nothing about accounts.
    let lines = chats(&events);
    assert!(lines.iter().any(|l| l == "ANN JOINED"), "{lines:?}");
    assert!(lines.iter().all(|l| !l.contains("ACCOUNT") && !l.contains("PIN")), "{lines:?}");
    h.finish();
}

#[test]
fn a_player_who_does_not_answer_the_login_in_fifteen_seconds_is_refused() {
    // (Each round of this server's loop counts for a tenth of a second.)
    let h = Harness::start_with(0.1, |s| add_account(s, "ann", "4821"));
    let msgs = h.raw(crate::proto::VERSION, "ann", 8000);
    assert!(matches!(msgs.first(), Some(Msg::AuthChallenge { .. })), "{msgs:?}");
    assert!(matches!(msgs.last(), Some(Msg::Refuse { reason }) if reason.contains("15 seconds")), "{msgs:?}");
    h.finish();
}

#[test]
fn a_places_memory_is_kept_under_the_account_name() {
    let h = Harness::start(|s| add_account(s, "ann", "4821"));
    let (mut c, _) = h.join("Ann", Some("4821")).ok().unwrap();
    c.send_move(1.0, [10.0, 70.0, 20.0], 0.5, 0.1, 0);
    pump(&mut c, 200);
    drop(c);
    let (mut guest, _) = h.join("Dee", None).ok().unwrap();
    guest.send_move(1.0, [1.0, 71.0, 2.0], 0.0, 0.0, 0);
    pump(&mut guest, 200);
    drop(guest);
    std::thread::sleep(std::time::Duration::from_millis(300));
    let server = h.finish();
    assert!(server.spots.contains_key("ann"));
    assert!(server.spots.contains_key("dee"));
}
