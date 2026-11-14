//! Armour: four worn slots (head, chest, legs, feet) beside the pack, the
//! `armor <slot> <defense> [toughness]` item property, putting a piece on
//! and taking it off, and what worn armour does to a blow.
//!
//! The rules are plain functions of numbers and of a lookup from an item to
//! its armour data (`Info`), so tests can use made-up items; `piece` is the
//! lookup of the game's own data.

use crate::block::reg;
use crate::inventory::{Inventory, Slot, Stack};
use alloc::vec::Vec;

/// Head, chest, legs, feet.
pub const SLOTS: usize = 4;
pub const NAMES: [&str; SLOTS] = ["head", "chest", "legs", "feet"];
/// Most defense one piece may give, and the most toughness.
pub const MAX_DEFENSE: u8 = 20;
pub const MAX_TOUGHNESS: f32 = 20.0;
/// Each point of defense takes this much off a blow, and no armour takes more than the cap.
pub const PER_POINT: f32 = 0.04;
pub const CAP: f32 = 0.80;

/// Which worn slot a word names.
pub fn slot_named(word: &str) -> Option<usize> {
    NAMES.iter().position(|n| *n == word)
}

/// What an armour item gives.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Piece {
    pub slot: u8,
    pub defense: u8,
    pub toughness: f32,
}

/// Where armour data comes from (an item number to its piece, if it is armour).
pub type Info<'a> = &'a dyn Fn(u16) -> Option<Piece>;

/// The game's data for an item.
pub fn piece(item: u16) -> Option<Piece> {
    reg().items.get(item as usize).and_then(|i| i.x.armor).map(|(slot, defense, toughness)| Piece { slot, defense, toughness })
}

/// Total defense points and toughness of what is worn.
pub fn totals(worn: &[Slot; SLOTS], info: Info) -> (f32, f32) {
    let (mut d, mut t) = (0.0f32, 0.0f32);
    for s in worn.iter().flatten() {
        if let Some(p) = info(s.item) {
            d += p.defense as f32;
            t += p.toughness;
        }
    }
    (d, t)
}

/// The part of a blow armour takes off, 0.0 ..= 0.8. Each point of defense
/// takes 4 percent; toughness adds up to 2 percent a point against the big
/// blows (10 damage or more, in proportion below that).
pub fn reduction(defense: f32, toughness: f32, damage: i32) -> f32 {
    if !(defense > 0.0) || damage <= 0 {
        return 0.0;
    }
    let big = (damage as f32 / 10.0).clamp(0.0, 1.0);
    let t = if toughness > 0.0 { toughness } else { 0.0 };
    (defense * PER_POINT + t * 0.02 * big).clamp(0.0, CAP)
}

/// A blow after armour: never below 1 for a blow that was 1 or more.
pub fn reduced(damage: i32, defense: f32, toughness: f32) -> i32 {
    if damage <= 0 {
        return damage;
    }
    let cut = damage as f32 * reduction(defense, toughness, damage);
    let taken = damage - crate::math::floorf(cut + 0.5) as i32;
    taken.max(1)
}

/// Whether a blow of this cause is one armour helps against: a creature's
/// blow, an arrow or other shot, an explosion. Falls, drowning, hunger,
/// touching blocks, effects and a script's `hurt` go through it.
pub fn blocks_cause(cause: &str, by_creature: bool) -> bool {
    by_creature || matches!(cause, "mob" | "arrow" | "shot" | "explosion" | "melee" | "projectile")
}

/// What putting a piece on did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Put {
    /// The piece is worn now (what it replaced went to the pack).
    Worn,
    /// Not armour, or no such slot.
    NotArmor,
    /// What it would replace has nowhere to go.
    NoRoom,
}

