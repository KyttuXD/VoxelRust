//! Code that mods bring with them.
//!
//! A pack's text can only say so much. A mod that needs more (a new kind
//! of terrain, a machine with its own physics) carries Rust of its own in
//! `<mods folder>/<name>/code/mod.rs` (see `build.rs`). The build finds every such folder and
//! compiles it in as a module of the game (only when built with the
//! `native-mods` feature: a plain build has no mod code at all); the game itself names none of
//! them. Each one offers a [`Native`]: a table of hooks the game calls at
//! fixed points. A hook that is `None` is simply not called.
//!
//! A mod's code runs only while a loaded pack says `feature <name>`, so
//! switching the mod off in Settings > MODS switches its code off too.

use crate::audio::Audio;
use crate::client::modui::binds::{KeyDecl, SwitchDecl};
use crate::client::modui::screens::UiEvent;
use crate::client::modui::ModApi;
use crate::explosion::ExplosionInfo;
use crate::block::Block;
use crate::game::hooks::HudSink;
use crate::render::Renderer;
use crate::session::{Controls, Session};
use crate::settings::Settings;
use crate::ui::Ui;
use alloc::string::String;
use alloc::vec::Vec;

/// Sound and music for a mod's code: handles, positional and looping sounds,
/// one channel of streamed music. Call these from a hook with the session
/// (`sound::play(g, "<mod>", "<sound>", volume, pitch, pan, None, false)` returns
/// a handle for `sound::stop`, `fade`, `set` and `is_playing`; `sound::play_music`
/// and its fellows). The limits are the scripts' (see docs/SOUND.md).
pub use crate::game::mod_sound as sound;

/// The version of the hook interface in this file. It goes up whenever a hook
/// is removed or its signature changes, never for a new hook; a [`Native`]
/// says in `api` which version it was written for. See "Stability" in
/// docs/MODDING.md.
pub const API_VERSION: u32 = 1;

/// Whether code written for interface version `api` can run in this build
/// (0 means "not said": any). Code written for a newer interface is left off,
/// since its hooks may mean something else here.
pub fn api_ok(api: u32) -> bool {
    api <= API_VERSION
}

/// What the game hands the drawing hooks.
pub struct View<'a> {
    /// Camera position.
    pub eye: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    /// Whether the player is seen from outside.
    pub third_person: bool,
    /// Brightness (0..1) at a place in the world.
    pub light: &'a dyn Fn([f64; 3]) -> f32,
}

/// A mod's own folder for what it keeps for good (its saves and settings):
/// `<data folder>/mods-data/<mod>/`. Made if it is missing. Mods keep nothing
/// loose in the data folder itself.
pub fn mod_data_dir(base: &str, name: &str) -> String {
    let dir = alloc::format!("{}mods-data/{}/", base, name);
    crate::sys::make_dirs(&dir);
    dir
}

/// A mod's folder for what can be fetched or made again (caches, downloads):
/// `<data folder>/tmp/<mod>/`. Made if it is missing.
pub fn mod_tmp_dir(base: &str, name: &str) -> String {
    let dir = alloc::format!("{}tmp/{}/", base, name);
    crate::sys::make_dirs(&dir);
    dir
}

