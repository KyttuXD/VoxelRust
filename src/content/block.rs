//! The content registry: textures, blocks and items, all read from data.
//!
//! Nothing about a particular block is written in Rust. `data/base.txt` is
//! built into the game and declares the standard content; pack files are
//! parsed after it and may add or redefine anything. See docs/DATA.md.
//!
//! A block that can face several ways (stairs) or sit upside-down (slabs)
//! becomes several *states*. Each state has its own number, and the world
//! stores state numbers, so a rotated block costs nothing extra per block.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, Ordering};

/// A block state number, as stored in the world.
pub type Block = u16;
pub const AIR: Block = 0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Air,
    /// A full, opaque cube.
    Cube,
    /// A full cube whose texture has holes (leaves, glass).
    Cutout,
    /// Two crossed quads (flowers, grass tufts). No collision.
    Cross,
    Liquid,
    /// A full, solid cube you can see through (stained glass, ice).
    Translucent,
    /// One or more boxes smaller than the cell (stairs, slabs, posts).
    Boxes,
}

/// An axis-aligned box inside a block cell, in 1/16ths of a block.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bx {
    pub min: [u8; 3],
    pub max: [u8; 3],
}

impl Bx {
    pub const FULL: Bx = Bx { min: [0, 0, 0], max: [16, 16, 16] };

    /// Quarter turn about the vertical axis: (x, z) -> (z, 16 - x).
    fn turned(self) -> Bx {
        Bx {
            min: [self.min[2], self.min[1], 16 - self.max[0]],
            max: [self.max[2], self.max[1], 16 - self.min[0]],
        }
    }

    /// Quarter-turned `n` times.
    pub fn turned_by(self, n: u8) -> Bx {
        let mut b = self;
        for _ in 0..n % 4 {
            b = b.turned();
        }
        b
    }

    fn flipped(self) -> Bx {
        Bx {
            min: [self.min[0], 16 - self.max[1], self.min[2]],
            max: [self.max[0], 16 - self.min[1], self.max[2]],
        }
    }
}

#[derive(Clone, Debug)]
pub struct BlockDef {
    /// The id from the data file, shared by all states of the block.
    pub id: String,
    pub name: String,
    pub kind: Kind,
    /// Texture tiles: top, side, bottom.
    pub tex: [u16; 3],
    /// Light given off, 0..=15.
    pub emit: u8,
    /// Sunlight lost passing straight down through the block.
    pub sky_loss: u8,
    pub solid: bool,
    pub unbreakable: bool,
    pub hidden: bool,
    pub boxes: Vec<Bx>,
    /// For fences and walls: the boxes added toward a connected neighbour,
    /// described for the north (-z) side. Empty for everything else.
    pub arms: Vec<Bx>,
    /// Blocks with the same non-zero group connect to each other.
    pub connect_group: u8,
    /// Whether it also reaches toward full cubes (fences and walls do).
    pub join_cubes: bool,
    /// Seconds to break by hand in survival mode.
    pub hardness: f32,
    /// Item given when broken in survival (an index into the items), if any.
    pub drop: Option<usize>,
    /// Percent chance that the drop is given (100 = always).
    pub drop_chance: u8,
    /// Which kind of tool digs it faster: 0 none, 1 pick, 2 axe, 3 shovel.
    pub tool: u8,
    /// What right-clicking it does.
    pub usable: Usable,
    /// First state of this block, and how its states are laid out:
    /// `first + flip * rotations + rotation`.
    pub first: Block,
    pub rotations: u8,
    pub flips: bool,
    /// Behaviour beyond shape and material.
    pub x: BlockExtra,
}

/// How a sound is made: a burst of filtered noise and a sweeping tone.
#[derive(Clone, Debug)]
pub struct SoundDef {
    pub id: String,
    /// Which volume slider it follows: 0 effects, 1 footsteps, 2 interface.
    pub bus: u8,
    pub seconds: f32,
    /// Share of noise against tone, 0..1.
    pub noise: f32,
    /// The tone glides from the first pitch to the second.
    pub hz: [f32; 2],
    /// 1 leaves the noise bright; small values make it a dull rumble.
    pub low_pass: f32,
    pub loudness: f32,
    /// What to show for it when subtitles are on (empty = nothing).
    pub caption: String,
    /// A recorded sound (read from a sound file) instead of a made one:
    /// mono, 16-bit, at the mixer's rate.
    pub clip: Option<Vec<i16>>,
}

/// Something a pack makes happen: when `event` occurs (to `what`, if
/// given), the chat commands in `actions` are carried out.
#[derive(Clone, Debug)]
pub struct Script {
    pub event: String,
    pub what: String,
    pub actions: Vec<String>,
}

/// A pack's episode (a storm of frogs, a quake, a festival): something
/// that comes over the world for a while. When it may come, how long it lasts, and the chat
/// commands carried out a few times a second while it does.
#[derive(Clone, Debug)]
pub struct EpisodeDef {
    pub id: String,
    /// It starts about once in this many seconds (0 = never by itself).
    pub every: u32,
    pub lasts: f32,
    /// `/if` conditions that must all hold for it to start.
    pub needs: Vec<String>,
    /// Blocks a second it travels (0 = it is everywhere at once).
    pub moves: f32,
    /// Drawn as a funnel where it is.
    pub funnel: bool,
    /// The episode that follows it, if that one's conditions hold.
    pub then: String,
    pub actions: Vec<String>,
}

fn split_actions(text: &str) -> Vec<String> {
    text.split(';').map(|c| String::from(c.trim())).filter(|c| !c.is_empty()).collect()
}

/// A pack's ore: found in stone between two heights, about one block in N.
#[derive(Clone, Debug)]
pub struct OreRule {
    pub block: Block,
    pub low: i32,
    pub high: i32,
    pub one_in: u32,
}

/// A pack's wild plant: on about one grass block in N, in some biomes
/// (lower-case names; none = everywhere).
#[derive(Clone, Debug)]
pub struct PlantRule {
    pub block: Block,
    pub one_in: u32,
    pub biomes: Vec<String>,
}

/// What a block does besides sitting there. All of it comes from the data
/// files, so packs can make their own doors, crops and falling blocks.
#[derive(Clone, Debug, Default)]
pub struct BlockExtra {
    /// Drops when nothing holds it up (sand, gravel).
    pub falls: bool,
    /// Right click swaps it for this block (a door and its open form).
    pub toggle: Option<Block>,
    /// Two blocks high: placed, broken and toggled as a pair.
    pub tall: bool,
    /// Turns into this block after about so many seconds under open sky.
    pub grows: Option<(Block, f32)>,
    /// May only stand on these blocks; pops off when they go.
    pub on: Vec<Block>,
    /// A second thing given when broken: item, percent chance, count.
    pub also: Option<(usize, u8, u16)>,
    /// Hurts whoever stands in or on it, per second.
    pub hurts: u8,
    /// A liquid: (lava rather than water, level 1-8 where 8 is the source).
    pub fluid: Option<(bool, u8)>,
    /// Slows whoever walks through it (percent of normal speed, 0 = no change).
    pub slow: u8,
    /// The sound of breaking or placing it, if not the usual ones.
    pub sound: String,
    /// What sitting on it lifts the player to, in sixteenths (0 = not a seat).
    pub seat: u8,
    /// Food: right click eats it where it stands, for this many points.
    pub eat: u8,
    /// Marks for the mods' own code to look for (`tag <name> [number]`):
    /// what a mark means is up to whichever code reads it.
    pub tags: Vec<(String, f32)>,
    /// Drawn as a model (a prop) instead of as boxes: the model, and how
    /// tall to draw it in blocks. Its `box` lines still say where it is solid.
    /// The third number lifts it off the floor (a lamp hung from above).
    pub prop: Option<(usize, f32, f32)>,
    /// A big prop: how many blocks wide (square) and tall it stands. The
    /// extra cells are taken by hidden filler blocks. (0, 0) = one cell.
    pub fills: (u8, u8),
    /// The hidden filler itself: solid, never drawn, part of a big prop.
    pub part: bool,
    /// Two blocks long: the other half stands one block ahead of this one
    /// (`true`) or one behind it (`false`), the way it faces.
    pub long: Option<(bool, Block)>,
    /// `react <next to> <becomes>`: turns into the second block when the first
    /// is next to it.
    pub reacts: Vec<(Block, Block)>,
}

impl BlockExtra {
    /// The number after `tag <name>` (1 if none was given), if the block
    /// has that mark.
    pub fn tag(&self, name: &str) -> Option<f32> {
        self.tags.iter().find(|t| t.0 == name).map(|t| t.1)
    }
}

/// What an item does besides being placed or eaten.
#[derive(Clone, Debug, Default)]
pub struct ItemExtra {
    /// Right click turns the first block into the second (a hoe on dirt).
    pub turns: Vec<(Block, Block)>,
    /// Damage dealt to creatures (0 = a bare hand's).
    pub damage: i32,
    /// A bow: damage, speed, seconds between shots, and the item each
    /// shot uses up (none = needs no ammunition).
    pub shoots: Option<(i32, f32, f32, Option<usize>)>,
    /// A spawn egg: using it brings out one of this creature.
    pub egg: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct ItemDef {
    pub id: String,
    pub name: String,
    pub icon: u16,
    /// First state of the block this item places, if any.
    pub place: Option<Block>,
    /// Radius of the sphere an action affects; 0 is a single block.
    pub brush: u8,
    pub reach: f32,
    /// How many fit in one inventory slot.
    pub stack: u16,
    /// Tool kind (see [`TOOLS`]) and how many times faster it digs.
    pub tool: u8,
    pub speed: f32,
    /// Health and hunger restored by eating it (0 = not food).
    pub food: i32,
    /// The pack section (see [`Section`]) this item stands for, if it is
    /// one: what it does in the hand is up to the mod's code.
    pub section: Option<usize>,
    /// Tab in the creative item list.
    pub category: String,
    pub x: ItemExtra,
}

/// How a creature behaves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Behavior {
    /// Strolls about.
    Wander,
    /// A friendly character: strolls, and walks with a player who is near.
    Npc,
    /// Hunts players.
    Hostile,
    /// Lives in water and stays there.
    Swim,
    /// Flutters about above the ground (birds, bats, butterflies).
    Fly,
    /// A person with a home and a job.
    Villager,
}

/// Ways of behaving that any creature may have on top of its behaviour.
pub mod traits {
    /// Runs from players who come close.
    pub const FLEES: u32 = 1;
    /// Keeps near others of its kind.
    pub const HERD: u32 = 2;
    /// Moves in hops.
    pub const JUMPS: u32 = 4;
    /// Leaves players alone unless they come close or hit it.
    pub const TERRITORIAL: u32 = 8;
    /// Chases and kills creatures marked as prey.
    pub const HUNTER: u32 = 16;
    pub const PREY: u32 = 32;
    /// Spends most of its time standing or lying about.
    pub const LAZY: u32 = 64;
    /// Hits back, with its whole pack, when hit.
    pub const PACK: u32 = 128;
    /// Tags along behind a player holding its favourite food.
    pub const FOLLOWS: u32 = 256;
    pub const NAMES: [(&str, u32); 9] =
        [("flees", FLEES), ("herd", HERD), ("jumps", JUMPS), ("territorial", TERRITORIAL), ("hunter", HUNTER), ("prey", PREY), ("lazy", LAZY), ("pack", PACK), ("follows", FOLLOWS)];
}

/// How a creature's model moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Anim {
    #[default]
    Walk,
    Hop,
    Slither,
    Flap,
    Scuttle,
    Waddle,
    Float,
}

pub const ANIMS: [(&str, Anim); 7] =
    [("walk", Anim::Walk), ("hop", Anim::Hop), ("slither", Anim::Slither), ("flap", Anim::Flap), ("scuttle", Anim::Scuttle), ("waddle", Anim::Waddle), ("float", Anim::Float)];

/// One look a creature may have: its own model and colour.
#[derive(Clone, Debug)]
pub struct Variant {
    pub name: String,
    pub model: usize,
    pub tint: [u8; 3],
    /// Scale relative to the entity's height (a pony next to a horse).
    pub size: f32,
    /// Its picture (`data/textures/entity/<tex>.png`, see `pack_entities`);
    /// empty = the model's own.
    pub tex: String,
    /// Surface or cave biomes this look is for (camouflage). With any, it is
    /// only given to creatures that appear in one of them, and always to
    /// those; empty = a look for anywhere. See `entity_variant`.
    pub biomes: Vec<String>,
}

/// The parts of a creature that only some have.
#[derive(Clone, Debug, Default)]
pub struct EntityExtra {
    pub traits: u32,
    /// The picture it is painted with, or empty for its model's own.
    pub tex: String,
    pub anim: Anim,
    /// Other looks; one is picked for each creature. Empty = just the one.
    pub variants: Vec<Variant>,
    /// Young: size relative to an adult, and seconds until grown.
    pub baby: Option<(f32, f32)>,
    /// Right click collects this item, then again after so many seconds.
    pub gives: Option<(usize, f32)>,
    /// Swaps: (item wanted, count, item given, count).
    pub trades: Vec<(usize, u16, usize, u16)>,
    /// What a villager does all day: `farmer`, `builder`, `guard`...
    pub job: String,
    /// Lives underground, in caves of these kinds.
    pub cave: bool,
    /// The food it follows (with the `follows` trait).
    pub likes: Option<usize>,
    /// Light it gives off, for glow in the dark (0..=15).
    pub glow: u8,
    /// The call it makes now and then (a sound's name).
    pub sound: String,
    /// Feeding it what it likes makes it yours.
    pub tamable: bool,
    /// What it shoots at its quarry, and how hard that hits (0 = nothing).
    pub shoots: i32,
    /// Players may ride it (once tamed, if it is the taming kind).
    pub rideable: bool,
    /// The creatures it hunts, by id: its place in the food chain. (With
    /// none named, a `hunter` goes after anything marked `prey`.)
    pub hunts: Vec<String>,
    /// `explodes <power> <fuse seconds> <trigger range>`: it closes in on a
    /// player, and when within range for the whole fuse it blows up (see
    /// `entity_explode`).
    pub explodes: Option<(f32, f32, f32)>,
}

