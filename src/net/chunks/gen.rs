//! The server's chunk maker. The dedicated server has no world of its own
//! (it only keeps the players' edits), so for a world whose land only the
//! server can make it keeps this: it asks the world generator for a chunk on
//! a worker thread, puts the players' edits on top, compresses the result
//! (`codec`) and remembers the last 256 answers (`cache`).
//!
//! Everything here is plain data and one worker; the server's tick calls
//! [`ChunkGen::request`] for what players ask for and [`ChunkGen::poll`] for
//! what is ready to send. Without a worker thread (the tests, or a system
//! that gives none) `poll` makes a couple of chunks itself.

use super::cache::{Lru, Pieces};
use super::codec;
use crate::block::Block;
use crate::chunk::ChunkData;
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

/// How a chunk is made: the seed and the chunk's place give its blocks.
pub type GenFn = fn(seed: u32, cx: i32, cz: i32) -> ChunkData;

/// The players' edits in one chunk: (x in the chunk, y, z in the chunk, state).
pub type Edits = Vec<(i32, i32, i32, Block)>;

/// Chunks that may be waiting to be made at once (a request beyond is dropped;
/// the client asks again).
pub const MAX_WAITING: usize = 128;
/// Chunks `poll` makes itself when there is no worker thread.
const INLINE_PER_POLL: usize = 2;

struct Job {
    cx: i32,
    cz: i32,
    edits: Edits,
}

struct Done {
    cx: i32,
    cz: i32,
    pieces: Option<Pieces>,
}

struct Shared {
    waiting: VecDeque<Job>,
    done: Vec<Done>,
    /// A worker is running (or about to).
    busy: bool,
}

/// A lock that is only held for a few instructions (so it may spin), and that
/// needs no SDL: the tests make chunk makers too.
struct Spin<T> {
    lock: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Spin<T> {}
unsafe impl<T: Send> Send for Spin<T> {}

impl<T> Spin<T> {
    fn new(value: T) -> Spin<T> {
        Spin { lock: AtomicBool::new(false), value: UnsafeCell::new(value) }
    }

    fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        while self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        let r = f(unsafe { &mut *self.value.get() });
        self.lock.store(false, Ordering::Release);
        r
    }
}

/// Make one chunk's answer: the generator's blocks, the edits on top, compressed
/// and cut into pieces. `None` if it cannot be sent (more pieces than a chunk may take).
pub fn make(gen: GenFn, seed: u32, cx: i32, cz: i32, edits: &[(i32, i32, i32, Block)]) -> Option<Pieces> {
    let mut c = gen(seed, cx, cz);
    codec::apply_edits(&mut c, edits);
    codec::pack(&c).map(Arc::new)
}

/// The worker: takes jobs until none are left. `busy` is cleared in the same
/// locked step that finds the queue empty, so a job added at that moment
/// starts a new worker instead of waiting for ever.
fn work(shared: &Spin<Shared>, gen: GenFn, seed: u32) {
    loop {
        let job = shared.with(|s| {
            let j = s.waiting.pop_front();
            if j.is_none() {
                s.busy = false;
            }
            j
        });
        let Some(job) = job else { return };
        let pieces = make(gen, seed, job.cx, job.cz, &job.edits);
        shared.with(|s| s.done.push(Done { cx: job.cx, cz: job.cz, pieces }));
    }
}

/// Who waits for a chunk that is being made.
struct Wanted {
    who: Vec<u16>,
    /// A player changed a block in it meanwhile: the answer is still sent
    /// (the asker is told of the change too) but not kept.
    stale: bool,
}

/// A chunk to send: to the connection (player id) `to`.
pub struct Ready {
    pub to: u16,
    pub cx: i32,
    pub cz: i32,
    /// `None`: it could not be made; the asker's place is freed and it asks again later.
    pub pieces: Option<Pieces>,
}

/// What [`ChunkGen::request`] did.
pub enum Taken {
    /// The answer is at hand.
    Hit(Pieces),
    /// It will come out of [`ChunkGen::poll`].
    Queued,
    /// Too many are waiting: the request is dropped.
    Full,
}

