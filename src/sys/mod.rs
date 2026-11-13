//! The platform layer: the only place that talks to the C library directly.
//!
//! It is deliberately tiny. Memory comes from `malloc`, SDL2 is opened at run
//! time with `dlopen` / `LoadLibraryA`, and everything else (threads, files,
//! clocks, the window) goes through SDL2. Nothing here needs an API newer
//! than Mac OS X 10.4, Windows XP or any glibc.

pub mod gl;
#[cfg(not(target_arch = "wasm32"))]
pub mod net;
pub mod sdl;
pub mod thread;

// The browser build (docs/WEB.md): the C functions declared below, SDL2 and
// OpenGL do not exist there, so `web/` supplies them, and sockets that
// always fail take the place of `net`.
#[cfg(target_arch = "wasm32")]
pub mod web;
#[cfg(target_arch = "wasm32")]
#[path = "web/net.rs"]
pub mod net;
// Its allocator is plain Rust: test it on every platform.
#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "web/heap.rs"]
mod web_heap;

use alloc::string::String;
use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};
use core::ffi::{c_char, c_int, c_void};

#[cfg(not(windows))]
#[cfg_attr(all(unix, not(target_os = "macos")), link(name = "c"))]
#[cfg_attr(target_os = "linux", link(name = "dl"))]
#[cfg_attr(target_os = "macos", link(name = "System"))]
extern "C" {
    fn system(command: *const c_char) -> c_int;
    fn malloc(n: usize) -> *mut c_void;
    fn realloc(p: *mut c_void, n: usize) -> *mut c_void;
    fn free(p: *mut c_void);
    fn abort() -> !;
    fn getenv(name: *const c_char) -> *const c_char;
    fn puts(s: *const c_char) -> c_int;
    fn fflush(f: *mut c_void) -> c_int;
}

#[cfg(unix)]
extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

// Windows. `raw-dylib` makes the compiler write the import tables itself, so
// no Windows SDK, MinGW or import library is needed to build, from any host.
// msvcrt.dll is the C runtime that is part of Windows itself (XP to 11); the
// compiler's own calls to memcpy and friends resolve to it as well.
#[cfg(windows)]
#[link(name = "msvcrt", kind = "raw-dylib")]
extern "C" {
    fn system(command: *const c_char) -> c_int;
    fn malloc(n: usize) -> *mut c_void;
    fn realloc(p: *mut c_void, n: usize) -> *mut c_void;
    fn free(p: *mut c_void);
    fn abort() -> !;
    fn getenv(name: *const c_char) -> *const c_char;
    fn puts(s: *const c_char) -> c_int;
    fn fflush(f: *mut c_void) -> c_int;
    fn exit(code: c_int) -> !;
    fn __getmainargs(argc: *mut c_int, argv: *mut *mut *mut c_char, env: *mut *mut *mut c_char, wildcard: c_int, startinfo: *mut c_int) -> c_int;
    pub fn memcpy(d: *mut c_void, s: *const c_void, n: usize) -> *mut c_void;
    pub fn memmove(d: *mut c_void, s: *const c_void, n: usize) -> *mut c_void;
    pub fn memset(d: *mut c_void, c: c_int, n: usize) -> *mut c_void;
    pub fn memcmp(a: *const c_void, b: *const c_void, n: usize) -> c_int;
    // The optimiser turns malloc + zeroing into calloc.
    pub fn calloc(n: usize, size: usize) -> *mut c_void;
    // Named by the unwind tables of the precompiled `core`; never called,
    // because panics abort.
    pub fn __CxxFrameHandler3();
}

#[cfg(windows)]
#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    fn LoadLibraryA(name: *const c_char) -> *mut c_void;
    fn GetProcAddress(handle: *mut c_void, name: *const c_char) -> *mut c_void;
}