/// A part of a pack kept as written, for a mod's own code to read: a
/// top-level word that code has claimed (`native::Native::sections`), the
/// id after it, and the indented lines under it.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub kind: String,
    pub id: String,
    /// Where it was written, for messages.
    pub at: String,
    pub props: Vec<(String, Vec<String>)>,
}

impl Section {
    /// The words after the first line called `key`.
    pub fn get(&self, key: &str) -> Option<&[String]> {
        self.props.iter().rev().find(|p| p.0 == key).map(|p| p.1.as_slice())
    }
    pub fn word(&self, key: &str) -> &str {
        self.get(key).and_then(|w| w.first()).map_or("", |w| w.as_str())
    }
    pub fn text(&self, key: &str) -> String {
        self.get(key).map(|w| w.join(" ")).unwrap_or_default()
    }
    /// A number, kept within `lo..=hi`; `default` if missing or not one.
    pub fn num(&self, key: &str, default: f32, lo: f32, hi: f32) -> f32 {
        self.word(key).parse::<f32>().ok().filter(|v| *v >= lo && *v <= hi).unwrap_or(default)
    }
    /// A line that is there and not followed by `no`.
    pub fn flag(&self, key: &str) -> bool {
        self.get(key).map_or(false, |w| !matches!(w.first().map(|x| x.as_str()), Some("no" | "false" | "0")))
    }
}

/// A kind of creature.
#[derive(Clone, Debug)]
pub struct EntityDef {
    pub id: String,
    pub name: String,
    /// Index into the registry's models.
    pub model: usize,
    /// Size in blocks. The model is scaled to `height`.
    pub height: f32,
    pub width: f32,
    /// Blocks per second.
    pub speed: f32,
    pub health: i32,
    pub behavior: Behavior,
    /// Damage per hit, for hostile creatures.
    pub damage: i32,
    /// Colour the model is multiplied by.
    pub tint: [u8; 3],
    /// Biomes it appears in (lower-case names); empty = anywhere.
    pub biomes: Vec<String>,
    /// Appears only at night.
    pub night: bool,
    /// Relative chance of being picked when something spawns.
    pub weight: u32,
    /// Item and count left behind when killed in survival.
    pub drop: Option<(usize, u16)>,
    pub x: EntityExtra,
}

/// What a block does when used (right-clicked).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Usable {
    Nothing,
    /// Opens the 3 x 3 crafting grid (`use craft`, or `use craft 3`).
    Craft,
    /// Opens the 5 x 5 grid of a crafting bench (`use craft 5`).
    Craft5,
    /// Opens the 7 x 7 grid of a workshop (`use craft 7`).
    Craft7,
    /// Opens its own storage.
    Chest,
    /// Opens the baking recipes.
    Oven,
    /// Sleep through the night; wake up here after dying.
    Bed,
    /// Opens the smelting recipes.
    Furnace,
}

impl Usable {
    /// The side of the crafting grid this opens (3, 5 or 7), if it is one.
    pub fn craft_side(self) -> Option<usize> {
        match self {
            Usable::Craft => Some(3),
            Usable::Craft5 => Some(5),
            Usable::Craft7 => Some(7),
            _ => None,
        }
    }
}

pub const TOOLS: [&str; 5] = ["none", "pick", "axe", "shovel", "hoe"];

/// A model's Blockbench source, by id.
#[derive(Clone, Debug)]
pub struct ModelSpec {
    pub id: String,
    pub text: String,
}

/// Every file under `data/` except base.txt, embedded at build time.
pub mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"));

    /// A built-in data file by its path relative to `data/`.
    pub fn file(path: &str) -> Option<&'static [u8]> {
        FILES.iter().find(|f| f.0 == path).map(|f| f.1)
    }
}

/// A `bake` or `smelt` recipe: a list entry at an oven or a furnace. (The
/// crafting grid's recipes are `crafting::GridRecipe`.)
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    /// Item made, and how many.
    pub out: (usize, u16),
    /// Items used up.
    pub inputs: Vec<(usize, u16)>,
    /// Unused: grid recipes are told apart by their size (see `GridRecipe::needs_table`).
    pub table: bool,
    /// Needs an oven instead (`bake` recipes).
    pub oven: bool,
    /// Needs a furnace instead (`smelt` recipes).
    pub furnace: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TileSrc {
    Builtin(String),
    Flat { rgb: [u8; 3], noise: u8 },
    /// A built-in pattern (concrete, wool, planks, bark...) in a given colour.
    Paint { style: String, rgb: [u8; 3] },
    /// The same patterns drawn with a palette: several colours, roles, a
    /// gradient (see `content/palette.rs`).
    Palette { style: String, pal: crate::palette::Palette },
    /// 16 x 16 RGBA.
    Pixels(Vec<u8>),
}

#[derive(Clone, Debug)]
pub struct TileDef {
    pub id: String,
    pub src: TileSrc,
}

/// Blocks the terrain generator asks for by id.
#[derive(Clone, Copy, Default, Debug)]
pub struct WellKnown {
    pub stone: Block,
    pub dirt: Block,
    pub grass: Block,
    pub sand: Block,
    pub gravel: Block,
    pub water: Block,
    pub lava: Block,
    pub log: Block,
    pub leaves: Block,
    pub coal_ore: Block,
    pub iron_ore: Block,
    pub copper_ore: Block,
    pub crystal_ore: Block,
    pub snow: Block,
    pub snowy_grass: Block,
    pub ice: Block,
    pub sandstone: Block,
    pub cactus: Block,
    pub flower_red: Block,
    pub flower_yellow: Block,
    pub tall_grass: Block,
    pub pine_log: Block,
    pub pine_leaves: Block,
    pub deepstone: Block,
    pub clay: Block,
    pub birch_log: Block,
    pub birch_leaves: Block,
    pub cactus_arm: Block,
    pub cactus_arm_z: Block,
    /// The trunk where an arm leaves it: +x, -x, +z, -z.
    pub cactus_joint: [Block; 4],
    pub acacia_log: Block,
    pub acacia_leaves: Block,
    pub dark_oak_log: Block,
    pub dark_oak_leaves: Block,
    pub maple_log: Block,
    pub maple_leaves: Block,
    pub palm_log: Block,
    pub palm_leaves: Block,
    pub blossom_log: Block,
    pub blossom_leaves: Block,
    pub ruby_ore: Block,
    pub snow_layer: Block,
    /// What grows and glows underground; any may be AIR if the content
    /// leaves it out.
    pub moss: Block,
    pub glow_mushroom: Block,
    pub mushroom: Block,
    pub crystal_cluster: Block,
    pub icicle: Block,
    pub magma: Block,
    pub basalt: Block,
    pub cave_vine: Block,
    pub stalagmite: Block,
    pub web: Block,
    pub farmland: Block,
    /// Ripe crops, for the fields of terrain made from a map (AIR where missing).
    pub crops: [Block; 4],
    /// Road surface, its yellow and white paint, and what towns are
    /// furnished with: floor, stairs, table, chest, workbench, bed, lamp.
    pub asphalt: Block,
    pub paint: [Block; 2],
    pub floor: Block,
    pub stairs: Block,
    pub furniture: [Block; 5],
    /// Building materials for terrain made from a map, and their window glass.
    pub walls: [Block; 4],
    pub window: Block,
    /// Banded clay for badlands: four colours, bottom to top.
    pub bands: [Block; 4],
}

pub struct Registry {
    /// One entry per state.
    pub blocks: Vec<BlockDef>,
    pub items: Vec<ItemDef>,
    pub tiles: Vec<TileDef>,
    /// `bake` and `smelt` recipes (the station lists).
    pub recipes: Vec<Recipe>,
    /// `craft`, `recipe`, `shapeless` and `shaped` recipes, for the crafting grid.
    pub grid_recipes: Vec<crate::crafting::GridRecipe>,
    pub models: Vec<ModelSpec>,
    pub entities: Vec<EntityDef>,
    /// Sections and lines of the packs kept for the mods' own code.
    pub sections: Vec<Section>,
    /// Status effects: poison, burning... (see content/effects.rs).
    pub fx: crate::content::effects::EffectTable,
    pub lines: Vec<(String, String, Vec<String>)>,
    /// Item tabs, in the order they first appear.
    pub categories: Vec<String>,
    /// Things packs ask the terrain generator to add.
    pub ores: Vec<OreRule>,
    /// Every sound the game can make, by name; packs may retune or add.
    pub sounds: Vec<SoundDef>,
    /// Commands packs add to the chat, and things they make happen.
    pub commands: Vec<(String, Vec<String>)>,
    pub scripts: Vec<Script>,
    /// Other worlds inside a world: a name and the lie of its land
    /// (an index into `worldgen::SHAPES`).
    pub dimensions: Vec<(String, u8)>,
    /// Parts of the game that packs switch on (`feature <name>`): things
    /// built into the program that only run when a pack asks for them.
    pub features: Vec<String>,
    /// Things that come over the world for a while, as packs define them.
    pub episodes: Vec<EpisodeDef>,
    /// Switches packs add to Settings > GAME: id, whether on to begin
    /// with, and the label.
    pub toggles: Vec<(String, bool, String)>,
    pub plants: Vec<PlantRule>,
    /// Per-state lookup tables for the hot loops.
    pub opaque: Vec<u8>,
    pub emit: Vec<u8>,
    pub sky_loss: Vec<u8>,
    pub kind: Vec<Kind>,
    /// Per state: the liquid it is, if any (see [`fluid_of`]).
    pub fluid: Vec<Option<(bool, u8)>>,
    /// The state of each liquid at each level: `flow[lava][level]`.
    pub flow: [[Block; 9]; 2],
    pub wk: WellKnown,
    /// The hidden filler that big props put in the cells they take up.
    pub part: Block,
    pub sun_tile: u16,
    pub moon_tile: u16,
    by_id: BTreeMap<String, Block>,
    /// Problems found while reading the data, for the log.
    pub warnings: Vec<String>,
}

pub const BASE_DATA: &str = include_str!("../../data/base.txt");

static REG: AtomicPtr<Registry> = AtomicPtr::new(core::ptr::null_mut());

/// The active registry. If none was installed, the built-in data is used.
pub fn reg() -> &'static Registry {
    let p = REG.load(Ordering::Acquire);
    if !p.is_null() {
        return unsafe { &*p };
    }
    // Nothing installed yet (tests, tools): the base data alone; the
    // tests also get the mods that come with the game.
    #[allow(unused_mut)]
    let mut sources: Vec<(&str, &str)> = alloc::vec![("base.txt", BASE_DATA)];
    #[cfg(test)]
    for (path, bytes) in crate::native::shipped() {
        if path.ends_with("/pack.txt") {
            sources.push((path, core::str::from_utf8(bytes).unwrap()));
        }
    }
    install(Registry::from_sources(&sources, &mut |_path| {
        #[cfg(test)]
        if let Some(f) = crate::native::shipped().iter().find(|f| f.0 == _path) {
            return Some(f.1.to_vec());
        }
        None
    }));
    unsafe { &*REG.load(Ordering::Acquire) }
}

/// Make `r` the active registry. Only the first call has an effect, because
/// references handed out by [`reg`] live for the rest of the program.
pub fn install(r: Registry) {
    let p = Box::into_raw(Box::new(r));
    if REG.compare_exchange(core::ptr::null_mut(), p, Ordering::AcqRel, Ordering::Acquire).is_err() {
        unsafe { drop(Box::from_raw(p)) };
    }
}

/// Swap in a new registry while the game runs (packs reloaded, or sent by a
/// server). The old one is deliberately never freed: references to it may
/// still be in use, on this thread or a worker.
pub fn replace(r: Registry) {
    REG.store(Box::into_raw(Box::new(r)), Ordering::Release);
}

#[inline]
/// The liquid a block is, if any: (lava rather than water, level 1-8 where 8
/// is the source). Flows are one block per level; see `src/game/fluid.rs`.
pub fn fluid_of(b: Block) -> Option<(bool, u8)> {
    reg().fluid.get(b as usize).copied().flatten()
}

/// Whether a block is water, at whatever level (lava is not).
pub fn is_water(b: Block) -> bool {
    matches!(fluid_of(b), Some((false, _)))
}

/// Whether two blocks are the same liquid, at whatever levels.
pub fn same_fluid(a: Block, b: Block) -> bool {
    matches!((fluid_of(a), fluid_of(b)), (Some((x, _)), Some((y, _))) if x == y)
}

pub fn def(b: Block) -> &'static BlockDef {
    let r = reg();
    &r.blocks[if (b as usize) < r.blocks.len() { b as usize } else { 0 }]
}

#[inline]
pub fn is_solid(b: Block) -> bool {
    def(b).solid
}

