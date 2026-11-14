//! Player versus player and the punch, the playing game's half (the server's is
//! `net/pvp`, see docs/MULTIPLAYER.md).
//!
//! * A click with nothing in reach swings the arm all the same (a short
//!   `punch` that is not a cooldown: mining is never held up by it).
//! * A click on another player sends a hit to the server (only the swing and
//!   the sound are done here at once); the server decides, and tells the one
//!   who was hit, who then takes the damage from their own health.
//! * The game also tells the server its armour and whether it is alive, so
//!   that the server can work out what a hit does and who has died.

use crate::audio::{Audio, Sound};
use crate::math::forward;
use crate::net::pvp::{aim, check, wire};
use crate::proto::{channel, PERM_PVP};
use crate::session::{GameMode, Session};
use alloc::string::String;

/// How long a punch swing runs (the same as the blow of a tool, see `hand_swing`).
pub const PUNCH_SECONDS: f32 = 0.35;
/// A new punch may start when the last has this much left: a click every 0.25 s.
pub const PUNCH_AGAIN: f32 = 0.10;
/// How hard a hit throws the one who takes it (blocks a second): along the ground, and up.
const KNOCK_SPEED: f32 = 5.5;
const KNOCK_UP: f32 = 3.8;
/// Longest the server may go without hearing our gear, and the shortest between two messages.
const GEAR_REFRESH: f32 = 10.0;
const GEAR_GAP: f32 = 0.25;
/// How far away the sound of someone else's fight carries (blocks).
const HEAR: f64 = 24.0;
/// How long a hit player's model glows red.
pub const FLASH_SECONDS: f32 = 0.35;

/// What the game remembers of its talk with the server about fights.
#[derive(Default)]
pub struct Client {
    /// The gear the server was last told.
    sent: Option<wire::Gear>,
    /// Seconds since then.
    since: f32,
}

/// The gear to tell the server: armour as whole points, and whether the player lives.
pub fn gear_of(defense: f32, toughness: f32, alive: bool) -> wire::Gear {
    let whole = |v: f32| if v >= 255.0 { 255 } else if v > 0.0 { v as u8 } else { 0 };
    wire::Gear { defense: whole(defense), toughness: whole(toughness), alive }
}

/// Where the swing of a punch is, 0..1, `left` seconds before it ends (None at rest).
pub fn punch_phase(left: f32) -> Option<f32> {
    if left > 0.0 {
        Some((1.0 - left / PUNCH_SECONDS).clamp(0.001, 1.0))
    } else {
        None
    }
}

impl Session {
    /// The time left of the swing the arm is making: a blow's cooldown or a punch.
    pub fn blow_left(&self) -> f32 {
        self.cooldown[0].max(self.punch)
    }

    /// A click that hit nothing: the arm swings anyway (at most four times a second), and
    /// nothing is held up for it.
    pub fn air_punch(&mut self) {
        if self.dead || self.punch > PUNCH_AGAIN {
            return;
        }
        self.punch = PUNCH_SECONDS;
    }

    /// Try to hit the other player the crosshair is on (not behind a nearer block):
    /// sent to the server, which decides. Returns whether there was one, so that the
    /// click is spent (the swing and the sound are done at once; the damage is the
    /// server's word). `base_damage` is what the item in hand does, `held` the item
    /// as index plus one (0 for the bare hand).
    pub fn strike_player(&mut self, reach: f32, base_damage: i32, held: u16, audio: &mut Audio) -> bool {
        if self.mode != GameMode::Survival || self.dead || self.sleeping || !self.may(PERM_PVP) {
            return false;
        }
        let near_block = self.target.as_ref().map_or(f32::MAX, |h| h.dist);
        let Some(net) = self.net.as_ref() else { return false };
        let eye = self.player.eye();
        let d = forward(self.player.yaw, self.player.pitch);
        let bodies = net.players.iter().filter(|(_, p)| p.seen).map(|(id, p)| (*id, p.pos));
        let Some((id, t)) = aim::hit_test(eye, [d.x as f64, d.y as f64, d.z as f64], reach, bodies) else { return false };
        if t >= near_block {
            return false;
        }
        // The same blow as against a creature: strength and weakness, then the damage attribute.
        let blow = crate::game::effects::apply::melee_damage(&self.fx, &crate::game::effects::apply::lookup, base_damage.max(1));
        let blow = crate::game::gear::session_gear::scaled_blow(blow, self.attrs.value(crate::game::gear::attributes::DAMAGE));
        let damage = blow.clamp(1, check::MAX_BLOW) as u8;
        self.cooldown[0] = 0.35;
        self.wear_held(1, audio);
        audio.play(Sound::Hurt, 0.8);
        if let Some(n) = self.net.as_mut() {
            n.send_state(channel::PVP_HIT, false, "", &wire::Hit { target: id, item: held, damage }.pack());
        }
        true
    }

