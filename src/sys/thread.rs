//! Threads, mutexes and semaphores on top of SDL2.

use super::sdl::{self, sdl};
use alloc::boxed::Box;
use core::cell::UnsafeCell;
use core::ffi::{c_char, c_int, c_void};
use core::ops::{Deref, DerefMut};

pub struct Mutex<T> {
    raw: *mut c_void,
    value: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for Mutex<T> {}
unsafe impl<T: Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    pub fn new(value: T) -> Mutex<T> {
        let raw = unsafe { (sdl().SDL_CreateMutex)() };
        assert!(!raw.is_null(), "SDL_CreateMutex failed");
        Mutex { raw, value: UnsafeCell::new(value) }
    }

    pub fn lock(&self) -> Guard<'_, T> {
        unsafe { (sdl().SDL_LockMutex)(self.raw) };
        Guard { m: self }
    }
}

impl<T> Drop for Mutex<T> {
    fn drop(&mut self) {
        unsafe { (sdl().SDL_DestroyMutex)(self.raw) };
    }
}

pub struct Guard<'a, T> {
    m: &'a Mutex<T>,
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.m.value.get() }
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.m.value.get() }
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        unsafe { (sdl().SDL_UnlockMutex)(self.m.raw) };
    }
}

pub struct Sem(*mut c_void);

unsafe impl Send for Sem {}
unsafe impl Sync for Sem {}

impl Sem {
    pub fn new() -> Sem {
        let raw = unsafe { (sdl().SDL_CreateSemaphore)(0) };
        assert!(!raw.is_null(), "SDL_CreateSemaphore failed");
        Sem(raw)
    }
    pub fn wait(&self) {
        unsafe { (sdl().SDL_SemWait)(self.0) };
    }
    pub fn post(&self) {
        unsafe { (sdl().SDL_SemPost)(self.0) };
    }
}

impl Default for Sem {
    fn default() -> Self {
        Sem::new()
    }
}

impl Drop for Sem {
    fn drop(&mut self) {
        unsafe { (sdl().SDL_DestroySemaphore)(self.0) };
    }
}

pub struct Thread(*mut c_void);

type Task = Box<dyn FnOnce() + Send + 'static>;

unsafe extern "C" fn trampoline(p: *mut c_void) -> c_int {
    let task: Box<Task> = Box::from_raw(p as *mut Task);
    task();
    0
}

/// Start a thread. Returns `None` if the system refuses to create one.
pub fn spawn(f: Task) -> Option<Thread> {
    let data = Box::into_raw(Box::new(f)) as *mut c_void;
    let t = unsafe { sdl::create_thread(trampoline, b"voxel-worker\0".as_ptr() as *const c_char, data) };
    if t.is_null() {
        // Reclaim the closure so it is not leaked.
        unsafe { drop(Box::from_raw(data as *mut Task)) };
        None
    } else {
        Some(Thread(t))
    }
}

impl Thread {
    pub fn join(self) {
        unsafe { (sdl().SDL_WaitThread)(self.0, core::ptr::null_mut()) };
    }
}

pub fn cpu_count() -> usize {
    let n = unsafe { (sdl().SDL_GetCPUCount)() };
    if n < 1 {
        1
    } else {
        n as usize
    }
}
