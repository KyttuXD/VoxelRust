//! Sound. There are no sound files: every effect is synthesised from noise
//! and simple waves, mixed in SDL's audio callback.
//!
//! The mixer state is a fixed table of voices shared with the audio thread;
//! the main thread only adds voices while holding SDL's audio lock.

use crate::sys::sdl::sdl;
use alloc::vec::Vec;
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
    /// A recorded sound instead of a made one: its samples (null for a made
    /// one), how many there are, where in them we are, and how fast we go.
    clip: *const i16,
    clip_len: u32,
    pos: f32,
    step: f32,
}

const VOICES: usize = 12;
const RATE: i32 = 22050;

struct Mixer {
    voices: [Voice; VOICES],
}

const SILENT: Voice =
    Voice { left: 0, total: 1, gain: 0.0, noise: 0.0, step0: 0.0, step1: 0.0, phase: 0.0, lp: 0.0, lp_k: 1.0, rng: 1, clip: core::ptr::null(), clip_len: 0, pos: 0.0, step: 1.0 };

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
            if !v.clip.is_null() {
                // A recorded sound: read it at its own speed, with a short
                // fade at the end so it does not click.
                let i = v.pos as usize;
                if i + 1 >= v.clip_len as usize {
                    v.left = 0;
                    break;
                }
                // SAFETY: `clip` points into samples that are never freed (the
                // registry and the sound folder's clips are kept for good).
                let (a, b) = unsafe { (*v.clip.add(i) as f32, *v.clip.add(i + 1) as f32) };
                let x = a + (b - a) * (v.pos - i as f32);
                let fade = ((v.clip_len as f32 - v.pos) / 200.0).min(1.0);
                let mixed = *s as i32 + (x * fade * v.gain) as i32;
                *s = mixed.clamp(-32000, 32000) as i16;
                v.pos += v.step;
                v.left -= 1;
                continue;
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
    /// What was heard lately, for subtitles: text and seconds left to show.
    pub captions: Vec<(alloc::string::String, f32)>,
    /// Sounds from the data folder's `sounds/` folder (`<id>.wav`): they
    /// replace the sound of that name, made or recorded.
    user: Vec<(alloc::string::String, &'static [i16])>,
}

impl Audio {
    /// Open the default sound device. If there is none the game simply
    /// stays silent.
    pub fn new() -> Audio {
        let mixer = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(Mixer { voices: [SILENT; VOICES] }));
        let mut audio = Audio { device: 0, mixer, seed: 0x1234_5678, effects: 0.5, steps: 0.4, ui: 0.4, captions: Vec::new(), user: Vec::new() };
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
        Audio { device: 0, mixer, seed: 1, effects: 0.0, steps: 0.0, ui: 0.0, captions: Vec::new(), user: Vec::new() }
    }

    /// Read the sounds the player has put in `<base>sounds/` (`<id>.wav`, `.flac`, `.aiff`, `.ogg`).
    /// Each replaces the sound of that name. Returns the names found.
    pub fn load_folder(&mut self, base: &str) -> Vec<alloc::string::String> {
        let dir = alloc::format!("{}sounds/", base);
        let mut names = Vec::new();
        for f in crate::sys::list_dir(&dir) {
            let lower = f.to_ascii_lowercase();
            let Some(stem) = [".wav", ".flac", ".aiff", ".aif", ".ogg"].iter().find_map(|e| lower.strip_suffix(e)) else { continue };
            let id = &f[..stem.len()];
            if let Some(samples) = crate::sys::read_file(&alloc::format!("{}{}", dir, f)).and_then(|b| crate::wav::decode(&b)) {
                let leaked: &'static [i16] = alloc::boxed::Box::leak(samples.into_boxed_slice());
                self.user.retain(|u| u.0 != id);
                self.user.push((alloc::string::String::from(id), leaked));
                names.push(alloc::string::String::from(id));
            }
        }
        names
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

    /// The name a built-in sound goes by in the data files.
    fn name(sound: Sound) -> &'static str {
        match sound {
            Sound::Step => "step",
            Sound::Break => "break",
            Sound::Place => "place",
            Sound::Hurt => "hurt",
            Sound::Click => "click",
            Sound::Pop => "pop",
            Sound::Splash => "splash",
            Sound::Thunder => "thunder",
            Sound::Rain => "rain",
            Sound::Rumble => "rumble",
            Sound::Wave => "wave",
        }
    }

    /// Stop everything at once (leaving a world must not leave its rain
    /// playing over the menu).
    pub fn silence(&mut self) {
        self.captions.clear();
        if self.device == 0 {
            return;
        }
        let api = sdl();
        unsafe {
            (api.SDL_LockAudioDevice)(self.device);
            (*self.mixer).voices = [SILENT; VOICES];
            (api.SDL_UnlockAudioDevice)(self.device);
        }
    }

    /// Count the captions down; call once a frame.
    pub fn tick(&mut self, dt: f32) {
        for c in self.captions.iter_mut() {
            c.1 -= dt;
        }
        self.captions.retain(|c| c.1 > 0.0);
    }

    /// Play a sound. `volume` scales it further (distance, for example).
    pub fn play(&mut self, sound: Sound, volume: f32) {
        self.play_named(Self::name(sound), volume);
    }

    /// Play a sound by the name the data files give it. Unknown names are
    /// silently nothing: a pack may name sounds another pack provides.
    pub fn play_named(&mut self, name: &str, volume: f32) {
        let own: Option<&'static [i16]> = self.user.iter().find(|u| u.0 == name).map(|u| u.1);
        let def = crate::block::reg().sounds.iter().find(|s| s.id == name);
        if def.is_none() && own.is_none() {
            return;
        }
        // Subtitles are kept whether or not there is a sound device.
        if let Some(d) = def.filter(|d| !d.caption.is_empty() && volume > 0.05) {
            match self.captions.iter_mut().find(|c| c.0 == d.caption) {
                Some(c) => c.1 = 2.5,
                None => {
                    if self.captions.len() >= 5 {
                        self.captions.remove(0);
                    }
                    self.captions.push((d.caption.clone(), 2.5));
                }
            }
        }
        if self.device == 0 {
            return;
        }
        self.seed = self.seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let jitter = 0.9 + ((self.seed >> 16) & 255) as f32 / 1275.0; // 0.9..1.1
        let bus = match def.map_or(0, |d| d.bus) {
            1 => Bus::Steps,
            2 => Bus::Ui,
            _ => Bus::Effects,
        };
        let (secs, noise, f0, f1, lp, loud) = match def {
            Some(d) => (d.seconds, d.noise, d.hz[0], d.hz[1], d.low_pass, d.loudness),
            None => (0.1, 0.0, 0.0, 0.0, 1.0, 1.0),
        };
        let gain = loud * volume * match bus {
            Bus::Effects => self.effects,
            Bus::Steps => self.steps,
            Bus::Ui => self.ui,
        };
        if gain <= 0.001 {
            return;
        }
        // A recorded sound: the folder's own, else the data's.
        let clip: Option<&'static [i16]> = own.or_else(|| def.and_then(|d| d.clip.as_deref()));
        let mut voice = Voice {
            left: (secs * RATE as f32) as u32,
            total: (secs * RATE as f32).max(1.0) as u32,
            gain,
            noise,
            step0: f0 * jitter / RATE as f32,
            step1: f1 * jitter / RATE as f32,
            phase: 0.0,
            lp: 0.0,
            lp_k: lp,
            rng: self.seed | 1,
            clip: core::ptr::null(),
            clip_len: 0,
            pos: 0.0,
            step: 1.0,
        };
        if let Some(c) = clip {
            // Pitch varies a little, as made sounds do (less: a recording is a voice).
            let pitch = 0.97 + (jitter - 0.9) * 0.3;
            voice.clip = c.as_ptr();
            voice.clip_len = c.len() as u32;
            voice.step = pitch;
            voice.left = (c.len() as f32 / pitch) as u32 + 2;
            voice.total = voice.left;
            // The recording's own level: 14000 is what made sounds peak at.
            voice.gain = gain * 0.45;
        }
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
        m.voices[0] = Voice { left: 1000, total: 1000, gain: 0.8, noise: 0.5, step0: 0.02, step1: 0.01, phase: 0.0, lp: 0.0, lp_k: 0.3, rng: 7, clip: core::ptr::null(), clip_len: 0, pos: 0.0, step: 1.0 };
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
            *v = Voice { left: 400, total: 400, gain: 5.0, noise: 1.0, step0: 0.0, step1: 0.0, phase: 0.0, lp: 0.0, lp_k: 1.0, rng: 99 + i as u32, clip: core::ptr::null(), clip_len: 0, pos: 0.0, step: 1.0 };
        }
        let mut buf = [0i16; 400];
        mix(&mut m, &mut buf);
        assert!(buf.iter().all(|&s| (-32000..=32000).contains(&s)));
    }
}
