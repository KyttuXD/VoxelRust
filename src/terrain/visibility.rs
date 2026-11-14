//! Which sections of the world can be seen at all: occlusion culling by
//! cave visibility.
//!
//! Every section (a 16 x 16 x 16 block of a chunk, see `sections.rs`) has a
//! word saying which of its six faces are joined by a path through cells
//! that do not block sight. Starting from the section the camera is in, the
//! walk below steps from section to section through the faces that are
//! joined, always away from the camera (never back along an axis it has
//! already moved along), and reports every section it enters. A section
//! behind solid stone, or in a cave the tunnels do not lead to, is never
//! entered, so it is never drawn.
//!
//! The walk is conservative where it matters: sections are entered whole
//! (everything in them is drawn once any face of them can be reached), the
//! camera's own section and the 26 around it are always reported, and a
//! section whose chunk is not meshed yet counts as open. It only ever
//! visits sections inside the view (`inside`), which keeps it cheap: the
//! cost is proportional to the sections that are in view and reachable.
//!
//! The function is pure (a grid of words in, a list of sections out), so
//! the tests need no window.

use crate::sections::{connected, SECTIONS};
use alloc::vec::Vec;

/// Bits of the per-section state kept by [`Scratch`].
const ENTRY_ALL: u8 = 0x3F;
/// The section has been reported.
pub const SEEN: u8 = 0x40;
/// The section has been tested against the view.
pub const TESTED: u8 = 0x80;

/// The connectivity words of the sections around the camera. The grid is
/// `2 * radius + 1` chunks wide in each direction with the camera's chunk in
/// the middle, and `SECTIONS` sections in each column.
pub struct Grid<'a> {
    pub radius: i32,
    pub conn: &'a [u16],
}

impl<'a> Grid<'a> {
    pub fn width(&self) -> i32 {
        2 * self.radius + 1
    }

    /// Index of the section at chunk offset (`dx`, `dz`) from the camera's
    /// chunk and height `sy`.
    #[inline]
    pub fn index(&self, dx: i32, sy: i32, dz: i32) -> usize {
        let w = self.width();
        (((dx + self.radius) * w + (dz + self.radius)) as usize) * SECTIONS + sy as usize
    }

    /// The inverse of [`Grid::index`].
    pub fn coords(&self, idx: usize) -> (i32, i32, i32) {
        let col = (idx / SECTIONS) as i32;
        let w = self.width();
        (col / w - self.radius, (idx % SECTIONS) as i32, col % w - self.radius)
    }

    /// Number of sections in the grid.
    pub fn len(&self) -> usize {
        let w = self.width() as usize;
        w * w * SECTIONS
    }
}

/// Memory the walk reuses from frame to frame.
#[derive(Default)]
pub struct Scratch {
    /// Per section: the faces it has been entered through (bits 0..6), and
    /// [`SEEN`] and [`TESTED`]. A section outside the view is marked as
    /// entered through every face, so it is never looked at again.
    pub state: Vec<u8>,
    queue: Vec<(u32, u8)>,
}

