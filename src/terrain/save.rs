//! Saving. Only chunks the player has changed are stored; everything else is
//! regenerated from the seed.
//!
//! Chunks are run-length encoded and grouped into region files of 16 x 16
//! chunks. Each region file carries the names of the block states it uses,
//! so a world still loads after the data files gain, lose or reorder blocks.

use crate::block::{reg, Block};
use crate::chunk::{ChunkData, VOLUME};
use crate::world::ChunkStore;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Run-length encode a chunk as `(count: u16, state: u16)` pairs.
pub fn encode(c: &ChunkData) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.blocks.len() {
        let b = c.blocks[i];
        let mut n = 1;
        while i + n < c.blocks.len() && c.blocks[i + n] == b && n < 0xFFFF {
            n += 1;
        }
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(&b.to_le_bytes());
        i += n;
    }
    out
}

/// Decode a chunk. `remap[saved]` gives the current state for a saved state
/// number; states outside the table become air. `None` on corrupt data.
pub fn decode(d: &[u8], remap: &[Block]) -> Option<ChunkData> {
    if d.len() % 4 != 0 {
        return None;
    }
    let mut blocks = Vec::with_capacity(VOLUME);
    for run in d.chunks_exact(4) {
        let n = u16::from_le_bytes([run[0], run[1]]) as usize;
        let saved = u16::from_le_bytes([run[2], run[3]]) as usize;
        if n == 0 || blocks.len() + n > VOLUME {
            return None;
        }
        let b = remap.get(saved).copied().unwrap_or(0);
        blocks.resize(blocks.len() + n, b);
    }
    if blocks.len() != VOLUME {
        return None;
    }
    Some(ChunkData { blocks })
}

/// The names of every current block state, in state order.
pub fn current_palette() -> Vec<String> {
    let r = reg();
    (0..r.blocks.len()).map(|i| r.state_name(i as Block)).collect()
}

const MAGIC: &[u8; 4] = b"VXR2";
/// Chunks per region edge.
pub const REGION: i32 = 16;
const SLOTS: usize = (REGION * REGION) as usize;

pub struct Region {
    /// Encoded chunks, in current state numbers.
    blobs: Vec<Option<Vec<u8>>>,
    dirty: bool,
}

impl Region {
    fn empty() -> Region {
        Region { blobs: (0..SLOTS).map(|_| None).collect(), dirty: false }
    }

    /// Parse a region file, translating its block states to the current ones.
    pub fn from_bytes(d: &[u8]) -> Option<Region> {
        if d.len() < 8 || &d[0..4] != MAGIC {
            return None;
        }
        let pal_len = u32::from_le_bytes([d[4], d[5], d[6], d[7]]) as usize;
        let table = 8 + pal_len;
        let mut at = table.checked_add(SLOTS * 4)?;
        if d.len() < at {
            return None;
        }
        let r = reg();
        let names = core::str::from_utf8(&d[8..table]).ok()?;
        let remap: Vec<Block> = names.split('\n').map(|n| r.state_by_name(n).unwrap_or(0)).collect();
        let identity = remap.len() == r.blocks.len() && remap.iter().enumerate().all(|(i, &b)| b as usize == i);
        let mut region = Region::empty();
        for slot in 0..SLOTS {
            let o = table + slot * 4;
            let len = u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]) as usize;
            if len == 0 {
                continue;
            }
            let blob = d.get(at..at.checked_add(len)?)?;
            at += len;
            if identity {
                region.blobs[slot] = Some(blob.to_vec());
            } else if let Some(c) = decode(blob, &remap) {
                // Stored with other state numbers: rewrite in the current ones.
                region.blobs[slot] = Some(encode(&c));
                region.dirty = true;
            }
        }
        Some(region)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let pal = current_palette().join("\n");
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(pal.len() as u32).to_le_bytes());
        out.extend_from_slice(pal.as_bytes());
        for b in &self.blobs {
            let len = b.as_ref().map_or(0, |b| b.len()) as u32;
            out.extend_from_slice(&len.to_le_bytes());
        }
        for b in self.blobs.iter().flatten() {
            out.extend_from_slice(b);
        }
        out
    }
}

// ------------------------------------------------------------- world bundles

const BUNDLE_MAGIC: &[u8; 4] = b"VXW1";

