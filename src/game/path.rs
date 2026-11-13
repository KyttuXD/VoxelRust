//! Path finding for creatures: A* over the cells a walker can stand in.
//!
//! A cell can be stood in when it and the cell above are free and the cell
//! below is solid. From there a walker may move to any of the four
//! neighbouring columns: level, one block up (if there is head room for the
//! jump), or up to three blocks down.

use alloc::collections::{BTreeMap, BinaryHeap};
use alloc::vec::Vec;
use core::cmp::Reverse;

pub type Cell = [i32; 3];

/// What the path finder needs to know about the world: is this block solid?
pub trait Ground {
    fn solid(&self, x: i32, y: i32, z: i32) -> bool;
}

impl<F: Fn(i32, i32, i32) -> bool> Ground for F {
    fn solid(&self, x: i32, y: i32, z: i32) -> bool {
        self(x, y, z)
    }
}

/// Whether a two-block-tall walker can stand with its feet in `c`.
pub fn standable(g: &dyn Ground, c: Cell) -> bool {
    !g.solid(c[0], c[1], c[2]) && !g.solid(c[0], c[1] + 1, c[2]) && g.solid(c[0], c[1] - 1, c[2])
}

/// The standable cell in column (x, z) nearest to height `y`, searching a
/// few blocks up and down.
pub fn ground_near(g: &dyn Ground, x: i32, y: i32, z: i32, range: i32) -> Option<Cell> {
    for d in 0..=range {
        for cy in [y - d, y + d] {
            if standable(g, [x, cy, z]) {
                return Some([x, cy, z]);
            }
        }
    }
    None
}

fn neighbours(g: &dyn Ground, c: Cell, out: &mut Vec<(Cell, u32)>) {
    out.clear();
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let (x, z) = (c[0] + dx, c[2] + dz);
        if standable(g, [x, c[1], z]) {
            out.push(([x, c[1], z], 10));
        } else if standable(g, [x, c[1] + 1, z]) && !g.solid(c[0], c[1] + 2, c[2]) {
            // A jump up one block: needs room above the walker's own head.
            out.push(([x, c[1] + 1, z], 16));
        } else if !g.solid(x, c[1], z) && !g.solid(x, c[1] + 1, z) {
            // A drop: the gap must be open, and the landing not too far down.
            for down in 1..=3 {
                if standable(g, [x, c[1] - down, z]) {
                    out.push(([x, c[1] - down, z], 10 + 4 * down as u32));
                    break;
                }
                if g.solid(x, c[1] - down, z) {
                    break;
                }
            }
        }
    }
}

fn estimate(a: Cell, b: Cell) -> u32 {
    ((a[0] - b[0]).unsigned_abs() + (a[2] - b[2]).unsigned_abs()) * 10 + (a[1] - b[1]).unsigned_abs() * 6
}

