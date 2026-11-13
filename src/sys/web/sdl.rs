//! The part of SDL2 the game uses, for a browser.
//!
//! Each function has SDL's name and signature and is found by name through
//! [`sym`], just as `sys/sdl.rs` finds them in a real SDL2 library. Most
//! are a line or two: either they have no meaning in a page (threads, the
//! window's icon) or they pass the request on to JavaScript.
#![allow(non_snake_case)]

use crate::sys::sdl::{Event, RWops, ThreadFn};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void};

#[link(wasm_import_module = "env")]
extern "C" {
    /// Milliseconds since the page loaded.
    fn js_now() -> f64;
    /// Size of a stored file in bytes, or -1 if there is none.
    fn js_file_size(path: *const u8, path_len: usize) -> c_int;
    /// Copy a stored file to `dst` (at most `cap` bytes); returns the count.
    fn js_file_read(path: *const u8, path_len: usize, dst: *mut u8, cap: usize) -> c_int;
    /// Store a file (or add to the end of it); 1 if it was stored.
    fn js_file_write(path: *const u8, path_len: usize, src: *const u8, len: usize, append: c_int) -> c_int;
    /// Set up the canvas and WebGL; 0 if WebGL is not available.
    fn js_window_open(fullscreen: c_int) -> c_int;
    fn js_window_width() -> c_int;
    fn js_window_height() -> c_int;
    fn js_fullscreen(on: c_int);
    /// Capture the mouse for looking around (1) or let it go (0).
    fn js_pointer_lock(on: c_int);
    /// Write the next input event, laid out as an `SDL_Event`, to `event`
    /// (56 bytes); 0 when there are none waiting.
    fn js_poll_event(event: *mut u8) -> c_int;
    /// Show a message the player must see (the game cannot go on).
    fn js_message(text: *const u8, len: usize);
    /// Start playing what `voxelrust_audio` produces, `rate` samples a
    /// second; 0 if the browser has no sound.
    fn js_audio_open(rate: c_int) -> c_int;
    fn js_audio_pause(paused: c_int);
    fn js_audio_close();
}

fn c_len(s: *const c_char) -> usize {
    let mut n = 0;
    if !s.is_null() {
        while unsafe { *s.add(n) } != 0 {
            n += 1;
        }
    }
    n
}

/// Show `text` to the player in the page's error box.
pub fn message(text: &str) {
    unsafe { js_message(text.as_ptr(), text.len()) }
}

/// A handle for things that need one but have nothing behind them here.
fn token() -> *mut c_void {
    1 as *mut c_void
}

// ------------------------------------------------------- start-up, window

unsafe extern "C" fn SDL_Init(_flags: u32) -> c_int {
    0
}
unsafe extern "C" fn SDL_InitSubSystem(_flags: u32) -> c_int {
    0
}
unsafe extern "C" fn SDL_Quit() {}

static mut NO_WEBGL: bool = false;

unsafe extern "C" fn SDL_GetError() -> *const c_char {
    let text: &[u8] = if NO_WEBGL { b"this browser has no WebGL (or it is switched off)\0" } else { b"\0" };
    text.as_ptr() as *const c_char
}

unsafe extern "C" fn SDL_CreateWindow(_title: *const c_char, _x: c_int, _y: c_int, _w: c_int, _h: c_int, flags: u32) -> *mut c_void {
    if js_window_open((flags & 1 != 0) as c_int) == 0 {
        NO_WEBGL = true;
        return core::ptr::null_mut();
    }
    token()
}
unsafe extern "C" fn SDL_DestroyWindow(_window: *mut c_void) {}

unsafe extern "C" fn SDL_GetWindowSize(_window: *mut c_void, w: *mut c_int, h: *mut c_int) {
    *w = js_window_width();
    *h = js_window_height();
}

