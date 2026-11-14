//! Keeping distant tiles true to what people built.
//!
//! Distant tiles are made on the worker threads from the terrain function plus
//! a [`FarInput`]: the changes of the shared world (`World::overlay`) and the
//! tops of changed chunks (live ones, and `far_tops`, the summaries kept of
//! changed chunks that were saved or dropped, 1 KB each, at most
//! `MAX_SUMMARIES`). When a change arrives or a changed chunk is dropped, the
//! tile that covers it is marked dirty and made again, at most once a second
//! (a build of 900 blocks makes one rebuild a second, not 900).

use crate::chunk::chunk_of;
use crate::far_surface::{self, FarInput};
use crate::lod::TILE_CHUNKS;
use crate::world::{Chunk, ChunkStore, World};

/// Chunk summaries kept, at most (about 3 MB).
pub const MAX_SUMMARIES: usize = 3072;
/// A tile is made again at most this often, in seconds.
pub const REBUILD_SECONDS: f64 = 1.0;

/// Seconds on the clock, or `None` where there is no window (tests, servers).
pub fn clock() -> Option<f64> {
    if crate::sys::sdl::loaded() {
        Some(crate::sys::now())
    } else {
        None
    }
}

/// Whether a tile built at `built` may be built again at `now`.
pub fn rebuild_due(now: Option<f64>, built: f64) -> bool {
    match now {
        None => true,
        Some(t) => t - built >= REBUILD_SECONDS || t < built,
    }
}

impl World {
    /// The tile covering chunk `(cx, cz)` has to be made again.
    pub(crate) fn far_mark(&mut self, cx: i32, cz: i32) {
        let key = (cx.div_euclid(TILE_CHUNKS), cz.div_euclid(TILE_CHUNKS));
        if let Some(t) = self.far.get_mut(&key) {
            t.dirty = true;
            self.settled = false;
        }
    }

    /// A change in the overlay at block column `(x, z)`: the tile is marked
    /// now unless the chunk is on screen as real blocks (then when it is dropped).
    pub(crate) fn far_note_edit(&mut self, x: i32, z: i32) {
        let key = (chunk_of(x), chunk_of(z));
        if self.chunks.get(&key).map_or(false, |c| c.mesh.is_some()) {
            self.far_unseen.insert(key);
        } else {
            self.far_mark(key.0, key.1);
        }
    }

    /// A chunk is about to be forgotten: keep what distant tiles need of it.
    pub(crate) fn far_note_drop(&mut self, key: (i32, i32), c: &Chunk) {
        if let Some(d) = c.data.as_ref() {
            if c.modified || c.from_store {
                let had = self.far_tops.contains_key(&key);
                if (c.modified || !had) && (had || self.far_tops.len() < MAX_SUMMARIES) {
                    self.far_tops.insert(key, far_surface::summarise(d));
                    self.far_mark(key.0, key.1);
                }
            }
        }
        if self.far_unseen.remove(&key) {
            self.far_mark(key.0, key.1);
        }
    }

    /// Before the tile `(tx, tz)` is made: read the summaries of the chunks the
    /// player saved in its region of the save (once per region, one a frame).
    /// Returns false when this frame's turn is used up.
    pub(crate) fn far_scan_region(&mut self, tx: i32, tz: i32, store: &mut dyn ChunkStore) -> bool {
        if !store.persistent() {
            return true;
        }
        let r = crate::save::REGION;
        let (rx, rz) = ((tx * TILE_CHUNKS).div_euclid(r), (tz * TILE_CHUNKS).div_euclid(r));
        if self.far_scanned.contains(&(rx, rz)) {
            return true;
        }
        if self.far_scans > 0 {
            return false;
        }
        self.far_scans += 1;
        self.far_scanned.insert((rx, rz));
        for cx in rx * r..rx * r + r {
            for cz in rz * r..rz * r + r {
                if self.far_tops.len() >= MAX_SUMMARIES {
                    return true;
                }
                if self.far_tops.contains_key(&(cx, cz)) || self.chunks.get(&(cx, cz)).map_or(false, |c| c.data.is_some()) {
                    continue;
                }
                if let Some(d) = store.load(cx, cz) {
                    self.far_tops.insert((cx, cz), far_surface::summarise(&d));
                }
            }
        }
        true
    }