/// The whole of the Windows start-up code: fetch the command line from the C
/// runtime, run the program, exit. Called from `mainCRTStartup` in main.rs.
///
/// # Safety
/// Must be called once, as the process entry point.
#[cfg(windows)]
pub unsafe fn windows_entry(main: extern "C" fn(c_int, *const *const c_char) -> c_int) -> ! {
    let mut argc: c_int = 0;
    let mut argv: *mut *mut c_char = core::ptr::null_mut();
    let mut env: *mut *mut c_char = core::ptr::null_mut();
    let mut startinfo: c_int = 0;
    __getmainargs(&mut argc, &mut argv, &mut env, 0, &mut startinfo);
    if argv.is_null() {
        argc = 0;
    }
    exit(main(argc, argv as *const *const c_char))
}

// The MSVC ABI expects two things from the C runtime's static part, which we
// do not link: a marker that floating point is used, and the stack probe
// called by any function with more than a page of locals.
#[cfg(all(windows, target_env = "msvc", not(test)))]
#[no_mangle]
pub static _fltused: i32 = 0x9875;

#[cfg(all(windows, target_env = "msvc", target_arch = "x86_64", not(test)))]
core::arch::global_asm!(
    ".globl __chkstk",
    "__chkstk:",
    // In: rax = bytes about to be allocated. Touch each page going down so
    // the guard page is hit in order. Preserves every register but r10/r11.
    "mov r10, rcx",
    "mov r11, rax",
    "lea rcx, [rsp + 8]",
    "cmp r11, 0x1000",
    "jb 3f",
    "2:",
    "sub rcx, 0x1000",
    "test qword ptr [rcx], rcx",
    "sub r11, 0x1000",
    "cmp r11, 0x1000",
    "ja 2b",
    "3:",
    "sub rcx, r11",
    "test qword ptr [rcx], rcx",
    "mov rcx, r10",
    "ret",
);

// ---------------------------------------------------------------- allocator

/// `malloc` already aligns to this much on every platform we target.
const MIN_ALIGN: usize = if cfg!(target_pointer_width = "64") { 16 } else { 8 };

pub struct CAlloc;

unsafe impl GlobalAlloc for CAlloc {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if l.align() <= MIN_ALIGN {
            return malloc(l.size()) as *mut u8;
        }
        // Over-aligned: allocate extra and keep the real pointer just below.
        let extra = l.align() + core::mem::size_of::<usize>();
        let raw = malloc(l.size() + extra) as *mut u8;
        if raw.is_null() {
            return raw;
        }
        let start = raw as usize + core::mem::size_of::<usize>();
        let aligned = (start + l.align() - 1) & !(l.align() - 1);
        *((aligned - core::mem::size_of::<usize>()) as *mut usize) = raw as usize;
        aligned as *mut u8
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if l.align() <= MIN_ALIGN {
            free(p as *mut c_void);
        } else {
            let raw = *((p as usize - core::mem::size_of::<usize>()) as *const usize);
            free(raw as *mut c_void);
        }
    }

    unsafe fn realloc(&self, p: *mut u8, l: Layout, new_size: usize) -> *mut u8 {
        if l.align() <= MIN_ALIGN {
            return realloc(p as *mut c_void, new_size) as *mut u8;
        }
        let nl = Layout::from_size_align_unchecked(new_size, l.align());
        let np = self.alloc(nl);
        if !np.is_null() {
            core::ptr::copy_nonoverlapping(p, np, core::cmp::min(l.size(), new_size));
            self.dealloc(p, l);
        }
        np
    }
}

#[cfg(not(test))]
#[global_allocator]
static ALLOC: CAlloc = CAlloc;

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    log(&alloc::format!("voxelrust: internal error: {}", info));
    unsafe { abort() }
}

// The precompiled `core` and `alloc` refer to these two symbols in unoptimised
// builds even though we never unwind (panic = "abort"). Without `std` nobody
// defines them, so we do.
#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn rust_eh_personality() {}

#[cfg(not(test))]
#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn _Unwind_Resume() -> ! {
    unsafe { abort() }
}

// ------------------------------------------------------------------ strings