unsafe extern "C" fn SDL_SetWindowFullscreen(_window: *mut c_void, flags: u32) -> c_int {
    js_fullscreen((flags != 0) as c_int);
    0
}
// The canvas is as large as the page; the game does not choose its size.
unsafe extern "C" fn SDL_SetWindowSize(_window: *mut c_void, _w: c_int, _h: c_int) {}
unsafe extern "C" fn SDL_SetWindowTitle(_window: *mut c_void, _title: *const c_char) {}

unsafe extern "C" fn SDL_ShowSimpleMessageBox(_flags: u32, _title: *const c_char, text: *const c_char, _window: *mut c_void) -> c_int {
    js_message(text as *const u8, c_len(text));
    0
}

// The page's icon is a file beside it.
unsafe extern "C" fn SDL_CreateRGBSurfaceFrom(_px: *mut c_void, _w: c_int, _h: c_int, _depth: c_int, _pitch: c_int, _r: u32, _g: u32, _b: u32, _a: u32) -> *mut c_void {
    core::ptr::null_mut()
}
unsafe extern "C" fn SDL_FreeSurface(_surface: *mut c_void) {}
unsafe extern "C" fn SDL_SetWindowIcon(_window: *mut c_void, _surface: *mut c_void) {}

// ------------------------------------------------------------------ OpenGL

unsafe extern "C" fn SDL_GL_SetAttribute(_attr: c_int, _value: c_int) -> c_int {
    0
}
// The WebGL context is made with the canvas, in `js_window_open`.
unsafe extern "C" fn SDL_GL_CreateContext(_window: *mut c_void) -> *mut c_void {
    token()
}
unsafe extern "C" fn SDL_GL_DeleteContext(_context: *mut c_void) {}
// The browser shows each frame when it is ready, in step with the display.
unsafe extern "C" fn SDL_GL_SetSwapInterval(_interval: c_int) -> c_int {
    0
}
unsafe extern "C" fn SDL_GL_SwapWindow(_window: *mut c_void) {}

unsafe extern "C" fn SDL_GL_GetProcAddress(name: *const c_char) -> *mut c_void {
    super::gl::sym(core::slice::from_raw_parts(name as *const u8, c_len(name)))
}

// ------------------------------------------------------------ input, time

unsafe extern "C" fn SDL_PollEvent(event: *mut Event) -> c_int {
    js_poll_event(event as *mut u8)
}

unsafe extern "C" fn SDL_SetRelativeMouseMode(on: c_int) -> c_int {
    js_pointer_lock(on);
    0
}
// Keys always produce text events in a page.
unsafe extern "C" fn SDL_StartTextInput() {}

// A page may not read the system clipboard without asking, so copy and
// paste work inside the game only: this is the text last copied.
static mut CLIP: [u8; 256] = [0; 256];

unsafe extern "C" fn SDL_SetClipboardText(text: *const c_char) -> c_int {
    let clip = &mut *core::ptr::addr_of_mut!(CLIP);
    let mut i = 0;
    while i < clip.len() - 1 && *text.add(i) != 0 {
        clip[i] = *text.add(i) as u8;
        i += 1;
    }
    clip[i] = 0;
    0
}

unsafe extern "C" fn SDL_GetClipboardText() -> *mut c_char {
    // The caller frees what it gets, so hand out a copy.
    let clip = &*core::ptr::addr_of!(CLIP);
    let out = super::malloc(clip.len()) as *mut u8;
    if !out.is_null() {
        core::ptr::copy_nonoverlapping(clip.as_ptr(), out, clip.len());
    }
    out as *mut c_char
}