/// Pack named files into one: `VXW1`, then for each file a name length
/// (u16), the name, a data length (u32) and the data. Chunk data inside is
/// already run-length encoded, so a bundle is about as small as the world.
pub fn bundle(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(BUNDLE_MAGIC);
    for (name, data) in files {
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// The inverse of [`bundle`]. File names are checked: only the plain
/// suffixes a world is made of are accepted, never a path.
pub fn unbundle(d: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    unbundle_with(d, true)
}

/// A bundle whose names are labels rather than file names (a server's
/// content packs: `pack:p0/pack.txt`). Nothing in it is written to disk
/// under its name, so more characters are allowed; `..` still is not.
pub fn unbundle_labels(d: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    unbundle_with(d, false)
}

fn unbundle_with(d: &[u8], file_names: bool) -> Option<Vec<(String, Vec<u8>)>> {
    if d.len() < 4 || &d[..4] != BUNDLE_MAGIC {
        return None;
    }
    let mut at = 4;
    let mut files = Vec::new();
    while at < d.len() {
        let n = u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?) as usize;
        let name = core::str::from_utf8(d.get(at + 2..at + 2 + n)?).ok()?;
        at += 2 + n;
        let len = u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?) as usize;
        let data = d.get(at + 4..(at + 4).checked_add(len)?)?;
        at += 4 + len;
        let safe = if file_names {
            !name.is_empty() && name.len() <= 40 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        } else {
            !name.is_empty() && name.len() <= 300 && name.chars().all(|c| c.is_ascii_graphic() || c == ' ')
        };
        if !safe || name.starts_with('.') || name.contains("..") || files.len() > 100_000 {
            return None;
        }
        files.push((String::from(name), data.to_vec()));
    }
    Some(files)
}

/// The suffixes of the small files every world has besides its regions.
pub const WORLD_FILES: [&str; 6] = ["meta.txt", "chests.txt", "vehicles.txt", "regions.txt", "explored.txt", "pets.txt"];

/// Gather a world's files into one bundle. `None` if there is no such world.
pub fn export_world(io: &mut dyn FileIo, base: &str, name: &str) -> Option<Vec<u8>> {
    let mut files = Vec::new();
    for suffix in WORLD_FILES {
        if let Some(data) = io.read(&alloc::format!("{}{}.{}", base, name, suffix)) {
            files.push((String::from(suffix), data));
        }
    }
    if !files.iter().any(|f| f.0 == "meta.txt") {
        return None;
    }
    // The index lists which region files exist ("x z" per line).
    let index = files.iter().find(|f| f.0 == "regions.txt").map(|f| String::from_utf8_lossy(&f.1).into_owned()).unwrap_or_default();
    for line in index.lines() {
        let mut w = line.split(' ');
        if let (Some(Ok(rx)), Some(Ok(rz))) = (w.next().map(|v| v.parse::<i32>()), w.next().map(|v| v.parse::<i32>())) {
            let suffix = alloc::format!("r.{}.{}.vxr", rx, rz);
            if let Some(data) = io.read(&alloc::format!("{}{}.{}", base, name, suffix)) {
                files.push((suffix, data));
            }
        }
    }
    // Each dimension keeps its own index, vehicles and regions.
    for (dim, _) in &reg().dimensions {
        for suffix in ["regions.txt", "vehicles.txt"] {
            let name_in = alloc::format!("{}.{}", dim, suffix);
            if let Some(data) = io.read(&alloc::format!("{}{}.{}", base, name, name_in)) {
                if suffix == "regions.txt" {
                    for line in String::from_utf8_lossy(&data).lines() {
                        let mut w = line.split(' ');
                        if let (Some(Ok(rx)), Some(Ok(rz))) = (w.next().map(|v| v.parse::<i32>()), w.next().map(|v| v.parse::<i32>())) {
                            let region = alloc::format!("{}.r.{}.{}.vxr", dim, rx, rz);
                            if let Some(bytes) = io.read(&alloc::format!("{}{}.{}", base, name, region)) {
                                files.push((region, bytes));
                            }
                        }
                    }
                }
                files.push((name_in, data));
            }
        }
    }
    Some(bundle(&files))
}

/// Unpack a bundle as the world `name`. Returns false if the bundle is
/// damaged or a file cannot be written.
pub fn import_world(io: &mut dyn FileIo, base: &str, name: &str, data: &[u8]) -> bool {
    let files = match unbundle(data) {
        Some(f) if f.iter().any(|f| f.0 == "meta.txt") => f,
        _ => return false,
    };
    files.iter().all(|(suffix, bytes)| io.write(&alloc::format!("{}{}.{}", base, name, suffix), bytes))
}

