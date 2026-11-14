//! Keeping the game within its memory budget (the numbers and the order of
//! giving things up are in `util/budget`; this gathers the numbers from the
//! game and carries out the steps).
//!
//! Once a second it adds up what the game holds, by kind, and asks the ladder
//! what to do. The settings are never changed: the budget only lowers what is
//! used (view distance, distant terrain) and puts it back when memory has
//! been low for a while. With `--log` a line with the numbers is written every
//! 30 seconds, and F3 shows them.

use super::audio::Audio;
use super::render::Renderer;
use crate::budget::{self, Action, Budget, Cat, Effective, Ladder, Wanted};
use crate::session::Session;
use alloc::string::String;

/// Seconds between two countings.
const MEASURE_EVERY: f32 = 1.0;
/// Seconds between two log lines.
const LOG_EVERY: f32 = 30.0;
/// Seconds between two returns of freed memory to the system.
const TRIM_EVERY: f32 = 60.0;
/// What the game needs whatever is loaded: the program, the registry, fonts,
/// the interface, the allocator's own overhead.
const BASE_BYTES: usize = 60 << 20;
/// A creature, with its model state, path and the like.
const MOB_BYTES: usize = 1536;
/// The block atlas on the card and in the driver, with its mip levels.
const ATLAS_BYTES: usize = crate::textures::ATLAS * crate::textures::ATLAS * 4 * 4 / 3;
/// Fonts, clouds, the interface and skins.
const UI_TEXTURE_BYTES: usize = 3 << 20;

pub struct MemWatch {
    pub budget: Budget,
    pub ladder: Ladder,
    /// Writing the numbers to the log is wanted (`--log`).
    pub log: bool,
    /// Whether the bigger limit (mods) is the base: the limit is it plus the HD allowance.
    with_mods: bool,
    since_measure: f32,
    since_log: f32,
    since_trim: f32,
    /// Decoded sound held at the last counting.
    audio_bytes: usize,
    /// Number of times sounds were trimmed and steps taken, for the log.
    trims: u32,
    steps_taken: u32,
}

impl MemWatch {
    pub fn new(with_mods: bool, log: bool) -> MemWatch {
        MemWatch { budget: Budget::new(budget::current_limit(with_mods)), ladder: Ladder::new(), log, with_mods, since_measure: MEASURE_EVERY, since_log: 0.0, since_trim: 0.0, audio_bytes: 0, trims: 0, steps_taken: 0 }
    }

    /// Set the limit for a game with mods (or without).
    pub fn set_mods(&mut self, with_mods: bool) {
        self.with_mods = with_mods;
        self.budget.limit = budget::current_limit(with_mods);
    }

    /// What to use now, given what the settings ask for.
    pub fn effective(&self, w: &Wanted) -> Effective {
        self.ladder.effective(w)
    }

    /// Count what is held. Cheap, but not free: once a second is enough.
    pub fn measure(&mut self, r: &Renderer, g: Option<&Session>, audio: &Audio) {
        let b = &mut self.budget;
        // HD textures and skins the player chose raise the limit by what they cost
        // (nothing changes without them: the allowance is 0).
        b.limit = budget::current_limit(self.with_mods);
        self.audio_bytes = audio.clip_bytes();
        b.set(Cat::Audio, self.audio_bytes);
        b.set(Cat::Meshes, r.vbo_bytes);
        b.set(Cat::Textures, ATLAS_BYTES + UI_TEXTURE_BYTES + r.model_tex_bytes + r.hd_bytes);
        b.set(Cat::Models, r.model_bytes());
        b.set(Cat::Other, BASE_BYTES);
        match g {
            Some(g) => {
                b.set(Cat::Chunks, g.world.chunk_bytes());
                b.set(Cat::Entities, g.mobs.list.len() * MOB_BYTES + g.drops.len() * 128 + g.shots.len() * 128 + g.primed.sparks.len() * 64 + crate::game::fx::state_ref(g).map_or(0, |s| s.falling.len() * 64 + s.quads.list.len() * 256));
            }
            None => {
                b.set(Cat::Chunks, 0);
                b.set(Cat::Entities, 0);
            }
        }
    }

    /// Call every frame. Counts now and then, and when over the limit gives
    /// something up (see the ladder). Returns whether what the game should use
    /// changed.
    pub fn tick(&mut self, dt: f32, r: &Renderer, g: Option<&Session>, audio: &mut Audio, wanted: &Wanted) -> bool {
        self.since_measure += dt;
        self.since_log += dt;
        self.since_trim += dt;
        if self.since_measure < MEASURE_EVERY {
            return false;
        }
        let elapsed = self.since_measure;
        self.since_measure = 0.0;
        self.measure(r, g, audio);
        if self.log && self.since_log >= LOG_EVERY {
            self.since_log = 0.0;
            logf!("voxelrust: {}, held back: {}, trims {}, steps {}", self.budget.line(), self.ladder.describe(), self.trims, self.steps_taken);
        }
        // Memory that was freed but that the allocator still holds goes back now and then.
        if self.since_trim >= TRIM_EVERY {
            self.since_trim = 0.0;
            // Long sounds not played for two minutes are let go, over the limit or not.
            audio.trim_clips(120);
            crate::sys::heap::trim();
        }
        // With nothing to look at (a menu, no world) nothing is given up.
        let a = self.ladder.update(elapsed, self.budget.total(), self.budget.limit, self.audio_bytes, wanted);
        match a {
            Action::Nothing => false,
            Action::TrimAudio => {
                let freed = audio.trim_clips(0);
                crate::sys::heap::trim();
                self.trims += 1;
                if self.log {
                    logf!("voxelrust: memory over the limit ({} MB): freed {} KB of decoded sound", self.budget.total() >> 20, freed >> 10);
                }
                false
            }
            Action::Changed => {
                self.steps_taken += 1;
                if self.log {
                    let e = self.ladder.effective(wanted);
                    logf!("voxelrust: memory {} MB of {}: now view {} far {} lod {}/{}/{} (held back: {})", self.budget.total() >> 20, self.budget.limit >> 20, e.distance, e.far, e.lod_real, e.lod_trees, e.lod_detail, self.ladder.describe());
                }
                true
            }
        }
    }

    /// Two lines for the F3 screen, and a third about the HD atlas when there is one.
    pub fn lines(&self, r: &Renderer) -> alloc::vec::Vec<String> {
        let b = &self.budget;
        let mb = |c: Cat| b.get(c) >> 20;
        let mut out = alloc::vec![
            alloc::format!("MEMORY {} OF {} MB ({}%)   HELD BACK: {}", b.total() >> 20, b.limit >> 20, b.percent(), self.ladder.describe()).to_ascii_uppercase(),
            alloc::format!(
                "CHUNKS {}  MESHES {}  TEXTURES {}  AUDIO {}  MODELS {}  ENTITIES {} MB",
                mb(Cat::Chunks),
                mb(Cat::Meshes),
                mb(Cat::Textures),
                mb(Cat::Audio),
                mb(Cat::Models),
                mb(Cat::Entities)
            ),
        ];
        let (pages, tiles, bytes, cap) = r.hd_summary();
        let allowed = budget::allowance();
        if pages > 0 || allowed > 0 {
            out.push(
                alloc::format!(
                    "HD ATLAS {} MB IN {} PAGES, {} TILES, TEXTURE SIZE {}   ALLOWANCE +{} MB",
                    bytes >> 20,
                    pages,
                    tiles,
                    cap,
                    allowed.min(budget::HARD_CEILING) >> 20
                )
                .to_ascii_uppercase(),
            );
        }
        out
    }
}
