//! The crafting grid: recipes laid out in cells (2 x 2 in the inventory
//! screen, 3 x 3 at a crafting table), an output that takes one of each
//! ingredient, and a way to fill the grid from the inventory.
//!
//! - [`recipe`]: what a recipe is (shaped or shapeless).
//! - [`matcher`]: whether the cells make a recipe, and what they give up.
//! - [`parse`]: the `craft` and `shaped` data lines.
//!
//! The screen itself is in `client/screens/crafting.rs`.

pub mod check;
pub mod matcher;
pub mod parse;
pub mod recipe;

#[cfg(test)]
mod data_tests;

pub use recipe::{GridRecipe, Ingredient, Pattern, HAND_SIDE, MAX_SIDE};

use crate::inventory::{click_slot, stack_limit, Inventory, Slot, Stack};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Cells in the biggest grid.
pub const CELLS: usize = MAX_SIDE * MAX_SIDE;

/// What lies in the crafting grid while its screen is open. The grid is
/// `side` by `side`, row by row, in the first `side * side` cells.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CraftGrid {
    pub cells: [Slot; CELLS],
    /// Items the recipe book wants in empty cells, shown dimmed.
    pub ghost: [Option<u16>; CELLS],
}

/// Whether `item` x `count` would fit into the inventory.
fn room_for(inv: &Inventory, item: u16, count: u16) -> bool {
    let limit = stack_limit(item) as u32;
    let mut room = 0u32;
    for s in inv.slots.iter() {
        match s {
            None => room += limit,
            Some(s) if s.item == item => room += limit.saturating_sub(s.count as u32),
            _ => {}
        }
    }
    room >= count as u32
}

impl CraftGrid {
    pub fn new() -> CraftGrid {
        CraftGrid::default()
    }

    pub fn is_empty(&self) -> bool {
        self.cells.iter().all(|c| c.is_none())
    }

    /// The recipe the cells make (its index in `recipes`) and what it gives.
    pub fn output(&self, recipes: &[GridRecipe], side: usize) -> Option<(usize, Stack)> {
        let side = side.min(MAX_SIDE);
        let (i, _) = matcher::find(recipes, side, side, &self.cells)?;
        let (item, count) = recipes[i].out;
        Some((i, Stack { item, count }))
    }

    /// A click on one cell, as in the inventory.
    pub fn click_cell(&mut self, cell: usize, cursor: &mut Slot, right: bool) {
        if let Some(here) = self.cells.get_mut(cell) {
            click_slot(here, cursor, right);
        }
        self.ghost = [None; CELLS];
    }

    /// Move what a cell holds into the inventory (shift-click on it).
    pub fn send_back(&mut self, cell: usize, inv: &mut Inventory) {
        if let Some(slot) = self.cells.get_mut(cell) {
            if let Some(s) = *slot {
                let left = inv.add(s.item, s.count);
                *slot = if left > 0 { Some(Stack { item: s.item, count: left }) } else { None };
            }
        }
        self.ghost = [None; CELLS];
    }

    /// Click on the output: the result goes onto the hand when it is empty
    /// or already holds the same item with room for more, and the grid
    /// gives up one of each ingredient. Whether anything was made.
    pub fn take(&mut self, recipes: &[GridRecipe], side: usize, cursor: &mut Slot) -> bool {
        let side = side.min(MAX_SIDE);
        let Some((i, uses)) = matcher::find(recipes, side, side, &self.cells) else { return false };
        let (item, count) = recipes[i].out;
        match *cursor {
            None => *cursor = Some(Stack { item, count }),
            Some(c) if c.item == item && c.count as u32 + count as u32 <= stack_limit(item) as u32 => {
                *cursor = Some(Stack { item, count: c.count + count });
            }
            _ => return false,
        }
        matcher::consume(&mut self.cells, &uses);
        self.ghost = [None; CELLS];
        true
    }

