//! SDL2, loaded at run time.
//!
//! Only functions that exist in SDL 2.0.0 are required, so any SDL2 ever
//! released will do, including the old builds that still run on Mac OS X
//! 10.5 and 10.6. Loading at run time also means no SDL headers or import
//! libraries are needed to build the game.

use super::Lib;
use alloc::boxed::Box;
use alloc::string::String;
use core::ffi::{c_char, c_int, c_void};
use core::sync::atomic::{AtomicPtr, Ordering};

/// The start of SDL2's `SDL_RWops`. The layout of these five function
/// pointers has been the same in every SDL2 release.
#[repr(C)]
pub struct RWops {
    pub size: unsafe extern "C" fn(*mut RWops) -> i64,
    pub seek: unsafe extern "C" fn(*mut RWops, i64, c_int) -> i64,
    pub read: unsafe extern "C" fn(*mut RWops, *mut c_void, usize, usize) -> usize,
    pub write: unsafe extern "C" fn(*mut RWops, *const c_void, usize, usize) -> usize,
    pub close: unsafe extern "C" fn(*mut RWops) -> c_int,
}

/// `SDL_Event` is a 56-byte union; we read fields by their fixed offsets.
#[repr(C, align(8))]
pub struct Event(pub [u8; 64]);

impl Event {
    pub fn new() -> Event {
        Event([0; 64])
    }
    pub fn u32_at(&self, o: usize) -> u32 {
        u32::from_ne_bytes([self.0[o], self.0[o + 1], self.0[o + 2], self.0[o + 3]])
    }
    pub fn i32_at(&self, o: usize) -> i32 {
        self.u32_at(o) as i32
    }
    pub fn u8_at(&self, o: usize) -> u8 {
        self.0[o]
    }
}

impl Default for Event {
    fn default() -> Self {
        Event::new()
    }
}

pub const INIT_VIDEO: u32 = 0x20;
pub const WINDOW_FULLSCREEN_DESKTOP: u32 = 0x1001;
/// Real full screen: the display changes to the window's size.
pub const WINDOW_FULLSCREEN: u32 = 0x1;
pub const WINDOW_OPENGL: u32 = 0x2;
pub const WINDOW_HIDDEN: u32 = 0x8;
pub const WINDOW_RESIZABLE: u32 = 0x20;
pub const WINDOWPOS_CENTERED: c_int = 0x2FFF_0000;
pub const GL_DOUBLEBUFFER: c_int = 5;
pub const GL_DEPTH_SIZE: c_int = 6;

pub const EV_QUIT: u32 = 0x100;
pub const EV_WINDOW: u32 = 0x200;
pub const EV_KEYDOWN: u32 = 0x300;
pub const EV_KEYUP: u32 = 0x301;
pub const EV_TEXTINPUT: u32 = 0x303;
pub const EV_MOUSEMOTION: u32 = 0x400;
pub const EV_MOUSEDOWN: u32 = 0x401;
pub const EV_MOUSEUP: u32 = 0x402;
pub const EV_MOUSEWHEEL: u32 = 0x403;
pub const WINEV_SIZE_CHANGED: u8 = 6;
pub const WINEV_FOCUS_LOST: u8 = 13;

pub type ThreadFn = unsafe extern "C" fn(*mut c_void) -> c_int;

macro_rules! sdl_api {
    ($(fn $name:ident($($t:ty),*) $(-> $r:ty)?;)*) => {
        #[allow(non_snake_case)]
        pub struct Api {
            $(pub $name: unsafe extern "C" fn($($t),*) $(-> $r)?,)*
            /// SDL 2.0.1 and later; absent in 2.0.0.
            pub SDL_GetPrefPath: Option<unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_char>,
        }
        impl Api {
            unsafe fn load(lib: &Lib) -> Result<Api, &'static str> {
                Ok(Api {
                    $($name: {
                        let p = lib.sym(concat!(stringify!($name), "\0").as_bytes());
                        if p.is_null() {
                            return Err(stringify!($name));
                        }
                        core::mem::transmute::<*mut c_void, unsafe extern "C" fn($($t),*) $(-> $r)?>(p)
                    },)*
                    SDL_GetPrefPath: {
                        let p = lib.sym(b"SDL_GetPrefPath\0");
                        if p.is_null() {
                            None
                        } else {
                            Some(core::mem::transmute::<
                                *mut c_void,
                                unsafe extern "C" fn(*const c_char, *const c_char) -> *mut c_char,
                            >(p))
                        }
                    },
                })
            }
        }
    };
}