/// Wear the armour in pack slot `from` (one of the stack). A piece already
/// worn there goes back to the pack, into the slot just freed when there is
/// one, so a swap works in a full pack.
pub fn equip(inv: &mut Inventory, from: usize, info: Info) -> Put {
    let Some(Some(stack)) = inv.slots.get(from).copied() else { return Put::NotArmor };
    let Some(p) = info(stack.item) else { return Put::NotArmor };
    let slot = p.slot as usize;
    if slot >= SLOTS || stack.count == 0 {
        return Put::NotArmor;
    }
    let old = inv.worn[slot];
    let one = stack.with_count(1);
    if let Some(old) = old {
        // The pack minus one of this stack must have room for the old piece.
        let mut probe = inv.clone();
        probe.take_one_stack(from);
        if !probe.has_room_for(&old) {
            return Put::NoRoom;
        }
        inv.take_one_stack(from);
        inv.add_stack(old);
    } else {
        inv.take_one_stack(from);
    }
    inv.worn[slot] = Some(one);
    Put::Worn
}

/// Take off the piece in worn slot `slot`: it goes to the pack. Whatever did
/// not fit is returned for the caller to set down in the world. `None` when
/// nothing is worn there.
pub fn take_off(inv: &mut Inventory, slot: usize) -> Option<Option<Stack>> {
    let s = inv.worn.get_mut(slot)?.take()?;
    let left = inv.add_stack(s);
    Some(if left > 0 { Some(s.with_count(left)) } else { None })
}

/// The slots of worn pieces that have durability, wear each by `amount` and
/// return the slots whose piece broke.
pub fn wear_worn(worn: &mut [Slot; SLOTS], amount: u32, durability: &dyn Fn(u16) -> u16) -> Vec<usize> {
    let mut broke = Vec::new();
    for (i, w) in worn.iter_mut().enumerate() {
        let max = w.as_ref().map_or(0, |s| durability(s.item));
        if super::wear::apply(w, amount, max) == super::wear::Outcome::Broke {
            broke.push(i);
        }
    }
    broke
}

#[cfg(test)]
mod tests {
    use super::*;

    // Made-up items: 1 helmet (head), 2 chestplate, 3 another helmet, 9 not armour.
    fn info(item: u16) -> Option<Piece> {
        match item {
            1 => Some(Piece { slot: 0, defense: 2, toughness: 0.0 }),
            2 => Some(Piece { slot: 1, defense: 6, toughness: 2.0 }),
            3 => Some(Piece { slot: 0, defense: 3, toughness: 1.0 }),
            _ => None,
        }
    }

    #[test]
    fn each_point_takes_four_percent_and_the_cap_is_eighty() {
        assert_eq!(reduction(0.0, 0.0, 10), 0.0);
        assert!((reduction(5.0, 0.0, 10) - 0.20).abs() < 1e-6);
        assert_eq!(reduction(40.0, 0.0, 10), CAP);
        assert_eq!(reduction(20.0, 20.0, 100), CAP);
        assert_eq!(reduction(5.0, 0.0, 0), 0.0);
        // Toughness helps more against big blows.
        assert!(reduction(5.0, 10.0, 20) > reduction(5.0, 10.0, 2));
        assert!(reduction(f32::NAN, 0.0, 10) == 0.0);
    }

    #[test]
    fn a_blow_is_cut_but_never_to_nothing() {
        assert_eq!(reduced(10, 5.0, 0.0), 8);
        assert_eq!(reduced(20, 10.0, 0.0), 12);
        assert_eq!(reduced(1, 20.0, 20.0), 1);
        assert_eq!(reduced(3, 20.0, 0.0), 1);
        assert_eq!(reduced(0, 20.0, 0.0), 0);
        assert_eq!(reduced(7, 0.0, 0.0), 7);
    }

    #[test]
    fn only_creature_arrow_and_blast_blows_are_cut() {
        assert!(blocks_cause("anything", true));
        assert!(blocks_cause("arrow", false));
        assert!(blocks_cause("explosion", false));
        assert!(!blocks_cause("fall", false));
        assert!(!blocks_cause("drown", false));
        assert!(!blocks_cause("other", false));
    }

