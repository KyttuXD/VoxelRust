//! Where the hearts of the health meter go. Ten hearts make a row (20
//! health); more health wraps into more rows above, up to 100 health (five
//! rows). Absorption (the yellow hearts) is a row of its own above all of
//! them, as before.

use alloc::vec::Vec;

pub const PER_ROW: usize = 10;
/// The most health the meter can show.
pub const MOST: i32 = 100;

/// One heart: its column (0 is the left), its row (0 is the bottom one) and
/// how full it is (0 empty, 1 half, 2 full).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub col: u8,
    pub row: u8,
    pub fill: u8,
}

/// How many hearts a maximum health has (a half heart counts as one).
pub fn count(max_health: i32) -> usize {
    ((max_health.clamp(2, MOST) + 1) / 2) as usize
}

/// How many rows of hearts a maximum health fills.
pub fn rows(max_health: i32) -> usize {
    (count(max_health) + PER_ROW - 1) / PER_ROW
}

/// Every heart of the meter, left to right, bottom row first.
pub fn layout(health: i32, max_health: i32) -> Vec<Cell> {
    (0..count(max_health))
        .map(|i| Cell { col: (i % PER_ROW) as u8, row: (i / PER_ROW) as u8, fill: (health - i as i32 * 2).clamp(0, 2) as u8 })
        .collect()
}

/// The yellow hearts for `absorb` points: a single row above the `rows`
/// rows of health, at most ten hearts. `fill` is 2 or 1 as for health.
pub fn absorption(absorb: i32, rows: usize) -> Vec<Cell> {
    let n = (((absorb.max(0) + 1) / 2) as usize).min(PER_ROW);
    (0..n).map(|i| Cell { col: i as u8, row: rows as u8, fill: (absorb - i as i32 * 2).clamp(0, 2) as u8 }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twenty_health_is_ten_hearts_in_one_row() {
        let l = layout(20, 20);
        assert_eq!(l.len(), 10);
        assert!(l.iter().all(|c| c.row == 0 && c.fill == 2));
        assert_eq!(rows(20), 1);
        assert_eq!(l[9].col, 9);
    }

    #[test]
    fn more_health_wraps_into_rows_of_ten() {
        let l = layout(37, 60);
        assert_eq!(l.len(), 30);
        assert_eq!(rows(60), 3);
        assert_eq!((l[10].col, l[10].row), (0, 1));
        assert_eq!((l[29].col, l[29].row), (9, 2));
        // 37 health: 18 full hearts, one half, the rest empty.
        assert_eq!(l.iter().filter(|c| c.fill == 2).count(), 18);
        assert_eq!(l[18].fill, 1);
        assert_eq!(l[19].fill, 0);
        assert_eq!(rows(21), 2);
    }

    #[test]
    fn the_meter_tops_out_at_a_hundred() {
        assert_eq!(count(1000), 50);
        assert_eq!(rows(1000), 5);
        assert_eq!(layout(1000, 1000).len(), 50);
        assert_eq!(count(0), 1);
        assert_eq!(count(-5), 1);
    }

    #[test]
    fn absorption_is_one_yellow_row_above_all_the_health_rows() {
        let a = absorption(5, 3);
        assert_eq!(a.len(), 3);
        assert!(a.iter().all(|c| c.row == 3));
        assert_eq!((a[0].fill, a[1].fill, a[2].fill), (2, 2, 1));
        assert_eq!(absorption(0, 1).len(), 0);
        assert_eq!(absorption(500, 1).len(), 10);
    }
}
