//! Keeps the skins of the other players on the graphics card within the
//! budget (`budget.rs`): takes in each skin as the server sent it (a PNG),
//! has it unpacked at the size its player's distance asks for (`work.rs`, on
//! a worker thread when it is big), uploads it, and later makes it smaller or
//! bigger as the player goes away or comes near. The unpacked pixels live on
//! the card only: the CPU copy is dropped as soon as it is uploaded. The
//! compressed PNG of a skin bigger than 64 x 64 is kept (up to [`KEEP_BYTES`]
//! in all) so that it can be unpacked again at another size.

use super::budget::{self, Entry, GPU_BUDGET, NEAR};
use super::work::{self, Done, Job, OwnSkin};
use crate::net::skinxfer::side_of_png;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Most players whose skins are kept.
const MAX_REMOTE: usize = 256;
/// Most bytes of compressed skins kept for unpacking again.
const KEEP_BYTES: usize = 24 << 20;
/// Most unpacking jobs under way at once.
const MAX_JOBS: usize = 2;
/// Seconds between looks at who is near.
const REPLAN_EVERY: f32 = 0.5;
/// A player missing this long loses their skin.
const ABSENT_AFTER: f32 = 3.0;

/// The graphics card, as far as skins need it.
pub trait SkinGpu {
    fn add(&mut self, side: usize, rgba: &[u8]) -> u32;
    fn update(&mut self, tex: u32, side: usize, rgba: &[u8]);
    fn delete(&mut self, tex: u32);
}

impl SkinGpu for crate::render::Renderer {
    fn add(&mut self, side: usize, rgba: &[u8]) -> u32 {
        self.add_skin_texture(side, rgba)
    }
    fn update(&mut self, tex: u32, side: usize, rgba: &[u8]) {
        self.update_skin_texture(tex, side, rgba)
    }
    fn delete(&mut self, tex: u32) {
        self.delete_textures(&[tex]);
    }
}

struct Remote {
    /// The side the player made it.
    full: usize,
    /// The PNG as received; empty once dropped.
    png: Vec<u8>,
    /// Side on the card (0 = not yet) and of the job under way (0 = none).
    cur: usize,
    want: usize,
    /// Told to jobs, so that the answer to an old one is not taken for the new skin's.
    gen: u32,
    last_near: u64,
    absent: f32,
}

#[derive(Default)]
pub struct SkinHub {
    remote: BTreeMap<u16, Remote>,
    gen: u32,
    tick_no: u64,
    timer: f32,
    dirty: bool,
    inflight: usize,
    own_gen: u32,
    own_ready: Option<OwnSkin>,
    /// The custom skin file being unpacked on a worker (generation), and its answer.
    load_gen: u32,
    loading: bool,
    loaded: Option<Option<(usize, Vec<u8>)>>,
}

impl SkinHub {
    /// Pack the player's own skin (off the main thread if it is big); the
    /// result comes out of [`SkinHub::take_own`] after a while. The pixels are
    /// moved in: nothing of them is kept here.
    pub fn pack_own(&mut self, side: usize, rgba: Vec<u8>) {
        self.own_gen += 1;
        self.own_ready = None;
        work::submit(Job::Own { gen: self.own_gen, side, rgba });
    }

    /// Unpack the custom skin file `png` (side `side` by its header) off the main thread; the
    /// answer comes out of [`SkinHub::take_loaded`]. A load under way is forgotten.
    pub fn load_custom(&mut self, side: usize, png: Vec<u8>) {
        self.load_gen += 1;
        self.loading = true;
        self.loaded = None;
        work::submit(Job::Load { gen: self.load_gen, side, png });
    }

    /// Forget a custom skin load under way (another skin was chosen).
    pub fn cancel_load(&mut self) {
        self.load_gen += 1;
        self.loading = false;
        self.loaded = None;
    }

    /// Whether the custom skin is still being unpacked.
    pub fn custom_loading(&self) -> bool {
        self.loading
    }

    /// The unpacked custom skin once it is: (side, pixels), or `None` inside if the file was no skin.
    pub fn take_loaded(&mut self) -> Option<Option<(usize, Vec<u8>)>> {
        self.loaded.take()
    }

