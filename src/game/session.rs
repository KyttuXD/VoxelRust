//! One open world being played: the world itself, the player, their
//! inventory and health, and the rules of the game mode.

use crate::audio::{Audio, Sound};
use crate::block::{self, reg, ItemDef, Kind, Usable, AIR};
use alloc::collections::BTreeMap;
use crate::entity::Mobs;
use crate::interact;
use crate::inventory::{Inventory, Slot, Stack, HOTBAR};
use crate::jobs::Pool;
use crate::math::{floord, forward, sqrtf};
use crate::player::{Input, Player};
use crate::save::{DiskIo, RegionStore};
use crate::settings::kv;
use crate::sys;
use crate::world::{chunk_at, Hit, MeshSink, World};
use crate::worldgen;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GameMode {
    Survival,
    Creative,
}

pub const MAX_HEALTH: i32 = 20;
pub const MAX_HUNGER: i32 = 20;
const MAX_AIR: f32 = 10.0;

/// What the player is asking for this frame.
#[derive(Clone, Copy, Default)]
pub struct Controls {
    pub input: Input,
    pub break_held: bool,
    pub place_held: bool,
    pub pick: bool,
    pub toggle_fly: bool,
    /// Get off, or out of, whatever is being ridden.
    pub leave: bool,
    /// Drop one of what is held.
    pub drop: bool,
}

/// An episode in progress. What kinds there are, when they come and what
/// they do is all in the packs (`episode` lines; see docs/MODDING.md).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Episode {
    /// Which of the registry's episodes.
    pub kind: usize,
    /// Seconds to go.
    pub left: f32,
    /// Where it is (one that does not move stays on the player's start).
    pub x: f32,
    pub z: f32,
    pub dx: f32,
    pub dz: f32,
}

/// How a new world should be made.
#[derive(Clone)]
pub struct NewWorld {
    pub seed: u32,
    pub mode: GameMode,
    pub kind: worldgen::WorldKind,
}