/// A NUL-terminated copy of `s`, for passing to C.
pub fn cstr(s: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len() + 1);
    v.extend(s.bytes().filter(|&b| b != 0));
    v.push(0);
    v
}

/// Copy a C string into a Rust `String` (lossy for non-UTF-8 bytes).
///
/// # Safety
/// `p` must be null or point at a NUL-terminated string.
pub unsafe fn from_cstr(p: *const c_char) -> String {
    let mut s = String::new();
    if p.is_null() {
        return s;
    }
    let mut q = p as *const u8;
    while *q != 0 {
        let b = *q;
        s.push(if b < 128 { b as char } else { '?' });
        q = q.add(1);
    }
    s
}

pub fn env(name: &str) -> Option<String> {
    let n = cstr(name);
    let p = unsafe { getenv(n.as_ptr() as *const c_char) };
    if p.is_null() {
        None
    } else {
        Some(unsafe { from_cstr(p) })
    }
}

static LOG_FILE: core::sync::atomic::AtomicPtr<String> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// Also append every log line to this file (programs started from a
/// desktop icon have nowhere to print).
pub fn log_to_file(path: &str) {
    let p = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(String::from(path)));
    LOG_FILE.store(p, core::sync::atomic::Ordering::Release);
}

/// Print a line to standard output (and the log file, if one is set).
pub fn log(s: &str) {
    let c = cstr(s);
    unsafe {
        puts(c.as_ptr() as *const c_char);
        fflush(core::ptr::null_mut());
        if let (Some(path), true) = (LOG_FILE.load(core::sync::atomic::Ordering::Acquire).as_ref(), sdl::loaded()) {
            let p = cstr(path);
            let rw = (sdl::sdl().SDL_RWFromFile)(p.as_ptr() as *const c_char, b"ab\0".as_ptr() as *const c_char);
            if !rw.is_null() {
                ((*rw).write)(rw, s.as_ptr() as *const c_void, 1, s.len());
                ((*rw).write)(rw, b"\n".as_ptr() as *const c_void, 1, 1);
                ((*rw).close)(rw);
            }
        }
    }
}

#[macro_export]
macro_rules! logf {
    ($($a:tt)*) => { $crate::sys::log(&alloc::format!($($a)*)) };
}

/// Run a command line through the system shell and wait for it. Returns
/// its exit status (0 = success), or -1 if it could not be started.
/// Callers must build `command` only from text they fully control.
pub fn run_command(command: &str) -> i32 {
    let c = cstr(command);
    unsafe { system(c.as_ptr() as *const c_char) }
}

/// Pause the calling thread. Does nothing before SDL is loaded.
pub fn sleep_ms(ms: u32) {
    if sdl::loaded() {
        unsafe { (sdl::sdl().SDL_Delay)(ms) };
    }
}

/// Pause the calling thread for real, SDL or not (for the updater).
pub fn sleep_real_ms(ms: u32) {
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    unsafe {
        usleep(ms.saturating_mul(1000));
    }
    #[cfg(windows)]
    unsafe {
        Sleep(ms);
    }
    #[cfg(not(any(all(unix, not(target_arch = "wasm32")), windows)))]
    let _ = ms;
}

/// `text` as one word for [`run_command`]: quoted so that the shell reads it
/// as exactly `text`. None where the shell (or cmd on Windows) cannot be
/// made to read it that way: quotes, percent signs and line breaks.
pub fn shell_arg(text: &str) -> Option<String> {
    #[cfg(windows)]
    {
        if text.contains(|c| c == '"' || c == '%' || c == '\r' || c == '\n') {
            return None;
        }
        Some(alloc::format!("\"{}\"", text))
    }
    #[cfg(not(windows))]
    {
        let mut out = String::from("'");
        for c in text.chars() {
            if c == '\'' {
                out.push_str("'\\''");
            } else {
                out.push(c);
            }
        }
        out.push('\'');
        Some(out)
    }
}