    /// The own skin, packed, once it is.
    pub fn take_own(&mut self) -> Option<OwnSkin> {
        self.own_ready.take()
    }

    /// Another player's skin arrived (as the PNG the server sent).
    pub fn on_skin(&mut self, id: u16, png: Vec<u8>) {
        let Some(side) = side_of_png(&png) else { return };
        if !self.remote.contains_key(&id) && self.remote.len() >= MAX_REMOTE {
            return;
        }
        self.gen += 1;
        let gen = self.gen;
        let e = self.remote.entry(id).or_insert(Remote { full: side, png: Vec::new(), cur: 0, want: 0, gen, last_near: 0, absent: 0.0 });
        e.full = side;
        e.png = png;
        e.gen = gen;
        e.want = 0;
        e.absent = 0.0;
        self.dirty = true;
    }

    /// Take what the workers finished: upload skins to the card and keep the
    /// player's own skin for sending.
    pub fn pump(&mut self, gpu: &mut dyn SkinGpu, textures: &mut BTreeMap<u16, u32>) {
        for d in work::take_done() {
            match d {
                Done::Own(o) => {
                    if o.gen == self.own_gen {
                        self.own_ready = Some(o);
                    }
                }
                Done::Loaded { gen, result } => {
                    if gen == self.load_gen {
                        self.loading = false;
                        self.loaded = Some(result);
                    }
                }
                Done::Remote { id, gen, result } => {
                    self.inflight = self.inflight.saturating_sub(1);
                    let Some(e) = self.remote.get_mut(&id) else { continue };
                    if e.gen != gen {
                        continue;
                    }
                    e.want = 0;
                    let Some((side, rgba)) = result else {
                        // Not a picture after all: the player wears the plain skin.
                        self.remote.remove(&id);
                        if let Some(t) = textures.remove(&id) {
                            gpu.delete(t);
                        }
                        continue;
                    };
                    match textures.get(&id).copied() {
                        Some(t) if e.cur == side => gpu.update(t, side, &rgba),
                        Some(t) => {
                            gpu.delete(t);
                            let n = gpu.add(side, &rgba);
                            textures.insert(id, n);
                        }
                        None => {
                            let n = gpu.add(side, &rgba);
                            textures.insert(id, n);
                        }
                    }
                    e.cur = side;
                    // A skin that is not bigger than the smallest size is never unpacked again.
                    if e.full <= budget::LOW_SIDE {
                        e.png = Vec::new();
                    }
                }
            }
        }
        self.trim();
    }

    /// Drop the compressed pictures of the skins whose players were near
    /// longest ago while more than [`KEEP_BYTES`] are kept. (Those skins stay
    /// at the size they have.)
    fn trim(&mut self) {
        let mut kept: usize = self.remote.values().map(|e| e.png.len()).sum();
        while kept > KEEP_BYTES {
            let pick = self.remote.iter().filter(|(_, e)| !e.png.is_empty() && e.cur != 0 && e.want == 0).min_by_key(|(id, e)| (e.last_near, **id)).map(|(id, _)| *id);
            let Some(id) = pick else { break };
            if let Some(e) = self.remote.get_mut(&id) {
                kept -= e.png.len();
                e.png = Vec::new();
            }
        }
    }

