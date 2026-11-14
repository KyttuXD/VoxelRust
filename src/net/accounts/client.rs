//! The player's side of accounts: answering the server's login challenge when joining
//! (`netplay::Client::connect_auth`), and the chat commands `/register`, `/pin` and `/forgetpin`
//! once in the game. The PIN is typed here and turned into keys here; it never goes on the wire.

use super::keys::{self, Nonce, Salt};
use super::state::{flag, op};
use super::{pin, remember};
use crate::netplay::{Client, Event};
use crate::proto::Msg;
use alloc::string::String;
use alloc::vec::Vec;

/// Where this game keeps PINs it was allowed to remember (the data folder), and which server this is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RememberAt {
    pub base: String,
    pub address: String,
}

/// What a player brings to a join: nothing, or a PIN (typed, or remembered from before).
#[derive(Clone, Debug, Default)]
pub struct JoinAuth {
    pub at: Option<RememberAt>,
    pub pin: Option<String>,
    /// The PIN is for making a new account (the server asked a new name to register).
    pub register: bool,
    /// Keep the PIN for this server once it has worked.
    pub remember: bool,
}

impl JoinAuth {
    /// What the data folder remembers for this server and name (nothing, perhaps; the place to
    /// remember to is always set).
    pub fn remembered(base: &str, address: &str, name: &str) -> JoinAuth {
        let pin = remember::lookup(base, address, name);
        JoinAuth { at: Some(RememberAt { base: String::from(base), address: String::from(address) }), remember: pin.is_some(), pin, register: false }
    }
}

/// Why a join did not happen.
#[derive(Debug, PartialEq, Eq)]
pub enum ConnectError {
    /// The text to show.
    Failed(String),
    /// The server wants a PIN (or a new one): ask the player and join again. `register` says
    /// the name has no account and must make one; `tries` is how many wrong tries are left;
    /// `note` is why we are asking again (empty the first time).
    NeedPin { register: bool, tries: u8, note: String, weak_ok: bool },
}

impl From<ConnectError> for String {
    fn from(e: ConnectError) -> String {
        match e {
            ConnectError::Failed(s) => s,
            ConnectError::NeedPin { register: true, .. } => String::from("THIS SERVER NEEDS AN ACCOUNT: JOIN FROM THE MULTIPLAYER SCREEN TO CHOOSE A PIN"),
            ConnectError::NeedPin { note, .. } if !note.is_empty() => note,
            ConnectError::NeedPin { .. } => String::from("THIS NAME HAS AN ACCOUNT: JOIN FROM THE MULTIPLAYER SCREEN AND GIVE ITS PIN"),
        }
    }
}

/// A challenge as it came from the server.
#[derive(Clone, Copy, Debug)]
pub struct Challenge {
    pub op: u8,
    pub flags: u8,
    pub rounds: u32,
    pub tries: u8,
    pub salt: Salt,
    pub new_salt: Salt,
    pub nonce: Nonce,
}

/// The answer to a challenge met while joining, or why there is none yet.
pub fn respond(auth: &JoinAuth, name: &str, c: &Challenge) -> Result<Msg, ConnectError> {
    let weak_ok = c.flags & flag::WEAK_OK != 0;
    if !keys::rounds_ok(c.rounds) {
        return Err(ConnectError::Failed(String::from("THE SERVER ASKED FOR ODD LOGIN SETTINGS: NOT ANSWERING")));
    }
    let register = c.op == op::REGISTER;
    let need = |note: &str| ConnectError::NeedPin { register, tries: c.tries, note: String::from(note), weak_ok };
    let Some(typed) = auth.pin.as_deref() else { return Err(need("")) };
    match c.op {
        op::LOGIN => {
            if auth.register {
                return Err(need("THIS NAME ALREADY HAS AN ACCOUNT: GIVE ITS PIN"));
            }
            if !pin::well_formed(typed) {
                return Err(need("A PIN IS 4 OR 6 DIGITS"));
            }
            let key = keys::derive_key(typed, &c.salt, c.rounds);
            Ok(Msg::AuthLogin { proof: keys::login_proof(&key, &c.nonce, name) })
        }
        op::REGISTER => {
            if !auth.register {
                return Err(need("THIS SERVER NEEDS AN ACCOUNT: CHOOSE A PIN"));
            }
            if let Err(e) = pin::check(typed, weak_ok) {
                return Err(need(&e.to_ascii_uppercase()));
            }
            Ok(Msg::AuthRegister { key: keys::derive_key(typed, &c.new_salt, c.rounds) })
        }
        _ => Err(ConnectError::Failed(String::from("THE SERVER SENT A LOGIN STEP THIS GAME DOES NOT KNOW"))),
    }
}

/// What an in-game `/register` or `/pin` waits to answer a challenge with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Task {
    #[default]
    None,
    Register(String),
    Change(String, String),
}

