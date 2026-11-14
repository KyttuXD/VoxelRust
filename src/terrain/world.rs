//! The loaded world: which chunks exist around the player, scheduling their
//! generation and meshing, block edits and ray casts.

use crate::block::{self, reg, Block, Kind, AIR};
use crate::chunk::{chunk_of, ChunkData, CH, CW};
use crate::jobs::{Job, JobResult, Pool};
use crate::math::{floord, Vec3};
use crate::mesh::MeshData;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// Where changed chunks are kept between sessions.
pub trait ChunkStore {
    fn load(&mut self, cx: i32, cz: i32) -> Option<ChunkData>;
    fn save(&mut self, cx: i32, cz: i32, data: &ChunkData);
}

/// A store that remembers nothing.
pub struct NullStore;

impl ChunkStore for NullStore {
    fn load(&mut self, _: i32, _: i32) -> Option<ChunkData> {
        None
    }
    fn save(&mut self, _: i32, _: i32, _: &ChunkData) {}
}

/// Handles to a chunk's geometry on the graphics card.
#[derive(Clone, Copy, Default, Debug)]
pub struct GpuMesh {
    pub opaque: u32,
    pub opaque_count: i32,
    pub water: u32,
    pub water_count: i32,
    pub glass: u32,
    pub glass_count: i32,
    pub ymin: i32,
    pub ymax: i32,
}

/// Whoever owns the graphics card memory (the renderer; a counter in tests).
pub trait MeshSink {
    fn upload(&mut self, m: &MeshData) -> GpuMesh;
    fn free(&mut self, m: GpuMesh);
}

#[derive(Default)]
pub struct Chunk {
    pub data: Option<Arc<ChunkData>>,
    pub mesh: Option<GpuMesh>,
    /// Blocks in this chunk that are drawn as models.
    pub props: Vec<([u8; 3], Block)>,
    gen_requested: bool,
    mesh_in_flight: bool,
    /// The mesh does not match the blocks any more.
    dirty: bool,
    /// Changed by the player since it was last saved.
    modified: bool,
}

#[derive(Clone, Copy)]
pub struct Hit {
    /// The block that was hit.
    pub cell: [i32; 3],
    /// The cell in front of the face that was hit.
    pub prev: [i32; 3],
    pub dist: f32,
    pub point: [f64; 3],
}

/// A distant-terrain tile.
#[derive(Default)]
pub struct FarTile {
    pub mesh: Option<GpuMesh>,
    requested: bool,
}

pub struct World {
    pub seed: u32,
    pub chunks: BTreeMap<(i32, i32), Chunk>,
    /// Distant terrain, keyed by tile coordinate (see `lod`).
    pub far: BTreeMap<(i32, i32), FarTile>,
    /// How far distant terrain reaches, in chunks (0 = off).
    pub far_radius: i32,
    far_lod: (i32, i32, i32),
    /// How the distant terrain is made (see `Settings`): tiles within this
    /// many tiles are made from real blocks; trees 0..=2; sampling detail 0..=2.
    pub far_real: i32,
    pub far_trees: i32,
    pub far_detail: i32,
    far_in_flight: usize,
    far_center: (i32, i32, i32),
    /// Multiplayer: block changes made by others, by chunk, applied to
    /// chunks as they are generated: (local x, y, local z, block).
    pub overlay: BTreeMap<(i32, i32), Vec<(i32, i32, i32, Block)>>,
    /// Multiplayer: changes made here that still have to be sent.
    pub edit_log: Vec<(i32, i32, i32, Block)>,
    pub log_edits: bool,
    /// Cells that changed lately, for whoever makes sand fall and crops pop.
    pub changed: Vec<[i32; 3]>,
    /// Finished `Job::Task`s: tag, what was asked, and the answer. Whoever
    /// asked takes its own out.
    pub task_results: Vec<(u32, Vec<u8>, Vec<u8>)>,
    /// New chunks and placed blocks, for the mod hooks (see `world_log`).
    pub log: crate::terrain::world_log::WorldLog,
    offsets: Vec<(i32, i32)>,
    offsets_radius: i32,
    urgent: Vec<(i32, i32)>,
    /// Nothing left to load or mesh around the current centre.
    pub settled: bool,
    last_center: (i32, i32, i32),
}