/// Whether a fence or wall `d` reaches toward a neighbouring block `nb`.
pub fn connects(d: &BlockDef, nb: Block) -> bool {
    if d.connect_group == 0 || nb == AIR {
        return false;
    }
    let n = def(nb);
    (d.join_cubes && n.kind == Kind::Cube) || n.connect_group == d.connect_group
}

/// Which sides a fence or wall is connected on: bit 0 north (-z), bit 1
/// west (-x), bit 2 south (+z), bit 3 east (+x). `get(dx, dz)` returns the
/// neighbouring block.
pub fn connect_mask(b: Block, get: &dyn Fn(i32, i32) -> Block) -> u8 {
    let d = def(b);
    if d.connect_group == 0 {
        return 0;
    }
    let mut m = 0;
    for (bit, (dx, dz)) in [(0, -1), (-1, 0), (0, 1), (1, 0)].into_iter().enumerate() {
        if connects(d, get(dx, dz)) {
            m |= 1 << bit;
        }
    }
    m
}

/// The boxes that make up block `b`, given its connections. Full blocks
/// give one whole-cell box.
pub fn shape(b: Block, mask: u8, out: &mut Vec<Bx>) {
    out.clear();
    let d = def(b);
    if d.kind != Kind::Boxes {
        out.push(Bx::FULL);
        return;
    }
    out.extend_from_slice(&d.boxes);
    for side in 0..4u8 {
        if mask & (1 << side) != 0 {
            out.extend(d.arms.iter().map(|a| a.turned_by(side)));
        }
    }
}

// ------------------------------------------------------------------ parsing

#[derive(Clone)]
struct RawBlock {
    id: String,
    name: String,
    shape: String,
    tex: [String; 3],
    emit: u8,
    sky_loss: Option<u8>,
    solid: Option<bool>,
    unbreakable: bool,
    hidden: bool,
    boxes: Vec<Bx>,
    rotate: bool,
    flip: bool,
    hardness: Option<f32>,
    drops: Option<String>,
    drop_chance: u8,
    tool: u8,
    usable: Usable,
    category: String,
    /// `connect <group> [cubes]` and `arm` boxes: joins neighbours of its group.
    connect: String,
    join_cubes: bool,
    arms: Vec<Bx>,
    /// Behaviour lines, resolved once every block and item is known.
    extra: Vec<(String, Vec<String>)>,
}

#[derive(Clone)]
struct RawItem {
    id: String,
    name: String,
    icon: String,
    place: String,
    brush: u8,
    reach: f32,
    stack: u16,
    tool: u8,
    speed: f32,
    food: i32,
    category: String,
    extra: Vec<(String, Vec<String>)>,
}

#[derive(Clone)]
struct RawEntity {
    id: String,
    name: String,
    model: String,
    height: f32,
    width: f32,
    speed: f32,
    health: i32,
    behavior: Behavior,
    damage: i32,
    tint: [u8; 3],
    biomes: Vec<String>,
    night: bool,
    weight: u32,
    drop: Option<(String, u16)>,
    extra: Vec<(String, Vec<String>)>,
}

enum Cur {
    None,
    Block(usize),
    Item(usize),
    Entity(usize),
    Section(usize),
    /// An `effect <id>` block (see content/effects.rs).
    Effect(usize),
}

struct Parser<'a> {
    tiles: Vec<TileDef>,
    blocks: Vec<RawBlock>,
    items: Vec<RawItem>,
    models: Vec<ModelSpec>,
    entities: Vec<RawEntity>,
    sections: Vec<Section>,
    /// The `effect <id>` blocks, read by content/effects.rs.
    effects: Vec<Section>,
    lines: Vec<(String, String, Vec<String>)>,
    /// `(file:line, words)` for each `recipe` line, resolved once items exist.
    recipes: Vec<(String, Vec<String>)>,
    /// `group <name> <item> ...` lines: `(file:line, words)`.
    groups: Vec<(String, Vec<String>)>,
    /// `ore` and `plant` lines, kept the same way.
    rules: Vec<(String, Vec<String>)>,
    /// Sound files read while parsing: the sound's id and its samples.
    clips: Vec<(String, Vec<i16>)>,
    warnings: Vec<String>,
    /// Palettes declared so far (`palette` lines), for `texture ... paint`.
    palettes: crate::palette::Palettes,
    load: &'a mut dyn FnMut(&str) -> Option<Vec<u8>>,
    depth: u32,
}

fn dir_of(path: &str) -> &str {
    match path.rfind(|c| c == '/' || c == '\\') {
        Some(i) => &path[..=i],
        None => "",
    }
}