/// Start the program at `exe` with the given arguments (already quoted) as a
/// separate process, and return at once; the caller is expected to exit
/// straight after. Returns whether it started.
pub fn start_program(exe: &str, args: &str) -> bool {
    let Some(q) = shell_arg(exe) else { return false };
    #[cfg(windows)]
    {
        run_command(&alloc::format!("start \"\" {} {}", q, args)) == 0
    }
    #[cfg(not(windows))]
    {
        run_command(&alloc::format!("{} {} </dev/null >/dev/null 2>&1 &", q, args)) == 0
    }
}

pub fn exit_abort() -> ! {
    unsafe { abort() }
}

// --------------------------------------------------------- shared libraries

pub struct Lib(*mut c_void);

impl Lib {
    /// `name` must be NUL-terminated.
    pub fn open(name: &[u8]) -> Option<Lib> {
        debug_assert!(name.last() == Some(&0));
        #[cfg(unix)]
        let h = unsafe {
            // RTLD_NOW | RTLD_GLOBAL (the value of GLOBAL differs per system).
            let global = if cfg!(target_os = "macos") { 0x8 } else { 0x100 };
            dlopen(name.as_ptr() as *const c_char, 0x2 | global)
        };
        #[cfg(windows)]
        let h = unsafe { LoadLibraryA(name.as_ptr() as *const c_char) };
        #[cfg(target_arch = "wasm32")]
        let h = web::open(name);
        if h.is_null() {
            None
        } else {
            Some(Lib(h))
        }
    }

    /// `name` must be NUL-terminated. Returns null when the symbol is absent.
    pub fn sym(&self, name: &[u8]) -> *mut c_void {
        debug_assert!(name.last() == Some(&0));
        #[cfg(unix)]
        unsafe {
            dlsym(self.0, name.as_ptr() as *const c_char)
        }
        #[cfg(windows)]
        unsafe {
            GetProcAddress(self.0, name.as_ptr() as *const c_char)
        }
        #[cfg(target_arch = "wasm32")]
        {
            web::sym(self.0, name)
        }
    }
}

// -------------------------------------------------------------------- files

#[cfg(unix)]
extern "C" {
    fn unlink(path: *const c_char) -> c_int;
    fn mkdir(path: *const c_char, mode: u32) -> c_int;
    fn opendir(path: *const c_char) -> *mut c_void;
    fn closedir(dir: *mut c_void) -> c_int;
    // On Linux the 64-bit form has one layout on every processor; on Intel
    // Macs the plain name is the classic layout, the same from 10.5 to now.
    #[cfg_attr(target_os = "linux", link_name = "readdir64")]
    fn readdir(dir: *mut c_void) -> *const u8;
}

#[cfg(windows)]
#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    fn FindFirstFileA(pattern: *const c_char, data: *mut u8) -> *mut c_void;
    fn FindNextFileA(handle: *mut c_void, data: *mut u8) -> c_int;
    fn FindClose(handle: *mut c_void) -> c_int;
    fn DeleteFileA(name: *const c_char) -> c_int;
    fn CreateDirectoryA(name: *const c_char, security: *mut c_void) -> c_int;
}

#[cfg(target_os = "linux")]
extern "C" {
    fn readlink(path: *const c_char, buf: *mut u8, size: usize) -> isize;
}

#[cfg(target_os = "macos")]
extern "C" {
    fn _NSGetExecutablePath(buf: *mut u8, size: *mut u32) -> c_int;
}

#[cfg(all(unix, not(target_arch = "wasm32")))]
extern "C" {
    fn rename(from: *const c_char, to: *const c_char) -> c_int;
    fn chmod(path: *const c_char, mode: u32) -> c_int;
    fn usleep(usec: u32) -> c_int;
}

#[cfg(windows)]
#[link(name = "kernel32", kind = "raw-dylib")]
extern "system" {
    fn GetModuleFileNameA(module: *mut c_void, buf: *mut u8, size: u32) -> u32;
    fn MoveFileExA(from: *const c_char, to: *const c_char, flags: u32) -> c_int;
    fn Sleep(ms: u32);
}