pub trait FileIo {
    fn read(&mut self, path: &str) -> Option<Vec<u8>>;
    fn write(&mut self, path: &str, data: &[u8]) -> bool;
}

/// Files on disk, through SDL.
pub struct DiskIo;

impl FileIo for DiskIo {
    fn read(&mut self, path: &str) -> Option<Vec<u8>> {
        crate::sys::read_file(path)
    }
    fn write(&mut self, path: &str, data: &[u8]) -> bool {
        crate::sys::write_file(path, data)
    }
}

pub struct RegionStore<F: FileIo> {
    pub io: F,
    /// Path prefix for this world's files, e.g. `/home/me/.../world1.`
    prefix: String,
    regions: BTreeMap<(i32, i32), Region>,
    pub write_failed: bool,
}

impl<F: FileIo> RegionStore<F> {
    pub fn new(io: F, prefix: String) -> RegionStore<F> {
        RegionStore { io, prefix, regions: BTreeMap::new(), write_failed: false }
    }

    fn path(&self, rx: i32, rz: i32) -> String {
        alloc::format!("{}r.{}.{}.vxr", self.prefix, rx, rz)
    }

    fn region(&mut self, rx: i32, rz: i32) -> &mut Region {
        if !self.regions.contains_key(&(rx, rz)) {
            let path = self.path(rx, rz);
            let region = self.io.read(&path).and_then(|d| Region::from_bytes(&d)).unwrap_or_else(Region::empty);
            self.regions.insert((rx, rz), region);
        }
        self.regions.get_mut(&(rx, rz)).unwrap()
    }

    fn slot(cx: i32, cz: i32) -> usize {
        (cx.rem_euclid(REGION) * REGION + cz.rem_euclid(REGION)) as usize
    }

    /// Write every changed region to disk and forget the unchanged ones.
    /// Write everything out and drop what is held in memory. Needed before
    /// the content changes: what is held is in the old block numbers.
    pub fn flush_and_forget(&mut self) {
        self.flush();
        self.regions.clear();
    }

    pub fn flush(&mut self) {
        let keys: Vec<(i32, i32)> = self.regions.iter().filter(|(_, r)| r.dirty).map(|(k, _)| *k).collect();
        for (rx, rz) in keys {
            let path = self.path(rx, rz);
            let bytes = self.regions[&(rx, rz)].to_bytes();
            if self.io.write(&path, &bytes) {
                self.regions.get_mut(&(rx, rz)).unwrap().dirty = false;
                // Keep the list of region files up to date: nothing else
                // can say which exist when the world is exported.
                let index_path = alloc::format!("{}regions.txt", self.prefix);
                let mut index = self.io.read(&index_path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                let line = alloc::format!("{} {}", rx, rz);
                if !index.lines().any(|l| l == line) {
                    index.push_str(&line);
                    index.push('\n');
                    self.io.write(&index_path, index.as_bytes());
                }
            } else {
                self.write_failed = true;
            }
        }
        if self.regions.len() > 32 {
            self.regions.retain(|_, r| r.dirty);
        }
    }
}

impl<F: FileIo> ChunkStore for RegionStore<F> {
    fn load(&mut self, cx: i32, cz: i32) -> Option<ChunkData> {
        let region = self.region(cx.div_euclid(REGION), cz.div_euclid(REGION));
        let blob = region.blobs[Self::slot(cx, cz)].as_ref()?;
        let identity: Vec<Block> = (0..reg().blocks.len() as Block).collect();
        decode(blob, &identity)
    }

