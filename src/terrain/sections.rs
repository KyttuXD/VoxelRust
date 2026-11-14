//! Vertical sections of a chunk.
//!
//! A chunk is 256 blocks tall, but most of it is empty sky or solid stone.
//! Its mesh is therefore laid out in sixteen sections of 16 blocks: the
//! vertices of each kind of geometry (opaque, water, glass) are grouped by
//! section in one buffer, and a table says where each section's vertices
//! start. The renderer can then skip the sections it cannot see with a
//! plain `glDrawArrays(first, count)` on the one buffer, and the number of
//! buffers on the graphics card stays at three per chunk.
//!
//! Each section also carries a bounding box of its geometry and a
//! *connectivity* word (15 bits, one for each pair of its six faces) saying
//! which faces are joined by a path through cells that do not block sight.
//! `terrain/visibility.rs` walks those words from the camera to find the
//! sections that can be seen at all.
//!
//! Everything here is plain arithmetic, so it is tested without a window.

/// Blocks of height in one section.
pub const SECTION_H: i32 = 16;
/// Sections in a chunk.
pub const SECTIONS: usize = 16;

/// The six faces of a section, as numbers: `+x -x +y -y +z -z`. The opposite
/// of face `f` is `f ^ 1`.
pub const FACE_PX: usize = 0;
pub const FACE_NX: usize = 1;
pub const FACE_PY: usize = 2;
pub const FACE_NY: usize = 3;
pub const FACE_PZ: usize = 4;
pub const FACE_NZ: usize = 5;

/// Every pair of faces connected (an empty or open section).
pub const CONN_ALL: u16 = 0x7FFF;

/// Where each kind of geometry of a chunk starts, section by section, with
/// the other facts the renderer needs about the sections.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SectionTables {
    /// False for meshes made without sections (distant tiles): the renderer
    /// then draws the whole buffer and does no section culling.
    pub valid: bool,
    /// Vertex offsets: the vertices of section `s` are `start[s]..start[s + 1]`.
    pub opaque: [u32; SECTIONS + 1],
    pub water: [u32; SECTIONS + 1],
    pub glass: [u32; SECTIONS + 1],
    /// Bounding box of the geometry of each section, in blocks inside the
    /// chunk: `[x0, y0, z0, x1, y1, z1]` with y counted from the section's
    /// own floor (0..=16).
    pub bounds: [[u8; 6]; SECTIONS],
    /// Which faces of each section see each other (see [`pair_bit`]).
    pub conn: [u16; SECTIONS],
    /// A bit for each section that has faces of HD tiles (kept apart in `MeshData::hd`).
    pub hd: u16,
}

impl SectionTables {
    /// The start table of a kind of geometry: 0 opaque, 1 water, 2 glass.
    pub fn starts(&self, kind: usize) -> &[u32; SECTIONS + 1] {
        match kind {
            0 => &self.opaque,
            1 => &self.water,
            _ => &self.glass,
        }
    }

    /// Whether section `s` has any vertices at all.
    pub fn has_geometry(&self, s: usize) -> bool {
        self.opaque[s + 1] > self.opaque[s] || self.water[s + 1] > self.water[s] || self.glass[s + 1] > self.glass[s] || self.hd & (1 << s) != 0
    }
}

/// The bit that stands for the pair of faces `a` and `b` (different, 0..6).
#[inline]
pub fn pair_bit(a: usize, b: usize) -> u16 {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    // Pairs are numbered row by row: (0,1) (0,2).. (0,5) (1,2).. (4,5).
    const BASE: [usize; 5] = [0, 5, 9, 12, 14];
    1 << (BASE[lo.min(4)] + hi - lo - 1)
}

/// Whether a path through the section joins face `a` to face `b`.
#[inline]
pub fn connected(mask: u16, a: usize, b: usize) -> bool {
    a != b && a < 6 && b < 6 && mask & pair_bit(a, b) != 0
}

/// The mask of a single open region that touches the faces in `faces`
/// (bit `f` set for face `f`): every pair of them is joined.
pub fn mask_of_faces(faces: u8) -> u16 {
    let mut m = 0u16;
    for a in 0..6 {
        for b in a + 1..6 {
            if faces & (1 << a) != 0 && faces & (1 << b) != 0 {
                m |= pair_bit(a, b);
            }
        }
    }
    m
}

