//! The server's half of accounts: what happens when a client says hello, answers a
//! challenge or asks for one, letting a player in, and the administrators' commands.
//!
//! This file is declared inside `net/server.rs` (a child module), so that it can reach the
//! server's own fields; the rest of accounts lives in `net/accounts/`.

use super::*;
use crate::net::accounts::config::Require;
use crate::net::accounts::state::{flag, op, Pending, ANSWER_SECONDS, INGAME_SECONDS};
use crate::net::accounts::store::{account_name, Account, KIND_PIN, MAX_ACCOUNTS};
use crate::net::accounts::strikes::{lock_text, Fail, Strikes};
use crate::net::accounts::{keys, pin, ACCOUNTS_SINCE, OLD_CLIENT_HAS_ACCOUNT};

/// What to do with a client that has said hello.
pub(super) enum Gate {
    /// It may join as a guest (its name has no account): carry on with the normal join.
    Guest,
    /// A challenge was sent; the join goes on when it is answered.
    Wait,
}

/// A line of the server's log that never holds a PIN: the commands that carry one are cut short.
pub(super) fn loggable(text: &str) -> String {
    let w: Vec<&str> = text.split_whitespace().collect();
    match w.first().map(|c| c.to_ascii_lowercase()) {
        Some(c) if ["/register", "/pin", "/resetpin", "/login", "/setpin"].contains(&c.as_str()) => {
            alloc::format!("{}{}", c, if w.len() > 1 { " (the rest is not logged)" } else { "" })
        }
        _ => String::from(text),
    }
}

impl Server {
    fn acct_now(&self) -> u64 {
        crate::sys::clock::unix_seconds() + self.acct.skew
    }

    /// Say something to connection `i`, dropping it if it cannot be reached.
    fn tell(&mut self, i: usize, msg: &Msg) {
        if self.conns[i].stream.send_all(&msg.encode(), SEND_TIMEOUT).is_err() {
            self.conns[i].gone = true;
        }
    }

    /// Turn connection `i` away with a reason, and drop it.
    fn refuse_conn(&mut self, i: usize, reason: &str) {
        self.tell(i, &Msg::Refuse { reason: String::from(reason) });
        self.conns[i].gone = true;
    }

    fn fail_result(&mut self, i: usize, text: &str, tries: u8) {
        self.tell(i, &Msg::AuthResult { ok: false, tries, text: String::from(text) });
    }

    fn weak_flag(&self) -> u8 {
        if self.acct.cfg.allow_weak_pins { flag::WEAK_OK } else { 0 }
    }

    /// What the server's accounts say about the player on connection `i` (or about a name about to join).
    fn account_msg(&self, logged_in: bool, name: &str) -> Msg {
        let mut flags = self.weak_flag();
        if logged_in {
            flags |= flag::LOGGED_IN;
            if self.acct.cfg.is_admin_name(name) {
                flags |= flag::ADMIN;
            }
        }
        Msg::Account { mode: self.acct.cfg.require.code(), flags }
    }

    /// Send a challenge and wait for the answer.
    fn challenge(&mut self, i: usize, p: Pending, rounds: u32, tries: u32, salt: keys::Salt) {
        let msg = Msg::AuthChallenge { op: p.op, flags: self.weak_flag(), rounds, tries: tries.min(255) as u8, salt, new_salt: p.new_salt, nonce: p.nonce };
        self.conns[i].acct.pending = Some(p);
        self.conns[i].acct.waited = 0.0;
        self.tell(i, &msg);
    }

    /// Wrong tries left for this name from this connection's address.
    fn tries_left(&self, name: &str, ip: Option<[u8; 4]>) -> u32 {
        let a = self.acct.accounts.get(name).map_or(5, |a| a.strikes.tries_left());
        let b = ip.and_then(|ip| self.acct.addrs.get(&ip)).map_or(5, |r| r.strikes.tries_left());
        a.min(b)
    }

    /// Whether a name may not be taken by a guest: it has an account, or it is an administrator's.
    fn name_is_protected(&self, name: &str) -> bool {
        self.acct.accounts.get(name).is_some() || self.acct.cfg.is_admin_name(name)
    }