pub struct Native {
    /// The `feature` name that switches this code on.
    pub name: &'static str,
    /// The [`API_VERSION`] this code was written for (0: not said).
    pub api: u32,
    /// Top-level pack lines kept word for word for this code to read
    /// (`Registry::lines`).
    pub lines: &'static [&'static str],
    /// Indented pack sections kept for it (`Registry::sections`).
    pub sections: &'static [&'static str],
    /// Lines for `--help`.
    pub help: &'static str,
    /// Files it keeps per world, as suffixes after `<world>.`.
    pub files: &'static [&'static str],

    // ---- start-up
    /// The game has started: where its data is, and the settings.
    pub start: Option<fn(base: &str, settings: &Settings)>,
    /// A command-line option the game does not know. `Some(Ok)` if it was
    /// this code's (taking what it needs from `rest`), `Some(Err)` to stop
    /// with a message.
    pub option: Option<fn(name: &str, rest: &mut dyn Iterator<Item = String>) -> Option<Result<(), String>>>,
    /// The kind of world the options asked for, if they did.
    pub option_kind: Option<fn() -> Option<String>>,
    /// A scripted run may ask for one of this code's screens by name.
    pub show: Option<fn(what: &str, g: &mut Session) -> bool>,

    // ---- content
    /// The content was (re)loaded; say what is wrong with the lines and
    /// sections meant for this code.
    pub check: Option<fn(r: &crate::block::Registry) -> Vec<String>>,

    // ---- terrain
    /// Whether a world-kind text is one of this code's, tidied (`None` if
    /// it is, but makes no sense).
    pub terrain_parse: Option<fn(text: &str) -> Option<Option<String>>>,
    /// Open that kind of world: from now on the game asks the source.
    pub terrain_open: Option<fn(text: &str) -> Option<crate::worldgen::Source>>,
    /// The name of its terrain choice on the new-world screen, if it has
    /// one to offer just now.
    pub terrain_label: Option<fn() -> Option<&'static str>>,
    /// Draw that choice's own options under the terrain button; returns
    /// how many rows it used.
    pub terrain_form: Option<fn(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32, step: f32) -> f32>,
    /// The world kind the form now describes.
    pub terrain_chosen: Option<fn() -> String>,
    /// A dimension other than the world's own is being entered: the land of
    /// its own, if this code has one for that name. `None` leaves the
    /// dimension to the game (its declared kind) and to the world's source.
    pub dimension_terrain: Option<fn(id: &str) -> Option<crate::worldgen::Source>>,

    // ---- a world being played
    /// A world (or another dimension of it) was opened: load what this
    /// code keeps for it.
    pub open: Option<fn(g: &mut Session)>,
    /// Save what it keeps.
    pub save: Option<fn(g: &mut Session)>,
    /// The content is about to be reloaded (`before`), or just was: what
    /// is kept by number must be carried across by name.
    pub reload: Option<fn(g: &mut Session, before: bool)>,
    /// Once a frame, before the player moves.
    pub update: Option<fn(g: &mut Session, dt: f32, c: &Controls, audio: &mut Audio)>,
    /// The player's hands: called before the game's own handling of
    /// clicks. `reach` is how far off the block being looked at is.
    /// Returns whether the click was used up.
    pub interact: Option<fn(g: &mut Session, c: &Controls, reach: f32, audio: &mut Audio) -> bool>,
    /// A chat command the game does not know.
    pub command: Option<fn(g: &mut Session, words: &[&str]) -> Option<String>>,
    /// An `/if` condition the game does not know.
    pub condition: Option<fn(g: &mut Session, words: &[&str]) -> Option<bool>>,
    /// The body model a player is drawn with (`player`, `player_slim` or
    /// another model id the mod brings), for the local player (`player` equal
    /// to our own id, 0 when not in a shared game) or a remote one. `None`
    /// keeps the game's choice. Asked for every drawn player, so keep it cheap.
    pub body: Option<fn(g: &Session, player: u16) -> Option<String>>,
    /// The player killed a creature: its kind (an index into the registry's
    /// entities) and where it was. Not called for creatures that die some
    /// other way.
    pub killed: Option<fn(g: &mut Session, kind: usize, at: [f64; 3])>,
    /// The player hit a creature (and may have killed it): its stable id, its
    /// kind and where it was.
    pub hit: Option<fn(g: &mut Session, id: u32, kind: usize, at: [f64; 3])>,
    /// The player is about to be hurt by `amount`. `attacker` is the kind of
    /// creature that bit them, when a creature did. Returns the amount to
    /// apply (armour makes it smaller; 0 cancels it). Called in survival
    /// only, and only for damage above zero.
    pub hurt: Option<fn(g: &mut Session, amount: i32, attacker: Option<usize>) -> i32>,
    /// An explosion is about to happen (TNT, an exploding creature, `/blast`,
    /// another mod): its middle, its `radius` (0 to 16 blocks) and its cause
    /// (`tnt`, `creature`, `script`, `player`, `fire`, `other`). Return the
    /// radius to go ahead with (a different one changes the blast; 0 or more
    /// than 16 is held to 0..16, and 0 cancels) or `None` to cancel it.
    pub before_explode: Option<fn(g: &mut Session, x: f64, y: f64, z: f64, radius: f32, cause: &str) -> Option<f32>>,
    /// Blocks no explosion destroys, by block id (any radius, every path:
    /// TNT, creatures, `/blast`, scripts). Skipped without calling anything.
    pub immune_blocks: &'static [&'static str],
    /// A block an explosion is about to destroy: return false to spare it.
    /// Asked for the first 20 000 blocks of a blast (see `explosion_hooks`).
    pub explode_block: Option<fn(g: &mut Session, cell: [i32; 3], block: Block) -> bool>,
    /// An explosion has happened; `destroyed` is how many blocks it took.
    pub after_explode: Option<fn(g: &mut Session, info: &ExplosionInfo, destroyed: usize)>,
    /// A block of TNT at that cell is about to be lit: return false to stop it.
    pub on_prime: Option<fn(g: &mut Session, cell: [i32; 3]) -> bool>,
    /// An effect (poison, burning...) is about to be given to the player:
    /// its id, seconds and level (both may be changed). Return false to
    /// refuse it. Not asked when it would change nothing.
    pub effect_apply: Option<fn(g: &mut Session, id: &str, secs: &mut f32, level: &mut i32) -> bool>,
    /// Every frame an effect of the player's is running: its id, seconds
    /// left, level, and the time that passed.
    pub effect_tick: Option<fn(g: &mut Session, id: &str, secs_left: f32, level: i32, dt: f32)>,
    /// An effect started on the player (it was not active before): its id
    /// and level. Not called again when it is only made longer or stronger.
    pub effect_start: Option<fn(g: &mut Session, id: &str, level: i32)>,
    /// An effect of the player's ended: ran out, was put out by water or
    /// was cleared (death included).
    pub effect_end: Option<fn(g: &mut Session, id: &str)>,
    /// A message another player's copy of this code sent with
    /// `Session::send_ext`.
    pub message: Option<fn(g: &mut Session, key: &str, text: &str)>,
    /// A message another player's copy of this code sent with
    /// `Session::send_mod_message`: its channel, the sender's player name and
    /// the text (up to 512 bytes). See net/modmsg.rs.
    pub on_message: Option<fn(g: &mut Session, channel: &str, from: &str, text: &str)>,
    /// About three times a second in a shared game: send what others need.
    pub share: Option<fn(g: &mut Session)>,
    /// The chest screen is opening for `Session::opened_ext`: the storage it
    /// should show (it stays this code's; the pointer is used at once).
    pub storage: Option<fn(g: &mut Session, which: usize) -> Option<*mut crate::inventory::Inventory>>,

    // ---- events (see game/hooks.rs and "Hooks reference" in docs/MODDING.md)
    /// A block is about to be broken by the player. `false` keeps it.
    pub on_block_break: Option<fn(g: &mut Session, cell: [i32; 3], block: Block) -> bool>,
    /// A block is about to be placed by the player (`cell` is where it goes).
    /// `false` stops it.
    pub on_block_place: Option<fn(g: &mut Session, cell: [i32; 3], block: Block) -> bool>,
    /// A creature appeared (`kind` indexes the registry's entities).
    /// `false` takes it away again.
    pub on_entity_spawn: Option<fn(g: &mut Session, kind: usize) -> bool>,
    /// A creature died, from any cause.
    pub on_entity_death: Option<fn(g: &mut Session, kind: usize, at: [f64; 3])>,
    /// A chunk was just generated (not loaded from a save), on the main
    /// thread, a few chunks a frame: place structures with
    /// `g.world.set_block` in it and in its neighbours that are already
    /// loaded (at most 4096 writes are kept per call, see
    /// `script::ext::structures::MAX_WRITES`; `crate::script::ext::structures::chunk_hash`
    /// gives the same number for the same chunk every time). Only called
    /// for whoever runs the world.
    pub chunk_generated: Option<fn(g: &mut Session, cx: i32, cz: i32, seed: u32)>,
    /// A block was placed, by whatever cause: `player`, `script`, `remote`,
    /// `fall`, `fluid` or `other`. Called a frame later, not for air, and not
    /// for blocks a hook itself placed two hooks deep.
    pub on_block_placed: Option<fn(g: &mut Session, cell: [i32; 3], block: Block, cause: &str)>,
    /// A creature died: its stable id, kind and place, and who killed it (the
    /// player's name, a creature's id or a cause: `lava`, `script`, `other`).
    pub on_creature_death: Option<fn(g: &mut Session, id: u32, kind: usize, at: [f64; 3], killer: &str)>,
    /// A creature left the world: `killed`, `despawned`, `unloaded` or `scripted`.
    pub on_creature_removed: Option<fn(g: &mut Session, id: u32, kind: usize, reason: &str)>,
    /// The player right-clicked a creature holding `item` (if anything).
    /// `false` cancels the game's own handling of the click.
    pub on_creature_interact: Option<fn(g: &mut Session, id: u32, kind: usize, item: Option<usize>) -> bool>,
    /// A night creature would burn in daylight: `false` lets it live (it is
    /// asked again after twenty seconds).
    pub on_creature_sunburn: Option<fn(g: &mut Session, id: u32, kind: usize) -> bool>,
    /// The player is about to take damage in survival. The hook may change
    /// `amount`; `cause` is `fall`, `drown`, `hunger`, `block`, `arrow`,
    /// `command`, `other` or the id of the creature that bit. `false` (or an
    /// amount of 0) cancels it. Runs after `hurt`.
    pub on_player_hurt: Option<fn(g: &mut Session, amount: &mut i32, cause: &str) -> bool>,
    /// A worn armour piece was put on, taken off, set by code or broke:
    /// `slot` is 0 head, 1 chest, 2 legs, 3 feet. The code reads and sets
    /// what is worn through `g.inv.worn`, `Session::set_worn` and
    /// `Session::armor_points`, wears items with `game::gear::wear`, and
    /// keeps item data with `game::gear::item_data` (the same as the script
    /// calls `get_armor`, `set_armor`, `item_damage` and `item_set`).
    pub on_armor_change: Option<fn(g: &mut Session, slot: usize)>,
    /// The player typed a line (commands too). `false` swallows it.
    pub on_chat: Option<fn(g: &mut Session, line: &str) -> bool>,
    /// The player pressed the use button holding item `item` (once per
    /// press). `false` stops the use.
    pub on_item_use: Option<fn(g: &mut Session, item: usize) -> bool>,
    /// The player crafted item `out`.
    pub on_craft: Option<fn(g: &mut Session, out: usize)>,
    /// The player is about to pick up `count` of item `item` lying on the
    /// ground. `false` leaves it there (it is offered again a second later).
    pub on_item_pickup: Option<fn(g: &mut Session, item: usize, count: u16) -> bool>,
    /// Once a frame, after the game's own timers (cheaper to leave `None`
    /// than to return early).
    pub on_tick: Option<fn(g: &mut Session, dt: f32)>,
    /// Draw on the heads-up display while playing.
    pub on_draw_hud: Option<fn(g: &Session, hud: &mut dyn HudSink)>,
    /// Every script event, as `when` lines hear it (`name` is the canonical
    /// one). Events raised from inside this hook are not passed back.
    pub on_event: Option<fn(g: &mut Session, name: &str, what: &str)>,
    /// A world was opened (once; `open` is also called for dimensions).
    pub on_world_load: Option<fn(g: &mut Session)>,
    /// The world was saved.
    pub on_world_save: Option<fn(g: &mut Session)>,

    /// A few words on what this code is keeping, for the log.
    pub status: Option<fn(g: &Session) -> String>,

    // ---- what is seen
    /// Draw into the world.
    pub draw: Option<fn(g: &Session, r: &mut Renderer, view: &View)>,
    /// The key that opens the map was pressed: open this code's screen
    /// instead, if it has one for this world.
    pub map_open: Option<fn(g: &mut Session) -> bool>,
    /// Draw that screen. Returns false when it wants closing.
    pub map_screen: Option<fn(ui: &mut Ui, g: &mut Session) -> bool>,
    /// Rows for Settings > GAME; returns (rows used, whether anything changed).
    pub settings: Option<fn(ui: &mut Ui, st: &mut Settings, x: f32, y: f32, w: f32, h: f32, step: f32) -> (f32, bool)>,

    // ---- controls, HUD and screens (see client/modui and "Controls, HUD and screens" in docs/MODDING.md)
    /// Key bindings this code listens to: they are listed in Settings >
    /// Controls > Key bindings under the mod's name and can be rebound.
    pub bindings: &'static [KeyDecl],
    /// Rows this code adds to Settings > Game > Mod switches and reload
    /// (kept as `flag_<mod>.<id>`; read with `modui::keys::switch_on`).
    pub switches: &'static [SwitchDecl],
    /// A key binding of this code went down or up (`down`), while playing.
    pub on_key: Option<fn(g: &mut Session, id: &str, down: bool)>,
    /// Once a frame while playing: draw on the HUD through the checked API
    /// (`api.hud_text(...)`, `api.hud_set(...)`, `api.toast(...)`...).
    pub on_hud: Option<fn(g: &Session, api: &mut ModApi)>,
    /// The player used a screen this code opened (`api.ui_open(...)`).
    pub on_ui: Option<fn(g: &mut Session, api: &mut ModApi, event: &UiEvent)>,

    // ---- a server
    /// A line a client sent that the server does not know: the bytes to
    /// answer with.
    pub serve: Option<fn(words: &[&str]) -> Option<Vec<u8>>>,
}

