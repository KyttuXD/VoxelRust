//! The player's inventory: 36 slots, each either empty or a stack of one
//! kind of item, and four worn armour slots. Slots 0..9 are the hotbar.

use crate::block::reg;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stack {
    /// Index into the registry's items.
    pub item: u16,
    pub count: u16,
    /// Uses spent of the item's durability (0 = as new; see `game/gear/wear.rs`).
    pub wear: u16,
    /// Handle of the stack's item data (0 = none; see `game/gear/item_data.rs`).
    pub data: u16,
}

impl Stack {
    /// A fresh stack: unworn and with no data.
    pub const fn new(item: u16, count: u16) -> Stack {
        Stack { item, count, wear: 0, data: 0 }
    }

    /// The same stack with another count (wear and data are kept).
    pub fn with_count(self, count: u16) -> Stack {
        Stack { count, ..self }
    }

    /// Whether two stacks may merge: the same item, wear and data.
    pub fn same_kind(&self, other: &Stack) -> bool {
        self.item == other.item && self.wear == other.wear && self.data == other.data
    }
}

pub type Slot = Option<Stack>;

pub const SLOTS: usize = 36;
pub const HOTBAR: usize = 9;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Inventory {
    pub slots: [Slot; SLOTS],
    /// Armour being worn: head, chest, legs, feet (see `game/gear/armor.rs`).
    pub worn: [Slot; crate::game::gear::armor::SLOTS],
}

/// How many of an item fit in one slot.
pub fn stack_limit(item: u16) -> u16 {
    reg().items.get(item as usize).map_or(64, |i| i.stack.max(1))
}

/// A mouse click on one slot (`here`) while `cursor` is what the mouse
/// carries; see [`Inventory::click`]. Also used for the crafting grid.
pub fn click_slot(here: &mut Slot, cursor: &mut Slot, right: bool) {
    match (*cursor, *here, right) {
        (None, None, _) => {}
        (None, Some(s), false) => {
            *cursor = Some(s);
            *here = None;
        }
        (None, Some(s), true) => {
            // (Not `(count + 1) / 2`: that overflows a u16 at 65535.)
            let take = s.count / 2 + s.count % 2;
            *cursor = Some(s.with_count(take));
            *here = if s.count > take { Some(s.with_count(s.count - take)) } else { None };
        }
        (Some(c), None, false) => {
            *here = Some(c);
            *cursor = None;
        }
        (Some(c), None, true) => {
            *here = Some(c.with_count(1));
            *cursor = if c.count > 1 { Some(c.with_count(c.count - 1)) } else { None };
        }
        (Some(c), Some(s), _) if c.same_kind(&s) => {
            let limit = stack_limit(s.item);
            let n = if right { 1 } else { c.count }.min(limit.saturating_sub(s.count));
            *here = Some(s.with_count(s.count + n));
            *cursor = if c.count > n { Some(c.with_count(c.count - n)) } else { None };
        }
        (Some(c), Some(s), _) => {
            *here = Some(c);
            *cursor = Some(s);
        }
    }
}

impl Default for Inventory {
    fn default() -> Self {
        Inventory::new()
    }
}

impl Inventory {
    pub fn new() -> Inventory {
        Inventory { slots: [None; SLOTS], worn: [None; crate::game::gear::armor::SLOTS] }
    }

    /// Add items, topping up existing stacks first and then filling empty
    /// slots from the hotbar onward. Returns how many did not fit.
    pub fn add(&mut self, item: u16, count: u16) -> u16 {
        self.add_stack(Stack::new(item, count))
    }

