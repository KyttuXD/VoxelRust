//! Sounds that mods control: placed in the world, panned, pitched, looped,
//! and held by a handle so they can be stopped, faded and changed. The mod's
//! side of this is `game/mod_sound.rs` (which hands out the handles and
//! limits them); this is what the mixer does. See docs/SOUND.md.
//!
//! A started sound is a voice in the mixer carrying its handle. The game
//! thread keeps a [`Tracked`] record for each handle (where the source is, the
//! volume the mod asked for, a fade in progress) and once a frame works out
//! the voice's gain, pan and speed from it and the listener's place; the
//! mixer glides to the new numbers over the next stretch of samples, so a
//! once-a-frame update does not make steps.

use super::*;
use crate::util::data::reread;
use crate::wav::clip::Clip;
use crate::wav::fade::{Curve, Fade};
use crate::wav::{loopseam, resample, spatial};
use alloc::format;
use alloc::string::String;

/// A sound a mod wants played.
pub struct Request {
    /// The mod (or native code) playing it.
    pub owner: String,
    /// The sound's name: a sound of the data files, or a file of the mod's `sounds/` folder.
    pub name: String,
    /// 0 to 2.
    pub volume: f32,
    /// Playback rate, 0.25 to 4.
    pub pitch: f32,
    /// -1 (left) to 1 (right); ignored when `at` is given.
    pub pan: f32,
    /// A place in the world: the sound is then louder near it and comes from its side.
    pub at: Option<[f64; 3]>,
    /// Play until stopped.
    pub looping: bool,
    /// The handle it is held by (0: none, it cannot be controlled).
    pub handle: u32,
}

/// What became of the sounds and music since the game last asked.
#[derive(Default)]
pub struct Ended {
    /// Handles whose sounds are over (or never started).
    pub handles: Vec<u32>,
    /// The music ended by itself.
    pub music: bool,
}

/// What the game thread knows of a sound with a handle.
pub(super) struct Tracked {
    handle: u32,
    at: Option<[f64; 3]>,
    /// Loudness of the recording itself, with its bus volume.
    base: f32,
    /// The volume the mod asked for (a fade moves it).
    vol: f32,
    pan: f32,
    pitch: f32,
    fade: Option<Fade>,
    /// Seconds until the voice is cut, after its gain was sent to zero.
    cut_in: Option<f32>,
    /// A recording (its speed follows the pitch); a made sound's pitch is fixed when it starts.
    clip: bool,
}

/// Most sounds with handles at once.
const MAX_TRACKED: usize = 64;
/// Most distinct sound files of mods kept.
const MAX_MOD_CLIPS: usize = 96;
/// What the game's own ambience is turned down to while music is audible (-3 dB).
pub(super) const DUCK: f32 = 0.708;
/// Seconds a stopped sound takes to go quiet before it is cut.
const STOP_SECS: f32 = 0.04;

const EXTS: [&str; 6] = [".ogg", ".wav", ".mp3", ".flac", ".aiff", ".aif"];

/// Whether a name may be turned into a file path: letters, digits, `_ - .`
/// and `/` inside it, no `..`, nothing absolute.
pub fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('/')
        && !name.starts_with('.')
        && !name.contains("..")
        && !name.contains("//")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
}

/// What `find_sound` found.
struct Found {
    clip: Option<Clip>,
    /// A made sound: seconds, noise share, pitch from and to, low-pass.
    made: Option<(f32, f32, f32, f32, f32)>,
    loud: f32,
    bus: u8,
    caption: String,
}

impl Audio {
    /// Where the listener is and which way they face (once a frame, before `tick`).
    pub fn set_listener(&mut self, eye: [f64; 3], yaw: f32) {
        self.listener = (eye, if yaw.is_finite() { yaw } else { 0.0 });
    }

    /// Where the mods' own files are (`<data>/mods`).
    pub fn set_mods_dir(&mut self, dir: &str) {
        self.mods_dir = String::from(dir.trim_end_matches(|c| c == '/' || c == '\\'));
        self.mod_clips.clear();
    }

    /// Handles of sounds that ended, and whether the music did.
    pub fn take_ended(&mut self) -> Ended {
        Ended { handles: core::mem::take(&mut self.ended), music: self.take_music_ended() }
    }

    /// Say once that a name was not found.
    pub(super) fn note_missing(&mut self, owner: &str, name: &str, kind: &str) {
        let key = format!("{}:{}:{}", kind, owner, name);
        if self.told.contains(&key) {
            return;
        }
        if self.told.len() >= 64 {
            self.told.remove(0);
        }
        self.told.push(key);
        crate::logf!("voxelrust: mod `{}` asked for the {} `{}`, which does not exist: ignored", owner, kind, name);
    }

