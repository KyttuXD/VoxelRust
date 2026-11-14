//! Does what is in the grid make something? The grid is `w` by `h` cells,
//! row by row, each empty or a stack.

use super::recipe::{GridRecipe, Ingredient, Pattern};
use crate::inventory::Slot;
use alloc::vec::Vec;

/// What a matching recipe takes: `(cell index, how many)` for each cell it
/// uses up from.
pub type Uses = Vec<(usize, u16)>;

/// How `recipe` would use the cells of a grid `w` by `h`, or `None` when it
/// does not match them. A shaped recipe matches where it sits anywhere in
/// the grid, also mirrored left to right, with every other cell empty. A
/// shapeless one matches when each ingredient has a cell of its own and no
/// cell is left over.
pub fn plan(recipe: &GridRecipe, w: usize, h: usize, cells: &[Slot]) -> Option<Uses> {
    if w == 0 || h == 0 || cells.len() < w * h {
        return None;
    }
    let cells = &cells[..w * h];
    // Cheap test first: the number of filled cells has to agree.
    if cells.iter().filter(|c| c.is_some()).count() != recipe.cell_count() {
        return None;
    }
    match &recipe.pattern {
        Pattern::Shaped { w: pw, h: ph, cells: pat } => {
            if *pw > w || *ph > h {
                return None;
            }
            for oy in 0..=(h - *ph) {
                for ox in 0..=(w - *pw) {
                    for mirror in [false, true] {
                        if let Some(u) = shaped_at(pat, (*pw, *ph), (ox, oy), mirror, (w, h), cells) {
                            return Some(u);
                        }
                    }
                }
            }
            None
        }
        Pattern::Shapeless(list) => shapeless(list, cells),
    }
}

fn shaped_at(pat: &[Option<Ingredient>], pat_size: (usize, usize), off: (usize, usize), mirror: bool, grid: (usize, usize), cells: &[Slot]) -> Option<Uses> {
    let (pw, ph) = pat_size;
    let (w, h) = grid;
    let mut uses = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let inside = x >= off.0 && x < off.0 + pw && y >= off.1 && y < off.1 + ph;
            let want = if inside {
                let px = x - off.0;
                let px = if mirror { pw - 1 - px } else { px };
                pat[(y - off.1) * pw + px].as_ref()
            } else {
                None
            };
            match (want, cells[y * w + x]) {
                (None, None) => {}
                (Some(ing), Some(s)) => {
                    if !ing.fits(s.item, s.count) {
                        return None;
                    }
                    uses.push((y * w + x, ing.need));
                }
                _ => return None,
            }
        }
    }
    Some(uses)
}

fn shapeless(list: &[Ingredient], cells: &[Slot]) -> Option<Uses> {
    let filled: Vec<usize> = (0..cells.len()).filter(|&i| cells[i].is_some()).collect();
    if filled.len() != list.len() {
        return None;
    }
    // For each ingredient, which of the filled cells would do.
    let ok: Vec<Vec<usize>> = list
        .iter()
        .map(|ing| (0..filled.len()).filter(|&k| cells[filled[k]].map_or(false, |s| ing.fits(s.item, s.count))).collect())
        .collect();
    // Give every ingredient a cell of its own (augmenting paths).
    let mut owner: Vec<Option<usize>> = alloc::vec![None; filled.len()];
    for e in 0..list.len() {
        let mut seen = alloc::vec![false; filled.len()];
        if !augment(e, &ok, &mut seen, &mut owner) {
            return None;
        }
    }
    let mut uses = Vec::new();
    for (k, o) in owner.iter().enumerate() {
        let e = (*o)?;
        uses.push((filled[k], list[e].need));
    }
    Some(uses)
}

fn augment(e: usize, ok: &[Vec<usize>], seen: &mut [bool], owner: &mut [Option<usize>]) -> bool {
    for &c in &ok[e] {
        if seen[c] {
            continue;
        }
        seen[c] = true;
        let free = match owner[c] {
            None => true,
            Some(prev) => augment(prev, ok, seen, owner),
        };
        if free {
            owner[c] = Some(e);
            return true;
        }
    }
    false
}