impl Native {
    /// Nothing at all: write `..Native::NONE` after the hooks you fill in.
    pub const NONE: Native = Native {
        name: "",
        api: 0,
        lines: &[],
        sections: &[],
        help: "",
        files: &[],
        start: None,
        option: None,
        option_kind: None,
        show: None,
        check: None,
        terrain_parse: None,
        terrain_open: None,
        terrain_label: None,
        terrain_form: None,
        terrain_chosen: None,
        dimension_terrain: None,
        open: None,
        save: None,
        reload: None,
        update: None,
        interact: None,
        command: None,
        condition: None,
        body: None,
        killed: None,
        hit: None,
        hurt: None,
        before_explode: None,
        immune_blocks: &[],
        explode_block: None,
        after_explode: None,
        on_prime: None,
        effect_apply: None,
        effect_tick: None,
        effect_start: None,
        effect_end: None,
        message: None,
        on_message: None,
        share: None,
        storage: None,
        on_block_break: None,
        on_block_place: None,
        on_entity_spawn: None,
        on_entity_death: None,
        chunk_generated: None,
        on_block_placed: None,
        on_creature_death: None,
        on_creature_removed: None,
        on_creature_interact: None,
        on_creature_sunburn: None,
        on_player_hurt: None,
        on_armor_change: None,
        on_chat: None,
        on_item_use: None,
        on_craft: None,
        on_item_pickup: None,
        on_tick: None,
        on_draw_hud: None,
        on_event: None,
        on_world_load: None,
        on_world_save: None,
        status: None,
        draw: None,
        map_open: None,
        map_screen: None,
        settings: None,
        bindings: &[],
        switches: &[],
        on_key: None,
        on_hud: None,
        on_ui: None,
        serve: None,
    };
}

