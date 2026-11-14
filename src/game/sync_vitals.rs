//! The player's health, hunger and worn armour told to the others in a shared
//! game (see `net/sync`), the death notice, and the health the server gives
//! back on joining again.

use crate::net::sync::vitals::{Vitals, MOST_FOOD, MOST_HEALTH};
use crate::session::Session;
use alloc::string::String;

/// Seconds between messages when something changed (at most four a second).
const FASTEST: f32 = 0.25;
/// Seconds between messages when nothing did (it keeps the ping and a missed message fresh).
const SLOWEST: f32 = 2.0;
/// Seconds after joining before the first message: the server's answer about
/// our old health must come first, or the first message would say full health.
const WARM_UP: f32 = 0.6;

/// What the session keeps for this.
#[derive(Default)]
pub struct VitalsSync {
    age: f32,
    timer: f32,
    last: Option<Vitals>,
    died_sent: bool,
    /// What last hurt the player, for the death notice.
    pub cause: String,
    /// The server has said how we were (it does that once, on joining).
    restored: bool,
}

impl VitalsSync {
    /// Send the next message at once (something outside changed what we are).
    pub fn resend(&mut self) {
        self.last = None;
        self.timer = SLOWEST;
    }
}

impl Session {
    /// How the player is now, as the others see it.
    pub fn vitals_now(&self) -> Vitals {
        let mut worn = [0u16; crate::net::sync::vitals::SLOTS];
        for (w, slot) in worn.iter_mut().zip(self.inv.worn.iter()) {
            *w = slot.map_or(0, |s| s.item.saturating_add(1));
        }
        Vitals { hp: self.health.clamp(0, MOST_HEALTH as i32) as u8, max: self.max_health().clamp(1, MOST_HEALTH as i32) as u8, food: self.hunger.clamp(0, MOST_FOOD as i32) as u8, worn, ping: 0 }.held_to_range()
    }

    /// Every frame of a shared game: tell the others how we are when it changed (at most four
    /// times a second, and every couple of seconds regardless), and that we died.
    pub(crate) fn share_vitals(&mut self, dt: f32) {
        if self.net.is_none() {
            return;
        }
        let now = self.vitals_now();
        let dead = self.dead;
        let v = &mut self.vsync;
        v.age += dt;
        v.timer += dt;
        if v.age < WARM_UP {
            return;
        }
        let send_status = (v.last != Some(now) && v.timer >= FASTEST) || v.timer >= SLOWEST;
        let send_death = dead && !v.died_sent;
        if !dead {
            v.died_sent = false;
        }
        if send_status {
            v.last = Some(now);
            v.timer = 0.0;
        }
        if send_death {
            v.died_sent = true;
        }
        let cause = if send_death { Some(core::mem::take(&mut v.cause)) } else { None };
        if let Some(n) = self.net.as_mut() {
            if send_status || send_death {
                n.send_vitals(&now);
            }
            if let Some(cause) = cause {
                n.send_died(&cause);
            }
        }
    }

    /// While the world is still loading or the game is not running: keep the server
    /// from thinking we are gone (it drops a player who says nothing for a minute).
    pub(crate) fn keep_alive(&mut self, dt: f32) {
        let held = self.held().map_or(0, |h| h.0 as u16 + 1);
        let (pos, yaw, pitch) = (self.player.pos, self.player.yaw, self.player.pitch);
        if let Some(n) = self.net.as_mut() {
            n.send_move(dt, pos, yaw, pitch, held);
        }
    }

    /// The server tells how this player was when they left (`health max hunger`, once, on joining).
    pub(crate) fn restore_health(&mut self, text: &str) {
        if self.vsync.restored {
            return;
        }
        let w: alloc::vec::Vec<i32> = text.split(' ').filter_map(|v| v.parse::<i32>().ok()).collect();
        if let [hp, _max, food] = w[..] {
            self.vsync.restored = true;
            let top = self.max_health();
            if hp >= 1 && hp <= MOST_HEALTH as i32 {
                self.health = hp.min(top);
            }
            if (0..=MOST_FOOD as i32).contains(&food) {
                self.hunger = food;
            }
            self.vsync.resend();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resend_makes_the_next_message_go_at_once() {
        let mut v = VitalsSync::default();
        v.last = Some(Vitals { hp: 20, max: 20, food: 20, worn: [0; 4], ping: 0 });
        v.resend();
        assert_eq!(v.last, None);
        assert!(v.timer >= SLOWEST);
    }
}
