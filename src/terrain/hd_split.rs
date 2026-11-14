//! Moving the faces of HD tiles out of a chunk's mesh.
//!
//! The mesher (`mesh.rs`) knows nothing of HD: every face gets the texture
//! coordinates of its tile's 16 x 16 picture in the normal atlas. Afterwards
//! [`split`] takes the quads whose tile is in the HD atlas (`content/hd`) out
//! of the opaque list and puts them in a list of their own, sorted by page,
//! with coordinates in that page's own units: quarter pixels, so that a page
//! of up to 4096 pixels fits a 16-bit coordinate. The renderer draws each
//! page's run with the page's texture; nothing else about the mesh changes.
//!
//! With no HD tile at all (nearly always) the cost is one look at a pointer.

use crate::hd::plan::MAX_PAGES;
use crate::hd::table::{self, Table};
use crate::mesh::Vertex;
use alloc::vec::Vec;

/// Texture coordinate units in an HD page: this many to the pixel.
pub const UNITS: i32 = 4;

/// The HD faces of a mesh: vertices, four to a quad, grouped by page.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct HdMesh {
    pub verts: Vec<Vertex>,
    /// How many vertices are on each page (the groups follow each other in `verts`).
    pub runs: [u32; MAX_PAGES],
}

impl HdMesh {
    pub fn is_empty(&self) -> bool {
        self.verts.is_empty()
    }
}

/// The tile a quad of the normal atlas shows, from the texture coordinates of one of its corners.
/// (Coordinates are 1/16 pixel: 256 to a tile, 32 tiles to a row.)
pub fn tile_of(uv: [i16; 2]) -> usize {
    let n = crate::textures::PER_ROW as i32;
    let (u, v) = (uv[0] as i32, uv[1] as i32);
    (v.div_euclid(256) * n + u.div_euclid(256)) as usize
}

/// Take the quads of HD tiles out of `opaque` (see the module's comment).
pub fn split(opaque: &mut Vec<Vertex>) -> HdMesh {
    match table::current() {
        Some(t) => split_with(opaque, t),
        None => HdMesh::default(),
    }
}

