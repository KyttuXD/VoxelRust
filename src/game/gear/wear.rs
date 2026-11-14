//! Durability: tools, weapons, bows and armour that wear out. An item with
//! `durability <uses>` in the data has `Stack::wear` uses spent; at the last
//! use the item breaks and its slot empties. Creative mode never wears
//! anything (the callers check that). Two damaged stacks of one item are
//! repaired together at the crafting grid (`crafting/repair.rs`).

use crate::block::reg;
use crate::inventory::{Slot, Stack};

/// What wearing an item did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// The item has no durability, or it still holds.
    Fine,
    /// The item broke and its slot is empty now.
    Broke,
}

/// The uses an item has when new (0: it does not wear).
pub fn durability_of(item: u16) -> u16 {
    reg().items.get(item as usize).map_or(0, |i| i.x.durability)
}

/// Wear the stack in `slot` by `amount` uses against a durability of `max`
/// (0 = never wears). Items that wear are stacks of one (the data forces
/// it), so the whole stack wears; a stack of several that somehow wears
/// breaks all at once.
pub fn apply(slot: &mut Slot, amount: u32, max: u16) -> Outcome {
    let Some(s) = slot.as_mut() else { return Outcome::Fine };
    if max == 0 || amount == 0 || s.count == 0 {
        return Outcome::Fine;
    }
    let spent = (s.wear as u32).saturating_add(amount);
    if spent >= max as u32 {
        *slot = None;
        return Outcome::Broke;
    }
    s.wear = spent as u16;
    Outcome::Fine
}

/// As [`apply`], looking the durability up from the item.
pub fn wear_slot(slot: &mut Slot, amount: u32) -> Outcome {
    let max = slot.as_ref().map_or(0, |s| durability_of(s.item));
    apply(slot, amount, max)
}

/// Uses left of a stack with durability `max`.
pub fn left(s: &Stack, max: u16) -> u16 {
    max.saturating_sub(s.wear)
}

/// How much is left, 1.0 (new) down to 0.0.
pub fn fraction_left(s: &Stack, max: u16) -> f32 {
    if max == 0 {
        return 1.0;
    }
    (left(s, max) as f32 / max as f32).clamp(0.0, 1.0)
}

/// The colour of the wear bar for what is left: green when new, through
/// yellow to red.
pub fn bar_colour(frac: f32) -> [u8; 3] {
    let f = frac.clamp(0.0, 1.0);
    if f >= 0.5 {
        // Yellow (f = 0.5) to green (f = 1).
        let t = (f - 0.5) * 2.0;
        [(230.0 - 170.0 * t) as u8, (200.0 + 20.0 * t) as u8, (40.0 + 20.0 * t) as u8]
    } else {
        // Red (f = 0) to yellow (f = 0.5).
        let t = f * 2.0;
        [(220.0 + 10.0 * t) as u8, (50.0 + 150.0 * t) as u8, 40]
    }
}

/// The name of an item for a tooltip, with the uses left once it has been used.
pub fn tooltip(name: &str, s: &Stack) -> alloc::string::String {
    if shows_bar(s) {
        alloc::format!("{} ({} USES LEFT)", name, left(s, durability_of(s.item)))
    } else {
        alloc::string::String::from(name)
    }
}

/// Whether a wear bar is shown for the stack: it wears and has been used.
pub fn shows_bar(s: &Stack) -> bool {
    s.wear > 0 && durability_of(s.item) > 0
}

/// Mend a stack by `amount` uses (never past new). Whether it changed.
pub fn repair(slot: &mut Slot, amount: u32) -> bool {
    match slot.as_mut() {
        Some(s) if s.wear > 0 => {
            let was = s.wear;
            s.wear = (s.wear as u32).saturating_sub(amount) as u16;
            s.wear != was
        }
        _ => false,
    }
}

/// What two stacks of one worn item make when crafted together: the uses
/// left of both added, plus 5 percent of the full durability, up to new.
/// Both must be one item that wears, one of each. The result keeps the
/// first stack's item data.
pub fn combine(a: &Stack, b: &Stack, max: u16) -> Option<Stack> {
    if max == 0 || a.item != b.item || a.count != 1 || b.count != 1 {
        return None;
    }
    let sum = left(a, max) as u32 + left(b, max) as u32 + max as u32 * 5 / 100;
    let left = sum.min(max as u32);
    Some(Stack { wear: (max as u32 - left) as u16, ..*a })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_wears_and_breaks_at_zero() {
        let mut slot = Some(Stack::new(5, 1));
        assert_eq!(apply(&mut slot, 1, 3), Outcome::Fine);
        assert_eq!(slot.unwrap().wear, 1);
        assert_eq!(apply(&mut slot, 1, 3), Outcome::Fine);
        assert_eq!(left(&slot.unwrap(), 3), 1);
        assert_eq!(apply(&mut slot, 1, 3), Outcome::Broke);
        assert_eq!(slot, None);
    }

    #[test]
    fn an_item_that_does_not_wear_never_does() {
        let mut slot = Some(Stack::new(5, 64));
        assert_eq!(apply(&mut slot, 1000, 0), Outcome::Fine);
        assert_eq!(slot, Some(Stack::new(5, 64)));
        let mut empty: Slot = None;
        assert_eq!(apply(&mut empty, 1, 10), Outcome::Fine);
    }

    #[test]
    fn a_big_wear_breaks_at_once_and_does_not_overflow() {
        let mut slot = Some(Stack { wear: 10, ..Stack::new(5, 1) });
        assert_eq!(apply(&mut slot, u32::MAX, 100), Outcome::Broke);
    }

    #[test]
    fn the_bar_goes_from_green_to_red() {
        let green = bar_colour(1.0);
        let red = bar_colour(0.0);
        assert!(green[1] > green[0]);
        assert!(red[0] > red[1]);
        let mid = bar_colour(0.5);
        assert!(mid[0] > 200 && mid[1] > 190);
        assert_eq!(bar_colour(7.0), bar_colour(1.0));
        assert_eq!(bar_colour(-3.0), bar_colour(0.0));
        assert_eq!(fraction_left(&Stack { wear: 25, ..Stack::new(1, 1) }, 100), 0.75);
        assert_eq!(fraction_left(&Stack::new(1, 1), 0), 1.0);
    }

    #[test]
    fn repair_never_goes_past_new() {
        let mut slot = Some(Stack { wear: 10, ..Stack::new(5, 1) });
        assert!(repair(&mut slot, 4));
        assert_eq!(slot.unwrap().wear, 6);
        assert!(repair(&mut slot, 100));
        assert_eq!(slot.unwrap().wear, 0);
        assert!(!repair(&mut slot, 1));
    }

    #[test]
    fn two_worn_items_combine_to_the_sum_and_five_percent() {
        // Durability 100: 40 left + 30 left + 5 = 75 left.
        let a = Stack { wear: 60, ..Stack::new(7, 1) };
        let b = Stack { wear: 70, ..Stack::new(7, 1) };
        assert_eq!(combine(&a, &b, 100).unwrap().wear, 25);
        // Two nearly new ones make a new one, not better than new.
        let c = Stack { wear: 3, ..Stack::new(7, 1) };
        assert_eq!(combine(&c, &c, 100).unwrap().wear, 0);
        // Different items, stacks of several and items that do not wear do not combine.
        assert_eq!(combine(&a, &Stack::new(8, 1), 100), None);
        assert_eq!(combine(&a, &Stack::new(7, 2), 100), None);
        assert_eq!(combine(&a, &b, 0), None);
    }
}