impl Parser<'_> {
    fn warn(&mut self, file: &str, line: usize, msg: &str) {
        self.warnings.push(alloc::format!("{}:{}: {}", file, line, msg));
    }

    fn parse(&mut self, file: &str, text: &str) {
        let mut cur = Cur::None;
        for (n, raw) in text.lines().enumerate() {
            let line_no = n + 1;
            let line = match crate::palette::comment_start(raw) {
                Some(i) => &raw[..i],
                None => raw,
            };
            let mut words = line.split_whitespace();
            let key = match words.next() {
                Some(k) => k,
                None => continue,
            };
            let args: Vec<&str> = words.collect();
            let rest = args.join(" ");
            let indent = raw.len() - raw.trim_start().len();
            match key {
                "texture" => {
                    cur = Cur::None;
                    self.texture(file, line_no, &args);
                }
                "palette" => {
                    cur = Cur::None;
                    if let Err(m) = crate::palette::declare(&mut self.palettes, &args) {
                        self.warn(file, line_no, &m);
                    }
                }
                "block" | "item" => {
                    let id = match args.first() {
                        Some(id) => id.to_string(),
                        None => {
                            self.warn(file, line_no, "missing id");
                            cur = Cur::None;
                            continue;
                        }
                    };
                    if key == "block" {
                        let fresh = RawBlock {
                            id: id.clone(),
                            name: id.clone(),
                            shape: String::from("cube"),
                            tex: [String::new(), String::new(), String::new()],
                            emit: 0,
                            sky_loss: None,
                            solid: None,
                            unbreakable: false,
                            hidden: false,
                            boxes: Vec::new(),
                            rotate: false,
                            flip: false,
                            hardness: None,
                            drops: None,
                            drop_chance: 100,
                            tool: 0,
                            usable: Usable::Nothing,
                            category: String::new(),
                            connect: String::new(),
                            join_cubes: false,
                            arms: Vec::new(),
                            extra: Vec::new(),
                        };
                        // Redefining an id replaces it in place, keeping its position.
                        let i = match self.blocks.iter().position(|b| b.id == id) {
                            Some(i) => {
                                self.blocks[i] = fresh;
                                i
                            }
                            None => {
                                self.blocks.push(fresh);
                                self.blocks.len() - 1
                            }
                        };
                        cur = Cur::Block(i);
                    } else {
                        let fresh = RawItem {
                            id: id.clone(),
                            name: id.clone(),
                            icon: String::new(),
                            place: String::new(),
                            brush: 0,
                            reach: 0.0,
                            stack: 64,
                            tool: 0,
                            speed: 1.0,
                            food: 0,
                            category: String::new(),
                            extra: Vec::new(),
                        };
                        let i = match self.items.iter().position(|b| b.id == id) {
                            Some(i) => {
                                self.items[i] = fresh;
                                i
                            }
                            None => {
                                self.items.push(fresh);
                                self.items.len() - 1
                            }
                        };
                        cur = Cur::Item(i);
                    }
                }
                // Inside an entity, `model <id>` names the model it uses.
                "model" if !(matches!(cur, Cur::Entity(_) | Cur::Section(_)) && args.len() == 1) => {
                    cur = Cur::None;
                    let text = match (args.first(), args.get(1), args.get(2)) {
                        (Some(_), Some(&"builtin"), Some(name)) => {
                            embedded::file(&alloc::format!("models/{}.bbmodel", name)).map(|b| String::from_utf8_lossy(b).into_owned())
                        }
                        (Some(_), Some(&"file"), Some(_)) => {
                            let path = alloc::format!("{}{}", dir_of(file), args[2..].join(" "));
                            (self.load)(&path).map(|b| String::from_utf8_lossy(&b).into_owned())
                        }
                        _ => None,
                    };
                    match text {
                        Some(text) => {
                            let spec = ModelSpec { id: args[0].to_string(), text };
                            match self.models.iter().position(|m| m.id == spec.id) {
                                Some(i) => self.models[i] = spec,
                                None => self.models.push(spec),
                            }
                        }
                        None => self.warn(file, line_no, "model needs: <id> builtin <name>  or  <id> file <model.bbmodel>"),
                    }
                }
                "entity" => {
                    let id = match args.first() {
                        Some(id) => id.to_string(),
                        None => {
                            self.warn(file, line_no, "missing id");
                            cur = Cur::None;
                            continue;
                        }
                    };
                    let fresh = RawEntity {
                        id: id.clone(),
                        name: id.clone(),
                        model: String::new(),
                        height: 1.8,
                        width: 0.6,
                        speed: 2.5,
                        health: 10,
                        behavior: Behavior::Wander,
                        damage: 2,
                        tint: [255, 255, 255],
                        biomes: Vec::new(),
                        night: false,
                        weight: 10,
                        drop: None,
                        extra: Vec::new(),
                    };
                    let i = match self.entities.iter().position(|e| e.id == id) {
                        Some(i) => {
                            self.entities[i] = fresh;
                            i
                        }
                        None => {
                            self.entities.push(fresh);
                            self.entities.len() - 1
                        }
                    };
                    cur = Cur::Entity(i);
                }
                // A status effect: its lines are read by content/effects.rs.
                "effect" if indent == 0 => match args.first() {
                    Some(id) => cur = Cur::Effect(crate::content::effects::begin(&mut self.effects, id, alloc::format!("{}:{}", file, line_no))),
                    None => {
                        self.warn(file, line_no, "missing id");
                        cur = Cur::None;
                    }
                },
                // A section some mod's code has claimed: kept as written.
                k if indent == 0 && crate::native::reads_section(k) => {
                    let id = match args.first() {
                        Some(id) => id.to_string(),
                        None => {
                            self.warn(file, line_no, "missing id");
                            cur = Cur::None;
                            continue;
                        }
                    };
                    let fresh = Section { kind: String::from(k), id: id.clone(), at: alloc::format!("{}:{}", file, line_no), props: Vec::new() };
                    let i = match self.sections.iter().position(|v| v.id == id && v.kind == k) {
                        Some(i) => {
                            self.sections[i] = fresh;
                            i
                        }
                        None => {
                            self.sections.push(fresh);
                            self.sections.len() - 1
                        }
                    };
                    cur = Cur::Section(i);
                }
                // A line some mod's code has claimed: kept as written.
                k if indent == 0 && crate::native::reads_line(k) => {
                    cur = Cur::None;
                    self.lines.push((String::from(k), alloc::format!("{}:{}", file, line_no), args.iter().map(|a| a.to_string()).collect()));
                }
                // (Inside a block or creature, `sound <id>` names the sound it makes.)
                "ore" | "plant" | "sound" | "command" | "when" | "dimension" | "feature" | "episode" | "setting" if !(key == "sound" && args.len() == 1 && !matches!(cur, Cur::None)) => {
                    cur = Cur::None;
                    let mut words: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                    words.insert(0, key.to_string());
                    // sound <id> <bus> file <path> [loudness] [caption...]: read the file now.
                    if key == "sound" && words.get(3).map(|s| s.as_str()) == Some("file") {
                        let path = alloc::format!("{}{}", dir_of(file), words.get(4).map(|s| s.as_str()).unwrap_or(""));
                        match (self.load)(&path).and_then(|b| crate::wav::decode(&b)) {
                            Some(samples) => {
                                self.clips.retain(|c| c.0 != words[1]);
                                self.clips.push((words[1].clone(), samples));
                                words.remove(4);
                                words[3] = String::from("clip");
                            }
                            None => {
                                self.warn(file, line_no, &alloc::format!("cannot read {} as a sound (WAV, AIFF, FLAC, Ogg Vorbis or MP3, up to 12 seconds)", path));
                                continue;
                            }
                        }
                    }
                    self.rules.push((alloc::format!("{}:{}", file, line_no), words));
                }
                "recipe" | "craft" | "shapeless" | "bake" | "smelt" => {
                    cur = Cur::None;
                    // The keyword is carried as a first word and read when
                    // resolving.
                    let mut words: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                    words.insert(0, key.to_string());
                    self.recipes.push((alloc::format!("{}:{}", file, line_no), words));
                }
                // Rows may be quoted and hold spaces, so this line is split
                // again with that in mind (`crafting::parse`).
                "shaped" => {
                    cur = Cur::None;
                    self.recipes.push((alloc::format!("{}:{}", file, line_no), crate::crafting::parse::split_words(line)));
                }
                "group" => {
                    cur = Cur::None;
                    let words: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                    if words.len() < 2 {
                        self.warn(file, line_no, "group needs a name and items");
                    } else {
                        self.groups.push((alloc::format!("{}:{}", file, line_no), words));
                    }
                }
                "include" => {
                    cur = Cur::None;
                    if self.depth >= 8 {
                        self.warn(file, line_no, "includes nested too deeply");
                        continue;
                    }
                    let path = alloc::format!("{}{}", dir_of(file), rest);
                    match (self.load)(&path) {
                        Some(bytes) => {
                            let text = String::from_utf8_lossy(&bytes).into_owned();
                            self.depth += 1;
                            self.parse(&path, &text);
                            self.depth -= 1;
                        }
                        None => self.warn(file, line_no, &alloc::format!("cannot read {}", path)),
                    }
                }
                _ => match cur {
                    Cur::Block(i) => self.block_prop(file, line_no, i, key, &args, &rest),
                    Cur::Item(i) => self.item_prop(file, line_no, i, key, &args, &rest),
                    Cur::Entity(i) => self.entity_prop(file, line_no, i, key, &args, &rest),
                    Cur::Section(i) => self.sections[i].props.push((String::from(key), args.iter().map(|a| a.to_string()).collect())),
                    Cur::Effect(i) => self.effects[i].props.push((String::from(key), args.iter().map(|a| a.to_string()).collect())),
                    Cur::None => self.warn(file, line_no, &alloc::format!("unexpected `{}`", key)),
                },
            }
        }
    }

    fn texture(&mut self, file: &str, line: usize, a: &[&str]) {
        if a.len() < 3 {
            self.warn(file, line, "texture needs: <id> builtin|flat|file ...");
            return;
        }
        let src = match a[1] {
            "builtin" => TileSrc::Builtin(a[2].to_string()),
            "flat" => {
                let num = |i: usize| a.get(i).and_then(|s| s.parse::<u8>().ok());
                match (num(2), num(3), num(4)) {
                    (Some(r), Some(g), Some(b)) => {
                        let noise = if a.get(5) == Some(&"noise") { num(6).unwrap_or(0) } else { 0 };
                        TileSrc::Flat { rgb: [r, g, b], noise }
                    }
                    _ => {
                        self.warn(file, line, "flat needs three numbers 0..255");
                        return;
                    }
                }
            }
            "paint" => match crate::palette::paint_source(&a[2..], &self.palettes) {
                Ok(src) => src,
                Err(m) => {
                    self.warn(file, line, &alloc::format!("paint: {}", m));
                    return;
                }
            },
            "file" => {
                let path = alloc::format!("{}{}", dir_of(file), a[2..].join(" "));
                let read = (self.load)(&path).and_then(|b| crate::image::decode(&b)).map(|img| if img.w == 16 && img.h == 16 { img.rgba } else { crate::image::fit(&img, 16) });
                match read {
                    Some(px) => TileSrc::Pixels(px),
                    None => {
                        self.warn(file, line, &alloc::format!("cannot read {} as a picture (PNG, JPEG, GIF, BMP or TGA)", path));
                        return;
                    }
                }
            }
            other => {
                self.warn(file, line, &alloc::format!("unknown texture source `{}`", other));
                return;
            }
        };
        let def = TileDef { id: a[0].to_string(), src };
        match self.tiles.iter().position(|t| t.id == def.id) {
            Some(i) => self.tiles[i] = def,
            None if self.tiles.len() < crate::textures::MAX_TILES => self.tiles.push(def),
            None => self.warn(file, line, "too many textures (1024 at most)"),
        }
    }

    fn block_prop(&mut self, file: &str, line: usize, i: usize, key: &str, a: &[&str], rest: &str) {
        let yes = |s: Option<&&str>| !matches!(s, Some(&"no") | Some(&"false") | Some(&"0"));
        let mut bad = None;
        {
            let b = &mut self.blocks[i];
            match key {
                "name" => b.name = rest.to_string(),
                "shape" => b.shape = a.first().unwrap_or(&"cube").to_string(),
                "tex" => match a.len() {
                    1 => b.tex = [a[0].to_string(), a[0].to_string(), a[0].to_string()],
                    3 => b.tex = [a[0].to_string(), a[1].to_string(), a[2].to_string()],
                    _ => bad = Some("tex takes one texture, or top side bottom"),
                },
                "light" => match a.first().and_then(|s| s.parse::<u8>().ok()) {
                    Some(v) if v <= 15 => b.emit = v,
                    _ => bad = Some("light must be 0..15"),
                },
                "skyloss" => match a.first().and_then(|s| s.parse::<u8>().ok()) {
                    Some(v) if v <= 15 => b.sky_loss = Some(v),
                    _ => bad = Some("skyloss must be 0..15"),
                },
                "solid" => b.solid = Some(yes(a.first())),
                "unbreakable" => b.unbreakable = yes(a.first()),
                "hidden" => b.hidden = yes(a.first()),
                "rotate" => b.rotate = yes(a.first()),
                "flip" => b.flip = yes(a.first()),
                "hardness" => match a.first().and_then(|s| s.parse::<f32>().ok()) {
                    Some(v) if (0.0..=600.0).contains(&v) => b.hardness = Some(v),
                    _ => bad = Some("hardness must be 0..600 seconds"),
                },
                "drops" => {
                    b.drops = Some(a.first().unwrap_or(&"nothing").to_string());
                    b.drop_chance = a.get(1).and_then(|s| s.parse::<u8>().ok()).unwrap_or(100).min(100);
                }
                "tool" => match a.first().and_then(|t| TOOLS.iter().position(|x| x == t)) {
                    Some(t) => b.tool = t as u8,
                    None => bad = Some("tool must be none, pick, axe, shovel or hoe"),
                },
                "long" | "react" | "falls" | "tall" | "toggle" | "grows" | "on" | "also" | "hurts" | "fluid" | "slow" | "sound" | "seat" | "eat" | "tag" | "prop" | "fills" | "part" => {
                    // Said twice, the later line wins; except marks, of
                    // which a block may carry several.
                    b.extra.retain(|e| e.0 != key || key == "react" || (key == "tag" && e.1.first().map(|x| x.as_str()) != a.first().copied()));
                    b.extra.push((key.to_string(), a.iter().map(|s| s.to_string()).collect()));
                }
                "use" => match a.first() {
                    Some(&"craft") => match a.get(1).copied() {
                        None | Some("3") => b.usable = Usable::Craft,
                        Some("5") => b.usable = Usable::Craft5,
                        Some("7") => b.usable = Usable::Craft7,
                        _ => bad = Some("use craft takes a grid size: 3, 5 or 7"),
                    },
                    Some(&"chest") => b.usable = Usable::Chest,
                    Some(&"oven") => b.usable = Usable::Oven,
                    Some(&"furnace") => b.usable = Usable::Furnace,
                    Some(&"bed") => b.usable = Usable::Bed,
                    _ => bad = Some("use must be craft [3|5|7], chest, oven, furnace or bed"),
                },
                "category" => b.category = rest.to_string(),
                // connect <group> [cubes]: joins blocks of the same group (and full cubes)
                "connect" => match a.first() {
                    Some(g) if g.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => {
                        b.connect = g.to_string();
                        b.join_cubes = a.get(1) == Some(&"cubes");
                    }
                    _ => bad = Some("connect needs a group name (letters, digits, _) and may add `cubes`"),
                },
                // arm x0 y0 z0 x1 y1 z1: drawn toward a joined neighbour, written for the north (-z) side
                "arm" => {
                    let v: Vec<u8> = a.iter().filter_map(|s| s.parse::<u8>().ok()).filter(|&v| v <= 16).collect();
                    if v.len() == 6 && a.len() == 6 && v[0] < v[3] && v[1] < v[4] && v[2] < v[5] {
                        if b.arms.len() < 4 {
                            b.arms.push(Bx { min: [v[0], v[1], v[2]], max: [v[3], v[4], v[5]] });
                        } else {
                            bad = Some("a block may have 4 arms at most");
                        }
                    } else {
                        bad = Some("arm needs x0 y0 z0 x1 y1 z1, each 0..16, with x0<x1 y0<y1 z0<z1");
                    }
                }
                "box" => {
                    let v: Vec<u8> = a.iter().filter_map(|s| s.parse::<u8>().ok()).filter(|&v| v <= 16).collect();
                    if v.len() == 6 && a.len() == 6 && v[0] < v[3] && v[1] < v[4] && v[2] < v[5] {
                        if b.boxes.len() < 8 {
                            b.boxes.push(Bx { min: [v[0], v[1], v[2]], max: [v[3], v[4], v[5]] });
                            b.shape = String::from("boxes");
                        } else {
                            bad = Some("a block may have 8 boxes at most");
                        }
                    } else {
                        bad = Some("box needs x0 y0 z0 x1 y1 z1, each 0..16, with x0<x1 y0<y1 z0<z1");
                    }
                }
                _ => bad = Some("unknown block property"),
            }
        }
        if let Some(msg) = bad {
            self.warn(file, line, &alloc::format!("{} (`{}`)", msg, key));
        }
    }

    fn entity_prop(&mut self, file: &str, line: usize, i: usize, key: &str, a: &[&str], rest: &str) {
        let mut bad = None;
        {
            let e = &mut self.entities[i];
            let f = |lo: f32, hi: f32| a.first().and_then(|s| s.parse::<f32>().ok()).filter(|v| (lo..=hi).contains(v));
            let n = |lo: i32, hi: i32| a.first().and_then(|s| s.parse::<i32>().ok()).filter(|v| (lo..=hi).contains(v));
            match key {
                "name" => e.name = rest.to_string(),
                "model" => e.model = a.first().unwrap_or(&"").to_string(),
                "height" => match f(0.1, 6.0) {
                    Some(v) => e.height = v,
                    None => bad = Some("height must be 0.1..6 blocks"),
                },
                "width" => match f(0.1, 3.0) {
                    Some(v) => e.width = v,
                    None => bad = Some("width must be 0.1..3 blocks"),
                },
                "speed" => match f(0.0, 12.0) {
                    Some(v) => e.speed = v,
                    None => bad = Some("speed must be 0..12 blocks per second"),
                },
                "health" => match n(1, 1000) {
                    Some(v) => e.health = v,
                    None => bad = Some("health must be 1..1000"),
                },
                "damage" => match n(0, 100) {
                    Some(v) => e.damage = v,
                    None => bad = Some("damage must be 0..100"),
                },
                "weight" => match n(0, 1000) {
                    Some(v) => e.weight = v as u32,
                    None => bad = Some("weight must be 0..1000"),
                },
                "behavior" => match a.first() {
                    Some(&"wander") => e.behavior = Behavior::Wander,
                    Some(&"npc") => e.behavior = Behavior::Npc,
                    Some(&"hostile") => e.behavior = Behavior::Hostile,
                    Some(&"swim") => e.behavior = Behavior::Swim,
                    Some(&"fly") => e.behavior = Behavior::Fly,
                    Some(&"villager") => e.behavior = Behavior::Villager,
                    _ => bad = Some("behavior must be wander, npc, hostile, swim, fly or villager"),
                },
                "trait" | "anim" | "variant" | "tex" | "baby" | "gives" | "trade" | "job" | "likes" | "glow" | "sound" | "tamable" | "shoots" | "rideable" | "hunts" | "explodes" => {
                    e.extra.push((key.to_string(), a.iter().map(|s| s.to_string()).collect()));
                }
                "tint" => {
                    let v: Vec<u8> = a.iter().filter_map(|s| s.parse::<u8>().ok()).collect();
                    if v.len() == 3 && a.len() == 3 {
                        e.tint = [v[0], v[1], v[2]];
                    } else {
                        bad = Some("tint needs three numbers 0..255");
                    }
                }
                "biomes" => e.biomes = a.iter().map(|s| s.to_ascii_lowercase()).collect(),
                "night" => e.night = !matches!(a.first(), Some(&"no") | Some(&"false") | Some(&"0")),
                "drops" => match (a.first(), a.get(1).and_then(|s| s.parse::<u16>().ok()).unwrap_or(1)) {
                    (Some(id), n) if (1..=64).contains(&n) => e.drop = Some((id.to_string(), n)),
                    _ => bad = Some("drops needs an item and a count 1..64"),
                },
                _ => bad = Some("unknown entity property"),
            }
        }
        if let Some(msg) = bad {
            self.warn(file, line, &alloc::format!("{} (`{}`)", msg, key));
        }
    }

    fn item_prop(&mut self, file: &str, line: usize, i: usize, key: &str, a: &[&str], rest: &str) {
        let mut bad = None;
        {
            let it = &mut self.items[i];
            match key {
                "name" => it.name = rest.to_string(),
                "icon" => it.icon = a.first().unwrap_or(&"").to_string(),
                "place" => it.place = a.first().unwrap_or(&"").to_string(),
                "brush" => match a.first().and_then(|s| s.parse::<u8>().ok()) {
                    Some(v) if v <= 6 => it.brush = v,
                    _ => bad = Some("brush must be 0..6"),
                },
                "reach" => match a.first().and_then(|s| s.parse::<f32>().ok()) {
                    Some(v) if (1.0..=64.0).contains(&v) => it.reach = v,
                    _ => bad = Some("reach must be 1..64"),
                },
                "tool" => match (a.first().and_then(|t| TOOLS.iter().position(|x| x == t)), a.get(1).and_then(|s| s.parse::<f32>().ok())) {
                    (Some(t), Some(sp)) if (1.0..=100.0).contains(&sp) => {
                        it.tool = t as u8;
                        it.speed = sp;
                    }
                    _ => bad = Some("tool needs a kind (pick, axe, shovel) and a speed 1..100"),
                },
                "food" => match a.first().and_then(|s| s.parse::<i32>().ok()) {
                    Some(v) if (1..=20).contains(&v) => it.food = v,
                    _ => bad = Some("food must be 1..20"),
                },
                "stack" => match a.first().and_then(|s| s.parse::<u16>().ok()) {
                    Some(v) if (1..=999).contains(&v) => it.stack = v,
                    _ => bad = Some("stack must be 1..999"),
                },
                "category" => it.category = rest.to_string(),
                "turns" | "damage" | "shoots" => it.extra.push((key.to_string(), a.iter().map(|s| s.to_string()).collect())),
                _ => bad = Some("unknown item property"),
            }
        }
        if let Some(msg) = bad {
            self.warn(file, line, &alloc::format!("{} (`{}`)", msg, key));
        }
    }
}

