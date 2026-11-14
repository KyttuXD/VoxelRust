//! What a crafting-grid recipe is: a pattern of ingredients and a result.

use alloc::vec::Vec;

/// The biggest grid there is (the workshop's, 7 x 7).
pub const MAX_SIDE: usize = 7;
/// The grid of the player's own inventory screen.
pub const HAND_SIDE: usize = 2;
/// The crafting table's grid. A shapeless recipe never needs more room than
/// this: its units get a cell each up to `TABLE_SIDE` squared, and are
/// folded into fewer cells with bigger needs beyond that (see `parse`).
pub const TABLE_SIDE: usize = 3;
/// The crafting bench's grid (5 x 5).
pub const BENCH_SIDE: usize = 5;
/// Every grid there is, smallest first: the inventory's, the crafting
/// table's, the bench's and the workshop's.
pub const SIDES: [usize; 4] = [HAND_SIDE, TABLE_SIDE, BENCH_SIDE, MAX_SIDE];

/// What one cell of the grid must hold: any one of `items`, at least `need`
/// of it (the usual need is 1). A single-item list is a plain ingredient; a
/// longer one comes from a `group` (any planks, any wool...).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ingredient {
    pub items: Vec<u16>,
    pub need: u16,
}

impl Ingredient {
    pub fn one(item: u16) -> Ingredient {
        Ingredient { items: alloc::vec![item], need: 1 }
    }

    pub fn accepts(&self, item: u16) -> bool {
        self.items.contains(&item)
    }

    /// Whether a cell holding `count` of `item` satisfies this ingredient.
    pub fn fits(&self, item: u16, count: u16) -> bool {
        count >= self.need && self.accepts(item)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pattern {
    /// `w` x `h` cells, row by row. `None` must be left empty. The first and
    /// last row and column hold at least one ingredient each (the pattern
    /// can then sit anywhere in a bigger grid).
    Shaped { w: usize, h: usize, cells: Vec<Option<Ingredient>> },
    /// These ingredients, one to a cell, in any places.
    Shapeless(Vec<Ingredient>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridRecipe {
    /// Item made and how many.
    pub out: (u16, u16),
    pub pattern: Pattern,
}

impl GridRecipe {
    /// A shaped recipe. Empty rows and columns around the edge are trimmed
    /// away. `None` when `cells` is not `w * h`, nothing is in it, or it is
    /// bigger than the biggest grid (7 x 7) once trimmed.
    pub fn shaped(out: (u16, u16), w: usize, h: usize, cells: Vec<Option<Ingredient>>) -> Option<GridRecipe> {
        if w == 0 || h == 0 || cells.len() != w * h {
            return None;
        }
        let used = |x: usize, y: usize| cells[y * w + x].is_some();
        let rows: Vec<usize> = (0..h).filter(|&y| (0..w).any(|x| used(x, y))).collect();
        let cols: Vec<usize> = (0..w).filter(|&x| (0..h).any(|y| used(x, y))).collect();
        let (y0, y1, x0, x1) = match (rows.first(), rows.last(), cols.first(), cols.last()) {
            (Some(&a), Some(&b), Some(&c), Some(&d)) => (a, b, c, d),
            _ => return None,
        };
        let (nw, nh) = (x1 - x0 + 1, y1 - y0 + 1);
        if nw > MAX_SIDE || nh > MAX_SIDE {
            return None;
        }
        let mut trimmed = Vec::with_capacity(nw * nh);
        for y in y0..=y1 {
            for x in x0..=x1 {
                trimmed.push(cells[y * w + x].clone());
            }
        }
        Some(GridRecipe { out, pattern: Pattern::Shaped { w: nw, h: nh, cells: trimmed } })
    }

    /// A shapeless recipe: one to nine ingredients (a table's cells).
    pub fn shapeless(out: (u16, u16), ingredients: Vec<Ingredient>) -> Option<GridRecipe> {
        if ingredients.is_empty() || ingredients.len() > TABLE_SIDE * TABLE_SIDE {
            return None;
        }
        Some(GridRecipe { out, pattern: Pattern::Shapeless(ingredients) })
    }

    /// The ingredients that must be in the grid, in pattern order.
    pub fn ingredients(&self) -> Vec<&Ingredient> {
        match &self.pattern {
            Pattern::Shaped { cells, .. } => cells.iter().flatten().collect(),
            Pattern::Shapeless(list) => list.iter().collect(),
        }
    }

    /// What the recipe uses in all, for showing it: each ingredient's first
    /// item and how many of it (the cells that ask for it, times their need).
    pub fn totals(&self) -> Vec<(u16, u32)> {
        let mut out: Vec<(u16, u32)> = Vec::new();
        for ing in self.ingredients() {
            let Some(&item) = ing.items.first() else { continue };
            match out.iter_mut().find(|t| t.0 == item) {
                Some(t) => t.1 += ing.need as u32,
                None => out.push((item, ing.need as u32)),
            }
        }
        out
    }

    /// Cells that must be filled.
    pub fn cell_count(&self) -> usize {
        match &self.pattern {
            Pattern::Shaped { cells, .. } => cells.iter().filter(|c| c.is_some()).count(),
            Pattern::Shapeless(list) => list.len(),
        }
    }

    /// Whether this can be laid out in a grid `w` by `h`.
    pub fn fits(&self, w: usize, h: usize) -> bool {
        match &self.pattern {
            Pattern::Shaped { w: pw, h: ph, .. } => *pw <= w && *ph <= h,
            Pattern::Shapeless(list) => list.len() <= w * h,
        }
    }

    /// The side of the smallest grid this can be made in: 2 (by hand), 3 (a
    /// crafting table), 5 (a crafting bench) or 7 (a workshop). It follows
    /// from the size of the pattern; nothing is written in the data.
    pub fn min_side(&self) -> usize {
        SIDES.iter().copied().find(|&n| self.fits(n, n)).unwrap_or(MAX_SIDE)
    }

    /// Whether the 2 x 2 grid of the inventory screen is too small for it.
    pub fn needs_table(&self) -> bool {
        !self.fits(HAND_SIDE, HAND_SIDE)
    }

    /// Where each ingredient goes when the recipe is laid out at the top
    /// left of a grid `w` wide: `(cell index, ingredient)`.
    pub fn layout(&self, w: usize) -> Vec<(usize, &Ingredient)> {
        let mut out = Vec::new();
        match &self.pattern {
            Pattern::Shaped { w: pw, cells, .. } => {
                for (i, c) in cells.iter().enumerate() {
                    if let Some(ing) = c {
                        out.push(((i / pw) * w + i % pw, ing));
                    }
                }
            }
            Pattern::Shapeless(list) => {
                for (i, ing) in list.iter().enumerate() {
                    out.push((i, ing));
                }
            }
        }
        out
    }
}
