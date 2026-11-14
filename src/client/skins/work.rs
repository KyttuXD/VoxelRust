//! Skin work that must not stop a frame: packing the player's own skin into a
//! PNG small enough to send (a 4096 x 4096 one takes a few seconds), and
//! unpacking and shrinking other players' skins. Big jobs run on a worker
//! thread and leave their results in a queue that the frame takes from;
//! small ones (256 x 256 and under, a few milliseconds) run on the spot and
//! use the same queue, so the rest of the game sees no difference.
//!
//! [`run`] does a job and is a plain function, tested directly.

use crate::net::skinxfer::side_of_png;
use crate::proto::MAX_SKIN;
use crate::sys;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

/// Skins up to this side are worked on in the frame itself.
pub const INLINE_SIDE: usize = 256;

pub enum Job {
    /// Pack the player's own skin (`side` x `side` pixels): the PNG to send, and the 64 x 64 one.
    Own { gen: u32, side: usize, rgba: Vec<u8> },
    /// Unpack another player's skin, shrunk to `target` pixels at most.
    Remote { id: u16, gen: u32, full: usize, png: Vec<u8>, target: usize },
}

pub enum Done {
    Own(OwnSkin),
    Remote { id: u16, gen: u32, result: Option<(usize, Vec<u8>)> },
}

/// The player's own skin, packed.
pub struct OwnSkin {
    pub gen: u32,
    /// The PNG to send and its side (smaller than the picture if the picture
    /// would not fit in [`MAX_SKIN`] bytes).
    pub hd: Vec<u8>,
    pub side: usize,
    /// The 64 x 64 PNG, for servers and games that know only that size.
    pub small: Vec<u8>,
}

impl Job {
    fn side(&self) -> usize {
        match self {
            Job::Own { side, .. } => *side,
            Job::Remote { full, .. } => *full,
        }
    }
}

/// Pack a skin: the 64 x 64 version and the biggest version that fits in
/// `limit` bytes of PNG (halving the side, to a multiple of 64, until it
/// does). A skin that is 64 wide is both.
pub fn pack_own(side: usize, rgba: &[u8], limit: usize) -> (Vec<u8>, usize, Vec<u8>) {
    if side <= 64 {
        let small = crate::png::encode(64, 64, rgba);
        return (small.clone(), 64, small);
    }
    let small = crate::png::encode(64, 64, &crate::image::resize::downscale_square(rgba, side, 64));
    // The picture as given first; halved copies after that.
    let mut halved: Option<Vec<u8>> = None;
    let mut cur_side = side;
    loop {
        let src: &[u8] = halved.as_deref().unwrap_or(rgba);
        let png = crate::png::encode(cur_side, cur_side, src);
        if png.len() <= limit {
            return (png, cur_side, small);
        }
        let next = ((cur_side / 2) / 64 * 64).max(64);
        if next >= cur_side || next <= 64 {
            return (small.clone(), 64, small);
        }
        let down = crate::image::resize::downscale_square(src, cur_side, next);
        halved = Some(down);
        cur_side = next;
    }
}

/// Do a job.
pub fn run(job: Job) -> Done {
    match job {
        Job::Own { gen, side, rgba } => {
            let (hd, side, small) = pack_own(side, &rgba, MAX_SKIN);
            Done::Own(OwnSkin { gen, hd, side, small })
        }
        Job::Remote { id, gen, png, target, .. } => {
            let result = side_of_png(&png).and_then(|_| crate::png::decode(&png)).filter(|img| img.w == img.h && crate::skin_grid::valid_side(img.w)).map(|img| {
                let want = target.clamp(64, img.w);
                if want < img.w {
                    (want, crate::image::resize::downscale_square(&img.rgba, img.w, want))
                } else {
                    (img.w, img.rgba)
                }
            });
            Done::Remote { id, gen, result }
        }
    }
}

/// A value that two threads share for a moment at a time. (A spin lock: what
/// it guards is a push or a take, never anything long.)
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

static DONE: Spin<Vec<Done>> = Spin::new(Vec::new());