impl World {
    pub fn new(seed: u32) -> World {
        World {
            seed,
            chunks: BTreeMap::new(),
            far: BTreeMap::new(),
            far_radius: 0,
            far_lod: (-1, -1, -1),
            far_real: 3,
            far_trees: 2,
            far_detail: 1,
            far_in_flight: 0,
            far_center: (i32::MIN, 0, 0),
            overlay: BTreeMap::new(),
            edit_log: Vec::new(),
            changed: Vec::new(),
            log_edits: false,
            task_results: Vec::new(),
            log: Default::default(),
            offsets: Vec::new(),
            offsets_radius: -1,
            urgent: Vec::new(),
            settled: false,
            last_center: (i32::MIN, 0, 0),
        }
    }

    /// The block at a world position; air where nothing is loaded.
    pub fn get(&self, x: i32, y: i32, z: i32) -> Block {
        if !(0..CH).contains(&y) {
            return AIR;
        }
        match self.chunks.get(&(chunk_of(x), chunk_of(z))).and_then(|c| c.data.as_ref()) {
            Some(d) => d.get(x & 15, y, z & 15),
            None => AIR,
        }
    }

    /// How many of the chunks within `radius` of (ccx, ccz) are loaded and
    /// meshed, and how many there are in all. Only chunks the world really
    /// loads count: with a short view `distance` the corners of the square
    /// are never loaded, and waiting for them would never end.
    pub fn progress(&self, ccx: i32, ccz: i32, radius: i32, distance: i32) -> (usize, usize) {
        let reach = distance * distance + distance;
        let (mut done, mut total) = (0, 0);
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                if dx * dx + dz * dz > reach {
                    continue;
                }
                total += 1;
                if let Some(c) = self.chunks.get(&(ccx + dx, ccz + dz)) {
                    if c.data.is_some() && !c.dirty && !c.mesh_in_flight && c.mesh.is_some() {
                        done += 1;
                    }
                }
            }
        }
        (done, total)
    }

    pub fn is_loaded(&self, x: i32, z: i32) -> bool {
        self.chunks.get(&(chunk_of(x), chunk_of(z))).map_or(false, |c| c.data.is_some())
    }

    /// Like [`World::get`], but unloaded space and the world's floor are
    /// solid, so the player cannot fall out of the world.
    pub fn get_for_physics(&self, x: i32, y: i32, z: i32) -> Block {
        if y < 0 || (y < CH && !self.is_loaded(x, z)) {
            return reg().wk.deepstone;
        }
        self.get(x, y, z)
    }

    /// Change a block. Returns false if that part of the world is not loaded.
    pub fn set_block(&mut self, x: i32, y: i32, z: i32, b: Block) -> bool {
        let changed = self.get(x, y, z) != b;
        let ok = self.set_block_inner(x, y, z, b);
        if ok && changed && self.changed.len() < 8192 {
            self.changed.push([x, y, z]);
        }
        if ok && changed {
            let why = self.log.cause;
            self.log.note_placed([x, y, z], b, why);
        }
        if ok && changed && self.log_edits {
            self.edit_log.push((x, y, z, b));
            self.remember(x, y, z, b);
        }
        ok
    }

    /// As [`World::set_block`], noting `why` (see `world_log::cause`) as the
    /// cause for `on_block_placed`.
    pub fn set_block_cause(&mut self, why: &'static str, x: i32, y: i32, z: i32, b: Block) -> bool {
        let before = core::mem::replace(&mut self.log.cause, why);
        let ok = self.set_block(x, y, z, b);
        self.log.cause = before;
        ok
    }

    fn remember(&mut self, x: i32, y: i32, z: i32, b: Block) {
        let list = self.overlay.entry((chunk_of(x), chunk_of(z))).or_default();
        list.retain(|e| (e.0, e.1, e.2) != (x & 15, y, z & 15));
        list.push((x & 15, y, z & 15, b));
    }

    /// A change made by another player: remembered for chunks not loaded
    /// yet, applied now where the chunk is here.
    pub fn apply_remote(&mut self, x: i32, y: i32, z: i32, b: Block) {
        if !(0..CH).contains(&y) || b as usize >= reg().blocks.len() {
            return;
        }
        self.remember(x, y, z, b);
        let before = self.get(x, y, z);
        if self.set_block_inner(x, y, z, b) {
            if self.changed.len() < 8192 {
                self.changed.push([x, y, z]);
            }
            if before != b {
                self.log.note_placed([x, y, z], b, crate::terrain::world_log::cause::REMOTE);
            }
        }
    }

    fn set_block_inner(&mut self, x: i32, y: i32, z: i32, b: Block) -> bool {
        if !(0..CH).contains(&y) {
            return false;
        }
        let (cx, cz) = (chunk_of(x), chunk_of(z));
        match self.chunks.get_mut(&(cx, cz)) {
            Some(Chunk { data: Some(d), modified, .. }) => {
                if d.get(x & 15, y, z & 15) == b {
                    return true;
                }
                Arc::make_mut(d).set(x & 15, y, z & 15, b);
                *modified = true;
            }
            _ => return false,
        }
        // Light can change up to 15 blocks away, which reaches every
        // neighbouring chunk; the edited chunk goes first.
        for (dx, dz) in [(0, 0), (-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
            let key = (cx + dx, cz + dz);
            if let Some(c) = self.chunks.get_mut(&key) {
                if c.mesh.is_some() || c.dirty {
                    c.dirty = true;
                    if !self.urgent.contains(&key) {
                        self.urgent.push(key);
                    }
                }
            }
        }
        self.settled = false;
        true
    }

    /// Squared distance (in chunks) from the centre to the nearest chunk of
    /// the 3 x 3 around `(dx, dz)`: a chunk is needed if this is within the
    /// meshed circle, because meshing reads the neighbours.
    fn reach2(dx: i32, dz: i32) -> i32 {
        let (ax, az) = ((dx.abs() - 1).max(0), (dz.abs() - 1).max(0));
        ax * ax + az * az
    }

    fn neighbourhood(&self, cx: i32, cz: i32) -> Option<[Arc<ChunkData>; 9]> {
        let mut v: Vec<Arc<ChunkData>> = Vec::with_capacity(9);
        for dz in -1..=1 {
            for dx in -1..=1 {
                v.push(self.chunks.get(&(cx + dx, cz + dz))?.data.as_ref()?.clone());
            }
        }
        v.try_into().ok()
    }

    fn try_mesh(&mut self, key: (i32, i32), pool: &mut Pool, urgent: bool) -> bool {
        match self.chunks.get(&key) {
            Some(c) if c.dirty && !c.mesh_in_flight && c.data.is_some() => {}
            _ => return false,
        }
        let n = match self.neighbourhood(key.0, key.1) {
            Some(n) => n,
            None => return false,
        };
        let c = self.chunks.get_mut(&key).unwrap();
        c.dirty = false;
        c.mesh_in_flight = true;
        pool.submit(Job::Mesh { cx: key.0, cz: key.1, n }, urgent);
        true
    }

    /// Advance loading around the chunk `(ccx, ccz)`: take finished work,
    /// drop far chunks, and queue what is missing, nearest first.
    pub fn update(&mut self, ccx: i32, ccz: i32, radius: i32, pool: &mut Pool, sink: &mut dyn MeshSink, store: &mut dyn ChunkStore) {
        let center = (ccx, ccz, radius);
        let moved = center != self.last_center;
        if moved {
            self.last_center = center;
            self.settled = false;
        }

        // 1. Finished work.
        for result in pool.drain(2) {
            self.settled = false;
            match result {
                JobResult::Gen { cx, cz, mut data } => {
                    if let Some(edits) = self.overlay.get(&(cx, cz)) {
                        for &(x, y, z, b) in edits {
                            data.set(x, y, z, b);
                        }
                    }
                    if let Some(c) = self.chunks.get_mut(&(cx, cz)) {
                        if c.data.is_none() {
                            c.data = Some(Arc::new(data));
                            c.dirty = true;
                            self.log.note_generated(cx, cz);
                        }
                    }
                }
                JobResult::Task { tag, arg, out } => self.task_results.push((tag, arg, out)),
                JobResult::Far { tx, tz, mesh } => {
                    self.far_in_flight = self.far_in_flight.saturating_sub(1);
                    if let Some(t) = self.far.get_mut(&(tx, tz)) {
                        if let Some(old) = t.mesh.take() {
                            sink.free(old);
                        }
                        t.mesh = Some(sink.upload(&mesh));
                    }
                }
                JobResult::Mesh { cx, cz, mesh } => {
                    if let Some(c) = self.chunks.get_mut(&(cx, cz)) {
                        c.mesh_in_flight = false;
                        if let Some(old) = c.mesh.take() {
                            sink.free(old);
                        }
                        c.mesh = Some(sink.upload(&mesh));
                        c.props = mesh.props;
                        c.props.shrink_to_fit();
                    }
                }
            }
        }

        // 2. Edits are re-meshed ahead of everything else.
        if !self.urgent.is_empty() {
            let urgent = core::mem::take(&mut self.urgent);
            for key in urgent {
                self.try_mesh(key, pool, true);
            }
        }
        if self.settled {
            return;
        }

        // 3. Forget chunks that are far away. A chunk is kept while it is, or
        // touches, one that gets a mesh (see `needed`), plus a margin so that
        // walking back and forth at the edge does not load and drop the same
        // chunks again and again.
        if moved {
            let keep = (radius + 2) * (radius + 2);
            let gone: Vec<(i32, i32)> =
                self.chunks.keys().filter(|k| Self::reach2(k.0 - ccx, k.1 - ccz) > keep || (k.0 - ccx).abs() > radius + 3 || (k.1 - ccz).abs() > radius + 3).copied().collect();
            for key in gone {
                if let Some(c) = self.chunks.remove(&key) {
                    if let (true, Some(d)) = (c.modified, &c.data) {
                        store.save(key.0, key.1, d);
                    }
                    if let Some(m) = c.mesh {
                        sink.free(m);
                    }
                }
            }
        }

        // 4. Queue missing chunks and meshes, nearest first.
        if self.offsets_radius != radius {
            self.offsets_radius = radius;
            let r = radius + 1;
            self.offsets.clear();
            for dx in -r..=r {
                for dz in -r..=r {
                    self.offsets.push((dx, dz));
                }
            }
            self.offsets.sort_by_key(|o| o.0 * o.0 + o.1 * o.1);
        }
        let cap = (pool.thread_count() * 3).max(4);
        let mesh_r2 = radius * radius + radius;
        let mut loads = 0;
        let mut waiting = false;
        for i in 0..self.offsets.len() {
            if pool.in_flight >= cap {
                waiting = true;
                break;
            }
            let (dx, dz) = self.offsets[i];
            // The corners of the square are not needed: only what gets a mesh and its neighbours.
            if Self::reach2(dx, dz) > mesh_r2 {
                continue;
            }
            let key = (ccx + dx, ccz + dz);
            let c = self.chunks.entry(key).or_default();
            if c.data.is_none() {
                if !c.gen_requested {
                    if loads >= 8 {
                        waiting = true;
                        continue;
                    }
                    loads += 1;
                    match store.load(key.0, key.1) {
                        Some(d) => {
                            // Liquid in a saved chunk is looked at again by the current rules.
                            for l in crate::migrate::flowing_cells(&d, 400) {
                                self.changed.push([key.0 * 16 + l[0], l[1], key.1 * 16 + l[2]]);
                            }
                            c.data = Some(Arc::new(d));
                            c.dirty = true;
                            c.modified = false;
                        }
                        None => {
                            c.gen_requested = true;
                            pool.submit(Job::Gen { cx: key.0, cz: key.1 }, false);
                        }
                    }
                }
                waiting = true;
                continue;
            }
            if dx * dx + dz * dz <= mesh_r2 && (c.dirty || c.mesh_in_flight) {
                waiting = true;
                self.try_mesh(key, pool, false);
            }
        }
        let far_waiting = self.update_far(ccx, ccz, pool, sink, waiting);
        self.settled = !waiting && !far_waiting && pool.in_flight == 0;
    }

    /// Request and drop distant-terrain tiles. Detailed chunks come first:
    /// tiles are only queued while the workers have room. Returns whether
    /// any tile is still missing.
    fn update_far(&mut self, ccx: i32, ccz: i32, pool: &mut Pool, sink: &mut dyn MeshSink, busy: bool) -> bool {
        use crate::lod::{step_with, TILE_CHUNKS};
        let (tcx, tcz) = (ccx.div_euclid(TILE_CHUNKS), ccz.div_euclid(TILE_CHUNKS));
        // A change of how distant terrain is made: start it afresh.
        let lod = (self.far_real, self.far_trees, self.far_detail);
        if lod != self.far_lod {
            self.far_lod = lod;
            for (_, t) in core::mem::take(&mut self.far) {
                if let Some(m) = t.mesh {
                    sink.free(m);
                }
            }
            self.far_center = (i32::MIN, 0, 0);
        }
        let reach = (self.far_radius + TILE_CHUNKS - 1) / TILE_CHUNKS;
        let center = (tcx, tcz, reach);
        if center != self.far_center {
            self.far_center = center;
            // Drop what is out of range (everything, when turned off).
            let gone: Vec<(i32, i32)> = self
                .far
                .keys()
                .filter(|k| self.far_radius <= 0 || (k.0 - tcx).abs() > reach + 1 || (k.1 - tcz).abs() > reach + 1)
                .copied()
                .collect();
            for key in gone {
                if let Some(m) = self.far.remove(&key).and_then(|t| t.mesh) {
                    sink.free(m);
                }
            }
        }
        if self.far_radius <= 0 {
            return false;
        }
        let cap = (pool.thread_count() * 3).max(4);
        let limit = if busy { 1 } else { 4 };
        let mut missing = false;
        // Nearest tiles first: walk outward ring by ring.
        'rings: for ring in 0..=reach {
            for dx in -ring..=ring {
                for dz in -ring..=ring {
                    if dx.abs().max(dz.abs()) != ring || dx * dx + dz * dz > reach * reach {
                        continue;
                    }
                    let t = self.far.entry((tcx + dx, tcz + dz)).or_default();
                    if t.mesh.is_some() {
                        continue;
                    }
                    missing = true;
                    if t.requested {
                        continue;
                    }
                    if self.far_in_flight >= limit || pool.in_flight >= cap {
                        break 'rings;
                    }
                    t.requested = true;
                    self.far_in_flight += 1;
                    let (tx, tz) = (tcx + dx, tcz + dz);
                    // Near the player: made from the real blocks, with the player's edits.
                    let real = ring <= self.far_real;
                    let edits = if real {
                        let mut v = Vec::with_capacity(16);
                        for i in 0..TILE_CHUNKS {
                            for j in 0..TILE_CHUNKS {
                                v.push(self.overlay.get(&(tx * TILE_CHUNKS + i, tz * TILE_CHUNKS + j)).cloned().unwrap_or_default());
                            }
                        }
                        v
                    } else {
                        Vec::new()
                    };
                    pool.submit(Job::Far { tx, tz, step: step_with(ring, self.far_detail), real, trees: self.far_trees.clamp(0, 2) as u8, edits }, false);
                }
            }
        }
        missing
    }

    /// The content files changed: translate every loaded block from its
    /// old state number to its new one (`map[old]`), and rebuild all meshes.
    pub fn remap(&mut self, map: &[Block], sink: &mut dyn MeshSink) {
        let fix = |b: Block| map.get(b as usize).copied().unwrap_or(AIR);
        for c in self.chunks.values_mut() {
            if let Some(d) = c.data.as_mut() {
                for b in Arc::make_mut(d).blocks.iter_mut() {
                    *b = fix(*b);
                }
            }
            c.dirty = c.data.is_some();
        }
        for list in self.overlay.values_mut() {
            for e in list.iter_mut() {
                e.3 = fix(e.3);
            }
        }
        // Distant terrain is cheap to make again with the new textures.
        for t in core::mem::take(&mut self.far).into_values() {
            if let Some(m) = t.mesh {
                sink.free(m);
            }
        }
        self.far_center = (i32::MIN, 0, 0);
        self.settled = false;
    }

    /// Save every chunk the player changed and mark them clean.
    pub fn save_all(&mut self, store: &mut dyn ChunkStore) {
        for (key, c) in self.chunks.iter_mut() {
            if let (true, Some(d)) = (c.modified, &c.data) {
                store.save(key.0, key.1, d);
                c.modified = false;
            }
        }
    }

    /// Release every mesh (when the graphics context goes away).
    pub fn free_meshes(&mut self, sink: &mut dyn MeshSink) {
        for t in self.far.values_mut() {
            if let Some(m) = t.mesh.take() {
                sink.free(m);
            }
        }
        for c in self.chunks.values_mut() {
            if let Some(m) = c.mesh.take() {
                sink.free(m);
            }
        }
    }

    /// Find the first block along a ray. Liquids and air are passed through;
    /// partial blocks are tested against their actual boxes.
    pub fn raycast(&self, origin: [f64; 3], dir: Vec3, reach: f32) -> Option<Hit> {
        let d = [dir.x as f64, dir.y as f64, dir.z as f64];
        let mut cell = [floord(origin[0]), floord(origin[1]), floord(origin[2])];
        let mut step = [0i32; 3];
        let mut t_max = [f64::INFINITY; 3];
        let mut t_delta = [f64::INFINITY; 3];
        for a in 0..3 {
            if d[a] > 1e-9 {
                step[a] = 1;
                t_max[a] = (cell[a] as f64 + 1.0 - origin[a]) / d[a];
                t_delta[a] = 1.0 / d[a];
            } else if d[a] < -1e-9 {
                step[a] = -1;
                t_max[a] = (cell[a] as f64 - origin[a]) / d[a];
                t_delta[a] = -1.0 / d[a];
            }
        }
        let reach = reach as f64;
        let mut t_enter = 0.0f64;
        let mut normal = [0i32; 3];
        let r = reg();
        let mut shape = Vec::new();
        loop {
            let b = self.get(cell[0], cell[1], cell[2]);
            let kind = r.kind[b as usize];
            let mut found: Option<(f64, [i32; 3])> = None;
            match kind {
                Kind::Air | Kind::Liquid => {}
                Kind::Boxes => {
                    let mask = block::connect_mask(b, &|dx, dz| self.get(cell[0] + dx, cell[1], cell[2] + dz));
                    block::shape(b, mask, &mut shape);
                    for bx in &shape {
                        if let Some((t, n)) = ray_box(origin, d, cell, bx.min, bx.max) {
                            if t <= reach && found.map_or(true, |(ft, _)| t < ft) {
                                found = Some((t, n));
                            }
                        }
                    }
                }
                // A full block the ray starts inside of cannot be aimed at.
                _ => {
                    if normal != [0, 0, 0] {
                        found = Some((t_enter, normal));
                    }
                }
            }
            if let Some((t, n)) = found {
                return Some(Hit {
                    cell,
                    prev: [cell[0] + n[0], cell[1] + n[1], cell[2] + n[2]],
                    dist: t as f32,
                    point: [origin[0] + d[0] * t, origin[1] + d[1] * t, origin[2] + d[2] * t],
                });
            }
            let a = if t_max[0] <= t_max[1] && t_max[0] <= t_max[2] {
                0
            } else if t_max[1] <= t_max[2] {
                1
            } else {
                2
            };
            t_enter = t_max[a];
            if t_enter > reach || step[a] == 0 {
                return None;
            }
            cell[a] += step[a];
            t_max[a] += t_delta[a];
            normal = [0, 0, 0];
            normal[a] = -step[a];
        }
    }
}