/// The full path of the running program; empty if it cannot be told.
pub fn exe_path() -> String {
    #[allow(unused_mut)]
    let mut buf = [0u8; 2048];
    #[allow(unused_assignments, unused_mut)]
    let mut n = 0usize;
    #[cfg(target_os = "linux")]
    unsafe {
        n = readlink(b"/proc/self/exe\0".as_ptr() as *const c_char, buf.as_mut_ptr(), buf.len() - 1).max(0) as usize;
    }
    #[cfg(target_os = "macos")]
    unsafe {
        let mut size = buf.len() as u32 - 1;
        if _NSGetExecutablePath(buf.as_mut_ptr(), &mut size) == 0 {
            n = buf.iter().position(|&b| b == 0).unwrap_or(0);
        }
    }
    #[cfg(windows)]
    unsafe {
        n = GetModuleFileNameA(core::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32 - 1) as usize;
    }
    String::from_utf8_lossy(&buf[..n.min(buf.len())]).into_owned()
}

/// The folder the running program is in, with a trailing separator.
pub fn exe_dir() -> String {
    let p = exe_path();
    match p.rfind(|c| c == '/' || c == '\\') {
        Some(i) => String::from(&p[..=i]),
        None => String::new(),
    }
}

/// Put the file `from` in place of `to` (which may be the running
/// program: on Windows that one is moved aside first, as `<to>.old`).
/// Returns whether it worked.
pub fn replace_file(from: &str, to: &str) -> bool {
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    unsafe {
        let (f, t) = (cstr(from), cstr(to));
        chmod(f.as_ptr() as *const c_char, 0o755);
        return rename(f.as_ptr() as *const c_char, t.as_ptr() as *const c_char) == 0;
    }
    #[cfg(windows)]
    unsafe {
        let (f, t, old) = (cstr(from), cstr(to), cstr(&alloc::format!("{}.old", to)));
        // 1 = replace an existing file.
        MoveFileExA(t.as_ptr() as *const c_char, old.as_ptr() as *const c_char, 1);
        if MoveFileExA(f.as_ptr() as *const c_char, t.as_ptr() as *const c_char, 1) != 0 {
            return true;
        }
        // The new file did not go in: put the old program back, so that a
        // program is always at the path.
        MoveFileExA(old.as_ptr() as *const c_char, t.as_ptr() as *const c_char, 1);
        return false;
    }
    #[allow(unreachable_code)]
    {
        let _ = (from, to);
        false
    }
}

/// The names in a folder (files and folders alike, without `.` and `..`),
/// sorted. Empty when there is no such folder.
pub fn list_dir(path: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    #[cfg(unix)]
    unsafe {
        // Where the name starts inside a directory entry.
        const NAME: usize = if cfg!(target_os = "linux") { 19 } else if cfg!(target_arch = "aarch64") { 21 } else { 8 };
        let p = cstr(if path.is_empty() { "." } else { path });
        let dir = opendir(p.as_ptr() as *const c_char);
        if !dir.is_null() {
            loop {
                let e = readdir(dir);
                if e.is_null() {
                    break;
                }
                names.push(from_cstr(e.add(NAME) as *const c_char));
            }
            closedir(dir);
        }
    }
    #[cfg(windows)]
    unsafe {
        // WIN32_FIND_DATAA is 320 bytes; the name starts at byte 44.
        let mut data = [0u8; 320];
        let pattern = cstr(&alloc::format!("{}{}*", path, if path.is_empty() || path.ends_with('/') || path.ends_with('\\') { "" } else { "/" }));
        let h = FindFirstFileA(pattern.as_ptr() as *const c_char, data.as_mut_ptr());
        if h as isize != -1 {
            loop {
                names.push(from_cstr(data.as_ptr().add(44) as *const c_char));
                if FindNextFileA(h, data.as_mut_ptr()) == 0 {
                    break;
                }
            }
            FindClose(h);
        }
    }
    // A page has no folders to look in.
    #[cfg(target_arch = "wasm32")]
    let _ = path;
    names.retain(|n| n != "." && n != ".." && !n.is_empty());
    names.sort();
    names
}