    #[test]
    fn totals_add_up_what_is_worn() {
        let mut inv = Inventory::new();
        inv.worn[0] = Some(Stack::new(1, 1));
        inv.worn[1] = Some(Stack::new(2, 1));
        assert_eq!(totals(&inv.worn, &info), (8.0, 2.0));
        assert_eq!(totals(&Inventory::new().worn, &info), (0.0, 0.0));
    }

    #[test]
    fn putting_a_piece_on_takes_one_from_the_stack() {
        let mut inv = Inventory::new();
        inv.slots[4] = Some(Stack::new(1, 2));
        assert_eq!(equip(&mut inv, 4, &info), Put::Worn);
        assert_eq!(inv.worn[0], Some(Stack::new(1, 1)));
        assert_eq!(inv.slots[4], Some(Stack::new(1, 1)));
        // Not armour, an empty slot and a slot that does not exist.
        inv.slots[5] = Some(Stack::new(9, 1));
        assert_eq!(equip(&mut inv, 5, &info), Put::NotArmor);
        assert_eq!(equip(&mut inv, 6, &info), Put::NotArmor);
        assert_eq!(equip(&mut inv, 99, &info), Put::NotArmor);
    }

    #[test]
    fn swapping_works_in_a_full_pack_and_refuses_when_there_is_no_room() {
        let mut inv = Inventory::new();
        inv.worn[0] = Some(Stack::new(1, 1));
        inv.slots[0] = Some(Stack::new(3, 1));
        for i in 1..crate::inventory::SLOTS {
            inv.slots[i] = Some(Stack::new(9, 1));
        }
        // The pack is full, but the helmet being put on frees its own slot.
        assert_eq!(equip(&mut inv, 0, &info), Put::Worn);
        assert_eq!(inv.worn[0], Some(Stack::new(3, 1)));
        assert_eq!(inv.slots[0], Some(Stack::new(1, 1)));
        // Two of the new one in a full pack: the old one has nowhere to go.
        let mut inv = Inventory::new();
        inv.worn[0] = Some(Stack::new(1, 1));
        inv.slots[0] = Some(Stack::new(3, 2));
        for i in 1..crate::inventory::SLOTS {
            inv.slots[i] = Some(Stack::new(9, 1));
        }
        let before = inv.clone();
        assert_eq!(equip(&mut inv, 0, &info), Put::NoRoom);
        assert_eq!(inv, before);
    }

    #[test]
    fn taking_off_goes_to_the_pack_or_comes_back_as_the_rest() {
        let mut inv = Inventory::new();
        inv.worn[1] = Some(Stack::new(2, 1));
        assert_eq!(take_off(&mut inv, 1), Some(None));
        assert_eq!(inv.worn[1], None);
        assert_eq!(inv.count(2), 1);
        assert_eq!(take_off(&mut inv, 1), None);
        assert_eq!(take_off(&mut inv, 9), None);
        // A full pack gives the piece back to be dropped.
        let mut inv = Inventory::new();
        inv.worn[1] = Some(Stack { wear: 4, ..Stack::new(2, 1) });
        for i in 0..crate::inventory::SLOTS {
            inv.slots[i] = Some(Stack::new(9, 1));
        }
        let left = take_off(&mut inv, 1).unwrap().unwrap();
        assert_eq!((left.item, left.count, left.wear), (2, 1, 4));
    }

    #[test]
    fn worn_pieces_wear_and_break() {
        let mut worn: [Slot; SLOTS] = [None; SLOTS];
        worn[0] = Some(Stack::new(1, 1));
        worn[1] = Some(Stack { wear: 1, ..Stack::new(2, 1) });
        worn[2] = Some(Stack::new(9, 1));
        let dur = |i: u16| match i {
            1 => 5,
            2 => 2,
            _ => 0,
        };
        assert_eq!(wear_worn(&mut worn, 1, &dur), alloc::vec![1]);
        assert_eq!(worn[0].unwrap().wear, 1);
        assert_eq!(worn[1], None);
        assert_eq!(worn[2], Some(Stack::new(9, 1)));
    }
}