/// Where a ray enters a box inside `cell` (box corners in 1/16ths), and the
/// outward normal of the face it enters through.
fn ray_box(o: [f64; 3], d: [f64; 3], cell: [i32; 3], min: [u8; 3], max: [u8; 3]) -> Option<(f64, [i32; 3])> {
    let mut t_near = f64::NEG_INFINITY;
    let mut t_far = f64::INFINITY;
    let mut normal = [0i32; 3];
    for a in 0..3 {
        let lo = cell[a] as f64 + min[a] as f64 / 16.0;
        let hi = cell[a] as f64 + max[a] as f64 / 16.0;
        if d[a] > -1e-9 && d[a] < 1e-9 {
            if o[a] < lo || o[a] > hi {
                return None;
            }
            continue;
        }
        let (mut t1, mut t2) = ((lo - o[a]) / d[a], (hi - o[a]) / d[a]);
        let mut n = -1;
        if t1 > t2 {
            core::mem::swap(&mut t1, &mut t2);
            n = 1;
        }
        if t1 > t_near {
            t_near = t1;
            normal = [0, 0, 0];
            normal[a] = n;
        }
        if t2 < t_far {
            t_far = t2;
        }
    }
    if t_near > t_far || t_far < 0.0 || t_near < 0.0 {
        return None;
    }
    Some((t_near, normal))
}