    /// Per tick: the login limits fill up again, and challenges that nobody answers run out.
    pub(super) fn account_tick(&mut self, dt: f32) {
        self.acct.global.refill(dt);
        self.acct.write_wait -= dt;
        let mut expired = Vec::new();
        for (k, c) in self.conns.iter_mut().enumerate() {
            c.acct.ask_gap += dt;
            if c.acct.pending.is_some() {
                c.acct.waited += dt;
                let limit = if c.joined { INGAME_SECONDS } else { ANSWER_SECONDS };
                if c.acct.waited > limit {
                    expired.push(k);
                }
            }
        }
        for k in expired {
            if self.conns[k].joined {
                self.conns[k].acct.pending = None;
            } else if !self.conns[k].gone {
                self.refuse_conn(k, "no answer to the login in 15 seconds: please try again");
            }
        }
    }

    /// A client has said hello with this (cleaned) name and has been let through the other checks:
    /// is its name one that needs a login? `Err` is the reason to turn it away.
    pub(super) fn hello_gate(&mut self, i: usize, version: u16, name: &str) -> Result<Gate, String> {
        let now = self.acct_now();
        let ip = self.conns[i].stream.peer_ip();
        self.conns[i].acct.version = version;
        self.conns[i].acct.ip = ip;
        let lower = name.to_ascii_lowercase();
        let modern = version >= ACCOUNTS_SINCE;
        let account = self.acct.accounts.get(&lower).map(|a| (a.kind == KIND_PIN, a.salt, a.rounds));
        if let Some((is_pin, salt, rounds)) = account {
            if !modern {
                return Err(String::from(OLD_CLIENT_HAS_ACCOUNT));
            }
            if !is_pin {
                return Err(String::from("this account is not a PIN account: it needs a newer game to log in"));
            }
            if let Some(until) = self.acct.locked_until(&lower, ip, now) {
                return Err(lock_text(until, now));
            }
            let nonce = self.acct.rng.bytes16();
            let tries = self.tries_left(&lower, ip);
            let p = Pending { op: op::LOGIN, name: lower, shown: String::from(name), nonce, new_salt: [0; 16] };
            self.challenge(i, p, rounds, tries, salt);
            return Ok(Gate::Wait);
        }
        if self.acct.cfg.is_admin_name(&lower) {
            return Err(String::from("this name is reserved"));
        }
        if self.acct.cfg.require == Require::Yes {
            if !modern {
                return Err(String::from("this server needs accounts: update your game to make one"));
            }
            if self.acct.accounts.len() >= MAX_ACCOUNTS {
                return Err(String::from("this server has no room for more accounts"));
            }
            let new_salt = self.acct.rng.bytes16();
            let p = Pending { op: op::REGISTER, name: lower, shown: String::from(name), nonce: [0; 16], new_salt };
            let rounds = self.acct.cfg.rounds;
            self.challenge(i, p, rounds, 0, [0; 16]);
            return Ok(Gate::Wait);
        }
        Ok(Gate::Guest)
    }

    /// One of the account messages from connection `i`. False if the connection is finished
    /// with (dropped, or let in and the rest of its messages are for the main loop).
    pub(super) fn auth_message(&mut self, i: usize, msg: Msg, log: &mut Vec<String>) -> bool {
        let joined = self.conns[i].joined;
        match msg {
            Msg::AuthLogin { proof } if !joined => self.auth_login(i, proof, log),
            Msg::AuthRegister { key } => self.auth_register(i, key, log),
            Msg::AuthChange { proof, key } if joined => self.auth_change(i, proof, key, log),
            Msg::AuthAsk { op } if joined => {
                self.auth_ask(i, op);
                true
            }
            _ => {
                self.conns[i].gone = true;
                false
            }
        }
    }