pub struct Session {
    pub name: String,
    /// The name this player goes by (for things that belong to a player, such as a locked vehicle).
    pub who: String,
    pub world: World,
    pub pool: Pool,
    pub store: RegionStore<DiskIo>,
    pub player: Player,
    pub mobs: Mobs,
    /// What the mods' own code keeps for this world, by its name.
    pub ext: Vec<(&'static str, alloc::boxed::Box<dyn core::any::Any>)>,
    /// Where the player's eyes are while something a mod runs carries
    /// them (set afresh every frame by its `update`).
    pub seat: Option<[f64; 3]>,
    /// Whether that was so last frame: hands and feet are not free.
    pub carried: bool,
    /// The start of the names of this world's files, dimension included.
    pub prefix: String,
    /// Where the player has been, in squares of [`EXPLORE`] blocks: what the
    /// minimap shows.
    pub explored: alloc::collections::BTreeSet<(i32, i32)>,
    explored_path: String,
    /// The world's waypoints on its map (see `game::waypoints`).
    pub waypoints: Vec<crate::game::waypoints::Waypoint>,
    waypoints_path: String,
    pets_path: String,
    explore_timer: f32,
    /// Storage kept by a mod's code that the player just asked to open:
    /// whose, and which.
    pub opened_ext: Option<(&'static str, usize)>,
    /// The server connection, in multiplayer.
    pub net: Option<crate::netplay::Client>,
    /// Recent chat and notices, with seconds left on screen.
    pub chat: Vec<(String, f32)>,
    /// The server went away.
    pub lost: bool,
    /// The weather overhead, and any episode in progress.
    pub sky: crate::weather::Sky,
    pub episode: Option<Episode>,
    /// The switches packs add to the settings, as the player has them.
    pub flags: Vec<(String, bool)>,
    /// A `/pull` in force: how far it reaches, how hard, seconds left.
    pull: (f32, f32, f32),
    /// Camera shake strength.
    pub shake: f32,
    /// Lightning: counts down from a flash.
    pub flash: f32,
    weather_tick: f32,
    /// Water put down by `/flood`, to be taken back when it drains.
    flood: Vec<[i32; 3]>,
    /// Content sent by the server, complete and waiting to be applied.
    pub server_pack: Option<Vec<u8>>,
    pack_buffer: Vec<u8>,
    /// Messages that arrived behind a pack, kept until it is applied.
    held_events: Vec<crate::netplay::Event>,
    tick_timer: f32,
    touch_timer: f32,
    /// Sounds asked for by commands, played when the sound system is at hand.
    pub sound_queue: Vec<String>,
    /// Damage asked for by commands, dealt likewise.
    pending_hurt: i32,
    /// Things in flight: arrows, bolts, spells.
    pub shots: Vec<Shot>,
    /// Items on the ground.
    pub drops: Vec<Dropped>,
    /// Seconds each `when every N` script has left.
    script_timers: Vec<f32>,
    started: bool,
    call_timer: f32,
    /// The seat the player is sitting on.
    pub sitting: Option<[i32; 3]>,
    /// The creature being ridden (its id).
    pub riding_mob: Option<u32>,
    /// Numbers that pack scripts keep (`/var`), saved with the world.
    pub vars: alloc::collections::BTreeMap<String, i64>,
    vars_path: String,
    /// Commands waiting their turn (`/after`): seconds left, command.
    later: Vec<(f32, String)>,
    was_night: bool,
    death_told: bool,
    /// Shared games: when state was last sent, the chest last opened and
    /// what was last said about it.
    share_timer: f32,
    bite_timer: f32,
    open_chest: Option<[i32; 3]>,
    chest_sent: Option<([i32; 3], String)>,
    /// Which dimension of the world is being played (empty = the world itself).
    pub dim: String,
    pending_dim: Option<String>,
    /// The world's own seed (each dimension's is made from it) and file prefix.
    base_seed: u32,
    file_prefix: String,
    /// Rain set by `/weather` (0 clear ..= 1 storm), overriding the sky.
    pub forced_sky: Option<f32>,
    /// Skin images that arrived from other players, for the renderer.
    pub skin_events: Vec<(u16, Vec<u8>, bool)>,
    /// The player may move: the chunks right around them are in.
    pub playable: bool,
    pub inv: Inventory,
    /// The stack carried by the mouse in the inventory screen.
    pub cursor: Slot,
    pub selected: usize,
    pub mode: GameMode,
    pub kind: worldgen::WorldKind,
    pub health: i32,
    /// 0 (starving) ..= 20 (full); survival only.
    pub hunger: i32,
    hunger_clock: f32,
    /// What is stored in each chest, by the chest's position.
    pub chests: BTreeMap<[i32; 3], Inventory>,
    /// A block the player just used: the screen to open, and where it is.
    pub opened: Option<(Usable, [i32; 3])>,
    chests_path: String,
    rng: u32,
    pub air: f32,
    /// Counts down after taking damage; drives the red flash.
    pub hurt_flash: f32,
    pub dead: bool,
    pub spawn: [f64; 3],
    /// 0..1; 0 is sunrise.
    pub time: f32,
    pub clock: f32,
    pub target: Option<Hit>,
    /// The block being mined and how far along, 0..1.
    pub mining: Option<([i32; 3], f32)>,
    pub note: (String, f32),
    pub cooldown: [f32; 2],
    peak_y: f64,
    was_airborne: bool,
    step_distance: f32,
    since_damage: f32,
    regen: f32,
    drown: f32,
    autosave: f32,
    meta_path: String,
    pub is_new: bool,
}

/// Something dropped and lying about until it is picked up again.
pub struct Dropped {
    pub pos: [f64; 3],
    vel: [f32; 3],
    pub item: u16,
    pub count: u16,
    /// Seconds it has lain there (it cannot be picked up at once, and
    /// is gone after five minutes).
    age: f32,
}

/// Something flying through the air: an arrow, a bolt, a spell.
pub struct Shot {
    pub pos: [f64; 3],
    pub vel: [f32; 3],
    pub damage: i32,
    /// Seconds before it fizzles out.
    pub life: f32,
    /// Block left where it lands (AIR = none).
    pub leaves: block::Block,
    pub colour: [u8; 3],
    /// Shot at the player rather than by them.
    pub hostile: bool,
}

/// Size of the squares in which explored ground is remembered.
pub const EXPLORE: i32 = 32;

fn meta_path(base: &str, name: &str) -> String {
    alloc::format!("{}{}.meta.txt", world_dir(base), name)
}

/// The folder holding every world's files and the world index. Worlds used
/// to sit loose in the data folder; `migrate_worlds` moves them here once.
pub fn world_dir(base: &str) -> String {
    alloc::format!("{}worlds/", base)
}

/// Files in the data folder that are not part of any world.
const ROOT_FILES: [&str; 4] = ["settings.txt", "servers.txt", "skin.png", "worlds.txt"];

/// Move worlds that sit loose in the data folder (from before `worlds/`
/// existed) into `worlds/`. Done once: afterwards the index is in `worlds/`.
pub fn migrate_worlds(base: &str) {
    let old = match sys::read_file(&alloc::format!("{}worlds.txt", base)) {
        Some(b) => b,
        None => return,
    };
    if sys::read_file(&alloc::format!("{}index.txt", world_dir(base))).is_some() {
        return;
    }
    sys::make_dir(&world_dir(base));
    let names: Vec<String> = String::from_utf8_lossy(&old).lines().map(|l| crate::settings::clean_world_name(l.trim(), 24)).filter(|n| !n.is_empty()).collect();
    for name in &names {
        let prefix = alloc::format!("{}.", name);
        for f in sys::list_dir(base) {
            if !f.starts_with(&prefix) || ROOT_FILES.contains(&f.as_str()) {
                continue;
            }
            if let Some(data) = sys::read_file(&alloc::format!("{}{}", base, f)) {
                if sys::write_file(&alloc::format!("{}{}", world_dir(base), f), &data) {
                    sys::remove_file(&alloc::format!("{}{}", base, f));
                }
            }
        }
    }
    sys::write_file(&alloc::format!("{}index.txt", world_dir(base)), names.join("\n").as_bytes());
    sys::remove_file(&alloc::format!("{}worlds.txt", base));
}

/// Names of the worlds that exist, from the index file.
pub fn list_worlds(base: &str) -> Vec<String> {
    let text = sys::read_file(&alloc::format!("{}index.txt", world_dir(base))).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    for l in text.lines() {
        let n = crate::settings::clean_world_name(l.trim(), 24);
        if !n.is_empty() && !names.contains(&n) && sys::read_file(&meta_path(base, &n)).is_some() {
            names.push(n);
        }
    }
    names
}

pub fn remember_world(base: &str, name: &str) {
    sys::make_dir(&world_dir(base));
    let mut names = list_worlds(base);
    if !names.iter().any(|n| n == name) {
        names.push(String::from(name));
        sys::make_dir(&world_dir(base));
        sys::make_dir(&world_dir(base));
        sys::write_file(&alloc::format!("{}index.txt", world_dir(base)), names.join("\n").as_bytes());
    }
}

/// The files a world is made of: everything in the data folder that starts
/// with the world's name and a dot (its regions, its dimensions, its small
/// files). Names cannot hold dots, so no other world's files match.
pub fn world_files(base: &str, name: &str) -> Vec<String> {
    let prefix = alloc::format!("{}.", name);
    sys::list_dir(&world_dir(base)).into_iter().filter(|f| f.starts_with(&prefix)).collect()
}

/// Take a world out of the index (its files are left alone).
pub fn forget_world(base: &str, name: &str) {
    let names: Vec<String> = list_worlds(base).into_iter().filter(|n| n != name).collect();
    sys::make_dir(&world_dir(base));
    sys::write_file(&alloc::format!("{}index.txt", world_dir(base)), names.join("\n").as_bytes());
}

/// Delete a world: every file it is made of, and its place in the index.
/// Returns whether the world was there and every file went.
pub fn delete_world(base: &str, name: &str) -> bool {
    if !world_exists(base, name) {
        return false;
    }
    let mut all = true;
    for f in world_files(base, name) {
        all &= sys::remove_file(&alloc::format!("{}{}", world_dir(base), f));
    }
    forget_world(base, name);
    all
}

/// Copy a world's files under a new name. The old world is left as it was.
pub fn copy_world(base: &str, name: &str, to: &str) -> bool {
    if !world_exists(base, name) || world_exists(base, to) {
        return false;
    }
    let from_prefix = alloc::format!("{}.", name);
    for f in world_files(base, name) {
        let data = match sys::read_file(&alloc::format!("{}{}", world_dir(base), f)) {
            Some(d) => d,
            None => return false,
        };
        let new_name = alloc::format!("{}.{}", to, &f[from_prefix.len()..]);
        if !sys::write_file(&alloc::format!("{}{}", world_dir(base), new_name), &data) {
            return false;
        }
    }
    remember_world(base, to);
    true
}

/// Rename a world: copy it under the new name, then delete the old files.
pub fn rename_world(base: &str, name: &str, to: &str) -> bool {
    if !copy_world(base, name, to) {
        return false;
    }
    for f in world_files(base, name) {
        sys::remove_file(&alloc::format!("{}{}", world_dir(base), f));
    }
    forget_world(base, name);
    true
}

pub fn world_exists(base: &str, name: &str) -> bool {
    sys::read_file(&meta_path(base, name)).is_some()
}

impl Session {
    /// Open the world called `name`, creating it from `new` if it does not
    /// exist yet. `threads` is the number of worker threads.
    pub fn open(base: &str, name: &str, new: &NewWorld, threads: usize) -> Session {
        sys::make_dir(&world_dir(base));
        let path = meta_path(base, name);
        let meta = sys::read_file(&path).map(|b| String::from_utf8_lossy(&b).into_owned());
        let is_new = meta.is_none();
        let meta = meta.unwrap_or_default();
        let seed = kv(&meta, "seed").and_then(|v| v.parse::<u32>().ok()).unwrap_or(new.seed);
        let kind = kv(&meta, "kind").and_then(worldgen::WorldKind::parse).unwrap_or_else(|| new.kind.clone());
        let mode = match kv(&meta, "gamemode") {
            Some("creative") => GameMode::Creative,
            Some("survival") => GameMode::Survival,
            _ => new.mode,
        };
        worldgen::set_shape(0);
        worldgen::set_kind(kind.clone());
        // Numbers that are not numbers (a damaged file) are left out, so
        // the fallbacks below apply.
        let floats = |k: &str| -> Vec<f64> { kv(&meta, k).map(|v| v.split(' ').filter_map(|x| x.parse::<f64>().ok()).filter(|f| f.is_finite() && *f > -3.0e7 && *f < 3.0e7).collect()).unwrap_or_default() };
        let spawn = match floats("spawn").as_slice() {
            [x, y, z] => [*x, *y, *z],
            _ => {
                let (x, y, z) = worldgen::find_spawn(seed);
                [x as f64 + 0.5, y as f64 + 1.01, z as f64 + 0.5]
            }
        };
        let mut player = match floats("pos").as_slice() {
            [x, y, z] => Player::new([*x, *y, *z]),
            _ => Player::new(spawn),
        };
        player.yaw = floats("yaw").first().copied().unwrap_or(0.0) as f32;
        player.pitch = (floats("pitch").first().copied().unwrap_or(0.0) as f32).clamp(-1.55, 1.55);
        player.flying = kv(&meta, "flying") == Some("1") && mode == GameMode::Creative;
        let inv = match kv(&meta, "inventory") {
            Some(text) => Inventory::load(text),
            None => starting_inventory(mode),
        };
        if is_new && !name.starts_with('~') {
            remember_world(base, name);
        }
        // Chest contents: one line per chest, `x y z <slots>`.
        let chests_path = alloc::format!("{}{}.chests.txt", world_dir(base), name);
        let mut chests = BTreeMap::new();
        if let Some(text) = sys::read_file(&chests_path) {
            for line in String::from_utf8_lossy(&text).lines() {
                let mut w = line.splitn(4, ' ');
                let mut n = || w.next().and_then(|v| v.parse::<i32>().ok());
                if let (Some(x), Some(y), Some(z)) = (n(), n(), n()) {
                    chests.insert([x, y, z], Inventory::load(w.next().unwrap_or("")));
                }
            }
        }
        let mut session = Session {
            name: String::from(name),
            who: String::from("Player"),
            world: World::new(seed),
            pool: Pool::new(threads, seed),
            store: RegionStore::new(DiskIo, alloc::format!("{}{}.", world_dir(base), name)),
            peak_y: player.pos[1],
            player,
            mobs: {
                let mut m = Mobs::new(seed ^ 0xA5A5_1234);
                if let Some(text) = sys::read_file(&alloc::format!("{}{}.pets.txt", world_dir(base), name)) {
                    m.load_pets(&String::from_utf8_lossy(&text));
                }
                m
            },
            ext: Vec::new(),
            seat: None,
            carried: false,
            prefix: alloc::format!("{}{}.", world_dir(base), name),
            explored: sys::read_file(&alloc::format!("{}{}.explored.txt", world_dir(base), name))
                .map(|b| {
                    String::from_utf8_lossy(&b)
                        .lines()
                        .filter_map(|l| {
                            let mut w = l.split(' ');
                            Some((w.next()?.parse().ok()?, w.next()?.parse().ok()?))
                        })
                        .collect()
                })
                .unwrap_or_default(),
            explored_path: alloc::format!("{}{}.explored.txt", world_dir(base), name),
            waypoints: sys::read_file(&alloc::format!("{}{}.waypoints.txt", world_dir(base), name))
                .map(|b| crate::game::waypoints::parse(&String::from_utf8_lossy(&b)))
                .unwrap_or_default(),
            waypoints_path: alloc::format!("{}{}.waypoints.txt", world_dir(base), name),
            pets_path: alloc::format!("{}{}.pets.txt", world_dir(base), name),
            explore_timer: 0.0,
            opened: None,
            net: None,
            chat: Vec::new(),
            lost: false,
            sky: crate::weather::CLEAR,
            episode: None,
            flags: Vec::new(),
            pull: (0.0, 0.0, 0.0),
            shake: 0.0,
            flash: 0.0,
            weather_tick: 0.0,
            flood: Vec::new(),
            server_pack: None,
            pack_buffer: Vec::new(),
            held_events: Vec::new(),
            tick_timer: 0.0,
            touch_timer: 0.0,
            sound_queue: Vec::new(),
            pending_hurt: 0,
            shots: Vec::new(),
            drops: Vec::new(),
            script_timers: Vec::new(),
            started: false,
            call_timer: 3.0,
            sitting: None,
            riding_mob: None,
            vars: sys::read_file(&alloc::format!("{}{}.vars.txt", world_dir(base), name))
                .map(|b| String::from_utf8_lossy(&b).lines().filter_map(|l| l.split_once('=').and_then(|(k, v)| Some((String::from(k), v.parse::<i64>().ok()?)))).collect())
                .unwrap_or_default(),
            vars_path: alloc::format!("{}{}.vars.txt", world_dir(base), name),
            later: Vec::new(),
            was_night: false,
            death_told: false,
            share_timer: 0.0,
            bite_timer: 0.0,
            open_chest: None,
            chest_sent: None,
            dim: String::new(),
            pending_dim: crate::settings::kv(&meta, "dim").filter(|d| !d.is_empty()).map(String::from),
            base_seed: seed,
            file_prefix: alloc::format!("{}{}.", world_dir(base), name),
            forced_sky: None,
            skin_events: Vec::new(),
            playable: false,
            inv,
            cursor: None,
            selected: kv(&meta, "selected").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0).min(HOTBAR - 1),
            mode,
            kind,
            health: kv(&meta, "health").and_then(|v| v.parse::<i32>().ok()).unwrap_or(MAX_HEALTH).clamp(1, MAX_HEALTH),
            hunger: kv(&meta, "hunger").and_then(|v| v.parse::<i32>().ok()).unwrap_or(MAX_HUNGER).clamp(0, MAX_HUNGER),
            hunger_clock: 0.0,
            chests,
            opened_ext: None,
            chests_path,
            rng: seed ^ 0x9E37_79B9,
            air: MAX_AIR,
            hurt_flash: 0.0,
            dead: false,
            spawn,
            time: (floats("time").first().copied().unwrap_or(0.12) as f32).clamp(0.0, 0.999),
            clock: 0.0,
            target: None,
            mining: None,
            note: (String::new(), 0.0),
            cooldown: [0.0; 2],
            was_airborne: false,
            step_distance: 0.0,
            since_damage: 100.0,
            regen: 0.0,
            drown: 0.0,
            autosave: 0.0,
            meta_path: path,
            is_new,
        };
        session.natives(|n, g| {
            if let Some(open) = n.open {
                open(g)
            }
        });
        session
    }

    /// Join a multiplayer server. Nothing of such a world is saved here:
    /// the server keeps it.
    pub fn join(base: &str, address: &str, name: &str, threads: usize) -> Result<Session, String> {
        let (host, port) = crate::netplay::parse_address(address).ok_or_else(|| String::from("TYPE THE SERVER'S ADDRESS, LIKE HOST:25590"))?;
        let (client, welcome) = crate::netplay::Client::connect(&host, port, name)?;
        let kind = worldgen::WorldKind::parse(&welcome.kind).unwrap_or(worldgen::WorldKind::Normal);
        let new = NewWorld { seed: welcome.seed, mode: GameMode::Creative, kind };
        // A name no saved world can have, so nothing on disk is touched.
        let mut s = Session::open(base, "~multiplayer", &new, threads);
        s.world.log_edits = true;
        s.time = welcome.time;
        s.net = Some(client);
        s.push_chat(alloc::format!("CONNECTED TO {}", address.to_ascii_uppercase()));
        Ok(s)
    }

    pub fn push_chat(&mut self, line: String) {
        self.chat.push((line, 10.0));
        if self.chat.len() > 8 {
            self.chat.remove(0);
        }
    }

    /// Say something to everyone (and show it locally).
    pub fn send_chat(&mut self, name: &str, text: &str) {
        if text.trim().is_empty() {
            return;
        }
        if let Some(n) = self.net.as_mut() {
            n.send_chat(text);
        }
        // The speaker's name in a colour of its own (the same every time).
        let hue = name.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
        let code = [b'9', b'A', b'B', b'C', b'D', b'E', b'6', b'3'][(hue % 8) as usize] as char;
        self.push_chat(alloc::format!("&{}<{}>&F {}", code, name, text).to_ascii_uppercase());
    }

    /// Call into every mod's own code that is switched on.
    pub fn natives(&mut self, mut f: impl FnMut(&'static crate::native::Native, &mut Session)) {
        for (_, n) in crate::native::active() {
            f(n, self);
        }
    }

    /// What a mod's code keeps for this world, taken out so that it can be
    /// worked on alongside everything else here; hand it back with
    /// [`Session::put_ext`]. A fresh one if there was none.
    pub fn take_ext<T: Default + 'static>(&mut self, name: &'static str) -> alloc::boxed::Box<T> {
        match self.ext.iter().position(|e| e.0 == name) {
            Some(i) => self.ext.swap_remove(i).1.downcast::<T>().unwrap_or_default(),
            None => alloc::boxed::Box::default(),
        }
    }

    pub fn put_ext<T: 'static>(&mut self, name: &'static str, value: alloc::boxed::Box<T>) {
        self.ext.retain(|e| e.0 != name);
        self.ext.push((name, value));
    }