    /// A file of the mod's folder: `<mods>/<owner>/<folder>/<name>[.ext]`,
    /// found as a plain file (or by the program's reader, for a zip).
    fn mod_file(&self, owner: &str, folder: &str, name: &str, limit: usize) -> Option<(String, Vec<u8>)> {
        if self.mods_dir.is_empty() || !safe_name(owner) || !safe_name(name) {
            return None;
        }
        let known = EXTS.iter().any(|e| name.to_ascii_lowercase().ends_with(e));
        let mut tries: Vec<String> = Vec::new();
        if known {
            tries.push(format!("{}/{}/{}/{}", self.mods_dir, owner, folder, name));
        }
        for e in EXTS {
            tries.push(format!("{}/{}/{}/{}{}", self.mods_dir, owner, folder, name, e));
        }
        for path in tries {
            let bytes = if crate::sys::file_exists(&path) { crate::sys::read_file_limit(&path, limit) } else { reread::read(&path).filter(|b| b.len() <= limit) };
            if let Some(b) = bytes {
                return Some((path, b));
            }
        }
        None
    }

    /// The bytes of a mod's music file (`music/` first, then `sounds/`).
    pub(super) fn music_bytes_of(&self, owner: &str, name: &str) -> Option<Vec<u8>> {
        let limit = crate::wav::music_source::MAX_MUSIC_BYTES;
        self.mod_file(owner, "music", name, limit).or_else(|| self.mod_file(owner, "sounds", name, limit)).map(|f| f.1)
    }

    /// A recording out of the mod's `sounds/` folder (decoded once, kept).
    fn mod_clip(&mut self, owner: &str, name: &str) -> Option<Clip> {
        let key = format!("{}\0{}", owner, name);
        if let Some(e) = self.mod_clips.iter().find(|e| e.0 == key) {
            return e.1.clone();
        }
        if self.mod_clips.len() >= MAX_MOD_CLIPS {
            return None;
        }
        let clip = self.mod_file(owner, "sounds", name, crate::wav::MAX_BYTES).and_then(|(path, bytes)| crate::wav::decode(&bytes).map(|s| Clip::new(&path, s)));
        self.mod_clips.push((key, clip.clone()));
        clip
    }

    /// The sound a name stands for: the player's own `sounds/` folder, else
    /// the data files (a recording or a made sound), else the mod's own
    /// `sounds/` folder.
    fn find_sound(&mut self, owner: &str, name: &str) -> Option<Found> {
        let own: Vec<Clip> = self.user.iter().filter(|u| variation::is_variant_of(&u.0, name)).map(|u| u.1.clone()).collect();
        let def = crate::block::reg().sounds.iter().find(|s| s.id == name);
        let caption = def.map(|d| d.caption.clone()).unwrap_or_default();
        if !own.is_empty() {
            let take = self.pick_variant(variation::name_key(name), own.len());
            return Some(Found { clip: Some(own[take].clone()), made: None, loud: 1.0, bus: def.map_or(0, |d| d.bus), caption });
        }
        if let Some(d) = def {
            if let Some(c) = &d.clip {
                return Some(Found { clip: Some(c.clone()), made: None, loud: d.loudness, bus: d.bus, caption });
            }
            return Some(Found { clip: None, made: Some((d.seconds, d.noise, d.hz[0], d.hz[1], d.low_pass)), loud: d.loudness, bus: d.bus, caption });
        }
        self.mod_clip(owner, name).map(|c| Found { clip: Some(c), made: None, loud: 1.0, bus: 0, caption })
    }

    /// Loudness, brightness and pan of a source at `at` (or without a place).
    fn spatial(&self, at: Option<[f64; 3]>, pan: f32) -> (f32, f32, f32) {
        match at {
            Some(p) => {
                let d = spatial::distance(self.listener.0, p);
                (spatial::distance_gain(d), spatial::brightness(d), spatial::pan_of(self.listener.0, self.listener.1, p))
            }
            None => (1.0, 1.0, if pan.is_finite() { pan.clamp(-1.0, 1.0) } else { 0.0 }),
        }
    }

    /// Start a sound for a mod. Returns whether it plays (an unknown name,
    /// no sound device, or a sound too far to hear does not). A sound with a
    /// handle that does not start is reported ended at once (`take_ended`).
    pub fn play_request(&mut self, r: &Request) -> bool {
        let ok = self.start_request(r);
        if !ok && r.handle != 0 {
            self.ended.push(r.handle);
        }
        ok
    }

