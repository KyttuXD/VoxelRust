//! A recorded sound that is decoded when it is wanted and let go when it is not.
//!
//! A mod with many long sounds would otherwise keep every one of them decoded
//! (a 12 second clip is half a megabyte) for the whole run. A [`Clip`] keeps
//! only where its file is and how long it is. The samples are decoded the
//! first time the sound plays and stay while it is in use; [`Clip::evict`]
//! frees the ones not used lately (the memory budget calls it, see
//! `util/budget`). Short clips (footsteps, clicks) are always kept: decoding
//! them again would cost more than they hold.
//!
//! Only the main thread touches the samples' lifetime (`get` and `evict`);
//! the mixer thread reads a clip through a pointer taken by `get`, so the
//! caller of `evict` passes the pointers the mixer is playing (see
//! `client/audio`) and those are left alone.

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

/// Clips this short (in samples; one second at the mixer's rate) stay decoded for good.
pub const KEEP_SAMPLES: usize = 22050;

use crate::util::data::reread;

struct Inner {
    path: String,
    /// How many samples it has once decoded.
    len: usize,
    /// The decoded samples (a leaked `Box<Vec<i16>>`), or null.
    samples: AtomicPtr<Vec<i16>>,
    /// The clock (seconds, see `get`) at the last use.
    used: AtomicU32,
    /// Its file can be read again.
    evictable: bool,
}

impl Drop for Inner {
    fn drop(&mut self) {
        let p = *self.samples.get_mut();
        if !p.is_null() {
            // SAFETY: made by `Box::into_raw` in `get` or `new`, and taken out of the slot here only.
            unsafe { drop(alloc::boxed::Box::from_raw(p)) };
        }
    }
}

/// A recorded sound. Cloning shares it.
#[derive(Clone)]
pub struct Clip(Arc<Inner>);

impl core::fmt::Debug for Clip {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Clip({}, {} samples)", self.0.path, self.0.len)
    }
}

impl Clip {
    /// A clip just decoded from `path`. A long one is let go at once and
    /// decoded again when played (when `path` can be read again: an empty
    /// path means it cannot, and the samples stay).
    pub fn new(path: &str, samples: Vec<i16>) -> Clip {
        let len = samples.len();
        let keep = path.is_empty() || len <= KEEP_SAMPLES || reread::keeping();
        let ptr = if keep { alloc::boxed::Box::into_raw(alloc::boxed::Box::new(samples)) } else { core::ptr::null_mut() };
        let evictable = !keep || (!path.is_empty() && !reread::keeping());
        Clip(Arc::new(Inner { path: String::from(path), len, samples: AtomicPtr::new(ptr), used: AtomicU32::new(0), evictable }))
    }

    /// Number of samples (known without decoding).
    pub fn len(&self) -> usize {
        self.0.len
    }

    pub fn is_empty(&self) -> bool {
        self.0.len == 0
    }

    /// Whether the samples are in memory now.
    pub fn resident(&self) -> bool {
        !self.0.samples.load(Ordering::Acquire).is_null()
    }

    /// Bytes of decoded samples held now.
    pub fn resident_bytes(&self) -> usize {
        if self.resident() {
            self.0.len * 2
        } else {
            0
        }
    }

    /// The samples, decoded now if need be, and marked as used at `clock`
    /// (any seconds counter). `None` if the file cannot be read or decoded
    /// again. The slice stays valid until [`Clip::evict`] frees it, which
    /// the caller keeps from happening while the mixer plays it.
    pub fn get(&self, clock: u32) -> Option<&[i16]> {
        self.0.used.store(clock, Ordering::Relaxed);
        let mut p = self.0.samples.load(Ordering::Acquire);
        if p.is_null() {
            let samples = reread::read(&self.0.path).and_then(|b| crate::wav::decode(&b))?;
            if samples.len() != self.0.len {
                // The file changed under us: still play what it holds now.
                crate::logf!("voxelrust: sound {} changed on disk", self.0.path);
            }
            p = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(samples));
            self.0.samples.store(p, Ordering::Release);
        }
        // SAFETY: `p` is a live box owned by this clip; it is freed only by `evict` or drop.
        Some(unsafe { (*p).as_slice() })
    }

    /// Free the samples if they were last used at `clock - idle` or earlier
    /// and are not in `playing` (pointers the mixer is reading) and the clip
    /// is long enough to be worth decoding again. Returns the bytes freed.
    pub fn evict(&self, clock: u32, idle: u32, playing: &[*const i16]) -> usize {
        let p = self.0.samples.load(Ordering::Acquire);
        if p.is_null() || self.0.path.is_empty() || self.0.len <= KEEP_SAMPLES || !self.0.evictable {
            return 0;
        }
        if clock.saturating_sub(self.0.used.load(Ordering::Relaxed)) < idle {
            return 0;
        }
        // SAFETY: as in `get`.
        let ptr = unsafe { (*p).as_ptr() };
        if playing.contains(&ptr) {
            return 0;
        }
        self.0.samples.store(core::ptr::null_mut(), Ordering::Release);
        // SAFETY: taken out of the slot just above; nothing else frees it.
        let freed = unsafe { alloc::boxed::Box::from_raw(p) };
        freed.len() * 2
    }

    /// When it was last used (the clock given to `get`).
    pub fn last_used(&self) -> u32 {
        self.0.used.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_clips_stay_and_empty_paths_stay() {
        let c = Clip::new("x.wav", alloc::vec![1i16; 100]);
        assert!(c.resident());
        assert_eq!(c.evict(1000, 0, &[]), 0);
        let long = Clip::new("", alloc::vec![1i16; KEEP_SAMPLES + 5]);
        assert!(long.resident());
        assert_eq!(long.evict(1000, 0, &[]), 0);
        assert_eq!(long.len(), KEEP_SAMPLES + 5);
    }

    #[test]
    fn under_test_every_clip_stays_and_get_returns_it() {
        // (Tests keep everything: there are no files to read again.)
        let c = Clip::new("does/not/exist.wav", alloc::vec![1i16; KEEP_SAMPLES * 2]);
        assert!(c.resident());
        assert_eq!(c.resident_bytes(), KEEP_SAMPLES * 4);
        assert_eq!(c.get(5).map(|s| s.len()), Some(KEEP_SAMPLES * 2));
        assert_eq!(c.last_used(), 5);
        assert_eq!(c.evict(100, 0, &[]), 0);
    }
}