/// Read a whole file. `None` when it does not exist or cannot be read.
pub fn read_file(path: &str) -> Option<Vec<u8>> {
    let p = cstr(path);
    let api = sdl::sdl();
    unsafe {
        let rw = (api.SDL_RWFromFile)(p.as_ptr() as *const c_char, b"rb\0".as_ptr() as *const c_char);
        if rw.is_null() {
            return None;
        }
        let size = ((*rw).size)(rw);
        let mut out = Vec::new();
        if size > 0 {
            out.resize(size as usize, 0u8);
            let mut got = 0usize;
            while got < out.len() {
                let n = ((*rw).read)(rw, out.as_mut_ptr().add(got) as *mut c_void, 1, out.len() - got);
                if n == 0 {
                    break;
                }
                got += n;
            }
            out.truncate(got);
        }
        ((*rw).close)(rw);
        Some(out)
    }
}

/// Make a folder, if it is not there already. `true` if it is there now.
pub fn make_dir(path: &str) -> bool {
    let p = cstr(path);
    #[cfg(unix)]
    unsafe {
        // EEXIST is not a failure: the folder is there.
        return mkdir(p.as_ptr() as *const c_char, 0o755) == 0 || read_dir_exists(path);
    }
    #[cfg(windows)]
    unsafe {
        return CreateDirectoryA(p.as_ptr() as *const c_char, core::ptr::null_mut()) != 0 || read_dir_exists(path);
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        false
    }
}

/// Whether a folder is there (by listing it).
#[cfg(not(target_arch = "wasm32"))]
fn read_dir_exists(path: &str) -> bool {
    let p = cstr(path);
    #[cfg(unix)]
    unsafe {
        let d = opendir(p.as_ptr() as *const c_char);
        if d.is_null() {
            return false;
        }
        closedir(d);
        return true;
    }
    #[cfg(windows)]
    {
        let _ = p;
        let _ = path;
        return true;
    }
}

/// Delete a file. `false` if it could not be deleted (or does not exist).
pub fn remove_file(path: &str) -> bool {
    let p = cstr(path);
    #[cfg(unix)]
    unsafe {
        return unlink(p.as_ptr() as *const c_char) == 0;
    }
    #[cfg(windows)]
    unsafe {
        return DeleteFileA(p.as_ptr() as *const c_char) != 0;
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        false
    }
}

pub fn write_file(path: &str, data: &[u8]) -> bool {
    let p = cstr(path);
    let api = sdl::sdl();
    unsafe {
        let rw = (api.SDL_RWFromFile)(p.as_ptr() as *const c_char, b"wb\0".as_ptr() as *const c_char);
        if rw.is_null() {
            return false;
        }
        let mut ok = true;
        if !data.is_empty() {
            let n = ((*rw).write)(rw, data.as_ptr() as *const c_void, 1, data.len());
            ok = n == data.len();
        }
        ((*rw).close)(rw);
        ok
    }
}

/// Seconds since SDL started, from the high-resolution counter.
pub fn now() -> f64 {
    let api = sdl::sdl();
    unsafe {
        let c = (api.SDL_GetPerformanceCounter)();
        let f = (api.SDL_GetPerformanceFrequency)();
        c as f64 / f as f64
    }
}

/// The text on the system clipboard (empty if there is none).
pub fn clipboard() -> String {
    if !sdl::loaded() {
        return String::new();
    }
    unsafe {
        let p = (sdl::sdl().SDL_GetClipboardText)();
        if p.is_null() {
            return String::new();
        }
        let text = from_cstr(p);
        (sdl::sdl().SDL_free)(p as *mut core::ffi::c_void);
        text
    }
}

/// Put `text` on the system clipboard.
pub fn set_clipboard(text: &str) {
    if sdl::loaded() {
        let c = cstr(text);
        unsafe { (sdl::sdl().SDL_SetClipboardText)(c.as_ptr() as *const core::ffi::c_char) };
    }
}
