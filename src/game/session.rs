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
    /// Get out of the vehicle being ridden.
    pub leave: bool,
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
    pub world: World,
    pub pool: Pool,
    pub store: RegionStore<DiskIo>,
    pub player: Player,
    pub mobs: Mobs,
    pub vehicles: crate::vehicle::Vehicles,
    vehicles_path: String,
    /// Where the player has been, in squares of [`EXPLORE`] blocks: what the
    /// minimap shows.
    pub explored: alloc::collections::BTreeSet<(i32, i32)>,
    explored_path: String,
    pets_path: String,
    explore_timer: f32,
    /// A vehicle whose storage the player just asked to open.
    pub opened_vehicle: Option<usize>,
    /// The server connection, in multiplayer.
    pub net: Option<crate::netplay::Client>,
    /// Recent chat and notices, with seconds left on screen.
    pub chat: Vec<(String, f32)>,
    /// The server went away.
    pub lost: bool,
    /// The weather overhead, and any disaster in progress.
    pub sky: crate::weather::Sky,
    pub disaster: Option<crate::weather::Disaster>,
    /// Camera shake strength (earthquakes).
    pub shake: f32,
    /// Lightning: counts down from a flash.
    pub flash: f32,
    weather_tick: f32,
    /// Place notices in Earth worlds: the detail level wanted (0 = off),
    /// the last name shown, and a timer between questions.
    pub place_level: i32,
    last_place: String,
    place_timer: f32,
    place_asked: bool,
    /// Water put down by a tsunami, to be taken back when it drains.
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
    /// Seconds each `when every N` script has left.
    script_timers: Vec<f32>,
    started: bool,
    call_timer: f32,
    /// The seat the player is sitting on.
    pub sitting: Option<[i32; 3]>,
    /// The cart a chain has been hooked to, waiting for the second one.
    coupling: Option<usize>,
    /// Which dimension of the world is being played (empty = the world itself).
    pub dim: String,
    pending_dim: Option<String>,
    /// The world's own seed (each dimension's is made from it) and file prefix.
    base_seed: u32,
    file_prefix: String,
    /// Rain set by `/weather` (0 clear ..= 1 storm), overriding the sky.
    pub forced_sky: Option<f32>,
    /// Skin images that arrived from other players, for the renderer.
    pub skin_events: Vec<(u16, Vec<u8>)>,
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
    cooldown: [f32; 2],
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
    alloc::format!("{}{}.meta.txt", base, name)
}

/// Names of the worlds that exist, from the index file.
pub fn list_worlds(base: &str) -> Vec<String> {
    let text = sys::read_file(&alloc::format!("{}worlds.txt", base)).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    for l in text.lines() {
        let n = crate::settings::clean_name(l.trim(), 24);
        if !n.is_empty() && !names.contains(&n) && sys::read_file(&meta_path(base, &n)).is_some() {
            names.push(n);
        }
    }
    names
}

pub fn remember_world(base: &str, name: &str) {
    let mut names = list_worlds(base);
    if !names.iter().any(|n| n == name) {
        names.push(String::from(name));
        sys::write_file(&alloc::format!("{}worlds.txt", base), names.join("\n").as_bytes());
    }
}

pub fn world_exists(base: &str, name: &str) -> bool {
    sys::read_file(&meta_path(base, name)).is_some()
}

