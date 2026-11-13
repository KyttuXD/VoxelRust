//! Code that mods bring with them.
//!
//! A pack's text can only say so much. A mod that needs more (a new kind
//! of terrain, a machine with its own physics) carries Rust of its own in
//! `mods/<name>/code/mod.rs`. The build finds every such folder and
//! compiles it in as a module of the game; the game itself names none of
//! them. Each one offers a [`Native`]: a table of hooks the game calls at
//! fixed points. A hook that is `None` is simply not called.
//!
//! A mod's code runs only while a loaded pack says `feature <name>`, so
//! switching the mod off in Settings > MODS switches its code off too.

use crate::audio::Audio;
use crate::explosion::ExplosionInfo;
use crate::render::Renderer;
use crate::session::{Controls, Session};
use crate::settings::Settings;
use crate::ui::Ui;
use alloc::string::String;
use alloc::vec::Vec;

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

pub struct Native {
    /// The `feature` name that switches this code on.
    pub name: &'static str,
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
    /// another mod): change it (`at`, `power`) or return false to cancel it.
    pub before_explode: Option<fn(g: &mut Session, info: &mut ExplosionInfo) -> bool>,
    /// An explosion has happened; `destroyed` is how many blocks it took.
    pub after_explode: Option<fn(g: &mut Session, info: &ExplosionInfo, destroyed: usize)>,
    /// A block of TNT at that cell is about to be lit: return false to stop it.
    pub on_prime: Option<fn(g: &mut Session, cell: [i32; 3]) -> bool>,
    /// A message another player's copy of this code sent with
    /// `Session::send_ext`.
    pub message: Option<fn(g: &mut Session, key: &str, text: &str)>,
    /// About three times a second in a shared game: send what others need.
    pub share: Option<fn(g: &mut Session)>,
    /// The chest screen is opening for `Session::opened_ext`: the storage it
    /// should show (it stays this code's; the pointer is used at once).
    pub storage: Option<fn(g: &mut Session, which: usize) -> Option<*mut crate::inventory::Inventory>>,

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

    // ---- a server
    /// A line a client sent that the server does not know: the bytes to
    /// answer with.
    pub serve: Option<fn(words: &[&str]) -> Option<Vec<u8>>>,
}

impl Native {
    /// Nothing at all: write `..Native::NONE` after the hooks you fill in.
    pub const NONE: Native = Native {
        name: "",
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
        killed: None,
        hit: None,
        hurt: None,
        before_explode: None,
        after_explode: None,
        on_prime: None,
        message: None,
        share: None,
        storage: None,
        status: None,
        draw: None,
        map_open: None,
        map_screen: None,
        settings: None,
        serve: None,
    };
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
    all().iter().copied().enumerate().filter(move |(_, n)| r.has(n.name))
}

/// Whether a top-level pack line belongs to some mod's code.
pub fn reads_line(key: &str) -> bool {
    all().iter().any(|n| n.lines.contains(&key))
}

/// Whether an indented pack section belongs to some mod's code.
pub fn reads_section(key: &str) -> bool {
    all().iter().any(|n| n.sections.contains(&key))
}