/// Walk from the camera's section, `cam_sy` high, in the camera's chunk (the
/// middle of the grid). `reach2` limits how far the walk goes, as a squared
/// distance in chunks; `inside(dx, sy, dz)` says whether a section may be
/// seen from the camera (the view frustum). The grid indices of the sections
/// reached go to `out`, roughly nearest first.
pub fn traverse(grid: &Grid, cam_sy: i32, reach2: i32, inside: &mut dyn FnMut(i32, i32, i32) -> bool, scratch: &mut Scratch, out: &mut Vec<u32>) {
    out.clear();
    scratch.queue.clear();
    scratch.state.clear();
    scratch.state.resize(grid.len(), 0);
    let cam_sy = cam_sy.clamp(0, SECTIONS as i32 - 1);
    let start = grid.index(0, cam_sy, 0);
    scratch.state[start] = SEEN | TESTED;
    out.push(start as u32);
    // The camera's own section lets sight out through every face.
    scratch.queue.push((start as u32, 6));

    let mut head = 0;
    while head < scratch.queue.len() {
        let (idx, entry) = scratch.queue[head];
        head += 1;
        let (dx, sy, dz) = grid.coords(idx as usize);
        let mask = grid.conn[idx as usize];
        let rel = [dx, sy - cam_sy, dz];
        for d in 0..6usize {
            if entry < 6 && !connected(mask, entry as usize, d) {
                continue;
            }
            // Away from the camera only: a step along an axis is allowed
            // while the section is not on the other side of the camera's.
            let axis = d >> 1;
            let positive = d & 1 == 0;
            if (positive && rel[axis] < 0) || (!positive && rel[axis] > 0) {
                continue;
            }
            let mut n = rel;
            n[axis] += if positive { 1 } else { -1 };
            let (ndx, nsy, ndz) = (n[0], n[1] + cam_sy, n[2]);
            if !(0..SECTIONS as i32).contains(&nsy) || ndx * ndx + ndz * ndz > reach2 || ndx.abs() > grid.radius || ndz.abs() > grid.radius {
                continue;
            }
            let ni = grid.index(ndx, nsy, ndz);
            let face = (d ^ 1) as u8;
            let st = scratch.state[ni];
            if st & (1 << face) != 0 {
                continue;
            }
            let mut st = st;
            if st & TESTED == 0 {
                if !inside(ndx, nsy, ndz) {
                    scratch.state[ni] = ENTRY_ALL | TESTED;
                    continue;
                }
                st |= TESTED;
            }
            st |= 1 << face;
            if st & SEEN == 0 {
                st |= SEEN;
                out.push(ni as u32);
            }
            scratch.state[ni] = st;
            scratch.queue.push((ni as u32, face));
        }
    }

    // Whatever touches the camera's section is always drawn: the camera may
    // sit in a cell the section's faces say nothing about.
    for ddx in -1..=1i32 {
        for ddy in -1..=1 {
            for ddz in -1..=1i32 {
                let (x, y, z) = (ddx, cam_sy + ddy, ddz);
                if !(0..SECTIONS as i32).contains(&y) || x.abs() > grid.radius || z.abs() > grid.radius {
                    continue;
                }
                let i = grid.index(x, y, z);
                if scratch.state[i] & SEEN == 0 {
                    scratch.state[i] |= SEEN;
                    out.push(i as u32);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sections::{pair_bit, CONN_ALL, FACE_NX, FACE_NZ, FACE_PX, FACE_PZ};

    const R: i32 = 4;
    const CAM_Y: i32 = 8;

    struct World {
        conn: Vec<u16>,
    }

    impl World {
        fn new(fill: u16) -> World {
            let w = (2 * R + 1) as usize;
            World { conn: alloc::vec![fill; w * w * SECTIONS] }
        }
        fn grid(&self) -> Grid<'_> {
            Grid { radius: R, conn: &self.conn }
        }
        fn set(&mut self, dx: i32, sy: i32, dz: i32, m: u16) {
            let i = self.grid().index(dx, sy, dz);
            self.conn[i] = m;
        }
        fn run(&self) -> Vec<(i32, i32, i32)> {
            let g = self.grid();
            let mut scratch = Scratch::default();
            let mut out = Vec::new();
            traverse(&g, CAM_Y, 1000, &mut |_, _, _| true, &mut scratch, &mut out);
            out.iter().map(|&i| g.coords(i as usize)).collect()
        }
    }

    #[test]
    fn open_country_is_all_visible() {
        let w = World::new(CONN_ALL);
        let seen = w.run();
        let everything = ((2 * R + 1) * (2 * R + 1)) as usize * SECTIONS;
        assert_eq!(seen.len(), everything);
        // Nothing twice.
        let mut sorted = seen.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), everything);
    }

    #[test]
    fn index_and_coords_agree() {
        let w = World::new(0);
        let g = w.grid();
        for dx in -R..=R {
            for dz in -R..=R {
                for sy in 0..SECTIONS as i32 {
                    assert_eq!(g.coords(g.index(dx, sy, dz)), (dx, sy, dz));
                }
            }
        }
    }

    #[test]
    fn a_solid_mountain_hides_the_cave_behind_it() {
        let mut w = World::new(CONN_ALL);
        // A wall of solid sections across the whole width at dx = 2.
        for sy in 0..SECTIONS as i32 {
            for dz in -R..=R {
                w.set(2, sy, dz, 0);
            }
        }
        let seen = w.run();
        // The wall's near face is seen: its sections are entered.
        assert!(seen.contains(&(2, CAM_Y, 0)));
        // Everything past it is not, caves or not.
        assert!(seen.iter().all(|s| s.0 <= 2), "something behind the mountain is drawn");
        assert!(!seen.contains(&(3, CAM_Y, 0)));
        assert!(!seen.contains(&(4, CAM_Y + 2, 1)));
        // Everything this side of it is.
        assert!(seen.contains(&(-4, 0, -4)));
        assert!(seen.contains(&(1, 15, 3)));
    }

    #[test]
    fn a_tunnel_keeps_the_cave_visible() {
        let mut w = World::new(CONN_ALL);
        for sy in 0..SECTIONS as i32 {
            for dz in -R..=R {
                w.set(2, sy, dz, 0);
            }
        }
        // A tunnel through the wall at the camera's height, straight along x.
        w.set(2, CAM_Y, 0, pair_bit(FACE_NX, FACE_PX));
        let seen = w.run();
        assert!(seen.contains(&(3, CAM_Y, 0)), "the cave at the end of the tunnel is hidden");
        // The cave is open, so sight spreads inside it...
        assert!(seen.contains(&(3, CAM_Y, 1)));
        assert!(seen.contains(&(4, CAM_Y + 1, -1)));
    }

    #[test]
    fn the_camera_in_a_cave_sees_what_the_tunnels_allow() {
        // Everything is rock except a winding tunnel: east, then north (+z), then a chamber.
        let mut w = World::new(0);
        w.set(0, CAM_Y, 0, CONN_ALL);
        w.set(1, CAM_Y, 0, pair_bit(FACE_NX, FACE_PZ));
        w.set(1, CAM_Y, 1, pair_bit(FACE_NZ, FACE_PZ));
        w.set(1, CAM_Y, 2, CONN_ALL);
        // A chamber nowhere near the tunnel.
        w.set(-3, CAM_Y, -3, CONN_ALL);
        w.set(3, CAM_Y, 3, CONN_ALL);
        let seen = w.run();
        for s in [(0, CAM_Y, 0), (1, CAM_Y, 0), (1, CAM_Y, 1), (1, CAM_Y, 2)] {
            assert!(seen.contains(&s), "{:?} should be seen", s);
        }
        assert!(!seen.contains(&(-3, CAM_Y, -3)));
        assert!(!seen.contains(&(3, CAM_Y, 3)));
        // The chamber's far wall is entered (its face shows), the rock past it is not.
        assert!(seen.contains(&(1, CAM_Y, 3)));
        assert!(!seen.contains(&(1, CAM_Y, 4)));
        // Rock beside the tunnel is not entered either, except the camera's neighbours.
        assert!(!seen.contains(&(2, CAM_Y, 1)));
        assert!(!seen.contains(&(2, CAM_Y + 3, 1)));
    }

    #[test]
    fn the_camera_and_its_neighbours_are_always_drawn() {
        // The camera is walled in: everything solid.
        let w = World::new(0);
        let seen = w.run();
        assert_eq!(seen.len(), 27);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    assert!(seen.contains(&(dx, CAM_Y + dy, dz)));
                }
            }
        }
    }

    #[test]
    fn the_view_prunes_the_walk() {
        let w = World::new(CONN_ALL);
        let g = w.grid();
        let mut scratch = Scratch::default();
        let mut out = Vec::new();
        // Only the half of the world with dx >= 0 is in view.
        traverse(&g, CAM_Y, 1000, &mut |dx, _, _| dx >= 0, &mut scratch, &mut out);
        let seen: Vec<_> = out.iter().map(|&i| g.coords(i as usize)).collect();
        assert!(seen.contains(&(3, 0, -2)));
        // (-1 is there only as a neighbour of the camera.)
        assert!(seen.iter().all(|s| s.0 >= -1));
        assert!(!seen.contains(&(-3, CAM_Y, 0)));
    }

    #[test]
    fn the_walk_stops_at_the_reach() {
        let w = World::new(CONN_ALL);
        let g = w.grid();
        let mut scratch = Scratch::default();
        let mut out = Vec::new();
        traverse(&g, CAM_Y, 4, &mut |_, _, _| true, &mut scratch, &mut out);
        let seen: Vec<_> = out.iter().map(|&i| g.coords(i as usize)).collect();
        assert!(seen.iter().all(|s| s.0 * s.0 + s.2 * s.2 <= 4));
        assert!(seen.contains(&(2, 0, 0)));
        assert!(!seen.contains(&(3, 0, 0)));
    }

    #[test]
    fn a_wall_beside_the_camera_hides_everything_past_it() {
        let mut w = World::new(CONN_ALL);
        for sy in 0..SECTIONS as i32 {
            for dz in -R..=R {
                w.set(1, sy, dz, 0);
            }
        }
        // The camera sits at dx = 0; sections behind the wall stay hidden although they are open.
        let seen = w.run();
        assert!(!seen.contains(&(2, CAM_Y, 0)));
        assert!(!seen.contains(&(2, CAM_Y + 4, 3)));
    }

    #[test]
    fn work_is_proportional_to_what_is_visible() {
        // A world that is all rock but the camera's own section visits few sections.
        let w = World::new(0);
        let g = w.grid();
        let mut scratch = Scratch::default();
        let mut out = Vec::new();
        let mut tested = 0;
        traverse(&g, CAM_Y, 1000, &mut |_, _, _| {
            tested += 1;
            true
        }, &mut scratch, &mut out);
        assert!(tested <= 6, "{} sections were looked at", tested);
        // Unused imports of the helpers keep the test module honest.
        let _ = (FACE_PX, FACE_PZ);
    }
}