fn push_done(d: Done) {
    DONE.with(|v| v.push(d));
}

/// Everything finished since the last call.
pub fn take_done() -> Vec<Done> {
    DONE.with(core::mem::take)
}

/// Start a job. A big one goes to a worker thread (or, if the system gives
/// none, is done here and now).
pub fn submit(job: Job) {
    if job.side() <= INLINE_SIDE {
        push_done(run(job));
        return;
    }
    let slot = Arc::new(Spin::new(Some(job)));
    let theirs = slot.clone();
    let task = Box::new(move || {
        if let Some(job) = theirs.with(|j| j.take()) {
            push_done(run(job));
        }
    });
    if sys::thread::spawn(task).is_none() {
        if let Some(job) = slot.with(|j| j.take()) {
            push_done(run(job));
        }
    }
}

/// Tests that use the queue take turns.
#[cfg(test)]
pub static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    fn noisy(side: usize) -> Vec<u8> {
        let mut s = 31u32;
        (0..side * side * 4)
            .map(|i| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                if i % 4 == 3 {
                    255
                } else {
                    (s >> 24) as u8
                }
            })
            .collect()
    }

    #[test]
    fn a_64_skin_is_its_own_small_version() {
        let px = noisy(64);
        let (hd, side, small) = pack_own(64, &px, MAX_SKIN);
        assert_eq!((side, hd.clone()), (64, small.clone()));
        assert_eq!(crate::png::decode(&small).unwrap().rgba, px);
    }

    #[test]
    fn a_big_skin_that_fits_is_sent_as_it_is_with_a_64_version_beside_it() {
        let px = noisy(128);
        let (hd, side, small) = pack_own(128, &px, MAX_SKIN);
        assert_eq!(side, 128);
        assert_eq!(crate::png::decode(&hd).unwrap().rgba, px);
        let s = crate::png::decode(&small).unwrap();
        assert_eq!((s.w, s.h), (64, 64));
    }

    #[test]
    fn a_skin_too_big_for_the_wire_is_halved_until_it_fits() {
        let px = noisy(256);
        // The noise of 256 x 256 takes about 196 KB, of 128 x 128 about 49 KB.
        let (hd, side, _) = pack_own(256, &px, 100_000);
        assert_eq!(side, 128);
        assert!(hd.len() <= 100_000);
        assert_eq!(side_of_png(&hd), Some(128));
        let (hd, side, small) = pack_own(256, &px, 1000);
        assert_eq!((side, hd), (64, small));
    }

    #[test]
    fn a_remote_skin_is_unpacked_at_the_size_asked_for() {
        let px = noisy(256);
        let png = crate::png::encode(256, 256, &px);
        for (target, want) in [(256usize, 256usize), (1024, 256), (64, 64), (100, 100)] {
            let Done::Remote { id, gen, result } = run(Job::Remote { id: 3, gen: 9, full: 256, png: png.clone(), target }) else { panic!() };
            assert_eq!((id, gen), (3, 9));
            let (side, rgba) = result.unwrap();
            assert_eq!(side, want);
            assert_eq!(rgba.len(), side * side * 4);
        }
        // Nonsense gives no picture, never a panic.
        let Done::Remote { result, .. } = run(Job::Remote { id: 1, gen: 1, full: 64, png: b"nonsense that is not a picture at all".to_vec(), target: 64 }) else { panic!() };
        assert!(result.is_none());
        let wide = crate::png::encode(128, 64, &[1u8; 128 * 64 * 4]);
        let Done::Remote { result, .. } = run(Job::Remote { id: 1, gen: 1, full: 128, png: wide, target: 64 }) else { panic!() };
        assert!(result.is_none());
    }

    #[test]
    fn small_jobs_go_through_the_queue_on_the_spot() {
        let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        take_done();
        let px = noisy(64);
        submit(Job::Own { gen: 4, side: 64, rgba: px });
        let done = take_done();
        assert!(done.iter().any(|d| matches!(d, Done::Own(o) if o.gen == 4 && o.side == 64)));
        assert!(take_done().is_empty());
    }
}