/// The names SDL gives the keys, for the controls screen.
fn scancode_name(code: c_int, out: &mut [u8; 16]) {
    const LETTERS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";
    const NAMED: &[(c_int, &str)] = &[
        (40, "Return"),
        (41, "Escape"),
        (42, "Backspace"),
        (43, "Tab"),
        (44, "Space"),
        (45, "-"),
        (46, "="),
        (47, "["),
        (48, "]"),
        (49, "\\"),
        (51, ";"),
        (52, "'"),
        (53, "`"),
        (54, ","),
        (55, "."),
        (56, "/"),
        (57, "CapsLock"),
        (73, "Insert"),
        (74, "Home"),
        (75, "PageUp"),
        (76, "Delete"),
        (77, "End"),
        (78, "PageDown"),
        (79, "Right"),
        (80, "Left"),
        (81, "Down"),
        (82, "Up"),
        (84, "Keypad /"),
        (85, "Keypad *"),
        (86, "Keypad -"),
        (87, "Keypad +"),
        (88, "Keypad Enter"),
        (98, "Keypad 0"),
        (99, "Keypad ."),
        (224, "Left Ctrl"),
        (225, "Left Shift"),
        (226, "Left Alt"),
        (227, "Left GUI"),
        (228, "Right Ctrl"),
        (229, "Right Shift"),
        (230, "Right Alt"),
        (231, "Right GUI"),
    ];
    let mut put = |s: &[u8]| {
        out[..s.len()].copy_from_slice(s);
        out[s.len()] = 0;
    };
    match code {
        4..=39 => put(&LETTERS[(code - 4) as usize..(code - 3) as usize]),
        58..=66 => put(&[b'F', b'1' + (code - 58) as u8]),
        67..=69 => put(&[b'F', b'1', b'0' + (code - 67) as u8]),
        89..=97 => put(&[b'K', b'e', b'y', b'p', b'a', b'd', b' ', b'1' + (code - 89) as u8]),
        _ => match NAMED.iter().find(|n| n.0 == code) {
            Some(n) => put(n.1.as_bytes()),
            None => put(b""),
        },
    }
}

unsafe extern "C" fn SDL_GetScancodeName(code: c_int) -> *const c_char {
    // The caller copies the name at once, as with SDL's own static names.
    static mut NAME: [u8; 16] = [0; 16];
    let name = &mut *core::ptr::addr_of_mut!(NAME);
    scancode_name(code, name);
    name.as_ptr() as *const c_char
}

unsafe extern "C" fn SDL_GetPerformanceCounter() -> u64 {
    (js_now() * 1000.0) as u64
}
unsafe extern "C" fn SDL_GetPerformanceFrequency() -> u64 {
    1_000_000
}
// A page must not wait; the browser paces the frames.
unsafe extern "C" fn SDL_Delay(_ms: u32) {}

// ----------------------------------------------------------------- threads

// There are none. Starting one fails, which the job pool takes to mean
// "do the work on the caller's thread", and with a single thread a mutex
// has nothing to keep out.
unsafe extern "C" fn SDL_GetCPUCount() -> c_int {
    1
}
unsafe extern "C" fn SDL_CreateThread(_f: ThreadFn, _name: *const c_char, _data: *mut c_void) -> *mut c_void {
    core::ptr::null_mut()
}
unsafe extern "C" fn SDL_WaitThread(_thread: *mut c_void, _status: *mut c_int) {}
unsafe extern "C" fn SDL_CreateMutex() -> *mut c_void {
    token()
}
unsafe extern "C" fn SDL_LockMutex(_mutex: *mut c_void) -> c_int {
    0
}
unsafe extern "C" fn SDL_UnlockMutex(_mutex: *mut c_void) -> c_int {
    0
}
unsafe extern "C" fn SDL_DestroyMutex(_mutex: *mut c_void) {}
unsafe extern "C" fn SDL_CreateSemaphore(_value: u32) -> *mut c_void {
    token()
}
unsafe extern "C" fn SDL_SemWait(_sem: *mut c_void) -> c_int {
    0
}
unsafe extern "C" fn SDL_SemPost(_sem: *mut c_void) -> c_int {
    0
}
unsafe extern "C" fn SDL_DestroySemaphore(_sem: *mut c_void) {}

// ------------------------------------------------------------------- files