impl Registry {
    /// Build a registry from data files, given as `(path, text)` in order.
    /// `load` reads a file named by an `include` or `texture ... file` line.
    pub fn from_sources(sources: &[(&str, &str)], load: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Registry {
        // Files named by the built-in data come from inside the program.
        let mut with_builtin = |path: &str| embedded::file(path).map(|b| b.to_vec()).or_else(|| load(path));
        let load: &mut dyn FnMut(&str) -> Option<Vec<u8>> = &mut with_builtin;
        let mut p = Parser {
            tiles: Vec::new(),
            blocks: Vec::new(),
            items: Vec::new(),
            models: Vec::new(),
            entities: Vec::new(),
            sections: Vec::new(),
            effects: Vec::new(),
            lines: Vec::new(),
            recipes: Vec::new(),
            groups: Vec::new(),
            rules: Vec::new(),
            clips: Vec::new(),
            warnings: Vec::new(),
            palettes: Vec::new(),
            load,
            depth: 0,
        };
        for (path, text) in sources {
            p.parse(path, text);
        }
        let Parser { tiles, blocks: raw_blocks, items: raw_items, recipes: raw_recipes, groups: raw_groups, rules: raw_rules, clips, models, entities: raw_entities, sections, effects: raw_effects, lines, mut warnings, .. } = p;

        let tile_of = |id: &str, warnings: &mut Vec<String>, what: &str| -> u16 {
            match tiles.iter().position(|t| t.id == id) {
                Some(i) => i as u16,
                None => {
                    if !id.is_empty() {
                        warnings.push(alloc::format!("{}: unknown texture `{}`", what, id));
                    } else {
                        warnings.push(alloc::format!("{}: no texture given", what));
                    }
                    0
                }
            }
        };

        let air = BlockDef {
            id: String::from("air"),
            name: String::from("Air"),
            kind: Kind::Air,
            tex: [0; 3],
            emit: 0,
            sky_loss: 0,
            solid: false,
            unbreakable: false,
            hidden: true,
            boxes: Vec::new(),
            arms: Vec::new(),
            connect_group: 0,
            join_cubes: false,
            hardness: 0.0,
            drop: None,
            drop_chance: 100,
            tool: 0,
            usable: Usable::Nothing,
            first: 0,
            rotations: 1,
            flips: false,
            x: BlockExtra::default(),
        };
        let mut blocks = alloc::vec![air];
        let mut by_id = BTreeMap::new();
        by_id.insert(String::from("air"), 0 as Block);

        let mut connect_groups: Vec<String> = Vec::new();
        for rb in &raw_blocks {
            let bx = |a: [u8; 3], b: [u8; 3]| Bx { min: a, max: b };
            // Groups by name: `fence` and `wall` are the built-in ones, so a
            // custom block can join them; others are numbered as they appear.
            let group_of = |name: &str, groups: &mut Vec<String>| -> u8 {
                match name {
                    "" => 0,
                    "fence" => 1,
                    "wall" => 2,
                    _ => match groups.iter().position(|g| g == name) {
                        Some(i) => 3 + i as u8,
                        None => {
                            groups.push(name.to_string());
                            3 + (groups.len() - 1) as u8
                        }
                    },
                }
            };
            let (arms, connect_group, join_cubes) = match rb.shape.as_str() {
                "fence" => (alloc::vec![bx([7, 6, 0], [9, 9, 6]), bx([7, 12, 0], [9, 15, 6])], 1, true),
                "wall" => (alloc::vec![bx([5, 0, 0], [11, 14, 4])], 2, true),
                _ => (rb.arms.clone(), group_of(&rb.connect, &mut connect_groups), rb.join_cubes),
            };
            let (kind, mut boxes, mut rotate, mut flip) = match rb.shape.as_str() {
                "cube" => (Kind::Cube, Vec::new(), rb.rotate, rb.flip),
                "fence" => (Kind::Boxes, alloc::vec![bx([6, 0, 6], [10, 16, 10])], false, false),
                "wall" => (Kind::Boxes, alloc::vec![bx([4, 0, 4], [12, 16, 12])], false, false),
                "translucent" => (Kind::Translucent, Vec::new(), false, false),
                "cutout" => (Kind::Cutout, Vec::new(), false, false),
                "cross" => (Kind::Cross, Vec::new(), false, false),
                "liquid" => (Kind::Liquid, Vec::new(), false, false),
                "slab" => (Kind::Boxes, alloc::vec![Bx { min: [0, 0, 0], max: [16, 8, 16] }], false, true),
                "stairs" => (
                    Kind::Boxes,
                    alloc::vec![Bx { min: [0, 0, 0], max: [16, 8, 16] }, Bx { min: [0, 8, 0], max: [16, 16, 8] }],
                    true,
                    true,
                ),
                "boxes" => (Kind::Boxes, rb.boxes.clone(), rb.rotate, rb.flip),
                other => {
                    warnings.push(alloc::format!("block {}: unknown shape `{}`, using cube", rb.id, other));
                    (Kind::Cube, Vec::new(), false, false)
                }
            };
            if kind == Kind::Cube {
                // A cube looks the same from every side: no states needed.
                rotate = false;
                flip = false;
            }
            if kind == Kind::Boxes && boxes.is_empty() {
                boxes.push(Bx::FULL);
            }
            let rotations: u8 = if rotate { 4 } else { 1 };
            let states = rotations as usize * if flip { 2 } else { 1 };
            if blocks.len() + states > 4096 {
                warnings.push(alloc::format!("block {}: too many block states, skipped", rb.id));
                continue;
            }
            let what = alloc::format!("block {}", rb.id);
            let top = tile_of(&rb.tex[0], &mut warnings, &what);
            let same = rb.tex[1] == rb.tex[0] && rb.tex[2] == rb.tex[0];
            let tex = if same {
                [top, top, top]
            } else {
                [top, tile_of(&rb.tex[1], &mut warnings, &what), tile_of(&rb.tex[2], &mut warnings, &what)]
            };
            let first = blocks.len() as Block;
            by_id.insert(rb.id.clone(), first);
            for f in 0..(states / rotations as usize) {
                for r in 0..rotations {
                    let mut bs = boxes.clone();
                    for b in bs.iter_mut() {
                        for _ in 0..r {
                            *b = b.turned();
                        }
                        if f == 1 {
                            *b = b.flipped();
                        }
                    }
                    blocks.push(BlockDef {
                        id: rb.id.clone(),
                        name: rb.name.clone(),
                        kind,
                        tex,
                        emit: rb.emit,
                        sky_loss: rb.sky_loss.unwrap_or(0),
                        solid: rb.solid.unwrap_or(matches!(kind, Kind::Cube | Kind::Cutout | Kind::Boxes | Kind::Translucent)),
                        unbreakable: rb.unbreakable,
                        hidden: rb.hidden,
                        boxes: bs,
                        arms: arms.clone(),
                        connect_group,
                        join_cubes,
                        hardness: rb.hardness.unwrap_or(match kind {
                            Kind::Cross => 0.0,
                            Kind::Cutout | Kind::Translucent => 0.3,
                            _ => 1.0,
                        }),
                        drop: None,
                        drop_chance: rb.drop_chance,
                        tool: rb.tool,
                        usable: rb.usable,
                        first,
                        rotations,
                        flips: flip,
                        x: BlockExtra::default(),
                    });
                }
            }
        }

        // Items: one per visible block, then the explicitly declared ones.
        let mut items: Vec<ItemDef> = Vec::new();
        for rb in &raw_blocks {
            // A block keeps its own item unless a plain item already places it
            // (seeds for a crop); an area tool that paints with it does not count.
            if rb.hidden || raw_items.iter().any(|it| it.place == rb.id && it.brush == 0) {
                continue;
            }
            if let Some(&first) = by_id.get(&rb.id) {
                let d = &blocks[first as usize];
                items.push(ItemDef {
                    id: rb.id.clone(),
                    name: d.name.clone(),
                    icon: d.tex[1],
                    place: Some(first),
                    brush: 0,
                    reach: 0.0,
                    stack: 64,
                    tool: 0,
                    speed: 1.0,
                    food: 0,
                    section: None,
                    category: if rb.category.is_empty() { String::from("Blocks") } else { rb.category.clone() },
                    x: ItemExtra::default(),
                });
            }
        }
        // (index of the finished item, its behaviour lines)
        let mut raw_items_final: Vec<(usize, Vec<(String, Vec<String>)>)> = Vec::new();
        for ri in &raw_items {
            let what = alloc::format!("item {}", ri.id);
            let place = if ri.place.is_empty() {
                None
            } else {
                match by_id.get(&ri.place) {
                    Some(&b) => Some(b),
                    None => {
                        warnings.push(alloc::format!("{}: unknown block `{}`", what, ri.place));
                        None
                    }
                }
            };
            let icon = if ri.icon.is_empty() {
                match place {
                    Some(b) => blocks[b as usize].tex[1],
                    None => tile_of("", &mut warnings, &what),
                }
            } else {
                tile_of(&ri.icon, &mut warnings, &what)
            };
            raw_items_final.push((items.len(), ri.extra.clone()));
            items.push(ItemDef {
                id: ri.id.clone(),
                name: ri.name.clone(),
                icon,
                place,
                brush: ri.brush,
                reach: ri.reach,
                stack: ri.stack,
                tool: ri.tool,
                speed: ri.speed,
                food: ri.food,
                section: None,
                category: if ri.category.is_empty() { String::from("Tools") } else { ri.category.clone() },
                x: ItemExtra::default(),
            });
        }

        // Every section is also something to hold: an item with the
        // section's id, its `name`, `icon` and `category` lines.
        for (k, sec) in sections.iter().enumerate() {
            let what = alloc::format!("{} {}", sec.kind, sec.id);
            let name = sec.text("name");
            items.push(ItemDef {
                id: sec.id.clone(),
                name: if name.is_empty() { sec.id.clone() } else { name },
                icon: tile_of(sec.word("icon"), &mut warnings, &what),
                place: None,
                brush: 0,
                reach: 0.0,
                stack: 1,
                tool: 0,
                speed: 1.0,
                food: 0,
                section: Some(k),
                category: if sec.word("category").is_empty() { String::from("Things") } else { sec.text("category") },
                x: ItemExtra::default(),
            });
        }

        // What each block gives when broken: itself unless the data says
        // otherwise (`drops <item>` or `drops nothing`).
        for rb in &raw_blocks {
            let first = match by_id.get(&rb.id) {
                Some(&f) => f as usize,
                None => continue,
            };
            let wanted = rb.drops.as_deref().unwrap_or(&rb.id);
            let drop = if wanted == "nothing" { None } else { items.iter().position(|it| it.id == wanted && it.brush == 0) };
            if drop.is_none() && rb.drops.is_some() && wanted != "nothing" {
                warnings.push(alloc::format!("block {}: drops unknown item `{}`", rb.id, wanted));
            }
            let d = &blocks[first];
            let states = d.rotations as usize * if d.flips { 2 } else { 1 };
            for b in blocks[first..first + states].iter_mut() {
                b.drop = drop;
            }
        }

        // Behaviour that names other blocks and items.
        for rb in &raw_blocks {
            let first = match by_id.get(&rb.id) {
                Some(&f) => f as usize,
                None => continue,
            };
            let mut x = BlockExtra::default();
            for (key, a) in &rb.extra {
                let word = |i: usize| a.get(i).map(|s| s.as_str()).unwrap_or("");
                let other = |i: usize| by_id.get(word(i)).copied();
                let mut ok = true;
                match key.as_str() {
                    "falls" => x.falls = word(0) != "no",
                    "tall" => x.tall = word(0) != "no",
                    "long" => match (word(0), other(1)) {
                        ("ahead", Some(b)) => x.long = Some((true, b)),
                        ("behind", Some(b)) => x.long = Some((false, b)),
                        _ => ok = false,
                    },
                    "react" => match (other(0), other(1)) {
                        (Some(next), Some(becomes)) if x.reacts.len() < 8 => x.reacts.push((next, becomes)),
                        _ => ok = false,
                    },
                    "toggle" => {
                        x.toggle = other(0);
                        ok = x.toggle.is_some();
                    }
                    "grows" => match (other(0), word(1).parse::<f32>()) {
                        (Some(b), Ok(t)) if t > 0.0 => x.grows = Some((b, t)),
                        _ => ok = false,
                    },
                    "on" => {
                        x.on = a.iter().filter_map(|w| by_id.get(w.as_str()).copied()).collect();
                        ok = !x.on.is_empty() && x.on.len() == a.len();
                    }
                    "also" => match items.iter().position(|it| it.id == word(0) && it.brush == 0) {
                        Some(i) => x.also = Some((i, word(1).parse::<u8>().unwrap_or(100).min(100), word(2).parse::<u16>().unwrap_or(1).max(1))),
                        None => ok = false,
                    },
                    "hurts" => x.hurts = word(0).parse::<u8>().unwrap_or(1),
                    "fluid" => x.fluid = Some((word(0) == "lava", word(1).parse::<u8>().unwrap_or(8).clamp(1, 8))),
                    "slow" => x.slow = word(0).parse::<u8>().unwrap_or(50).clamp(5, 100),
                    "sound" => x.sound = word(0).to_string(),
                    "seat" => x.seat = word(0).parse::<u8>().unwrap_or(8).clamp(1, 16),
                    "eat" => x.eat = word(0).parse::<u8>().unwrap_or(4).clamp(1, 40),
                    // prop <model> [height in blocks] [lift off the floor]
                    "prop" => match models.iter().position(|m| m.id == word(0)) {
                        Some(m) => x.prop = Some((m, word(1).parse::<f32>().unwrap_or(1.0).clamp(0.1, 24.0), word(2).parse::<f32>().unwrap_or(0.0).clamp(-1.0, 4.0))),
                        None => ok = false,
                    },
                    // fills <blocks wide> <blocks tall>
                    "fills" => x.fills = (word(0).parse::<u8>().unwrap_or(1).clamp(1, 7), word(1).parse::<u8>().unwrap_or(1).clamp(1, 24)),
                    "part" => x.part = true,
                    "tag" => {
                        if word(0).is_empty() || x.tags.len() >= 16 {
                            ok = false;
                        } else {
                            x.tags.retain(|t| t.0 != word(0));
                            x.tags.push((word(0).to_string(), word(1).parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or(1.0)));
                        }
                    }
                    _ => {}
                }
                if !ok {
                    warnings.push(alloc::format!("block {}: `{} {}` names something unknown", rb.id, key, a.join(" ")));
                }
            }
            let d = &blocks[first];
            let states = d.rotations as usize * if d.flips { 2 } else { 1 };
            for b in blocks[first..first + states].iter_mut() {
                b.x = x.clone();
            }
        }
        for ri in raw_items_final.iter() {
            for (key, a) in &ri.1 {
                let word = |k: usize| a.get(k).map(|s| s.as_str()).unwrap_or("");
                match key.as_str() {
                    "turns" => match (by_id.get(word(0)), by_id.get(word(1))) {
                        (Some(&from), Some(&to)) => items[ri.0].x.turns.push((from, to)),
                        _ => warnings.push(alloc::format!("item {}: `turns {}` names an unknown block", items[ri.0].id, a.join(" "))),
                    },
                    "damage" => items[ri.0].x.damage = word(0).parse::<i32>().unwrap_or(0).clamp(0, 1000),
                    // shoots <damage> [speed] [seconds between shots] [ammunition item]
                    "shoots" => {
                        let ammo = if word(3).is_empty() { None } else { items.iter().position(|it| it.id == word(3)) };
                        if !word(3).is_empty() && ammo.is_none() {
                            warnings.push(alloc::format!("item {}: shoots unknown ammunition `{}`", items[ri.0].id, word(3)));
                        }
                        items[ri.0].x.shoots = Some((word(0).parse::<i32>().unwrap_or(4).clamp(0, 500), word(1).parse::<f32>().unwrap_or(32.0).clamp(4.0, 80.0), word(2).parse::<f32>().unwrap_or(0.6).clamp(0.1, 10.0), ammo));
                    }
                    _ => {}
                }
            }
        }

        // Groups: `group <name> <item> [<item> ...]`. Naming one again adds to it.
        let mut groups: Vec<(String, Vec<u16>)> = Vec::new();
        for (at, words) in &raw_groups {
            let mut found = Vec::new();
            for id in &words[1..] {
                match items.iter().position(|it| it.id == *id && it.brush == 0) {
                    Some(i) => found.push(i as u16),
                    None => warnings.push(alloc::format!("{}: group {} names an unknown item {}", at, words[0], id)),
                }
            }
            match groups.iter_mut().find(|g| g.0 == words[0]) {
                Some(g) => g.1.extend(found),
                None => groups.push((words[0].clone(), found)),
            }
        }
        // Recipes: `bake` and `smelt` go to the stations' lists; `recipe`,
        // `craft`, `shapeless` and `shaped` to the crafting grid.
        let mut recipes = Vec::new();
        let mut grid_recipes = Vec::new();
        let find_item = |id: &str| items.iter().position(|it| it.id == id && it.brush == 0).map(|i| i as u16);
        let find_group = |name: &str| groups.iter().find(|g| g.0 == name).map(|g| g.1.clone());
        let lookup = crate::crafting::parse::Lookup { item: &find_item, group: &find_group };
        for (at, words) in &raw_recipes {
            if words[0] != "bake" && words[0] != "smelt" {
                match crate::crafting::parse::resolve(words, &lookup) {
                    Ok(rc) => {
                        // An ingredient that more is wanted of than a stack holds can never be made.
                        let too_much = rc.ingredients().iter().any(|ing| ing.items.iter().all(|&i| items[i as usize].stack.max(1) < ing.need));
                        if too_much {
                            warnings.push(alloc::format!("{}: an ingredient needs more than fits in one stack", at));
                        } else {
                            grid_recipes.push(rc);
                        }
                    }
                    Err(why) => warnings.push(alloc::format!("{}: recipe: {}", at, why)),
                }
                continue;
            }
            let oven = words[0] == "bake";
            let furnace = words[0] == "smelt";
            let words = &words[1..];
            let mut parts = Vec::new();
            let mut ok = words.len() >= 4 && words.len() % 2 == 0;
            for pair in words.chunks(2) {
                if !ok {
                    break;
                }
                let item = items.iter().position(|it| it.id == pair[0] && it.brush == 0);
                match (item, pair.get(1).and_then(|n| n.parse::<u16>().ok())) {
                    (Some(i), Some(n)) if (1..=999).contains(&n) => parts.push((i, n)),
                    _ => ok = false,
                }
            }
            if ok {
                recipes.push(Recipe { out: parts[0], inputs: parts[1..].to_vec(), table: false, oven, furnace });
            } else {
                warnings.push(alloc::format!("{}: recipe needs `<item> <count>` pairs of known items", at));
            }
        }
        let mut entities = Vec::new();
        for e in &raw_entities {
            match models.iter().position(|m| m.id == e.model) {
                Some(model) => entities.push(EntityDef {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    model,
                    height: e.height,
                    width: e.width,
                    speed: e.speed,
                    health: e.health,
                    behavior: e.behavior,
                    damage: e.damage,
                    tint: e.tint,
                    biomes: e.biomes.clone(),
                    night: e.night,
                    weight: e.weight,
                    drop: e.drop.as_ref().and_then(|(id, n)| {
                        let found = items.iter().position(|it| it.id == *id);
                        if found.is_none() {
                            // A creature that drops something a pack lacks still works.
                            warnings.push(alloc::format!("entity {}: drops unknown item `{}`", e.id, id));
                        }
                        found.map(|i| (i, *n))
                    }),
                    x: {
                        let mut x = EntityExtra::default();
                        let item = |id: &str| items.iter().position(|it| it.id == id && it.brush == 0);
                        x.cave = e.biomes.iter().any(|b| b.ends_with("cave"));
                        for (key, a) in &e.extra {
                            let word = |k: usize| a.get(k).map(|s| s.as_str()).unwrap_or("");
                            let mut ok = true;
                            match key.as_str() {
                                "trait" => {
                                    for w in a {
                                        match traits::NAMES.iter().find(|t| t.0 == w.as_str()) {
                                            Some(t) => x.traits |= t.1,
                                            None => ok = false,
                                        }
                                    }
                                }
                                "anim" => match ANIMS.iter().find(|t| t.0 == word(0)) {
                                    Some(t) => x.anim = t.1,
                                    None => ok = false,
                                },
                                // variant <name> [<r> <g> <b>] [model <id>] [size <factor>] [tex <picture>] [biomes <names...>]
                                "variant" => {
                                    let num = |k: usize| word(k).parse::<u8>().ok();
                                    let mut v = Variant { name: word(0).replace('_', " "), model, tint: e.tint, size: 1.0, tex: String::new(), biomes: Vec::new() };
                                    let mut k = 1;
                                    if let (Some(r), Some(g), Some(b)) = (num(1), num(2), num(3)) {
                                        v.tint = [r, g, b];
                                        k = 4;
                                    }
                                    while k < a.len() {
                                        match word(k) {
                                            "model" => match models.iter().position(|m| m.id == word(k + 1)) {
                                                Some(m) => v.model = m,
                                                None => ok = false,
                                            },
                                            "size" => v.size = word(k + 1).parse::<f32>().unwrap_or(1.0).clamp(0.2, 3.0),
                                            "tex" => v.tex = crate::pack_entities::clean_name(word(k + 1)),
                                            "biomes" => {
                                                // Every word up to the next keyword is a biome.
                                                k += 1;
                                                while k < a.len() && !matches!(word(k), "model" | "size" | "tex" | "biomes") {
                                                    v.biomes.push(word(k).to_ascii_lowercase());
                                                    k += 1;
                                                }
                                                continue;
                                            }
                                            _ => ok = false,
                                        }
                                        k += 2;
                                    }
                                    x.variants.push(v);
                                }
                                "baby" => {
                                    let scale = word(0).parse::<f32>().unwrap_or(0.5).clamp(0.2, 0.9);
                                    x.baby = Some((scale, word(1).parse::<f32>().unwrap_or(300.0).max(5.0)));
                                }
                                "gives" => match item(word(0)) {
                                    Some(i) => x.gives = Some((i, word(1).parse::<f32>().unwrap_or(120.0).max(1.0))),
                                    None => ok = false,
                                },
                                "trade" => match (item(word(0)), word(1).parse::<u16>(), item(word(2)), word(3).parse::<u16>()) {
                                    (Some(w), Ok(n), Some(g), Ok(m)) if n > 0 && m > 0 => x.trades.push((w, n, g, m)),
                                    _ => ok = false,
                                },
                                "tex" => x.tex = crate::pack_entities::clean_name(word(0)),
                                "job" => x.job = word(0).to_string(),
                                "likes" => {
                                    x.likes = item(word(0));
                                    ok = x.likes.is_some();
                                }
                                "glow" => x.glow = word(0).parse::<u8>().unwrap_or(8).min(15),
                                "sound" => x.sound = word(0).to_string(),
                                "tamable" => x.tamable = word(0) != "no",
                                "shoots" => x.shoots = word(0).parse::<i32>().unwrap_or(3).clamp(0, 100),
                                "rideable" => x.rideable = word(0) != "no",
                                "hunts" => {
                                    x.hunts.extend(a.iter().cloned());
                                    x.traits |= traits::HUNTER;
                                }
                                // explodes <power> <fuse seconds> <trigger range>
                                "explodes" => {
                                    let f = |k: usize| word(k).parse::<f32>().ok().filter(|v| v.is_finite());
                                    match (f(0), f(1), f(2)) {
                                        (Some(power), Some(fuse), Some(range)) => x.explodes = Some((power.clamp(0.1, 16.0), fuse.clamp(0.2, 30.0), range.clamp(0.5, 16.0))),
                                        _ => ok = false,
                                    }
                                }
                                _ => {}
                            }
                            if !ok {
                                warnings.push(alloc::format!("entity {}: `{} {}` is not understood", e.id, key, a.join(" ")));
                            }
                        }
                        x
                    },
                }),
                None => warnings.push(alloc::format!("entity {}: unknown model `{}`", e.id, e.model)),
            }
        }
        // Sounds, chat commands and scripts are plain lists.
        let mut sounds: Vec<SoundDef> = Vec::new();
        let mut commands: Vec<(String, Vec<String>)> = Vec::new();
        let mut scripts: Vec<Script> = Vec::new();
        let mut dimensions: Vec<(String, u8)> = Vec::new();
        let mut features: Vec<String> = Vec::new();
        let mut episodes: Vec<EpisodeDef> = Vec::new();
        let mut toggles: Vec<(String, bool, String)> = Vec::new();
        for (at, w) in &raw_rules {
            let f = |k: usize| w.get(k).and_then(|v| v.parse::<f32>().ok());
            match w[0].as_str() {
                // sound <id> <effects|steps|ui> <seconds> <noise> <hz from> <hz to> <low-pass> <loudness> [caption...]
                // sound <id> <effects|steps|ui> file <path> [loudness] [caption...]  (read by the parser)
                "sound" if w.get(3).map(|s| s.as_str()) == Some("clip") => match (w.get(1), w.get(2).map(|s| s.as_str())) {
                    (Some(id), Some(bus @ ("effects" | "steps" | "ui"))) => {
                        let samples = clips.iter().find(|c| &c.0 == id).map(|c| c.1.clone());
                        let (loud, from) = match f(4) {
                            Some(v) => (v.clamp(0.0, 2.0), 5),
                            None => (1.0, 4),
                        };
                        let def = SoundDef {
                            id: id.clone(),
                            bus: match bus {
                                "steps" => 1,
                                "ui" => 2,
                                _ => 0,
                            },
                            seconds: samples.as_ref().map_or(0.1, |s| s.len() as f32 / crate::wav::RATE as f32),
                            noise: 0.0,
                            hz: [0.0, 0.0],
                            low_pass: 1.0,
                            loudness: loud,
                            caption: w[from.min(w.len())..].join(" "),
                            clip: samples,
                        };
                        match sounds.iter().position(|s| s.id == def.id) {
                            Some(i) => sounds[i] = def,
                            None => sounds.push(def),
                        }
                    }
                    _ => warnings.push(alloc::format!("{}: sound needs <id> <effects|steps|ui> file <path> [loudness] [caption]", at)),
                },
                "sound" => match (w.get(1), w.get(2).map(|s| s.as_str()), f(3), f(4), f(5), f(6), f(7), f(8)) {
                    (Some(id), Some(bus @ ("effects" | "steps" | "ui")), Some(secs), Some(noise), Some(f0), Some(f1), Some(lp), Some(loud)) => {
                        let def = SoundDef {
                            id: id.clone(),
                            bus: match bus {
                                "steps" => 1,
                                "ui" => 2,
                                _ => 0,
                            },
                            seconds: secs.clamp(0.01, 6.0),
                            noise: noise.clamp(0.0, 1.0),
                            hz: [f0.clamp(10.0, 8000.0), f1.clamp(10.0, 8000.0)],
                            low_pass: lp.clamp(0.005, 1.0),
                            loudness: loud.clamp(0.0, 2.0),
                            caption: w[9.min(w.len())..].join(" "),
                            clip: None,
                        };
                        match sounds.iter().position(|s| s.id == def.id) {
                            Some(i) => sounds[i] = def,
                            None => sounds.push(def),
                        }
                    }
                    _ => warnings.push(alloc::format!("{}: sound needs <id> <effects|steps|ui> <seconds> <noise> <hz> <hz> <low-pass> <loudness> [caption]", at)),
                },
                // command <name> <chat commands separated by ;>
                "command" if w.len() >= 3 => {
                    let name = alloc::format!("/{}", w[1].trim_start_matches('/').to_ascii_lowercase());
                    let actions = split_actions(&w[2..].join(" "));
                    commands.retain(|c| c.0 != name);
                    commands.push((name, actions));
                }
                // when <event> [<what>] do <chat commands separated by ;>
                "when" => match w.iter().position(|x| x == "do") {
                    Some(d) if d >= 2 && d + 1 < w.len() => scripts.push(Script {
                        event: crate::game::events::canonical(&w[1].to_ascii_lowercase()).into(),
                        what: w[2..d].join(" ").to_ascii_lowercase(),
                        actions: split_actions(&w[d + 1..].join(" ")),
                    }),
                    _ => warnings.push(alloc::format!("{}: when needs <event> [what] do <commands>", at)),
                },
                // setting <id> <on|off> <LABEL...>: a switch of the pack's own
                // in Settings > GAME, read by scripts as `setting <id>`.
                "setting" => match (w.get(1), w.get(2).map(|x| x.as_str())) {
                    (Some(id), Some(def @ ("on" | "off"))) if w.len() > 3 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') => {
                        toggles.retain(|t: &(String, bool, String)| &t.0 != id);
                        toggles.push((id.to_ascii_lowercase(), def == "on", w[3..].join(" ").to_ascii_uppercase()));
                    }
                    _ => warnings.push(alloc::format!("{}: setting needs <id> <on|off> <label>", at)),
                },
                // episode <id> [every S] [lasts S] [moves SPEED] [funnel]
                //   [then ID] [if COND [and COND]...] do <commands>
                "episode" => match w.iter().position(|x| x == "do") {
                    Some(d) if d >= 2 && d + 1 < w.len() => {
                        let mut def = EpisodeDef { id: w[1].to_ascii_lowercase(), every: 0, lasts: 10.0, needs: Vec::new(), moves: 0.0, funnel: false, then: String::new(), actions: split_actions(&w[d + 1..].join(" ")) };
                        let mut k = 2;
                        let mut bad = false;
                        while k < d {
                            let val = w.get(k + 1).filter(|_| k + 1 < d).map(|s| s.as_str()).unwrap_or("");
                            match w[k].as_str() {
                                "every" => def.every = val.parse::<u32>().unwrap_or(0).min(1_000_000),
                                "lasts" => def.lasts = val.parse::<f32>().unwrap_or(10.0).clamp(1.0, 600.0),
                                "moves" => def.moves = val.parse::<f32>().unwrap_or(3.5).clamp(0.0, 20.0),
                                "then" => def.then = val.to_ascii_lowercase(),
                                "funnel" => {
                                    def.funnel = true;
                                    k += 1;
                                    continue;
                                }
                                "if" => {
                                    def.needs = w[k + 1..d].join(" ").to_ascii_lowercase().split(" and ").map(|c| String::from(c.trim())).filter(|c| !c.is_empty()).collect();
                                    break;
                                }
                                _ => bad = true,
                            }
                            k += 2;
                        }
                        if bad {
                            warnings.push(alloc::format!("{}: episode options are every, lasts, moves, funnel, then, if", at));
                        } else {
                            episodes.retain(|x: &EpisodeDef| x.id != def.id);
                            episodes.push(def);
                        }
                    }
                    _ => warnings.push(alloc::format!("{}: episode needs <name> [options] do <commands>", at)),
                },
                "command" => warnings.push(alloc::format!("{}: command needs <name> <what it does>", at)),
                "feature" => {
                    for f in &w[1..] {
                        if !features.contains(f) {
                            features.push(f.to_ascii_lowercase());
                        }
                    }
                }
                // dimension <id> <normal|flat|void|islands>
                "dimension" => {
                    let id = w.get(1).map(|s| s.to_ascii_lowercase()).unwrap_or_default();
                    let shape = crate::worldgen::SHAPES.iter().position(|s| Some(*s) == w.get(2).map(|x| x.as_str()));
                    match shape {
                        Some(k) if !id.is_empty() && id.len() <= 12 && id.chars().all(|c| c.is_ascii_alphanumeric()) => {
                            dimensions.retain(|d| d.0 != id);
                            dimensions.push((id, k as u8));
                        }
                        _ => warnings.push(alloc::format!("{}: dimension needs <name of letters and digits> <normal|flat|void|islands>", at)),
                    }
                }
                _ => {}
            }
        }
        let raw_rules: Vec<(String, Vec<String>)> = raw_rules.into_iter().filter(|r| r.1[0] == "ore" || r.1[0] == "plant").collect();
        // An egg for every creature, to set one down in creative mode (or
        // as a prize): `<id>_egg`. Its picture is the creature itself.
        for (k, e) in entities.iter().enumerate() {
            let id = alloc::format!("{}_egg", e.id);
            if items.iter().any(|it| it.id == id) || items.len() >= 60_000 {
                continue;
            }
            items.push(ItemDef {
                id,
                name: alloc::format!("{} Egg", e.name),
                icon: 0,
                place: None,
                brush: 0,
                reach: 0.0,
                stack: 64,
                tool: 0,
                speed: 1.0,
                food: 0,
                section: None,
                category: String::from("Eggs"),
                x: ItemExtra { egg: Some(k), ..ItemExtra::default() },
            });
        }
        // What packs add to the terrain.
        let (mut ores, mut plants) = (Vec::new(), Vec::new());
        for (at, w) in &raw_rules {
            let block = w.get(1).and_then(|id| by_id.get(id.as_str()).copied());
            let n = |k: usize| w.get(k).and_then(|v| v.parse::<i32>().ok());
            match (w[0].as_str(), block) {
                // ore <block> <lowest> <highest> <one in N>
                ("ore", Some(b)) => match (n(2), n(3), n(4)) {
                    (Some(low), Some(high), Some(rarity)) if low <= high && rarity >= 1 => ores.push(OreRule { block: b, low, high, one_in: rarity as u32 }),
                    _ => warnings.push(alloc::format!("{}: ore needs <block> <lowest> <highest> <one in N>", at)),
                },
                // plant <block> <one in N> [biomes...]
                ("plant", Some(b)) => match n(2) {
                    Some(rarity) if rarity >= 1 => plants.push(PlantRule { block: b, one_in: rarity as u32, biomes: w[3..].iter().map(|s| s.to_ascii_lowercase()).collect() }),
                    _ => warnings.push(alloc::format!("{}: plant needs <block> <one in N> [biomes]", at)),
                },
                _ => warnings.push(alloc::format!("{}: unknown block `{}`", at, w.get(1).map(|s| s.as_str()).unwrap_or(""))),
            }
        }
        let mut categories: Vec<String> = Vec::new();
        for it in &items {
            if !categories.contains(&it.category) {
                categories.push(it.category.clone());
            }
        }

        let mut get = |id: &str| -> Block {
            match by_id.get(id) {
                Some(&b) => b,
                None => {
                    warnings.push(alloc::format!("the terrain generator needs a block called `{}`", id));
                    by_id.get("stone").copied().unwrap_or(AIR)
                }
            }
        };
        let wk = WellKnown {
            stone: get("stone"),
            dirt: get("dirt"),
            grass: get("grass"),
            sand: get("sand"),
            gravel: get("gravel"),
            water: get("water"),
            lava: get("lava"),
            log: get("log"),
            leaves: get("leaves"),
            coal_ore: get("coal_ore"),
            iron_ore: get("iron_ore"),
            copper_ore: get("copper_ore"),
            crystal_ore: get("crystal_ore"),
            snow: get("snow"),
            snowy_grass: get("snowy_grass"),
            ice: get("ice"),
            sandstone: get("sandstone"),
            cactus: get("cactus"),
            flower_red: get("flower_red"),
            flower_yellow: get("flower_yellow"),
            tall_grass: get("tall_grass"),
            pine_log: get("pine_log"),
            pine_leaves: get("pine_leaves"),
            deepstone: get("deepstone"),
            clay: get("clay"),
            // Optional: a pack without these woods just grows oak instead.
            birch_log: AIR,
            birch_leaves: AIR,
            cactus_arm: AIR,
            cactus_arm_z: AIR,
            cactus_joint: [AIR; 4],
            acacia_log: AIR,
            acacia_leaves: AIR,
            dark_oak_log: AIR,
            dark_oak_leaves: AIR,
            maple_log: AIR,
            maple_leaves: AIR,
            palm_log: AIR,
            palm_leaves: AIR,
            blossom_log: AIR,
            blossom_leaves: AIR,
            ruby_ore: AIR,
            snow_layer: AIR,
            moss: AIR,
            glow_mushroom: AIR,
            mushroom: AIR,
            crystal_cluster: AIR,
            icicle: AIR,
            magma: AIR,
            basalt: AIR,
            cave_vine: AIR,
            stalagmite: AIR,
            web: AIR,
            farmland: AIR,
            crops: [AIR; 4],
            asphalt: AIR,
            paint: [AIR; 2],
            floor: AIR,
            stairs: AIR,
            furniture: [AIR; 5],
            walls: [AIR; 4],
            window: AIR,
            bands: [AIR; 4],
        };
        let mut wk = wk;
        let or = |id: &str, fallback: Block| by_id.get(id).copied().unwrap_or(fallback);
        wk.cactus_arm = or("cactus_arm", wk.cactus);
        wk.cactus_arm_z = or("cactus_arm_z", wk.cactus);
        wk.cactus_joint = [or("cactus_joint_px", wk.cactus), or("cactus_joint_nx", wk.cactus), or("cactus_joint_pz", wk.cactus), or("cactus_joint_nz", wk.cactus)];
        wk.birch_log = or("birch_log", wk.log);
        wk.birch_leaves = or("birch_leaves", wk.leaves);
        wk.acacia_log = or("acacia_log", wk.log);
        wk.acacia_leaves = or("acacia_leaves", wk.leaves);
        wk.dark_oak_log = or("dark_oak_log", wk.log);
        wk.dark_oak_leaves = or("dark_oak_leaves", wk.leaves);
        wk.maple_log = or("maple_log", wk.log);
        wk.maple_leaves = or("maple_leaves", wk.leaves);
        wk.palm_log = or("palm_log", wk.log);
        wk.palm_leaves = or("palm_leaves", wk.leaves);
        wk.blossom_log = or("blossom_log", wk.log);
        wk.blossom_leaves = or("blossom_leaves", wk.leaves);
        wk.ruby_ore = or("ruby_ore", wk.stone);
        wk.snow_layer = or("snow_layer", AIR);
        wk.moss = or("moss", AIR);
        wk.glow_mushroom = or("glow_mushroom", AIR);
        wk.mushroom = or("brown_mushroom", AIR);
        wk.crystal_cluster = or("crystal_cluster", AIR);
        wk.icicle = or("icicle", AIR);
        wk.magma = or("magma", AIR);
        wk.basalt = or("basalt", wk.stone);
        wk.cave_vine = or("cave_vine", AIR);
        wk.stalagmite = or("stalagmite", AIR);
        wk.web = or("cobweb", AIR);
        wk.farmland = or("farmland", AIR);
        wk.crops = [or("wheat_4", AIR), or("carrot_4", AIR), or("potato_4", AIR), or("tomato_4", AIR)];
        wk.asphalt = or("gray_concrete", wk.gravel);
        wk.paint = [or("yellow_concrete", wk.asphalt), or("white_concrete", wk.asphalt)];
        wk.floor = or("planks", wk.stone);
        wk.stairs = or("planks_stairs", AIR);
        wk.furniture = [or("planks_table", AIR), or("oak_chest", AIR), or("crafting_table", AIR), or("red_bed", AIR), or("lamp", AIR)];
        wk.walls = [or("light_gray_concrete", wk.stone), or("brick", wk.stone), or("white_concrete", wk.stone), or("stone_bricks", wk.stone)];
        wk.window = or("light_blue_glass", or("glass", wk.stone));
        wk.bands = [
            or("orange_terracotta", wk.sandstone),
            or("red_terracotta", wk.sandstone),
            or("yellow_terracotta", wk.sandstone),
            or("white_terracotta", wk.sandstone),
        ];
        let find_tile = |id: &str| tiles.iter().position(|t| t.id == id).unwrap_or(0) as u16;

        let fluid: Vec<Option<(bool, u8)>> = blocks.iter().map(|b| b.x.fluid).collect();
        let mut flow = [[AIR; 9]; 2];
        for (state, f) in fluid.iter().enumerate() {
            if let Some((lava, level)) = f {
                flow[*lava as usize][*level as usize] = state as Block;
            }
        }
        let fx = crate::content::effects::build(&raw_effects, &tiles, &mut warnings);
        let mut registry = Registry {
            fluid,
            flow,
            opaque: blocks.iter().map(|b| (b.kind == Kind::Cube) as u8).collect(),
            emit: blocks.iter().map(|b| b.emit).collect(),
            sky_loss: blocks.iter().map(|b| b.sky_loss).collect(),
            kind: blocks.iter().map(|b| b.kind).collect(),
            sun_tile: find_tile("sun"),
            moon_tile: find_tile("moon"),
            wk,
            part: blocks.iter().position(|b| b.x.part).map_or(AIR, |i| i as Block),
            blocks,
            items,
            tiles,
            recipes,
            grid_recipes,
            models,
            entities,
            sections,
            fx,
            lines,
            categories,
            ores,
            sounds,
            commands,
            scripts,
            episodes,
            toggles,
            dimensions,
            features,
            plants,
            by_id,
            warnings,
        };
        // The mods' own code looks over what was kept for it.
        for n in crate::native::all() {
            if registry.has(n.name) {
                if let Some(check) = n.check {
                    let found = check(&registry);
                    registry.warnings.extend(found);
                }
            } else if let Some(sec) = registry.sections.iter().find(|x| n.sections.contains(&x.kind.as_str())) {
                registry.warnings.push(alloc::format!("{}: `{}` needs the mod `{}`, which is switched off or missing", sec.at, sec.kind, n.name));
            }
        }
        registry
    }