    fn start_request(&mut self, r: &Request) -> bool {
        let Some(found) = self.find_sound(&r.owner, &r.name) else {
            self.note_missing(&r.owner, &r.name, "sound");
            return false;
        };
        let vol = if r.volume.is_finite() { r.volume.clamp(0.0, 2.0) } else { 1.0 };
        let pitch = resample::clamp_pitch(r.pitch);
        let (dist, bright, pan) = self.spatial(r.at, r.pan);
        // The subtitle, for what could be heard.
        if !found.caption.is_empty() && vol * dist > 0.05 {
            self.caption(&found.caption);
        }
        if self.device == 0 {
            return false;
        }
        let bus = match found.bus {
            1 => self.steps,
            2 => self.ui,
            _ => self.effects,
        };
        let (pl, pr) = spatial::balance(pan);
        let mut voice = SILENT;
        voice.handle = r.handle;
        voice.pl = pl;
        voice.pr = pr;
        let base;
        let clock = crate::sys::now() as u32;
        if let Some(clip) = &found.clip {
            let Some(samples) = clip.get(clock) else { return false };
            if samples.len() < 2 {
                return false;
            }
            base = found.loud * bus * 0.45;
            voice.clip = samples.as_ptr();
            voice.clip_len = samples.len() as u32;
            voice.step = pitch;
            voice.lp_k = bright.clamp(0.2, 1.0);
            if r.looping {
                voice.looping = true;
                voice.xf = loopseam::seam_len(samples.len()) as u32;
                voice.left = u32::MAX;
                voice.total = u32::MAX;
            } else {
                voice.left = (samples.len() as f32 / pitch) as u32 + 2;
                voice.total = voice.left;
            }
        } else if let Some((secs, noise, f0, f1, lp)) = found.made {
            if r.looping {
                // A made sound is a burst; there is nothing to loop.
                self.note_missing(&r.owner, &r.name, "loopable sound (a made one cannot loop)");
                return false;
            }
            base = found.loud * bus;
            let total = ((secs.clamp(0.01, 6.0) * RATE as f32) as u32).max(1);
            voice.left = total;
            voice.total = total;
            voice.noise = noise.clamp(0.0, 1.0);
            voice.step0 = f0 * pitch / RATE as f32;
            voice.step1 = f1 * pitch / RATE as f32;
            voice.lp_k = (lp * bright).clamp(0.003, 0.6);
            voice.rng = self.rng.next_u32() | 1;
        } else {
            return false;
        }
        voice.gain = base * vol * dist;
        // Too far to hear is not started, unless it is a loop that may come near.
        if voice.gain <= 0.001 && !r.looping {
            return false;
        }
        if r.handle != 0 {
            if self.tracked.len() >= MAX_TRACKED {
                return false;
            }
            self.tracked.push(Tracked { handle: r.handle, at: r.at, base, vol, pan: if r.at.is_some() { 0.0 } else { pan }, pitch, fade: None, cut_in: None, clip: found.clip.is_some() });
        }
        self.start_voice(voice, true);
        true
    }

    fn tracked_at(&mut self, handle: u32) -> Option<&mut Tracked> {
        self.tracked.iter_mut().find(|t| t.handle == handle)
    }

    /// Stop a sound: it goes quiet in a few milliseconds and is cut.
    pub fn stop_handle(&mut self, handle: u32) {
        if let Some(t) = self.tracked_at(handle) {
            t.fade = None;
            t.cut_in = Some(STOP_SECS);
        }
    }

    /// Move a sound's volume to `volume` (0 to 2) over `secs`; a volume of 0
    /// stops it when it gets there.
    pub fn fade_handle(&mut self, handle: u32, volume: f32, secs: f32) {
        let target = if volume.is_finite() { volume.clamp(0.0, 2.0) } else { return };
        if let Some(t) = self.tracked_at(handle) {
            if t.cut_in.is_some() {
                return;
            }
            let mut f = Fade::new(t.vol, target, secs, Curve::Smooth);
            if f.done() {
                t.vol = f.tick(0.0);
                t.fade = None;
                if target <= 0.0005 {
                    t.cut_in = Some(STOP_SECS);
                }
            } else {
                t.fade = Some(f);
            }
        }
    }

    /// Change a playing sound: its volume (ends a fade), pitch and pan (a
    /// sound with a place takes its pan from the place) and, for a sound with
    /// a place, the place.
    pub fn set_handle(&mut self, handle: u32, volume: f32, pitch: f32, pan: f32, at: Option<[f64; 3]>) {
        if let Some(t) = self.tracked_at(handle) {
            if t.cut_in.is_some() {
                return;
            }
            if volume.is_finite() {
                t.vol = volume.clamp(0.0, 2.0);
                t.fade = None;
            }
            t.pitch = resample::clamp_pitch(pitch);
            if pan.is_finite() && t.at.is_none() {
                t.pan = pan.clamp(-1.0, 1.0);
            }
            if let (Some(p), Some(_)) = (at, t.at) {
                t.at = Some(p);
            }
        }
    }