/// An open file. Files live in the browser's storage, whole: reading takes
/// a copy when the file is opened, and each write stores at once (the game
/// writes a file in one piece), so that a full store is reported by the
/// write itself.
#[repr(C)]
struct File {
    ops: RWops,
    path: Vec<u8>,
    data: Vec<u8>,
    pos: usize,
    writing: bool,
    /// The next write adds to the file instead of replacing it.
    append: bool,
    stored: bool,
}

unsafe extern "C" fn rw_size(rw: *mut RWops) -> i64 {
    (*(rw as *mut File)).data.len() as i64
}

unsafe extern "C" fn rw_seek(rw: *mut RWops, offset: i64, whence: c_int) -> i64 {
    let f = &mut *(rw as *mut File);
    let from = match whence {
        0 => 0,
        1 => f.pos as i64,
        _ => f.data.len() as i64,
    };
    f.pos = (from + offset).clamp(0, f.data.len() as i64) as usize;
    f.pos as i64
}

unsafe extern "C" fn rw_read(rw: *mut RWops, dst: *mut c_void, size: usize, count: usize) -> usize {
    let f = &mut *(rw as *mut File);
    if size == 0 || f.writing {
        return 0;
    }
    let n = count.min((f.data.len() - f.pos) / size);
    core::ptr::copy_nonoverlapping(f.data.as_ptr().add(f.pos), dst as *mut u8, n * size);
    f.pos += n * size;
    n
}

unsafe extern "C" fn rw_write(rw: *mut RWops, src: *const c_void, size: usize, count: usize) -> usize {
    let f = &mut *(rw as *mut File);
    if !f.writing {
        return 0;
    }
    if js_file_write(f.path.as_ptr(), f.path.len(), src as *const u8, size * count, f.append as c_int) == 0 {
        return 0;
    }
    f.append = true;
    f.stored = true;
    count
}

unsafe extern "C" fn rw_close(rw: *mut RWops) -> c_int {
    let f = Box::from_raw(rw as *mut File);
    if f.writing && !f.stored && !f.append {
        // Opened for writing and nothing written: an empty file.
        js_file_write(f.path.as_ptr(), f.path.len(), core::ptr::null(), 0, 0);
    }
    0
}

unsafe extern "C" fn SDL_RWFromFile(path: *const c_char, mode: *const c_char) -> *mut RWops {
    let path = core::slice::from_raw_parts(path as *const u8, c_len(path)).to_vec();
    let mode = *mode as u8;
    let mut data = Vec::new();
    if mode == b'r' {
        let size = js_file_size(path.as_ptr(), path.len());
        if size < 0 {
            return core::ptr::null_mut();
        }
        data.resize(size as usize, 0u8);
        let got = js_file_read(path.as_ptr(), path.len(), data.as_mut_ptr(), data.len());
        data.truncate(got.max(0) as usize);
    }
    let ops = RWops { size: rw_size, seek: rw_seek, read: rw_read, write: rw_write, close: rw_close };
    let file = File { ops, path, data, pos: 0, writing: mode != b'r', append: mode == b'a', stored: false };
    Box::into_raw(Box::new(file)) as *mut RWops
}

unsafe extern "C" fn SDL_free(p: *mut c_void) {
    super::free(p)
}

// ------------------------------------------------------------------- sound

/// The start of SDL's `SDL_AudioSpec`, as the game fills it in.
#[repr(C)]
struct AudioSpec {
    freq: c_int,
    format: u16,
    channels: u8,
    silence: u8,
    samples: u16,
    padding: u16,
    size: u32,
    callback: Option<unsafe extern "C" fn(*mut c_void, *mut u8, c_int)>,
    userdata: *mut c_void,
}

struct Sound {
    callback: unsafe extern "C" fn(*mut c_void, *mut u8, c_int),
    userdata: *mut c_void,
    paused: bool,
}

static mut SOUND: Option<Sound> = None;
const MIX_FRAMES: usize = 4096;
static mut MIXED: [i16; MIX_FRAMES] = [0; MIX_FRAMES];