    /// First state of the block with this data-file id.
    /// Whether a pack has switched this part of the game on.
    pub fn has(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }

    pub fn block(&self, id: &str) -> Option<Block> {
        self.by_id.get(id).copied()
    }

    /// Name of a state for save files, e.g. `stone_stairs` or `stone_stairs@5`.
    pub fn state_name(&self, b: Block) -> String {
        let d = &self.blocks[b as usize];
        if b == d.first {
            d.id.clone()
        } else {
            alloc::format!("{}@{}", d.id, b - d.first)
        }
    }

    pub fn state_by_name(&self, name: &str) -> Option<Block> {
        let (id, off) = match name.find('@') {
            Some(i) => (&name[..i], name[i + 1..].parse::<u16>().ok()?),
            None => (name, 0),
        };
        let first = self.block(id)?;
        let d = &self.blocks[first as usize];
        let states = d.rotations as u16 * if d.flips { 2 } else { 1 };
        if off < states {
            Some(first + off)
        } else {
            Some(first)
        }
    }

    /// The state to place for block `first`, given which way the player
    /// faces (quarter turns, 0 = looking down -Z) and whether they aimed at
    /// the upper half of a cell.
    pub fn placed_state(&self, first: Block, facing: u8, upper: bool) -> Block {
        let d = &self.blocks[first as usize];
        let mut s = d.first;
        if d.rotations == 4 {
            s += (facing % 4) as Block;
        }
        if d.flips && upper {
            s += d.rotations as Block;
        }
        s
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_better_the_ore_the_harder_the_tool_hits() {
        let damage = |id: &str| reg().items.iter().find(|i| i.id == id).unwrap().x.damage;
        for kind in ["pick", "axe", "shovel"] {
            let tiers = ["wood", "stone", "copper", "iron", "crystal", "ruby"].map(|m| damage(&alloc::format!("{m}_{kind}")));
            assert!(tiers.windows(2).all(|w| w[0] < w[1]), "{kind}: {tiers:?}");
        }
        let swords = ["wood", "copper", "ruby"].map(|m| damage(&alloc::format!("{m}_sword")));
        assert!(swords.windows(2).all(|w| w[0] < w[1]), "swords: {swords:?}");
    }

    use super::*;

    #[test]
    fn packs_define_episodes_and_settings() {
        let pack = "setting storms off Wild weather\n\
                    episode hail every 300 lasts 20 then thaw if setting storms and storm do /hurt 1; /sound pop\n\
                    episode thaw lasts 5 moves 2 funnel do /say thaw\n\
                    episode bad sometimes do /say hi\n\
                    setting nolabel on\n";
        let p = Registry::from_sources(&[("base.txt", BASE_DATA), ("p", pack)], &mut |_| None);
        assert_eq!(p.episodes.len(), 2);
        let hail = &p.episodes[0];
        assert_eq!((hail.id.as_str(), hail.every, hail.lasts, hail.then.as_str()), ("hail", 300, 20.0, "thaw"));
        assert_eq!(hail.needs, ["setting storms", "storm"]);
        assert_eq!(hail.actions, ["/hurt 1", "/sound pop"]);
        let thaw = &p.episodes[1];
        assert!(thaw.funnel && thaw.moves == 2.0 && thaw.every == 0, "one with no `every` never comes by itself");
        assert_eq!(p.toggles, [(String::from("storms"), false, String::from("WILD WEATHER"))]);
        assert_eq!(p.warnings.len(), 2, "{:?}", p.warnings);
    }

    #[test]
    fn base_data_loads_cleanly() {
        let r = reg();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let stone = r.block("stone").unwrap();
        assert_eq!(r.kind[stone as usize], Kind::Cube);
        assert_eq!(r.opaque[stone as usize], 1);
        assert_eq!(r.emit[r.block("lamp").unwrap() as usize], 15);
        assert_eq!(r.sky_loss[r.block("leaves").unwrap() as usize], 1);
        assert!(!is_solid(r.block("water").unwrap()));
        assert!(!is_solid(r.block("tall_grass").unwrap()));
        // Area items (a brush) are still possible in packs: no built-in one.
        let brush = Registry::from_sources(&[("p", "item ball_wand\n  icon stone\n  brush 2\n  reach 12\n  stack 1\n")], &mut |_| None);
        assert!(brush.items.iter().any(|i| i.id == "ball_wand" && i.brush == 2 && i.reach == 12.0));
        assert!(r.items.iter().all(|i| i.id != "air"));
    }

    #[test]
    fn stairs_expand_to_eight_states() {
        let r = reg();
        let s = r.block("stone_stairs").unwrap();
        let d = &r.blocks[s as usize];
        assert_eq!((d.rotations, d.flips), (4, true));
        // Facing 0: the high step is on the -Z side.
        assert_eq!(d.boxes[1], Bx { min: [0, 8, 0], max: [16, 16, 8] });
        // Facing 1 (player looks toward -X): high step on the -X side.
        assert_eq!(r.blocks[s as usize + 1].boxes[1], Bx { min: [0, 8, 0], max: [8, 16, 16] });
        // Flipped: the slab part is on top.
        assert_eq!(r.blocks[s as usize + 4].boxes[0], Bx { min: [0, 8, 0], max: [16, 16, 16] });
        assert_eq!(r.placed_state(s, 3, true), s + 7);
        assert_eq!(r.state_by_name(&r.state_name(s + 5)), Some(s + 5));
        // The next block starts after all eight states.
        assert_eq!(r.block("cobble_stairs").unwrap(), s + 8);
    }

    #[test]
    fn packs_add_and_override() {
        let pack = "texture ruby flat 200 20 40 noise 3\n\
                    block ruby\n name Ruby Block\n tex ruby\n light 4\n\
                    block stone\n name Hard Stone\n tex stone\n\
                    block step\n tex ruby\n box 0 0 0 16 4 16\n\
                    item ruby_wand\n name Ruby Wand\n icon ruby\n place ruby\n brush 1\n\
                    block broken\n tex nope\n bogus 1\n";
        let r = Registry::from_sources(&[("base.txt", BASE_DATA), ("pack.txt", pack)], &mut |_| None);
        let ruby = r.block("ruby").unwrap();
        assert_eq!(r.blocks[ruby as usize].emit, 4);
        // Overriding keeps the original position, so old saves stay valid.
        assert_eq!(r.block("stone"), reg().block("stone"));
        assert_eq!(r.blocks[r.block("stone").unwrap() as usize].name, "Hard Stone");
        let step = &r.blocks[r.block("step").unwrap() as usize];
        assert_eq!(step.kind, Kind::Boxes);
        assert_eq!(r.opaque[step.first as usize], 0);
        // The block keeps one plain item of its own beside any area tool.
        assert_eq!(r.items.iter().filter(|i| i.place == Some(ruby) && i.brush == 0).count(), 1);
        assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
    }

    #[test]
    fn fences_and_walls_connect_to_cubes_and_their_own_kind() {
        let r = reg();
        let (fence, wall) = (r.block("planks_fence").unwrap(), r.block("stone_wall").unwrap());
        let stone = r.wk.stone;
        // A fence with stone to the north and another fence to the east.
        let mask = connect_mask(fence, &|dx, dz| match (dx, dz) {
            (0, -1) => stone,
            (1, 0) => fence,
            (0, 1) => wall,
            _ => AIR,
        });
        assert_eq!(mask, 0b1001);
        let mut out = Vec::new();
        shape(fence, mask, &mut out);
        // Post, two rails north, two rails east.
        assert_eq!(out.len(), 5);
        assert_eq!(out[1], Bx { min: [7, 6, 0], max: [9, 9, 6] });
        assert_eq!(out[3], Bx { min: [10, 6, 7], max: [16, 9, 9] });
        shape(stone, 0b1111, &mut out);
        assert_eq!(out, [Bx::FULL]);
    }

    #[test]
    fn a_sound_can_come_from_a_wav_file() {
        let pcm: Vec<u8> = (0..2205i16).flat_map(|i| (i * 10).to_le_bytes()).collect();
        let wav = crate::wav::make_wav(22050, 1, 16, &pcm);
        let r = Registry::from_sources(
            &[("mods/x/pack.txt", "sound ding ui file sounds/ding.wav 0.8 A ding\nsound broken effects file missing.wav\nsound hiss effects 0.2 1 100 100 1 1\n")],
            &mut |p| if p == "mods/x/sounds/ding.wav" { Some(wav.clone()) } else { None },
        );
        let ding = r.sounds.iter().find(|s| s.id == "ding").expect("ding");
        assert_eq!((ding.bus, ding.loudness, ding.caption.as_str()), (2, 0.8, "A ding"));
        assert_eq!(ding.clip.as_ref().map(|c| c.len()), Some(2205));
        assert!((ding.seconds - 0.1).abs() < 0.001);
        // A file that cannot be read is a warning, not a sound; made sounds are as before.
        assert!(r.sounds.iter().all(|s| s.id != "broken"));
        assert!(r.warnings.iter().any(|w| w.contains("missing.wav")), "{:?}", r.warnings);
        assert!(r.sounds.iter().find(|s| s.id == "hiss").unwrap().clip.is_none());
    }

    #[test]
    fn drops_recipes_and_categories_come_from_data() {
        let r = reg();
        let item = |id: &str| r.items.iter().position(|i| i.id == id).unwrap();
        assert_eq!(def(r.wk.stone).drop, Some(item("cobble")));
        assert_eq!(def(r.block("glass").unwrap()).drop, None);
        assert_eq!(def(r.block("planks").unwrap()).drop, Some(item("planks")));
        // Every state of a block shares its drop.
        let stairs = r.block("stone_stairs").unwrap();
        assert_eq!(def(stairs + 7).drop, Some(item("stone_stairs")));
        let planks = r.grid_recipes.iter().find(|x| x.out.0 as usize == item("planks")).expect("no planks recipe");
        assert_eq!(planks.out.1, 4);
        assert_eq!(planks.ingredients().iter().map(|i| i.items.clone()).collect::<Vec<_>>(), [alloc::vec![item("log") as u16]]);
        assert!(r.categories.iter().any(|c| c == "Colours"));
        assert!(r.kind[r.block("red_glass").unwrap() as usize] == Kind::Translucent);
        let bad = Registry::from_sources(&[("base.txt", BASE_DATA), ("p", "recipe planks 4 nothing_real 1\n")], &mut |_| None);
        assert_eq!(bad.warnings.len(), 1);
    }

    #[test]
    fn includes_are_resolved_relative_to_the_pack() {
        let mut asked = Vec::new();
        let r = Registry::from_sources(&[("base.txt", BASE_DATA), ("mods/pack.txt", "include extra.txt\n")], &mut |p| {
            asked.push(p.to_string());
            Some(b"block extra\n tex stone\n".to_vec())
        });
        assert_eq!(asked, ["mods/extra.txt"]);
        assert!(r.block("extra").is_some());
    }

    #[test]
    fn variants_name_their_biomes_and_creatures_can_explode() {
        let pack = "model m builtin pig\nentity bang\n  name Bang\n  model m\n  explodes 2.5 1.5 3\n  variant Dry biomes desert savanna\n  variant Wet biomes swamp model m size 1.2 tex x\n  variant Odd\nentity dud\n  name Dud\n  model m\n  explodes 2\n  variant Bad model nothing_real\n";
        let r = Registry::from_sources(&[("base.txt", BASE_DATA), ("p", pack)], &mut |_| None);
        let e = r.entities.iter().find(|e| e.id == "bang").expect("bang");
        assert_eq!(e.x.explodes, Some((2.5, 1.5, 3.0)));
        assert_eq!(e.x.variants.len(), 3);
        assert_eq!(e.x.variants[0].biomes, ["desert", "savanna"]);
        assert_eq!((e.x.variants[1].biomes.as_slice(), e.x.variants[1].size), (&["swamp".to_string()][..], 1.2));
        assert!(e.x.variants[2].biomes.is_empty());
        let dud = r.entities.iter().find(|e| e.id == "dud").expect("dud");
        assert_eq!(dud.x.explodes, None);
        assert!(r.warnings.iter().any(|w| w.contains("explodes 2")), "{:?}", r.warnings);
    }
}
