//! Where each HD tile is, for whoever draws: the mesher (any thread) and the renderer.
//!
//! The table is replaced whenever the atlas is built again. The old one is
//! kept for one more build before it is freed (see `textures::swap_retiring`):
//! a mesh job that took a reference a moment ago can finish with it.

use super::plan::{PageSpec, Place};
use alloc::vec::Vec;
use core::sync::atomic::AtomicPtr;

pub struct Table {
    /// For each tile of the registry.
    pub place: Vec<Option<Place>>,
    pub pages: Vec<PageSpec>,
    /// What the pages cost on the card, mip levels included.
    pub bytes: usize,
    /// The cap in force when the table was made.
    pub cap: usize,
}

impl Table {
    pub fn get(&self, tile: usize) -> Option<Place> {
        self.place.get(tile).copied().flatten()
    }

    /// The texture coordinates of a tile's picture in its page: (page, u0, v0, u1, v1),
    /// a quarter pixel in from each edge so that a neighbour never shows.
    pub fn uv(&self, tile: usize) -> Option<(usize, [f32; 4])> {
        let p = self.get(tile)?;
        let g = self.pages.get(p.page as usize)?;
        let (w, h) = (g.w as f32, g.h as f32);
        let (x, y, s) = (p.x as f32, p.y as f32, p.side as f32);
        Some((p.page as usize, [(x + 0.25) / w, (y + 0.25) / h, (x + s - 0.25) / w, (y + s - 0.25) / h]))
    }
}

static TABLE: AtomicPtr<Table> = AtomicPtr::new(core::ptr::null_mut());
static TABLE_OLD: AtomicPtr<Table> = AtomicPtr::new(core::ptr::null_mut());

/// Make `t` the table in use.
pub fn install(t: Table) {
    let p = if t.pages.is_empty() { core::ptr::null_mut() } else { alloc::boxed::Box::into_raw(alloc::boxed::Box::new(t)) };
    crate::textures::swap_retiring(&TABLE, &TABLE_OLD, p);
}

/// The table in use, if any tile is HD.
pub fn current() -> Option<&'static Table> {
    let p = TABLE.load(core::sync::atomic::Ordering::Acquire);
    // SAFETY: from a boxed table, freed only two installs after it was replaced.
    if p.is_null() { None } else { Some(unsafe { &*p }) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Table {
        let place = alloc::vec![None, Some(Place { page: 0, x: 64, y: 0, side: 128 }), None, Some(Place { page: 1, x: 0, y: 256, side: 64 })];
        let pages = alloc::vec![PageSpec { w: 512, h: 256, levels: 4 }, PageSpec { w: 512, h: 512, levels: 4 }];
        Table { place, pages, bytes: 0, cap: 512 }
    }

    #[test]
    fn a_tile_has_a_place_or_none() {
        let t = table();
        assert_eq!(t.get(0), None);
        assert_eq!(t.get(1).unwrap().side, 128);
        assert_eq!(t.get(99), None);
    }

    #[test]
    fn coordinates_are_inside_the_picture_and_in_the_pages_own_units() {
        let t = table();
        let (page, uv) = t.uv(1).unwrap();
        assert_eq!(page, 0);
        assert!((uv[0] - 64.25 / 512.0).abs() < 1e-6 && (uv[2] - 191.75 / 512.0).abs() < 1e-6);
        assert!((uv[1] - 0.25 / 256.0).abs() < 1e-6 && (uv[3] - 127.75 / 256.0).abs() < 1e-6);
        let (page, uv) = t.uv(3).unwrap();
        assert_eq!(page, 1);
        assert!(uv[1] > 0.5 && uv[3] < 0.8125);
        assert!(t.uv(0).is_none() && t.uv(2).is_none());
    }
}
