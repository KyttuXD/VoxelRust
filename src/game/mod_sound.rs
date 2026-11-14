//! Sound and music for mods, the game's side: handles, limits and the queue.
//!
//! A mod (a script, or native code) calls the functions below with its own
//! name. They hand out a handle at once, queue what the mixer is to do, and
//! `Session::update` carries the queue out once a frame with the sound system
//! at hand ([`flush`]). So a call never waits for the audio device, never
//! fails because of it, and a sound that could not start (a name that does not
//! exist, no sound device) just has its handle reported over at the next
//! flush.
//!
//! The limits are in `util/audio/handles.rs`: a mod holds at most 8 live
//! handles (4 of them loops), starts at most 8 sounds a frame, and a mod can
//! only control its own handles. Music is one channel per world: any mod may
//! start a track (it replaces the one playing, with a crossfade); only the mod
//! whose track it is can stop it or set its volume.
//!
//! Native code calls these through `crate::native::sound`; scripts through
//! `script/ext/sound.rs`. What the mixer does with a request is in
//! `client/audio/modplay.rs` and `client/audio/music.rs`; the whole is
//! described in docs/SOUND.md ("Sound for mods").

use crate::audio::{Audio, Request};
use crate::session::Session;
use crate::wav::handles::HandleBook;
use alloc::string::String;
use alloc::vec::Vec;

/// Most requests queued between two flushes.
const MAX_QUEUED: usize = 128;

/// One thing the mixer is asked to do.
pub enum Cmd {
    Play(Request),
    Stop(u32),
    Fade { handle: u32, volume: f32, secs: f32 },
    Set { handle: u32, volume: f32, pitch: f32, pan: f32, at: Option<[f64; 3]> },
    Music { owner: String, name: String, volume: f32, fade_in: f32, looping: bool },
    StopMusic(f32),
    MusicVolume(f32),
}

#[derive(Default)]
pub struct ModSound {
    book: HandleBook,
    cmds: Vec<Cmd>,
    /// The mod whose track plays on the music channel.
    music: Option<String>,
    /// Seconds until the mods that hold sounds are checked to still be on.
    check: f32,
    /// Refusals already reported (a mod is told once in the log).
    told: Vec<String>,
}

impl ModSound {
    fn tell(&mut self, owner: &str, what: &str) {
        let key = alloc::format!("{}:{}", owner, what);
        if !self.told.contains(&key) && self.told.len() < 64 {
            crate::logf!("voxelrust: mod `{}`: {}", owner, what);
            self.told.push(key);
        }
    }

    /// Queue a sound; its handle, or 0 when it has none (it then cannot be
    /// controlled; a loop is refused, a one-shot plays anyway) or is dropped.
    #[allow(clippy::too_many_arguments)]
    pub fn play(&mut self, owner: &str, name: &str, volume: f32, pitch: f32, pan: f32, at: Option<[f64; 3]>, looping: bool) -> u32 {
        if self.cmds.len() >= MAX_QUEUED || !self.book.may_play(owner) {
            return 0;
        }
        let handle = match self.book.alloc(owner, looping) {
            Ok(h) => h,
            Err(why) => {
                if looping {
                    self.tell(owner, &alloc::format!("a loop was refused ({:?}): a mod may hold 8 sounds, 4 of them loops", why));
                    return 0;
                }
                0
            }
        };
        self.cmds.push(Cmd::Play(Request { owner: String::from(owner), name: String::from(name), volume, pitch, pan, at, looping, handle }));
        handle
    }

    /// Stop a sound of `owner` (a stale or foreign handle does nothing). It is
    /// over for the mod at once; the mixer takes a few milliseconds to go quiet.
    pub fn stop(&mut self, owner: &str, handle: u32) -> bool {
        if !self.book.owned_by(handle, owner) {
            return false;
        }
        self.book.release(handle);
        self.cmds.push(Cmd::Stop(handle));
        true
    }

    /// Move a sound's volume to `volume` over `secs` (to 0: it stops there).
    pub fn fade(&mut self, owner: &str, handle: u32, volume: f32, secs: f32) -> bool {
        if !self.book.owned_by(handle, owner) || self.cmds.len() >= MAX_QUEUED {
            return false;
        }
        self.cmds.push(Cmd::Fade { handle, volume, secs });
        true
    }

    /// Change a sound's volume, pitch, pan and place.
    pub fn set(&mut self, owner: &str, handle: u32, volume: f32, pitch: f32, pan: f32, at: Option<[f64; 3]>) -> bool {
        if !self.book.owned_by(handle, owner) || self.cmds.len() >= MAX_QUEUED {
            return false;
        }
        self.cmds.push(Cmd::Set { handle, volume, pitch, pan, at });
        true
    }

    /// Whether the mod's sound is still playing (queued or in the mixer).
    pub fn is_playing(&self, owner: &str, handle: u32) -> bool {
        self.book.owned_by(handle, owner)
    }