/// What the client knows about its account (a field of [`Client`]).
#[derive(Default)]
pub struct ClientAccount {
    /// The server's mode (0 no accounts, 1 optional, 2 required) and `flag` bits; None until it said
    /// (servers from before accounts never do).
    pub info: Option<(u8, u8)>,
    /// The name this player logs in with, lower case.
    pub name: String,
    pub at: Option<RememberAt>,
    /// Keep the PIN for this server.
    pub remember: bool,
    pub task: Task,
    /// The PIN that a pending `/register` or `/pin` would make the new one.
    pub new_pin: Option<String>,
}

impl ClientAccount {
    pub fn logged_in(&self) -> bool {
        self.info.map_or(false, |i| i.1 & flag::LOGGED_IN != 0)
    }
}

/// Whether a chat line (lower case) is one the game handles itself and must not tell anyone
/// (not the server as chat, not the mods' scripts: it holds a PIN).
pub fn is_account_command(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or("");
    first.eq_ignore_ascii_case("/register") || first.eq_ignore_ascii_case("/pin") || first.eq_ignore_ascii_case("/forgetpin")
}

fn chat(text: &str) -> Event {
    Event::Chat(alloc::format!("&E{}", text).to_ascii_uppercase())
}

impl Client {
    /// `/register PIN [PIN] [remember]`, `/pin OLD NEW`, `/forgetpin`: what to show the player now.
    pub fn account_command(&mut self, line: &str) -> String {
        let w: Vec<&str> = line.split_whitespace().collect();
        let first = w.first().map(|c| c.to_ascii_lowercase()).unwrap_or_default();
        let Some((mode, flags)) = self.acct.info else {
            return String::from("THIS SERVER DOES NOT HAVE ACCOUNTS (IT RUNS AN OLDER VERSION)");
        };
        let weak_ok = flags & flag::WEAK_OK != 0;
        match first.as_str() {
            "/register" => {
                if mode == 0 {
                    return String::from("ACCOUNTS ARE SWITCHED OFF ON THIS SERVER");
                }
                if flags & flag::LOGGED_IN != 0 {
                    return String::from("YOU HAVE AN ACCOUNT ALREADY. CHANGE ITS PIN WITH /PIN OLD NEW");
                }
                let keep = w.iter().any(|x| x.eq_ignore_ascii_case("remember"));
                let pins: Vec<&str> = w[1.min(w.len())..].iter().copied().filter(|x| !x.eq_ignore_ascii_case("remember")).collect();
                let Some(&new_pin) = pins.first() else {
                    return String::from("USE  /REGISTER PIN PIN  (4 OR 6 DIGITS, TYPED TWICE)  [REMEMBER]");
                };
                if pins.get(1).map_or(false, |again| *again != new_pin) {
                    return String::from("THE TWO PINS ARE NOT THE SAME");
                }
                if let Err(e) = pin::check(new_pin, weak_ok) {
                    return e.to_ascii_uppercase();
                }
                self.acct.task = Task::Register(String::from(new_pin));
                self.acct.new_pin = Some(String::from(new_pin));
                self.acct.remember = keep && self.acct.at.is_some();
                self.send(&Msg::AuthAsk { op: op::REGISTER });
                String::from("MAKING YOUR ACCOUNT...")
            }
            "/pin" => {
                if flags & flag::LOGGED_IN == 0 {
                    return String::from("YOU HAVE NO ACCOUNT YET: MAKE ONE WITH /REGISTER PIN PIN");
                }
                let (Some(&old), Some(&new)) = (w.get(1), w.get(2)) else {
                    return String::from("USE  /PIN OLD NEW");
                };
                if let Err(e) = pin::check(new, weak_ok) {
                    return e.to_ascii_uppercase();
                }
                if !pin::well_formed(old) {
                    return String::from("YOUR OLD PIN IS 4 OR 6 DIGITS");
                }
                self.acct.task = Task::Change(String::from(old), String::from(new));
                self.send(&Msg::AuthAsk { op: op::CHANGE });
                String::from("CHANGING YOUR PIN...")
            }
            _ => match self.acct.at.clone() {
                Some(at) => {
                    remember::forget(&at.base, &at.address, &self.acct.name);
                    self.acct.remember = false;
                    String::from("THE GAME FORGOT YOUR PIN FOR THIS SERVER")
                }
                None => String::from("THERE IS NOTHING TO FORGET HERE"),
            },
        }
    }

    /// The server sent an account message after joining (called by `poll`).
    pub(crate) fn on_account_message(&mut self, msg: Msg, events: &mut Vec<Event>) {
        match msg {
            Msg::Account { mode, flags } => {
                let first = self.acct.info.is_none();
                let was_in = self.acct.logged_in();
                self.acct.info = Some((mode, flags));
                if flags & flag::LOGGED_IN != 0 {
                    if !was_in {
                        let admin = if flags & flag::ADMIN != 0 { " (ADMINISTRATOR)" } else { "" };
                        events.push(chat(&alloc::format!("logged in as {}{}", self.acct.name, admin)));
                    }
                } else if first && mode == 1 {
                    events.push(chat("this server has accounts: protect your name with /register PIN PIN (4 or 6 digits)"));
                } else if first && mode == 2 {
                    events.push(chat("this server needs an account, but you joined without one: use /register PIN PIN"));
                }
            }
            Msg::AuthChallenge { op, flags, rounds, tries, salt, new_salt, nonce } => {
                let c = Challenge { op, flags, rounds, tries, salt, new_salt, nonce };
                self.answer_in_game(&c);
            }
            Msg::AuthResult { ok, text, .. } => {
                if !text.is_empty() {
                    events.push(chat(&text));
                }
                let keep = core::mem::take(&mut self.acct.new_pin);
                self.acct.task = Task::None;
                if let (true, Some(new_pin), Some(at)) = (ok, keep, self.acct.at.clone()) {
                    // A PIN that was kept for this server stays the PIN that works.
                    if self.acct.remember || remember::lookup(&at.base, &at.address, &self.acct.name).is_some() {
                        remember::save(&at.base, &at.address, &self.acct.name, &new_pin);
                        self.acct.remember = true;
                    }
                }
            }
            _ => {}
        }
    }