    /// As [`Inventory::add`] for a stack that may be worn or carry data: it
    /// tops up only stacks of the same kind. Returns how many did not fit.
    pub fn add_stack(&mut self, stack: Stack) -> u16 {
        let mut count = stack.count;
        // Adding nothing must not leave an empty stack in a slot.
        if count == 0 {
            return 0;
        }
        let limit = stack_limit(stack.item);
        for s in self.slots.iter_mut().flatten() {
            if s.same_kind(&stack) && s.count < limit {
                let n = (limit - s.count).min(count);
                s.count += n;
                count -= n;
                if count == 0 {
                    return 0;
                }
            }
        }
        for slot in self.slots.iter_mut() {
            if slot.is_none() {
                let n = limit.min(count);
                *slot = Some(stack.with_count(n));
                count -= n;
                if count == 0 {
                    return 0;
                }
            }
        }
        count
    }

    /// Whether [`Inventory::add_stack`] would take all of the stack.
    pub fn has_room_for(&self, stack: &Stack) -> bool {
        let limit = stack_limit(stack.item) as u32;
        let mut room = 0u32;
        for s in self.slots.iter() {
            match s {
                None => room += limit,
                Some(s) if s.same_kind(stack) => room += limit.saturating_sub(s.count as u32),
                _ => {}
            }
        }
        room >= stack.count as u32
    }

    pub fn count(&self, item: u16) -> u32 {
        self.slots.iter().flatten().filter(|s| s.item == item).map(|s| s.count as u32).sum()
    }

    /// Remove `count` of an item if that many are held. Returns whether it did.
    pub fn remove(&mut self, item: u16, mut count: u16) -> bool {
        if self.count(item) < count as u32 {
            return false;
        }
        for slot in self.slots.iter_mut().rev() {
            if let Some(s) = slot {
                if s.item == item {
                    let n = s.count.min(count);
                    s.count -= n;
                    count -= n;
                    if s.count == 0 {
                        *slot = None;
                    }
                    if count == 0 {
                        break;
                    }
                }
            }
        }
        true
    }

    /// Use up one item from a slot; the slot empties when it runs out.
    pub fn take_one(&mut self, slot: usize) -> Option<u16> {
        self.take_one_stack(slot).map(|s| s.item)
    }

    /// As [`Inventory::take_one`], giving the one taken (with its wear and data).
    pub fn take_one_stack(&mut self, slot: usize) -> Option<Stack> {
        let s = self.slots.get_mut(slot)?.as_mut()?;
        let taken = s.with_count(1);
        s.count = s.count.saturating_sub(1);
        if s.count == 0 {
            self.slots[slot] = None;
        }
        Some(taken)
    }

    /// A mouse click on a slot while `cursor` is what the mouse carries.
    /// Left: pick up, put down, merge or swap. Right: pick up half, or put
    /// down one.
    pub fn click(&mut self, slot: usize, cursor: &mut Slot, right: bool) {
        if let Some(here) = self.slots.get_mut(slot) {
            click_slot(here, cursor, right);
        }
    }

    /// Text form for the save file: one word per slot (see
    /// `game/gear/stack_text.rs`), `-` for empty; the four worn pieces follow
    /// the 36 slots when any is worn.
    pub fn save(&self) -> alloc::string::String {
        let mut parts: alloc::vec::Vec<alloc::string::String> = self.slots.iter().map(crate::game::gear::stack_text::write).collect();
        if self.worn.iter().any(|w| w.is_some()) {
            parts.extend(self.worn.iter().map(crate::game::gear::stack_text::write));
        }
        parts.join(" ")
    }

    /// Inverse of [`Inventory::save`]. Items that no longer exist are dropped.
    /// A save of the older kind (36 words, plain `item:count`) loads as is.
    pub fn load(text: &str) -> Inventory {
        Inventory::load_with(text, &crate::game::gear::armor::piece)
    }

