//! Painting the pages of the HD atlas.
//!
//! The atlas is built in two passes over the tiles (see `textures::build_atlas_and_hd`):
//! first only the sizes of the tiles' pictures are found out, and [`Builder::new`]
//! makes the plan from them; then each picture is read once more and put on its
//! page with [`Builder::add`], and let go. Only the pages are held, never
//! all the decoded pictures at once. [`Builder::finish`] gives the pages to the
//! renderer and installs the table the mesher and the renderer look places up in.

use super::plan::{self, Place, Plan};
use super::scale;
use super::table::{self, Table};
use alloc::vec::Vec;

/// One page of the atlas, ready for the card: its first level.
pub struct PageImage {
    pub w: usize,
    pub h: usize,
    /// Mip levels below the first (see `plan::levels_for`).
    pub levels: usize,
    pub rgba: Vec<u8>,
}

/// What the build made.
pub struct Built {
    pub pages: Vec<PageImage>,
    /// What the pages cost on the card, mip levels included.
    pub bytes: usize,
    /// Tiles in the atlas.
    pub tiles: usize,
    /// The cap the pictures ended up at.
    pub cap: usize,
}

impl Built {
    pub fn none() -> Built {
        Built { pages: Vec::new(), bytes: 0, tiles: 0, cap: super::cap() }
    }
}

pub struct Builder {
    plan: Plan,
    pages: Vec<PageImage>,
    placed: Vec<Option<Place>>,
}

impl Builder {
    /// Plan the atlas for tiles whose pictures are `natural[i]` pixels on a side
    /// (16 for a normal tile), with the TEXTURE SIZE, the card's limit and the
    /// memory budget as they are now.
    pub fn new(natural: &[usize]) -> Builder {
        Builder::with_limits(natural, super::cap(), super::max_texture(), super::byte_limit())
    }

    pub fn with_limits(natural: &[usize], cap: usize, max_tex: usize, max_bytes: usize) -> Builder {
        let plan = plan::plan(natural, cap, max_tex, max_bytes);
        let pages = plan.pages.iter().map(|g| PageImage { w: g.w as usize, h: g.h as usize, levels: g.levels as usize, rgba: Vec::new() }).collect();
        let placed = alloc::vec![None; natural.len()];
        Builder { plan, pages, placed }
    }

    /// The side tile `i` is to be kept at in the atlas, if it is in it.
    pub fn planned_side(&self, i: usize) -> Option<usize> {
        self.plan.place.get(i).copied().flatten().map(|p| p.side as usize)
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Put the picture of tile `i` (`natural` x `natural` RGBA) on its page, shrunk to the
    /// planned side if need be. A tile that is not in the plan, or a picture of another
    /// size than planned, is left out (the tile is then its 16 x 16 in the normal atlas).
    pub fn add(&mut self, i: usize, natural: usize, px: &[u8]) -> bool {
        let Some(place) = self.plan.place.get(i).copied().flatten() else { return false };
        if natural == 0 || px.len() != natural * natural * 4 || (place.side as usize) > natural {
            return false;
        }
        let page = &mut self.pages[place.page as usize];
        if page.rgba.is_empty() {
            page.rgba = alloc::vec![0u8; page.w * page.h * 4];
        }
        let side = place.side as usize;
        if side == natural {
            scale::blit(&mut page.rgba, page.w, place.x as usize, place.y as usize, px, side);
        } else {
            scale::blit(&mut page.rgba, page.w, place.x as usize, place.y as usize, &scale::box_down(px, natural, side), side);
        }
        self.placed[i] = Some(place);
        true
    }

    /// Install the table of the tiles that were added and hand over the pages.
    pub fn finish(self) -> Built {
        self.finish_with(true)
    }

    /// [`Builder::finish`], with the table installed only if `install` (the shared table is
    /// left alone otherwise: for tests and for building without drawing).
    pub fn finish_with(self, install: bool) -> Built {
        let Builder { plan, pages, placed } = self;
        let tiles = placed.iter().flatten().count();
        if tiles == 0 {
            if install {
                table::install(Table { place: Vec::new(), pages: Vec::new(), bytes: 0, cap: plan.cap });
            }
            return Built { cap: plan.cap, ..Built::none() };
        }
        let bytes = plan.bytes();
        if install {
            table::install(Table { place: placed, pages: plan.pages.clone(), bytes, cap: plan.cap });
        }
        // A page that nothing was painted on is kept as clear pixels.
        let pages = pages.into_iter().map(|mut p| {
            if p.rgba.is_empty() {
                p.rgba = alloc::vec![0u8; p.w * p.h * 4];
            }
            p
        });
        Built { pages: pages.collect(), bytes, tiles, cap: plan.cap }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(side: usize, c: [u8; 4]) -> Vec<u8> {
        (0..side * side).flat_map(|_| c).collect()
    }

    #[test]
    fn pictures_land_on_their_pages_and_nowhere_else() {
        let natural = [16, 64, 16, 128];
        let mut b = Builder::with_limits(&natural, 512, 4096, 1 << 30);
        assert_eq!(b.planned_side(0), None);
        assert_eq!(b.planned_side(1), Some(64));
        assert_eq!(b.planned_side(3), Some(128));
        assert!(!b.add(0, 16, &solid(16, [1, 1, 1, 255])), "a normal tile is not added");
        assert!(b.add(1, 64, &solid(64, [255, 0, 0, 255])));
        assert!(b.add(3, 128, &solid(128, [0, 0, 255, 255])));
        let (p1, p3) = (b.plan().place[1].unwrap(), b.plan().place[3].unwrap());
        // (Not installed here: the table is shared with other tests, so look at the pages only.)
        let (w, built_pages) = (b.pages[p1.page as usize].w, b.pages);
        let red = &built_pages[p1.page as usize].rgba[(p1.y as usize * w + p1.x as usize) * 4..][..4];
        assert_eq!(red, &[255, 0, 0, 255]);
        let blue = &built_pages[p3.page as usize].rgba[((p3.y as usize + 127) * built_pages[p3.page as usize].w + p3.x as usize + 127) * 4..][..4];
        assert_eq!(blue, &[0, 0, 255, 255]);
    }

    #[test]
    fn a_picture_over_the_cap_is_shrunk_when_painted() {
        let mut b = Builder::with_limits(&[256], 64, 4096, 1 << 30);
        assert_eq!(b.planned_side(0), Some(64));
        assert!(b.add(0, 256, &solid(256, [10, 20, 30, 255])));
        let place = b.plan().place[0].unwrap();
        let page = &b.pages[0];
        assert_eq!((place.side, page.w, page.h), (64, 64, 64));
        assert!(page.rgba.chunks(4).all(|p| p == [10, 20, 30, 255]));
    }

    #[test]
    fn a_picture_of_another_size_than_planned_is_refused() {
        let mut b = Builder::with_limits(&[64], 512, 4096, 1 << 30);
        assert!(!b.add(0, 32, &solid(32, [1, 1, 1, 255])), "smaller than planned");
        assert!(!b.add(0, 64, &solid(32, [1, 1, 1, 255])), "pixels do not match the size");
        assert!(!b.add(5, 64, &solid(64, [1, 1, 1, 255])), "no such tile");
    }

    #[test]
    fn nothing_is_built_when_nothing_is_hd() {
        let b = Builder::with_limits(&[16, 16], 512, 4096, 1 << 30);
        assert!(b.plan().pages.is_empty());
    }
}