/// [`split`] with a given table.
pub fn split_with(opaque: &mut Vec<Vertex>, t: &Table) -> HdMesh {
    // Which quads are HD: a first look, so that a mesh without any is not copied.
    let any = opaque.chunks_exact(4).any(|q| t.get(tile_of(q[0].uv)).is_some());
    if !any {
        return HdMesh::default();
    }
    let mut keep: Vec<Vertex> = Vec::with_capacity(opaque.len());
    let mut pages: [Vec<Vertex>; MAX_PAGES] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for q in opaque.chunks_exact(4) {
        let place = t.get(tile_of(q[0].uv)).filter(|p| (p.page as usize) < MAX_PAGES);
        match place {
            None => keep.extend_from_slice(q),
            Some(p) => {
                for v in q {
                    // The place inside the tile, 1..=255 of 256 (the mesher keeps off the very edge),
                    // stretched over the HD picture from a quarter pixel in to a quarter pixel in.
                    let (lu, lv) = ((v.uv[0] as i32).rem_euclid(256).clamp(1, 255), (v.uv[1] as i32).rem_euclid(256).clamp(1, 255));
                    let span = p.side as i32 * UNITS - 2;
                    let mut w = *v;
                    w.uv = [(p.x as i32 * UNITS + 1 + (lu - 1) * span / 254) as i16, (p.y as i32 * UNITS + 1 + (lv - 1) * span / 254) as i16];
                    pages[p.page as usize].push(w);
                }
            }
        }
    }
    // (A leftover of fewer than four vertices would be a mesher error; it is kept as it was.)
    keep.extend_from_slice(opaque.chunks_exact(4).remainder());
    *opaque = keep;
    let mut out = HdMesh::default();
    for (i, list) in pages.iter().enumerate() {
        out.runs[i] = list.len() as u32;
        out.verts.extend_from_slice(list);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hd::plan::{PageSpec, Place};

    fn quad(tile: u16, u: [i32; 2], v: [i32; 2]) -> [Vertex; 4] {
        let n = crate::textures::PER_ROW as i32;
        let at = |pu: i32, pv: i32| Vertex { pos: [0; 4], col: [255; 4], uv: [((tile as i32 % n) * 256 + pu) as i16, ((tile as i32 / n) * 256 + pv) as i16], luv: [8, 8] };
        [at(u[0], v[0]), at(u[1], v[0]), at(u[1], v[1]), at(u[0], v[1])]
    }

    fn table() -> Table {
        let mut place = alloc::vec![None; 40];
        place[3] = Some(Place { page: 0, x: 0, y: 0, side: 128 });
        place[35] = Some(Place { page: 1, x: 256, y: 512, side: 64 });
        let pages = alloc::vec![PageSpec { w: 1024, h: 1024, levels: 4 }, PageSpec { w: 512, h: 1024, levels: 4 }];
        Table { place, pages, bytes: 0, cap: 512 }
    }

    #[test]
    fn tiles_are_found_from_their_coordinates() {
        for tile in [0u16, 1, 31, 32, 35, 900, 1023] {
            let q = quad(tile, [1, 255], [1, 255]);
            for v in q {
                assert_eq!(tile_of(v.uv), tile as usize, "{tile}");
            }
        }
    }

    #[test]
    fn a_mesh_without_hd_faces_is_left_alone() {
        let t = table();
        let mut v: Vec<Vertex> = quad(4, [1, 255], [1, 255]).to_vec();
        v.extend_from_slice(&quad(5, [1, 255], [1, 255]));
        let before = v.clone();
        let hd = split_with(&mut v, &t);
        assert!(hd.is_empty());
        assert_eq!(v, before);
    }

    #[test]
    fn hd_quads_move_to_their_page_with_coordinates_in_quarter_pixels() {
        let t = table();
        let mut v: Vec<Vertex> = Vec::new();
        v.extend_from_slice(&quad(4, [1, 255], [1, 255])); // normal
        v.extend_from_slice(&quad(3, [1, 255], [1, 255])); // HD, page 0, 128 pixels at (0, 0)
        v.extend_from_slice(&quad(35, [1, 255], [1, 255])); // HD, page 1, 64 pixels at (256, 512)
        v.extend_from_slice(&quad(3, [1, 255], [1, 255])); // HD, page 0 again
        let hd = split_with(&mut v, &t);
        assert_eq!(v.len(), 4, "only the normal quad stays");
        assert_eq!(hd.runs, [8, 4, 0, 0]);
        assert_eq!(hd.verts.len(), 12);
        // Page 0 first: the whole tile of 128 pixels is 512 units, used from 1 to 511 (a quarter pixel in).
        assert_eq!(hd.verts[0].uv, [1, 1]);
        assert_eq!(hd.verts[1].uv[0], 511);
        assert_eq!(hd.verts[2].uv, [511, 511]);
        // Page 1: starts at pixel 256 (1024 units) and 512 (2048 units).
        let p1 = &hd.verts[8..12];
        assert_eq!(p1[0].uv, [(256 * 4 + 1) as i16, (512 * 4 + 1) as i16]);
        assert_eq!(p1[2].uv, [(256 * 4 + 255) as i16, (512 * 4 + 255) as i16]);
        assert!(p1.iter().all(|v| (1024..1024 + 256).contains(&(v.uv[0] as i32)) && (2048..2048 + 256).contains(&(v.uv[1] as i32))));
        // The rest of a vertex is untouched.
        assert!(hd.verts.iter().all(|v| v.col == [255; 4] && v.luv == [8, 8]));
    }

    #[test]
    fn all_hd_faces_may_empty_the_opaque_list_and_order_inside_a_page_is_kept() {
        let t = table();
        let mut v: Vec<Vertex> = Vec::new();
        for i in 0..5 {
            let mut q = quad(3, [1, 255], [1, 255]);
            for x in q.iter_mut() {
                x.pos[0] = i;
            }
            v.extend_from_slice(&q);
        }
        let hd = split_with(&mut v, &t);
        assert!(v.is_empty());
        assert_eq!(hd.runs[0], 20);
        let order: Vec<i16> = hd.verts.chunks(4).map(|q| q[0].pos[0]).collect();
        assert_eq!(order, alloc::vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn the_biggest_coordinate_fits_sixteen_bits() {
        // A 512 tile at the far corner of a 4096 page: 4096 * 4 = 16384 units, under 32767.
        let mut place = alloc::vec![None; 8];
        place[2] = Some(Place { page: 0, x: 3584, y: 3584, side: 512 });
        let t = Table { place, pages: alloc::vec![PageSpec { w: 4096, h: 4096, levels: 4 }], bytes: 0, cap: 512 };
        let mut v: Vec<Vertex> = quad(2, [1, 255], [1, 255]).to_vec();
        let hd = split_with(&mut v, &t);
        assert!(hd.verts.iter().all(|v| v.uv[0] > 0 && v.uv[1] > 0 && (v.uv[0] as i32) < 16384 && (v.uv[1] as i32) < 16384));
    }
}