    /// Shift-click on the output: make as much as the grid holds and the
    /// inventory has room for, straight into the inventory. How many times
    /// the recipe was made.
    pub fn take_all(&mut self, recipes: &[GridRecipe], side: usize, inv: &mut Inventory) -> u32 {
        let side = side.min(MAX_SIDE);
        let mut made = 0;
        // Each round uses up at least one item, so this ends.
        while made < 2000 {
            let Some((i, uses)) = matcher::find(recipes, side, side, &self.cells) else { break };
            let (item, count) = recipes[i].out;
            if !room_for(inv, item, count) {
                break;
            }
            matcher::consume(&mut self.cells, &uses);
            inv.add(item, count);
            made += 1;
        }
        if made > 0 {
            self.ghost = [None; CELLS];
        }
        made
    }

    /// Give every cell's contents back to the inventory (what does not fit
    /// stays where it is).
    pub fn return_all(&mut self, inv: &mut Inventory) {
        for cell in self.cells.iter_mut() {
            if let Some(s) = *cell {
                let left = inv.add(s.item, s.count);
                *cell = if left > 0 { Some(Stack { item: s.item, count: left }) } else { None };
            }
        }
        self.ghost = [None; CELLS];
    }

    /// Lay `recipe` out in the grid from the inventory (the recipe book's
    /// click): the grid is emptied first, then each ingredient is taken
    /// from the inventory. What the inventory lacks is left as a ghost.
    pub fn fill(&mut self, recipe: &GridRecipe, side: usize, inv: &mut Inventory) {
        let side = side.min(MAX_SIDE);
        if !recipe.fits(side, side) {
            return;
        }
        self.return_all(inv);
        if !self.is_empty() {
            return;
        }
        for (cell, ing) in recipe.layout(side) {
            let mut best: Option<u16> = None;
            for &it in &ing.items {
                let have = inv.count(it);
                if have >= ing.need as u32 && best.map_or(true, |b| have > inv.count(b)) {
                    best = Some(it);
                }
            }
            match best {
                Some(it) => {
                    inv.remove(it, ing.need);
                    self.cells[cell] = Some(Stack { item: it, count: ing.need });
                }
                None => self.ghost[cell] = ing.items.first().copied(),
            }
        }
    }
}

/// How many of each item the inventory and the grid hold together.
pub fn available(inv: &Inventory, grid: &CraftGrid) -> BTreeMap<u16, u32> {
    let mut have = BTreeMap::new();
    for s in inv.slots.iter().chain(grid.cells.iter()).flatten() {
        *have.entry(s.item).or_insert(0u32) += s.count as u32;
    }
    have
}

/// Whether `have` covers every ingredient of the recipe.
pub fn can_make(recipe: &GridRecipe, have: &BTreeMap<u16, u32>) -> bool {
    let mut used: Vec<(u16, u32)> = Vec::new();
    for ing in recipe.ingredients() {
        let free = |it: u16, used: &[(u16, u32)]| -> u32 {
            let taken: u32 = used.iter().filter(|u| u.0 == it).map(|u| u.1).sum();
            have.get(&it).copied().unwrap_or(0).saturating_sub(taken)
        };
        let best = ing.items.iter().copied().max_by_key(|&it| free(it, &used));
        match best {
            Some(it) if free(it, &used) >= ing.need as u32 => used.push((it, ing.need as u32)),
            _ => return false,
        }
    }
    true
}