    fn save(&mut self, cx: i32, cz: i32, data: &ChunkData) {
        let region = self.region(cx.div_euclid(REGION), cz.div_euclid(REGION));
        region.blobs[Self::slot(cx, cz)] = Some(encode(data));
        region.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MemIo(HashMap<String, Vec<u8>>);
    impl FileIo for MemIo {
        fn read(&mut self, path: &str) -> Option<Vec<u8>> {
            self.0.get(path).cloned()
        }
        fn write(&mut self, path: &str, data: &[u8]) -> bool {
            self.0.insert(path.to_string(), data.to_vec());
            true
        }
    }

    fn identity() -> Vec<Block> {
        (0..reg().blocks.len() as Block).collect()
    }

    #[test]
    fn chunk_round_trip() {
        let c = worldgen::generate(5, 2, -3);
        let e = encode(&c);
        assert!(e.len() < VOLUME, "RLE should shrink terrain, got {}", e.len());
        assert_eq!(decode(&e, &identity()).unwrap(), c);
    }

    #[test]
    fn corrupt_chunks_are_rejected() {
        let e = encode(&worldgen::generate(5, 0, 0));
        assert!(decode(&e[..e.len() - 4], &identity()).is_none());
        assert!(decode(&e[..e.len() - 1], &identity()).is_none());
        assert!(decode(&[], &identity()).is_none());
    }

    #[test]
    fn store_survives_a_restart_and_negative_coordinates() {
        let a = worldgen::generate(5, -1, -17);
        let b = worldgen::generate(5, 15, 0);
        let mut s = RegionStore::new(MemIo::default(), String::from("w."));
        s.save(-1, -17, &a);
        s.save(15, 0, &b);
        assert!(s.load(0, 0).is_none());
        s.flush();
        assert_eq!(s.io.0.len(), 3, "two regions and their index expected: {:?}", s.io.0.keys());
        let mut s2 = RegionStore::new(MemIo(s.io.0.clone()), String::from("w."));
        assert_eq!(s2.load(-1, -17).unwrap(), a);
        assert_eq!(s2.load(15, 0).unwrap(), b);
        assert!(s2.load(14, 0).is_none());
    }

    #[test]
    fn worlds_export_and_import_as_one_file() {
        let a = worldgen::generate(5, -1, -17);
        let b = worldgen::generate(5, 40, 3);
        let mut s = RegionStore::new(MemIo::default(), String::from("data/old."));
        s.save(-1, -17, &a);
        s.save(40, 3, &b);
        s.flush();
        s.io.write("data/old.meta.txt", b"seed=5\n");
        s.io.write("data/old.chests.txt", b"1 2 3 stone:4\n");
        let bundle = export_world(&mut s.io, "data/", "old").expect("export failed");
        assert!(export_world(&mut s.io, "data/", "missing").is_none());

        let mut other = MemIo::default();
        assert!(import_world(&mut other, "elsewhere/", "copy", &bundle));
        assert_eq!(other.0.get("elsewhere/copy.meta.txt").map(|v| v.as_slice()), Some(&b"seed=5\n"[..]));
        assert_eq!(other.0.len(), 5, "{:?}", other.0.keys());
        let mut s2 = RegionStore::new(other, String::from("elsewhere/copy."));
        assert_eq!(s2.load(-1, -17).unwrap(), a);
        assert_eq!(s2.load(40, 3).unwrap(), b);

        // Damaged or hostile bundles are refused whole.
        let mut io = MemIo::default();
        assert!(!import_world(&mut io, "", "x", &bundle[..bundle.len() - 5]));
        assert!(!import_world(&mut io, "", "x", b"nope"));
        let evil = super::bundle(&[(String::from("meta.txt"), alloc::vec![1]), (String::from("../../etc/passwd"), alloc::vec![2])]);
        assert!(!import_world(&mut io, "", "x", &evil));
        assert!(io.0.is_empty());
    }

    #[test]
    fn regions_are_translated_when_states_were_numbered_differently() {
        let r = reg();
        let (stone, lamp) = (r.wk.stone, r.block("lamp").unwrap());
        // A file whose palette lists only: air, lamp, something unknown, stone.
        let mut c = ChunkData::new();
        c.blocks[0] = 1; // lamp in the file's numbering
        c.blocks[1] = 2; // unknown
        c.blocks[2] = 3; // stone
        let blob = encode(&c);
        let pal = "air\nlamp\nremoved_block\nstone";
        let mut f = Vec::new();
        f.extend_from_slice(MAGIC);
        f.extend_from_slice(&(pal.len() as u32).to_le_bytes());
        f.extend_from_slice(pal.as_bytes());
        for slot in 0..SLOTS {
            f.extend_from_slice(&(if slot == 0 { blob.len() as u32 } else { 0 }).to_le_bytes());
        }
        f.extend_from_slice(&blob);
        let region = Region::from_bytes(&f).unwrap();
        assert!(region.dirty);
        let got = decode(region.blobs[0].as_ref().unwrap(), &identity()).unwrap();
        assert_eq!(&got.blocks[0..4], &[lamp, 0, stone, 0]);
        assert!(Region::from_bytes(&f[..f.len() - 3]).is_none());
    }
}