pub struct ChunkGen {
    seed: u32,
    gen: GenFn,
    threaded: bool,
    cache: Lru,
    wanted: BTreeMap<(i32, i32), Wanted>,
    shared: Arc<Spin<Shared>>,
}

impl ChunkGen {
    /// A chunk maker for the world `seed`. With `threaded` the work is done by a worker
    /// thread (started when there is something to do); without, by `poll`.
    pub fn new(seed: u32, gen: GenFn, threaded: bool) -> ChunkGen {
        ChunkGen { seed, gen, threaded, cache: Lru::default(), wanted: BTreeMap::new(), shared: Arc::new(Spin::new(Shared { waiting: VecDeque::new(), done: Vec::new(), busy: false })) }
    }

    /// `who` wants chunk (cx, cz). `edits` is asked for only if the chunk has to be made.
    pub fn request(&mut self, who: u16, cx: i32, cz: i32, edits: impl FnOnce() -> Edits) -> Taken {
        let key = (cx, cz);
        if let Some(p) = self.cache.get(key) {
            return Taken::Hit(p);
        }
        if let Some(w) = self.wanted.get_mut(&key) {
            if !w.who.contains(&who) {
                w.who.push(who);
            }
            return Taken::Queued;
        }
        if self.wanted.len() >= MAX_WAITING {
            return Taken::Full;
        }
        self.wanted.insert(key, Wanted { who: alloc::vec![who], stale: false });
        let job = Job { cx, cz, edits: edits() };
        let start = self.shared.with(|s| {
            s.waiting.push_back(job);
            let start = !s.busy;
            s.busy = s.busy || self.threaded;
            start && self.threaded
        });
        if start {
            let shared = self.shared.clone();
            let (gen, seed) = (self.gen, self.seed);
            let task = Box::new(move || work(&shared, gen, seed));
            if crate::sys::thread::spawn(task).is_none() {
                // No thread to be had: do it here and now.
                work(&self.shared, gen, seed);
            }
        }
        Taken::Queued
    }

    /// A block in chunk (cx, cz) was changed by a player: what was kept of it is out of date.
    pub fn edited(&mut self, cx: i32, cz: i32) {
        self.cache.remove((cx, cz));
        if let Some(w) = self.wanted.get_mut(&(cx, cz)) {
            w.stale = true;
        }
    }

    /// Chunks that have been made since the last call, one entry for each player waiting.
    pub fn poll(&mut self) -> Vec<Ready> {
        if !self.threaded {
            for _ in 0..INLINE_PER_POLL {
                let Some(job) = self.shared.with(|s| s.waiting.pop_front()) else { break };
                let pieces = make(self.gen, self.seed, job.cx, job.cz, &job.edits);
                self.shared.with(|s| s.done.push(Done { cx: job.cx, cz: job.cz, pieces }));
            }
        }
        let done = self.shared.with(|s| core::mem::take(&mut s.done));
        let mut out = Vec::new();
        for d in done {
            let Some(w) = self.wanted.remove(&(d.cx, d.cz)) else { continue };
            if let (Some(p), false) = (&d.pieces, w.stale) {
                self.cache.put((d.cx, d.cz), p.clone());
            }
            for who in w.who {
                out.push(Ready { to: who, cx: d.cx, cz: d.cz, pieces: d.pieces.clone() });
            }
        }
        out
    }

    /// Chunks kept, and the bytes they take.
    pub fn cached(&self) -> (usize, usize) {
        (self.cache.len(), self.cache.bytes())
    }

