//! Deciding what is drawn: frustum and occlusion culling of the chunks'
//! sections, the draw lists they make, and the distance cuts for creatures.
//!
//! Each frame [`Culler::plan`] works out, for every chunk in view, which of
//! its sixteen vertical sections to draw (see `terrain/sections.rs`):
//!
//! 1. the connectivity words of all meshed sections around the camera are
//!    copied into a flat grid (unmeshed chunks count as open);
//! 2. `terrain/visibility.rs` walks that grid from the camera's section,
//!    through the faces the words say are joined, and only into sections
//!    inside the view frustum: what it does not enter is behind rock;
//! 3. each section it entered that has vertices gets a tight box test, and
//!    the survivors are joined into as few `glDrawArrays` ranges as possible;
//! 4. chunks are put in front-to-back order (the opaque pass wants that for
//!    early depth rejection; the water pass walks the list backwards).
//!
//! All buffers are kept between frames, so a frame allocates nothing once
//! they have grown. The renderer only has to walk the lists.

use crate::chunk::CW;
use crate::math::{floord, Frustum, Vec3};
use crate::sections::{self, SECTIONS};
use crate::visibility::{self, Grid, Scratch};
use crate::world::World;
use alloc::vec::Vec;

/// What to draw of one chunk: its three buffers and, for each, a slice of
/// [`Culler::runs`] (`[first vertex, vertex count]`).
#[derive(Clone, Copy, Default)]
pub struct ChunkDraw {
    /// Squared distance from the camera's chunk, in chunks.
    pub d2: i32,
    /// Where the chunk's corner is, relative to the camera.
    pub origin: [f32; 3],
    /// Opaque, water, glass.
    pub vbo: [u32; 3],
    pub run_start: [u32; 3],
    pub run_len: [u8; 3],
    /// The chunk's HD faces (`terrain/hd_split.rs`): buffer and vertices per page of the HD atlas.
    /// They are drawn whole whenever any section of the chunk is.
    pub hd: u32,
    pub hd_runs: [i32; 4],
}

/// What the culling did this frame, for the F3 screen.
#[derive(Clone, Copy, Default)]
pub struct CullStats {
    /// Sections with geometry in chunks within the view distance.
    pub total: u32,
    pub drawn: u32,
    pub frustum: u32,
    pub occluded: u32,
    pub micros: u32,
}

pub struct Culler {
    /// Occlusion culling on (F3 + O or `--show nocull` turns it off, to compare).
    pub occlusion: bool,
    conn: Vec<u16>,
    scratch: Scratch,
    out: Vec<u32>,
    /// The chunks to draw, nearest first.
    pub chunks: Vec<ChunkDraw>,
    pub runs: Vec<[i32; 2]>,
    pub stats: CullStats,
}

impl Default for Culler {
    fn default() -> Self {
        Culler::new()
    }
}

impl Culler {
    pub fn new() -> Culler {
        Culler { occlusion: true, conn: Vec::new(), scratch: Scratch::default(), out: Vec::new(), chunks: Vec::new(), runs: Vec::new(), stats: CullStats::default() }
    }

    /// The slice of `runs` for one kind of geometry of a chunk.
    #[inline]
    pub fn runs_of(&self, c: &ChunkDraw, kind: usize) -> &[[i32; 2]] {
        let s = c.run_start[kind] as usize;
        &self.runs[s..s + c.run_len[kind] as usize]
    }

