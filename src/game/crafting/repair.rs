//! Repairing without an anvil: two damaged stacks of the same item laid in
//! the crafting grid, and nothing else, make one that has the uses left of
//! both plus 5 percent of a new one's (`game/gear/wear.rs`, `combine`).
//! A real recipe for the same cells wins. This is not a recipe of the data:
//! it is tried when no recipe matches.

use crate::game::gear::wear;
use crate::inventory::{Slot, Stack};
use alloc::vec::Vec;

/// The recipe number given for a repair, where a recipe's index is expected.
pub const REPAIR: usize = usize::MAX;

/// What the cells repair into, and which two cells it uses up: both must be
/// one item that wears, one of each, at least one of them worn.
/// `durability` gives an item's uses when new.
pub fn plan(cells: &[Slot], durability: &dyn Fn(u16) -> u16) -> Option<(Vec<usize>, Stack)> {
    let filled: Vec<usize> = (0..cells.len()).filter(|&i| cells[i].is_some()).collect();
    if filled.len() != 2 {
        return None;
    }
    let (a, b) = (cells[filled[0]]?, cells[filled[1]]?);
    let max = durability(a.item);
    if max == 0 || (a.wear == 0 && b.wear == 0) {
        return None;
    }
    let made = wear::combine(&a, &b, max)?;
    Some((filled, made))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dur(item: u16) -> u16 {
        if item == 7 {
            100
        } else {
            0
        }
    }

    fn cells(list: &[(usize, Stack)]) -> Vec<Slot> {
        let mut v: Vec<Slot> = alloc::vec![None; 9];
        for (i, s) in list {
            v[*i] = Some(*s);
        }
        v
    }

    #[test]
    fn two_worn_ones_anywhere_in_the_grid_make_a_mended_one() {
        let a = Stack { wear: 60, ..Stack::new(7, 1) };
        let b = Stack { wear: 70, ..Stack::new(7, 1) };
        let (used, made) = plan(&cells(&[(1, a), (8, b)]), &dur).unwrap();
        assert_eq!(used, alloc::vec![1, 8]);
        assert_eq!(made.wear, 25);
    }

    #[test]
    fn nothing_else_repairs() {
        let a = Stack { wear: 60, ..Stack::new(7, 1) };
        // One alone, three, two new ones, two different items, an item that does not wear.
        assert!(plan(&cells(&[(0, a)]), &dur).is_none());
        assert!(plan(&cells(&[(0, a), (1, a), (2, a)]), &dur).is_none());
        assert!(plan(&cells(&[(0, Stack::new(7, 1)), (1, Stack::new(7, 1))]), &dur).is_none());
        assert!(plan(&cells(&[(0, a), (1, Stack::new(8, 1))]), &dur).is_none());
        let plain = Stack { wear: 3, ..Stack::new(9, 1) };
        assert!(plan(&cells(&[(0, plain), (1, plain)]), &dur).is_none());
        assert!(plan(&cells(&[]), &dur).is_none());
    }
}
