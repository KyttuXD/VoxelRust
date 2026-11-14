//! Deciding the HD atlas: which tiles are in it, at what size, and where.
//!
//! The plan starts from the TEXTURE SIZE cap. If the pictures at that size
//! do not fit the pages the card allows (at most [`MAX_PAGES`], each at most
//! the card's limit or 4096 on a side) or cost more than the memory budget
//! lets the atlas have, the cap is lowered a step (512, 256, 128, 64, 32) until they do.
//! Below 32 nothing is HD: every tile is its 16 x 16 in the normal atlas.

use super::pack;
use alloc::vec::Vec;

/// Most pages (textures) of the HD atlas.
pub const MAX_PAGES: usize = 4;
/// What the atlas may cost (card and main memory, mip levels included) unless the
/// game says otherwise: four pages of 4096 x 4096.
pub const DEFAULT_BYTE_LIMIT: usize = 360 << 20;

/// Where a tile's picture is in the HD atlas. Sides and places are in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub page: u8,
    pub x: u16,
    pub y: u16,
    pub side: u16,
}

/// A page: its size and how many mip levels below the first it has (see [`levels_for`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSpec {
    pub w: u16,
    pub h: u16,
    pub levels: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// For each tile of the registry: its place, if it is in the HD atlas.
    pub place: Vec<Option<Place>>,
    pub pages: Vec<PageSpec>,
    /// The cap the plan ended with (lower than the setting if the pictures did not fit).
    pub cap: usize,
}

impl Plan {
    pub fn empty(tiles: usize, cap: usize) -> Plan {
        Plan { place: alloc::vec![None; tiles], pages: Vec::new(), cap }
    }

    pub fn bytes(&self) -> usize {
        bytes_of(&self.pages)
    }

    pub fn tiles(&self) -> usize {
        self.place.iter().flatten().count()
    }
}

/// What pages cost on the card: four bytes a pixel, and a third more for the mip levels.
pub fn bytes_of(pages: &[PageSpec]) -> usize {
    pages.iter().fold(0usize, |a, p| a.saturating_add(p.w as usize * p.h as usize * 4 * 4 / 3))
}

/// How many mip levels below the first a page may have so that no tile's
/// picture mixes with its neighbour's: the number of times every tile's place
/// and size halves evenly (at least 4 when everything is a multiple of 16).
pub fn levels_for(w: usize, h: usize, tiles: &[(usize, usize, usize)]) -> u8 {
    let mut lv = 8u32;
    for &(x, y, side) in tiles {
        lv = lv.min((x | y | side).trailing_zeros());
    }
    lv = lv.min(w.min(h).trailing_zeros());
    lv as u8
}