    /// Chunks being made or waiting for it.
    pub fn waiting(&self) -> usize {
        self.wanted.len()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Land that tells which chunk it is: stone below 30, and blocks (stone, dirt, grass, sand, gravel) on top that depend on the place.
    pub(crate) fn land(seed: u32, cx: i32, cz: i32) -> ChunkData {
        let mut c = ChunkData::new();
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..30 {
                    c.set(x, y, z, 1);
                }
            }
        }
        c.set(0, 30, 0, (1 + cx.rem_euclid(4) + (seed % 2) as i32) as Block);
        c.set(1, 30, 0, (1 + cz.rem_euclid(4)) as Block);
        c
    }

    fn pieces_of(r: &Ready) -> ChunkData {
        codec::unpack(&r.pieces.as_ref().unwrap().concat()).unwrap()
    }

    #[test]
    fn a_chunk_is_made_sent_to_whoever_asked_and_kept_for_the_next() {
        let mut g = ChunkGen::new(7, land, false);
        assert!(matches!(g.request(1, 3, -2, Vec::new), Taken::Queued));
        // A second player asks for the same chunk while it waits: one chunk is made, both are served.
        assert!(matches!(g.request(2, 3, -2, Vec::new), Taken::Queued));
        assert_eq!(g.waiting(), 1);
        let ready = g.poll();
        assert_eq!(ready.len(), 2);
        assert_eq!((ready[0].to, ready[1].to), (1, 2));
        assert_eq!(pieces_of(&ready[0]), land(7, 3, -2));
        assert_eq!(g.waiting(), 0);
        assert_eq!(g.cached().0, 1);
        match g.request(3, 3, -2, || panic!("a kept chunk needs no edits")) {
            Taken::Hit(p) => assert_eq!(codec::unpack(&p.concat()).unwrap(), land(7, 3, -2)),
            _ => panic!("kept"),
        }
    }

    #[test]
    fn the_servers_edits_are_put_over_the_generated_land() {
        let mut g = ChunkGen::new(1, land, false);
        let edits = || alloc::vec![(5, 31, 6, 9), (0, 0, 0, 0), (0, 30, 0, 4)];
        assert!(matches!(g.request(1, 0, 0, edits), Taken::Queued));
        let ready = g.poll();
        let c = pieces_of(&ready[0]);
        assert_eq!(c.get(5, 31, 6), 9);
        assert_eq!(c.get(0, 0, 0), 0);
        assert_eq!(c.get(0, 30, 0), 4);
        assert_eq!(c.get(7, 7, 7), 1, "the rest is the generator's");
    }

    #[test]
    fn a_change_makes_the_kept_chunk_old_and_a_chunk_changed_while_made_is_not_kept() {
        let mut g = ChunkGen::new(1, land, false);
        g.request(1, 1, 1, Vec::new);
        g.poll();
        assert_eq!(g.cached().0, 1);
        g.edited(1, 1);
        assert_eq!(g.cached().0, 0);
        assert!(matches!(g.request(1, 1, 1, || alloc::vec![(1, 1, 1, 5)]), Taken::Queued));
        g.edited(1, 1);
        let ready = g.poll();
        assert_eq!(ready.len(), 1, "still sent");
        assert_eq!(g.cached().0, 0, "but not kept");
    }

    #[test]
    fn only_a_few_chunks_wait_and_a_few_are_made_each_poll() {
        let mut g = ChunkGen::new(1, land, false);
        for i in 0..MAX_WAITING as i32 {
            assert!(matches!(g.request(1, i, 0, Vec::new), Taken::Queued));
        }
        assert!(matches!(g.request(1, 5000, 0, Vec::new), Taken::Full));
        assert_eq!(g.poll().len(), INLINE_PER_POLL);
        assert_eq!(g.waiting(), MAX_WAITING - INLINE_PER_POLL);
        let mut total = INLINE_PER_POLL;
        while g.waiting() > 0 {
            total += g.poll().len();
        }
        assert_eq!(total, MAX_WAITING);
        assert!(g.cached().0 <= super::super::cache::MAX_ENTRIES);
    }

    #[test]
    fn the_same_chunk_comes_out_the_same_however_often_it_is_made() {
        let a = make(land, 9, -4, 8, &[]).unwrap();
        let b = make(land, 9, -4, 8, &[]).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, make(land, 9, -3, 8, &[]).unwrap());
    }
}
