//! Sound. There are no sound files: every effect is synthesised from noise
//! and simple waves, mixed in SDL's audio callback.
//!
//! The mixer state is a fixed table of voices (and a few continuous beds,
//! see `util/audio/bed.rs`) shared with the audio thread; the main thread
//! only adds voices while holding SDL's audio lock. How sounds are varied,
//! limited and made ambient is in docs/SOUND.md; the ambient part of this
//! type is in `audio/ambient.rs`.

mod ambient;

use super::voice::stream::VoiceStream;
use crate::sys::sdl::sdl;
use crate::wav::ambience::{Ambience, Env, Timed};
use crate::wav::bed::{Bed, BedKind, BEDS};
use crate::wav::limiter::soft_limit;
use crate::wav::rand::Rng;
use crate::wav::variation::{self, NoRepeat, RateLimiter};
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

const VOICES: usize = 16;
const RATE: i32 = 22050;

/// Other players' voices playing at once (proximity voice chat, see `voice`).
const VOICE_STREAMS: usize = 6;

struct Mixer {
    voices: [Voice; VOICES],
    /// Rain, wind, water and the other continuous sounds.
    beds: [Bed; BEDS],
    /// Streamed speech: fed by the game thread, played from here.
    streams: [VoiceStream; VOICE_STREAMS],
}

impl Mixer {
    fn new() -> Mixer {
        Mixer { voices: [SILENT; VOICES], beds: Self::fresh_beds(), streams: core::array::from_fn(|_| VoiceStream::new()) }
    }

