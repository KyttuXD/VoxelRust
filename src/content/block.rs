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
}

/// Something a pack makes happen: when `event` occurs (to `what`, if
/// given), the chat commands in `actions` are carried out.
#[derive(Clone, Debug)]
pub struct Script {
    pub event: String,
    pub what: String,
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
    /// Slows whoever walks through it (percent of normal speed, 0 = no change).
    pub slow: u8,
    /// The sound of breaking or placing it, if not the usual ones.
    pub sound: String,
    /// What sitting on it lifts the player to, in sixteenths (0 = not a seat).
    pub seat: u8,
    /// Food: right click eats it where it stands, for this many points.
    pub eat: u8,
    /// Rail carts run on it, at this percentage of their speed (0 = not track).
    pub track: u16,
    /// Track where carts pull up and wait this many seconds before going on.
    pub station: u8,
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
    /// The vehicle this item sets down, if it is one.
    pub vehicle: Option<usize>,
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
}

/// The parts of a creature that only some have.
#[derive(Clone, Debug, Default)]
pub struct EntityExtra {
    pub traits: u32,
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
}

/// What a vehicle travels on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Medium {
    Water,
    Rail,
    Land,
    Air,
}

/// A kind of vehicle. Each one also gets an item that sets it down.
#[derive(Clone, Debug)]
pub struct VehicleDef {
    pub id: String,
    pub name: String,
    pub model: usize,
    pub medium: Medium,
    /// Top speed, blocks per second.
    pub speed: f32,
    /// Size in blocks; the model is scaled to `height`.
    pub height: f32,
    pub width: f32,
    /// Whether it carries a chest's worth of storage.
    pub storage: bool,
    /// Where the rider's eyes are, above the vehicle's base.
    pub seat: f32,
    /// Keeps going by itself once set moving (a locomotive).
    pub powered: bool,
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
    /// Opens the full recipe list.
    Craft,
    /// Opens its own storage.
    Chest,
    /// Opens the baking recipes.
    Oven,
    /// Sleep through the night; wake up here after dying.
    Bed,
    /// Opens the smelting recipes.
    Furnace,
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

/// Something that can be made by hand from other items.
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    /// Item made, and how many.
    pub out: (usize, u16),
    /// Items used up.
    pub inputs: Vec<(usize, u16)>,
    /// Needs a crafting table (otherwise it can be made by hand).
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
    pub maple_log: Block,
    pub maple_leaves: Block,
    pub palm_log: Block,
    pub palm_leaves: Block,
    pub blossom_log: Block,
    pub blossom_leaves: Block,
    pub ruby_ore: Block,
    pub snow_layer: Block,
    pub rail: Block,
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
    /// Ripe crops, for the fields of Earth worlds (AIR where missing).
    pub crops: [Block; 4],
    /// Road surface, its yellow and white paint, and what towns are
    /// furnished with: floor, stairs, table, chest, workbench, bed, lamp.
    pub asphalt: Block,
    pub paint: [Block; 2],
    pub floor: Block,
    pub stairs: Block,
    pub furniture: [Block; 5],
    /// Building materials for Earth worlds, and their window glass.
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
    pub recipes: Vec<Recipe>,
    pub models: Vec<ModelSpec>,
    pub entities: Vec<EntityDef>,
    pub vehicles: Vec<VehicleDef>,
    /// Item tabs, in the order they first appear.
    pub categories: Vec<String>,
    /// Things packs ask the terrain generator to add.
    pub ores: Vec<OreRule>,
    /// Every sound the game can make, by name; packs may retune or add.
    pub sounds: Vec<SoundDef>,
    /// Commands packs add to the chat, and things they make happen.
    pub commands: Vec<(String, Vec<String>)>,
    pub scripts: Vec<Script>,
    pub plants: Vec<PlantRule>,
    /// Per-state lookup tables for the hot loops.
    pub opaque: Vec<u8>,
    pub emit: Vec<u8>,
    pub sky_loss: Vec<u8>,
    pub kind: Vec<Kind>,
    pub wk: WellKnown,
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
    install(Registry::from_sources(&[("base.txt", BASE_DATA)], &mut |_| None));
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
    n.kind == Kind::Cube || n.connect_group == d.connect_group
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
    Vehicle(usize),
}

#[derive(Clone)]
struct RawVehicle {
    id: String,
    name: String,
    model: String,
    icon: String,
    medium: Medium,
    speed: f32,
    height: f32,
    width: f32,
    storage: bool,
    powered: bool,
    seat: f32,
}