    fn auth_login(&mut self, i: usize, proof: keys::Proof, log: &mut Vec<String>) -> bool {
        let Some(p) = self.conns[i].acct.pending.take().filter(|p| p.op == op::LOGIN) else {
            self.conns[i].gone = true;
            return false;
        };
        let (now, ip) = (self.acct_now(), self.conns[i].acct.ip);
        if !self.acct.global.take() {
            self.fail_result(i, "the server is busy with logins: please try again in a minute", 0);
            self.conns[i].gone = true;
            return false;
        }
        if let Some(until) = self.acct.locked_until(&p.name, ip, now) {
            self.refuse_conn(i, &lock_text(until, now));
            return false;
        }
        let ok = self.acct.accounts.get(&p.name).map_or(false, |a| keys::proof_matches(&a.key, &p.nonce, &p.name, &proof));
        if ok {
            if let Some(a) = self.acct.accounts.get_mut(&p.name) {
                // (The address keeps its wrong tries: a logged-in account must not wipe them,
                // or a guesser with an account of his own could reset the count between guesses.)
                a.strikes.forgive();
            }
            self.acct.accounts.dirty = true;
            self.conns[i].acct.account = Some(p.name.clone());
            return self.admit(i, p.shown, log);
        }
        let fail = self.acct.charge_failure(&p.name, ip, now);
        log.push(alloc::format!("a wrong PIN was given for {}", p.name));
        match fail {
            Fail::Locked(until) => self.refuse_conn(i, &lock_text(until, now)),
            Fail::Left(n) => {
                self.fail_result(i, &alloc::format!("wrong PIN: {} {} left", n, if n == 1 { "try" } else { "tries" }), n.min(255) as u8);
                self.conns[i].gone = true;
            }
        }
        false
    }