impl Session {
    /// Open the world called `name`, creating it from `new` if it does not
    /// exist yet. `threads` is the number of worker threads.
    pub fn open(base: &str, name: &str, new: &NewWorld, threads: usize) -> Session {
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
        let chests_path = alloc::format!("{}{}.chests.txt", base, name);
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
        Session {
            name: String::from(name),
            world: World::new(seed),
            pool: Pool::new(threads, seed),
            store: RegionStore::new(DiskIo, alloc::format!("{}{}.", base, name)),
            peak_y: player.pos[1],
            player,
            mobs: {
                let mut m = Mobs::new(seed ^ 0xA5A5_1234);
                if let Some(text) = sys::read_file(&alloc::format!("{}{}.pets.txt", base, name)) {
                    m.load_pets(&String::from_utf8_lossy(&text));
                }
                m
            },
            vehicles: {
                let mut v = crate::vehicle::Vehicles::default();
                if let Some(text) = sys::read_file(&alloc::format!("{}{}.vehicles.txt", base, name)) {
                    v.load_text(&String::from_utf8_lossy(&text));
                }
                v
            },
            vehicles_path: alloc::format!("{}{}.vehicles.txt", base, name),
            explored: sys::read_file(&alloc::format!("{}{}.explored.txt", base, name))
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
            explored_path: alloc::format!("{}{}.explored.txt", base, name),
            pets_path: alloc::format!("{}{}.pets.txt", base, name),
            explore_timer: 0.0,
            opened_vehicle: None,
            net: None,
            chat: Vec::new(),
            lost: false,
            sky: crate::weather::CLEAR,
            disaster: None,
            shake: 0.0,
            flash: 0.0,
            weather_tick: 0.0,
            place_level: 0,
            last_place: String::new(),
            place_timer: 4.0,
            place_asked: false,
            flood: Vec::new(),
            server_pack: None,
            pack_buffer: Vec::new(),
            held_events: Vec::new(),
            tick_timer: 0.0,
            touch_timer: 0.0,
            sound_queue: Vec::new(),
            pending_hurt: 0,
            shots: Vec::new(),
            script_timers: Vec::new(),
            started: false,
            call_timer: 3.0,
            sitting: None,
            coupling: None,
            dim: String::new(),
            pending_dim: crate::settings::kv(&meta, "dim").filter(|d| !d.is_empty()).map(String::from),
            base_seed: seed,
            file_prefix: alloc::format!("{}{}.", base, name),
            forced_sky: None,
            skin_events: Vec::new(),
            inv,
            cursor: None,
            selected: kv(&meta, "selected").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0).min(HOTBAR - 1),
            mode,
            kind,
            health: kv(&meta, "health").and_then(|v| v.parse::<i32>().ok()).unwrap_or(MAX_HEALTH).clamp(1, MAX_HEALTH),
            hunger: kv(&meta, "hunger").and_then(|v| v.parse::<i32>().ok()).unwrap_or(MAX_HUNGER).clamp(0, MAX_HUNGER),
            hunger_clock: 0.0,
            chests,
            opened: None,
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
        }
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
        Some(match arg(0) {
            // ---- For scripts mostly: things that happen to the player.
            "/say" => {
                self.push_chat(words[1.min(words.len())..].join(" ").to_ascii_uppercase());
                String::new()
            }
            "/sound" => {
                self.sound_queue.push(String::from(arg(1)));
                String::new()
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
            "/spawn" if !online => match r.entities.iter().position(|e| e.id == arg(1)) {
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
            "/time" | "/spawn" | "/mode" | "/weather" | "/set" | "/dim" | "/shoot" | "/aim" | "/strike" => String::from("NOT ON A SERVER"),
            _ => return None,
        })
    }

    /// Step into another dimension of this world (`""` is the world
    /// itself). Each has its own terrain and its own saved chunks.
    fn enter_dimension(&mut self, id: &str, sink: &mut dyn MeshSink) {
        let r = reg();
        let shape = match r.dimensions.iter().find(|d| d.0 == id) {
            Some(d) => d.1,
            None if id.is_empty() => 0,
            None => return,
        };
        if id == self.dim || self.net.is_some() {
            return;
        }
        // Put away where we are.
        self.world.save_all(&mut self.store);
        self.store.flush();
        sys::write_file(&self.vehicles_path, self.vehicles.save_text().as_bytes());
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
        self.vehicles_path = alloc::format!("{}vehicles.txt", prefix);
        self.vehicles = crate::vehicle::Vehicles::default();
        if let Some(text) = sys::read_file(&self.vehicles_path) {
            self.vehicles.load_text(&String::from_utf8_lossy(&text));
        }
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
                if self.mobs.hurt(k, damage, self.player.pos) {
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
    /// content by number (blocks in the world, items in slots, vehicles) is
    /// carried across by name; creatures are simply let go.
    pub fn reload_content(&mut self, new: crate::block::Registry, sink: &mut dyn MeshSink) {
        let old_names = crate::save::current_palette();
        let inv = self.inv.save();
        let chests: Vec<([i32; 3], String)> = self.chests.iter().map(|(k, v)| (*k, v.save())).collect();
        let vehicles = self.vehicles.save_text();
        let riding = self.vehicles.riding;
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
        self.vehicles = crate::vehicle::Vehicles::default();
        self.vehicles.load_text(&vehicles);
        self.vehicles.riding = riding.filter(|&i| i < self.vehicles.list.len());
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

    /// Weather, lightning, snow settling, and disasters. `weather` and
    /// `disasters` are the player's settings.
    pub fn weather_update(&mut self, dt: f32, weather: bool, disasters: bool, audio: &mut Audio) {
        use crate::weather::{self, Disaster};
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
                let water = reg().wk.water;
                let near = [(-9, 0), (9, 0), (0, -9), (0, 9), (-6, -6), (6, 6), (-6, 6), (6, -6)]
                    .iter()
                    .filter(|d| self.world.get(px + d.0, crate::worldgen::SEA, pz + d.1) == water)
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
        // Disasters are a single-player affair: the server does not run them.
        if !disasters || self.net.is_some() {
            if self.disaster.is_some() {
                self.disaster = None;
                self.drain_flood(0);
            }
            return;
        }
        match self.disaster {
            None => {
                if tick && roll() % 3 == 0 {
                    // About once a second.
                    self.disaster = weather::maybe_start(seed, px, pz, self.sky, roll());
                    match self.disaster {
                        Some(Disaster::Earthquake { .. }) => self.say("EARTHQUAKE"),
                        Some(Disaster::Tornado { .. }) => self.say("TORNADO"),
                        _ => {}
                    }
                }
            }
            Some(Disaster::Earthquake { left }) => {
                self.shake = 0.35;
                if tick {
                    audio.play(Sound::Rumble, 1.0);
                }
                let left = left - dt;
                self.disaster = if left > 0.0 {
                    Some(Disaster::Earthquake { left })
                } else {
                    let next = weather::aftermath(seed, px, pz, Disaster::Earthquake { left: 0.0 });
                    if next.is_some() {
                        self.say("TSUNAMI - GET TO HIGH GROUND");
                    }
                    next
                };
            }
            Some(Disaster::Tornado { x, z, dx, dz, left }) => {
                let (x, z) = (x + dx * dt, z + dz * dt);
                // Anything close is dragged in and thrown upward.
                let (tx, tz) = (x - self.player.pos[0] as f32, z - self.player.pos[2] as f32);
                let d2 = tx * tx + tz * tz;
                if d2 < 14.0 * 14.0 && !self.player.flying && self.vehicles.riding.is_none() {
                    let d = sqrtf(d2).max(0.5);
                    let pull = (14.0 - d) * 2.2 * dt;
                    self.player.vel[0] += tx / d * pull;
                    self.player.vel[2] += tz / d * pull;
                    if d < 5.0 {
                        self.player.vel[1] = (self.player.vel[1] + 34.0 * dt).min(9.0);
                    }
                }
                // It tears up plants and leaves along its path.
                if tick {
                    audio.play(Sound::Rumble, (1.0 - sqrtf(d2) / 40.0).clamp(0.1, 1.0));
                    for _ in 0..40 {
                        let (bx, bz) = (floord(x as f64) + (roll() % 7) as i32 - 3, floord(z as f64) + (roll() % 7) as i32 - 3);
                        let top = worldgen::column(seed, bx, bz).height;
                        let by = top + 1 + (roll() % 10) as i32;
                        let b = self.world.get(bx, by, bz);
                        if b != AIR && matches!(reg().kind[b as usize], Kind::Cross | Kind::Cutout) {
                            self.world.set_block(bx, by, bz, AIR);
                        }
                    }
                }
                let left = left - dt;
                self.disaster = if left > 0.0 { Some(Disaster::Tornado { x, z, dx, dz, left }) } else { None };
            }
            Some(Disaster::Tsunami { left, .. }) => {
                let rise = weather::tsunami_height(left);
                let level = worldgen::SEA + rise as i32;
                if tick {
                    // The sea climbs over low ground near the player...
                    let water = reg().wk.water;
                    for _ in 0..160 {
                        let (x, z) = (px + (roll() % 49) as i32 - 24, pz + (roll() % 49) as i32 - 24);
                        let ground = worldgen::column(seed, x, z).height;
                        for y in (ground + 1).max(worldgen::SEA + 1)..=level {
                            if self.world.get(x, y, z) == AIR && self.world.set_block(x, y, z, water) {
                                self.flood.push([x, y, z]);
                            }
                        }
                    }
                    // ...and falls back as the wave drains.
                    self.drain_flood(level);
                }
                let left = left - dt;
                self.disaster = if left > 0.0 {
                    Some(Disaster::Tsunami { left, rise })
                } else {
                    self.drain_flood(0);
                    None
                };
            }
        }
    }

    /// Take back flood water standing above `level`.
    fn drain_flood(&mut self, level: i32) {
        let water = reg().wk.water;
        let mut kept = Vec::new();
        for c in core::mem::take(&mut self.flood) {
            if c[1] > level {
                if self.world.get(c[0], c[1], c[2]) == water {
                    self.world.set_block(c[0], c[1], c[2], AIR);
                }
            } else {
                kept.push(c);
            }
        }
        self.flood = kept;
    }

    /// In Earth worlds, say where the player is whenever that changes:
    /// crossing a border brings up the new country, region or town.
    fn place_update(&mut self, dt: f32) {
        for name in core::mem::take(&mut self.world.place_results) {
            self.place_asked = false;
            if let Some(name) = name {
                if name != self.last_place {
                    let verb = if self.last_place.is_empty() { "YOU ARE IN" } else { "ENTERING" };
                    self.note = (alloc::format!("{} {}", verb, name.to_ascii_uppercase()), 5.0);
                    self.push_chat(alloc::format!("{} {}", verb, name.to_ascii_uppercase()));
                    self.last_place = name;
                }
            }
        }
        let proj = match worldgen::earth_proj() {
            Some(p) if self.place_level > 0 => p,
            _ => return,
        };
        self.place_timer -= dt;
        if self.place_timer <= 0.0 && !self.place_asked {
            // One question every twelve seconds at most: the service is a
            // shared, free one.
            self.place_timer = 12.0;
            self.place_asked = true;
            let (lat, lon) = proj.block_to_latlon(self.player.pos[0], self.player.pos[2]);
            self.pool.submit(crate::jobs::Job::Place { lat, lon, level: self.place_level }, false);
        }
    }

    fn net_update(&mut self, dt: f32) {
        for c in self.chat.iter_mut() {
            c.1 -= dt;
        }
        let polled = match self.net.as_mut() {
            Some(n) => {
                n.send_move(dt, self.player.pos, self.player.yaw, self.player.pitch);
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
                crate::netplay::Event::Skin(id, png) => self.skin_events.push((id, png)),
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
    }

    /// Advance the world by `dt` seconds.
    pub fn update(&mut self, dt: f32, c: &Controls, distance: i32, far: i32, sink: &mut dyn MeshSink, audio: &mut Audio) {
        self.world.far_radius = if far > distance { far } else { 0 };
        self.clock += dt;
        self.note.1 -= dt;
        self.net_update(dt);
        self.place_update(dt);
        self.hurt_flash = (self.hurt_flash - dt).max(0.0);
        // A full day lasts twenty minutes. (No `%` on floats: that would
        // need the C math library.)
        self.time += dt / 1200.0;
        if self.time >= 1.0 {
            self.time -= 1.0;
        }
        if c.toggle_fly && self.vehicles.riding.is_none() {
            if self.mode == GameMode::Creative {
                self.player.flying = !self.player.flying;
            }
        }

        if let Some(id) = self.pending_dim.take() {
            self.enter_dimension(&id, sink);
        }
        self.block_updates(dt);
        self.fly_shots(dt);
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
        // In a vehicle the player goes where it goes; F gets out.
        if self.vehicles.riding.is_some() && (c.leave || self.dead) {
            self.vehicles.riding = None;
            self.player.pos[1] += 0.6;
            self.player.vel = [0.0; 3];
        }
        let seat = self.vehicles.update(dt, &self.world, &c.input, (self.player.yaw, self.player.pitch));
        // Seated: held in place on the chair until getting up (jump, or
        // the leave key), or until the chair is gone.
        if let Some(at) = self.sitting {
            let seat_height = block::def(self.world.get(at[0], at[1], at[2])).x.seat;
            if seat_height == 0 || c.input.jump || c.leave || self.dead || self.vehicles.riding.is_some() {
                self.sitting = None;
                self.player.pos[1] = at[1] as f64 + 1.05;
            }
        }
        if let Some(at) = self.sitting {
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
        let bitten = self.mobs.update(dt, &self.world, self.player.pos, self.time, hunted, held_item);
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
        let (fx, fy, fz) = (floord(self.player.pos[0]), floord(self.player.pos[1] + 0.1), floord(self.player.pos[2]));
        let touch = block::def(self.world.get(fx, fy, fz)).x.hurts.max(block::def(self.world.get(fx, fy - 1, fz)).x.hurts);
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
            vehicle: None,
            category: String::new(),
            x: Default::default(),
        };
        let held = self.held();
        let item: &ItemDef = held.map_or(&none, |h| h.1);

        // Vehicles: left click picks one up, right click gets in, and a
        // vehicle item in hand sets a new one down.
        let eye = self.player.eye();
        let dir = forward(self.player.yaw, self.player.pitch);
        let near_block = self.target.as_ref().map_or(f32::MAX, |h| h.dist);
        if self.vehicles.riding.is_none() {
            if let Some((i, t)) = self.vehicles.hit_test(eye, dir, 5.0).filter(|v| v.1 < near_block) {
                let _ = t;
                if c.place_held && self.cooldown[1] <= 0.0 && item.id == "chain" {
                    // A chain joins rail carts into a train: click the one
                    // in front, then the one to pull behind it.
                    self.cooldown[1] = 0.4;
                    let on_rails = |k: usize| reg().vehicles[self.vehicles.list[k].kind].medium == crate::block::Medium::Rail;
                    if !on_rails(i) {
                        self.say("CHAINS ARE FOR RAIL CARTS");
                    } else if self.vehicles.list[i].leader.is_some() {
                        self.vehicles.list[i].leader = None;
                        self.say("UNCHAINED");
                    } else {
                        match self.coupling.take().filter(|&f| f != i && f < self.vehicles.list.len()) {
                            Some(front) => {
                                self.vehicles.list[i].leader = Some(front);
                                if !creative {
                                    self.inv.take_one(self.selected);
                                }
                                audio.play_named("rail", 1.0);
                                self.say("CHAINED");
                            }
                            None => {
                                self.coupling = Some(i);
                                self.say("NOW CLICK THE CART TO PULL BEHIND IT");
                            }
                        }
                    }
                    return;
                }
                if c.place_held && self.cooldown[1] <= 0.0 {
                    self.cooldown[1] = 0.4;
                    if c.input.descend && reg().vehicles[self.vehicles.list[i].kind].storage {
                        self.opened_vehicle = Some(i);
                    } else {
                        self.vehicles.riding = Some(i);
                    }
                    return;
                }
                if c.break_held && self.cooldown[0] <= 0.0 {
                    self.cooldown[0] = 0.4;
                    if let Some(v) = self.vehicles.remove(i) {
                        let id = &reg().vehicles[v.kind].id;
                        if let Some(item) = reg().items.iter().position(|it| it.id == *id) {
                            if !creative || self.inv.count(item as u16) == 0 {
                                self.inv.add(item as u16, 1);
                            }
                        }
                        for s in v.storage.slots.iter().flatten() {
                            self.inv.add(s.item, s.count);
                        }
                        audio.play(Sound::Pop, 0.8);
                    }
                    return;
                }
            } else if let (Some(kind), true) = (item.vehicle, c.place_held && self.cooldown[1] <= 0.0) {
                // On the block aimed at, or on water if that comes first.
                let water = reg().wk.water;
                let mut spot = self.target.as_ref().map(|h| ([h.prev[0] as f64 + 0.5, h.prev[1] as f64 + 0.05, h.prev[2] as f64 + 0.5], h.dist));
                let mut t = 0.5f32;
                while t < 6.0 && spot.map_or(true, |s| t < s.1) {
                    let p = [eye[0] + (dir.x * t) as f64, eye[1] + (dir.y * t) as f64, eye[2] + (dir.z * t) as f64];
                    if self.world.get(floord(p[0]), floord(p[1]), floord(p[2])) == water {
                        spot = Some(([p[0], floord(p[1]) as f64 + 0.9, p[2]], t));
                        break;
                    }
                    t += 0.2;
                }
                if let Some((pos, _)) = spot {
                    self.cooldown[1] = 0.5;
                    self.vehicles.place(kind, pos, self.player.yaw);
                    audio.play(Sound::Place, 1.0);
                    if !creative {
                        self.inv.take_one(self.selected);
                    }
                    return;
                }
            }
        } else {
            // Riding: hands are busy steering. Sneak + right click opens the hold.
            if c.place_held && self.cooldown[1] <= 0.0 && c.input.descend {
                if let Some(i) = self.vehicles.riding.filter(|&i| reg().vehicles[self.vehicles.list[i].kind].storage) {
                    self.cooldown[1] = 0.4;
                    self.opened_vehicle = Some(i);
                }
            }
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
                    let killed = self.mobs.hurt(i, if creative { 1000 } else { item.x.damage.max(3) }, eye);
                    if let (true, false, Some((drop, n))) = (killed, creative, reg().entities[kind].drop) {
                        self.inv.add(drop as u16, n);
                    }
                    audio.play(if killed { Sound::Pop } else { Sound::Hurt }, 0.8);
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
                    return;
                }
                // Chairs and couches: sit down.
                if aimed.x.seat > 0 && self.vehicles.riding.is_none() {
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
        let p = &self.player;
        let mut chest_text = String::new();
        for (pos, inv) in &self.chests {
            if inv.slots.iter().any(|s| s.is_some()) {
                chest_text.push_str(&alloc::format!("{} {} {} {}\n", pos[0], pos[1], pos[2], inv.save()));
            }
        }
        sys::write_file(&self.chests_path, chest_text.as_bytes());
        sys::write_file(&self.vehicles_path, self.vehicles.save_text().as_bytes());
        if self.net.is_none() {
            let mut text = String::new();
            for (x, z) in &self.explored {
                text.push_str(&alloc::format!("{} {}\n", x, z));
            }
            sys::write_file(&self.explored_path, text.as_bytes());
            sys::write_file(&self.pets_path, self.mobs.save_pets().as_bytes());
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

/// What a new player starts with.
pub fn starting_inventory(mode: GameMode) -> Inventory {
    let mut inv = Inventory::new();
    if mode == GameMode::Creative {
        let items = &reg().items;
        let want = ["grass", "stone", "planks", "log", "glass", "lamp", "stone_stairs", "planks_slab", "builders_wand"];
        for (slot, id) in want.iter().enumerate() {
            if let Some(i) = items.iter().position(|it| it.id == *id) {
                inv.slots[slot] = Some(Stack { item: i as u16, count: items[i].stack });
            }
        }
    }
    // Survival starts with empty hands.
    inv
}
