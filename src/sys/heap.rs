//! The C library's allocator is the game's allocator (see `CAlloc` in the
//! parent module). On Linux with glibc two things keep a long session's
//! memory up for no reason: every worker thread gets an allocator arena of
//! its own, and freed memory in the middle of the heap is not given back
//! (the meshing and generating workers free megabytes after every job).
//! These two calls fix that where the function exists (looked up at run time,
//! so a C library without it, musl for one, just does nothing).

#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
mod linux {
    use core::ffi::{c_char, c_int, c_void};

    extern "C" {
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    // mallopt parameters of glibc.
    const M_TRIM_THRESHOLD: c_int = -1;
    const M_MMAP_THRESHOLD: c_int = -3;
    const M_ARENA_MAX: c_int = -8;

    unsafe fn find(name: &[u8]) -> *mut c_void {
        // RTLD_DEFAULT is a null handle on Linux.
        dlsym(core::ptr::null_mut(), name.as_ptr() as *const c_char)
    }

    pub fn tune() {
        unsafe {
            let p = find(b"mallopt\0");
            if p.is_null() {
                return;
            }
            let mallopt: extern "C" fn(c_int, c_int) -> c_int = core::mem::transmute(p);
            // A few arenas, not one per thread.
            mallopt(M_ARENA_MAX, 4);
            // The meshing workers' scratch space (a few MB) is reused from the heap rather than
            // mapped and unmapped for every job. (Setting any of these also stops glibc from
            // raising the threshold by itself.) What they leave free goes back in `trim`.
            mallopt(M_MMAP_THRESHOLD, 4 << 20);
            // Give the top of the heap back once 8 MB of it is free.
            mallopt(M_TRIM_THRESHOLD, 8 << 20);
        }
    }

    pub fn trim() {
        unsafe {
            let p = find(b"malloc_trim\0");
            if p.is_null() {
                return;
            }
            let malloc_trim: extern "C" fn(usize) -> c_int = core::mem::transmute(p);
            malloc_trim(0);
        }
    }
}

/// Set up the allocator once, at start-up, before threads are made.
pub fn tune() {
    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    linux::tune();
}

/// Give freed memory back to the system, where the C library can. Takes a
/// few milliseconds on a big heap: call it after something big was freed (a
/// world closed, sounds dropped, the content reloaded), not every frame.
pub fn trim() {
    #[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
    linux::trim();
}
