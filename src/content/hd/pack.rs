//! The shelf packer: places square pictures of different sizes in one or more
//! pages (textures), each at most `max_side` on a side.
//!
//! The pictures are put in order of size, biggest first (ties by their
//! number), in rows ("shelves") as high as the first picture in them. It is
//! simple, always gives the same answer for the same input, and wastes
//! little when the sizes are powers of two. Every place is a multiple of 16
//! pixels from the page's corner when the sizes are multiples of 16, which
//! keeps the first mip levels of neighbouring tiles apart.

use alloc::vec::Vec;

/// Where a picture went.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Slot {
    pub page: usize,
    pub x: usize,
    pub y: usize,
}

/// The result: for each input, its slot (`None` if its size was 0 or it did
/// not fit), and the size of each page (powers of two, wide and high).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Packed {
    pub slots: Vec<Option<Slot>>,
    pub pages: Vec<(usize, usize)>,
}

impl Packed {
    /// Whether every picture of a non-zero size was placed.
    pub fn all_placed(&self, sides: &[usize]) -> bool {
        sides.iter().zip(&self.slots).all(|(s, slot)| *s == 0 || slot.is_some())
    }
}

/// The least power of two that is at least `n` (1 for 0).
pub fn pow2_ceil(n: usize) -> usize {
    let mut p = 1;
    while p < n {
        p *= 2;
    }
    p
}

/// The greatest power of two that is at most `n` (1 for 0).
pub fn pow2_floor(n: usize) -> usize {
    let mut p = 1;
    while p * 2 <= n {
        p *= 2;
    }
    p
}

fn isqrt(n: usize) -> usize {
    let mut r = 0usize;
    while (r + 1) * (r + 1) <= n {
        // (Steps of one are slow for big areas: grow by halving.)
        let mut step = 1usize;
        while (r + step * 2) * (r + step * 2) <= n {
            step *= 2;
        }
        r += step;
    }
    r
}

struct Shelf {
    y: usize,
    h: usize,
    x: usize,
}

struct Page {
    shelves: Vec<Shelf>,
    next_y: usize,
    used_x: usize,
}