/// The body model id for `player`, from the first mod that picks one; `None`
/// if none does (the game then uses `player` or `player_slim`).
///
/// An id the registry does not know (a model its mod did not bring, or one
/// whose pack is off) is refused here, so the caller falls back to the game's
/// own body rather than drawing nothing.
pub fn body_of(g: &Session, player: u16) -> Option<String> {
    let id = active().filter_map(|(_, n)| n.body).find_map(|body| body(g, player))?;
    crate::content::block::reg().model(&id).is_some().then_some(id)
}

/// Every mod's code that was compiled in, switched on or not.
pub fn all() -> &'static [&'static Native] {
    crate::mods::NATIVES
}

/// The mods that come with the game, as (path, bytes). For tests and tools:
/// the game itself reads its mods through the functions in this module.
#[cfg(test)]
pub fn shipped() -> &'static [(&'static str, &'static [u8])] {
    crate::mods::SHIPPED
}

/// For a test that needs the content of the mods (creatures, colours, farming,
/// tools of more metals...): says so and returns true when this build has no
/// shipped mods, so the test can return at once. The core alone has none of it.
#[cfg(test)]
pub fn skip_without_mods() -> bool {
    let none = shipped().is_empty();
    if none {
        std::eprintln!("SKIPPED: needs the mods (build with --features native-mods and VOXELRUST_MODS_DIR)");
    }
    none
}

/// Outside the tests there are no shipped mods to hand out.
#[cfg(not(test))]
pub fn shipped() -> &'static [(&'static str, &'static [u8])] {
    &[]
}

/// Whether a path is inside a mods folder (the program's or the game's own).
pub fn in_mods_folder(path: &str) -> bool {
    path.contains("/mods/") || path.starts_with("mods/")
}

/// The code whose mods are switched on, with each one's place in [`all`]
/// (its channel number in a shared game).
pub fn active() -> impl Iterator<Item = (usize, &'static Native)> {
    let r = crate::block::reg();
    all().iter().copied().enumerate().filter(move |(_, n)| api_ok(n.api) && r.has(n.name))
}

/// Whether a top-level pack line belongs to some mod's code.
pub fn reads_line(key: &str) -> bool {
    all().iter().any(|n| n.lines.contains(&key))
}

/// Whether an indented pack section belongs to some mod's code.
pub fn reads_section(key: &str) -> bool {
    all().iter().any(|n| n.sections.contains(&key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_for_a_newer_interface_is_left_off() {
        assert!(api_ok(0));
        assert!(api_ok(API_VERSION));
        assert!(!api_ok(API_VERSION + 1));
    }
}