    /// Work out what to draw from the camera at `cam` with the view
    /// `frustum` (in camera-relative coordinates), `distance` chunks out.
    pub fn plan(&mut self, world: &World, cam: [f64; 3], frustum: &Frustum, distance: i32) {
        let t0 = crate::sys::now();
        self.chunks.clear();
        self.runs.clear();
        self.stats = CullStats::default();
        let (ccx, ccz) = crate::world::chunk_at(cam[0], cam[2]);
        let reach = distance * distance + distance;
        let radius = distance.max(1);
        let w = 2 * radius + 1;
        let column = |dx: i32, dz: i32| (((dx + radius) * w + (dz + radius)) as usize) * SECTIONS;
        let origin_of = |cx: i32, cz: i32| [((cx * CW) as f64 - cam[0]) as f32, (0.0 - cam[1]) as f32, ((cz * CW) as f64 - cam[2]) as f32];

        // 1. The connectivity grid: open unless a mesh says otherwise.
        self.conn.clear();
        self.conn.resize((w * w) as usize * SECTIONS, sections::CONN_ALL);
        for (&(cx, cz), chunk) in world.chunks.iter() {
            let (dx, dz) = (cx - ccx, cz - ccz);
            if dx * dx + dz * dz > reach {
                continue;
            }
            if let Some(m) = &chunk.mesh {
                if m.sec.valid {
                    let base = column(dx, dz);
                    self.conn[base..base + SECTIONS].copy_from_slice(&m.sec.conn);
                }
            }
        }

        // 2. The walk from the camera's section.
        if self.occlusion {
            let cam_sy = sections::section_of(floord(cam[1])) as i32;
            let grid = Grid { radius, conn: &self.conn };
            let mut inside = |dx: i32, sy: i32, dz: i32| {
                let o = origin_of(ccx + dx, ccz + dz);
                let y0 = o[1] + (sy * 16) as f32;
                frustum.box_visible(Vec3::new(o[0], y0, o[2]), Vec3::new(o[0] + 16.0, y0 + 16.0, o[2] + 16.0))
            };
            visibility::traverse(&grid, cam_sy, reach, &mut inside, &mut self.scratch, &mut self.out);
        }

        // 3. Sections to draw, chunk by chunk.
        for (&(cx, cz), chunk) in world.chunks.iter() {
            let m = match &chunk.mesh {
                Some(m) if m.has_geometry() => m,
                _ => continue,
            };
            let (dx, dz) = (cx - ccx, cz - ccz);
            let d2 = dx * dx + dz * dz;
            if d2 > reach {
                continue;
            }
            let o = origin_of(cx, cz);
            let mut cd = ChunkDraw { d2, origin: o, vbo: [m.opaque, m.water, m.glass], hd: m.hd, hd_runs: m.hd_runs, ..Default::default() };
            let counts = [m.opaque_count, m.water_count, m.glass_count];
            if !m.sec.valid {
                // A mesh without sections: all of it or nothing.
                let min = Vec3::new(o[0], o[1] + m.ymin as f32, o[2]);
                let max = Vec3::new(o[0] + CW as f32, o[1] + m.ymax as f32, o[2] + CW as f32);
                if !frustum.box_visible(min, max) {
                    continue;
                }
                for kind in 0..3 {
                    if counts[kind] > 0 {
                        cd.run_start[kind] = self.runs.len() as u32;
                        cd.run_len[kind] = 1;
                        self.runs.push([0, counts[kind]]);
                    }
                }
                self.chunks.push(cd);
                continue;
            }
            let col = column(dx, dz);
            let mut mask = 0u16;
            for s in 0..SECTIONS {
                if !m.sec.has_geometry(s) {
                    continue;
                }
                self.stats.total += 1;
                if self.occlusion {
                    let st = self.scratch.state[col + s];
                    if st & visibility::SEEN == 0 {
                        // Never entered: outside the view, or behind rock.
                        if st & visibility::TESTED != 0 {
                            self.stats.frustum += 1;
                        } else {
                            self.stats.occluded += 1;
                        }
                        continue;
                    }
                }
                // The box of the geometry itself (a little loose: water sways).
                let b = m.sec.bounds[s];
                let y0 = o[1] + (s * 16) as f32;
                let min = Vec3::new(o[0] + b[0] as f32 - 0.5, y0 + b[1] as f32 - 0.5, o[2] + b[2] as f32 - 0.5);
                let max = Vec3::new(o[0] + b[3] as f32 + 0.5, y0 + b[4] as f32 + 0.5, o[2] + b[5] as f32 + 0.5);
                if frustum.box_visible(min, max) {
                    mask |= 1 << s;
                    self.stats.drawn += 1;
                } else {
                    self.stats.frustum += 1;
                }
            }
            if mask == 0 {
                continue;
            }
            for kind in 0..3 {
                if cd.vbo[kind] == 0 || counts[kind] == 0 {
                    continue;
                }
                cd.run_start[kind] = self.runs.len() as u32;
                cd.run_len[kind] = sections::runs(m.sec.starts(kind), mask, &mut self.runs) as u8;
            }
            if cd.run_len.iter().any(|&n| n > 0) || cd.hd_runs.iter().any(|&n| n > 0) {
                self.chunks.push(cd);
            }
        }
        // 4. Nearest first.
        self.chunks.sort_unstable_by_key(|c| c.d2);
        self.stats.micros = ((crate::sys::now() - t0) * 1.0e6) as u32;
    }
}