    /// Silent beds, each with its own grain pattern.
    fn fresh_beds() -> [Bed; BEDS] {
        core::array::from_fn(|i| Bed::new(BedKind::ALL[i], 0x2545_F491u32.wrapping_add((i as u32).wrapping_mul(0x9E37_79B1))))
    }
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

/// Add what one voice plays next to `acc` (sums of the voices, in 16-bit
/// units but wider than 16 bits so many loud voices do not wrap).
fn mix_voice(v: &mut Voice, acc: &mut [i32]) {
    if v.left == 0 {
        return;
    }
    for s in acc.iter_mut() {
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
            // SAFETY: `clip` points into samples that are never freed while the
            // voice plays (see `Audio::playing_clips` and `Clip::evict`).
            let (a, b) = unsafe { (*v.clip.add(i) as f32, *v.clip.add(i + 1) as f32) };
            let x = a + (b - a) * (v.pos - i as f32);
            // Distant sounds are duller (lp_k below 1); near ones pass as they are.
            v.lp += (x - v.lp) * v.lp_k;
            let fade = ((v.clip_len as f32 - v.pos) / 200.0).min(1.0);
            *s += (v.lp * fade * v.gain) as i32;
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
        *s += (sample * 14000.0) as i32;
        v.left -= 1;
    }
}

/// Fill `out` with the mix of all active voices and beds, bent softly
/// below full scale (see `util/audio/limiter.rs`). Pure, so it can be
/// tested. Allocates nothing: it runs in the audio callback.
fn mix(m: &mut Mixer, out: &mut [i16]) {
    const CHUNK: usize = 256;
    for chunk in out.chunks_mut(CHUNK) {
        let mut sums = [0i32; CHUNK];
        let acc = &mut sums[..chunk.len()];
        for v in m.voices.iter_mut() {
            mix_voice(v, acc);
        }
        for b in m.beds.iter_mut() {
            if b.quiet() {
                continue;
            }
            for a in acc.iter_mut() {
                *a += (b.next() * 32767.0) as i32;
            }
        }
        for (o, a) in chunk.iter_mut().zip(acc.iter()) {
            *o = soft_limit(*a);
        }
    }
    for st in m.streams.iter_mut() {
        st.mix_into(out);
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
    rng: Rng,
    /// 0..1 per bus, already multiplied by the master volume.
    pub effects: f32,
    pub steps: f32,
    pub ui: f32,
    /// The master volume alone, and the AMBIENCE volume with it (rain,
    /// wind, water, fire, lava and cave sounds).
    master: f32,
    ambience: f32,
    /// What is around the player, set by the game every few tenths of a
    /// second (see `game/ambience_probe.rs`), and how long ago.
    env: Env,
    env_age: f32,
    amb: Ambience,
    /// Seconds since the sound system started (counts `tick`s).
    clock: f32,
    limiter: RateLimiter,
    /// Which take of each sound played last, so the next one differs.
    choosers: Vec<(u32, NoRepeat)>,
    /// Single sounds waiting for their moment (thunder's delay, an echo).
    pending: Vec<(f32, Timed)>,
    scratch: Vec<Timed>,
    caption_timer: f32,
    /// What was heard lately, for subtitles: text and seconds left to show.
    pub captions: Vec<(alloc::string::String, f32)>,
    /// Sounds from the data folder's `sounds/` folder (`<id>.wav`): they
    /// replace the sound of that name, made or recorded.
    user: Vec<(alloc::string::String, crate::wav::clip::Clip)>,
}

impl Audio {
    /// Open the default sound device. If there is none the game simply
    /// stays silent.
    pub fn new() -> Audio {
        let mixer = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(Mixer::new()));
        let mut audio = Self::build(mixer, 0x1234_5678, 0.5, 0.4, 0.4);
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
        let mixer = alloc::boxed::Box::into_raw(alloc::boxed::Box::new(Mixer::new()));
        Self::build(mixer, 1, 0.0, 0.0, 0.0)
    }

    fn build(mixer: *mut Mixer, seed: u32, effects: f32, steps: f32, ui: f32) -> Audio {
        Audio {
            device: 0,
            mixer,
            rng: Rng::new(seed),
            effects,
            steps,
            ui,
            master: effects,
            ambience: effects * 0.6,
            env: Env::default(),
            env_age: 0.0,
            amb: Ambience::new(seed ^ 0x5bd1_e995),
            clock: 0.0,
            limiter: RateLimiter::new(),
            choosers: Vec::new(),
            pending: Vec::new(),
            scratch: Vec::new(),
            caption_timer: 0.0,
            captions: Vec::new(),
            user: Vec::new(),
        }
    }

    /// Read the sounds the player has put in `<base>sounds/` (`<id>.wav`, `.flac`, `.aiff`, `.ogg`, `.mp3`).
    /// Each replaces the sound of that name. Returns the names found.
    pub fn load_folder(&mut self, base: &str) -> Vec<alloc::string::String> {
        let dir = alloc::format!("{}sounds/", base);
        let mut names = Vec::new();
        for f in crate::sys::list_dir(&dir) {
            let lower = f.to_ascii_lowercase();
            let Some(stem) = [".wav", ".flac", ".aiff", ".aif", ".ogg", ".mp3"].iter().find_map(|e| lower.strip_suffix(e)) else { continue };
            let id = &f[..stem.len()];
            let path = alloc::format!("{}{}", dir, f);
            if let Some(samples) = crate::sys::read_file_limit(&path, crate::wav::MAX_BYTES).and_then(|b| crate::wav::decode(&b)) {
                // (A long one is decoded again when it plays, not kept.)
                let clip = crate::wav::clip::Clip::new(&path, samples);
                self.stop_clips();
                self.user.retain(|u| u.0 != id);
                self.user.push((alloc::string::String::from(id), clip));
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
        self.master = m;
        self.effects = m * effects as f32 / 10.0;
        self.steps = m * steps as f32 / 10.0;
        self.ui = m * ui as f32 / 10.0;
    }

    /// Set the AMBIENCE volume (0..=10, with the master volume set by
    /// `set_volumes`): how loud rain, wind, water, fire, lava and cave
    /// sounds are.
    pub fn set_ambience_volume(&mut self, ambience: i32) {
        self.ambience = self.master * ambience.clamp(0, 10) as f32 / 10.0;
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

    /// The clips the mixer is reading right now (they must not be freed).
    fn playing_clips(&mut self) -> Vec<*const i16> {
        if self.device == 0 {
            return Vec::new();
        }
        let api = sdl();
        unsafe {
            (api.SDL_LockAudioDevice)(self.device);
            let v: Vec<*const i16> = (*self.mixer).voices.iter().filter(|v| v.left > 0 && !v.clip.is_null()).map(|v| v.clip).collect();
            (api.SDL_UnlockAudioDevice)(self.device);
            v
        }
    }

    /// Silence the voices that play a recording (before its samples change hands).
    fn stop_clips(&mut self) {
        if self.device == 0 {
            return;
        }
        let api = sdl();
        unsafe {
            (api.SDL_LockAudioDevice)(self.device);
            for v in (*self.mixer).voices.iter_mut() {
                if !v.clip.is_null() {
                    *v = SILENT;
                }
            }
            (api.SDL_UnlockAudioDevice)(self.device);
        }
    }

    /// Free the decoded samples of long recordings not played for `idle`
    /// seconds (0 = all that are not playing now). They are decoded again
    /// when next played. Returns the bytes freed.
    pub fn trim_clips(&mut self, idle: u32) -> usize {
        let playing = self.playing_clips();
        let clock = crate::sys::now() as u32;
        let mut freed = 0;
        for s in crate::block::reg().sounds.iter() {
            if let Some(c) = &s.clip {
                freed += c.evict(clock, idle, &playing);
            }
        }
        for u in &self.user {
            freed += u.1.evict(clock, idle, &playing);
        }
        freed
    }

    /// Bytes of decoded recordings held now.
    pub fn clip_bytes(&self) -> usize {
        let a: usize = crate::block::reg().sounds.iter().filter_map(|s| s.clip.as_ref()).map(|c| c.resident_bytes()).sum();
        a + self.user.iter().map(|u| u.1.resident_bytes()).sum::<usize>()
    }

    /// Stop everything at once (leaving a world must not leave its rain
    /// playing over the menu).
    pub fn silence(&mut self) {
        self.captions.clear();
        self.pending.clear();
        self.env = Env::default();
        self.amb.reset();
        if self.device == 0 {
            return;
        }
        let api = sdl();
        unsafe {
            (api.SDL_LockAudioDevice)(self.device);
            (*self.mixer).voices = [SILENT; VOICES];
            (*self.mixer).beds = Mixer::fresh_beds();
            for st in (*self.mixer).streams.iter_mut() {
                st.release();
            }
            (api.SDL_UnlockAudioDevice)(self.device);
        }
    }

    /// Count the captions down; call once a frame.
    pub fn tick(&mut self, dt: f32) {
        for c in self.captions.iter_mut() {
            c.1 -= dt;
        }
        self.captions.retain(|c| c.1 > 0.0);
        self.clock += dt.max(0.0);
        self.ambient_tick(dt);
    }

    /// Add decoded speech of player `id` (mono, at [`crate::wav::RATE`]) to
    /// that player's stream in the mixer. `gain` is the final loudness (0..1);
    /// `pan` is -1 (left) to 1 (right). Does nothing with no sound device.
    pub fn voice_push(&mut self, id: u16, samples: &[i16], gain: f32, pan: f32) {
        if self.device == 0 || id == 0 || samples.is_empty() {
            return;
        }
        let api = sdl();
        unsafe {
            (api.SDL_LockAudioDevice)(self.device);
            let m = &mut *self.mixer;
            let slot = match m.streams.iter().position(|v| v.id == id) {
                Some(i) => Some(i),
                None => m.streams.iter().position(|v| v.free()).or_else(|| {
                    // All busy: take over the one with least left to play.
                    (0..VOICE_STREAMS).min_by_key(|&i| m.streams[i].buffered())
                }),
            };
            if let Some(i) = slot {
                if m.streams[i].id != id {
                    m.streams[i].claim(id);
                }
                m.streams[i].push(samples, gain, pan);
            }
            (api.SDL_UnlockAudioDevice)(self.device);
        }
    }

    /// Play a sound. `volume` scales it further (distance, for example).
    pub fn play(&mut self, sound: Sound, volume: f32) {
        self.play_named(Self::name(sound), volume);
    }

    /// Show a subtitle for a couple of seconds (whether or not there is a sound device).
    fn caption(&mut self, text: &str) {
        match self.captions.iter_mut().find(|c| c.0 == text) {
            Some(c) => c.1 = 2.5,
            None => {
                if self.captions.len() >= 5 {
                    self.captions.remove(0);
                }
                self.captions.push((alloc::string::String::from(text), 2.5));
            }
        }
    }

    /// Which of `n` takes of the sound `key` to play: never the one played last.
    fn pick_variant(&mut self, key: u32, n: usize) -> usize {
        let at = match self.choosers.iter().position(|c| c.0 == key) {
            Some(i) => i,
            None => {
                if self.choosers.len() >= 64 {
                    self.choosers.remove(0);
                }
                self.choosers.push((key, NoRepeat::new()));
                self.choosers.len() - 1
            }
        };
        self.choosers[at].1.pick(n, &mut self.rng)
    }

    /// Put a voice in the mixer: in a free slot, else (when `steal`) in the
    /// one closest to finishing, else not at all.
    fn start_voice(&mut self, voice: Voice, steal: bool) {
        let api = sdl();
        unsafe {
            (api.SDL_LockAudioDevice)(self.device);
            let m = &mut *self.mixer;
            let slot = match m.voices.iter().position(|v| v.left == 0) {
                Some(i) => Some(i),
                None if steal => (0..VOICES).min_by_key(|&i| m.voices[i].left),
                None => None,
            };
            if let Some(i) = slot {
                m.voices[i] = voice;
            }
            (api.SDL_UnlockAudioDevice)(self.device);
        }
    }

    /// Play a sound by the name the data files give it. Unknown names are
    /// silently nothing: a pack may name sounds another pack provides.
    ///
    /// Every play is varied: the take differs from the last one (a made
    /// sound has four, a pack's may have more: `name_v2.wav`...), the pitch
    /// moves up to 6 percent and the volume 1.5 dB either way, and a sound
    /// that is asked for too fast or too often at once is dropped (see
    /// docs/SOUND.md).
    pub fn play_named(&mut self, name: &str, volume: f32) {
        let own: Vec<crate::wav::clip::Clip> = self.user.iter().filter(|u| variation::is_variant_of(&u.0, name)).map(|u| u.1.clone()).collect();
        let def = crate::block::reg().sounds.iter().find(|s| s.id == name);
        if def.is_none() && own.is_empty() {
            return;
        }
        // The game's own thunder is one of several, and comes late from afar.
        if name == "thunder" && own.is_empty() && def.map_or(false, |d| d.clip.is_none()) {
            self.thunder(30.0);
            return;
        }
        // Subtitles are kept whether or not there is a sound device.
        if let Some(d) = def.filter(|d| !d.caption.is_empty() && volume > 0.05) {
            self.caption(&d.caption);
        }
        if self.device == 0 {
            return;
        }
        let bus_n = def.map_or(0, |d| d.bus);
        let bus = match bus_n {
            1 => Bus::Steps,
            2 => Bus::Ui,
            _ => Bus::Effects,
        };
        let (secs, noise, f0, f1, lp, loud) = match def {
            Some(d) => (d.seconds, d.noise, d.hz[0], d.hz[1], d.low_pass, d.loudness),
            None => (0.1, 0.0, 0.0, 0.0, 1.0, 1.0),
        };
        let recorded = !own.is_empty() || def.map_or(false, |d| d.clip.is_some());
        let key = variation::name_key(name);
        let hint = match own.first() {
            Some(c) => c.len() as f32 / RATE as f32,
            None => secs,
        };
        if !self.limiter.allow(key, self.clock, hint, variation::policy_for(bus_n, hint)) {
            return;
        }
        let takes = if !own.is_empty() {
            own.len()
        } else if recorded {
            1
        } else {
            MADE_TAKES
        };
        let take = self.pick_variant(key, takes);
        let j = variation::jitter(&mut self.rng, variation::PITCH_SPREAD, variation::GAIN_DB);
        // Distance: `volume` below 1 means far. It falls off smoothly (an
        // eased curve, not a straight line), the sound goes duller, and a
        // far sound is quieter again when the player is indoors.
        let v = volume.max(0.0);
        let far = 1.0 - v.min(1.0);
        let eased = if v >= 1.0 { v } else { v * v * (3.0 - 2.0 * v) };
        let shelter = self.amb.shelter();
        let dull = (0.45 + 0.55 * (1.0 - far)) * (1.0 - 0.3 * shelter * far);
        let gain = loud
            * eased
            * (1.0 - 0.4 * shelter * far)
            * j.gain
            * match bus {
                Bus::Effects => self.effects,
                Bus::Steps => self.steps,
                Bus::Ui => self.ui,
            };
        if gain <= 0.001 {
            return;
        }
        // A recorded sound: the folder's own, else the data's.
        let clock = crate::sys::now() as u32;
        let clip: Option<&[i16]> = if !own.is_empty() { own[take].get(clock) } else { def.and_then(|d| d.clip.as_ref()).and_then(|c| c.get(clock)) };
        let rng = self.rng.next_u32() | 1;
        // A tone high up pierces: made sounds above 1.8 kHz are let down.
        let top = f0.max(f1);
        let pierce = if top > 1800.0 { (1800.0 / top).clamp(0.5, 1.0) } else { 1.0 };
        let pitch = j.pitch * MADE_PITCH[take % MADE_TAKES];
        let mut voice = Voice {
            left: (secs * RATE as f32) as u32,
            total: (secs * RATE as f32).max(1.0) as u32,
            gain: gain * (1.0 - (1.0 - pierce) * (1.0 - noise)),
            noise,
            step0: f0 * pitch / RATE as f32,
            step1: f1 * pitch / RATE as f32,
            phase: 0.0,
            lp: 0.0,
            // Never as bright as the data asks for: above about 0.6 the noise is a hiss.
            lp_k: (lp * MADE_DULL[take % MADE_TAKES] * dull).clamp(0.003, 0.6),
            rng,
            clip: core::ptr::null(),
            clip_len: 0,
            pos: 0.0,
            step: 1.0,
        };
        if let Some(c) = clip {
            voice.clip = c.as_ptr();
            voice.clip_len = c.len() as u32;
            voice.step = j.pitch;
            voice.left = (c.len() as f32 / j.pitch) as u32 + 2;
            voice.total = voice.left;
            voice.lp_k = (dull * dull).clamp(0.2, 1.0);
            // The recording's own level: 14000 is what made sounds peak at.
            voice.gain = gain * 0.45;
        }
        self.start_voice(voice, true);
    }
}

/// How many takes a made sound has: the same sound with its pitch and
/// brightness moved a little, so two in a row are never alike.
const MADE_TAKES: usize = 4;
const MADE_PITCH: [f32; MADE_TAKES] = [1.0, 0.94, 1.06, 0.97];
const MADE_DULL: [f32; MADE_TAKES] = [1.0, 0.8, 1.2, 0.9];

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
        let mut m = Mixer::new();
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
        let mut m = Mixer::new();
        for (i, v) in m.voices.iter_mut().enumerate() {
            *v = Voice { left: 400, total: 400, gain: 5.0, noise: 1.0, step0: 0.0, step1: 0.0, phase: 0.0, lp: 0.0, lp_k: 1.0, rng: 99 + i as u32, clip: core::ptr::null(), clip_len: 0, pos: 0.0, step: 1.0 };
        }
        let mut buf = [0i16; 400];
        mix(&mut m, &mut buf);
        assert!(buf.iter().all(|&s| (-32000..=32000).contains(&s)));
    }

    #[test]
    fn a_bed_plays_through_the_mixer_and_a_quiet_one_costs_nothing() {
        let mut m = Mixer::new();
        let mut buf = [0i16; 700];
        mix(&mut m, &mut buf);
        assert!(buf.iter().all(|&s| s == 0));
        m.beds[BedKind::Rain.index()].set(1.0, 6000.0);
        let mut loud = 0i32;
        for _ in 0..40 {
            mix(&mut m, &mut buf);
            loud = buf.iter().map(|&s| (s as i32).abs()).max().unwrap();
        }
        assert!(loud > 300 && loud < 20000, "{loud}");
    }

    #[test]
    fn beds_and_voices_together_never_pass_full_scale() {
        let mut m = Mixer::new();
        for b in m.beds.iter_mut() {
            b.set(1.0, 8000.0);
        }
        for (i, v) in m.voices.iter_mut().enumerate() {
            *v = Voice { left: 3000, total: 3000, gain: 5.0, noise: 1.0, step0: 0.0, step1: 0.0, phase: 0.0, lp: 0.0, lp_k: 1.0, rng: 5 + i as u32, clip: core::ptr::null(), clip_len: 0, pos: 0.0, step: 1.0 };
        }
        let mut buf = [0i16; 3000];
        for _ in 0..10 {
            mix(&mut m, &mut buf);
            assert!(buf.iter().all(|&s| (-32000..=32000).contains(&s)));
        }
    }

    #[test]
    fn a_recorded_voice_can_be_made_duller_for_distance() {
        static CLIP: [i16; 64] = {
            let mut a = [0i16; 64];
            let mut i = 0;
            while i < 64 {
                a[i] = if i % 2 == 0 { 8000 } else { -8000 };
                i += 1;
            }
            a
        };
        let play = |k: f32| {
            let mut m = Mixer::new();
            m.voices[0] = Voice { left: 70, total: 70, gain: 1.0, noise: 0.0, step0: 0.0, step1: 0.0, phase: 0.0, lp: 0.0, lp_k: k, rng: 1, clip: CLIP.as_ptr(), clip_len: 64, pos: 0.0, step: 1.0 };
            let mut buf = [0i16; 40];
            mix(&mut m, &mut buf);
            buf.iter().map(|&s| (s as i32).abs()).max().unwrap()
        };
        assert!(play(0.2) < play(1.0) / 2);
    }
}