    pub fn ext_ref<T: 'static>(&self, name: &str) -> Option<&T> {
        self.ext.iter().find(|e| e.0 == name).and_then(|e| e.1.downcast_ref::<T>())
    }

    pub fn ext_mut<T: 'static>(&mut self, name: &str) -> Option<&mut T> {
        self.ext.iter_mut().find(|e| e.0 == name).and_then(|e| e.1.downcast_mut::<T>())
    }

    /// In a shared game: send `data` to the same mod's code on the other
    /// players' machines (their `message` hook gets it with `key`).
    pub fn send_ext(&mut self, name: &str, keep: bool, key: &str, data: &[u8]) {
        let channel = crate::native::all().iter().position(|n| n.name == name);
        if let (Some(k), Some(net)) = (channel, self.net.as_mut()) {
            net.send_state(crate::proto::channel::NATIVE + k as u8, keep, key, data);
        }
    }

    /// A typed command (`/give`, `/time`...), for testing and for making
    /// packs. Returns what to tell the player; `None` if `line` is not a
    /// command this knows.
    pub fn command(&mut self, line: &str) -> Option<String> {
        self.run(line, 0)
    }

    /// Something happened that packs may have a `when` line for: carry out
    /// what they ask. `what` is the id of the thing it happened to.
    pub fn fire(&mut self, event: &str, what: &str) {
        let r = reg();
        if r.scripts.is_empty() {
            return;
        }
        for s in r.scripts.iter().filter(|s| s.event == event && (s.what.is_empty() || s.what == what)) {
            for a in &s.actions {
                self.run(&a.to_ascii_lowercase(), 1);
            }
        }
    }

    /// Whether a condition of `/if` is true. Conditions: `chance N`,
    /// `holding ITEM`, `has ITEM [N]`, `dim NAME`, `night`, `day`,
    /// `creative`, `survival`, `riding`, `hurt` (health below full),
    /// `hungry`, `NAME OP NUMBER` for a variable (= != < > <= >=); and
    /// `not` in front of any of them.
    fn holds(&mut self, cond: &[&str]) -> bool {
        let w = |k: usize| cond.get(k).copied().unwrap_or("");
        let r = reg();
        match w(0) {
            "not" => !self.holds(&cond[1.min(cond.len())..]),
            "chance" => {
                self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((self.rng >> 8) % 100) < w(1).parse::<u32>().unwrap_or(50)
            }
            "holding" => self.held().map_or(false, |h| h.1.id == w(1)),
            "has" => r.items.iter().position(|it| it.id == w(1)).map_or(false, |i| self.inv.count(i as u16) >= w(2).parse::<u32>().unwrap_or(1)),
            "dim" => self.dim == if w(1) == "home" { "" } else { w(1) },
            "night" => crate::math::sinf(self.time * crate::math::TAU) < -0.05,
            "day" => crate::math::sinf(self.time * crate::math::TAU) >= -0.05,
            "creative" => self.mode == GameMode::Creative,
            "survival" => self.mode == GameMode::Survival,
            "riding" => self.riding_mob.is_some() || self.carried,
            "hurt" => self.health < MAX_HEALTH,
            "hungry" => self.hunger < MAX_HUNGER,
            // A switch a pack put in the settings (`setting <id> ...`).
            "setting" => self.flags.iter().find(|f| f.0 == w(1)).map_or_else(|| r.toggles.iter().any(|t| t.0 == w(1) && t.1), |f| f.1),
            "storm" => self.sky.storm,
            "rain" => self.sky.rain > 0.2 && !self.sky.snow,
            "snow" => self.sky.rain > 0.2 && self.sky.snow,
            "coast" => crate::weather::near_sea(self.world.seed, floord(self.player.pos[0]), floord(self.player.pos[2])),
            "lowland" => worldgen::column(self.world.seed, floord(self.player.pos[0]), floord(self.player.pos[2])).height < worldgen::SEA + 24,
            "biome" => {
                let b = worldgen::column(self.world.seed, floord(self.player.pos[0]), floord(self.player.pos[2])).biome;
                cond[1.min(cond.len())..].iter().any(|n| alloc::format!("{:?}", b).eq_ignore_ascii_case(n))
            }
            name => {
                // A condition of some mod's own code, perhaps.
                if let Some(holds) = crate::native::active().filter_map(|(_, n)| n.condition).find_map(|condition| condition(self, cond)) {
                    return holds;
                }
                let have = match name {
                    "health" => self.health as i64,
                    "hunger" => self.hunger as i64,
                    "y" => floord(self.player.pos[1]) as i64,
                    _ => self.vars.get(name).copied().unwrap_or(0),
                };
                let want = w(2).parse::<i64>().unwrap_or(0);
                match w(1) {
                    "=" | "==" => have == want,
                    "!=" => have != want,
                    "<" => have < want,
                    ">" => have > want,
                    "<=" => have <= want,
                    ">=" => have >= want,
                    _ => false,
                }
            }
        }
    }

    /// Whether any pack script listens for this.
    pub fn listens(&self, event: &str, what: &str) -> bool {
        reg().scripts.iter().any(|s| s.event == event && s.what == what)
    }

    fn run(&mut self, line: &str, depth: u32) -> Option<String> {
        let words: Vec<&str> = line.split_whitespace().collect();
        let r = reg();
        let arg = |k: usize| words.get(k).copied().unwrap_or("");
        let online = self.net.is_some();
        // Commands that packs define: their own list of commands, with
        // $1, $2... standing for what was typed after the name.
        if let Some((_, actions)) = r.commands.iter().find(|c| c.0 == arg(0)) {
            if depth >= 6 {
                return Some(String::from("COMMANDS NESTED TOO DEEPLY"));
            }
            let mut last = String::new();
            for a in actions {
                let mut filled = a.to_ascii_lowercase();
                for k in (1..=9).rev() {
                    filled = filled.replace(&alloc::format!("${}", k), arg(k));
                }
                filled = filled.replace("$*", &words[1.min(words.len())..].join(" "));
                if let Some(reply) = self.run(&filled, depth + 1) {
                    last = reply;
                }
            }
            return Some(last);
        }
        // "~" is where the player stands, "~3" three blocks further.
        let here = [floord(self.player.pos[0]), floord(self.player.pos[1]), floord(self.player.pos[2])];
        let coord = |k: usize, axis: usize| -> Option<f64> {
            let a = arg(k);
            match a.strip_prefix('~') {
                Some(rest) => Some(here[axis] as f64 + if rest.is_empty() { 0.0 } else { rest.parse::<f64>().ok()? }),
                None => a.parse::<f64>().ok(),
            }
        };
        // "$name" stands for a variable set with /var; "$x $y $z" for
        // where the player stands.
        if line.contains('$') {
            let mut filled = String::from(line);
            for (k, axis) in ["$x", "$y", "$z"].iter().enumerate() {
                filled = filled.replace(axis, &alloc::format!("{}", here[k]));
            }
            let mut names: Vec<&String> = self.vars.keys().collect();
            names.sort_by(|a, b| b.len().cmp(&a.len()));
            for name in names {
                filled = filled.replace(&alloc::format!("${}", name), &alloc::format!("{}", self.vars[name]));
            }
            if filled != line && depth < 6 {
                return self.run(&filled, depth + 1);
            }
        }
        Some(match arg(0) {
            // ---- Scripting: variables, conditions, timing.
            // /var NAME = N | + N | - N | random N
            "/var" if !arg(1).is_empty() => {
                let n = arg(3).parse::<i64>().unwrap_or(0);
                let now = self.vars.get(arg(1)).copied().unwrap_or(0);
                let value = match arg(2) {
                    "=" => n,
                    "+" => now.saturating_add(n),
                    "-" => now.saturating_sub(n),
                    "random" => {
                        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        ((self.rng >> 8) as i64) % n.max(1)
                    }
                    _ => return Some(String::from("USE  /VAR NAME = N,  + N,  - N  OR  RANDOM N")),
                };
                if self.vars.len() < 256 || self.vars.contains_key(arg(1)) {
                    self.vars.insert(String::from(arg(1)), value);
                }
                String::new()
            }
            // /if CONDITION then COMMAND [else COMMAND]
            "/if" => {
                let then = words.iter().position(|w| *w == "then");
                let other = words.iter().position(|w| *w == "else");
                match then {
                    Some(t) if depth < 6 => {
                        let yes = self.holds(&words[1..t]);
                        let end = other.filter(|&o| o > t).unwrap_or(words.len());
                        let branch = if yes { words[t + 1..end].join(" ") } else if end < words.len() { words[end + 1..].join(" ") } else { String::new() };
                        if branch.is_empty() {
                            String::new()
                        } else {
                            self.run(&branch, depth + 1).unwrap_or_default()
                        }
                    }
                    _ => String::from("USE  /IF CONDITION THEN COMMAND [ELSE COMMAND]"),
                }
            }
            // /after SECONDS COMMAND: do it later.
            "/after" => {
                let wait = arg(1).parse::<f32>().unwrap_or(1.0).clamp(0.0, 3600.0);
                if self.later.len() < 64 && words.len() > 2 {
                    self.later.push((wait, words[2..].join(" ")));
                }
                String::new()
            }
            // /repeat N COMMAND
            "/repeat" if depth < 6 => {
                let n = arg(1).parse::<u32>().unwrap_or(1).min(64);
                let what = words[2.min(words.len())..].join(" ");
                for _ in 0..n {
                    self.run(&what, depth + 1);
                }
                String::new()
            }
            // /take ITEM [N]: remove from the player's pack (if they have it).
            "/take" => match r.items.iter().position(|it| it.id == arg(1)) {
                Some(item) => {
                    self.inv.remove(item as u16, arg(2).parse::<u16>().unwrap_or(1).clamp(1, 999));
                    String::new()
                }
                None => String::from("NO SUCH ITEM"),
            },
            // /title TEXT: a notice in the middle of the screen.
            "/title" => {
                let text = words[1.min(words.len())..].join(" ").to_ascii_uppercase();
                self.say(&text);
                String::new()
            }
            // /blast R: clear a ball of blocks where the player is pointing.
            "/blast" if !online => {
                if let Some(c) = self.target.as_ref().map(|h| h.cell) {
                    let rad = arg(1).parse::<i32>().unwrap_or(2).clamp(1, 5);
                    for dx in -rad..=rad {
                        for dy in -rad..=rad {
                            for dz in -rad..=rad {
                                let cell = [c[0] + dx, c[1] + dy, c[2] + dz];
                                let b = self.world.get(cell[0], cell[1], cell[2]);
                                if dx * dx + dy * dy + dz * dz <= rad * rad && b != AIR && !block::def(b).unbreakable && cell[1] > 0 {
                                    self.world.set_block(cell[0], cell[1], cell[2], AIR);
                                }
                            }
                        }
                    }
                    self.sound_queue.push(String::from("thunder"));
                }
                String::new()
            }
            // ---- For scripts mostly: things that happen to the player.
            "/say" => {
                self.push_chat(words[1.min(words.len())..].join(" ").to_ascii_uppercase());
                String::new()
            }
            "/sound" => {
                self.sound_queue.push(String::from(arg(1)));
                String::new()
            }
            // /shake S: rock the view (it dies away by itself).
            "/shake" => {
                self.shake = arg(1).parse::<f32>().unwrap_or(0.35).clamp(0.0, 1.0);
                String::new()
            }
            // /episode ID | stop: begin one a pack defines, or end the one running.
            "/episode" if !online => match reg().episodes.iter().position(|d| d.id == arg(1)) {
                Some(k) => {
                    self.drain_flood(0);
                    self.start_episode(k);
                    String::from("it begins")
                }
                None => {
                    self.episode = None;
                    self.drain_flood(0);
                    String::from(if arg(1) == "stop" { "stopped" } else { "NO SUCH EPISODE" })
                }
            },
            // /pull REACH STRENGTH: for the next moment, drag the player
            // towards the episode and lift them when close.
            "/pull" => {
                self.pull = (arg(1).parse::<f32>().unwrap_or(14.0).clamp(1.0, 64.0), arg(2).parse::<f32>().unwrap_or(2.2).clamp(0.0, 10.0), 0.6);
                String::new()
            }
            // /strip R N: tear up to N plants and leaves within R blocks of
            // the episode (of the player, when none is running).
            "/strip" if !online => {
                let (cx, cz) = match self.episode {
                    Some(d) => (floord(d.x as f64), floord(d.z as f64)),
                    None => (floord(self.player.pos[0]), floord(self.player.pos[2])),
                };
                let rad = arg(1).parse::<i32>().unwrap_or(3).clamp(1, 16);
                let seed = self.world.seed;
                for _ in 0..arg(2).parse::<u32>().unwrap_or(40).min(400) {
                    let span = (rad * 2 + 1) as u32;
                    let (bx, bz) = (cx + (self.roll() % span) as i32 - rad, cz + (self.roll() % span) as i32 - rad);
                    let by = worldgen::column(seed, bx, bz).height + 1 + (self.roll() % 10) as i32;
                    let b = self.world.get(bx, by, bz);
                    if b != AIR && matches!(reg().kind[b as usize], Kind::Cross | Kind::Cutout) {
                        self.world.set_block(bx, by, bz, AIR);
                    }
                }
                String::new()
            }
            // /flood PEAK: while an episode runs, the sea stands up to PEAK
            // blocks over its level near the player: rising through the
            // first quarter of the episode and draining over the last third.
            "/flood" if !online => {
                if let Some(d) = self.episode {
                    let lasts = reg().episodes[d.kind].lasts.max(0.1);
                    let peak = arg(1).parse::<f32>().unwrap_or(3.0).clamp(0.0, 24.0);
                    let level = worldgen::SEA + crate::weather::wave_height(1.0 - d.left / lasts, peak) as i32;
                    let (px, pz) = (floord(self.player.pos[0]), floord(self.player.pos[2]));
                    let (seed, water) = (self.world.seed, reg().wk.water);
                    for _ in 0..160 {
                        let (x, z) = (px + (self.roll() % 49) as i32 - 24, pz + (self.roll() % 49) as i32 - 24);
                        let ground = worldgen::column(seed, x, z).height;
                        for y in (ground + 1).max(worldgen::SEA + 1)..=level {
                            if self.world.get(x, y, z) == AIR && self.world.set_block(x, y, z, water) {
                                self.flood.push([x, y, z]);
                            }
                        }
                    }
                    self.drain_flood(level);
                }
                String::new()
            }
            // /drop: toss the held item a little way ahead, as Q does.
            "/drop" => {
                match self.inv.take_one(self.selected) {
                    Some(item) => {
                        let dir = forward(self.player.yaw, self.player.pitch);
                        let eye = self.player.eye();
                        if self.drops.len() >= 128 {
                            self.drops.remove(0);
                        }
                        self.drops.push(Dropped { pos: [eye[0], eye[1] - 0.3, eye[2]], vel: [dir.x * 5.0, dir.y * 5.0 + 2.0, dir.z * 5.0], item, count: 1, age: 0.0 });
                        String::from("dropped")
                    }
                    None => String::from("NOTHING IN HAND"),
                }
            }
            "/heal" => {
                self.health = (self.health + arg(1).parse::<i32>().unwrap_or(4)).clamp(1, MAX_HEALTH);
                String::new()
            }
            "/feed" => {
                self.hunger = (self.hunger + arg(1).parse::<i32>().unwrap_or(4)).clamp(0, MAX_HUNGER);
                String::new()
            }
            "/hurt" => {
                self.pending_hurt += arg(1).parse::<i32>().unwrap_or(1).clamp(0, 100);
                String::new()
            }
            // /launch UP [FORWARD]: throw the player (a jump pad, a gust).
            "/launch" => {
                let up = arg(1).parse::<f32>().unwrap_or(8.0).clamp(-40.0, 40.0);
                let ahead = arg(2).parse::<f32>().unwrap_or(0.0).clamp(-40.0, 40.0);
                let f = forward(self.player.yaw, 0.0);
                self.player.vel = [f.x * ahead, up, f.z * ahead];
                self.player.on_ground = false;
                self.peak_y = self.player.pos[1];
                String::new()
            }
            // /shoot DAMAGE [SPEED] [BLOCK]: loose something the way the
            // player looks; BLOCK is left where it lands (fire, ice, web).
            "/shoot" if !online => {
                let damage = arg(1).parse::<i32>().unwrap_or(4).clamp(0, 200);
                let speed = arg(2).parse::<f32>().unwrap_or(30.0).clamp(4.0, 80.0);
                let leaves = r.block(arg(3)).unwrap_or(AIR);
                self.loose(damage, speed, leaves, [255, 240, 200]);
                String::new()
            }
            // /aim BLOCK: put a block where the player is pointing.
            "/aim" if !online => match (r.block(arg(1)), self.target.as_ref().map(|h| h.prev)) {
                (Some(b), Some(c)) => {
                    self.world.set_block(c[0], c[1], c[2], b);
                    String::new()
                }
                _ => String::new(),
            },
            // /strike: lightning where the player is pointing.
            "/strike" if !online => {
                if let Some(c) = self.target.as_ref().map(|h| h.cell) {
                    self.flash = 0.3;
                    self.sound_queue.push(String::from("thunder"));
                    let at = [c[0] as f64 + 0.5, c[1] as f64 + 1.0, c[2] as f64 + 0.5];
                    let hit: Vec<usize> = (0..self.mobs.list.len())
                        .filter(|&i| {
                            let p = self.mobs.list[i].body.pos;
                            (p[0] - at[0]) * (p[0] - at[0]) + (p[2] - at[2]) * (p[2] - at[2]) < 16.0
                        })
                        .collect();
                    for i in hit {
                        self.mobs.hurt(i, arg(1).parse::<i32>().unwrap_or(12), at);
                    }
                }
                String::new()
            }
            "/give" => match r.items.iter().position(|it| it.id == arg(1)) {
                Some(item) => {
                    let n = arg(2).parse::<u16>().unwrap_or(1).clamp(1, 999);
                    self.inv.add(item as u16, n);
                    alloc::format!("GAVE {} {}", n, r.items[item].name.to_ascii_uppercase())
                }
                None => String::from("NO SUCH ITEM. USE ITS ID, AS IN THE DATA FILES"),
            },
            "/time" if !online => {
                let t = match arg(1) {
                    "day" | "noon" => 0.25,
                    "night" | "midnight" => 0.75,
                    "dawn" | "morning" => 0.0,
                    "dusk" | "evening" => 0.5,
                    other => other.parse::<f32>().unwrap_or(f32::NAN),
                };
                if t >= 0.0 && t < 1.0 {
                    self.time = t;
                    String::from("TIME SET")
                } else {
                    String::from("USE  /TIME DAY, NIGHT, DAWN, DUSK  OR A NUMBER FROM 0 TO 1")
                }
            }
            "/tp" => match (coord(1, 0).ok_or(()), coord(2, 1).ok_or(()), coord(3, 2).ok_or(())) {
                // Real numbers only, and not off the edge of what
                // whole-number block positions can hold.
                (Ok(x), Ok(y), Ok(z)) if x.is_finite() && y.is_finite() && z.is_finite() => {
                    const EDGE: f64 = 30_000_000.0;
                    self.player.pos = [x.clamp(-EDGE, EDGE), y.clamp(1.0, crate::chunk::CH as f64 + 20.0), z.clamp(-EDGE, EDGE)];
                    self.player.vel = [0.0; 3];
                    self.peak_y = self.player.pos[1];
                    String::from("MOVED")
                }
                _ => String::from("USE  /TP X Y Z"),
            },
            // (In a shared game, whoever runs the creatures may add to them.)
            "/spawn" if !online || self.runs_the_world() => match r.entities.iter().position(|e| e.id == arg(1)) {
                Some(kind) => {
                    let n = arg(2).parse::<usize>().unwrap_or(1).clamp(1, 20);
                    let f = forward(self.player.yaw, 0.0);
                    for k in 0..n {
                        let at = [self.player.pos[0] + (f.x * 3.0) as f64 + k as f64 * 0.7, self.player.pos[1] + 0.5, self.player.pos[2] + (f.z * 3.0) as f64];
                        if arg(3) == "young" {
                            self.mobs.spawn_young(kind, at);
                        } else {
                            self.mobs.spawn(kind, at);
                        }
                    }
                    alloc::format!("SPAWNED {} {}", n, r.entities[kind].name.to_ascii_uppercase())
                }
                None => String::from("NO SUCH CREATURE. USE ITS ID, AS IN THE DATA FILES"),
            },
            "/mode" if !online => match arg(1) {
                "creative" => {
                    self.set_mode(GameMode::Creative);
                    String::from("CREATIVE MODE")
                }
                "survival" => {
                    self.set_mode(GameMode::Survival);
                    String::from("SURVIVAL MODE")
                }
                _ => String::from("USE  /MODE CREATIVE  OR  /MODE SURVIVAL"),
            },
            "/weather" if !online => {
                self.forced_sky = match arg(1) {
                    "clear" => Some(0.0),
                    "rain" => Some(0.7),
                    "storm" => Some(1.0),
                    _ => None,
                };
                String::from(if self.forced_sky.is_some() { "WEATHER SET" } else { "WEATHER BACK TO NORMAL" })
            }
            // /set X Y Z BLOCK [TURNS]: put a block somewhere (for building
            // test scenes); TURNS 0..3 picks which way it faces.
            "/set" if !online => match (coord(1, 0).map(|v| v as i32).ok_or(()), coord(2, 1).map(|v| v as i32).ok_or(()), coord(3, 2).map(|v| v as i32).ok_or(()), r.block(arg(4))) {
                (Ok(x), Ok(y), Ok(z), Some(b)) if (0..crate::chunk::CH).contains(&y) => {
                    let d = block::def(b);
                    let turn = arg(5).parse::<u16>().unwrap_or(0) % d.rotations.max(1) as u16;
                    if self.world.set_block(x, y, z, b + turn) {
                        String::from("SET")
                    } else {
                        String::from("THAT PLACE IS NOT LOADED")
                    }
                }
                _ => String::from("USE  /SET X Y Z BLOCK"),
            },
            // /dim NAME: step into another dimension of this world
            // (/dim alone, or /dim home, comes back).
            "/dim" if !online => {
                let id = if arg(1) == "home" { "" } else { arg(1) };
                if id.is_empty() || r.dimensions.iter().any(|d| d.0 == id) {
                    self.pending_dim = Some(String::from(id));
                    String::new()
                } else {
                    let names: Vec<&str> = r.dimensions.iter().map(|d| d.0.as_str()).collect();
                    alloc::format!("DIMENSIONS: HOME {}", names.join(" ").to_ascii_uppercase())
                }
            }
            "/seed" => alloc::format!("SEED {}", self.base_seed),
            "/time" | "/spawn" | "/mode" | "/weather" | "/set" | "/dim" | "/shoot" | "/aim" | "/strike" | "/blast" | "/episode" | "/strip" | "/flood" => String::from("NOT ON A SERVER"),
            // A command of some mod's own code, perhaps.
            _ => return crate::native::active().filter_map(|(_, n)| n.command).find_map(|command| command(self, &words)),
        })
    }

    /// Step into another dimension of this world (`""` is the world
    /// itself). Each has its own terrain and its own saved chunks.
    fn enter_dimension(&mut self, id: &str, sink: &mut dyn MeshSink) {
        let r = reg();
        let shape = match r.dimensions.iter().find(|d| d.0 == id) {
            Some(d) => d.1,
            None if id.is_empty() => worldgen::home_shape(&self.kind),
            None => return,
        };
        if id == self.dim || self.net.is_some() {
            return;
        }
        // Put away where we are.
        self.world.save_all(&mut self.store);
        self.store.flush();
        self.natives(|n, g| {
            if let Some(save) = n.save {
                save(g)
            }
        });
        self.world.free_meshes(sink);
        let seed = if id.is_empty() { self.base_seed } else { self.base_seed ^ id.bytes().fold(0x9E37_79B9u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)) };
        let threads = self.pool.thread_count();
        // The old workers finish and go before the land changes under them.
        self.pool = crate::jobs::Pool::new(0, seed);
        worldgen::set_shape(shape);
        self.pool = crate::jobs::Pool::new(threads, seed);
        let far = self.world.far_radius;
        self.world = crate::world::World::new(seed);
        self.world.far_radius = far;
        let prefix = if id.is_empty() { self.file_prefix.clone() } else { alloc::format!("{}{}.", self.file_prefix, id) };
        self.store = crate::save::RegionStore::new(crate::save::DiskIo, prefix.clone());
        self.prefix = prefix;
        self.natives(|n, g| {
            if let Some(open) = n.open {
                open(g)
            }
        });
        // Wild things stay behind; pets come along.
        self.mobs.list.retain(|m| m.tamed);
        self.shots.clear();
        self.sitting = None;
        self.target = None;
        self.mining = None;
        self.dim = String::from(id);
        // Somewhere to stand on arriving.
        let (x, z) = (floord(self.player.pos[0]), floord(self.player.pos[2]));
        let ground = worldgen::column(seed, x, z).height;
        let y = if shape >= 2 && ground < 4 {
            // Nothing there: a small platform to arrive on.
            for dx in -2..=2 {
                for dz in -2..=2 {
                    self.world.apply_remote(x + dx, 80, z + dz, r.wk.stone);
                }
            }
            81
        } else {
            ground + 1
        };
        self.player.pos = [x as f64 + 0.5, y as f64 + 0.05, z as f64 + 0.5];
        self.player.vel = [0.0; 3];
        self.peak_y = self.player.pos[1];
        self.was_airborne = false;
        for m in self.mobs.list.iter_mut() {
            m.body.pos = [self.player.pos[0] + 1.0, self.player.pos[1], self.player.pos[2] + 1.0];
        }
        self.fire("dimension", id);
    }

    /// Whether this program runs the creatures and the like: always when
    /// playing alone; in a shared game, for one player only.
    pub fn runs_the_world(&self) -> bool {
        self.net.as_ref().map_or(true, |n| n.runs_the_world())
    }

    /// Hit creature `i`. In a shared game run by someone else, they are
    /// told, and decide what becomes of it. Returns whether it died.
    fn strike(&mut self, i: usize, amount: i32, from: [f64; 3]) -> bool {
        if self.runs_the_world() {
            return self.mobs.hurt(i, amount, from);
        }
        let (id, health) = match self.mobs.list.get_mut(i) {
            Some(m) => {
                m.hurt = 0.35;
                (m.id, m.health)
            }
            None => return false,
        };
        if let Some(n) = self.net.as_mut() {
            n.send_state(crate::proto::channel::HIT, false, "", alloc::format!("{} {}", id, amount).as_bytes());
        }
        health - amount <= 0
    }

    /// Send something flying from the player's eye the way they look.
    pub fn loose(&mut self, damage: i32, speed: f32, leaves: block::Block, colour: [u8; 3]) {
        if self.shots.len() >= 64 {
            self.shots.remove(0);
        }
        let dir = forward(self.player.yaw, self.player.pitch);
        let eye = self.player.eye();
        self.shots.push(Shot {
            pos: [eye[0] + (dir.x * 0.6) as f64, eye[1] - 0.1 + (dir.y * 0.6) as f64, eye[2] + (dir.z * 0.6) as f64],
            vel: [dir.x * speed, dir.y * speed, dir.z * speed],
            damage,
            life: 5.0,
            leaves,
            colour,
            hostile: false,
        });
    }

    /// Move everything in flight; arrows drop, hit creatures and stop at blocks.
    fn fly_shots(&mut self, dt: f32) {
        let mut i = 0;
        while i < self.shots.len() {
            let (mut done, mut landed) = (false, None);
            {
                let s = &mut self.shots[i];
                s.life -= dt;
                s.vel[1] -= 12.0 * dt;
                // Short steps, so nothing thin is flown through.
                let steps = 4;
                for _ in 0..steps {
                    let before = s.pos;
                    for a in 0..3 {
                        s.pos[a] += (s.vel[a] * dt / steps as f32) as f64;
                    }
                    let c = [floord(s.pos[0]), floord(s.pos[1]), floord(s.pos[2])];
                    if block::is_solid(self.world.get(c[0], c[1], c[2])) {
                        landed = Some([floord(before[0]), floord(before[1]), floord(before[2])]);
                        done = true;
                        break;
                    }
                }
                done |= s.life <= 0.0 || s.pos[1] < -4.0;
            }
            let (pos, damage, leaves) = (self.shots[i].pos, self.shots[i].damage, self.shots[i].leaves);
            if self.shots[i].hostile {
                // Meant for the player.
                let (lo, hi) = self.player.aabb();
                if (0..3).all(|a| pos[a] > lo[a] - 0.2 && pos[a] < hi[a] + 0.2) {
                    self.pending_hurt += damage;
                    self.shots.swap_remove(i);
                } else if done {
                    self.shots.swap_remove(i);
                } else {
                    i += 1;
                }
                continue;
            }
            // A creature in the way takes the hit.
            let struck = self.mobs.list.iter().position(|m| {
                let (lo, hi) = m.body.aabb();
                (0..3).all(|a| pos[a] > lo[a] - 0.25 && pos[a] < hi[a] + 0.25)
            });
            if let Some(k) = struck {
                let kind = self.mobs.list[k].kind;
                if self.strike(k, damage, self.player.pos) {
                    if let (GameMode::Survival, Some((drop, n))) = (self.mode, reg().entities[kind].drop) {
                        self.inv.add(drop as u16, n);
                    }
                    let id = reg().entities[kind].id.clone();
                    self.fire("kill", &id);
                }
                self.sound_queue.push(String::from("hurt"));
                done = true;
                landed = None;
            }
            if let (Some(c), true) = (landed, leaves != AIR) {
                if interact::replaceable(self.world.get(c[0], c[1], c[2])) {
                    self.world.set_block(c[0], c[1], c[2], leaves);
                }
            }
            if done {
                self.shots.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Switch to new content while playing. Everything that refers to
    /// content by number (blocks in the world, items in slots) is
    /// carried across by name; creatures are simply let go.
    pub fn reload_content(&mut self, new: crate::block::Registry, sink: &mut dyn MeshSink) {
        let old_names = crate::save::current_palette();
        let inv = self.inv.save();
        let chests: Vec<([i32; 3], String)> = self.chests.iter().map(|(k, v)| (*k, v.save())).collect();
        self.natives(|n, g| {
            if let Some(reload) = n.reload {
                reload(g, true)
            }
        });
        // Chunks waiting to be written are in the old numbers: write them
        // now, under the old names, and hold nothing over.
        self.world.save_all(&mut self.store);
        self.store.flush_and_forget();
        self.pool.pause();
        block::replace(new);
        let r = reg();
        let map: Vec<block::Block> = old_names.iter().map(|n| r.state_by_name(n).unwrap_or(AIR)).collect();
        self.pool.resume(&map);
        self.world.remap(&map, sink);
        self.inv = Inventory::load(&inv);
        self.cursor = None;
        self.chests = chests.into_iter().map(|(k, v)| (k, Inventory::load(&v))).collect();
        self.natives(|n, g| {
            if let Some(reload) = n.reload {
                reload(g, false)
            }
        });
        self.mobs.list.clear();
        self.target = None;
        self.mining = None;
    }

    /// Blocks that act on their own: sand falls, crops grow, and things
    /// that lost what they stood on come loose.
    fn block_updates(&mut self, dt: f32) {
        self.tick_timer += dt;
        if self.tick_timer < 0.05 {
            return;
        }
        let step = self.tick_timer.min(0.25);
        self.tick_timer = 0.0;
        let r = reg();
        let mut pending = core::mem::take(&mut self.world.changed);
        if pending.len() > 600 {
            // Too much at once (a brush, an explosion): spread it out.
            self.world.changed = pending.split_off(600);
        }
        // Water and lava move on from whatever just changed (see game::fluid).
        crate::game::fluid::tick(&mut self.world, &pending);
        for c in pending {
            for cell in [c, [c[0], c[1] + 1, c[2]]] {
                let b = self.world.get(cell[0], cell[1], cell[2]);
                if b == AIR {
                    continue;
                }
                let d = block::def(b);
                let under = self.world.get(cell[0], cell[1] - 1, cell[2]);
                // One block down per step, so it is seen falling; the change
                // itself queues the next step and what sat on top.
                if interact::fall_step(&mut self.world, cell) {
                } else if !d.x.on.is_empty() && !d.x.on.contains(&block::def(under).first) {
                    interact::remove(&mut self.world, cell);
                    if let (GameMode::Survival, Some(drop)) = (self.mode, d.drop) {
                        self.inv.add(drop as u16, 1);
                    }
                }
            }
        }
        // Growth: look down a few random columns near the player. A plant
        // under open sky (or leaves, or glass) is found from above.
        let (px, pz) = (floord(self.player.pos[0]), floord(self.player.pos[2]));
        const SPAN: i32 = 48;
        const COLUMNS: u32 = 16;
        // How often, on average, any one column is looked at.
        let revisit = (SPAN * SPAN) as f32 * step / COLUMNS as f32;
        for _ in 0..COLUMNS {
            self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let x = px - SPAN / 2 + ((self.rng >> 8) % SPAN as u32) as i32;
            let z = pz - SPAN / 2 + ((self.rng >> 20) % SPAN as u32) as i32;
            if !self.world.is_loaded(x, z) {
                continue;
            }
            for y in (1..crate::chunk::CH).rev() {
                let b = self.world.get(x, y, z);
                if b == AIR {
                    continue;
                }
                let d = block::def(b);
                if let Some((next, seconds)) = d.x.grows {
                    self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    if ((self.rng >> 8) % 10_000) as f32 <= 10_000.0 * revisit / seconds {
                        let n = block::def(next);
                        let same = n.rotations == d.rotations && n.flips == d.flips;
                        self.world.set_block(x, y, z, if same { next + (b - d.first) } else { next });
                    }
                    break;
                }
                if matches!(r.kind[b as usize], crate::block::Kind::Cube | crate::block::Kind::Boxes | crate::block::Kind::Liquid) {
                    break;
                }
            }
        }
    }

    /// Weather, lightning, snow settling, and the packs' episodes.
    /// `weather` is the player's setting.
    pub fn weather_update(&mut self, dt: f32, weather: bool, audio: &mut Audio) {
        use crate::weather;
        let (px, pz) = (floord(self.player.pos[0]), floord(self.player.pos[2]));
        let seed = self.world.seed;
        self.sky = if weather { weather::sky_at(seed, px, pz, self.time * 1200.0) } else { weather::CLEAR };
        if let Some(rain) = self.forced_sky {
            self.sky = weather::Sky { rain, snow: self.sky.snow, storm: rain > 0.9 };
        }
        self.flash = (self.flash - dt).max(0.0);
        self.shake = (self.shake - dt * 0.5).max(0.0);
        self.weather_tick += dt;
        let mut roll = || {
            self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            self.rng >> 8
        };
        let tick = self.weather_tick >= 0.4;
        if tick {
            self.weather_tick = 0.0;
            if self.sky.rain > 0.2 && !self.sky.snow {
                audio.play(Sound::Rain, self.sky.rain);
            }
            // Near open water at sea level, waves wash in now and then;
            // louder the more water there is around.
            if roll() % 5 == 0 && (self.player.pos[1] as i32 - crate::worldgen::SEA).abs() < 14 {
                let near = [(-9, 0), (9, 0), (0, -9), (0, 9), (-6, -6), (6, 6), (-6, 6), (6, -6)]
                    .iter()
                    .filter(|d| crate::block::is_water(self.world.get(px + d.0, crate::worldgen::SEA, pz + d.1)))
                    .count();
                if near >= 2 {
                    audio.play(Sound::Wave, 0.25 + near as f32 * 0.09);
                }
            }
            if self.sky.storm && roll() % 24 == 0 {
                self.flash = 0.22;
                audio.play(Sound::Thunder, 1.0);
            }
            // Snow settles on open ground while it falls.
            let layer = reg().wk.snow_layer;
            if self.sky.snow && self.sky.rain > 0.25 && layer != AIR {
                let (x, z) = (px + (roll() % 25) as i32 - 12, pz + (roll() % 25) as i32 - 12);
                let top = (floord(self.player.pos[1]) + 24).min(crate::chunk::CH - 2);
                for y in (2..top).rev() {
                    let b = self.world.get(x, y, z);
                    if b != AIR {
                        if reg().kind[b as usize] == Kind::Cube && self.world.get(x, y + 1, z) == AIR {
                            self.world.set_block(x, y + 1, z, layer);
                        }
                        break;
                    }
                }
            }
        }
        // Episodes are a single-player affair: the server does not run them.
        if self.net.is_some() || reg().episodes.is_empty() {
            if self.episode.is_some() {
                self.episode = None;
                self.drain_flood(0);
            }
            return;
        }
        let run = match self.episode {
            Some(run) => run,
            None => {
                // About once a second, each kind a pack defines gets its
                // chance, if its conditions hold here and now.
                if tick && self.roll() % 3 == 0 {
                    let roll = self.roll();
                    for (k, d) in reg().episodes.iter().enumerate() {
                        if d.every > 0 && roll % d.every == k as u32 % d.every && self.episode_fits(k) {
                            self.start_episode(k);
                            break;
                        }
                    }
                }
                return;
            }
        };
        let def = &reg().episodes[run.kind];
        let (x, z) = (run.x + run.dx * dt, run.z + run.dz * dt);
        // A `/pull` in force drags the player in and throws them upward.
        self.pull.2 -= dt;
        if self.pull.2 > 0.0 && !self.player.flying && !self.carried {
            let (reach, strength) = (self.pull.0, self.pull.1);
            let (tx, tz) = (x - self.player.pos[0] as f32, z - self.player.pos[2] as f32);
            let d2 = tx * tx + tz * tz;
            if d2 < reach * reach {
                let d = sqrtf(d2).max(0.5);
                let pull = (reach - d) * strength * dt;
                self.player.vel[0] += tx / d * pull;
                self.player.vel[2] += tz / d * pull;
                if d < reach * 0.36 {
                    self.player.vel[1] = (self.player.vel[1] + 34.0 * dt).min(9.0);
                }
            }
        }
        let left = run.left - dt;
        self.episode = Some(Episode { x, z, left, ..run });
        if tick {
            for a in &def.actions {
                self.run(&a.to_ascii_lowercase(), 1);
            }
        }
        // (A command may have stopped it, or started another.)
        if left <= 0.0 && self.episode.map_or(false, |d| d.kind == run.kind) {
            self.episode = None;
            self.drain_flood(0);
            self.fire("episode_end", &def.id);
            if let Some(next) = reg().episodes.iter().position(|d| !def.then.is_empty() && d.id == def.then) {
                if self.episode.is_none() && self.episode_fits(next) {
                    self.start_episode(next);
                }
            }
        }
    }

    fn roll(&mut self) -> u32 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.rng >> 8
    }

    /// Whether the conditions a pack gave episode `k` hold where the
    /// player is.
    fn episode_fits(&mut self, k: usize) -> bool {
        let needs = reg().episodes[k].needs.clone();
        needs.iter().all(|c| {
            let words: Vec<&str> = c.split_whitespace().collect();
            self.holds(&words)
        })
    }

    /// Begin episode `k`. One that moves starts thirty blocks off and
    /// passes close by.
    pub fn start_episode(&mut self, k: usize) {
        let def = &reg().episodes[k];
        let (px, pz) = (self.player.pos[0] as f32, self.player.pos[2] as f32);
        let a = (self.roll() % 628) as f32 / 100.0;
        let (dx, dz) = (crate::math::sinf(a), crate::math::cosf(a));
        let far = if def.moves > 0.0 { 30.0 } else { 0.0 };
        self.episode = Some(Episode { kind: k, left: def.lasts, x: px - dx * far, z: pz - dz * far, dx: dx * def.moves, dz: dz * def.moves });
        self.fire("episode", &def.id);
    }

    /// Take back flood water standing above `level`.
    fn drain_flood(&mut self, level: i32) {
        let mut kept = Vec::new();
        for c in core::mem::take(&mut self.flood) {
            if c[1] > level {
                if crate::block::is_water(self.world.get(c[0], c[1], c[2])) {
                    self.world.set_block(c[0], c[1], c[2], AIR);
                }
            } else {
                kept.push(c);
            }
        }
        self.flood = kept;
    }

    fn net_update(&mut self, dt: f32) {
        for c in self.chat.iter_mut() {
            c.1 -= dt;
        }
        let held = self.held().map_or(0, |h| h.0 as u16 + 1);
        let polled = match self.net.as_mut() {
            Some(n) => {
                n.send_move(dt, self.player.pos, self.player.yaw, self.player.pitch, held);
                for (x, y, z, b) in self.world.edit_log.drain(..) {
                    n.send_block(x, y, z, b);
                }
                n.poll(dt)
            }
            None => return,
        };
        let mut events = core::mem::take(&mut self.held_events);
        events.extend(polled);
        if self.server_pack.is_some() {
            // The pack must be in use before anything after it is read.
            self.held_events = events;
            return;
        }
        let mut events = events.into_iter();
        while let Some(e) = events.next() {
            match e {
                crate::netplay::Event::Block(x, y, z, b) => self.world.apply_remote(x, y, z, b),
                crate::netplay::Event::Chat(line) => self.push_chat(line),
                crate::netplay::Event::Time(t) => self.time = t,
                crate::netplay::Event::Lost => self.lost = true,
                crate::netplay::Event::Skin(id, png, slim) => self.skin_events.push((id, png, slim)),
                crate::netplay::Event::State(_, channel, key, data) => {
                    use crate::proto::channel as ch;
                    let text = String::from_utf8_lossy(&data).into_owned();
                    let host = self.runs_the_world();
                    match channel {
                        ch::MOBS if !host => self.mobs.apply_snapshot(&text),
                        ch::HIT if host => {
                            let mut w = text.split(' ');
                            if let (Some(id), Some(n)) = (w.next().and_then(|v| v.parse::<u32>().ok()), w.next().and_then(|v| v.parse::<i32>().ok())) {
                                if let Some(i) = self.mobs.list.iter().position(|m| m.id == id) {
                                    let from = self.mobs.list[i].body.pos;
                                    self.mobs.hurt(i, n.clamp(0, 1000), [from[0], from[1], from[2] + 1.0]);
                                }
                            }
                        }
                        ch::CHEST => {
                            let c: Vec<i32> = key.split(' ').filter_map(|v| v.parse().ok()).collect();
                            if let [x, y, z] = c[..] {
                                if text.is_empty() {
                                    self.chests.remove(&[x, y, z]);
                                } else {
                                    self.chests.insert([x, y, z], Inventory::load(&text));
                                }
                                self.chest_sent = Some(([x, y, z], text));
                            }
                        }
                        // A mod's own code talking to itself on another
                        // player's machine.
                        c if c >= ch::NATIVE => {
                            if let Some(n) = crate::native::all().get((c - ch::NATIVE) as usize).filter(|n| reg().has(n.name)) {
                                if let Some(message) = n.message {
                                    message(self, &key, &text);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                crate::netplay::Event::Pack(last, data) => {
                    if self.pack_buffer.len() + data.len() <= 16 << 20 {
                        self.pack_buffer.extend_from_slice(&data);
                    }
                    if last {
                        self.server_pack = Some(core::mem::take(&mut self.pack_buffer));
                        self.held_events = events.collect();
                        break;
                    }
                }
            }
        }
    }

    pub fn held(&self) -> Option<(usize, &'static ItemDef)> {
        self.inv.slots[self.selected].map(|s| (s.item as usize, &reg().items[s.item as usize]))
    }

    fn reach(&self) -> f32 {
        match self.held() {
            Some((_, it)) if it.reach > 0.0 => it.reach,
            _ if self.mode == GameMode::Creative => 6.0,
            _ => 4.5,
        }
    }

    pub fn say(&mut self, text: &str) {
        self.note = (String::from(text), 2.0);
    }

    pub fn damage(&mut self, amount: i32, audio: &mut Audio) {
        if self.mode != GameMode::Survival || self.dead || amount <= 0 {
            return;
        }
        self.health -= amount;
        self.hurt_flash = 0.4;
        self.since_damage = 0.0;
        audio.play(Sound::Hurt, 1.0);
        if self.health <= 0 {
            self.health = 0;
            self.dead = true;
        }
    }

    /// Go to block column (x, z), arriving above the ground.
    pub fn travel_to(&mut self, x: f64, z: f64) {
        let top = worldgen::column(self.world.seed, floord(x), floord(z)).height;
        self.player.pos = [x, (top + 3).max(worldgen::SEA + 3) as f64, z];
        self.player.vel = [0.0; 3];
        self.peak_y = self.player.pos[1];
        self.was_airborne = false;
        self.say("ARRIVED");
    }

    pub fn respawn(&mut self) {
        self.player.pos = self.spawn;
        self.player.vel = [0.0; 3];
        self.peak_y = self.spawn[1];
        self.health = MAX_HEALTH;
        self.hunger = MAX_HUNGER;
        self.air = MAX_AIR;
        self.dead = false;
    }

    pub fn set_mode(&mut self, mode: GameMode) {
        self.mode = mode;
        if mode == GameMode::Survival {
            self.player.flying = false;
        }
    }

    fn retarget(&mut self) {
        self.target = self.world.raycast(self.player.eye(), forward(self.player.yaw, self.player.pitch), self.reach());
        // Aiming at any part of a big prop is aiming at the prop.
        if let Some(hit) = self.target.as_mut() {
            hit.cell = interact::prop_root(&self.world, hit.cell);
        }
    }

    /// Advance the world by `dt` seconds.
    pub fn update(&mut self, dt: f32, c: &Controls, distance: i32, far: i32, sink: &mut dyn MeshSink, audio: &mut Audio) {
        self.world.far_radius = if far > distance { far } else { 0 };
        self.clock += dt;
        self.note.1 -= dt;
        self.net_update(dt);
        self.hurt_flash = (self.hurt_flash - dt).max(0.0);
        // A full day lasts twenty minutes. (No `%` on floats: that would
        // need the C math library.)
        self.time += dt / 1200.0;
        if self.time >= 1.0 {
            self.time -= 1.0;
        }
        if c.toggle_fly && !self.carried {
            if self.mode == GameMode::Creative {
                self.player.flying = !self.player.flying;
            }
        }

        if let Some(id) = self.pending_dim.take() {
            self.enter_dimension(&id, sink);
        }
        self.block_updates(dt);
        self.fly_shots(dt);
        // Dropping the held item: it is tossed a little way ahead, falls,
        // and can be picked up again by walking over it.
        if c.drop && !self.dead {
            if let Some(item) = self.inv.take_one(self.selected) {
                let dir = forward(self.player.yaw, self.player.pitch);
                let eye = self.player.eye();
                if self.drops.len() >= 128 {
                    self.drops.remove(0);
                }
                self.drops.push(Dropped { pos: [eye[0], eye[1] - 0.3, eye[2]], vel: [dir.x * 5.0, dir.y * 5.0 + 2.0, dir.z * 5.0], item, count: 1, age: 0.0 });
            }
        }
        let (px, py, pz) = (self.player.pos[0], self.player.pos[1], self.player.pos[2]);
        let mut k = 0;
        while k < self.drops.len() {
            let d = &mut self.drops[k];
            d.age += dt;
            d.vel[1] -= 18.0 * dt;
            let next = [d.pos[0] + (d.vel[0] * dt) as f64, d.pos[1] + (d.vel[1] * dt) as f64, d.pos[2] + (d.vel[2] * dt) as f64];
            if block::is_solid(self.world.get(floord(next[0]), floord(next[1]), floord(next[2]))) {
                d.vel = [0.0; 3];
            } else {
                d.pos = next;
            }
            let near = (d.pos[0] - px) * (d.pos[0] - px) + (d.pos[2] - pz) * (d.pos[2] - pz) < 1.6 && d.pos[1] > py - 1.0 && d.pos[1] < py + 2.2;
            if d.age > 300.0 || d.pos[1] < -8.0 {
                self.drops.swap_remove(k);
            } else if near && d.age > 1.2 && !self.dead && self.inv.add(d.item, d.count) == 0 {
                self.drops.swap_remove(k);
                audio.play(Sound::Pop, 0.6);
            } else {
                k += 1;
            }
        }
        // Scripts: once at the start, and those that run on a clock.
        if !self.started {
            self.started = true;
            self.fire("start", "");
        }
        let scripts = &reg().scripts;
        if self.script_timers.len() != scripts.len() {
            self.script_timers = scripts.iter().map(|s| s.what.parse::<f32>().unwrap_or(0.0)).collect();
        }
        for k in 0..scripts.len() {
            if scripts[k].event == "every" {
                self.script_timers[k] -= dt;
                if self.script_timers[k] <= 0.0 {
                    self.script_timers[k] = scripts[k].what.parse::<f32>().unwrap_or(60.0).max(0.5);
                    for a in &scripts[k].actions {
                        self.run(&a.to_ascii_lowercase(), 1);
                    }
                }
            }
        }
        // Commands put off with /after, and the turn of day and night.
        let mut due = Vec::new();
        for l in self.later.iter_mut() {
            l.0 -= dt;
        }
        self.later.retain(|l| {
            if l.0 <= 0.0 {
                due.push(l.1.clone());
            }
            l.0 > 0.0
        });
        for line in due {
            self.run(&line, 1);
        }
        let night = crate::math::sinf(self.time * crate::math::TAU) < -0.05;
        if night != self.was_night {
            self.was_night = night;
            self.fire(if night { "night" } else { "day" }, "");
        }
        if self.dead && !self.death_told {
            self.death_told = true;
            self.fire("death", "");
        } else if !self.dead {
            self.death_told = false;
        }
        for name in core::mem::take(&mut self.sound_queue) {
            audio.play_named(&name, 1.0);
        }
        if self.pending_hurt > 0 {
            let n = core::mem::take(&mut self.pending_hurt);
            self.damage(n, audio);
        }
        // Remember what has been seen, for the minimap.
        self.explore_timer -= dt;
        if self.explore_timer <= 0.0 {
            self.explore_timer = 1.0;
            let reach = (distance.max(2) * 16) / EXPLORE + 1;
            let (tx, tz) = (floord(self.player.pos[0]).div_euclid(EXPLORE), floord(self.player.pos[2]).div_euclid(EXPLORE));
            for dx in -reach..=reach {
                for dz in -reach..=reach {
                    if dx * dx + dz * dz <= reach * reach {
                        self.explored.insert((tx + dx, tz + dz));
                    }
                }
            }
        }
        // Cobwebs and the like hold the player back.
        let (fx, fy, fz) = (floord(self.player.pos[0]), floord(self.player.pos[1] + 0.1), floord(self.player.pos[2]));
        let slow = block::def(self.world.get(fx, fy, fz)).x.slow.max(block::def(self.world.get(fx, fy + 1, fz)).x.slow);
        self.player.speed_scale = if slow > 0 { slow as f32 / 100.0 } else { 1.0 };
        // Movement. Until the ground under the player exists, hold still:
        // unloaded space counts as solid and must never be "stood on".
        let (px, pz) = (floord(self.player.pos[0]), floord(self.player.pos[2]));
        // Whatever the mods' own code runs moves now; it may carry the
        // player along (`seat`).
        self.seat = None;
        self.natives(|n, g| {
            if let Some(update) = n.update {
                update(g, dt, c, audio)
            }
        });
        let seat = self.seat;
        self.carried = seat.is_some();
        // On a creature's back: carried along until getting off (the
        // leave key), or until it is gone.
        if let Some(id) = self.riding_mob {
            let mount = self.mobs.list.iter().find(|m| m.id == id).map(|m| (m.body.pos, m.look().2));
            match mount {
                Some((pos, height)) if !c.leave && !self.dead => {
                    self.player.pos = [pos[0], pos[1] + height as f64 * 0.8 + 0.55 - crate::player::EYE, pos[2]];
                    self.player.vel = [0.0; 3];
                    self.player.on_ground = true;
                    self.peak_y = self.player.pos[1];
                    self.was_airborne = false;
                }
                _ => {
                    self.riding_mob = None;
                    self.mobs.rider = None;
                    self.player.pos[1] += 0.3;
                }
            }
        }
        // Seated: held in place on the chair until getting up (jump, or
        // the leave key), or until the chair is gone.
        if let Some(at) = self.sitting {
            let seat_height = block::def(self.world.get(at[0], at[1], at[2])).x.seat;
            if seat_height == 0 || c.input.jump || c.leave || self.dead || self.carried {
                self.sitting = None;
                self.player.pos[1] = at[1] as f64 + 1.05;
            }
        }
        if self.riding_mob.is_some() {
            // (Placed above; nothing more to move.)
        } else if let Some(at) = self.sitting {
            let seat_height = block::def(self.world.get(at[0], at[1], at[2])).x.seat as f64 / 16.0;
            // Eyes about 0.75 above the seat.
            self.player.pos = [at[0] as f64 + 0.5, at[1] as f64 + seat_height + 0.75 - crate::player::EYE, at[2] as f64 + 0.5];
            self.player.vel = [0.0; 3];
            self.player.on_ground = true;
            self.peak_y = self.player.pos[1];
            self.was_airborne = false;
        } else if let Some(eye) = seat {
            self.player.pos = [eye[0], eye[1] - crate::player::EYE, eye[2]];
            self.player.vel = [0.0; 3];
            self.player.on_ground = false;
            self.peak_y = self.player.pos[1];
            self.was_airborne = false;
        } else if self.world.is_loaded(px, pz) && !self.dead {
            let world = &self.world;
            let before = self.player.pos;
            let was_in_water = self.player.in_water;
            self.player.unstick(&|x, y, z| world.get(x, y, z));
            self.player.update(dt, &c.input, &|x, y, z| world.get_for_physics(x, y, z));
            let (dx, dz) = ((self.player.pos[0] - before[0]) as f32, (self.player.pos[2] - before[2]) as f32);
            if self.player.on_ground {
                self.step_distance += sqrtf(dx * dx + dz * dz);
                if self.step_distance > 2.1 {
                    self.step_distance = 0.0;
                    audio.play(Sound::Step, 1.0);
                }
            }
            if self.player.in_water && !was_in_water {
                audio.play(Sound::Splash, (-self.player.vel[1] * 0.12).clamp(0.2, 1.0));
            }
            self.survival_rules(dt, audio);
        }
        // Creatures. Hunters only come for players in survival mode.
        let hunted = self.mode == GameMode::Survival && !self.dead;
        let held_item = self.held().map(|h| h.0);
        // In a shared game one player's program runs the creatures and
        // whatever else moves by itself; the others are told where it all is.
        let host = self.runs_the_world();
        self.mobs.rider = self.riding_mob.map(|id| (id, c.input.clone(), self.player.yaw));
        let bitten = if host {
            self.mobs.update(dt, &self.world, self.player.pos, self.time, hunted, held_item)
        } else {
            self.mobs.animate(dt);
            // Their bite is still judged here, where the player is.
            self.bite_timer -= dt;
            let reach = self.mobs.list.iter().find(|m| {
                let d = &reg().entities[m.kind];
                let (dx, dy, dz) = (m.body.pos[0] - self.player.pos[0], m.body.pos[1] - self.player.pos[1], m.body.pos[2] - self.player.pos[2]);
                d.behavior == crate::block::Behavior::Hostile && !m.tamed && dx * dx + dz * dz < 1.7 && dy > -1.6 && dy < 2.0
            });
            match reach {
                Some(m) if hunted && self.bite_timer <= 0.0 => {
                    self.bite_timer = 1.0;
                    reg().entities[m.kind].damage
                }
                _ => 0,
            }
        };
        self.share_timer -= dt;
        if self.share_timer <= 0.0 && self.net.is_some() {
            self.share_timer = 0.2;
            use crate::proto::channel as ch;
            // The chest in front of the player, when what is in it changed.
            let chest = self.open_chest.filter(|c| self.chests.contains_key(c)).map(|c| (c, self.chests[&c].save()));
            let chest = chest.filter(|c| self.chest_sent.as_ref() != Some(c));
            let mobs = if host { Some(self.mobs.snapshot()) } else { None };
            if let Some(n) = self.net.as_mut() {
                if let Some(text) = mobs {
                    n.send_state(ch::MOBS, false, "", text.as_bytes());
                }
                if let Some((c, text)) = &chest {
                    n.send_state(ch::CHEST, true, &alloc::format!("{} {} {}", c[0], c[1], c[2]), text.as_bytes());
                }
            }
            if chest.is_some() {
                self.chest_sent = chest;
            }
            self.natives(|n, g| {
                if let Some(share) = n.share {
                    share(g)
                }
            });
        }
        // Now and then a creature nearby makes its call.
        self.call_timer -= dt;
        if self.call_timer <= 0.0 && dt > 0.0 {
            self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            self.call_timer = 1.5 + ((self.rng >> 8) % 300) as f32 / 100.0;
            let n = self.mobs.list.len();
            if n > 0 {
                let m = &self.mobs.list[(self.rng >> 16) as usize % n];
                let name = &reg().entities[m.kind].x.sound;
                let (dx, dz) = ((m.body.pos[0] - self.player.pos[0]) as f32, (m.body.pos[2] - self.player.pos[2]) as f32);
                let far = crate::math::sqrtf(dx * dx + dz * dz);
                if !name.is_empty() && far < 24.0 {
                    audio.play_named(name, (1.0 - far / 24.0).max(0.1));
                }
            }
        }
        for a in core::mem::take(&mut self.mobs.actions) {
            // On a server each player has their own villagers, so their
            // building stays imaginary there.
            match a {
                crate::entity::Action::Set(c, b) if self.net.is_none() => {
                    self.world.set_block(c[0], c[1], c[2], b);
                }
                crate::entity::Action::Shoot(from, dir, damage) => {
                    if self.shots.len() < 64 {
                        self.shots.push(Shot { pos: from, vel: [dir[0] * 26.0, dir[1] * 26.0, dir[2] * 26.0], damage, life: 4.0, leaves: AIR, colour: [255, 120, 90], hostile: true });
                    }
                    audio.play_named("bow", 0.6);
                }
                _ => {}
            }
        }
        if bitten > 0 {
            self.damage(bitten, audio);
        }

        let (cx, cz) = chunk_at(self.player.pos[0], self.player.pos[2]);
        self.world.update(cx, cz, distance, &mut self.pool, sink, &mut self.store);

        self.retarget();
        for t in self.cooldown.iter_mut() {
            *t -= dt;
        }
        if !self.dead {
            self.use_items(dt, c, audio);
        }

        self.autosave += dt;
    }

    fn survival_rules(&mut self, dt: f32, audio: &mut Audio) {
        // What the player stands in or on: magma burns, cactus pricks.
        // Every block the body touches counts: standing in or on one, or
        // brushing against its side.
        let (mn, mx) = self.player.aabb();
        let grow = 0.15;
        let mut touch = 0u8;
        for x in floord(mn[0] - grow)..=floord(mx[0] + grow) {
            for y in floord(mn[1] - grow)..=floord(mx[1] + grow) {
                for z in floord(mn[2] - grow)..=floord(mx[2] + grow) {
                    touch = touch.max(block::def(self.world.get(x, y, z)).x.hurts);
                }
            }
        }
        if touch > 0 && !self.dead {
            self.touch_timer += dt;
            if self.touch_timer >= 1.0 {
                self.touch_timer = 0.0;
                self.damage(touch as i32, audio);
            }
        } else {
            self.touch_timer = 0.6;
        }
        // Falling: measured from the highest point since leaving the ground.
        let (y, on_ground) = (self.player.pos[1], self.player.on_ground);
        let airborne = !on_ground && !self.player.flying && !self.player.in_water;
        if airborne {
            self.peak_y = if self.was_airborne { y.max(self.peak_y) } else { y };
        } else {
            if self.was_airborne && on_ground {
                let fell = (self.peak_y - y) as f32;
                if fell > 3.5 {
                    self.damage((fell - 3.0) as i32, audio);
                } else if fell > 1.2 {
                    audio.play(Sound::Step, 1.0);
                }
            }
            self.peak_y = y;
        }
        self.was_airborne = airborne;
        if self.mode != GameMode::Survival {
            self.air = MAX_AIR;
            return;
        }
        // Breath.
        let eye = self.player.eye();
        let head = self.world.get(floord(eye[0]), floord(eye[1]), floord(eye[2]));
        if reg().kind[head as usize] == Kind::Liquid {
            self.air = (self.air - dt).max(0.0);
            if self.air <= 0.0 {
                self.drown += dt;
                if self.drown >= 1.0 {
                    self.drown = 0.0;
                    self.damage(2, audio);
                }
            }
        } else {
            self.air = (self.air + dt * 4.0).min(MAX_AIR);
            self.drown = 0.0;
        }
        // Hunger: a point every 45 seconds, faster when sprinting about.
        let moving = self.player.vel[0] * self.player.vel[0] + self.player.vel[2] * self.player.vel[2] > 20.0;
        self.hunger_clock += dt * if moving { 2.0 } else { 1.0 };
        if self.hunger_clock >= 45.0 {
            self.hunger_clock = 0.0;
            if self.hunger > 0 {
                self.hunger -= 1;
            } else {
                self.damage(1, audio);
            }
        }
        // Slow healing once things have been quiet for a while, if fed.
        self.since_damage += dt;
        if self.health < MAX_HEALTH && self.since_damage > 6.0 && self.hunger > 6 {
            self.regen += dt;
            if self.regen >= 3.0 {
                self.regen = 0.0;
                self.health += 1;
            }
        }
    }

    fn use_items(&mut self, dt: f32, c: &Controls, audio: &mut Audio) {
        let creative = self.mode == GameMode::Creative;
        let none = ItemDef {
            id: String::new(),
            name: String::new(),
            icon: 0,
            place: None,
            brush: 0,
            reach: 0.0,
            stack: 1,
            tool: 0,
            speed: 1.0,
            food: 0,
            section: None,
            category: String::new(),
            x: Default::default(),
        };
        let held = self.held();
        let item: &ItemDef = held.map_or(&none, |h| h.1);

        // The mods' own code gets the click first (things to ride in, say).
        let eye = self.player.eye();
        let dir = forward(self.player.yaw, self.player.pitch);
        let near_block = self.target.as_ref().map_or(f32::MAX, |h| h.dist);
        let mut used = false;
        self.natives(|n, g| {
            if let (false, Some(interact)) = (used, n.interact) {
                used = interact(g, c, near_block, audio);
            }
        });
        if used || self.carried {
            return;
        }

        // Right click on a creature: collect what it gives, or trade.
        if c.place_held && self.cooldown[1] <= 0.0 {
            if let Some((i, t)) = self.mobs.hit_test(eye, dir, 4.0) {
                if self.target.as_ref().map_or(true, |hit| t < hit.dist) {
                    use crate::entity::Interaction;
                    self.cooldown[1] = 0.5;
                    let inv = &self.inv;
                    match self.mobs.interact(i, held.map(|h| h.0), &|item| inv.count(item as u16)) {
                        Interaction::Give(item, n) => {
                            self.inv.add(item as u16, n);
                            audio.play(Sound::Pop, 0.8);
                            let name = reg().items[item].name.to_ascii_uppercase();
                            self.say(&alloc::format!("GOT {}", name));
                            return;
                        }
                        Interaction::Trade((want, n), (give, count)) => {
                            if self.inv.remove(want as u16, n) {
                                self.inv.add(give as u16, count);
                                audio.play(Sound::Pop, 0.8);
                                let name = reg().items[give].name.to_ascii_uppercase();
                                self.say(&alloc::format!("TRADED FOR {} {}", count, name));
                            }
                            return;
                        }
                        Interaction::Say(text) => {
                            self.say(&text);
                            return;
                        }
                        Interaction::Ride(id) => {
                            if let Some(m) = self.mobs.list.get(i) {
                                let what = reg().entities[m.kind].id.clone();
                                self.fire("ride", &what);
                            }
                            self.riding_mob = Some(id);
                            self.sitting = None;
                            return;
                        }
                        Interaction::Tame(item) => {
                            if !creative {
                                self.inv.remove(item as u16, 1);
                            }
                            audio.play(Sound::Pop, 1.0);
                            let name = self.mobs.list.get(i).map(|m| m.title().to_ascii_uppercase()).unwrap_or_default();
                            self.say(&alloc::format!("THE {} IS NOW YOURS", name));
                            self.fire("tame", &reg().entities[self.mobs.list[i].kind].id);
                            return;
                        }
                        Interaction::Nothing => {}
                    }
                }
            }
        }

        // Hitting a creature comes before breaking what is behind it.
        let mut struck = false;
        if c.break_held && self.cooldown[0] <= 0.0 {
            let eye = self.player.eye();
            let dir = forward(self.player.yaw, self.player.pitch);
            if let Some((i, t)) = self.mobs.hit_test(eye, dir, self.reach().min(4.5)) {
                if self.target.as_ref().map_or(true, |hit| t < hit.dist) {
                    struck = true;
                    self.cooldown[0] = 0.35;
                    let kind = self.mobs.list[i].kind;
                    let killed = self.strike(i, if creative { 1000 } else { item.x.damage.max(3) }, eye);
                    if let (true, false, Some((drop, n))) = (killed, creative, reg().entities[kind].drop) {
                        self.inv.add(drop as u16, n);
                    }
                    audio.play(if killed { Sound::Pop } else { Sound::Hurt }, 0.8);
                    self.fire("hit", &reg().entities[kind].id);
                    if killed {
                        self.fire("kill", &reg().entities[kind].id);
                    }
                }
            }
        }

        // Breaking.
        let mut mining = None;
        if let (true, false, Some(hit)) = (c.break_held, struck, &self.target) {
            let b = self.world.get(hit.cell[0], hit.cell[1], hit.cell[2]);
            let d = block::def(b);
            if !d.unbreakable && b != AIR {
                if creative {
                    if self.cooldown[0] <= 0.0 {
                        self.cooldown[0] = 0.2;
                        // What was inside goes with it.
                        self.chests.remove(&hit.cell);
                        interact::break_at(&mut self.world, item, hit);
                        audio.play_named(if d.x.sound.is_empty() { "break" } else { &d.x.sound }, 1.0);
                        self.fire("break", &d.id);
                    }
                } else {
                    let progress = match self.mining {
                        Some((cell, p)) if cell == hit.cell => p,
                        _ => 0.0,
                    };
                    // The right kind of tool digs several times faster.
                    let speed = if item.tool != 0 && item.tool == d.tool { item.speed } else { 1.0 };
                    let progress = if d.hardness <= 0.0 { 1.0 } else { progress + dt * speed / d.hardness };
                    if progress >= 1.0 {
                        if self.cooldown[0] <= 0.0 {
                            self.cooldown[0] = 0.12;
                            let cell = hit.cell;
                            interact::remove(&mut self.world, cell);
                            audio.play_named(if d.x.sound.is_empty() { "break" } else { &d.x.sound }, 1.0);
                            self.fire("break", &d.id);
                            // A broken chest hands over what was inside.
                            if let Some(inside) = self.chests.remove(&cell) {
                                for s in inside.slots.iter().flatten() {
                                    self.inv.add(s.item, s.count);
                                }
                            }
                            self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                            if let Some((extra, chance, n)) = d.x.also {
                                if (self.rng >> 8) % 100 < chance as u32 {
                                    self.inv.add(extra as u16, n);
                                }
                            }
                            let lucky = (self.rng >> 16) % 100 < d.drop_chance as u32;
                            if let (Some(drop), true) = (d.drop, lucky) {
                                if self.inv.add(drop as u16, 1) > 0 {
                                    self.say("INVENTORY FULL");
                                } else {
                                    audio.play(Sound::Pop, 0.6);
                                }
                            }
                        }
                    } else {
                        mining = Some((hit.cell, progress));
                    }
                }
            }
        }
        self.mining = mining;

        // Using: a crafting table or chest in front of you opens; food is eaten.
        if c.place_held && self.cooldown[1] <= 0.0 && !c.input.descend {
            if let Some(hit) = &self.target {
                let aimed = block::def(self.world.get(hit.cell[0], hit.cell[1], hit.cell[2]));
                let u = aimed.usable;
                if u == Usable::Bed {
                    self.cooldown[1] = 0.6;
                    self.spawn = [hit.cell[0] as f64 + 0.5, hit.cell[1] as f64 + 1.0, hit.cell[2] as f64 + 0.5];
                    let night = crate::math::sinf(self.time * crate::math::TAU) < -0.05;
                    if night && self.net.is_none() {
                        self.time = 0.0;
                        self.say("GOOD MORNING");
                    } else {
                        self.say(if night { "YOU WILL WAKE UP HERE" } else { "YOU CAN SLEEP WHEN IT IS DARK" });
                    }
                    return;
                }
                if u != Usable::Nothing {
                    self.cooldown[1] = 0.3;
                    self.opened = Some((u, hit.cell));
                    if u == Usable::Chest {
                        self.open_chest = Some(hit.cell);
                    }
                    return;
                }
                // Chairs and couches: sit down.
                if aimed.x.seat > 0 && !self.carried {
                    self.cooldown[1] = 0.4;
                    self.sitting = Some(hit.cell);
                    return;
                }
                // Food set out on a table: eat it there.
                if aimed.x.eat > 0 && (creative || self.hunger < MAX_HUNGER || self.health < MAX_HEALTH) {
                    self.cooldown[1] = 0.5;
                    self.hunger = (self.hunger + aimed.x.eat as i32).min(MAX_HUNGER);
                    self.health = (self.health + aimed.x.eat as i32 / 2).min(MAX_HEALTH);
                    interact::remove(&mut self.world, hit.cell);
                    audio.play(Sound::Pop, 0.8);
                    return;
                }
                if self.listens("use", &aimed.id) {
                    self.cooldown[1] = 0.4;
                    self.fire("use", &aimed.id);
                    return;
                }
                // Doors, gates and the like open and shut.
                if aimed.x.toggle.is_some() {
                    self.cooldown[1] = 0.3;
                    interact::toggle(&mut self.world, hit.cell);
                    audio.play_named(if aimed.x.sound.is_empty() { "door" } else { &aimed.x.sound }, 0.8);
                    return;
                }
                // Tools that work the block they touch (a hoe on soil).
                if let Some(&(_, to)) = item.x.turns.iter().find(|t| t.0 == aimed.first) {
                    self.cooldown[1] = 0.3;
                    self.world.set_block(hit.cell[0], hit.cell[1], hit.cell[2], to);
                    audio.play(Sound::Break, 0.7);
                    return;
                }
            }
            if item.food > 0 && !creative && (self.hunger < MAX_HUNGER || self.health < MAX_HEALTH) {
                self.cooldown[1] = 0.6;
                self.hunger = (self.hunger + item.food).min(MAX_HUNGER);
                self.health = (self.health + item.food / 2).min(MAX_HEALTH);
                self.inv.take_one(self.selected);
                audio.play(Sound::Pop, 0.8);
                self.fire("eat", &item.id);
                return;
            }
            // Spawn eggs hatch where they are pointed.
            if let (Some(kind), Some(hit)) = (item.x.egg, &self.target) {
                self.cooldown[1] = 0.35;
                let at = [hit.prev[0] as f64 + 0.5, hit.prev[1] as f64 + 0.05, hit.prev[2] as f64 + 0.5];
                if self.runs_the_world() && self.mobs.list.len() < 200 {
                    self.mobs.spawn(kind, at);
                    audio.play(Sound::Pop, 0.9);
                    if !creative {
                        self.inv.take_one(self.selected);
                    }
                }
                return;
            }
            // Bows and the like.
            if let Some((damage, speed, wait, ammo)) = item.x.shoots {
                let loaded = creative || ammo.map_or(true, |a| self.inv.remove(a as u16, 1));
                self.cooldown[1] = wait;
                if loaded {
                    self.loose(damage, speed, AIR, [230, 220, 200]);
                    audio.play_named("bow", 0.9);
                } else {
                    self.say("OUT OF AMMUNITION");
                }
                return;
            }
            // Items packs have given a use of their own (wands, horns...).
            if !item.id.is_empty() && self.listens("useitem", &item.id) {
                self.cooldown[1] = 0.4;
                self.fire("useitem", &item.id);
                return;
            }
        }

        // Placing.
        if c.place_held && self.cooldown[1] <= 0.0 {
            if let (Some(hit), Some((index, it))) = (&self.target, held) {
                // Area tools are a creative-mode convenience.
                if creative || it.brush == 0 {
                    self.cooldown[1] = 0.2;
                    if interact::place_at(&mut self.world, &self.player, it, hit) {
                        let placed = it.place.map(|b| block::def(b).x.sound.as_str()).filter(|n| !n.is_empty()).unwrap_or("place");
                        audio.play_named(placed, 1.0);
                        if let Some(b) = it.place {
                            self.fire("place", &block::def(b).id);
                        }
                        if !creative {
                            debug_assert_eq!(self.inv.slots[self.selected].map(|s| s.item as usize), Some(index));
                            self.inv.take_one(self.selected);
                        }
                    }
                }
            }
        }

        // Picking: select (or in creative, fetch) the block looked at.
        if c.pick {
            if let Some(i) = self.target.as_ref().and_then(|hit| interact::pick(&self.world, hit)) {
                let have = self.inv.slots[..HOTBAR].iter().position(|s| s.map_or(false, |s| s.item as usize == i));
                match have {
                    Some(slot) => self.selected = slot,
                    None if creative => {
                        // Prefer an empty hotbar slot over replacing what is held.
                        let slot = if self.inv.slots[self.selected].is_none() {
                            self.selected
                        } else {
                            self.inv.slots[..HOTBAR].iter().position(|s| s.is_none()).unwrap_or(self.selected)
                        };
                        self.inv.slots[slot] = Some(Stack { item: i as u16, count: reg().items[i].stack });
                        self.selected = slot;
                    }
                    None => {}
                }
            }
        }
        if c.break_held || c.place_held {
            self.retarget();
        }
    }

    /// Whether it is time for a periodic save.
    pub fn autosave_due(&mut self, every: f32) -> bool {
        if self.autosave >= every {
            self.autosave = 0.0;
            true
        } else {
            false
        }
    }

    /// Write everything to disk. Returns false if something could not be written.
    pub fn save(&mut self) -> bool {
        if self.net.is_some() {
            return true; // the server keeps multiplayer worlds
        }
        self.world.save_all(&mut self.store);
        self.store.flush();
        let mut chest_text = String::new();
        for (pos, inv) in &self.chests {
            if inv.slots.iter().any(|s| s.is_some()) {
                chest_text.push_str(&alloc::format!("{} {} {} {}\n", pos[0], pos[1], pos[2], inv.save()));
            }
        }
        sys::write_file(&self.chests_path, chest_text.as_bytes());
        self.natives(|n, g| {
            if let Some(save) = n.save {
                save(g)
            }
        });
        let p = &self.player;
        if self.net.is_none() {
            let mut text = String::new();
            for (x, z) in &self.explored {
                text.push_str(&alloc::format!("{} {}\n", x, z));
            }
            sys::write_file(&self.explored_path, text.as_bytes());
            sys::write_file(&self.waypoints_path, crate::game::waypoints::format(&self.waypoints).as_bytes());
            sys::write_file(&self.pets_path, self.mobs.save_pets().as_bytes());
            let vars: String = self.vars.iter().map(|(k, v)| alloc::format!("{}={}\n", k, v)).collect();
            sys::write_file(&self.vars_path, vars.as_bytes());
        }
        let text = alloc::format!(
            "seed={}\ndim={}\nkind={}\ngamemode={}\npos={} {} {}\nspawn={} {} {}\nyaw={}\npitch={}\nflying={}\ntime={}\nselected={}\nhealth={}\nhunger={}\ninventory={}\n",
            self.base_seed,
            self.dim,
            self.kind.to_text(),
            if self.mode == GameMode::Creative { "creative" } else { "survival" },
            p.pos[0],
            p.pos[1],
            p.pos[2],
            self.spawn[0],
            self.spawn[1],
            self.spawn[2],
            p.yaw,
            p.pitch,
            p.flying as i32,
            self.time,
            self.selected,
            self.health.max(1),
            self.hunger,
            self.inv.save(),
        );
        sys::write_file(&self.meta_path, text.as_bytes()) && !self.store.write_failed
    }
}

/// What a new player starts with: nothing, in either mode. In creative the
/// items come from the creative list.
pub fn starting_inventory(_mode: GameMode) -> Inventory {
    Inventory::new()
}
