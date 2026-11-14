//! The ambient part of the sound system: beds that run in the mixer (rain,
//! wind, water, lava, fire, cave), the single sounds on top of them, and
//! thunder. The plan for all of it is `util/audio/ambience.rs`; this only
//! carries it out. See docs/SOUND.md.

use super::*;
use crate::wav::ambience::OneShot;

/// The game's own sound of that id and its subtitle, if it has one.
fn caption_of(id: &str) -> Option<alloc::string::String> {
    crate::block::reg().sounds.iter().find(|s| s.id == id).filter(|s| !s.caption.is_empty()).map(|s| s.caption.clone())
}

/// Longest the game may go without telling us where the player is before the
/// ambience fades away (menus and pauses stop the updates).
const ENV_STALE_SECONDS: f32 = 1.5;

impl Audio {
    /// Say what is around the player (call every few tenths of a second
    /// while the game runs).
    pub fn set_environment(&mut self, env: Env) {
        self.env = env;
        self.env_age = 0.0;
    }

    /// A lightning strike `dist` blocks away: it is heard later the farther
    /// it is, and quieter and duller. Never the same take twice in a row.
    pub fn thunder(&mut self, dist: f32) {
        if let Some(text) = caption_of("thunder") {
            self.caption(&text);
        }
        if self.device == 0 {
            return;
        }
        let mut out = core::mem::take(&mut self.scratch);
        out.clear();
        self.amb.thunder(dist, &mut out);
        for t in out.drain(..) {
            if self.pending.len() < 32 {
                self.pending.push((t.delay, t));
            }
        }
        self.scratch = out;
    }

    /// A single made sound, from the ambience plan. It never takes a voice
    /// that is playing something else.
    fn start_shot(&mut self, s: &OneShot, bus_gain: f32) {
        let gain = s.gain * bus_gain;
        if self.device == 0 || gain <= 0.001 {
            return;
        }
        let secs = s.secs.clamp(0.01, 6.0);
        let total = ((secs * RATE as f32) as u32).max(1);
        let voice = Voice {
            left: total,
            total,
            gain,
            noise: s.noise.clamp(0.0, 1.0),
            step0: s.hz0 / RATE as f32,
            step1: s.hz1 / RATE as f32,
            phase: 0.0,
            lp: 0.0,
            lp_k: s.lp.clamp(0.003, 0.6),
            rng: self.rng.next_u32() | 1,
            clip: core::ptr::null(),
            clip_len: 0,
            pos: 0.0,
            step: 1.0,
        };
        self.start_voice(voice, false);
    }

    /// Once a frame: move the plan on, play what is due, and hand the beds
    /// their targets.
    pub(super) fn ambient_tick(&mut self, dt: f32) {
        self.env_age += dt.max(0.0);
        let env = if self.env_age > ENV_STALE_SECONDS { Env::default() } else { self.env };
        let mut out = core::mem::take(&mut self.scratch);
        out.clear();
        self.amb.update(dt, &env, &mut out);
        for t in out.drain(..) {
            if self.pending.len() < 32 {
                self.pending.push((t.delay, t));
            }
        }
        self.scratch = out;
        // Single sounds whose time has come.
        let mut i = 0;
        while i < self.pending.len() {
            self.pending[i].0 -= dt;
            if self.pending[i].0 <= 0.0 {
                let (_, t) = self.pending.swap_remove(i);
                let bus = if t.effects { self.effects } else { self.ambience };
                self.start_shot(&t.shot, bus);
            } else {
                i += 1;
            }
        }
        // Subtitles for the beds that carry one.
        self.caption_timer -= dt;
        if self.caption_timer <= 0.0 {
            self.caption_timer = 2.0;
            if self.amb.targets[BedKind::Rain.index()].level > 0.2 {
                if let Some(text) = caption_of("rain") {
                    self.caption(&text);
                }
            }
            if self.amb.targets[BedKind::Water.index()].level > 0.3 && !env.underwater {
                if let Some(text) = caption_of("wave") {
                    self.caption(&text);
                }
            }
        }
        if self.device == 0 {
            return;
        }
        let api = sdl();
        unsafe {
            (api.SDL_LockAudioDevice)(self.device);
            let m = &mut *self.mixer;
            for k in BedKind::ALL {
                let t = self.amb.targets[k.index()];
                m.beds[k.index()].set(t.level * self.ambience, t.top_hz);
            }
            (api.SDL_UnlockAudioDevice)(self.device);
        }
    }
}
