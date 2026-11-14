//! The player's inventory: 36 slots, each either empty or a stack of one
//! kind of item. Slots 0..9 are the hotbar.

use crate::block::reg;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stack {
    /// Index into the registry's items.
    pub item: u16,
    pub count: u16,
}

pub type Slot = Option<Stack>;

pub const SLOTS: usize = 36;
pub const HOTBAR: usize = 9;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Inventory {
    pub slots: [Slot; SLOTS],
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
            let take = (s.count + 1) / 2;
            *cursor = Some(Stack { item: s.item, count: take });
            *here = if s.count > take { Some(Stack { item: s.item, count: s.count - take }) } else { None };
        }
        (Some(c), None, false) => {
            *here = Some(c);
            *cursor = None;
        }
        (Some(c), None, true) => {
            *here = Some(Stack { item: c.item, count: 1 });
            *cursor = if c.count > 1 { Some(Stack { item: c.item, count: c.count - 1 }) } else { None };
        }
        (Some(c), Some(s), _) if c.item == s.item => {
            let limit = stack_limit(s.item);
            let n = if right { 1 } else { c.count }.min(limit.saturating_sub(s.count));
            *here = Some(Stack { item: s.item, count: s.count + n });
            *cursor = if c.count > n { Some(Stack { item: c.item, count: c.count - n }) } else { None };
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
        Inventory { slots: [None; SLOTS] }
    }

    /// Add items, topping up existing stacks first and then filling empty
    /// slots from the hotbar onward. Returns how many did not fit.
    pub fn add(&mut self, item: u16, mut count: u16) -> u16 {
        let limit = stack_limit(item);
        for s in self.slots.iter_mut().flatten() {
            if s.item == item && s.count < limit {
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
                *slot = Some(Stack { item, count: n });
                count -= n;
                if count == 0 {
                    return 0;
                }
            }
        }
        count
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
        let s = self.slots.get_mut(slot)?.as_mut()?;
        let item = s.item;
        s.count -= 1;
        if s.count == 0 {
            self.slots[slot] = None;
        }
        Some(item)
    }

    /// A mouse click on a slot while `cursor` is what the mouse carries.
    /// Left: pick up, put down, merge or swap. Right: pick up half, or put
    /// down one.
    pub fn click(&mut self, slot: usize, cursor: &mut Slot, right: bool) {
        if let Some(here) = self.slots.get_mut(slot) {
            click_slot(here, cursor, right);
        }
    }

    /// Text form for the save file: `item_id:count` per slot, `-` for empty.
    pub fn save(&self) -> alloc::string::String {
        let items = &reg().items;
        let parts: alloc::vec::Vec<alloc::string::String> = self
            .slots
            .iter()
            .map(|s| match s {
                Some(s) => alloc::format!("{}:{}", items[s.item as usize].id, s.count),
                None => alloc::string::String::from("-"),
            })
            .collect();
        parts.join(" ")
    }

    /// Inverse of [`Inventory::save`]. Items that no longer exist are dropped.
    pub fn load(text: &str) -> Inventory {
        let items = &reg().items;
        let mut inv = Inventory::new();
        for (i, part) in text.split(' ').take(SLOTS).enumerate() {
            let mut p = part.splitn(2, ':');
            let (id, n) = (p.next().unwrap_or(""), p.next().and_then(|n| n.parse::<u16>().ok()).unwrap_or(0));
            if let (Some(item), true) = (items.iter().position(|it| it.id == id), n > 0) {
                inv.slots[i] = Some(Stack { item: item as u16, count: n.min(items[item].stack.max(1)) });
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
        assert_eq!(inv.slots[0], Some(Stack { item: stone, count: 64 }));
        assert_eq!(inv.slots[1], Some(Stack { item: stone, count: 36 }));
        assert_eq!(inv.slots[2], None);
        assert_eq!(inv.count(stone), 100);
        // The wand does not stack.
        let wand = item("iron_sword");
        assert_eq!(inv.add(wand, 2), 0);
        assert_eq!(inv.slots[2], Some(Stack { item: wand, count: 1 }));
        assert_eq!(inv.slots[3], Some(Stack { item: wand, count: 1 }));
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
        inv.slots[6] = Some(Stack { item: b, count: 3 });
        inv.click(5, &mut cur, false);
        inv.click(6, &mut cur, false); // swap with a different item
        assert_eq!(inv.slots[6], Some(Stack { item: a, count: 5 }));
        assert_eq!(cur, Some(Stack { item: b, count: 3 }));
        inv.click(5, &mut cur, false);
        assert_eq!((inv.slots[5], cur), (Some(Stack { item: b, count: 3 }), None));
        inv.click(20, &mut cur, false); // empty hand on an empty slot: nothing
        assert_eq!(inv.slots[20], None);
    }

    #[test]
    fn remove_and_save_round_trip() {
        let mut inv = Inventory::new();
        let (a, b) = (item("stone"), item("planks"));
        inv.add(a, 70);
        inv.slots[30] = Some(Stack { item: b, count: 7 });
        assert!(!inv.remove(a, 71));
        assert!(inv.remove(a, 66));
        assert_eq!(inv.count(a), 4);
        let text = inv.save();
        assert_eq!(Inventory::load(&text), inv);
        assert_eq!(Inventory::load("no_such_item:5 - stone:0"), Inventory::new());
    }
}
