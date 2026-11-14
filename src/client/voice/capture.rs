//! The microphone, through SDL2's audio capture.
//!
//! SDL calls a function of ours on its own thread with each block of
//! samples; that function only copies them into a lock-free ring
//! ([`super::ring::Ring`]) and the game thread reads them from there. (The
//! simpler `SDL_DequeueAudio` needs SDL 2.0.5, and the old Mac builds have
//! 2.0.4.) A [`Capture`] exists only while the microphone is open: dropping
//! it closes the device. In a browser there is no microphone and
//! [`Capture::open`] returns `None`.

use super::ring::Ring;
use crate::sys::sdl::sdl;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::{c_int, c_void};

/// Signed 16-bit samples in this machine's byte order (see audio.rs).
const FORMAT: u16 = if cfg!(target_endian = "big") { 0x9010 } else { 0x8010 };

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

/// Runs on SDL's capture thread.
unsafe extern "C" fn on_samples(user: *mut c_void, stream: *mut u8, len: c_int) {
    if user.is_null() || stream.is_null() || len <= 0 {
        return;
    }
    let ring = &*(user as *const Ring);
    // SDL's buffer is suitably aligned for the sample type it was asked for.
    let samples = core::slice::from_raw_parts(stream as *const i16, len as usize / 2);
    ring.push(samples);
}

/// The open microphone.
pub struct Capture {
    device: u32,
    ring: Arc<Ring>,
}

/// The names of the machine's microphones (empty if none, or in a browser).
pub fn devices() -> Vec<String> {
    if !crate::sys::sdl::loaded() {
        return Vec::new();
    }
    let api = sdl();
    let mut names = Vec::new();
    unsafe {
        if (api.SDL_InitSubSystem)(0x10) != 0 {
            return names;
        }
        let n = (api.SDL_GetNumAudioDevices)(1).clamp(0, 32);
        for i in 0..n {
            let name = crate::sys::from_cstr((api.SDL_GetAudioDeviceName)(i, 1));
            if !name.is_empty() {
                names.push(name);
            }
        }
    }
    names
}

impl Capture {
    /// Open the microphone called `name` (empty: the system's default one)
    /// at 16 kHz, mono. `None` if there is none or it will not open.
    pub fn open(name: &str) -> Option<Capture> {
        if !crate::sys::sdl::loaded() {
            return None;
        }
        let api = sdl();
        // A second of slack: the game thread reads every frame.
        let ring = Arc::new(Ring::new(super::codec::RATE as usize));
        let want = AudioSpec {
            freq: super::codec::RATE as c_int,
            format: FORMAT,
            channels: 1,
            silence: 0,
            samples: 512,
            padding: 0,
            size: 0,
            callback: Some(on_samples),
            userdata: Arc::as_ptr(&ring) as *mut c_void,
        };
        let cname = crate::sys::cstr(name);
        unsafe {
            if (api.SDL_InitSubSystem)(0x10) != 0 {
                return None;
            }
            let mut got: AudioSpec = core::mem::zeroed();
            let device = (api.SDL_OpenAudioDevice)(
                if name.is_empty() { core::ptr::null() } else { cname.as_ptr() as *const _ },
                1,
                &want as *const AudioSpec as *const c_void,
                &mut got as *mut AudioSpec as *mut c_void,
                0,
            );
            if device == 0 {
                return None;
            }
            // Asked for no changes, so SDL converts; but never trust what we cannot check.
            if got.channels != 1 || got.format != FORMAT {
                (api.SDL_CloseAudioDevice)(device);
                return None;
            }
            (api.SDL_PauseAudioDevice)(device, 0);
            Some(Capture { device, ring })
        }
    }

    /// Take what has been recorded since last time, oldest first.
    pub fn read(&self, out: &mut [i16]) -> usize {
        self.ring.pop(out)
    }

    /// Samples waiting.
    pub fn waiting(&self) -> usize {
        self.ring.len()
    }

    /// Throw away what is waiting.
    pub fn flush(&self) {
        self.ring.clear();
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // Closing the device waits for the callback to finish, after which
        // nothing touches the ring but us.
        unsafe { (sdl().SDL_CloseAudioDevice)(self.device) };
    }
}