    /// As [`Inventory::load`], with `info` telling which items are armour.
    pub fn load_with(text: &str, info: crate::game::gear::armor::Info) -> Inventory {
        let mut inv = Inventory::new();
        for (i, part) in text.split(' ').take(SLOTS + crate::game::gear::armor::SLOTS).enumerate() {
            let slot = crate::game::gear::stack_text::read(part);
            if i < SLOTS {
                inv.slots[i] = slot;
            } else {
                // A worn piece must be armour for the slot it sits in.
                inv.worn[i - SLOTS] = slot.filter(|s| info(s.item).map_or(false, |p| p.slot as usize == i - SLOTS));
            }
        }
        inv
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> u16 {
        reg().items.iter().position(|i| i.id == id).unwrap() as u16
    }

    #[test]
    fn starts_empty_and_stacks_to_the_limit() {
        let mut inv = Inventory::new();
        assert!(inv.slots.iter().all(|s| s.is_none()));
        let stone = item("stone");
        assert_eq!(inv.add(stone, 100), 0);
        assert_eq!(inv.slots[0], Some(Stack::new(stone, 64)));
        assert_eq!(inv.slots[1], Some(Stack::new(stone, 36)));
        assert_eq!(inv.slots[2], None);
        assert_eq!(inv.count(stone), 100);
        // The wand does not stack.
        let wand = item("iron_sword");
        assert_eq!(inv.add(wand, 2), 0);
        assert_eq!(inv.slots[2], Some(Stack::new(wand, 1)));
        assert_eq!(inv.slots[3], Some(Stack::new(wand, 1)));
    }

    #[test]
    fn a_full_inventory_reports_what_did_not_fit() {
        let mut inv = Inventory::new();
        let dirt = item("dirt");
        assert_eq!(inv.add(dirt, 64 * 36), 0);
        assert_eq!(inv.add(dirt, 5), 5);
        assert_eq!(inv.add(item("stone"), 1), 1);
    }

    #[test]
    fn using_the_last_item_leaves_an_empty_slot() {
        let mut inv = Inventory::new();
        let s = item("sand");
        inv.add(s, 2);
        assert_eq!(inv.take_one(0), Some(s));
        assert_eq!(inv.take_one(0), Some(s));
        assert_eq!(inv.slots[0], None);
        assert_eq!(inv.take_one(0), None);
        assert_eq!(inv.take_one(99), None);
    }

    #[test]
    fn clicking_moves_splits_merges_and_swaps() {
        let mut inv = Inventory::new();
        let (a, b) = (item("stone"), item("dirt"));
        inv.add(a, 10);
        let mut cur = None;
        inv.click(0, &mut cur, true); // take half
        assert_eq!((inv.slots[0].unwrap().count, cur.unwrap().count), (5, 5));
        inv.click(5, &mut cur, true); // put one down in an empty slot
        assert_eq!((inv.slots[5].unwrap().count, cur.unwrap().count), (1, 4));
        inv.click(5, &mut cur, false); // merge the rest
        assert_eq!((inv.slots[5].unwrap().count, cur), (5, None));
        inv.slots[6] = Some(Stack::new(b, 3));
        inv.click(5, &mut cur, false);
        inv.click(6, &mut cur, false); // swap with a different item
        assert_eq!(inv.slots[6], Some(Stack::new(a, 5)));
        assert_eq!(cur, Some(Stack::new(b, 3)));
        inv.click(5, &mut cur, false);
        assert_eq!((inv.slots[5], cur), (Some(Stack::new(b, 3)), None));
        inv.click(20, &mut cur, false); // empty hand on an empty slot: nothing
        assert_eq!(inv.slots[20], None);
    }

    #[test]
    fn remove_and_save_round_trip() {
        let mut inv = Inventory::new();
        let (a, b) = (item("stone"), item("planks"));
        inv.add(a, 70);
        inv.slots[30] = Some(Stack::new(b, 7));
        assert!(!inv.remove(a, 71));
        assert!(inv.remove(a, 66));
        assert_eq!(inv.count(a), 4);
        let text = inv.save();
        assert_eq!(Inventory::load(&text), inv);
        assert_eq!(Inventory::load("no_such_item:5 - stone:0"), Inventory::new());
    }

    #[test]
    fn adding_nothing_makes_no_empty_stack() {
        let mut inv = Inventory::new();
        assert_eq!(inv.add(item("stone"), 0), 0);
        assert!(inv.slots.iter().all(|s| s.is_none()));
    }

    #[test]
    fn using_one_of_an_empty_stack_does_not_underflow() {
        let mut inv = Inventory::new();
        inv.slots[0] = Some(Stack::new(item("sand"), 0));
        assert_eq!(inv.take_one(0), Some(item("sand")));
        assert_eq!(inv.slots[0], None);
    }

    #[test]
    fn halving_a_huge_stack_does_not_overflow() {
        let mut here = Some(Stack::new(item("stone"), u16::MAX));
        let mut cursor = None;
        click_slot(&mut here, &mut cursor, true);
        assert_eq!(cursor.unwrap().count, 32768);
        assert_eq!(here.unwrap().count, 32767);
    }

    #[test]
    fn a_big_saved_count_is_clamped_not_dropped() {
        let inv = Inventory::load("stone:100000 dirt:70");
        assert_eq!(inv.slots[0], Some(Stack::new(item("stone"), stack_limit(item("stone")))));
        assert_eq!(inv.slots[1].unwrap().count, 64);
    }

    #[test]
    fn saving_skips_items_that_are_gone() {
        let mut inv = Inventory::new();
        inv.slots[0] = Some(Stack::new(u16::MAX, 3));
        assert!(inv.save().starts_with("- "));
    }

    #[test]
    fn stacks_with_different_wear_or_data_do_not_merge() {
        let sword = item("iron_sword");
        let mut inv = Inventory::new();
        inv.slots[0] = Some(Stack { wear: 5, ..Stack::new(item("stone"), 3) });
        // A plain add does not top up a stack that is worn or has data.
        assert_eq!(inv.add(item("stone"), 2), 0);
        assert_eq!(inv.slots[0].unwrap().count, 3);
        assert_eq!(inv.slots[1], Some(Stack::new(item("stone"), 2)));
        let tagged = Stack { data: crate::game::gear::item_data::with_key(0, "inv_t", "1").unwrap(), ..Stack::new(item("stone"), 1) };
        let mut here = Some(Stack::new(item("stone"), 4));
        let mut cursor = Some(tagged);
        click_slot(&mut here, &mut cursor, false);
        // They swapped rather than merged.
        assert_eq!(here, Some(tagged));
        assert_eq!(cursor, Some(Stack::new(item("stone"), 4)));
        // The same data does merge.
        let mut a = Some(tagged);
        let mut b = Some(tagged.with_count(2));
        click_slot(&mut a, &mut b, false);
        assert_eq!(a.unwrap().count, 3);
        assert_eq!(b, None);
        let _ = sword;
    }

    #[test]
    fn wear_data_and_worn_pieces_survive_a_save_and_old_saves_load() {
        use crate::game::gear::armor::Piece;
        let helmet = item("iron_sword");
        let info = |i: u16| if i == helmet { Some(Piece { slot: 0, defense: 2, toughness: 0.0 }) } else { None };
        let mut inv = Inventory::new();
        inv.slots[3] = Some(Stack { wear: 7, data: crate::game::gear::item_data::with_key(0, "inv_owner", "me you").unwrap(), ..Stack::new(helmet, 1) });
        inv.worn[0] = Some(Stack { wear: 2, ..Stack::new(helmet, 1) });
        let text = inv.save();
        assert_eq!(Inventory::load_with(&text, &info), inv);
        // The same piece in the wrong worn slot is not worn.
        let wrong = text.replace("iron_sword:1@2", "-");
        assert_eq!(Inventory::load_with(&wrong, &info).worn[0], None);
        // An old save has 36 plain words; no worn pieces make no extra words.
        let old = "stone:5 - dirt:2";
        let loaded = Inventory::load(old);
        assert_eq!(loaded.slots[0], Some(Stack::new(item("stone"), 5)));
        assert_eq!(loaded.worn, [None; 4]);
        assert_eq!(Inventory::new().save().split(' ').count(), SLOTS);
    }
}
