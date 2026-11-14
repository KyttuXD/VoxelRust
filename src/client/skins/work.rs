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
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Skins up to this side are worked on in the frame itself.
pub const INLINE_SIDE: usize = 256;

pub enum Job {
    /// Pack the player's own skin (`side` x `side` pixels): the PNG to send, and the 64 x 64 one.
    Own { gen: u32, side: usize, rgba: Vec<u8> },
    /// Unpack another player's skin, shrunk to `target` pixels at most.
    Remote { id: u16, gen: u32, full: usize, png: Vec<u8>, target: usize },
    /// Unpack the player's own custom skin file (`skin.png`), so that a big one does
    /// not stop the first frame: the PNG's bytes, and the side its header gives.
    Load { gen: u32, side: usize, png: Vec<u8> },
}

pub enum Done {
    Own(OwnSkin),
    Remote { id: u16, gen: u32, result: Option<(usize, Vec<u8>)> },
    /// The custom skin file, unpacked as (side, square RGBA pixels), or `None` if it is no skin.
    Loaded { gen: u32, result: Option<(usize, Vec<u8>)> },
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
            Job::Load { side, .. } => *side,
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

/// Bytes of the decode jobs running right now (see [`inflight_bytes`]).
static INFLIGHT: AtomicUsize = AtomicUsize::new(0);

/// The total bytes of decode jobs currently running: each job's input (its PNG,
/// or its RGBA for an own skin) plus the RGBA it makes. For memory accounting.
pub fn inflight_bytes() -> usize {
    INFLIGHT.load(Ordering::Relaxed)
}

/// Holds one running job's bytes in [`INFLIGHT`] until it is dropped, so a
/// panic that unwinds through the job still gives them back.
struct Inflight(usize);

impl Inflight {
    fn start(bytes: usize) -> Inflight {
        INFLIGHT.fetch_add(bytes, Ordering::Relaxed);
        Inflight(bytes)
    }
}

impl Drop for Inflight {
    fn drop(&mut self) {
        INFLIGHT.fetch_sub(self.0, Ordering::Relaxed);
    }
}

/// The bytes a job holds while it runs. A remote skin is counted at the size
/// it asks for (clamped as [`run`] clamps it); the real picture may be smaller.
fn job_bytes(job: &Job) -> usize {
    let rgba = |side: usize| side.saturating_mul(side).saturating_mul(4);
    match job {
        Job::Own { rgba: px, .. } => px.len(),
        Job::Remote { png, full, target, .. } => {
            let out = (*target).max(64).min((*full).max(64));
            png.len().saturating_add(rgba(out))
        }
        Job::Load { png, side, .. } => png.len().saturating_add(rgba(*side)),
    }
}

/// Do a job.
pub fn run(job: Job) -> Done {
    let _running = Inflight::start(job_bytes(&job));
    match job {
        Job::Own { gen, side, rgba } => {
            let (hd, side, small) = pack_own(side, &rgba, MAX_SKIN);
            Done::Own(OwnSkin { gen, hd, side, small })
        }
        Job::Load { gen, png, .. } => {
            let result = crate::png::decode(&png).and_then(|img| crate::client::skinfile::from_image(&img).ok().map(|px| (img.w, px)));
            Done::Loaded { gen, result }
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
        // No worker thread (the system refused one, or the platform has none):
        // the job is done here on the frame's thread after all. That can stall
        // one frame for as long as a big skin takes to decode (seconds at most
        // for a 4096 x 4096 one). It is acceptable because it is rare, and the
        // only other outcome is that the skin never arrives.
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
    fn a_4096_skin_of_noise_is_halved_to_fit_the_real_wire_limit() {
        let px = noisy(4096);
        let (hd, side, _) = pack_own(4096, &px, MAX_SKIN);
        assert!(hd.len() <= MAX_SKIN, "{} bytes", hd.len());
        assert!(side < 4096 && side >= 64 && side.is_power_of_two(), "side {}", side);
        assert_eq!(side_of_png(&hd), Some(side));
    }

    #[test]
    fn a_custom_skin_file_is_unpacked_by_a_load_job() {
        let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let px = noisy(64);
        let png = crate::png::encode(64, 64, &px);
        let Done::Loaded { gen, result } = run(Job::Load { gen: 7, side: 64, png }) else { panic!() };
        assert_eq!(gen, 7);
        let (side, rgba) = result.unwrap();
        assert_eq!((side, rgba), (64, px));
        // Bytes that are no picture give no skin, never a panic.
        let Done::Loaded { result, .. } = run(Job::Load { gen: 1, side: 64, png: b"not a picture".to_vec() }) else { panic!() };
        assert!(result.is_none());
    }

    #[test]
    fn a_job_is_counted_by_its_input_and_its_rgba() {
        let png = vec![0u8; 100];
        assert_eq!(job_bytes(&Job::Load { gen: 1, side: 64, png: png.clone() }), 100 + 64 * 64 * 4);
        assert_eq!(job_bytes(&Job::Remote { id: 1, gen: 1, full: 256, png: png.clone(), target: 128 }), 100 + 128 * 128 * 4);
        // A target below 64, or a skin below 64, is counted at 64.
        assert_eq!(job_bytes(&Job::Remote { id: 1, gen: 1, full: 32, png: png.clone(), target: 16 }), 100 + 64 * 64 * 4);
        assert_eq!(job_bytes(&Job::Own { gen: 1, side: 64, rgba: vec![0u8; 64 * 64 * 4] }), 64 * 64 * 4);
    }

    #[test]
    fn a_running_job_is_counted_until_it_ends() {
        let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = inflight_bytes();
        {
            let _held = Inflight::start(1000);
            assert_eq!(inflight_bytes(), before + 1000);
        }
        assert_eq!(inflight_bytes(), before);
        let png = crate::png::encode(64, 64, &noisy(64));
        run(Job::Load { gen: 1, side: 64, png });
        assert_eq!(inflight_bytes(), before);
    }

    #[test]
    fn a_panic_gives_the_counted_bytes_back() {
        let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let before = inflight_bytes();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = Inflight::start(5000);
            panic!("expected: a job that panics");
        }));
        assert!(r.is_err());
        assert_eq!(inflight_bytes(), before);
    }

    #[test]
    fn a_remote_skin_is_unpacked_at_the_size_asked_for() {
        let _turn = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