/// The recipes (indices into `recipes`) that fit a grid `side` by `side`.
pub fn fitting(recipes: &[GridRecipe], side: usize) -> Vec<usize> {
    (0..recipes.len()).filter(|&i| recipes[i].fits(side, side)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::reg;

    fn item(id: &str) -> u16 {
        reg().items.iter().position(|i| i.id == id).unwrap() as u16
    }

    fn stack(id: &str, n: u16) -> Slot {
        Some(Stack { item: item(id), count: n })
    }

    /// Sticks from two planks, and a plank-and-stick pick that needs a table.
    fn recipes() -> Vec<GridRecipe> {
        let (planks, stick, pick) = (item("planks"), item("stick"), item("wood_pick"));
        let p = || Some(Ingredient::one(planks));
        let s = || Some(Ingredient::one(stick));
        alloc::vec![
            GridRecipe::shaped((stick, 4), 1, 2, alloc::vec![p(), p()]).unwrap(),
            GridRecipe::shaped((pick, 1), 3, 3, alloc::vec![p(), p(), p(), None, s(), None, None, s(), None]).unwrap(),
        ]
    }

    #[test]
    fn taking_the_output_uses_one_of_each_and_stacks_on_the_hand() {
        let r = recipes();
        let mut g = CraftGrid::new();
        g.cells[0] = stack("planks", 3);
        // One plank is not enough; two, one above the other, make sticks.
        assert!(g.output(&r, 2).is_none());
        g.cells[2] = stack("planks", 5);
        assert!(g.output(&r, 2).is_some());
        let mut hand: Slot = None;
        assert!(g.take(&r, 2, &mut hand));
        assert_eq!(hand, stack("stick", 4));
        assert_eq!((g.cells[0], g.cells[2]), (stack("planks", 2), stack("planks", 4)));
        assert!(g.take(&r, 2, &mut hand));
        assert_eq!(hand, stack("stick", 8));
        // A hand holding something else cannot take it.
        let mut other = stack("dirt", 1);
        assert!(!g.take(&r, 2, &mut other));
        assert_eq!(other, stack("dirt", 1));
        assert_eq!(g.cells[0], stack("planks", 1));
    }

    #[test]
    fn a_full_stack_on_the_hand_cannot_take_more() {
        let r = recipes();
        let mut g = CraftGrid::new();
        g.cells[0] = stack("planks", 1);
        g.cells[2] = stack("planks", 1);
        let mut hand = stack("stick", 62);
        assert!(!g.take(&r, 2, &mut hand));
        assert_eq!(g.cells[0], stack("planks", 1));
    }

    #[test]
    fn shift_click_makes_as_many_as_possible() {
        let r = recipes();
        let mut g = CraftGrid::new();
        g.cells[0] = stack("planks", 5);
        g.cells[2] = stack("planks", 3);
        let mut inv = Inventory::new();
        assert_eq!(g.take_all(&r, 2, &mut inv), 3);
        assert_eq!(inv.count(item("stick")), 12);
        assert_eq!((g.cells[0], g.cells[2]), (stack("planks", 2), None));
        // With no room, nothing is made.
        let mut full = Inventory::new();
        full.add(item("dirt"), 64 * 36);
        g.cells[2] = stack("planks", 3);
        assert_eq!(g.take_all(&r, 2, &mut full), 0);
        assert_eq!(g.cells[2], stack("planks", 3));
    }

    #[test]
    fn the_grid_size_matters() {
        let r = recipes();
        let mut g = CraftGrid::new();
        let (planks, stick) = ("planks", "stick");
        // The pick laid out in 3 x 3 cells.
        for (i, id) in [(0, planks), (1, planks), (2, planks), (4, stick), (7, stick)] {
            g.cells[i] = stack(id, 1);
        }
        assert_eq!(g.output(&r, 3).map(|o| o.0), Some(1));
        // In a 2 x 2 grid the same cells read differently (and make nothing).
        assert!(g.output(&r, 2).is_none());
        assert!(!r[1].fits(2, 2));
        assert!(r[0].fits(2, 2));
    }

    #[test]
    fn closing_gives_everything_back_and_filling_takes_from_the_inventory() {
        let r = recipes();
        let mut inv = Inventory::new();
        inv.add(item("planks"), 7);
        let mut g = CraftGrid::new();
        g.fill(&r[0], 2, &mut inv);
        assert_eq!(inv.count(item("planks")), 5);
        assert_eq!((g.cells[0], g.cells[2]), (stack("planks", 1), stack("planks", 1)));
        assert!(g.output(&r, 2).is_some());
        // Filling the pick lacks sticks: they are ghosts.
        g.fill(&r[1], 3, &mut inv);
        assert_eq!(inv.count(item("planks")), 4);
        assert_eq!(g.cells[0], stack("planks", 1));
        assert_eq!(g.ghost[4], Some(item("stick")));
        assert_eq!(g.ghost[7], Some(item("stick")));
        assert!(g.output(&r, 3).is_none());
        // The recipe book's list: the pick is not makeable, sticks are.
        let have = available(&inv, &g);
        assert!(can_make(&r[0], &have));
        assert!(!can_make(&r[1], &have));
        g.return_all(&mut inv);
        assert!(g.is_empty());
        assert_eq!(inv.count(item("planks")), 7);
    }
}