struct Parser<'a> {
    tiles: Vec<TileDef>,
    blocks: Vec<RawBlock>,
    items: Vec<RawItem>,
    models: Vec<ModelSpec>,
    entities: Vec<RawEntity>,
    vehicles: Vec<RawVehicle>,
    /// `(file:line, words)` for each `recipe` line, resolved once items exist.
    recipes: Vec<(String, Vec<String>)>,
    /// `ore` and `plant` lines, kept the same way.
    rules: Vec<(String, Vec<String>)>,
    warnings: Vec<String>,
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
            let line = match raw.find('#') {
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
            match key {
                "texture" => {
                    cur = Cur::None;
                    self.texture(file, line_no, &args);
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
                "model" if !(matches!(cur, Cur::Entity(_) | Cur::Vehicle(_)) && args.len() == 1) => {
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
                "vehicle" => {
                    let id = match args.first() {
                        Some(id) => id.to_string(),
                        None => {
                            self.warn(file, line_no, "missing id");
                            cur = Cur::None;
                            continue;
                        }
                    };
                    let fresh = RawVehicle {
                        id: id.clone(),
                        name: id.clone(),
                        model: String::new(),
                        icon: String::new(),
                        medium: Medium::Land,
                        speed: 6.0,
                        height: 1.0,
                        width: 1.2,
                        storage: false,
                        powered: false,
                        seat: 0.9,
                    };
                    let i = match self.vehicles.iter().position(|v| v.id == id) {
                        Some(i) => {
                            self.vehicles[i] = fresh;
                            i
                        }
                        None => {
                            self.vehicles.push(fresh);
                            self.vehicles.len() - 1
                        }
                    };
                    cur = Cur::Vehicle(i);
                }
                // (Inside a block or creature, `sound <id>` names the sound it makes.)
                "ore" | "plant" | "sound" | "command" | "when" if !(key == "sound" && args.len() == 1 && !matches!(cur, Cur::None)) => {
                    cur = Cur::None;
                    let mut words: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                    words.insert(0, key.to_string());
                    self.rules.push((alloc::format!("{}:{}", file, line_no), words));
                }
                "recipe" | "craft" | "bake" | "smelt" => {
                    cur = Cur::None;
                    // `craft` recipes need a crafting table; the marker is
                    // carried as a first word and removed when resolving.
                    let mut words: Vec<String> = args.iter().map(|a| a.to_string()).collect();
                    words.insert(0, key.to_string());
                    self.recipes.push((alloc::format!("{}:{}", file, line_no), words));
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
                    Cur::Vehicle(i) => {
                        let v = &mut self.vehicles[i];
                        let f = |lo: f32, hi: f32| args.first().and_then(|s| s.parse::<f32>().ok()).filter(|x| (lo..=hi).contains(x));
                        let mut bad = None;
                        match key {
                            "name" => v.name = rest.clone(),
                            "model" => v.model = args.first().unwrap_or(&"").to_string(),
                            "icon" => v.icon = args.first().unwrap_or(&"").to_string(),
                            "medium" => match args.first() {
                                Some(&"water") => v.medium = Medium::Water,
                                Some(&"rail") => v.medium = Medium::Rail,
                                Some(&"land") => v.medium = Medium::Land,
                                Some(&"air") => v.medium = Medium::Air,
                                _ => bad = Some("medium must be water, rail, land or air"),
                            },
                            "speed" => match f(0.5, 60.0) {
                                Some(x) => v.speed = x,
                                None => bad = Some("speed must be 0.5..60"),
                            },
                            "height" => match f(0.2, 6.0) {
                                Some(x) => v.height = x,
                                None => bad = Some("height must be 0.2..6"),
                            },
                            "width" => match f(0.2, 3.0) {
                                Some(x) => v.width = x,
                                None => bad = Some("width must be 0.2..3"),
                            },
                            "seat" => match f(0.0, 5.0) {
                                Some(x) => v.seat = x,
                                None => bad = Some("seat must be 0..5"),
                            },
                            "storage" => v.storage = !matches!(args.first(), Some(&"no") | Some(&"false") | Some(&"0")),
                            "powered" => v.powered = !matches!(args.first(), Some(&"no") | Some(&"false") | Some(&"0")),
                            _ => bad = Some("unknown vehicle property"),
                        }
                        if let Some(msg) = bad {
                            self.warn(file, line_no, &alloc::format!("{} (`{}`)", msg, key));
                        }
                    }
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
            "paint" => {
                let num = |i: usize| a.get(i).and_then(|s| s.parse::<u8>().ok());
                match (num(3), num(4), num(5)) {
                    (Some(r), Some(g), Some(b)) if crate::textures::painted(a[2], [r, g, b], 0, 0).is_some() => {
                        TileSrc::Paint { style: a[2].to_string(), rgb: [r, g, b] }
                    }
                    _ => {
                        self.warn(file, line, "paint needs a known style and three numbers 0..255");
                        return;
                    }
                }
            }
            "file" => {
                let path = alloc::format!("{}{}", dir_of(file), a[2..].join(" "));
                match (self.load)(&path).and_then(|b| crate::textures::decode_tga(&b)) {
                    Some(px) => TileSrc::Pixels(px),
                    None => {
                        self.warn(file, line, &alloc::format!("cannot read {} as a 16x16 TGA", path));
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
                "falls" | "tall" | "toggle" | "grows" | "on" | "also" | "hurts" | "slow" | "sound" | "seat" | "eat" | "track" | "station" => {
                    b.extra.retain(|e| e.0 != key);
                    b.extra.push((key.to_string(), a.iter().map(|s| s.to_string()).collect()));
                }
                "use" => match a.first() {
                    Some(&"craft") => b.usable = Usable::Craft,
                    Some(&"chest") => b.usable = Usable::Chest,
                    Some(&"oven") => b.usable = Usable::Oven,
                    Some(&"furnace") => b.usable = Usable::Furnace,
                    Some(&"bed") => b.usable = Usable::Bed,
                    _ => bad = Some("use must be craft, chest, oven, furnace or bed"),
                },
                "category" => b.category = rest.to_string(),
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
                "trait" | "anim" | "variant" | "baby" | "gives" | "trade" | "job" | "likes" | "glow" | "sound" | "tamable" | "shoots" => {
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
            vehicles: Vec::new(),
            recipes: Vec::new(),
            rules: Vec::new(),
            warnings: Vec::new(),
            load,
            depth: 0,
        };
        for (path, text) in sources {
            p.parse(path, text);
        }
        let Parser { tiles, blocks: raw_blocks, items: raw_items, recipes: raw_recipes, rules: raw_rules, models, entities: raw_entities, vehicles: raw_vehicles, mut warnings, .. } = p;

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

        for rb in &raw_blocks {
            let bx = |a: [u8; 3], b: [u8; 3]| Bx { min: a, max: b };
            let (arms, connect_group) = match rb.shape.as_str() {
                "fence" => (alloc::vec![bx([7, 6, 0], [9, 9, 6]), bx([7, 12, 0], [9, 15, 6])], 1),
                "wall" => (alloc::vec![bx([5, 0, 0], [11, 14, 4])], 2),
                _ => (Vec::new(), 0),
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
                    vehicle: None,
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
                vehicle: None,
                category: if ri.category.is_empty() { String::from("Tools") } else { ri.category.clone() },
                x: ItemExtra::default(),
            });
        }

        // Vehicles, and the item that sets each one down.
        let mut vehicles = Vec::new();
        for rv in &raw_vehicles {
            let model = match models.iter().position(|m| m.id == rv.model) {
                Some(m) => m,
                None => {
                    warnings.push(alloc::format!("vehicle {}: unknown model `{}`", rv.id, rv.model));
                    continue;
                }
            };
            let what = alloc::format!("vehicle {}", rv.id);
            items.push(ItemDef {
                id: rv.id.clone(),
                name: rv.name.clone(),
                icon: tile_of(&rv.icon, &mut warnings, &what),
                place: None,
                brush: 0,
                reach: 0.0,
                stack: 1,
                tool: 0,
                speed: 1.0,
                food: 0,
                vehicle: Some(vehicles.len()),
                category: String::from("Travel"),
                x: ItemExtra::default(),
            });
            vehicles.push(VehicleDef {
                id: rv.id.clone(),
                name: rv.name.clone(),
                model,
                medium: rv.medium,
                speed: rv.speed,
                height: rv.height,
                width: rv.width,
                storage: rv.storage,
                powered: rv.powered,
                seat: rv.seat,
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
                    "slow" => x.slow = word(0).parse::<u8>().unwrap_or(50).clamp(5, 100),
                    "sound" => x.sound = word(0).to_string(),
                    "seat" => x.seat = word(0).parse::<u8>().unwrap_or(8).clamp(1, 16),
                    "eat" => x.eat = word(0).parse::<u8>().unwrap_or(4).clamp(1, 40),
                    "track" => x.track = word(0).parse::<u16>().unwrap_or(100).clamp(10, 400),
                    "station" => {
                        x.station = word(0).parse::<u8>().unwrap_or(5).clamp(1, 120);
                        if x.track == 0 {
                            x.track = 100;
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

        // Recipes: `recipe <item> <n> <ingredient> <n> [<ingredient> <n> ...]`.
        let mut recipes = Vec::new();
        for (at, words) in &raw_recipes {
            let table = words[0] == "craft";
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
                recipes.push(Recipe { out: parts[0], inputs: parts[1..].to_vec(), table, oven, furnace });
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
                    drop: e.drop.as_ref().and_then(|(id, n)| items.iter().position(|it| it.id == *id).map(|i| (i, *n))),
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
                                // variant <name> <r> <g> <b> [model <id>] [size <factor>]
                                "variant" => {
                                    let num = |k: usize| word(k).parse::<u8>().ok();
                                    let mut v = Variant { name: word(0).replace('_', " "), model, tint: e.tint, size: 1.0 };
                                    let mut k = 1;
                                    if let (Some(r), Some(g), Some(b)) = (num(1), num(2), num(3)) {
                                        v.tint = [r, g, b];
                                        k = 4;
                                    }
                                    while k + 1 < a.len() {
                                        match word(k) {
                                            "model" => match models.iter().position(|m| m.id == word(k + 1)) {
                                                Some(m) => v.model = m,
                                                None => ok = false,
                                            },
                                            "size" => v.size = word(k + 1).parse::<f32>().unwrap_or(1.0).clamp(0.2, 3.0),
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
                                "job" => x.job = word(0).to_string(),
                                "likes" => {
                                    x.likes = item(word(0));
                                    ok = x.likes.is_some();
                                }
                                "glow" => x.glow = word(0).parse::<u8>().unwrap_or(8).min(15),
                                "sound" => x.sound = word(0).to_string(),
                                "tamable" => x.tamable = word(0) != "no",
                                "shoots" => x.shoots = word(0).parse::<i32>().unwrap_or(3).clamp(0, 100),
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
        for (at, w) in &raw_rules {
            let f = |k: usize| w.get(k).and_then(|v| v.parse::<f32>().ok());
            match w[0].as_str() {
                // sound <id> <effects|steps|ui> <seconds> <noise> <hz from> <hz to> <low-pass> <loudness> [caption...]
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
                        event: w[1].to_ascii_lowercase(),
                        what: w[2..d].join(" ").to_ascii_lowercase(),
                        actions: split_actions(&w[d + 1..].join(" ")),
                    }),
                    _ => warnings.push(alloc::format!("{}: when needs <event> [what] do <commands>", at)),
                },
                "command" => warnings.push(alloc::format!("{}: command needs <name> <what it does>", at)),
                _ => {}
            }
        }
        let raw_rules: Vec<(String, Vec<String>)> = raw_rules.into_iter().filter(|r| r.1[0] == "ore" || r.1[0] == "plant").collect();
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
            maple_log: AIR,
            maple_leaves: AIR,
            palm_log: AIR,
            palm_leaves: AIR,
            blossom_log: AIR,
            blossom_leaves: AIR,
            ruby_ore: AIR,
            snow_layer: AIR,
            rail: AIR,
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
        wk.birch_log = or("birch_log", wk.log);
        wk.birch_leaves = or("birch_leaves", wk.leaves);
        wk.maple_log = or("maple_log", wk.log);
        wk.maple_leaves = or("maple_leaves", wk.leaves);
        wk.palm_log = or("palm_log", wk.log);
        wk.palm_leaves = or("palm_leaves", wk.leaves);
        wk.blossom_log = or("blossom_log", wk.log);
        wk.blossom_leaves = or("blossom_leaves", wk.leaves);
        wk.ruby_ore = or("ruby_ore", wk.stone);
        wk.snow_layer = or("snow_layer", AIR);
        wk.rail = or("rail", AIR);
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

        Registry {
            opaque: blocks.iter().map(|b| (b.kind == Kind::Cube) as u8).collect(),
            emit: blocks.iter().map(|b| b.emit).collect(),
            sky_loss: blocks.iter().map(|b| b.sky_loss).collect(),
            kind: blocks.iter().map(|b| b.kind).collect(),
            sun_tile: find_tile("sun"),
            moon_tile: find_tile("moon"),
            wk,
            blocks,
            items,
            tiles,
            recipes,
            models,
            entities,
            vehicles,
            categories,
            ores,
            sounds,
            commands,
            scripts,
            plants,
            by_id,
            warnings,
        }
    }

    /// First state of the block with this data-file id.
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
    use super::*;

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
        assert!(r.items.iter().any(|i| i.id == "builders_wand" && i.brush == 2));
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
    fn drops_recipes_and_categories_come_from_data() {
        let r = reg();
        let item = |id: &str| r.items.iter().position(|i| i.id == id).unwrap();
        assert_eq!(def(r.wk.stone).drop, Some(item("cobble")));
        assert_eq!(def(r.block("glass").unwrap()).drop, None);
        assert_eq!(def(r.block("planks").unwrap()).drop, Some(item("planks")));
        // Every state of a block shares its drop.
        let stairs = r.block("stone_stairs").unwrap();
        assert_eq!(def(stairs + 7).drop, Some(item("stone_stairs")));
        let planks = r.recipes.iter().find(|x| x.out.0 == item("planks")).expect("no planks recipe");
        assert_eq!((planks.out.1, &planks.inputs[..]), (4, &[(item("log"), 1)][..]));
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
}