/// Place pictures of the given sides (0 = none) in at most `max_pages` pages
/// of at most `max_side` on a side.
pub fn pack(sides: &[usize], max_side: usize, max_pages: usize) -> Packed {
    let max_side = pow2_floor(max_side.max(1));
    let mut out = Packed { slots: alloc::vec![None; sides.len()], pages: Vec::new() };
    let mut order: Vec<usize> = (0..sides.len()).filter(|&i| sides[i] > 0 && sides[i] <= max_side).collect();
    order.sort_by(|&a, &b| sides[b].cmp(&sides[a]).then(a.cmp(&b)));
    if order.is_empty() {
        return out;
    }
    // A page about as wide as it will be high for the area to place, at least as wide as the biggest picture.
    let area: usize = order.iter().map(|&i| sides[i] * sides[i]).sum();
    let root = isqrt(area);
    let width = pow2_ceil(root + root / 8).max(pow2_ceil(sides[order[0]])).min(max_side);
    let mut pages: Vec<Page> = Vec::new();
    for &i in &order {
        let s = sides[i];
        let mut at: Option<(usize, usize, usize)> = None;
        'find: for (pi, pg) in pages.iter_mut().enumerate() {
            for sh in pg.shelves.iter_mut() {
                if sh.h >= s && sh.x + s <= width {
                    at = Some((pi, sh.x, sh.y));
                    sh.x += s;
                    break 'find;
                }
            }
            if pg.next_y + s <= max_side {
                let y = pg.next_y;
                pg.shelves.push(Shelf { y, h: s, x: s });
                pg.next_y += s;
                at = Some((pi, 0, y));
                break 'find;
            }
        }
        if at.is_none() && pages.len() < max_pages {
            pages.push(Page { shelves: alloc::vec![Shelf { y: 0, h: s, x: s }], next_y: s, used_x: 0 });
            at = Some((pages.len() - 1, 0, 0));
        }
        if let Some((pi, x, y)) = at {
            pages[pi].used_x = pages[pi].used_x.max(x + s);
            out.slots[i] = Some(Slot { page: pi, x, y });
        }
    }
    out.pages = pages.iter().map(|p| (pow2_ceil(p.used_x).min(max_side), pow2_ceil(p.next_y).min(max_side))).collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlap(a: (Slot, usize), b: (Slot, usize)) -> bool {
        a.0.page == b.0.page && a.0.x < b.0.x + b.1 && b.0.x < a.0.x + a.1 && a.0.y < b.0.y + b.1 && b.0.y < a.0.y + a.1
    }

    fn check(sides: &[usize], packed: &Packed, max_side: usize) {
        let placed: Vec<(Slot, usize)> = packed.slots.iter().zip(sides).filter_map(|(s, side)| s.map(|s| (s, *side))).collect();
        for (i, a) in placed.iter().enumerate() {
            let (w, h) = packed.pages[a.0.page];
            assert!(a.0.x + a.1 <= w && a.0.y + a.1 <= h, "{a:?} outside its page {w}x{h}");
            assert!(w <= max_side && h <= max_side, "page {w}x{h} over {max_side}");
            assert!(w.is_power_of_two() && h.is_power_of_two());
            for b in &placed[i + 1..] {
                assert!(!overlap(*a, *b), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn nothing_to_pack_is_nothing() {
        let p = pack(&[], 4096, 4);
        assert!(p.pages.is_empty() && p.slots.is_empty());
        let p = pack(&[0, 0], 4096, 4);
        assert!(p.pages.is_empty());
        assert_eq!(p.slots, alloc::vec![None, None]);
    }

    #[test]
    fn one_picture_gets_a_page_of_its_size() {
        let p = pack(&[64], 4096, 4);
        assert_eq!(p.slots[0], Some(Slot { page: 0, x: 0, y: 0 }));
        assert_eq!(p.pages, alloc::vec![(64, 64)]);
    }

    #[test]
    fn mixed_sizes_never_overlap_and_stay_in_the_page() {
        let mut sides = Vec::new();
        for i in 0..300usize {
            sides.push([16, 32, 48, 64, 128, 256, 512, 80][(i * 7 + i / 3) % 8]);
        }
        let p = pack(&sides, 4096, 4);
        assert!(p.all_placed(&sides));
        check(&sides, &p, 4096);
        // Every place is a multiple of 16 (the first four mip levels never mix tiles).
        for s in p.slots.iter().flatten() {
            assert!(s.x % 16 == 0 && s.y % 16 == 0, "{s:?}");
        }
    }

    #[test]
    fn it_is_deterministic() {
        let sides: Vec<usize> = (0..200).map(|i| 16 * (1 + (i * 5 + i / 7) % 16)).collect();
        assert_eq!(pack(&sides, 2048, 4), pack(&sides, 2048, 4));
    }

    #[test]
    fn the_biggest_go_first_and_ties_keep_their_order() {
        let p = pack(&[32, 64, 64, 32], 1024, 2);
        let (a, b) = (p.slots[1].unwrap(), p.slots[2].unwrap());
        assert_eq!((a.x, a.y), (0, 0));
        assert!(b.x == 64 || b.y == 64, "{b:?}");
    }

    #[test]
    fn a_page_never_gets_bigger_than_the_cards_limit() {
        // 100 tiles of 512 = 25 million pixels: 2 pages of 2048 x 2048 hold 64, so not all fit in 2.
        let sides = alloc::vec![512usize; 100];
        let p = pack(&sides, 2048, 2);
        check(&sides, &p, 2048);
        assert_eq!(p.pages.len(), 2);
        assert_eq!(p.slots.iter().flatten().count(), 32, "two pages of 4 x 4 tiles");
        assert!(!p.all_placed(&sides));
        // With room for more pages all fit.
        let p = pack(&sides, 2048, 8);
        assert!(p.all_placed(&sides));
        check(&sides, &p, 2048);
    }

    #[test]
    fn a_picture_bigger_than_a_page_is_not_placed() {
        let p = pack(&[512, 64], 256, 4);
        assert_eq!(p.slots[0], None);
        assert!(p.slots[1].is_some());
    }

    #[test]
    fn a_non_power_of_two_limit_is_rounded_down() {
        let p = pack(&[256; 20], 3000, 4);
        check(&[256; 20], &p, 2048);
    }

    #[test]
    fn about_nine_hundred_hd_tiles_of_128_fit_in_two_big_pages() {
        let sides = alloc::vec![128usize; 900];
        let p = pack(&sides, 4096, 4);
        assert!(p.all_placed(&sides));
        check(&sides, &p, 4096);
        // 900 * 128 * 128 pixels = 56 MB of pixels: no more than 2 pages of 4096 x 4096 (64 MB each).
        assert!(p.pages.len() <= 2, "{:?}", p.pages);
    }

    #[test]
    fn the_integer_root_is_exact() {
        for n in [0usize, 1, 3, 4, 15, 16, 17, 1_000_000, 16_777_216, 99_999_999] {
            let r = isqrt(n);
            assert!(r * r <= n && (r + 1) * (r + 1) > n, "{n} -> {r}");
        }
    }
}
