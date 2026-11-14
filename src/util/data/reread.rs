//! Reading a content file again later.
//!
//! Big things a mod brings (long sounds, model files) are not kept in memory
//! after they are read: the registry keeps where the file is and reads it
//! again when it is needed (`util/audio/clip`, `ModelSpec`). This is how.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

pub type Reader = fn(&str) -> Option<Vec<u8>>;

/// How a mod's file is read again: set once by the program (mod files can live
/// inside a zip, which only it knows how to open).
static READER: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// While set, content keeps what it reads in memory instead of reading the
/// file again later: for content that came over the network (it has no
/// file), and in tests.
static KEEP_ALL: AtomicBool = AtomicBool::new(false);

pub fn set_reader(f: Reader) {
    READER.store(f as *mut (), Ordering::Release);
}

pub fn keep_all(on: bool) {
    KEEP_ALL.store(on, Ordering::Relaxed);
}

/// Whether what is read now should stay in memory.
pub fn keeping() -> bool {
    cfg!(test) || KEEP_ALL.load(Ordering::Relaxed)
}

/// The file at `path`: one built into the program, else on disk, else
/// whatever the program's reader finds (inside a zip).
pub fn read(path: &str) -> Option<Vec<u8>> {
    if let Some(b) = crate::block::embedded::file(path) {
        return Some(b.to_vec());
    }
    if crate::sys::sdl::loaded() {
        if let Some(b) = crate::sys::read_file(path) {
            return Some(b);
        }
    }
    let p = READER.load(Ordering::Acquire);
    if p.is_null() {
        return None;
    }
    // SAFETY: only `set_reader` stores here, and it stores a `Reader`.
    let f: Reader = unsafe { core::mem::transmute::<*mut (), Reader>(p) };
    f(path)
}
