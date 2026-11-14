//! Looking for recipes that cannot be told apart: two that make different
//! things from the same arrangement in the grid. The game takes the first
//! that matches, so the other could never be made. Used by the tests of the
//! game's data and of the mods.

use super::matcher;
use super::recipe::{GridRecipe, TABLE_SIDE};
use crate::inventory::{Slot, Stack};
use alloc::vec::Vec;

/// The grid a recipe is laid out in at its top left, with the first item of
/// each ingredient and as many of it as it needs: its side (at least 3, the
/// crafting table's) and the cells.
pub fn canonical(recipe: &GridRecipe) -> (usize, Vec<Slot>) {
    let side = recipe.min_side().max(TABLE_SIDE);
    let mut cells: Vec<Slot> = alloc::vec![None; side * side];
    for (cell, ing) in recipe.layout(side) {
        if let (Some(&item), Some(slot)) = (ing.items.first(), cells.get_mut(cell)) {
            *slot = Some(Stack { item, count: ing.need });
        }
    }
    (side, cells)
}

/// Pairs `(a, b)` of recipe indices where `b` also matches what `a` is made
/// from although it makes something else (a different item or count).
pub fn clashes(recipes: &[GridRecipe]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (a, ra) in recipes.iter().enumerate() {
        let (side, cells) = canonical(ra);
        for (b, rb) in recipes.iter().enumerate() {
            if a != b && rb.out != ra.out && matcher::plan(rb, side, side, &cells).is_some() {
                out.push((a, b));
            }
        }
    }
    out
}

/// Why a recipe is malformed, if it is: what the parser makes can always be
/// laid out, but recipes built by hand might not.
pub fn problem(recipe: &GridRecipe) -> Option<&'static str> {
    use super::recipe::{Pattern, MAX_SIDE};
    if recipe.out.1 == 0 {
        return Some("makes none");
    }
    match &recipe.pattern {
        Pattern::Shaped { w, h, cells } => {
            if *w == 0 || *h == 0 || *w > MAX_SIDE || *h > MAX_SIDE || cells.len() != w * h {
                return Some("bad size");
            }
            let row_used = |y: usize| (0..*w).any(|x| cells[y * w + x].is_some());
            let col_used = |x: usize| (0..*h).any(|y| cells[y * w + x].is_some());
            if !row_used(0) || !row_used(h - 1) || !col_used(0) || !col_used(w - 1) {
                return Some("empty edge");
            }
        }
        Pattern::Shapeless(list) => {
            if list.is_empty() || list.len() > TABLE_SIDE * TABLE_SIDE {
                return Some("bad ingredient count");
            }
        }
    }
    if recipe.ingredients().iter().any(|i| i.items.is_empty() || i.need == 0) {
        return Some("an ingredient is nothing");
    }
    None
}
