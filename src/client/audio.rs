//! Sound. There are no sound files: every effect is synthesised from noise
//! and simple waves, mixed in SDL's audio callback.
//!
//! The mixer state is a fixed table of voices shared with the audio thread;
//! the main thread only adds voices while holding SDL's audio lock.

use crate::sys::sdl::sdl;
use core::ffi::{c_int, c_void};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sound {
    Step,
    Break,
    Place,
    Hurt,
    Click,
    Pop,
    Splash,
    Thunder,
    Rain,
    Rumble,
    /// A wave washing in: soft, low and slow.
    Wave,
}

/// Which volume slider a sound follows.
#[derive(Clone, Copy)]
pub enum Bus {
    Effects,
    Steps,
    Ui,
}

#[derive(Clone, Copy)]
struct Voice {
    /// Samples left to play; 0 = free.
    left: u32,
    total: u32,
    /// 0..1 loudness after all volume sliders.
    gain: f32,
    /// Share of noise versus tone, 0..1.
    noise: f32,
    /// Tone phase step per sample at the start and at the end (a sweep).
    step0: f32,
    step1: f32,
    phase: f32,
    /// One-pole low-pass state and coefficient for the noise.
    lp: f32,
    lp_k: f32,
    rng: u32,
}

const VOICES: usize = 12;
const RATE: i32 = 22050;

struct Mixer {
    voices: [Voice; VOICES],
}

const SILENT: Voice =
    Voice { left: 0, total: 1, gain: 0.0, noise: 0.0, step0: 0.0, step1: 0.0, phase: 0.0, lp: 0.0, lp_k: 1.0, rng: 1 };

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

/// Fill `out` with the mix of all active voices. Pure, so it can be tested.
fn mix(m: &mut Mixer, out: &mut [i16]) {
    for s in out.iter_mut() {
        *s = 0;
    }
    for v in m.voices.iter_mut() {
        if v.left == 0 {
            continue;
        }
        for s in out.iter_mut() {
            if v.left == 0 {
                break;
            }
            let t = 1.0 - v.left as f32 / v.total as f32; // 0 at start, 1 at end
            // Quick attack, smooth decay.
            let env = if t < 0.03 { t / 0.03 } else { (1.0 - t) * (1.0 - t) };
            v.rng ^= v.rng << 13;
            v.rng ^= v.rng >> 17;
            v.rng ^= v.rng << 5;
            let white = (v.rng >> 8) as f32 / 8_388_608.0 - 1.0;
            v.lp += (white - v.lp) * v.lp_k;
            v.phase += v.step0 + (v.step1 - v.step0) * t;
            if v.phase >= 1.0 {
                v.phase -= 1.0;
            }
            // A triangle wave: soft, and needs no sine.
            let tri = if v.phase < 0.5 { v.phase * 4.0 - 1.0 } else { 3.0 - v.phase * 4.0 };
            let sample = (v.lp * v.noise + tri * (1.0 - v.noise)) * env * v.gain;
            let mixed = *s as i32 + (sample * 14000.0) as i32;
            *s = mixed.clamp(-32000, 32000) as i16;
            v.left -= 1;
        }
    }
}

unsafe extern "C" fn callback(user: *mut c_void, stream: *mut u8, len: c_int) {
    let m = &mut *(user as *mut Mixer);
    let out = core::slice::from_raw_parts_mut(stream as *mut i16, (len.max(0) as usize) / 2);
    mix(m, out);
}

pub struct Audio {
    device: u32,
    mixer: *mut Mixer,
    seed: u32,
    /// 0..1 per bus, already multiplied by the master volume.
    pub effects: f32,
    pub steps: f32,
    pub ui: f32,
}