    /// Start music (see the module's note on who may).
    pub fn play_music(&mut self, owner: &str, name: &str, volume: f32, fade_in: f32, looping: bool) -> bool {
        if self.cmds.len() >= MAX_QUEUED {
            return false;
        }
        self.cmds.push(Cmd::Music { owner: String::from(owner), name: String::from(name), volume, fade_in, looping });
        self.music = Some(String::from(owner));
        true
    }

    /// Fade the music out, if it is `owner`'s.
    pub fn stop_music(&mut self, owner: &str, secs: f32) -> bool {
        if self.music.as_deref() != Some(owner) {
            return false;
        }
        self.cmds.push(Cmd::StopMusic(secs));
        self.music = None;
        true
    }

    /// Set the music's volume, if it is `owner`'s.
    pub fn set_music_volume(&mut self, owner: &str, v: f32) -> bool {
        if self.music.as_deref() != Some(owner) || self.cmds.len() >= MAX_QUEUED {
            return false;
        }
        self.cmds.push(Cmd::MusicVolume(v));
        true
    }

    /// Whether the world's music channel has a track.
    pub fn music_playing(&self) -> bool {
        self.music.is_some()
    }

    /// Carry out the queue, learn what ended, and drop what belongs to mods
    /// that are no longer on (`alive` says which are). Once a frame.
    pub fn flush_into(&mut self, audio: &mut Audio, dt: f32, alive: &dyn Fn(&str) -> bool) {
        self.book.new_frame();
        // Mods that were switched off (or whose script failed) stop sounding.
        self.check -= dt;
        if self.check <= 0.0 {
            self.check = 0.5;
            for owner in self.book.owners() {
                if !alive(&owner) {
                    for h in self.book.release_owner(&owner) {
                        audio.stop_handle(h);
                    }
                }
            }
            if let Some(o) = self.music.clone() {
                if !alive(&o) {
                    audio.stop_music(1.0);
                    self.music = None;
                }
            }
        }
        for cmd in core::mem::take(&mut self.cmds) {
            match cmd {
                Cmd::Play(r) => {
                    audio.play_request(&r);
                }
                Cmd::Stop(h) => audio.stop_handle(h),
                Cmd::Fade { handle, volume, secs } => audio.fade_handle(handle, volume, secs),
                Cmd::Set { handle, volume, pitch, pan, at } => audio.set_handle(handle, volume, pitch, pan, at),
                Cmd::Music { owner, name, volume, fade_in, looping } => {
                    if !audio.play_music(&owner, &name, volume, fade_in, looping) {
                        self.music = None;
                    }
                }
                Cmd::StopMusic(secs) => audio.stop_music(secs),
                Cmd::MusicVolume(v) => audio.set_music_volume(v),
            }
        }
        let ended = audio.take_ended();
        for h in ended.handles {
            self.book.release(h);
        }
        if ended.music {
            self.music = None;
        }
    }
}

/// Whether the mod (script or native code) of that name is on and working.
pub fn owner_alive(owner: &str) -> bool {
    if crate::native::active().any(|(_, n)| n.name == owner) {
        return true;
    }
    // (While a script runs the host is busy: assume the mod is on.)
    crate::script::with_host(|h| h.mods.iter().any(|m| m.name == owner && m.scripts.iter().any(|s| s.disabled.is_none()))).unwrap_or(true)
}

/// Once a frame, with the sound system at hand (`Session::update`).
pub fn flush(g: &mut Session, audio: &mut Audio, dt: f32) {
    audio.set_listener(g.player.eye(), g.player.yaw);
    g.mod_sound.flush_into(audio, dt, &owner_alive);
}

// ---- for native code: the same calls on the session (see `native::sound`)

/// Play a sound (see [`ModSound::play`]); `at` places it in the world, else
/// `pan` (-1 to 1) places it between the ears.
#[allow(clippy::too_many_arguments)]
pub fn play(g: &mut Session, owner: &str, name: &str, volume: f32, pitch: f32, pan: f32, at: Option<[f64; 3]>, looping: bool) -> u32 {
    g.mod_sound.play(owner, name, volume, pitch, pan, at, looping)
}

pub fn stop(g: &mut Session, owner: &str, handle: u32) -> bool {
    g.mod_sound.stop(owner, handle)
}

pub fn fade(g: &mut Session, owner: &str, handle: u32, volume: f32, secs: f32) -> bool {
    g.mod_sound.fade(owner, handle, volume, secs)
}

pub fn set(g: &mut Session, owner: &str, handle: u32, volume: f32, pitch: f32, pan: f32, at: Option<[f64; 3]>) -> bool {
    g.mod_sound.set(owner, handle, volume, pitch, pan, at)
}

pub fn is_playing(g: &Session, owner: &str, handle: u32) -> bool {
    g.mod_sound.is_playing(owner, handle)
}

pub fn play_music(g: &mut Session, owner: &str, name: &str, volume: f32, fade_in: f32, looping: bool) -> bool {
    g.mod_sound.play_music(owner, name, volume, fade_in, looping)
}