/// Plan the atlas for tiles whose own pictures are `natural` pixels on a side
/// (16 or less: the tile is not HD). `cap` is the TEXTURE SIZE, `max_tex` the
/// card's biggest texture and `max_bytes` what the atlas may cost.
pub fn plan(natural: &[usize], cap: usize, max_tex: usize, max_bytes: usize) -> Plan {
    let mut cap = super::size::cap_for(cap as i32);
    let max_tex = max_tex.min(4096);
    while cap >= 32 {
        let sides: Vec<usize> = natural.iter().map(|&n| if n > 16 { super::size::effective_side(n, cap) } else { 0 }).collect();
        if sides.iter().all(|s| *s == 0) {
            return Plan::empty(natural.len(), cap);
        }
        let packed = pack::pack(&sides, max_tex, MAX_PAGES);
        if packed.all_placed(&sides) {
            let mut pages: Vec<PageSpec> = packed.pages.iter().map(|&(w, h)| PageSpec { w: w as u16, h: h as u16, levels: 0 }).collect();
            let mut on_page: Vec<Vec<(usize, usize, usize)>> = alloc::vec![Vec::new(); pages.len()];
            let mut place = alloc::vec![None; natural.len()];
            for (i, slot) in packed.slots.iter().enumerate() {
                if let Some(s) = slot {
                    place[i] = Some(Place { page: s.page as u8, x: s.x as u16, y: s.y as u16, side: sides[i] as u16 });
                    on_page[s.page].push((s.x, s.y, sides[i]));
                }
            }
            for (p, tiles) in pages.iter_mut().zip(&on_page) {
                p.levels = levels_for(p.w as usize, p.h as usize, tiles);
            }
            if bytes_of(&pages) <= max_bytes {
                return Plan { place, pages, cap };
            }
        }
        cap /= 2;
    }
    Plan::empty(natural.len(), cap.max(16))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_no_hd_tile_there_is_no_page() {
        let p = plan(&[16, 16, 0, 16], 512, 4096, DEFAULT_BYTE_LIMIT);
        assert!(p.pages.is_empty());
        assert_eq!(p.tiles(), 0);
        assert_eq!(p.place.len(), 4);
    }

    #[test]
    fn a_few_hd_tiles_get_a_small_page_and_normal_ones_stay_out() {
        let p = plan(&[16, 64, 128, 16, 512], 512, 4096, DEFAULT_BYTE_LIMIT);
        assert_eq!(p.cap, 512);
        assert_eq!(p.tiles(), 3);
        assert!(p.place[0].is_none() && p.place[3].is_none());
        assert_eq!(p.place[4].unwrap().side, 512);
        assert_eq!(p.pages.len(), 1);
        assert!(p.pages[0].w >= 512 && p.pages[0].w <= 1024 && p.pages[0].h <= 1024, "{:?}", p.pages);
        assert!(p.pages[0].levels >= 4);
        // No overlap between the three.
        let t: Vec<Place> = p.place.iter().flatten().copied().collect();
        for (i, a) in t.iter().enumerate() {
            for b in &t[i + 1..] {
                let apart = a.x + a.side <= b.x || b.x + b.side <= a.x || a.y + a.side <= b.y || b.y + b.side <= a.y;
                assert!(apart, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn the_cap_shrinks_the_tiles() {
        let p = plan(&[512, 256, 64, 48], 128, 4096, DEFAULT_BYTE_LIMIT);
        assert_eq!(p.cap, 128);
        let sides: Vec<u16> = p.place.iter().map(|q| q.unwrap().side).collect();
        assert_eq!(sides, alloc::vec![128, 128, 64, 48]);
    }

    #[test]
    fn a_cap_of_16_means_no_hd_at_all() {
        let p = plan(&[512, 256, 64], 16, 4096, DEFAULT_BYTE_LIMIT);
        assert!(p.pages.is_empty() && p.tiles() == 0);
    }

    #[test]
    fn too_many_big_tiles_for_the_pages_lower_the_cap() {
        // 900 tiles of 512: 4 pages of 4096 x 4096 hold 256 of them, so 256 pixels (1024 fit).
        let p = plan(&alloc::vec![512; 900], 512, 4096, 1 << 30);
        assert_eq!(p.cap, 256);
        assert_eq!(p.tiles(), 900);
        assert!(p.pages.len() <= MAX_PAGES);
        // A card that takes only 2048 holds a quarter as much: 128 pixels.
        let p = plan(&alloc::vec![512; 900], 512, 2048, 1 << 30);
        assert_eq!(p.cap, 128);
        assert_eq!(p.tiles(), 900);
        assert!(p.pages.iter().all(|g| g.w <= 2048 && g.h <= 2048));
    }

    #[test]
    fn a_small_memory_budget_lowers_the_cap_until_it_fits() {
        let natural = alloc::vec![512usize; 100];
        let roomy = plan(&natural, 512, 4096, 1 << 30);
        let tight = plan(&natural, 512, 4096, 40 << 20);
        assert!(roomy.cap > tight.cap, "{} vs {}", roomy.cap, tight.cap);
        assert!(tight.bytes() <= 40 << 20);
        assert_eq!(tight.tiles(), 100);
        // A budget too small for anything leaves none.
        let none = plan(&natural, 512, 4096, 1000);
        assert_eq!(none.tiles(), 0);
        assert!(none.pages.is_empty());
    }

    #[test]
    fn mip_levels_stop_where_tiles_would_blend() {
        // Everything on multiples of 16: four levels at least.
        assert_eq!(levels_for(256, 256, &[(0, 0, 64), (64, 0, 64), (0, 64, 128)]), 6);
        assert_eq!(levels_for(256, 256, &[(0, 0, 16), (16, 0, 16)]), 4);
        // A tile 48 wide at x = 48: only 4 (16 divides, 32 does not).
        assert_eq!(levels_for(256, 256, &[(0, 0, 48), (48, 0, 48)]), 4);
        // A page only 16 high has no more.
        assert_eq!(levels_for(256, 16, &[(0, 0, 16)]), 4);
        // Never more than 8.
        assert_eq!(levels_for(4096, 4096, &[(0, 0, 512)]), 8);
    }

    #[test]
    fn the_plan_is_the_same_each_time() {
        let natural: Vec<usize> = (0..300).map(|i| [16, 32, 64, 128, 512][i % 5]).collect();
        assert_eq!(plan(&natural, 512, 4096, DEFAULT_BYTE_LIMIT), plan(&natural, 512, 4096, DEFAULT_BYTE_LIMIT));
    }

    #[test]
    fn page_costs_include_the_mip_levels() {
        let one = [PageSpec { w: 1024, h: 1024, levels: 4 }];
        assert_eq!(bytes_of(&one), 1024 * 1024 * 4 * 4 / 3);
        assert_eq!(bytes_of(&[]), 0);
    }
}