    /// Look at who is near (every half second, or at once after a new skin),
    /// forget players who are gone, and start the jobs that bring each skin to
    /// the size it should have. `dists` is the distance of every player there is.
    pub fn plan(&mut self, dt: f32, dists: &BTreeMap<u16, f32>, gpu: &mut dyn SkinGpu, textures: &mut BTreeMap<u16, u32>) {
        if dt > 0.0 && dt < 3600.0 {
            self.timer += dt;
        }
        if self.timer < REPLAN_EVERY && !self.dirty {
            return;
        }
        let elapsed = self.timer;
        self.timer = 0.0;
        self.dirty = false;
        self.tick_no += 1;

        let mut gone: Vec<u16> = Vec::new();
        for (id, e) in self.remote.iter_mut() {
            if dists.contains_key(id) {
                e.absent = 0.0;
            } else {
                e.absent += elapsed;
                if e.absent > ABSENT_AFTER {
                    gone.push(*id);
                }
            }
        }
        for id in gone {
            self.remote.remove(&id);
            if let Some(t) = textures.remove(&id) {
                gpu.delete(t);
            }
        }

        let tick_no = self.tick_no;
        let mut entries: Vec<Entry> = Vec::with_capacity(self.remote.len());
        for (id, e) in self.remote.iter_mut() {
            let dist = dists.get(id).copied().unwrap_or(f32::INFINITY);
            if dist <= NEAR {
                e.last_near = tick_no;
            }
            let fixed = e.png.is_empty() && e.cur != 0;
            entries.push(Entry { id: *id, full: e.full, cur: if e.want != 0 { e.want } else { e.cur }, dist, last_near: e.last_near, fixed });
        }
        let planned = budget::plan(&entries, GPU_BUDGET);
        // The nearest players first.
        let mut order: Vec<(f32, u16, usize)> = planned.iter().zip(&entries).map(|(&(id, side), en)| (en.dist, id, side)).collect();
        order.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
        for (_, id, side) in order {
            if self.inflight >= MAX_JOBS {
                break;
            }
            let Some(e) = self.remote.get_mut(&id) else { continue };
            if e.want != 0 || e.png.is_empty() || side == e.cur {
                continue;
            }
            self.gen += 1;
            e.gen = self.gen;
            e.want = side;
            self.inflight += 1;
            work::submit(Job::Remote { id, gen: e.gen, full: e.full, png: e.png.clone(), target: side });
        }
    }

    /// Forget every other player's skin (the game was left): the textures go back to the card.
    pub fn forget_all(&mut self, gpu: &mut dyn SkinGpu, textures: &mut BTreeMap<u16, u32>) {
        for (_, t) in core::mem::take(textures) {
            gpu.delete(t);
        }
        self.remote.clear();
        self.dirty = false;
    }

    /// Bytes of the card the other players' skins take now (for the memory page).
    pub fn gpu_bytes(&self) -> usize {
        self.remote.values().map(|e| budget::gpu_bytes(e.cur)).sum()
    }

    /// What the other players' skins on the card cost beyond a plain 64 x 64 each: the memory
    /// the player's choice of HD skins uses (it raises the budget's limit, see client/memory).
    pub fn gpu_extra_bytes(&self) -> usize {
        let plain = budget::gpu_bytes(budget::LOW_SIDE);
        self.remote.values().map(|e| budget::gpu_bytes(e.cur).saturating_sub(plain)).sum()
    }