sdl_api! {
    fn SDL_Init(u32) -> c_int;
    fn SDL_Quit();
    fn SDL_GetError() -> *const c_char;
    fn SDL_CreateWindow(*const c_char, c_int, c_int, c_int, c_int, u32) -> *mut c_void;
    fn SDL_DestroyWindow(*mut c_void);
    fn SDL_GetWindowSize(*mut c_void, *mut c_int, *mut c_int);
    fn SDL_SetWindowFullscreen(*mut c_void, u32) -> c_int;
    fn SDL_SetWindowSize(*mut c_void, c_int, c_int);
    fn SDL_SetWindowTitle(*mut c_void, *const c_char);
    fn SDL_ShowSimpleMessageBox(u32, *const c_char, *const c_char, *mut c_void) -> c_int;
    fn SDL_GL_SetAttribute(c_int, c_int) -> c_int;
    fn SDL_GL_CreateContext(*mut c_void) -> *mut c_void;
    fn SDL_GL_DeleteContext(*mut c_void);
    fn SDL_GL_SetSwapInterval(c_int) -> c_int;
    fn SDL_GL_SwapWindow(*mut c_void);
    fn SDL_GL_GetProcAddress(*const c_char) -> *mut c_void;
    fn SDL_PollEvent(*mut Event) -> c_int;
    fn SDL_SetRelativeMouseMode(c_int) -> c_int;
    fn SDL_GetPerformanceCounter() -> u64;
    fn SDL_GetPerformanceFrequency() -> u64;
    fn SDL_Delay(u32);
    fn SDL_GetCPUCount() -> c_int;
    fn SDL_WaitThread(*mut c_void, *mut c_int);
    fn SDL_CreateMutex() -> *mut c_void;
    fn SDL_LockMutex(*mut c_void) -> c_int;
    fn SDL_UnlockMutex(*mut c_void) -> c_int;
    fn SDL_DestroyMutex(*mut c_void);
    fn SDL_CreateSemaphore(u32) -> *mut c_void;
    fn SDL_SemWait(*mut c_void) -> c_int;
    fn SDL_SemPost(*mut c_void) -> c_int;
    fn SDL_DestroySemaphore(*mut c_void);
    fn SDL_RWFromFile(*const c_char, *const c_char) -> *mut RWops;
    fn SDL_free(*mut c_void);
    fn SDL_InitSubSystem(u32) -> c_int;
    fn SDL_OpenAudioDevice(*const c_char, c_int, *const c_void, *mut c_void, c_int) -> u32;
    fn SDL_PauseAudioDevice(u32, c_int);
    fn SDL_CloseAudioDevice(u32);
    fn SDL_LockAudioDevice(u32);
    fn SDL_UnlockAudioDevice(u32);
    fn SDL_CreateRGBSurfaceFrom(*mut c_void, c_int, c_int, c_int, c_int, u32, u32, u32, u32) -> *mut c_void;
    fn SDL_FreeSurface(*mut c_void);
    fn SDL_SetWindowIcon(*mut c_void, *mut c_void);
    fn SDL_GetScancodeName(c_int) -> *const c_char;
    fn SDL_StartTextInput();
    fn SDL_GetClipboardText() -> *mut c_char;
    fn SDL_SetClipboardText(*const c_char) -> c_int;
}

static API: AtomicPtr<Api> = AtomicPtr::new(core::ptr::null_mut());
static THREAD_CREATE: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());

/// The loaded SDL2. Panics if [`load`] has not succeeded yet.
pub fn sdl() -> &'static Api {
    let p = API.load(Ordering::Acquire);
    assert!(!p.is_null(), "SDL2 is not loaded");
    unsafe { &*p }
}