    /// What the job for tile `(tx, tz)` is told about changes: the overlay's
    /// edits and the tops of changed chunks (loaded ones as they are now).
    pub fn far_input(&self, tx: i32, tz: i32) -> FarInput {
        let mut input = FarInput::default();
        for i in 0..TILE_CHUNKS {
            for j in 0..TILE_CHUNKS {
                let (cx, cz) = (tx * TILE_CHUNKS + i, tz * TILE_CHUNKS + j);
                if let Some(list) = self.overlay.get(&(cx, cz)) {
                    for &(x, y, z, b) in list {
                        if input.edits.len() >= far_surface::MAX_EDITS {
                            break;
                        }
                        input.edits.push((cx * 16 + x, y, cz * 16 + z, b));
                    }
                }
                match self.chunks.get(&(cx, cz)) {
                    Some(Chunk { data: Some(d), modified, from_store, .. }) => {
                        if *modified || *from_store {
                            far_surface::summary_tops(cx, cz, &far_surface::summarise(d), &mut input.tops);
                        }
                    }
                    _ => {
                        if let Some(s) = self.far_tops.get(&(cx, cz)) {
                            far_surface::summary_tops(cx, cz, s, &mut input.tops);
                        }
                    }
                }
            }
        }
        input
    }

    /// The colour distant terrain gives the column `(x, z)` right now (what
    /// `/blockat` prints, to compare with what you see far off).
    pub fn far_colour_at(&self, x: i32, z: i32) -> [u8; 3] {
        let (cx, cz) = (chunk_of(x), chunk_of(z));
        let mut input = FarInput::default();
        if let Some(list) = self.overlay.get(&(cx, cz)) {
            for &(lx, y, lz, b) in list {
                input.edits.push((cx * 16 + lx, y, cz * 16 + lz, b));
            }
        }
        match self.chunks.get(&(cx, cz)) {
            Some(Chunk { data: Some(d), modified, from_store, .. }) => {
                if *modified || *from_store {
                    far_surface::summary_tops(cx, cz, &far_surface::summarise(d), &mut input.tops);
                }
            }
            _ => {
                if let Some(s) = self.far_tops.get(&(cx, cz)) {
                    far_surface::summary_tops(cx, cz, s, &mut input.tops);
                }
            }
        }
        far_surface::column_colour(self.seed, x, z, &input)
    }

    /// The content changed: numbers in the kept summaries are translated.
    pub(crate) fn far_remap(&mut self, fix: &dyn Fn(crate::block::Block) -> crate::block::Block) {
        for s in self.far_tops.values_mut() {
            for v in s.iter_mut() {
                if *v != far_surface::NO_TOP {
                    *v = (*v & 0xFFFF_0000) | fix((*v & 0xFFFF) as crate::block::Block) as u32;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::reg;
    use crate::jobs::Pool;
    use crate::mesh::MeshData;
    use crate::world::{GpuMesh, MeshSink, NullStore};

    #[derive(Default)]
    struct Sink;
    impl MeshSink for Sink {
        fn upload(&mut self, m: &MeshData) -> GpuMesh {
            GpuMesh { opaque_count: m.opaque.len() as i32, ..Default::default() }
        }
        fn free(&mut self, _: GpuMesh) {}
    }

    fn settle(w: &mut World, pool: &mut Pool, sink: &mut Sink) {
        for _ in 0..6000 {
            w.update(0, 0, 1, pool, sink, &mut NullStore);
            if w.settled {
                return;
            }
        }
        panic!("never settled");
    }

    #[test]
    fn rebuilds_are_limited_to_once_a_second() {
        assert!(rebuild_due(None, 5.0));
        assert!(!rebuild_due(Some(5.5), 5.0));
        assert!(rebuild_due(Some(6.0), 5.0));
        // A clock that went back does not freeze the tile.
        assert!(rebuild_due(Some(1.0), 5.0));
    }

    #[test]
    fn an_edit_far_away_marks_its_tile_once_and_changes_its_input() {
        let seed = 11;
        let mut w = World::new(seed);
        let mut pool = Pool::new(0, seed);
        let mut sink = Sink;
        w.far_radius = 16;
        settle(&mut w, &mut pool, &mut sink);
        let key = (2, 1);
        assert!(w.far.get(&key).map_or(false, |t| t.mesh.is_some()));
        assert!(!w.far[&key].dirty);
        let clay = reg().block("red_terracotta").unwrap_or(reg().wk.stone);
        // 900 blocks of a build, in the chunk range of tile (2, 1): blocks x 128.., z 64..
        for k in 0..900 {
            w.apply_remote(128 + 20 + k % 30, 120 + k / 300, 64 + 20 + (k / 30) % 10, clay);
        }
        assert!(w.far[&key].dirty);
        let input = w.far_input(key.0, key.1);
        assert!(input.edits.len() >= 600, "{}", input.edits.len());
        // The tile is made again (no clock in tests: at once), and is clean after.
        assert!(!w.settled);
        settle(&mut w, &mut pool, &mut sink);
        assert!(!w.far[&key].dirty);
        assert!(w.far[&key].mesh.is_some());
        let c = w.far_colour_at(128 + 21, 64 + 21);
        assert_eq!(c, crate::block_colour::of_block(clay));
    }
}