    /// Bytes of compressed skins kept.
    pub fn kept_bytes(&self) -> usize {
        self.remote.values().map(|e| e.png.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::skins::work::TEST_LOCK;

    #[derive(Default)]
    struct Mock {
        next: u32,
        live: BTreeMap<u32, usize>,
        uploads: Vec<usize>,
    }

    impl SkinGpu for Mock {
        fn add(&mut self, side: usize, _rgba: &[u8]) -> u32 {
            self.next += 1;
            self.live.insert(self.next, side);
            self.uploads.push(side);
            self.next
        }
        fn update(&mut self, tex: u32, side: usize, _rgba: &[u8]) {
            assert_eq!(self.live.get(&tex), Some(&side), "an update keeps the side");
            self.uploads.push(side);
        }
        fn delete(&mut self, tex: u32) {
            assert!(self.live.remove(&tex).is_some(), "deleted twice");
        }
    }

    fn skin(side: usize) -> Vec<u8> {
        let px: Vec<u8> = (0..side * side).flat_map(|i| [(i % 251) as u8, (i / 7 % 251) as u8, 60, 255]).collect();
        crate::png::encode(side, side, &px)
    }

    fn dists(list: &[(u16, f32)]) -> BTreeMap<u16, f32> {
        list.iter().copied().collect()
    }

    #[test]
    fn a_skin_comes_up_at_the_size_its_distance_asks_and_changes_as_the_player_moves() {
        let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        work::take_done();
        let (mut hub, mut gpu, mut tex) = (SkinHub::default(), Mock::default(), BTreeMap::new());
        hub.on_skin(1, skin(128));
        hub.on_skin(2, skin(64));
        let near = dists(&[(1, 10.0), (2, 10.0)]);
        hub.plan(0.0, &near, &mut gpu, &mut tex);
        hub.pump(&mut gpu, &mut tex);
        assert_eq!((gpu.live[&tex[&1]], gpu.live[&tex[&2]]), (128, 64));
        // The 64 skin's picture is not kept; the 128 one's is, to be unpacked again.
        assert!(hub.kept_bytes() > 0);
        // Only what the 128 skin costs beyond a plain 64 raises the memory limit.
        assert_eq!(hub.gpu_extra_bytes(), budget::gpu_bytes(128) - budget::gpu_bytes(64));
        assert_eq!(hub.gpu_bytes(), budget::gpu_bytes(128) + budget::gpu_bytes(64));
        // Player 1 goes far away: after a while the skin is at 64.
        let far = dists(&[(1, 200.0), (2, 10.0)]);
        hub.plan(1.0, &far, &mut gpu, &mut tex);
        hub.pump(&mut gpu, &mut tex);
        assert_eq!(gpu.live[&tex[&1]], 64);
        // And back near: 128 again.
        hub.plan(1.0, &near, &mut gpu, &mut tex);
        hub.pump(&mut gpu, &mut tex);
        assert_eq!(gpu.live[&tex[&1]], 128);
        // A new skin from the same player replaces the old one on the card.
        hub.on_skin(1, skin(192));
        hub.plan(0.0, &near, &mut gpu, &mut tex);
        hub.pump(&mut gpu, &mut tex);
        assert_eq!(gpu.live[&tex[&1]], 192);
        assert_eq!(gpu.live.len(), 2, "no texture is left behind");
        // A player who left loses their skin after a few seconds.
        let only2 = dists(&[(2, 10.0)]);
        hub.plan(1.0, &only2, &mut gpu, &mut tex);
        assert!(tex.contains_key(&1));
        hub.plan(4.0, &only2, &mut gpu, &mut tex);
        assert!(!tex.contains_key(&1));
        assert_eq!(gpu.live.len(), 1);
        hub.forget_all(&mut gpu, &mut tex);
        assert!(gpu.live.is_empty() && tex.is_empty());
    }

    #[test]
    fn after_the_game_is_left_a_player_with_the_same_number_wears_nobody_elses_skin() {
        let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        work::take_done();
        let (mut hub, mut gpu, mut tex) = (SkinHub::default(), Mock::default(), BTreeMap::new());
        hub.on_skin(7, skin(128));
        hub.plan(0.0, &dists(&[(7, 5.0)]), &mut gpu, &mut tex);
        hub.pump(&mut gpu, &mut tex);
        assert!(tex.contains_key(&7));
        hub.forget_all(&mut gpu, &mut tex);
        assert_eq!(hub.kept_bytes(), 0);
        // The next visit: number 7 is someone else, who has sent no skin.
        hub.plan(1.0, &dists(&[(7, 5.0)]), &mut gpu, &mut tex);
        hub.pump(&mut gpu, &mut tex);
        assert!(tex.is_empty() && gpu.live.is_empty());
    }

    #[test]
    fn junk_and_odd_sizes_are_never_uploaded() {
        let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        work::take_done();
        let (mut hub, mut gpu, mut tex) = (SkinHub::default(), Mock::default(), BTreeMap::new());
        hub.on_skin(1, b"not a picture, but long enough to be looked at".to_vec());
        hub.on_skin(2, crate::png::encode(100, 100, &[3u8; 100 * 100 * 4]));
        hub.plan(0.0, &dists(&[(1, 5.0), (2, 5.0)]), &mut gpu, &mut tex);
        hub.pump(&mut gpu, &mut tex);
        assert!(tex.is_empty() && gpu.uploads.is_empty());
    }

    #[test]
    fn the_own_skin_comes_back_packed_and_only_the_newest_counts() {
        let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        work::take_done();
        let (mut hub, mut gpu, mut tex) = (SkinHub::default(), Mock::default(), BTreeMap::new());
        hub.pack_own(64, alloc::vec![200u8; 64 * 64 * 4]);
        hub.pack_own(128, alloc::vec![100u8; 128 * 128 * 4]);
        hub.pump(&mut gpu, &mut tex);
        let own = hub.take_own().expect("packed");
        assert_eq!(own.side, 128);
        assert!(hub.take_own().is_none());
    }
}