/// The first recipe of the list that matches the grid: its index and what
/// it takes.
pub fn find(recipes: &[GridRecipe], w: usize, h: usize, cells: &[Slot]) -> Option<(usize, Uses)> {
    recipes.iter().enumerate().find_map(|(i, r)| plan(r, w, h, cells).map(|u| (i, u)))
}

/// Use up what a match takes.
pub fn consume(cells: &mut [Slot], uses: &[(usize, u16)]) {
    for &(i, n) in uses {
        if let Some(slot) = cells.get_mut(i) {
            let left = slot.as_ref().map_or(0, |s| s.count.saturating_sub(n));
            *slot = match *slot {
                Some(s) if left > 0 => Some(crate::inventory::Stack { item: s.item, count: left }),
                _ => None,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Stack;

    const I: u16 = 1; // iron
    const S: u16 = 2; // stick
    const P: u16 = 3; // plank

    fn ing(item: u16) -> Option<Ingredient> {
        Some(Ingredient::one(item))
    }

    fn cell(item: u16, count: u16) -> Slot {
        Some(Stack { item, count })
    }

    /// A grid from rows of letters: `.` empty, `I`, `S`, `P` one item.
    fn grid(rows: &[&str]) -> Vec<Slot> {
        rows.iter()
            .flat_map(|r| r.chars())
            .map(|c| match c {
                'I' => cell(I, 1),
                'S' => cell(S, 1),
                'P' => cell(P, 1),
                _ => None,
            })
            .collect()
    }

    fn pick() -> GridRecipe {
        GridRecipe::shaped((10, 1), 3, 3, alloc::vec![ing(I), ing(I), ing(I), None, ing(S), None, None, ing(S), None]).unwrap()
    }

    fn axe() -> GridRecipe {
        // Two wide, three tall: II / IS / .S
        GridRecipe::shaped((11, 1), 2, 3, alloc::vec![ing(I), ing(I), ing(I), ing(S), None, ing(S)]).unwrap()
    }

    #[test]
    fn a_shaped_recipe_matches_its_pattern() {
        assert!(plan(&pick(), 3, 3, &grid(&["III", ".S.", ".S."])).is_some());
        // Wrong shape, wrong item, a stray item, too few.
        assert!(plan(&pick(), 3, 3, &grid(&["III", "S..", ".S."])).is_none());
        assert!(plan(&pick(), 3, 3, &grid(&["IIP", ".S.", ".S."])).is_none());
        assert!(plan(&pick(), 3, 3, &grid(&["III", ".S.", ".SS"])).is_none());
        assert!(plan(&pick(), 3, 3, &grid(&["II.", ".S.", ".S."])).is_none());
        assert!(plan(&pick(), 3, 3, &grid(&["...", "...", "..."])).is_none());
        // It does not fit a 2 x 2 grid at all.
        assert!(plan(&pick(), 2, 2, &grid(&["II", ".S"])).is_none());
    }

    #[test]
    fn a_shaped_recipe_matches_anywhere_in_the_grid() {
        let stick = GridRecipe::shaped((12, 4), 1, 2, alloc::vec![ing(P), ing(P)]).unwrap();
        for rows in [["P..", "P..", "..."], [".P.", ".P.", "..."], ["..P", "..P", "..."], ["...", "P..", "P.."], ["...", "..P", "..P"]] {
            assert!(plan(&stick, 3, 3, &grid(&rows)).is_some(), "{:?}", rows);
        }
        // Side by side is a different shape.
        assert!(plan(&stick, 3, 3, &grid(&["PP.", "...", "..."])).is_none());
        // And in a 2 x 2 grid.
        assert!(plan(&stick, 2, 2, &grid(&[".P", ".P"])).is_some());
        // A single item can be in any of the nine cells.
        let one = GridRecipe::shaped((13, 1), 1, 1, alloc::vec![ing(I)]).unwrap();
        for k in 0..9 {
            let mut cells = alloc::vec![None; 9];
            cells[k] = cell(I, 5);
            assert_eq!(plan(&one, 3, 3, &cells), Some(alloc::vec![(k, 1)]));
        }
    }

    #[test]
    fn a_shaped_recipe_matches_mirrored() {
        assert!(plan(&axe(), 3, 3, &grid(&["II.", "IS.", ".S."])).is_some());
        assert!(plan(&axe(), 3, 3, &grid(&[".II", ".SI", ".S."])).is_some());
        // Turned upside down is not allowed.
        assert!(plan(&axe(), 3, 3, &grid(&[".S.", "IS.", "II."])).is_none());
        // Every filled cell is used once (the axe has five).
        let uses = plan(&axe(), 3, 3, &grid(&[".II", ".SI", ".S."])).unwrap();
        assert_eq!(uses.len(), 5);
    }

    #[test]
    fn shapeless_recipes_match_in_any_order_and_place() {
        let r = GridRecipe::shapeless((14, 1), alloc::vec![Ingredient::one(I), Ingredient::one(S), Ingredient::one(S)]).unwrap();
        assert!(plan(&r, 3, 3, &grid(&["ISS", "...", "..."])).is_some());
        assert!(plan(&r, 3, 3, &grid(&["...", "S.S", "..I"])).is_some());
        assert!(plan(&r, 2, 2, &grid(&["SI", "S."])).is_some());
        // The wrong mix, an extra item, a missing one.
        assert!(plan(&r, 3, 3, &grid(&["IIS", "...", "..."])).is_none());
        assert!(plan(&r, 3, 3, &grid(&["ISS", "P..", "..."])).is_none());
        assert!(plan(&r, 3, 3, &grid(&["IS.", "...", "..."])).is_none());
    }

    #[test]
    fn ingredients_may_be_groups_and_need_more_than_one() {
        // Any plank or stick, then at least 3 iron in one cell.
        let r = GridRecipe::shapeless((15, 1), alloc::vec![Ingredient { items: alloc::vec![P, S], need: 1 }, Ingredient { items: alloc::vec![I], need: 3 }]).unwrap();
        let mut cells = alloc::vec![None; 4];
        cells[0] = cell(S, 1);
        cells[3] = cell(I, 3);
        assert_eq!(plan(&r, 2, 2, &cells), Some(alloc::vec![(0, 1), (3, 3)]));
        cells[0] = cell(P, 2);
        assert!(plan(&r, 2, 2, &cells).is_some());
        cells[3] = cell(I, 2);
        assert!(plan(&r, 2, 2, &cells).is_none());
        // An ingredient that can take either cell does not steal the other one.
        let any = Ingredient { items: alloc::vec![I, S], need: 1 };
        let r = GridRecipe::shapeless((16, 1), alloc::vec![any, Ingredient::one(I)]).unwrap();
        let mut cells = alloc::vec![None; 4];
        cells[0] = cell(I, 1);
        cells[1] = cell(S, 1);
        let uses = plan(&r, 2, 2, &cells).unwrap();
        assert_eq!(uses.len(), 2);
        cells[1] = cell(P, 1);
        assert!(plan(&r, 2, 2, &cells).is_none());
    }

    #[test]
    fn counts_in_the_cells_are_used_one_at_a_time() {
        let mut cells = grid(&["III", ".S.", ".S."]);
        cells[0] = cell(I, 3);
        cells[4] = cell(S, 2);
        let uses = plan(&pick(), 3, 3, &cells).unwrap();
        consume(&mut cells, &uses);
        assert_eq!((cells[0], cells[1], cells[4], cells[7]), (cell(I, 2), None, cell(S, 1), None));
        assert!(find(&[pick(), axe()], 3, 3, &cells).is_none());
    }

    #[test]
    fn find_returns_the_first_match() {
        let list = [pick(), axe()];
        assert_eq!(find(&list, 3, 3, &grid(&["II.", "IS.", ".S."])).map(|f| f.0), Some(1));
        assert_eq!(find(&list, 3, 3, &grid(&["III", ".S.", ".S."])).map(|f| f.0), Some(0));
        assert!(find(&list, 3, 3, &grid(&["III", ".S.", "..."])).is_none());
    }

    /// A recipe from rows of letters (`.` empty, `I`, `S`, `P`).
    fn shaped(rows: &[&str]) -> GridRecipe {
        let cells = rows
            .iter()
            .flat_map(|r| r.chars())
            .map(|c| match c {
                'I' => ing(I),
                'S' => ing(S),
                'P' => ing(P),
                _ => None,
            })
            .collect();
        GridRecipe::shaped((20, 1), rows[0].len(), rows.len(), cells).unwrap()
    }

    #[test]
    fn a_five_wide_recipe_matches_a_bench_anywhere_and_mirrored() {
        // Five wide, three tall, and not the same either way round.
        let rows = ["IIIII", "S...P", "SSSSP"];
        let r = shaped(&rows);
        assert_eq!(r.min_side(), 5);
        // In a 5 x 5 grid at each of the three heights.
        for oy in 0..3 {
            let mut g: Vec<&str> = alloc::vec!["....."; 5];
            for k in 0..3 {
                g[oy + k] = rows[k];
            }
            let cells = grid(&g);
            let uses = plan(&r, 5, 5, &cells).expect("matches");
            assert_eq!(uses.len(), 12, "oy {}", oy);
        }
        // Mirrored left to right: P and S change places.
        assert!(plan(&r, 5, 5, &grid(&["IIIII", "P...S", "PSSSS", ".....", "....."])).is_some());
        // Upside down is not allowed, nor is a stray cell.
        assert!(plan(&r, 5, 5, &grid(&["SSSSP", "S...P", "IIIII", ".....", "....."])).is_none());
        assert!(plan(&r, 5, 5, &grid(&["IIIII", "S...P", "SSSSP", ".....", "....P"])).is_none());
        // A table's grid is too small, and a 3 x 3 slice of it is no recipe.
        assert!(!r.fits(3, 3));
        assert!(r.fits(5, 5) && r.fits(7, 7));
        assert!(plan(&r, 3, 3, &grid(&["III", "S..", "SSS"])).is_none());
        // The same cells in a 7 x 7 grid (the workshop) make it too.
        let mut big = alloc::vec![None; 49];
        for (k, row) in rows.iter().enumerate() {
            for (j, c) in row.chars().enumerate() {
                big[(k + 4) * 7 + j + 2] = match c {
                    'I' => cell(I, 1),
                    'S' => cell(S, 1),
                    'P' => cell(P, 1),
                    _ => None,
                };
            }
        }
        let uses = plan(&r, 7, 7, &big).expect("matches in 7 x 7");
        assert_eq!(uses.len(), 12);
        assert!(uses.contains(&(4 * 7 + 2, 1)));
    }

    #[test]
    fn a_seven_by_seven_recipe_needs_the_workshop() {
        let rows = ["I.....P", ".......", "..S.S..", "...P...", "..S.S..", ".......", "P.....I"];
        let r = shaped(&rows);
        assert_eq!((r.min_side(), r.cell_count()), (7, 9));
        assert!(!r.fits(5, 5));
        assert!(plan(&r, 7, 7, &grid(&rows)).is_some());
        // The mirror image is the same recipe; one corner gone is not.
        assert!(plan(&r, 7, 7, &grid(&["P.....I", ".......", "..S.S..", "...P...", "..S.S..", ".......", "I.....P"])).is_some());
        assert!(plan(&r, 7, 7, &grid(&["I.....P", ".......", "..S.S..", "...P...", "..S.S..", ".......", "P......"])).is_none());
        // A 5 x 5 grid cannot hold it at all.
        assert!(plan(&r, 5, 5, &grid(&["I...P", ".....", "..S..", ".....", "P...I"])).is_none());
        // Taking from the grid uses each of the nine cells once.
        let mut cells = grid(&rows);
        let uses = plan(&r, 7, 7, &cells).unwrap();
        consume(&mut cells, &uses);
        assert!(cells.iter().all(|c| c.is_none()));
    }

    #[test]
    fn find_in_a_big_grid_still_finds_small_recipes() {
        let list = [shaped(&["IIIII", "S...S"]), pick(), shaped(&["I", "S"])];
        let mut cells = alloc::vec![None; 25];
        cells[12] = cell(I, 1);
        cells[17] = cell(S, 1);
        assert_eq!(find(&list, 5, 5, &cells).map(|f| f.0), Some(2));
        // A table's pick is made on a bench too, wherever it lies.
        let mut cells = alloc::vec![None; 25];
        for k in [6, 7, 8, 12, 17] {
            cells[k] = cell(if k < 9 { I } else { S }, 1);
        }
        assert_eq!(find(&list, 5, 5, &cells).map(|f| f.0), Some(1));
    }

    #[test]
    fn a_too_short_slice_matches_nothing() {
        assert!(plan(&pick(), 3, 3, &grid(&["III", ".S."])).is_none());
    }
}
