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

/// Add a job to the waiting list. `None` if too many are waiting (the job is
/// dropped); otherwise `Some(true)` when no worker is running and one must be
/// started, `Some(false)` when the running one will get to it.
fn enqueue(s: &mut Shared, job: Job) -> Option<bool> {
    if s.waiting.len() >= MAX_WAITING {
        return None;
    }
    s.waiting.push_back(job);
    let start = !s.busy;
    s.busy = true;
    Some(start)
}

/// Take the next job to do. Finding none clears `busy` in the same locked
/// step, so a job added at that moment starts a new worker instead of waiting
/// for ever.
fn next(s: &mut Shared) -> Option<Job> {
    let j = s.waiting.pop_front();
    if j.is_none() {
        s.busy = false;
    }
    j
}

/// The worker: takes jobs until none are left.
fn work() {
    loop {
        let job = SHARED.with(next);
        let Some(job) = job else { return };
        let d = run(job);
        SHARED.with(|s| s.done.push(d));
    }
}

/// Whether one more job would be taken now (the tick checks before it hands
/// a job over, so that a job is never dropped for want of room).
pub fn has_room() -> bool {
    SHARED.with(|s| s.waiting.len() < MAX_WAITING)
}

/// Queue a job. False if too many are waiting (the job is dropped).
pub fn submit(job: Job) -> bool {
    match SHARED.with(|s| enqueue(s, job)) {
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

#[cfg(test)]
mod tests {
    //! The queue's rules, tested on a queue of their own (no thread, no global).
    use super::*;
    use crate::net::skinslot::Pending;

    fn job(id: u16) -> Job {
        Job { id, pending: Pending { ticket: id as u32, png: Vec::new(), side: 128, slim: false } }
    }

    fn empty() -> Shared {
        Shared { waiting: VecDeque::new(), done: Vec::new(), busy: false }
    }

    #[test]
    fn a_full_queue_refuses_the_next_job_and_keeps_the_others() {
        let mut s = empty();
        for k in 0..MAX_WAITING as u16 {
            assert!(enqueue(&mut s, job(k)).is_some(), "job {k} fits");
        }
        assert_eq!(s.waiting.len(), MAX_WAITING);
        assert_eq!(enqueue(&mut s, job(99)), None, "the seventeenth is refused");
        assert_eq!(s.waiting.len(), MAX_WAITING);
        assert_eq!(s.waiting.back().map(|j| j.id), Some(MAX_WAITING as u16 - 1));
    }

    #[test]
    fn the_first_job_starts_a_worker_and_the_rest_wait_for_it() {
        let mut s = empty();
        assert_eq!(enqueue(&mut s, job(1)), Some(true), "no worker yet: start one");
        assert_eq!(enqueue(&mut s, job(2)), Some(false), "the worker is busy");
        assert_eq!(enqueue(&mut s, job(3)), Some(false));
    }

    #[test]
    fn jobs_come_out_in_the_order_they_went_in() {
        let mut s = empty();
        for k in [4u16, 9, 2] {
            enqueue(&mut s, job(k));
        }
        let order: Vec<u16> = core::iter::from_fn(|| next(&mut s).map(|j| j.id)).take(3).collect();
        assert_eq!(order, [4, 9, 2]);
    }

    #[test]
    fn an_empty_queue_lets_the_worker_stop_so_the_next_job_starts_one() {
        let mut s = empty();
        enqueue(&mut s, job(1));
        assert!(next(&mut s).is_some());
        assert!(s.busy, "still busy until the queue is found empty");
        assert!(next(&mut s).is_none());
        assert!(!s.busy);
        assert_eq!(enqueue(&mut s, job(2)), Some(true));
    }

    #[test]
    fn room_is_given_back_as_jobs_are_taken() {
        let mut s = empty();
        for k in 0..MAX_WAITING as u16 {
            enqueue(&mut s, job(k));
        }
        assert_eq!(enqueue(&mut s, job(50)), None);
        next(&mut s);
        assert!(enqueue(&mut s, job(50)).is_some(), "one taken, one fits again");
    }
}
