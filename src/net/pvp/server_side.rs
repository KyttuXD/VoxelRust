//! The server's half of player versus player: a child module of `net::server`
//! so that it reaches the fields of the server and its connections.
//! The rules themselves are in `net/pvp/check.rs`.

use super::*;
use crate::net::pvp::check::{self, Check};
use crate::net::pvp::state::ConnPvp;
use crate::net::pvp::wire::{pack_dir, Gear, Hit, Hurt, Seen};
use crate::proto::{channel, PERM_CREATIVE, PERM_PVP};

/// Players farther than this from the one who was hit are not told of it (blocks).
const SEEN_RANGE: f64 = 64.0;

impl Server {
    /// A new player has been let in: their spawn protection starts.
    pub(super) fn pvp_admitted(&mut self, i: usize) {
        self.conns[i].pvp = ConnPvp::joined(self.pvp.now, self.pvp.rules.protect);
    }

    /// The gear message of the player at `i`: their armour and whether they live.
    /// A game that sends it can be fought. Dying right after a hit is the
    /// hitter's kill: told to everybody.
    pub(super) fn pvp_gear(&mut self, i: usize, data: &[u8], log: &mut Vec<String>) {
        let Some(g) = Gear::unpack(data) else { return };
        let (now, protect) = (self.pvp.now, self.pvp.rules.protect);
        let change = self.conns[i].pvp.take_gear(g, now, protect);
        if change != Some(false) {
            return;
        }
        let victim = self.conns[i].name.clone();
        let Some(killer) = self.conns[i].pvp.killer(now).map(String::from) else { return };
        self.conns[i].pvp.hurt_by = None;
        *self.pvp.kills.entry(killer.to_ascii_lowercase()).or_insert(0) += 1;
        let text = alloc::format!("{} was slain by {}", victim, killer);
        log.push(text.clone());
        self.announce(&text);
    }

    /// A hit message from the player at `i`. Whatever is wrong with it, nothing happens.
    pub(super) fn pvp_hit(&mut self, i: usize, data: &[u8]) {
        let Some(hit) = Hit::unpack(data) else { return };
        let Some(k) = self.conns.iter().position(|c| c.joined && !c.gone && c.id == hit.target) else { return };
        if k == i {
            return;
        }
        let now = self.pvp.now;
        let (aflags, tflags) = (self.flags_for(&self.conns[i].name), self.flags_for(&self.conns[k].name));
        let (a, t) = (&self.conns[i], &self.conns[k]);
        let c = Check {
            pvp_on: self.pvp.rules.on,
            attacker_may: aflags & PERM_PVP != 0,
            target_may: tflags & PERM_PVP != 0,
            attacker_creative: aflags & PERM_CREATIVE != 0,
            target_creative: tflags & PERM_CREATIVE != 0,
            attacker_alive: a.pvp.gear.alive,
            target_alive: t.pvp.gear.alive,
            target_capable: t.pvp.capable,
            attacker_pos: a.last_move.map(|m| m.0),
            target_pos: t.last_move.map(|m| m.0),
            since_last: a.pvp.since_last(now),
            protected_left: t.pvp.protected_left(now),
        };
        if check::judge(&c).is_err() {
            return;
        }
        let (Some(apos), Some(tpos)) = (c.attacker_pos, c.target_pos) else { return };
        let yaw = a.last_move.map_or(0.0, |m| m.1);
        let (aid, aname, tid) = (a.id, a.name.clone(), t.id);
        let blow = check::blow(check::item_base(hit.item), hit.damage);
        let taken = check::after_armour(blow, t.pvp.gear);
        let dir = pack_dir(check::knock_dir(apos, tpos, yaw));
        self.conns[i].pvp.last_hit = Some(now);
        self.conns[k].pvp.hurt_by = Some((aname, now));
        let hurt = Msg::State { id: 0, channel: channel::PVP_HURT, keep: false, key: String::new(), data: Hurt { attacker: aid, damage: taken.clamp(1, 255) as u8, dir }.pack() };
        self.send_to(tid, &hurt);
        // Those near the one hit (and the one who hit) see the flash.
        let seen = Msg::State { id: 0, channel: channel::PVP_SEEN, keep: false, key: String::new(), data: Seen { victim: tid, attacker: aid }.pack() };
        let bytes = seen.encode();
        for c in self.conns.iter_mut() {
            if !c.joined || c.gone || c.id == tid {
                continue;
            }
            let close = c.last_move.map_or(true, |m| check::near(m.0, tpos, SEEN_RANGE));
            if close && c.stream.send_all(&bytes, SEND_TIMEOUT).is_err() {
                c.gone = true;
            }
        }
    }

    /// `/pvp` (anyone may ask), `/pvp on|off` and `/kills [NAME]`. None for any other command.
    pub(super) fn pvp_command(&mut self, who: &str, w: &[&str], is_op: bool) -> Option<String> {
        let arg = |k: usize| w.get(k).copied().unwrap_or("");
        match arg(0).to_ascii_lowercase().as_str() {
            "/pvp" if arg(1).is_empty() => Some(alloc::format!("pvp is {} (spawn protection {} seconds)", if self.pvp.rules.on { "on" } else { "off" }, self.pvp.rules.protect)),
            "/pvp" if !is_op => Some(String::from("only operators may do that")),
            "/pvp" => match crate::net::pvp::rules::parse_switch(arg(1)) {
                Some(on) => {
                    self.pvp.rules.on = on;
                    self.pvp.by_command = true;
                    self.ops_dirty = true;
                    // (What each player may do now includes whether fighting is on.)
                    self.send_perms(None);
                    self.announce(if on { "Fighting between players is now allowed" } else { "Fighting between players is now off" });
                    Some(String::new())
                }
                None => Some(String::from("use /pvp on or /pvp off")),
            },
            "/kills" => {
                let name = if arg(1).is_empty() { who } else { arg(1) };
                let n = self.pvp.kills.get(&name.to_ascii_lowercase()).copied().unwrap_or(0);
                Some(alloc::format!("{} has {} kill{} since the server started", name, n, if n == 1 { "" } else { "s" }))
            }
            _ => None,
        }
    }

    /// The lines of the permissions file that belong to player versus player: the
    /// switch, when a command set it.
    pub(super) fn pvp_text(&self) -> String {
        if self.pvp.by_command {
            alloc::format!("pvp {}\n", if self.pvp.rules.on { "on" } else { "off" })
        } else {
            String::new()
        }
    }

    /// Read a `pvp on|off` line of the permissions file.
    pub(super) fn pvp_load_line(&mut self, value: &str) -> bool {
        match crate::net::pvp::rules::parse_switch(value) {
            Some(on) => {
                self.pvp.rules.on = on;
                self.pvp.by_command = true;
                true
            }
            None => false,
        }
    }
}