/// The cells to walk through from `start` to `goal`, not including `start`.
///
/// At most `max_nodes` cells are examined. If the goal is not reached within
/// that budget (or cannot be reached at all), the path leads to the examined
/// cell closest to the goal instead, so a creature still makes progress.
/// Returns `None` only when `start` itself cannot be stood in or nothing
/// nearer than `start` was found.
pub fn find_path(g: &dyn Ground, start: Cell, goal: Cell, max_nodes: usize) -> Option<Vec<Cell>> {
    if !standable(g, start) {
        return None;
    }
    // cell -> (cost so far, the cell we came from)
    let mut seen: BTreeMap<Cell, (u32, Cell)> = BTreeMap::new();
    let mut open: BinaryHeap<Reverse<(u32, u32, Cell)>> = BinaryHeap::new();
    seen.insert(start, (0, start));
    open.push(Reverse((estimate(start, goal), 0, start)));
    let mut best = (estimate(start, goal), start);
    let mut scratch = Vec::with_capacity(4);
    let mut examined = 0;
    let mut reached = None;
    while let Some(Reverse((_, cost, cell))) = open.pop() {
        if seen.get(&cell).map_or(true, |s| s.0 < cost) {
            continue; // a cheaper route to this cell was already expanded
        }
        if cell == goal {
            reached = Some(cell);
            break;
        }
        let h = estimate(cell, goal);
        if h < best.0 {
            best = (h, cell);
        }
        examined += 1;
        if examined >= max_nodes {
            break;
        }
        neighbours(g, cell, &mut scratch);
        for &(next, step) in &scratch {
            let c = cost + step;
            if seen.get(&next).map_or(true, |s| c < s.0) {
                seen.insert(next, (c, cell));
                open.push(Reverse((c + estimate(next, goal), c, next)));
            }
        }
    }
    let end = reached.unwrap_or(best.1);
    if end == start {
        return None;
    }
    let mut path = Vec::new();
    let mut at = end;
    while at != start {
        path.push(at);
        at = seen[&at].1;
    }
    path.reverse();
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A world from rows of text: one string per z, characters are heights
    /// (0-9) of solid ground in that column; '#' is a wall 9 high.
    fn grid(rows: &'static [&'static str]) -> impl Fn(i32, i32, i32) -> bool {
        move |x, y, z| {
            if z < 0 || x < 0 || z as usize >= rows.len() || x as usize >= rows[z as usize].len() {
                return true; // solid outside: a closed arena
            }
            let c = rows[z as usize].as_bytes()[x as usize];
            let h = if c == b'#' { 9 } else { (c - b'0') as i32 };
            y < h
        }
    }

    fn assert_walkable(g: &dyn Ground, start: Cell, path: &[Cell]) {
        let mut at = start;
        for &c in path {
            assert!(standable(g, c), "path enters a cell that cannot be stood in: {c:?}");
            let (dx, dz) = ((c[0] - at[0]).abs(), (c[2] - at[2]).abs());
            assert_eq!(dx + dz, 1, "path jumps from {at:?} to {c:?}");
            assert!(c[1] - at[1] <= 1 && at[1] - c[1] <= 3, "bad height change {at:?} -> {c:?}");
            at = c;
        }
    }

    #[test]
    fn straight_line_on_flat_ground() {
        let g = grid(&["11111", "11111", "11111"]);
        let p = find_path(&g, [0, 1, 1], [4, 1, 1], 200).unwrap();
        assert_eq!(p.len(), 4);
        assert_eq!(*p.last().unwrap(), [4, 1, 1]);
        assert_walkable(&g, [0, 1, 1], &p);
    }

    #[test]
    fn goes_around_a_wall() {
        let g = grid(&["11111", "1###1", "11111", "1###1", "11111"]);
        // From the middle row's left to the middle row's right is direct;
        // from top-left to bottom-right must go around the walls.
        let p = find_path(&g, [0, 1, 0], [4, 1, 4], 500).unwrap();
        assert_eq!(*p.last().unwrap(), [4, 1, 4]);
        assert_eq!(p.len(), 8);
        assert_walkable(&g, [0, 1, 0], &p);
    }

    #[test]
    fn climbs_steps_but_not_cliffs_and_drops_at_most_three() {
        // A staircase up: 1,2,3,4.
        let g = grid(&["1234"]);
        let p = find_path(&g, [0, 1, 0], [3, 4, 0], 200).unwrap();
        assert_eq!(p, [[1, 2, 0], [2, 3, 0], [3, 4, 0]]);
        // A two-block cliff cannot be climbed...
        let g = grid(&["1133"]);
        let p = find_path(&g, [0, 1, 0], [3, 3, 0], 200).unwrap();
        assert_eq!(*p.last().unwrap(), [1, 1, 0], "should stop at the foot of the cliff");
        // ...but can be walked down.
        let p = find_path(&g, [3, 3, 0], [0, 1, 0], 200).unwrap();
        assert_eq!(*p.last().unwrap(), [0, 1, 0]);
        // Four blocks is too far to drop.
        let g = grid(&["5511"]);
        assert_eq!(find_path(&g, [0, 5, 0], [3, 1, 0], 200).unwrap().last(), Some(&[1, 5, 0]));
    }

    #[test]
    fn unreachable_goals_and_bad_starts() {
        let g = grid(&["11#11"]);
        // Walled off: go as near as possible.
        let p = find_path(&g, [0, 1, 0], [4, 1, 0], 200).unwrap();
        assert_eq!(*p.last().unwrap(), [1, 1, 0]);
        // Already as near as it gets.
        assert!(find_path(&g, [1, 1, 0], [4, 1, 0], 200).is_none());
        // Starting inside a wall or in mid-air.
        assert!(find_path(&g, [2, 1, 0], [0, 1, 0], 200).is_none());
        assert!(find_path(&g, [0, 5, 0], [1, 1, 0], 200).is_none());
    }

    #[test]
    fn the_node_budget_is_respected_and_still_gives_progress() {
        let g = |_: i32, y: i32, _: i32| y < 1; // endless flat ground
        let p = find_path(&g, [0, 1, 0], [500, 1, 0], 60).unwrap();
        assert!(p.len() < 60);
        assert!(p.last().unwrap()[0] > 10);
        assert_walkable(&g, [0, 1, 0], &p);
    }

    #[test]
    fn finds_ground_near_a_height() {
        let g = grid(&["3"]);
        assert_eq!(ground_near(&g, 0, 1, 0, 4), Some([0, 3, 0]));
        assert_eq!(ground_near(&g, 0, 9, 0, 2), None);
    }
}