pub fn stop_music(g: &mut Session, owner: &str, secs: f32) -> bool {
    g.mod_sound.stop_music(owner, secs)
}

pub fn set_music_volume(g: &mut Session, owner: &str, v: f32) -> bool {
    g.mod_sound.set_music_volume(owner, v)
}

pub fn music_playing(g: &Session) -> bool {
    g.mod_sound.music_playing()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wav::handles::{LOOPS_PER_OWNER, PER_OWNER, PLAYS_PER_FRAME};

    fn on(_: &str) -> bool {
        true
    }

    #[test]
    fn a_play_gives_a_handle_that_is_the_mods_own() {
        let mut m = ModSound::default();
        let h = m.play("a", "boom", 1.0, 1.0, 0.0, None, false);
        assert!(h != 0 && m.is_playing("a", h) && !m.is_playing("b", h));
        // Another mod cannot stop, fade or change it.
        assert!(!m.stop("b", h) && !m.fade("b", h, 0.0, 1.0) && !m.set("b", h, 1.0, 1.0, 0.0, None));
        assert!(m.fade("a", h, 0.0, 1.0));
        assert!(m.stop("a", h));
        assert!(!m.is_playing("a", h), "stopped is over at once");
        assert!(!m.stop("a", h), "and a stale handle does nothing");
    }

    #[test]
    fn the_limits_hold() {
        let mut m = ModSound::default();
        let mut loops = 0;
        for _ in 0..PLAYS_PER_FRAME {
            if m.play("a", "x", 1.0, 1.0, 0.0, None, true) != 0 {
                loops += 1;
            }
        }
        assert_eq!(loops, LOOPS_PER_OWNER);
        // The per-frame count is used up: more plays are dropped.
        assert_eq!(m.play("a", "x", 1.0, 1.0, 0.0, None, false), 0);
        assert_eq!(m.cmds.len(), LOOPS_PER_OWNER, "a refused loop is not queued");
        // Another mod is not held back.
        assert!(m.play("b", "x", 1.0, 1.0, 0.0, None, true) != 0);
    }

    #[test]
    fn a_one_shot_over_the_handle_limit_still_plays_without_one() {
        let mut m = ModSound::default();
        let mut with = 0;
        for _frame in 0..3 {
            m.book.new_frame();
            for _ in 0..PLAYS_PER_FRAME {
                if m.play("a", "x", 1.0, 1.0, 0.0, None, false) != 0 {
                    with += 1;
                }
            }
        }
        assert_eq!(with, PER_OWNER);
        assert_eq!(m.cmds.len(), PLAYS_PER_FRAME * 3, "all of them were queued");
    }

    #[test]
    fn the_queue_is_bounded() {
        let mut m = ModSound::default();
        for f in 0..40 {
            m.book.new_frame();
            for _ in 0..PLAYS_PER_FRAME {
                m.play(&alloc::format!("mod{}", f), "x", 1.0, 1.0, 0.0, None, false);
            }
        }
        assert!(m.cmds.len() <= MAX_QUEUED);
    }

    #[test]
    fn a_flush_into_a_silent_system_ends_the_handles_and_empties_the_queue() {
        let mut m = ModSound::default();
        let h = m.play("a", "no_such_sound_anywhere", 1.0, 1.0, 0.0, None, false);
        assert!(m.is_playing("a", h));
        let mut audio = Audio::silent();
        m.flush_into(&mut audio, 0.016, &on);
        assert!(m.cmds.is_empty());
        assert!(!m.is_playing("a", h), "it never started, so it is over");
    }

    #[test]
    fn music_belongs_to_the_mod_that_started_it() {
        let mut m = ModSound::default();
        assert!(!m.music_playing());
        assert!(m.play_music("a", "theme", 1.0, 2.0, true));
        assert!(m.music_playing());
        assert!(!m.stop_music("b", 1.0) && !m.set_music_volume("b", 0.5));
        assert!(m.set_music_volume("a", 0.5));
        // The silent system cannot play it: the channel is free again after the flush.
        let mut audio = Audio::silent();
        m.flush_into(&mut audio, 0.016, &on);
        assert!(!m.music_playing());
    }

    #[test]
    fn a_stopped_music_channel_is_free() {
        let mut m = ModSound::default();
        m.play_music("a", "theme", 1.0, 0.0, false);
        assert!(m.stop_music("a", 0.5));
        assert!(!m.music_playing());
        assert!(!m.stop_music("a", 0.5));
    }

    #[test]
    fn a_mod_that_is_switched_off_loses_its_sounds_and_music() {
        let mut m = ModSound::default();
        let h = m.play("gone", "x", 1.0, 1.0, 0.0, None, true);
        m.play_music("gone", "theme", 1.0, 0.0, true);
        assert!(m.is_playing("gone", h) && m.music_playing());
        let mut audio = Audio::silent();
        m.flush_into(&mut audio, 1.0, &|o| o != "gone");
        assert!(!m.is_playing("gone", h) && !m.music_playing());
        assert!(m.book.owners().is_empty());
    }
}