    /// Once a frame in a shared game: tell the server our armour and whether we are alive
    /// when that changes (and now and then, so that it never forgets us).
    pub fn pvp_tick(&mut self, dt: f32) {
        if self.net.is_none() {
            return;
        }
        self.pvp.since += dt;
        let (defense, toughness) = self.armor_points();
        let gear = gear_of(defense, toughness, !self.dead);
        let changed = self.pvp.sent != Some(gear);
        if (changed && self.pvp.since >= GEAR_GAP) || self.pvp.since >= GEAR_REFRESH {
            self.pvp.sent = Some(gear);
            self.pvp.since = 0.0;
            if let Some(n) = self.net.as_mut() {
                n.send_state(channel::PVP_GEAR, false, "", &gear.pack());
            }
        }
    }

    /// The server's news of a fight (only the server, player number 0, is believed):
    /// we were hit, or someone near was.
    pub fn pvp_message(&mut self, from: u16, chan: u8, data: &[u8]) {
        if from != 0 {
            return;
        }
        match chan {
            channel::PVP_HURT => {
                let Some(h) = wire::Hurt::unpack(data) else { return };
                if self.mode != GameMode::Survival || self.dead || h.damage == 0 {
                    return;
                }
                // The server has taken our armour off the blow already ("pvp" is no cause armour
                // takes a second share of); the armour wears as it would for any blow.
                self.ev.pending_cause = String::from("pvp");
                self.hurt_later((h.damage as i32).min(check::MAX_BLOW));
                let dir = wire::unpack_dir(h.dir);
                if !self.player.flying {
                    self.player.vel[0] += dir[0] * KNOCK_SPEED;
                    self.player.vel[2] += dir[1] * KNOCK_SPEED;
                    if self.player.vel[1] < KNOCK_UP {
                        self.player.vel[1] = KNOCK_UP;
                    }
                }
                let broke = crate::game::gear::armor::wear_worn(&mut self.inv.worn, 1, &crate::game::gear::wear::durability_of);
                for slot in broke {
                    self.sound_queue.push(String::from("break"));
                    self.say("YOUR ARMOUR BROKE");
                    crate::script::gear_bridge::armor_changed(self, slot);
                }
            }
            channel::PVP_SEEN => {
                let Some(s) = wire::Seen::unpack(data) else { return };
                let me = self.net.as_ref().map_or(0, |n| n.id);
                if s.victim == me {
                    return;
                }
                let mut at = None;
                if let Some(p) = self.net.as_mut().and_then(|n| n.players.get_mut(&s.victim)) {
                    p.hurt = FLASH_SECONDS;
                    at = Some(p.pos);
                }
                // (Our own blow made its sound when we struck.)
                if let (Some(at), true) = (at, s.attacker != me) {
                    if check::near(self.player.pos, at, HEAR) {
                        self.sound_queue.push(String::from("hurt"));
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gear_is_told_in_whole_points_and_never_more_than_a_byte() {
        assert_eq!(gear_of(7.9, 2.5, true), wire::Gear { defense: 7, toughness: 2, alive: true });
        assert_eq!(gear_of(-3.0, f32::NAN, false), wire::Gear { defense: 0, toughness: 0, alive: false });
        assert_eq!(gear_of(1000.0, 300.0, true), wire::Gear { defense: 255, toughness: 255, alive: true });
    }

    #[test]
    fn a_punch_runs_from_the_start_of_the_swing_to_its_end() {
        assert_eq!(punch_phase(0.0), None);
        assert_eq!(punch_phase(-1.0), None);
        let start = punch_phase(PUNCH_SECONDS).unwrap();
        assert!(start > 0.0 && start < 0.01);
        let mid = punch_phase(PUNCH_SECONDS * 0.5).unwrap();
        assert!((mid - 0.5).abs() < 1e-5);
        assert!(punch_phase(0.001).unwrap() > 0.99);
    }

    #[test]
    fn a_new_punch_may_start_a_quarter_second_after_the_last() {
        assert!((PUNCH_SECONDS - PUNCH_AGAIN - 0.25).abs() < 1e-6);
    }
}
