//! What a play session does with gear: armour taking a share of a blow,
//! wear for what is in the hand, putting armour on and off, and finding a
//! stack by the slot number scripts use.

use crate::audio::Audio;
use crate::game::gear::{armor, attributes, wear};
use crate::inventory::{Slot, SLOTS};
use crate::session::{GameMode, Session};

/// Slot numbers scripts use: 0..35 the pack, `HELD` the item in hand,
/// `CHEST` + n the open chest's slot n, `WORN` + n a worn piece (0 head ..
/// 3 feet).
pub const HELD: i64 = -1;
pub const CHEST: i64 = 100;
pub const WORN: i64 = 200;

/// A player's blow after the damage attribute (a multiplier, 1.0 for none).
pub fn scaled_blow(blow: i32, multiplier: f32) -> i32 {
    if blow <= 0 {
        return blow;
    }
    let v = blow as f32 * multiplier + 0.5;
    if v.is_finite() {
        (v as i32).clamp(0, 100_000)
    } else {
        blow
    }
}

impl Session {
    /// The most health the player has now (20 unless a mod changed it).
    pub fn max_health(&self) -> i32 {
        self.attrs.max_health()
    }

    /// Defense points and toughness of what is worn, with the `armor` attribute added.
    pub fn armor_points(&self) -> (f32, f32) {
        let (d, t) = armor::totals(&self.inv.worn, &armor::piece);
        (d + self.attrs.value(attributes::ARMOR), t)
    }

    /// A blow of `amount` after worn armour. A blow that armour helps against
    /// (see `armor::blocks_cause`) wears each piece worn by one use.
    pub fn armor_blow(&mut self, amount: i32, cause: &str, by_creature: bool, audio: &mut Audio) -> i32 {
        if amount <= 0 || !armor::blocks_cause(cause, by_creature) {
            return amount;
        }
        let (defense, toughness) = self.armor_points();
        if defense <= 0.0 {
            return amount;
        }
        let out = armor::reduced(amount, defense, toughness);
        if self.mode == GameMode::Survival {
            let broke = armor::wear_worn(&mut self.inv.worn, 1, &wear::durability_of);
            for slot in broke {
                audio.play_named("break", 1.0);
                self.say("YOUR ARMOUR BROKE");
                crate::script::gear_bridge::armor_changed(self, slot);
            }
        }
        out
    }

    /// Wear the item in hand by `amount` uses (never in creative). It
    /// breaks, with a sound, when the last use is gone.
    pub fn wear_held(&mut self, amount: u32, audio: &mut Audio) {
        if self.mode == GameMode::Creative {
            return;
        }
        let sel = self.selected;
        if wear::wear_slot(&mut self.inv.slots[sel], amount) == wear::Outcome::Broke {
            audio.play_named("break", 1.0);
            self.say("YOUR TOOL BROKE");
        }
    }

    /// Put the armour in pack slot `from` on (shift-click, or right click).
    pub fn put_on_armor(&mut self, from: usize) -> armor::Put {
        let Some(Some(s)) = self.inv.slots.get(from).copied() else { return armor::Put::NotArmor };
        let slot = armor::piece(s.item).map(|p| p.slot as usize);
        let r = armor::equip(&mut self.inv, from, &armor::piece);
        if let (armor::Put::Worn, Some(slot)) = (r, slot) {
            crate::script::gear_bridge::armor_changed(self, slot);
        } else if r == armor::Put::NoRoom {
            self.say("INVENTORY FULL");
        }
        r
    }

    /// Take off what is worn in `slot`: it goes to the pack, or is set down
    /// at the feet when the pack is full. Whether anything was worn.
    pub fn take_off_armor(&mut self, slot: usize) -> bool {
        match armor::take_off(&mut self.inv, slot) {
            None => false,
            Some(left) => {
                if let Some(rest) = left {
                    let p = self.player.pos;
                    self.drop_stack([p[0], p[1] + 0.8, p[2]], [0.0, 2.0, 0.0], rest);
                }
                crate::script::gear_bridge::armor_changed(self, slot);
                true
            }
        }
    }

    /// Set a worn slot to a stack (or empty it) as a script asks; what was
    /// worn goes to the pack or the ground. Whether it was done: the item
    /// has to be armour for that slot.
    pub fn set_worn(&mut self, slot: usize, stack: Slot) -> bool {
        if slot >= armor::SLOTS {
            return false;
        }
        if let Some(s) = stack {
            if armor::piece(s.item).map_or(true, |p| p.slot as usize != slot) {
                return false;
            }
        }
        if let Some(old) = self.inv.worn[slot].take() {
            let left = self.inv.add_stack(old);
            if left > 0 {
                let p = self.player.pos;
                self.drop_stack([p[0], p[1] + 0.8, p[2]], [0.0, 2.0, 0.0], old.with_count(left));
            }
        }
        self.inv.worn[slot] = stack.map(|s| s.with_count(1));
        crate::script::gear_bridge::armor_changed(self, slot);
        true
    }

    /// The stack a script's slot number names (see [`HELD`], [`CHEST`], [`WORN`]).
    /// `None` for a slot that does not exist; `Some(slot)` may be empty.
    pub fn slot_mut(&mut self, n: i64) -> Option<&mut Slot> {
        match n {
            HELD => Some(&mut self.inv.slots[self.selected]),
            0..=35 => self.inv.slots.get_mut(n as usize),
            100..=126 => {
                let at = self.chest_view?;
                self.chests.get_mut(&at)?.slots.get_mut((n - CHEST) as usize)
            }
            200..=203 => self.inv.worn.get_mut((n - WORN) as usize),
            _ => None,
        }
    }
}

/// How many pack slots there are (for the docs and checks).
pub const PACK: usize = SLOTS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_damage_attribute_scales_a_blow() {
        assert_eq!(scaled_blow(10, 1.0), 10);
        assert_eq!(scaled_blow(10, 1.5), 15);
        assert_eq!(scaled_blow(10, 0.0), 0);
        assert_eq!(scaled_blow(0, 3.0), 0);
        assert_eq!(scaled_blow(10, f32::NAN), 10);
        assert_eq!(scaled_blow(1_000_000, 10.0), 100_000);
    }
}