    /// Why this name cannot be registered now (without counting anything).
    fn registration_blocked(&self, lower: &str) -> Option<&'static str> {
        if self.acct.cfg.require == Require::No {
            Some("accounts are switched off on this server")
        } else if self.acct.accounts.get(lower).is_some() {
            Some("this name already has an account")
        } else if self.acct.cfg.is_admin_name(lower) {
            Some("this name is reserved")
        } else if self.acct.accounts.len() >= MAX_ACCOUNTS {
            Some("this server has no room for more accounts")
        } else if account_name(lower).is_none() {
            Some("this name cannot have an account")
        } else {
            None
        }
    }

    fn auth_register(&mut self, i: usize, key: keys::Key, log: &mut Vec<String>) -> bool {
        let joined = self.conns[i].joined;
        let Some(p) = self.conns[i].acct.pending.take().filter(|p| p.op == op::REGISTER) else {
            if !joined {
                self.conns[i].gone = true;
            }
            return joined;
        };
        let (now, ip) = (self.acct_now(), self.conns[i].acct.ip);
        let problem = match self.registration_blocked(&p.name) {
            Some(why) => Some(why),
            None if !self.acct.allow_registration(ip, now) => Some("too many accounts were made from your address lately: try again later"),
            None => None,
        };
        if let Some(why) = problem {
            if joined {
                self.fail_result(i, why, 0);
                return true;
            }
            self.refuse_conn(i, why);
            return false;
        }
        let account = Account { name: p.name.clone(), kind: String::from(KIND_PIN), salt: p.new_salt, key, rounds: self.acct.cfg.rounds, created: now, strikes: Strikes::default() };
        if !self.acct.accounts.put(account) {
            self.fail_result(i, "this name cannot have an account", 0);
            if !joined {
                self.conns[i].gone = true;
            }
            return joined;
        }
        self.conns[i].acct.account = Some(p.name.clone());
        log.push(alloc::format!("{} made an account", p.shown));
        if joined {
            self.tell(i, &Msg::AuthResult { ok: true, tries: 0, text: String::from("your name is now protected by your PIN: the game asks for it when you join") });
            let info = self.account_msg(true, &p.shown);
            self.tell(i, &info);
            true
        } else {
            self.admit(i, p.shown, log)
        }
    }

    /// An answer to a change challenge: the old PIN's proof and the new PIN's key.
    fn auth_change(&mut self, i: usize, proof: keys::Proof, key: keys::Key, log: &mut Vec<String>) -> bool {
        let Some(p) = self.conns[i].acct.pending.take().filter(|p| p.op == op::CHANGE) else { return true };
        let (now, ip) = (self.acct_now(), self.conns[i].acct.ip);
        if self.conns[i].acct.account.as_deref() != Some(p.name.as_str()) {
            return true;
        }
        if !self.acct.global.take() {
            self.fail_result(i, "the server is busy with logins: please try again in a minute", 0);
            return true;
        }
        if let Some(until) = self.acct.locked_until(&p.name, ip, now) {
            self.fail_result(i, &lock_text(until, now), 0);
            return true;
        }
        let ok = self.acct.accounts.get(&p.name).map_or(false, |a| keys::proof_matches(&a.key, &p.nonce, &p.name, &proof));
        if ok {
            if let Some(a) = self.acct.accounts.get_mut(&p.name) {
                a.key = key;
                a.salt = p.new_salt;
                a.strikes.forgive();
            }
            self.acct.accounts.dirty = true;
            log.push(alloc::format!("{} changed their PIN", p.shown));
            self.tell(i, &Msg::AuthResult { ok: true, tries: 0, text: String::from("your PIN is changed") });
        } else {
            let text = match self.acct.charge_failure(&p.name, ip, now) {
                Fail::Locked(until) => lock_text(until, now),
                Fail::Left(n) => alloc::format!("wrong PIN: {} {} left", n, if n == 1 { "try" } else { "tries" }),
            };
            self.fail_result(i, &text, 0);
        }
        true
    }

    /// A player in the game asks for a challenge: to register (`/register`) or to change the PIN (`/pin`).
    fn auth_ask(&mut self, i: usize, what: u8) {
        if self.conns[i].acct.ask_gap < 1.0 {
            self.fail_result(i, "please wait a moment", 0);
            return;
        }
        self.conns[i].acct.ask_gap = 0.0;
        let (now, ip) = (self.acct_now(), self.conns[i].acct.ip);
        let shown = self.conns[i].name.clone();
        let lower = shown.to_ascii_lowercase();
        match what {
            op::REGISTER => {
                if self.conns[i].acct.account.is_some() {
                    self.fail_result(i, "you have an account already: change its PIN with /pin OLD NEW", 0);
                } else if let Some(why) = self.registration_blocked(&lower) {
                    self.fail_result(i, why, 0);
                } else {
                    let new_salt = self.acct.rng.bytes16();
                    let rounds = self.acct.cfg.rounds;
                    self.challenge(i, Pending { op: op::REGISTER, name: lower, shown, nonce: [0; 16], new_salt }, rounds, 0, [0; 16]);
                }
            }
            op::CHANGE => {
                let Some(name) = self.conns[i].acct.account.clone() else {
                    self.fail_result(i, "you have no account yet: make one with /register PIN PIN", 0);
                    return;
                };
                if let Some(until) = self.acct.locked_until(&name, ip, now) {
                    self.fail_result(i, &lock_text(until, now), 0);
                    return;
                }
                let Some((salt, rounds)) = self.acct.accounts.get(&name).map(|a| (a.salt, a.rounds)) else { return };
                let (nonce, new_salt) = (self.acct.rng.bytes16(), self.acct.rng.bytes16());
                let tries = self.tries_left(&name, ip);
                self.challenge(i, Pending { op: op::CHANGE, name, shown, nonce, new_salt }, rounds, tries, salt);
            }
            _ => {}
        }
    }

    /// Let connection `i` join the game as `name` (a guest, or the account it has just logged in to):
    /// tell it about the world and who is here, and tell the others about it. False if it was dropped.
    pub(super) fn admit(&mut self, i: usize, name: String, log: &mut Vec<String>) -> bool {
        let mut name = name;
        let logged = self.conns[i].acct.account.is_some();
        let version = self.conns[i].acct.version;
        if logged {
            // The owner of the account is here: an older connection of it (one that was lost
            // and has not timed out yet) makes way.
            for k in 0..self.conns.len() {
                if k != i && self.conns[k].joined && !self.conns[k].gone && self.conns[k].name.eq_ignore_ascii_case(&name) {
                    // (Where it was and how it was are noted now, for the newcomer to be sent back to.)
                    let notes = Self::leave_notes(&self.conns[k]);
                    self.apply_notes(notes);
                    self.refuse_conn(k, "you logged in from another place");
                }
            }
        } else if let Some(k) = self.stale_twin(i, &name) {
            // A guest who comes again from the same address before their old connection has
            // timed out (a crash, a dropped wifi): the old one makes way, and they keep their name.
            let notes = Self::leave_notes(&self.conns[k]);
            self.apply_notes(notes);
            self.refuse_conn(k, "you joined again from this address");
            log.push(alloc::format!("{} came back; the old connection was replaced", name));
        }
        // (Looked at again here, with a player who makes way no longer counted.)
        if self.players() >= self.cfg.max_players {
            self.refuse_conn(i, "the server is full");
            return false;
        }
        // Two players never share a name (capital letters do not
        // make a different one: permissions go by the name without them). A guest also
        // keeps off names that have an account or are an administrator's.
        let taken = |n: &str, conns: &[Conn]| conns.iter().any(|c| c.joined && !c.gone && c.name.eq_ignore_ascii_case(n));
        let mut n = 2;
        let base = name.clone();
        while taken(&name, &self.conns) || (!logged && self.name_is_protected(&name)) {
            name = alloc::format!("{}{}", base, n);
            n += 1;
        }
        let id = self.fresh_id();
        // Whoever is first through the door holds the keys (unless the server names its administrators).
        if self.ops.is_empty() && self.acct.cfg.admins.is_empty() {
            self.ops.push(name.clone());
            self.ops_dirty = true;
        }
        // Tell the newcomer about the world and who is here.
        let mut out = Msg::Welcome { id, seed: self.cfg.seed, time: self.time, kind: self.cfg.kind.clone() }.encode();
        let me = crate::net::version::local();
        out.extend(Msg::Info { build: me.build, release: me.release, commit: me.commit }.encode());
        if version >= ACCOUNTS_SINCE {
            out.extend(self.account_msg(logged, &name).encode());
        }
        // Clients that know big skins are told they may send them.
        if version >= crate::net::skinslot::HD_PROTOCOL {
            out.extend(Msg::SkinCaps { max_side: crate::skin_grid::MAX_SIDE as u16, max_bytes: crate::proto::MAX_SKIN as u32 }.encode());
        }
        // The content pack first (even when empty), so the
        // edits that follow are read with the right blocks.
        let pack = &self.cfg.pack[..];
        let parts: Vec<&[u8]> = if pack.is_empty() { alloc::vec![&pack[..]] } else { pack.chunks(crate::proto::MAX_PACK_PART).collect() };
        for (k, part) in parts.iter().enumerate() {
            out.extend(Msg::Pack { last: k + 1 == parts.len(), data: part.to_vec() }.encode());
        }
        // Then what mods can be downloaded (even if none).
        out.extend(Msg::ModList { mods: self.cfg.mods.iter().map(|m| m.info()).collect() }.encode());
        // Who runs the creatures and the last lines of chat (see net/sync).
        out.extend(self.sync_intro());
        for other in self.conns.iter().filter(|c| c.joined && !c.gone) {
            out.extend(Msg::Join { id: other.id, name: other.name.clone() }.encode());
            out.extend(other.skin.wire_for(other.id, version));
            if let Some((pos, yaw, pitch, held, pose)) = other.last_move {
                out.extend(Msg::Move { id: other.id, pos, yaw, pitch, held, pose }.encode());
            }
            if !other.effects.is_empty() {
                out.extend(Msg::Effects { id: other.id, list: other.effects.clone() }.encode());
            }
            // Their health and armour, and the emote they are playing.
            out.extend(Self::player_intro(other));
        }
        let all: Vec<(i32, i32, i32, u16)> = self.edits.iter().map(|(k, &b)| (k.0, k.1, k.2, b)).collect();
        for part in all.chunks(4000) {
            out.extend(Msg::Edits { list: part.to_vec() }.encode());
        }
        for ((channel, key), data) in &self.kept {
            out.extend(Msg::State { id: 0, channel: *channel, keep: true, key: key.clone(), data: data.clone() }.encode());
        }
        // Back where they left off (an account's place is kept under its name, a guest's under theirs).
        let spot_key = self.conns[i].acct.account.clone().unwrap_or_else(|| name.to_ascii_lowercase());
        // And as well as they were, unless they died there: then they start again at the spawn, at full health.
        let (health, died) = self.health_intro(&spot_key);
        if let (Some(s), false) = (self.spots.get(&spot_key), died) {
            out.extend(Msg::State { id: 0, channel: crate::proto::channel::PLAYER, keep: false, key: String::from("you"), data: spot_text(s).into_bytes() }.encode());
        }
        out.extend(health);
        if self.conns[i].stream.send_all(&out, 4_000).is_err() {
            self.conns[i].gone = true;
            return false;
        }
        self.conns[i].id = id;
        self.conns[i].proto = version;
        self.conns[i].name = name.clone();
        self.conns[i].joined = true;
        self.sync.joined += 1;
        self.conns[i].sync.order = self.sync.joined;
        // (A player who has just joined may ask for a challenge at once.)
        self.conns[i].acct.ask_gap = 5.0;
        self.send_perms(Some(i));
        log.push(alloc::format!("{} joined ({} online)", name, self.players()));
        self.broadcast(&Msg::Join { id, name }, id);
        true
    }

    /// Whether `who` is an administrator who is logged in now.
    pub fn is_admin(&self, who: &str) -> bool {
        self.acct.cfg.is_admin_name(who) && self.conns.iter().any(|c| c.joined && !c.gone && c.acct.account.is_some() && c.name.eq_ignore_ascii_case(who))
    }

    /// A PIN of six digits that is not a trivial one.
    fn random_pin(&mut self) -> String {
        for _ in 0..100 {
            let digits: String = (0..6).map(|_| char::from(b'0' + self.acct.rng.below(10) as u8)).collect();
            if !pin::is_weak(&digits) {
                return digits;
            }
        }
        String::from("493017")
    }

    /// `/accounts` and `/resetpin NAME [PIN]`, for administrators.
    pub(super) fn admin_command(&mut self, who: &str, w: &[&str]) -> String {
        if !self.is_admin(who) {
            return String::from("only administrators may do that");
        }
        let arg = |k: usize| w.get(k).copied().unwrap_or("");
        let now = self.acct_now();
        match arg(0).to_ascii_lowercase().as_str() {
            "/accounts" => {
                let mut text = alloc::format!("accounts ({}): ", self.acct.accounts.len());
                let mut shown = 0;
                for a in self.acct.accounts.iter() {
                    let mut entry = a.name.clone();
                    if self.acct.cfg.is_admin_name(&a.name) {
                        entry.push('*');
                    }
                    if let Some(until) = a.strikes.locked(now) {
                        entry.push_str(&alloc::format!(" (locked until {} UTC)", crate::sys::clock::clock_text(until)));
                    }
                    if text.len() + entry.len() > 170 {
                        break;
                    }
                    if shown > 0 {
                        text.push_str(", ");
                    }
                    text.push_str(&entry);
                    shown += 1;
                }
                if shown < self.acct.accounts.len() {
                    text.push_str(&alloc::format!(" ... and {} more", self.acct.accounts.len() - shown));
                }
                text
            }
            "/resetpin" => {
                let Some(name) = account_name(arg(1)) else { return String::from("use /resetpin NAME [PIN]") };
                if self.acct.accounts.get(&name).is_none() {
                    return alloc::format!("{} has no account", name);
                }
                let new_pin = if arg(2).is_empty() { self.random_pin() } else { String::from(arg(2)) };
                if let Err(e) = pin::check(&new_pin, self.acct.cfg.allow_weak_pins) {
                    return e;
                }
                let (salt, rounds) = (self.acct.rng.bytes16(), self.acct.cfg.rounds);
                let key = keys::derive_key(&new_pin, &salt, rounds);
                if let Some(a) = self.acct.accounts.get_mut(&name) {
                    a.salt = salt;
                    a.key = key;
                    a.rounds = rounds;
                    a.strikes.forgive();
                }
                self.acct.accounts.dirty = true;
                alloc::format!("the PIN of {} is now {}: tell them privately; they can change it with /pin OLD NEW", name, new_pin)
            }
            _ => String::new(),
        }
    }

    /// The reply to `/register`, `/pin` and the like typed to the server as plain chat: only a
    /// game that knows accounts can use them (it never sends the PIN itself).
    pub(super) fn plain_account_command(&self) -> String {
        String::from("accounts need a newer game: update it to use /register and /pin (a PIN is never typed to the server as chat)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_never_holds_a_pin() {
        assert_eq!(loggable("/register 4821 4821"), "/register (the rest is not logged)");
        assert_eq!(loggable("/PIN 4821 7390"), "/pin (the rest is not logged)");
        assert_eq!(loggable("/resetpin bob 482915"), "/resetpin (the rest is not logged)");
        assert_eq!(loggable("/register"), "/register");
        assert_eq!(loggable("/kick bob"), "/kick bob");
        assert_eq!(loggable("hello 4821"), "hello 4821");
    }
}