/// Connectivity of one 16 x 16 x 16 section. `open[x + 16 * z + 256 * y]` is
/// non-zero for a cell that does not block sight. Open cells joined by
/// face-to-face steps form regions; two faces are connected when one region
/// touches both.
pub fn connectivity(open: &[u8; 4096]) -> u16 {
    let count = open.iter().filter(|&&c| c != 0).count();
    if count == 0 {
        return 0;
    }
    if count == 4096 {
        return CONN_ALL;
    }
    let mut work = *open;
    let mut stack: alloc::vec::Vec<u16> = alloc::vec::Vec::with_capacity(512);
    let mut mask = 0u16;
    for start in 0..4096usize {
        if work[start] == 0 {
            continue;
        }
        work[start] = 0;
        stack.push(start as u16);
        let mut faces = 0u8;
        while let Some(i) = stack.pop() {
            let i = i as usize;
            let (x, z, y) = (i & 15, (i >> 4) & 15, i >> 8);
            if x == 15 {
                faces |= 1 << FACE_PX;
            }
            if x == 0 {
                faces |= 1 << FACE_NX;
            }
            if y == 15 {
                faces |= 1 << FACE_PY;
            }
            if y == 0 {
                faces |= 1 << FACE_NY;
            }
            if z == 15 {
                faces |= 1 << FACE_PZ;
            }
            if z == 0 {
                faces |= 1 << FACE_NZ;
            }
            let mut go = |j: usize| {
                if work[j] != 0 {
                    work[j] = 0;
                    stack.push(j as u16);
                }
            };
            if x < 15 {
                go(i + 1);
            }
            if x > 0 {
                go(i - 1);
            }
            if z < 15 {
                go(i + 16);
            }
            if z > 0 {
                go(i - 16);
            }
            if y < 15 {
                go(i + 256);
            }
            if y > 0 {
                go(i - 256);
            }
        }
        mask |= mask_of_faces(faces);
        if mask == CONN_ALL {
            break;
        }
    }
    mask
}

/// The draw ranges for the sections whose bit is set in `visible`: runs of
/// neighbouring visible sections are joined into one range. Each range is
/// `[first vertex, vertex count]`, appended to `out`; returns how many were
/// added (at most eight).
pub fn runs(starts: &[u32; SECTIONS + 1], visible: u16, out: &mut alloc::vec::Vec<[i32; 2]>) -> usize {
    let before = out.len();
    let mut s = 0usize;
    while s < SECTIONS {
        if visible >> s & 1 == 0 {
            s += 1;
            continue;
        }
        let mut e = s;
        while e + 1 < SECTIONS && visible >> (e + 1) & 1 != 0 {
            e += 1;
        }
        let (first, end) = (starts[s], starts[e + 1]);
        if end > first {
            out.push([first as i32, (end - first) as i32]);
        }
        s = e + 1;
    }
    out.len() - before
}