fn sound() -> &'static mut Option<Sound> {
    // SAFETY: one thread; nothing holds this across a call.
    unsafe { &mut *core::ptr::addr_of_mut!(SOUND) }
}

unsafe extern "C" fn SDL_OpenAudioDevice(_device: *const c_char, _capture: c_int, want: *const c_void, _got: *mut c_void, _changes: c_int) -> u32 {
    let want = &*(want as *const AudioSpec);
    // Only what the game asks for: 16-bit samples, one channel.
    let callback = match want.callback {
        Some(c) if want.format == 0x8010 && want.channels == 1 && sound().is_none() => c,
        _ => return 0,
    };
    if js_audio_open(want.freq) == 0 {
        return 0;
    }
    *sound() = Some(Sound { callback, userdata: want.userdata, paused: true });
    1
}

unsafe extern "C" fn SDL_PauseAudioDevice(_device: u32, paused: c_int) {
    if let Some(s) = sound() {
        s.paused = paused != 0;
        js_audio_pause(paused);
    }
}

unsafe extern "C" fn SDL_CloseAudioDevice(_device: u32) {
    if sound().take().is_some() {
        js_audio_close();
    }
}

// Sound is mixed between frames on the one thread: nothing to lock.
unsafe extern "C" fn SDL_LockAudioDevice(_device: u32) {}
unsafe extern "C" fn SDL_UnlockAudioDevice(_device: u32) {}

/// The body of `voxelrust_audio`: have the game's callback fill the buffer.
pub fn mix_audio(frames: c_int) -> *const i16 {
    match sound() {
        Some(s) if !s.paused && frames > 0 => unsafe {
            let buf = core::ptr::addr_of_mut!(MIXED) as *mut i16;
            let n = (frames as usize).min(MIX_FRAMES);
            (s.callback)(s.userdata, buf as *mut u8, (n * 2) as c_int);
            buf as *const i16
        },
        _ => core::ptr::null(),
    }
}

// ------------------------------------------------------------------ lookup

macro_rules! lookup {
    ($name:expr; $($f:ident)*) => {
        $(if $name == stringify!($f).as_bytes() {
            return $f as *const () as *mut c_void;
        })*
    };
}

/// The function called `name` (with or without its closing NUL), or null.
/// `SDL_GetPrefPath` is deliberately absent: files have no folder here.
pub fn sym(name: &[u8]) -> *mut c_void {
    let name = name.strip_suffix(b"\0").unwrap_or(name);
    lookup! { name;
        SDL_Init SDL_Quit SDL_GetError SDL_CreateWindow SDL_DestroyWindow SDL_GetWindowSize
        SDL_SetWindowFullscreen SDL_SetWindowSize SDL_SetWindowTitle SDL_ShowSimpleMessageBox
        SDL_GL_SetAttribute SDL_GL_CreateContext SDL_GL_DeleteContext SDL_GL_SetSwapInterval
        SDL_GL_SwapWindow SDL_GL_GetProcAddress SDL_PollEvent SDL_SetRelativeMouseMode
        SDL_GetPerformanceCounter SDL_GetPerformanceFrequency SDL_Delay SDL_GetCPUCount
        SDL_WaitThread SDL_CreateMutex SDL_LockMutex SDL_UnlockMutex SDL_DestroyMutex
        SDL_CreateSemaphore SDL_SemWait SDL_SemPost SDL_DestroySemaphore SDL_RWFromFile SDL_free
        SDL_InitSubSystem SDL_OpenAudioDevice SDL_PauseAudioDevice SDL_CloseAudioDevice
        SDL_LockAudioDevice SDL_UnlockAudioDevice SDL_CreateRGBSurfaceFrom SDL_FreeSurface
        SDL_SetWindowIcon SDL_GetScancodeName SDL_StartTextInput SDL_CreateThread
        SDL_GetClipboardText SDL_SetClipboardText
    }
    core::ptr::null_mut()
}