/// Chunk coordinate of a world position.
pub fn chunk_at(x: f64, z: f64) -> (i32, i32) {
    (floord(x).div_euclid(CW), floord(z).div_euclid(CW))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct CountSink {
        live: i32,
        uploads: u32,
    }
    impl MeshSink for CountSink {
        fn upload(&mut self, m: &MeshData) -> GpuMesh {
            self.live += 1;
            self.uploads += 1;
            GpuMesh { opaque_count: m.opaque.len() as i32, ..Default::default() }
        }
        fn free(&mut self, _: GpuMesh) {
            self.live -= 1;
        }
    }

    fn settle(w: &mut World, cx: i32, cz: i32, r: i32, pool: &mut Pool, sink: &mut CountSink) {
        for _ in 0..5000 {
            w.update(cx, cz, r, pool, sink, &mut NullStore);
            if w.settled {
                return;
            }
        }
        panic!("world never settled");
    }

    #[test]
    fn loads_meshes_edits_and_unloads() {
        let mut w = World::new(11);
        let mut pool = Pool::new(0, 11);
        let mut sink = CountSink::default();
        settle(&mut w, 0, 0, 2, &mut pool, &mut sink);
        // Radius 2: every chunk within the circle has a mesh.
        let meshed = w.chunks.values().filter(|c| c.mesh.is_some()).count();
        assert_eq!(meshed, 21);
        assert_eq!(sink.live, 21);
        assert!(w.chunks.values().filter(|c| c.mesh.is_some()).any(|c| c.mesh.unwrap().opaque_count > 0));

        // An edit re-meshes the chunk and its neighbours, and nothing leaks.
        let before = sink.uploads;
        let lamp = reg().block("lamp").unwrap();
        assert!(w.set_block(8, 100, 8, lamp));
        assert_eq!(w.get(8, 100, 8), lamp);
        settle(&mut w, 0, 0, 2, &mut pool, &mut sink);
        assert_eq!(sink.uploads - before, 9);
        assert_eq!(sink.live, 21);

        // Walking far away drops the old chunks.
        settle(&mut w, 40, 0, 2, &mut pool, &mut sink);
        assert!(!w.is_loaded(8, 8));
        assert_eq!(sink.live, 21);
        assert!(!w.set_block(8, 100, 8, lamp));
    }

    #[test]
    fn distant_terrain_loads_around_the_player_and_follows_them() {
        let mut w = World::new(11);
        let mut pool = Pool::new(0, 11);
        let mut sink = CountSink::default();
        w.far_radius = 16; // four tiles
        settle(&mut w, 0, 0, 2, &mut pool, &mut sink);
        let tiles = w.far.values().filter(|t| t.mesh.is_some()).count();
        // Every tile within four tiles of the centre: a disc of radius 4.
        assert_eq!(tiles, 49);
        assert_eq!(sink.live as usize, 21 + tiles);
        // Far away, the old tiles are dropped and new ones made.
        settle(&mut w, 400, 0, 2, &mut pool, &mut sink);
        assert_eq!(w.far.values().filter(|t| t.mesh.is_some()).count(), 49);
        assert!(!w.far.contains_key(&(0, 0)));
        assert_eq!(sink.live as usize, 21 + 49);
        // Turning it off frees everything.
        w.far_radius = 0;
        settle(&mut w, 400, 0, 2, &mut pool, &mut sink);
        w.update(401, 0, 2, &mut pool, &mut sink, &mut NullStore);
        assert!(w.far.is_empty());
    }

    #[test]
    fn remapping_translates_every_block_and_remeshes() {
        let mut w = World::new(11);
        let mut pool = Pool::new(0, 11);
        let mut sink = CountSink::default();
        settle(&mut w, 0, 0, 1, &mut pool, &mut sink);
        let (brick, lamp) = (reg().block("brick").unwrap(), reg().block("lamp").unwrap());
        w.set_block(3, 100, 3, brick);
        settle(&mut w, 0, 0, 1, &mut pool, &mut sink);
        // New content in which brick has become lamp's number, and one
        // state (the last) no longer exists.
        let n = reg().blocks.len();
        let mut map: Vec<Block> = (0..n as Block).collect();
        map[brick as usize] = lamp;
        map.truncate(n - 1);
        let uploads = sink.uploads;
        w.remap(&map, &mut sink);
        assert_eq!(w.get(3, 100, 3), lamp);
        settle(&mut w, 0, 0, 1, &mut pool, &mut sink);
        assert!(sink.uploads > uploads, "chunks are drawn again");
        assert_eq!(sink.live as usize, w.chunks.values().filter(|c| c.mesh.is_some()).count() + w.far.values().filter(|t| t.mesh.is_some()).count());
    }

    #[test]
    fn multiplayer_edits_are_logged_and_remote_ones_wait_for_their_chunk() {
        let mut w = World::new(11);
        let mut pool = Pool::new(0, 11);
        let mut sink = CountSink::default();
        let brick = reg().block("brick").unwrap();
        w.log_edits = true;
        // A remote edit far away, before that chunk exists here.
        w.apply_remote(805, 100, 3, brick);
        assert_eq!(w.get(805, 100, 3), AIR);
        settle(&mut w, 0, 0, 1, &mut pool, &mut sink);
        // Local edits are logged once; remote ones and no-ops are not.
        w.set_block(3, 100, 3, brick);
        w.set_block(3, 100, 3, brick);
        w.apply_remote(4, 100, 3, brick);
        assert_eq!(w.edit_log, [(3, 100, 3, brick)]);
        assert_eq!(w.get(4, 100, 3), brick);
        // Arriving there, the waiting edit is in place.
        settle(&mut w, 50, 0, 1, &mut pool, &mut sink);
        assert_eq!(w.get(805, 100, 3), brick);
        // Coming back, both the local and the remote edit are regenerated.
        settle(&mut w, 0, 0, 1, &mut pool, &mut sink);
        assert_eq!((w.get(3, 100, 3), w.get(4, 100, 3)), (brick, brick));
    }

    #[test]
    fn changed_chunks_come_back_from_the_store() {
        struct OneChunk(Option<ChunkData>);
        impl ChunkStore for OneChunk {
            fn load(&mut self, cx: i32, cz: i32) -> Option<ChunkData> {
                if (cx, cz) == (0, 0) {
                    self.0.clone()
                } else {
                    None
                }
            }
            fn save(&mut self, cx: i32, cz: i32, d: &ChunkData) {
                assert_eq!((cx, cz), (0, 0));
                self.0 = Some(d.clone());
            }
        }
        let mut store = OneChunk(None);
        let mut pool = Pool::new(0, 3);
        let mut sink = CountSink::default();
        let brick = reg().block("brick").unwrap();
        {
            let mut w = World::new(3);
            for _ in 0..2000 {
                w.update(0, 0, 1, &mut pool, &mut sink, &mut store);
            }
            w.set_block(5, 120, 5, brick);
            w.save_all(&mut store);
        }
        let mut w = World::new(3);
        for _ in 0..2000 {
            w.update(0, 0, 1, &mut pool, &mut sink, &mut store);
        }
        assert_eq!(w.get(5, 120, 5), brick);
    }

    #[test]
    fn raycast_hits_cubes_and_respects_slab_shape() {
        let mut w = World::new(1);
        let mut pool = Pool::new(0, 1);
        let mut sink = CountSink::default();
        settle(&mut w, 0, 0, 1, &mut pool, &mut sink);
        let r = reg();
        let down = Vec3::new(0.0, -1.0, 0.0);
        w.set_block(4, 120, 4, r.wk.stone);
        let hit = w.raycast([4.5, 125.0, 4.5], down, 8.0).unwrap();
        assert_eq!((hit.cell, hit.prev), ([4, 120, 4], [4, 121, 4]));
        assert!((hit.dist - 4.0).abs() < 1e-4);
        assert!(w.raycast([4.5, 125.0, 4.5], down, 3.0).is_none());

        // A bottom slab: a ray from above lands on its top at half height;
        // a level ray through the upper half misses it.
        w.set_block(6, 120, 6, r.block("stone_slab").unwrap());
        let hit = w.raycast([6.5, 122.0, 6.5], down, 8.0).unwrap();
        assert_eq!(hit.cell, [6, 120, 6]);
        assert!((hit.point[1] - 120.5).abs() < 1e-6);
        assert_eq!(hit.prev, [6, 121, 6]);
        let level = Vec3::new(1.0, 0.0, 0.0);
        assert!(w.raycast([2.5, 120.75, 6.5], level, 8.0).is_none());
        assert_eq!(w.raycast([2.5, 120.25, 6.5], level, 8.0).unwrap().cell, [6, 120, 6]);

        // Water is not a target.
        w.set_block(4, 122, 4, r.wk.water);
        assert_eq!(w.raycast([4.5, 125.0, 4.5], down, 8.0).unwrap().cell, [4, 120, 4]);
    }
}