    /// Once a frame: music, the ducking it causes, and the sounds with handles.
    pub(super) fn mod_tick(&mut self, dt: f32) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.5) } else { 0.0 };
        self.music_tick(dt);
        let target = if self.deck.audible() { DUCK } else { 1.0 };
        self.duck += (target - self.duck) * (dt * 1.5).min(1.0);
        self.track_tick(dt);
    }

    fn track_tick(&mut self, dt: f32) {
        if self.tracked.is_empty() {
            return;
        }
        if self.device == 0 {
            let all: Vec<u32> = self.tracked.drain(..).map(|t| t.handle).collect();
            self.ended.extend(all);
            return;
        }
        let (ear, yaw) = self.listener;
        let api = sdl();
        unsafe {
            (api.SDL_LockAudioDevice)(self.device);
            let m = &mut *self.mixer;
            let mut i = 0;
            while i < self.tracked.len() {
                let t = &mut self.tracked[i];
                let handle = t.handle;
                let Some(vi) = m.voices.iter().position(|v| v.handle == handle && v.left > 0) else {
                    self.ended.push(handle);
                    self.tracked.swap_remove(i);
                    continue;
                };
                let v = &mut m.voices[vi];
                if let Some(f) = t.fade.as_mut() {
                    t.vol = f.tick(dt);
                    if f.done() {
                        t.fade = None;
                        if t.vol <= 0.0005 {
                            t.cut_in = Some(STOP_SECS);
                        }
                    }
                }
                if let Some(c) = t.cut_in.as_mut() {
                    // Quiet first, then cut (the mixer glides the gain down meanwhile).
                    v.gain_t = 0.0;
                    *c -= dt;
                    if *c <= 0.0 {
                        v.left = 0;
                        v.handle = 0;
                        self.ended.push(handle);
                        self.tracked.swap_remove(i);
                        continue;
                    }
                    i += 1;
                    continue;
                }
                let (d, pan) = match t.at {
                    Some(p) => (spatial::distance(ear, p), spatial::pan_of(ear, yaw, p)),
                    None => (0.0, t.pan),
                };
                let (pl, pr) = spatial::balance(pan);
                v.gain_t = t.base * t.vol * spatial::distance_gain(d);
                v.pl_t = pl;
                v.pr_t = pr;
                if t.clip {
                    v.step_t = t.pitch;
                    if t.at.is_some() {
                        v.lp_k = spatial::brightness(d).clamp(0.2, 1.0);
                    }
                }
                i += 1;
            }
            (api.SDL_UnlockAudioDevice)(self.device);
        }
    }

    /// Forget every sound and track of the mods (leaving a world).
    pub(super) fn silence_mods(&mut self) {
        self.tracked.clear();
        self.ended.clear();
        self.silence_music();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_that_could_leave_the_mods_folder_are_not_paths() {
        for ok in ["roar", "storm_loop", "boss-theme", "music/boss", "a.b", "Wind2"] {
            assert!(safe_name(ok), "{ok}");
        }
        for bad in ["", "..", "../x", "a/../b", "/etc/passwd", ".hidden", "a//b", "a\\b", "a b", "c:x", "x\0y", &"a".repeat(65)] {
            assert!(!safe_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_silent_audio_system_starts_nothing_and_reports_the_handle_ended() {
        let mut a = Audio::silent();
        let r = Request { owner: String::from("m"), name: String::from("no_such_sound_anywhere"), volume: 1.0, pitch: 1.0, pan: 0.0, at: None, looping: false, handle: 257 };
        assert!(!a.play_request(&r));
        let e = a.take_ended();
        assert_eq!(e.handles, alloc::vec![257]);
        assert!(!e.music);
        assert!(a.take_ended().handles.is_empty());
        // Controlling a handle that is not there is harmless.
        a.stop_handle(999);
        a.fade_handle(999, 0.5, 1.0);
        a.set_handle(999, 1.0, 1.0, 0.0, None);
        a.set_listener([0.0; 3], f32::NAN);
        a.mod_tick(0.016);
    }

    #[test]
    fn music_from_a_silent_system_is_refused_and_the_same_volume_calls_are_safe() {
        let mut a = Audio::silent();
        assert!(!a.play_music("m", "theme", 1.0, 2.0, true));
        a.stop_music(1.0);
        a.set_music_volume(f32::NAN);
        assert!(!a.music_active());
    }
}
