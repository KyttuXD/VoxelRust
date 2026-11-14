//! The server's skin decoding, off the tick.
//!
//! A big skin (up to 4096 x 4096) takes a good part of a second to unpack
//! and shrink to the 64 x 64 version older clients are sent; done on the
//! server's tick it would stop every player's game for that long. The jobs
//! go to one worker thread instead, one at a time (so a crowd of players
//! sending skins at once costs one core, not all of them), and the results
//! wait in a queue that the tick takes from ([`take_done`]).
//!
//! The queue is short (a player sends one skin in five seconds, so it only
//! grows if many do at once); a job that does not fit is refused. Where the
//! system gives no threads the job is done on the spot.

use crate::net::skinslot::Pending;
use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

/// Jobs that may wait at once.
pub const MAX_WAITING: usize = 16;

/// A skin to unpack, from the player with `id`.
pub struct Job {
    pub id: u16,
    pub pending: Pending,
}

/// A finished job: the 64 x 64 version, or `None` if the picture was not
/// what its header said.
pub struct Done {
    pub id: u16,
    pub pending: Pending,
    pub legacy: Option<Vec<u8>>,
}

/// Do a job.
pub fn run(job: Job) -> Done {
    let legacy = crate::net::skinxfer::legacy_version(&job.pending.png, job.pending.side);
    Done { id: job.id, pending: job.pending, legacy }
}

struct Spin<T> {
    lock: AtomicBool,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for Spin<T> {}

impl<T> Spin<T> {
    const fn new(value: T) -> Spin<T> {
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

struct Shared {
    waiting: VecDeque<Job>,
    done: Vec<Done>,
    /// A worker is running (or about to).
    busy: bool,
}

static SHARED: Spin<Shared> = Spin::new(Shared { waiting: VecDeque::new(), done: Vec::new(), busy: false });

/// The worker: takes jobs until none are left. `busy` is cleared in the same
/// locked step that finds the queue empty, so a job added at that moment
/// starts a new worker instead of waiting for ever.
fn work() {
    loop {
        let job = SHARED.with(|s| {
            let j = s.waiting.pop_front();
            if j.is_none() {
                s.busy = false;
            }
            j
        });
        let Some(job) = job else { return };
        let d = run(job);
        SHARED.with(|s| s.done.push(d));
    }
}

/// Queue a job. False if too many are waiting (the job is dropped).
pub fn submit(job: Job) -> bool {
    let start = SHARED.with(|s| {
        if s.waiting.len() >= MAX_WAITING {
            return None;
        }
        s.waiting.push_back(job);
        let start = !s.busy;
        s.busy = true;
        Some(start)
    });
    match start {
        None => false,
        Some(false) => true,
        Some(true) => {
            if crate::sys::thread::spawn(Box::new(work)).is_none() {
                // No thread to be had: do it here and now.
                work();
            }
            true
        }
    }
}

/// Everything finished since the last call.
pub fn take_done() -> Vec<Done> {
    SHARED.with(|s| core::mem::take(&mut s.done))
}