/// Whether the distant-terrain tile `(tx, tz)` (`TILE_CHUNKS` chunks square)
/// is wholly hidden by chunks that are drawn: every one of its chunks lies
/// inside the drawn circle (`reach` is `distance^2 + distance` in chunks) and
/// is meshed. `meshed(cx, cz)` says whether a chunk is.
pub fn tile_covered(tx: i32, tz: i32, ccx: i32, ccz: i32, reach: i32, meshed: &dyn Fn(i32, i32) -> bool) -> bool {
    let n = crate::lod::TILE_CHUNKS;
    // The corner farthest from the camera first: most tiles fail here.
    let far = |c: i32, lo: i32| (lo - c).abs().max((lo + n - 1 - c).abs());
    let (fx, fz) = (far(ccx, tx * n), far(ccz, tz * n));
    if fx * fx + fz * fz > reach {
        return false;
    }
    for i in 0..n {
        for j in 0..n {
            if !meshed(tx * n + i, tz * n + j) {
                return false;
            }
        }
    }
    true
}

/// How far from the camera a creature is still drawn, in blocks: 72 for an
/// ordinary body (more for the big ones), less for a small one, which is a
/// speck at that distance (a body half a block across: 35).
pub fn creature_range(width: f32, height: f32) -> f32 {
    let big = if width > height { width } else { height };
    if !(big >= 1.5) {
        // (Also catches NaN: drawn at the shortest range.)
        let size = if big.is_finite() { big.max(0.0) } else { 0.0 };
        return 16.0 + 38.0 * size;
    }
    72.0 + crate::creature_size::keep_extra(width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tile_inside_the_circle_with_all_chunks_meshed_is_covered() {
        let all = |_: i32, _: i32| true;
        // Tile (0, 0) covers chunks 0..4; the camera in chunk (1, 1), reach for distance 8.
        assert!(tile_covered(0, 0, 1, 1, 8 * 8 + 8, &all));
        // A tile at the edge of the circle is not.
        assert!(!tile_covered(2, 0, 1, 1, 8 * 8 + 8 - 40, &all));
        // A hole in the meshes uncovers it.
        let hole = |cx: i32, cz: i32| !(cx == 3 && cz == 2);
        assert!(!tile_covered(0, 0, 1, 1, 8 * 8 + 8, &hole));
    }

    #[test]
    fn a_tile_with_a_far_corner_outside_is_not_covered() {
        let all = |_: i32, _: i32| true;
        // Camera at chunk (0, 0); tile (2, 0) reaches chunk 11: 11^2 > 8^2 + 8.
        assert!(!tile_covered(2, 0, 0, 0, 8 * 8 + 8, &all));
        // Tile (-1, -1): chunks -4..-1, farthest 4: inside.
        assert!(tile_covered(-1, -1, 0, 0, 8 * 8 + 8, &all));
    }

    #[test]
    fn small_creatures_are_cut_closer() {
        assert!(creature_range(0.3, 0.3) < 30.0);
        assert!(creature_range(0.6, 0.9) > creature_range(0.3, 0.3));
        assert_eq!(creature_range(0.6, 1.8), 72.0);
        assert!(creature_range(8.0, 12.0) > 72.0);
        assert!(creature_range(f32::NAN, f32::NAN) <= 20.0);
        // Never beyond the distance everything else is cut at.
        assert!(creature_range(1.4, 1.4) <= 72.0);
    }
}