impl Audio {
    /// Open the default sound device. If there is none the game simply
    /// stays silent.
    pub fn new() -> Audio {
        let mixer = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(Mixer { voices: [SILENT; VOICES] }));
        let mut audio = Audio { device: 0, mixer, seed: 0x1234_5678, effects: 0.5, steps: 0.4, ui: 0.4 };
        let api = sdl();
        unsafe {
            if (api.SDL_InitSubSystem)(0x10) != 0 {
                return audio;
            }
            let want = AudioSpec {
                freq: RATE,
                format: if cfg!(target_endian = "big") { 0x9010 } else { 0x8010 }, // signed 16-bit, this machine's byte order
                channels: 1,
                silence: 0,
                samples: 512,
                padding: 0,
                size: 0,
                callback: Some(callback),
                userdata: mixer as *mut c_void,
            };
            let mut got: AudioSpec = core::mem::zeroed();
            audio.device = (api.SDL_OpenAudioDevice)(
                core::ptr::null(),
                0,
                &want as *const AudioSpec as *const c_void,
                &mut got as *mut AudioSpec as *mut c_void,
                0,
            );
            if audio.device != 0 {
                (api.SDL_PauseAudioDevice)(audio.device, 0);
            }
        }
        audio
    }

    /// An audio system that plays nothing (for scripted runs).
    pub fn silent() -> Audio {
        let mixer = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(Mixer { voices: [SILENT; VOICES] }));
        Audio { device: 0, mixer, seed: 1, effects: 0.0, steps: 0.0, ui: 0.0 }
    }

    pub fn available(&self) -> bool {
        self.device != 0
    }

    /// Set the bus volumes from the 0..=10 settings sliders.
    pub fn set_volumes(&mut self, master: i32, effects: i32, steps: i32, ui: i32) {
        let m = master as f32 / 10.0;
        self.effects = m * effects as f32 / 10.0;
        self.steps = m * steps as f32 / 10.0;
        self.ui = m * ui as f32 / 10.0;
    }

    /// Play a sound. `volume` scales it further (distance, for example).
    pub fn play(&mut self, sound: Sound, volume: f32) {
        if self.device == 0 {
            return;
        }
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let jitter = 0.9 + ((self.seed >> 16) & 255) as f32 / 1275.0; // 0.9..1.1
        // (bus, seconds, noise share, tone Hz start, tone Hz end, low-pass, loudness)
        let (bus, secs, noise, f0, f1, lp, loud) = match sound {
            Sound::Step => (Bus::Steps, 0.09, 0.95, 90.0, 60.0, 0.10, 0.55),
            Sound::Break => (Bus::Effects, 0.22, 0.90, 160.0, 70.0, 0.30, 0.80),
            Sound::Place => (Bus::Effects, 0.10, 0.70, 220.0, 140.0, 0.22, 0.70),
            Sound::Hurt => (Bus::Effects, 0.25, 0.25, 420.0, 180.0, 0.50, 0.80),
            Sound::Click => (Bus::Ui, 0.05, 0.20, 900.0, 700.0, 0.80, 0.45),
            Sound::Pop => (Bus::Effects, 0.08, 0.10, 500.0, 900.0, 0.80, 0.50),
            Sound::Splash => (Bus::Effects, 0.35, 1.00, 100.0, 100.0, 0.45, 0.60),
            Sound::Thunder => (Bus::Effects, 1.80, 1.00, 60.0, 40.0, 0.04, 1.00),
            Sound::Rain => (Bus::Effects, 0.45, 1.00, 100.0, 100.0, 0.60, 0.22),
            Sound::Rumble => (Bus::Effects, 0.60, 0.90, 46.0, 34.0, 0.03, 0.90),
            Sound::Wave => (Bus::Effects, 1.60, 1.00, 80.0, 60.0, 0.07, 0.30),
        };
        let gain = loud * volume * match bus {
            Bus::Effects => self.effects,
            Bus::Steps => self.steps,
            Bus::Ui => self.ui,
        };
        if gain <= 0.001 {
            return;
        }
        let total = (secs * RATE as f32) as u32;
        let voice = Voice {
            left: total,
            total,
            gain,
            noise,
            step0: f0 * jitter / RATE as f32,
            step1: f1 * jitter / RATE as f32,
            phase: 0.0,
            lp: 0.0,
            lp_k: lp,
            rng: self.seed | 1,
        };
        let api = sdl();
        unsafe {
            (api.SDL_LockAudioDevice)(self.device);
            let m = &mut *self.mixer;
            // Use a free voice, or steal the one closest to finishing.
            let slot = match m.voices.iter().position(|v| v.left == 0) {
                Some(i) => i,
                None => (0..VOICES).min_by_key(|&i| m.voices[i].left).unwrap_or(0),
            };
            m.voices[slot] = voice;
            (api.SDL_UnlockAudioDevice)(self.device);
        }
    }
}

impl Default for Audio {
    fn default() -> Self {
        Audio::new()
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        unsafe {
            if self.device != 0 {
                (sdl().SDL_CloseAudioDevice)(self.device);
            }
            // The device is closed, so the callback can no longer run.
            drop(alloc::boxed::Box::from_raw(self.mixer));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_voice_makes_sound_then_falls_silent() {
        let mut m = Mixer { voices: [SILENT; VOICES] };
        m.voices[0] = Voice { left: 1000, total: 1000, gain: 0.8, noise: 0.5, step0: 0.02, step1: 0.01, phase: 0.0, lp: 0.0, lp_k: 0.3, rng: 7 };
        let mut buf = [0i16; 1500];
        mix(&mut m, &mut buf);
        assert!(buf[..1000].iter().any(|&s| s.unsigned_abs() > 500), "no signal");
        assert!(buf[1000..].iter().all(|&s| s == 0), "sound did not stop");
        assert_eq!(m.voices[0].left, 0);
        // Starts and ends quietly (no click).
        assert!(buf[0].unsigned_abs() < 400 && buf[999].unsigned_abs() < 400);
    }

    #[test]
    fn many_loud_voices_do_not_overflow() {
        let mut m = Mixer { voices: [SILENT; VOICES] };
        for (i, v) in m.voices.iter_mut().enumerate() {
            *v = Voice { left: 400, total: 400, gain: 5.0, noise: 1.0, step0: 0.0, step1: 0.0, phase: 0.0, lp: 0.0, lp_k: 1.0, rng: 99 + i as u32 };
        }
        let mut buf = [0i16; 400];
        mix(&mut m, &mut buf);
        assert!(buf.iter().all(|&s| (-32000..=32000).contains(&s)));
    }
}