pub fn loaded() -> bool {
    !API.load(Ordering::Acquire).is_null()
}

#[cfg(target_os = "linux")]
const NAMES: &[&[u8]] = &[b"libSDL2-2.0.so.0\0", b"libSDL2.so\0", b"libSDL2-2.0.so\0"];
#[cfg(all(unix, not(target_os = "linux"), not(target_os = "macos")))]
const NAMES: &[&[u8]] = &[b"libSDL2.so\0", b"libSDL2-2.0.so.0\0", b"libSDL2-2.0.so\0"];
#[cfg(target_os = "macos")]
const NAMES: &[&[u8]] = &[
    b"@executable_path/libSDL2-2.0.0.dylib\0",
    b"@executable_path/../Frameworks/SDL2.framework/SDL2\0",
    b"@executable_path/SDL2.framework/SDL2\0",
    b"libSDL2-2.0.0.dylib\0",
    b"/Library/Frameworks/SDL2.framework/SDL2\0",
    b"/opt/local/lib/libSDL2-2.0.0.dylib\0",
    b"/usr/local/lib/libSDL2-2.0.0.dylib\0",
    b"/opt/homebrew/lib/libSDL2-2.0.0.dylib\0",
    b"/sw/lib/libSDL2-2.0.0.dylib\0",
];
#[cfg(windows)]
const NAMES: &[&[u8]] = &[b"SDL2.dll\0"];
// In a browser "SDL2" is the handful of its functions in `sys/web/sdl.rs`.
#[cfg(target_arch = "wasm32")]
const NAMES: &[&[u8]] = &[b"web\0"];

/// Find and load SDL2. `VOXELRUST_SDL2` may name the library file to use.
pub fn load() -> Result<(), String> {
    if loaded() {
        return Ok(());
    }
    let mut lib = None;
    if let Some(path) = super::env("VOXELRUST_SDL2") {
        lib = Lib::open(&super::cstr(&path));
    }
    if lib.is_none() {
        for n in NAMES {
            lib = Lib::open(n);
            if lib.is_some() {
                break;
            }
        }
    }
    let lib = match lib {
        Some(l) => l,
        None => {
            return Err(String::from(
                "SDL2 was not found. Install SDL2 (or put its library next to the game), \
                 or set VOXELRUST_SDL2 to the full path of the SDL2 library.",
            ))
        }
    };
    let api = unsafe { Api::load(&lib) }.map_err(|missing| {
        alloc::format!("the SDL2 library that was found has no `{}` function", missing)
    })?;
    let tc = lib.sym(b"SDL_CreateThread\0");
    if tc.is_null() {
        return Err(String::from("the SDL2 library that was found has no `SDL_CreateThread` function"));
    }
    THREAD_CREATE.store(tc, Ordering::Release);
    API.store(Box::into_raw(Box::new(api)), Ordering::Release);
    // `lib` is never closed: SDL2 stays loaded for the life of the process.
    Ok(())
}

/// `SDL_CreateThread`. On Windows the real function takes two extra
/// arguments (the C runtime's thread start/end hooks); null means "use
/// CreateThread", which is what we want without a C runtime of our own.
///
/// # Safety
/// `f` and `data` must be valid for the new thread; `name` NUL-terminated.
pub unsafe fn create_thread(f: ThreadFn, name: *const c_char, data: *mut c_void) -> *mut c_void {
    let p = THREAD_CREATE.load(Ordering::Acquire);
    #[cfg(windows)]
    {
        let func: unsafe extern "C" fn(ThreadFn, *const c_char, *mut c_void, *mut c_void, *mut c_void) -> *mut c_void =
            core::mem::transmute(p);
        func(f, name, data, core::ptr::null_mut(), core::ptr::null_mut())
    }
    #[cfg(not(windows))]
    {
        let func: unsafe extern "C" fn(ThreadFn, *const c_char, *mut c_void) -> *mut c_void =
            core::mem::transmute(p);
        func(f, name, data)
    }
}

pub fn error() -> String {
    unsafe { super::from_cstr((sdl().SDL_GetError)()) }
}