/// Section number of a block height (0..=15 for heights inside the world).
#[inline]
pub fn section_of(y: i32) -> usize {
    (y >> 4).clamp(0, SECTIONS as i32 - 1) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(x: usize, y: usize, z: usize) -> usize {
        x + 16 * z + 256 * y
    }

    #[test]
    fn pair_bits_are_fifteen_different_bits() {
        let mut seen = 0u16;
        for a in 0..6 {
            for b in a + 1..6 {
                let bit = pair_bit(a, b);
                assert_eq!(bit.count_ones(), 1);
                assert_eq!(seen & bit, 0, "pair {} {} repeats", a, b);
                assert_eq!(bit, pair_bit(b, a));
                seen |= bit;
            }
        }
        assert_eq!(seen, CONN_ALL);
        assert!(!connected(CONN_ALL, 2, 2));
    }

    #[test]
    fn an_empty_section_joins_every_face_and_a_solid_one_none() {
        assert_eq!(connectivity(&[1u8; 4096]), CONN_ALL);
        assert_eq!(connectivity(&[0u8; 4096]), 0);
    }

    #[test]
    fn a_tunnel_joins_only_its_two_ends() {
        let mut open = [0u8; 4096];
        for x in 0..16 {
            open[cell(x, 5, 7)] = 1;
        }
        let m = connectivity(&open);
        assert_eq!(m, pair_bit(FACE_PX, FACE_NX));
        assert!(connected(m, FACE_NX, FACE_PX));
        assert!(!connected(m, FACE_NX, FACE_PY));
    }

    #[test]
    fn a_wall_splits_the_faces_into_two_groups() {
        // A solid plane at x = 8: the two halves never meet.
        let mut open = [1u8; 4096];
        for y in 0..16 {
            for z in 0..16 {
                open[cell(8, y, z)] = 0;
            }
        }
        let m = connectivity(&open);
        assert!(connected(m, FACE_NX, FACE_PY));
        assert!(connected(m, FACE_PX, FACE_PY));
        assert!(connected(m, FACE_PZ, FACE_NZ));
        assert!(!connected(m, FACE_NX, FACE_PX));
    }

    #[test]
    fn a_bend_joins_the_faces_it_touches() {
        let mut open = [0u8; 4096];
        for x in 0..16 {
            open[cell(x, 3, 3)] = 1;
        }
        for z in 0..16 {
            open[cell(15, 3, z)] = 1;
        }
        let m = connectivity(&open);
        // Touches -x, +x, -z and +z.
        for (a, b) in [(FACE_NX, FACE_PX), (FACE_NX, FACE_PZ), (FACE_PX, FACE_NZ), (FACE_PZ, FACE_NZ)] {
            assert!(connected(m, a, b));
        }
        assert!(!connected(m, FACE_NX, FACE_PY));
    }

    #[test]
    fn cells_touching_only_at_an_edge_or_corner_do_not_join() {
        // Two halves of a tunnel that meet along an edge only: no path for sight.
        let mut open = [0u8; 4096];
        for x in 0..8 {
            open[cell(x, 5, 5)] = 1;
        }
        for x in 8..16 {
            open[cell(x, 6, 6)] = 1;
        }
        assert_eq!(connectivity(&open), 0);
        // And a corner-only meeting.
        let mut open = [0u8; 4096];
        for x in 0..8 {
            open[cell(x, 5, 5)] = 1;
        }
        open[cell(8, 6, 6)] = 1;
        for x in 9..16 {
            open[cell(x, 6, 6)] = 1;
        }
        assert_eq!(connectivity(&open), 0);
        // One shared face makes the path.
        let mut open = [0u8; 4096];
        for x in 0..16 {
            open[cell(x, 5, 5)] = 1;
        }
        open[cell(15, 6, 5)] = 1;
        assert_eq!(connectivity(&open), pair_bit(FACE_NX, FACE_PX));
    }

    #[test]
    fn single_cell_tunnels_along_each_axis_join_their_two_ends() {
        for (axis, a, b) in [(0, FACE_NX, FACE_PX), (1, FACE_NY, FACE_PY), (2, FACE_NZ, FACE_PZ)] {
            let mut open = [0u8; 4096];
            for i in 0..16 {
                let (x, y, z) = match axis {
                    0 => (i, 9, 2),
                    1 => (3, i, 12),
                    _ => (15, 0, i),
                };
                open[cell(x, y, z)] = 1;
            }
            assert_eq!(connectivity(&open), pair_bit(a, b), "axis {}", axis);
        }
    }

    #[test]
    fn a_one_cell_plug_closes_a_tunnel() {
        let mut open = [0u8; 4096];
        for x in 0..16 {
            open[cell(x, 5, 7)] = 1;
        }
        open[cell(9, 5, 7)] = 0;
        assert_eq!(connectivity(&open), 0);
    }

    #[test]
    fn separate_pockets_do_not_join() {
        // Two pockets, one touching only -x and one only +x.
        let mut open = [0u8; 4096];
        open[cell(0, 8, 8)] = 1;
        open[cell(15, 8, 8)] = 1;
        assert_eq!(connectivity(&open), 0);
    }

    #[test]
    fn neighbouring_visible_sections_make_one_range() {
        let mut starts = [0u32; SECTIONS + 1];
        for s in 0..SECTIONS {
            starts[s + 1] = starts[s] + 8;
        }
        let mut out = alloc::vec::Vec::new();
        // Sections 2, 3, 4 and 7.
        let n = runs(&starts, 0b1001_1100, &mut out);
        assert_eq!(n, 2);
        assert_eq!(out, alloc::vec![[16, 24], [56, 8]]);
        out.clear();
        assert_eq!(runs(&starts, 0, &mut out), 0);
        assert_eq!(runs(&starts, 0xFFFF, &mut out), 1);
        assert_eq!(out, alloc::vec![[0, 128]]);
    }

    #[test]
    fn empty_sections_add_no_range() {
        let mut starts = [0u32; SECTIONS + 1];
        // Only section 5 has vertices.
        for s in 0..SECTIONS {
            starts[s + 1] = starts[s] + if s == 5 { 12 } else { 0 };
        }
        let mut out = alloc::vec::Vec::new();
        assert_eq!(runs(&starts, 1 << 2, &mut out), 0);
        assert_eq!(runs(&starts, 1 << 5, &mut out), 1);
        assert_eq!(out, alloc::vec![[0, 12]]);
    }

    #[test]
    fn section_numbers_follow_the_height() {
        assert_eq!(section_of(0), 0);
        assert_eq!(section_of(15), 0);
        assert_eq!(section_of(16), 1);
        assert_eq!(section_of(255), 15);
        assert_eq!(section_of(-3), 0);
        assert_eq!(section_of(400), 15);
    }
}