    fn answer_in_game(&mut self, c: &Challenge) {
        if !keys::rounds_ok(c.rounds) {
            self.acct.task = Task::None;
            return;
        }
        match (c.op, core::mem::take(&mut self.acct.task)) {
            (op::REGISTER, Task::Register(new_pin)) => {
                let key = keys::derive_key(&new_pin, &c.new_salt, c.rounds);
                self.send(&Msg::AuthRegister { key });
            }
            (op::CHANGE, Task::Change(old, new)) => {
                let old_key = keys::derive_key(&old, &c.salt, c.rounds);
                let proof = keys::login_proof(&old_key, &c.nonce, &self.acct.name);
                let key = keys::derive_key(&new, &c.new_salt, c.rounds);
                self.acct.new_pin = Some(new);
                self.send(&Msg::AuthChange { proof, key });
            }
            // Not asked for: ignored (the task, if any, is kept for the challenge that fits).
            (_, task) => self.acct.task = task,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn challenge(op: u8) -> Challenge {
        Challenge { op, flags: 0, rounds: 1000, tries: 5, salt: [1; 16], new_salt: [2; 16], nonce: [3; 16] }
    }

    #[test]
    fn a_login_answer_is_the_proof_of_the_pin_and_a_missing_pin_asks_for_one() {
        let auth = JoinAuth { pin: Some(String::from("4821")), ..Default::default() };
        let c = challenge(op::LOGIN);
        match respond(&auth, "Kyttu", &c) {
            Ok(Msg::AuthLogin { proof }) => {
                let key = keys::derive_key("4821", &[1; 16], 1000);
                assert!(keys::proof_matches(&key, &[3; 16], "kyttu", &proof));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(respond(&JoinAuth::default(), "kyttu", &c), Err(ConnectError::NeedPin { register: false, tries: 5, note: String::new(), weak_ok: false }));
        // A typed PIN of the wrong shape is asked for again.
        let bad = JoinAuth { pin: Some(String::from("12")), ..Default::default() };
        assert!(matches!(respond(&bad, "kyttu", &c), Err(ConnectError::NeedPin { .. })));
        // Registering where the name has an account already.
        let reg = JoinAuth { pin: Some(String::from("4821")), register: true, ..Default::default() };
        assert!(matches!(respond(&reg, "kyttu", &c), Err(ConnectError::NeedPin { register: false, .. })));
    }

    #[test]
    fn a_register_answer_is_the_key_made_with_the_servers_salt() {
        let auth = JoinAuth { pin: Some(String::from("4821")), register: true, ..Default::default() };
        match respond(&auth, "ann", &challenge(op::REGISTER)) {
            Ok(Msg::AuthRegister { key }) => assert_eq!(key, keys::derive_key("4821", &[2; 16], 1000)),
            other => panic!("{other:?}"),
        }
        // A trivial PIN is not sent unless the server allows it.
        let weak = JoinAuth { pin: Some(String::from("1234")), register: true, ..Default::default() };
        assert!(matches!(respond(&weak, "ann", &challenge(op::REGISTER)), Err(ConnectError::NeedPin { register: true, .. })));
        let mut c = challenge(op::REGISTER);
        c.flags = flag::WEAK_OK;
        assert!(respond(&weak, "ann", &c).is_ok());
        // A player who typed a login PIN where the server wants a new account is asked again.
        let login = JoinAuth { pin: Some(String::from("4821")), ..Default::default() };
        assert!(matches!(respond(&login, "ann", &challenge(op::REGISTER)), Err(ConnectError::NeedPin { register: true, .. })));
    }

    #[test]
    fn a_server_cannot_make_the_game_hash_for_ever() {
        let auth = JoinAuth { pin: Some(String::from("4821")), ..Default::default() };
        let mut c = challenge(op::LOGIN);
        c.rounds = u32::MAX;
        assert!(matches!(respond(&auth, "ann", &c), Err(ConnectError::Failed(_))));
    }

    #[test]
    fn pin_commands_are_recognised_whatever_the_case() {
        assert!(is_account_command("/register 4821 4821") && is_account_command("/PIN 1 2") && is_account_command("/forgetpin"));
        assert!(!is_account_command("/pinned") && !is_account_command("hello /pin") && !is_account_command(""));
    }
}
