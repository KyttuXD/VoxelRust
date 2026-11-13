//! The browser backend (`wasm32-unknown-unknown`).
//!
//! The rest of the game asks the platform layer for three things: a few C
//! library functions, SDL2, and OpenGL 1.5. A browser has none of them, so
//! this folder supplies just the parts the game uses, under the same names:
//!
//! - `mod.rs` (this file): the C functions (`malloc`, `puts`, `abort`...)
//!   and the functions the page calls: `voxelrust_start`, `voxelrust_frame`
//!   and `voxelrust_audio`;
//! - `heap.rs`: the allocator behind `malloc`;
//! - `sdl.rs`: the SDL2 functions, looked up by name exactly as they are in
//!   a real SDL2 library, so `sys/sdl.rs` and `sys/thread.rs` are used as
//!   they are;
//! - `gl.rs`: the OpenGL functions, imported from JavaScript, which draws
//!   them with WebGL (`web/voxelrust.js`);
//! - `net.rs`: sockets that always fail (a page cannot open them).
//!
//! Everything imported from JavaScript is in the `env` module; the page
//! must supply every one of those functions or the program will not start.
//! See docs/WEB.md.

pub mod heap;
#[cfg(target_arch = "wasm32")]
mod gl;
#[cfg(target_arch = "wasm32")]
mod sdl;
#[cfg(target_arch = "wasm32")]
pub use glue::*;

#[cfg(target_arch = "wasm32")]
mod glue {
    use super::heap::Heap;
    use alloc::boxed::Box;
    use alloc::string::String;
    use core::ffi::{c_char, c_int, c_void};

    #[link(wasm_import_module = "env")]
    extern "C" {
        /// Print one line (UTF-8, not NUL-terminated) to the console.
        fn js_log(text: *const u8, len: usize);
        /// Tell the player something without stopping the game.
        fn js_notice(text: *const u8, len: usize);
    }

    // ---------------------------------------------------------- C library

    unsafe fn more(bytes: usize) -> *mut u8 {
        let page = 65536;
        let old = core::arch::wasm32::memory_grow(0, bytes / page);
        if old == usize::MAX {
            core::ptr::null_mut()
        } else {
            (old * page) as *mut u8
        }
    }

    static mut HEAP: Heap = Heap::new(more);

    fn heap() -> &'static mut Heap {
        // SAFETY: the browser build has one thread, and no call into the
        // heap is made while another is in progress.
        unsafe { &mut *core::ptr::addr_of_mut!(HEAP) }
    }

    #[no_mangle]
    pub unsafe extern "C" fn malloc(n: usize) -> *mut c_void {
        heap().alloc(n) as *mut c_void
    }

    #[no_mangle]
    pub unsafe extern "C" fn realloc(p: *mut c_void, n: usize) -> *mut c_void {
        heap().realloc(p as *mut u8, n) as *mut c_void
    }

    #[no_mangle]
    pub unsafe extern "C" fn free(p: *mut c_void) {
        heap().free(p as *mut u8)
    }

    // The optimiser turns malloc + zeroing into calloc, so there must be one.
    #[no_mangle]
    pub unsafe extern "C" fn calloc(count: usize, size: usize) -> *mut c_void {
        let n = match count.checked_mul(size) {
            Some(n) => n,
            None => return core::ptr::null_mut(),
        };
        let p = heap().alloc(n);
        if !p.is_null() {
            core::ptr::write_bytes(p, 0, n);
        }
        p as *mut c_void
    }

    /// Stops the program: the page sees a trap and shows the last line
    /// logged (the panic message).
    #[no_mangle]
    pub extern "C" fn abort() -> ! {
        core::arch::wasm32::unreachable()
    }

    #[no_mangle]
    pub extern "C" fn getenv(_name: *const c_char) -> *const c_char {
        core::ptr::null()
    }

    #[no_mangle]
    pub unsafe extern "C" fn puts(s: *const c_char) -> c_int {
        let mut n = 0;
        while *s.add(n) != 0 {
            n += 1;
        }
        js_log(s as *const u8, n);
        0
    }

    #[no_mangle]
    pub extern "C" fn fflush(_file: *mut c_void) -> c_int {
        0
    }

    /// There is no shell to run commands in. The game runs one command,
    /// `curl`, to download map data for Earth worlds; rather than leave the
    /// player in an empty sea wondering why, say so (once).
    #[no_mangle]
    pub extern "C" fn system(_command: *const c_char) -> c_int {
        static mut TOLD: bool = false;
        unsafe {
            if !TOLD {
                TOLD = true;
                let text = "Earth worlds need the desktop version of VoxelRust: a web page cannot download the map data, so this world has no land.";
                js_notice(text.as_ptr(), text.len());
            }
        }
        -1
    }

    // ----------------------------------------------------- "SDL2" library

    /// What `Lib::open` gets: there is one library, the one in this folder.
    pub fn open(_name: &[u8]) -> *mut c_void {
        1 as *mut c_void
    }

    /// What `Lib::sym` gets: the function of that name, or null.
    pub fn sym(_lib: *mut c_void, name: &[u8]) -> *mut c_void {
        super::sdl::sym(name)
    }

    // --------------------------------------------------------- frame loop

    /// One pass of the game's main loop; `Ok(false)` once the game has ended.
    pub type Frames = Box<dyn FnMut() -> Result<bool, String>>;

    static mut FRAMES: Option<Frames> = None;

    /// Called by `app::run` instead of looping: keep the loop, and let the
    /// page run it a frame at a time.
    pub fn run_frames(frames: Frames) -> Result<(), String> {
        unsafe { *core::ptr::addr_of_mut!(FRAMES) = Some(frames) };
        Ok(())
    }

    /// Start the game. Returns 0 when it is ready for `voxelrust_frame`;
    /// anything else means it could not start (the reason has been shown).
    #[no_mangle]
    pub extern "C" fn voxelrust_start() -> c_int {
        let code = crate::app::main(0, core::ptr::null());
        if code == 0 && unsafe { (*core::ptr::addr_of!(FRAMES)).is_none() } {
            return 1;
        }
        code
    }

    /// Run one frame. Returns 1 to be called again for the next frame, 0
    /// when the game has ended (the player chose QUIT, or it failed and has
    /// shown why).
    #[no_mangle]
    pub extern "C" fn voxelrust_frame() -> c_int {
        let slot = unsafe { &mut *core::ptr::addr_of_mut!(FRAMES) };
        let result = match slot.as_mut() {
            Some(frames) => frames(),
            None => return 0,
        };
        match result {
            Ok(true) => 1,
            Ok(false) => {
                *slot = None;
                0
            }
            Err(msg) => {
                *slot = None;
                crate::sys::log(&alloc::format!("voxelrust: {}", msg));
                super::sdl::message(&msg);
                0
            }
        }
    }

    /// Mix the next `frames` samples of sound (signed 16-bit, mono, at the
    /// rate given to `js_audio_open`). Returns where they are, or null when
    /// there is no sound to play.
    #[no_mangle]
    pub extern "C" fn voxelrust_audio(frames: c_int) -> *const i16 {
        super::sdl::mix_audio(frames)
    }
}
