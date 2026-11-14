//! The arithmetic of a scrolling list: clamping, the mouse wheel, keeping a
//! row in view and where the scroll indicator's thumb sits. Pure, so the
//! pages that scroll (key bindings, mod switches, mod screens) share it and
//! it is tested without a window.

/// The first row that may be at the top of a list of `total` rows of which
/// `shown` fit: never past the point where the last row is at the bottom.
pub fn max_pos(total: usize, shown: usize) -> i32 {
    total.saturating_sub(shown) as i32
}

/// `pos` brought into range.
pub fn clamp(pos: i32, total: usize, shown: usize) -> i32 {
    pos.clamp(0, max_pos(total, shown))
}

/// Move by the mouse wheel (`wheel` is positive when scrolled up, as the
/// game's input reports it), `per_notch` rows a notch, and clamp.
pub fn wheel(pos: i32, wheel: i32, per_notch: i32, total: usize, shown: usize) -> i32 {
    clamp(pos.saturating_sub(wheel.clamp(-20, 20) * per_notch), total, shown)
}

/// Scroll just far enough that row `row` is inside the `shown` rows.
pub fn reveal(pos: i32, row: usize, total: usize, shown: usize) -> i32 {
    let row = row as i32;
    let pos = if row < pos {
        row
    } else if row >= pos + shown as i32 {
        row + 1 - shown as i32
    } else {
        pos
    };
    clamp(pos, total, shown)
}

/// Where the scroll indicator's thumb goes in a track of `track` pixels:
/// (offset, length). The thumb is never shorter than `min_len`. `None`
/// when everything fits and there is nothing to show.
pub fn thumb(pos: i32, total: usize, shown: usize, track: f32, min_len: f32) -> Option<(f32, f32)> {
    if total <= shown || shown == 0 || track <= 0.0 {
        return None;
    }
    let len = (track * shown as f32 / total as f32).max(min_len).min(track);
    let max = max_pos(total, shown).max(1) as f32;
    let at = (track - len) * (clamp(pos, total, shown) as f32 / max);
    Some((at, len))
}

/// For a list whose entries take different numbers of rows (`units`), the
/// last entry that may be the first one shown when `room` rows are free:
/// the one from which everything to the end still fits (at least 0).
pub fn max_first(units: &[usize], room: usize) -> usize {
    let mut used = 0;
    let mut first = units.len();
    while first > 0 {
        let u = units[first - 1].max(1);
        if used + u > room {
            break;
        }
        used += u;
        first -= 1;
    }
    first.min(units.len().saturating_sub(1))
}

/// How many entries from `first` fit in `room` rows (at least one, if there is one).
pub fn fit(units: &[usize], first: usize, room: usize) -> usize {
    let mut used = 0;
    let mut n = 0;
    for u in units.iter().skip(first) {
        let u = (*u).max(1);
        if used + u > room && n > 0 {
            break;
        }
        used += u;
        n += 1;
        if used >= room {
            break;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolling_is_clamped_to_the_list() {
        assert_eq!(max_pos(20, 9), 11);
        assert_eq!(max_pos(5, 9), 0);
        assert_eq!(max_pos(0, 0), 0);
        assert_eq!(clamp(-3, 20, 9), 0);
        assert_eq!(clamp(50, 20, 9), 11);
        assert_eq!(clamp(4, 20, 9), 4);
        assert_eq!(clamp(7, 3, 9), 0);
    }

    #[test]
    fn the_wheel_moves_a_few_rows_and_stops_at_both_ends() {
        // Wheel up goes toward the top.
        assert_eq!(wheel(5, 1, 2, 20, 9), 3);
        assert_eq!(wheel(1, 1, 2, 20, 9), 0);
        assert_eq!(wheel(0, 3, 2, 20, 9), 0);
        assert_eq!(wheel(10, -1, 2, 20, 9), 11);
        assert_eq!(wheel(11, -5, 2, 20, 9), 11);
        // A list that fits never moves; a wild wheel value cannot overflow.
        assert_eq!(wheel(0, -1, 2, 5, 9), 0);
        assert_eq!(wheel(i32::MIN, i32::MAX, 2, 20, 9), 0);
        assert_eq!(wheel(i32::MAX, i32::MIN, 2, 20, 9), 11);
    }

    #[test]
    fn a_row_can_be_brought_into_view() {
        assert_eq!(reveal(0, 3, 20, 9), 0);
        assert_eq!(reveal(5, 2, 20, 9), 2);
        assert_eq!(reveal(0, 12, 20, 9), 4);
        assert_eq!(reveal(0, 19, 20, 9), 11);
        assert_eq!(reveal(0, 99, 20, 9), 11);
    }

    #[test]
    fn the_thumb_sits_in_the_track() {
        assert_eq!(thumb(0, 5, 9, 100.0, 8.0), None);
        let (at, len) = thumb(0, 18, 9, 90.0, 8.0).unwrap();
        assert_eq!((at, len), (0.0, 45.0));
        let (at, len) = thumb(9, 18, 9, 90.0, 8.0).unwrap();
        assert_eq!((at, len), (45.0, 45.0));
        // Halfway down the scroll is halfway down the free track.
        let (at, _) = thumb(5, 19, 9, 100.0, 8.0).unwrap();
        assert!(at > 0.0 && at < 100.0 - 9.0 * 100.0 / 19.0);
        // A very long list still gets a thumb you can see.
        let (_, len) = thumb(0, 10000, 9, 90.0, 8.0).unwrap();
        assert_eq!(len, 8.0);
        // An out of range position is clamped.
        let (at, len) = thumb(1000, 18, 9, 90.0, 8.0).unwrap();
        assert_eq!(at + len, 90.0);
    }

    #[test]
    fn mixed_height_entries_fit_by_rows() {
        // A text (1), a list (4), a button (1), a button (1).
        let u = [1, 4, 1, 1];
        assert_eq!(max_first(&u, 10), 0);
        assert_eq!(max_first(&u, 7), 0);
        assert_eq!(max_first(&u, 6), 1);
        assert_eq!(max_first(&u, 5), 2);
        assert_eq!(max_first(&u, 2), 2);
        assert_eq!(max_first(&u, 1), 3);
        assert_eq!(max_first(&u, 0), 3);
        assert_eq!(max_first(&[], 5), 0);
        assert_eq!(fit(&u, 0, 6), 3);
        assert_eq!(fit(&u, 1, 4), 1);
        assert_eq!(fit(&u, 1, 2), 1, "one entry is always shown, even if it does not fit");
        assert_eq!(fit(&u, 4, 5), 0);
        assert_eq!(fit(&u, 2, 5), 2);
    }
}
