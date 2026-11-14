//! Creatures: animals, villagers, night hunters and cave monsters. What
//! kinds exist, how they look and how they behave all comes from the data
//! files; this module moves them.

use crate::block::{self, reg, traits, Behavior, Block, EntityDef, Kind, AIR};
use crate::chunk::CH;
use crate::math::{atan2f, cosf, floord, sinf, sqrtf, Vec3, TAU};
use crate::path::{self, Cell};
use crate::player::{Input, Player};
use crate::world::World;
use crate::worldgen;
use alloc::string::String;
use alloc::vec::Vec;

pub struct Mob {
    /// Index into the registry's entities.
    pub kind: usize,
    pub body: Player,
    pub health: i32,
    /// Counts down after being hit (red flash).
    pub hurt: f32,
    /// Walk animation.
    pub phase: f32,
    pub stride: f32,
    /// Seconds alive, for idle movement (tails, ears, wings).
    pub clock: f32,
    /// Poison, burning and the like (see game/effects).
    pub fx: crate::game::effects::Effects,
    /// Which look it has: 0 is the entity's own, n is its n-th variant.
    pub variant: u8,
    /// Seconds left before a young one is grown (0 = adult).
    pub young: f32,
    /// Where it lives (villagers keep near it).
    pub home: Cell,
    /// Seconds it stays angry at the player after being hit.
    pub anger: f32,
    /// It belongs to the player: it follows, guards, and is kept.
    pub tamed: bool,
    /// Told to wait where it is.
    pub stay: bool,
    /// Where another player's game says it is (shared games).
    pub goal: Option<[f64; 3]>,
    give_timer: f32,
    job_timer: f32,
    /// Seconds a lit fuse has burnt (creatures that `explodes`; 0 = not lit).
    pub fuse: f32,
    /// How far along its building work a builder is.
    job_step: u32,
    path: Vec<Cell>,
    step: usize,
    /// Time until the next decision.
    think: f32,
    /// Standing-about time left.
    idle: f32,
    attack_cooldown: f32,
    /// For noticing when it is getting nowhere.
    stuck: f32,
    last_pos: [f64; 3],
    /// Stable number, used by multiplayer.
    pub id: u32,
}

/// How tough one animal is: its kind's health, give or take a quarter.
fn spawn_health(base: i32, roll: u32) -> i32 {
    ((base * (75 + (roll % 51) as i32) + 50) / 100).max(1)
}

impl Mob {
    /// How it should be drawn: model, colour and height.
    pub fn look(&self) -> (usize, [u8; 3], f32) {
        let d = &reg().entities[self.kind];
        let (model, tint, size) = match (self.variant as usize).checked_sub(1).and_then(|i| d.x.variants.get(i)) {
            Some(v) => (v.model, v.tint, v.size),
            None => (d.model, d.tint, 1.0),
        };
        (model, tint, d.height * size * self.growth() * self.swell())
    }

    /// How much bigger a lit fuse has made it (1 = not at all).
    pub fn swell(&self) -> f32 {
        match reg().entities[self.kind].x.explodes {
            Some((_, fuse_len, _)) => crate::entity_explode::swell(self.fuse, fuse_len),
            None => 1.0,
        }
    }

    /// The picture it is painted with (see `pack_entities`): its variant's or its kind's
    /// own `tex`, or empty for the model's own.
    pub fn tex(&self) -> &'static str {
        let d = &reg().entities[self.kind];
        match (self.variant as usize).checked_sub(1).and_then(|i| d.x.variants.get(i)) {
            Some(v) if !v.tex.is_empty() => v.tex.as_str(),
            _ => d.x.tex.as_str(),
        }
    }

    /// Size relative to an adult: young ones grow steadily.
    pub fn growth(&self) -> f32 {
        match reg().entities[self.kind].x.baby {
            Some((scale, seconds)) if self.young > 0.0 => scale + (1.0 - scale) * (1.0 - self.young / seconds).clamp(0.0, 1.0),
            _ => 1.0,
        }
    }

    /// The name to show: "Calf"-style names are left to the data; young
    /// ones are simply marked.
    pub fn title(&self) -> String {
        let d = &reg().entities[self.kind];
        let base = match (self.variant as usize).checked_sub(1).and_then(|i| d.x.variants.get(i)) {
            Some(v) if !v.name.is_empty() => alloc::format!("{} {}", v.name, d.name),
            _ => d.name.clone(),
        };
        if self.young > 0.0 {
            alloc::format!("Young {}", base)
        } else {
            base
        }
    }
}

/// What a creature asks of the world or the player.
pub enum Action {
    /// A villager's work: put this block there.
    Set(Cell, Block),
    /// A creature hurt another (hunters and guards): its id and the damage.
    Bite(u32, i32, [f64; 3]),
    /// A creature shot at the player: from where, which way, how hard.
    Shoot([f64; 3], [f32; 3], i32, usize),
    /// A fuse was lit here: time for the hiss.
    Fuse([f64; 3]),
    /// A creature blew up: where, how strongly, and its kind.
    Explode([f64; 3], f32, usize),
}

/// What right-clicking a creature comes to.
pub enum Interaction {
    Nothing,
    /// It hands this over (milk, wool, an egg).
    Give(usize, u16),
    /// It will swap: take `(item, count)` from the player and give the other.
    Trade((usize, u16), (usize, u16)),
    /// Something to tell the player.
    Say(String),
    /// It took the food and is now the player's: the item to use up.
    Tame(usize),
    /// Climb on: the creature's id.
    Ride(u32),
}

pub struct Mobs {
    pub list: Vec<Mob>,
    /// Work and bites decided this step, for the session to carry out.
    pub actions: Vec<Action>,
    /// The creature a player is riding and how they steer it: its id,
    /// their controls, and where they look (yaw).
    pub rider: Option<(u32, Input, f32)>,
    /// The kind of creature whose bite is being carried out this step, for
    /// mod code that wants to know who hurt the player (see `Native::hurt`).
    /// The session clears it before each step.
    pub last_biter: Option<usize>,
    /// Creatures that appeared since the session last looked: id, kind and
    /// where (for the `entityspawn` event). Kept short.
    pub born: Vec<(u32, usize, [f64; 3])>,
    /// Creatures that died the same way (health ran out).
    pub died: Vec<(u32, usize, [f64; 3])>,
    spawn_timer: f32,
    rng: u32,
    next_id: u32,
}

/// Most creatures alive at once near a player.
pub const CAP: usize = 22;

fn is_night(time: f32) -> bool {
    sinf(time * TAU) < -0.05
}

/// The first stage of the crop `b` belongs to (what to replant).
fn crop_start(b: Block) -> Block {
    let r = reg();
    let mut start = block::def(b).first;
    for _ in 0..8 {
        match r.blocks.iter().find(|d| d.x.grows.map(|g| g.0) == Some(start)) {
            Some(p) => start = p.first,
            None => break,
        }
    }
    start
}

/// The smallest size, in blocks, of what a swing or a shot can hit.
const MIN_TARGET: f64 = 0.6;

impl Mobs {
    pub fn new(seed: u32) -> Mobs {
        Mobs { list: Vec::new(), actions: Vec::new(), rider: None, last_biter: None, born: Vec::new(), died: Vec::new(), spawn_timer: 2.0, rng: seed | 1, next_id: 1 }
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    /// A random whole number in `lo..=hi`.
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + (self.rand() % (hi - lo + 1) as u32) as i32
    }

    pub fn spawn(&mut self, kind: usize, pos: [f64; 3]) -> usize {
        self.spawn_in(kind, pos, None)
    }

    /// As [`Mobs::spawn`], for a creature appearing in `biome`: it is given a look
    /// made for that biome if there is one (see `entity_variant`).
    pub fn spawn_in(&mut self, kind: usize, pos: [f64; 3], biome: Option<&str>) -> usize {
        let d = &reg().entities[kind];
        // One of its looks, the plain one included.
        let roll = self.rand();
        let variant = match biome {
            Some(b) => crate::entity_variant::pick(&d.x.variants, b, roll),
            None => crate::entity_variant::any(&d.x.variants, roll),
        };
        let size = (variant as usize).checked_sub(1).and_then(|i| d.x.variants.get(i)).map_or(1.0, |v| v.size);
        let mut body = Player::with_size(pos, (d.width * size) as f64, (d.height * size) as f64);
        body.speed_scale = d.speed / 4.4;
        body.yaw = (self.rand() % 628) as f32 / 100.0 - 3.14;
        let id = self.next_id;
        self.next_id += 1;
        let think = (self.rand() % 100) as f32 / 100.0;
        let give_timer = (self.rand() % 60) as f32;
        let health = spawn_health(d.health, self.rand());
        if self.born.len() < 64 {
            self.born.push((id, kind, pos));
        }
        self.list.push(Mob {
            kind,
            body,
            health,
            hurt: 0.0,
            phase: 0.0,
            stride: 0.0,
            clock: think * 10.0,
            fx: Default::default(),
            variant,
            young: 0.0,
            home: [floord(pos[0]), floord(pos[1]), floord(pos[2])],
            anger: 0.0,
            tamed: false,
            stay: false,
            goal: None,
            give_timer,
            job_timer: 3.0,
            fuse: 0.0,
            job_step: 0,
            path: Vec::new(),
            step: 0,
            think,
            idle: 1.0,
            attack_cooldown: 0.0,
            stuck: 0.0,
            last_pos: pos,
            id,
        });
        self.list.len() - 1
    }

    /// A young one of `kind`, if that kind has young. It is small, weak
    /// and grows up as time passes.
    pub fn spawn_young(&mut self, kind: usize, pos: [f64; 3]) -> Option<usize> {
        let (scale, seconds) = reg().entities[kind].x.baby?;
        let i = self.spawn(kind, pos);
        let d = &reg().entities[kind];
        let m = &mut self.list[i];
        m.young = seconds;
        m.health = (d.health / 2).max(1);
        let yaw = m.body.yaw;
        m.body = Player::with_size(pos, (d.width * scale) as f64, (d.height * scale) as f64);
        m.body.yaw = yaw;
        m.body.speed_scale = d.speed / 4.4;
        Some(i)
    }

    /// Try to add one creature somewhere around `center`, out of arm's reach.
    fn try_spawn(&mut self, world: &World, center: [f64; 3], time: f32, hostile_ok: bool) {
        let r = reg();
        if r.entities.is_empty() {
            return;
        }
        let angle = (self.rand() % 6283) as f32 / 1000.0;
        let solid = |x: i32, y: i32, z: i32| block::is_solid(world.get(x, y, z));
        // A player well below the surface meets cave dwellers instead.
        let here = worldgen::column(world.seed, floord(center[0]), floord(center[2]));
        let underground = (center[1] as i32) < here.height - 7 && self.rand() % 4 != 0;
        let dist = if underground { self.range(9, 22) } else { self.range(18, 44) } as f32;
        let (x, z) = (floord(center[0]) + (sinf(angle) * dist) as i32, floord(center[2]) + (cosf(angle) * dist) as i32);
        if !world.is_loaded(x, z) {
            return;
        }
        let col = worldgen::column(world.seed, x, z);
        let mut watery = false;
        let (y, biome) = if underground {
            let near = floord(center[1]) + self.range(-6, 6);
            match path::ground_near(&solid, x, near, z, 5) {
                // Really under rock, and standing on a floor.
                Some(c) if c[1] < col.height - 5 && (c[0], c[2]) == (x, z) => (c[1], String::from(worldgen::cave_biome(world.seed, x, c[1], z, col.biome).name())),
                _ => return,
            }
        } else {
            // Open water gets water creatures; land gets the rest.
            watery = world.get(x, worldgen::SEA, z) == r.wk.water && world.get(x, worldgen::SEA - 2, z) == r.wk.water;
            // The highest place to stand that is real ground, not a tree top.
            let mut feet = None;
            if watery {
                feet = Some(worldgen::SEA - 2);
            } else {
                for y in (2..CH - 2).rev() {
                    if path::standable(&solid, [x, y, z]) {
                        let below = world.get(x, y - 1, z);
                        if r.kind[below as usize] == Kind::Cube {
                            feet = Some(y);
                        }
                        break;
                    }
                }
            }
            match feet {
                Some(y) => (y, alloc::format!("{:?}", col.biome).to_ascii_lowercase()),
                None => return,
            }
        };
        let night = is_night(time);
        let fits = |d: &EntityDef| {
            let threat = d.behavior == Behavior::Hostile;
            d.weight > 0
                && d.x.cave == underground
                && (d.behavior == Behavior::Swim) == watery
                && (!d.night || night)
                && (!threat || hostile_ok)
                && (d.biomes.is_empty() || d.biomes.iter().any(|b| *b == biome || (underground && b == "cave")))
        };
        let total: u32 = r.entities.iter().filter(|d| fits(d)).map(|d| d.weight).sum();
        if total == 0 {
            return;
        }
        let mut roll = self.rand() % total;
        for (i, d) in r.entities.iter().enumerate() {
            if !fits(d) {
                continue;
            }
            if roll < d.weight {
                let lift = if d.behavior == Behavior::Fly { 2.0 } else { 0.01 };
                let at = [x as f64 + 0.5, y as f64 + lift, z as f64 + 0.5];
                self.spawn_in(i, at, Some(&biome));
                // Herds come in twos and threes, sometimes with young.
                if d.x.traits & traits::HERD != 0 {
                    for k in 0..self.range(1, 2) {
                        self.spawn_in(i, [at[0] + 1.2 * (k + 1) as f64, at[1], at[2] + 0.8], Some(&biome));
                    }
                }
                if d.x.baby.is_some() && self.rand() % 100 < 40 {
                    self.spawn_young(i, [at[0] - 0.9, at[1], at[2] - 0.9]);
                }
                return;
            }
            roll -= d.weight;
        }
    }

    /// What right-clicking creature `i` does, given the item in hand and
    /// how many of it the player has.
    pub fn interact(&mut self, i: usize, held: Option<usize>, have: &dyn Fn(usize) -> u32) -> Interaction {
        let r = reg();
        let m = match self.list.get_mut(i) {
            Some(m) => m,
            None => return Interaction::Nothing,
        };
        let d = &r.entities[m.kind];
        // Winning it over with its favourite food; then a pat tells it to
        // wait or to come along.
        if d.x.tamable && m.young <= 0.0 {
            if !m.tamed && held.is_some() && held == d.x.likes {
                m.tamed = true;
                m.anger = 0.0;
                return Interaction::Tame(held.unwrap_or(0));
            }
            if m.tamed && d.x.rideable && held.is_none() {
                return Interaction::Ride(m.id);
            }
            if m.tamed && held != d.x.likes {
                m.stay = !m.stay;
                return Interaction::Say(alloc::format!("{} WILL {}", m.title(), if m.stay { "WAIT HERE" } else { "FOLLOW YOU" }).to_ascii_uppercase());
            }
        }
        if d.x.rideable && !d.x.tamable && m.young <= 0.0 {
            return Interaction::Ride(m.id);
        }
        if let Some((want, n, give, count)) = d.x.trades.iter().copied().find(|t| Some(t.0) == held && have(t.0) >= t.1 as u32) {
            return Interaction::Trade((want, n), (give, count));
        }
        if let (Some((item, seconds)), true) = (d.x.gives, m.young <= 0.0) {
            if m.give_timer <= 0.0 {
                m.give_timer = seconds;
                return Interaction::Give(item, 1);
            }
        }
        if !d.x.trades.is_empty() {
            // Tell of one swap at a time; ask again for the next.
            m.job_step = m.job_step.wrapping_add(1);
            let (want, n, give, count) = d.x.trades[m.job_step as usize % d.x.trades.len()];
            return Interaction::Say(alloc::format!("{}: {} {} FOR {} {}", m.title(), n, r.items[want].name, count, r.items[give].name).to_ascii_uppercase());
        }
        if matches!(d.behavior, Behavior::Villager | Behavior::Npc) {
            let job = if d.x.job.is_empty() { String::new() } else { alloc::format!(" THE {}", d.x.job) };
            return Interaction::Say(alloc::format!("{}{}", m.title(), job).to_ascii_uppercase());
        }
        if d.x.gives.is_some() {
            return Interaction::Say(String::from("NOTHING YET - TRY AGAIN LATER"));
        }
        Interaction::Nothing
    }

    /// A villager's next piece of work near its home, if there is any.
    fn work(&mut self, world: &World, i: usize) -> Option<Action> {
        let r = reg();
        let (home, job, step) = {
            let m = &self.list[i];
            (m.home, r.entities[m.kind].x.job.clone(), m.job_step)
        };
        let farmland = r.wk.farmland;
        match job.as_str() {
            "farmer" if farmland != AIR => {
                // A small plot beside the house: harvest what is ripe,
                // then till and sow one more square.
                let roll = self.rand();
                let mut tilled = 0;
                let mut sow = None;
                for dx in -4..=4i32 {
                    for dz in -4..=4i32 {
                        for dy in -2..=2 {
                            let c = [home[0] + dx, home[1] + dy, home[2] + dz];
                            let b = world.get(c[0], c[1], c[2]);
                            let d = block::def(b);
                            // Ripe: the end of a growth chain (not some other
                            // thing a pack lets stand on farmland).
                            if d.x.on.contains(&farmland) && d.x.grows.is_none() && b != AIR && crop_start(b) != d.first {
                                return Some(Action::Set(c, crop_start(b)));
                            }
                            if d.first == farmland {
                                tilled += 1;
                                if world.get(c[0], c[1] + 1, c[2]) == AIR && sow.is_none() {
                                    sow = Some([c[0], c[1] + 1, c[2]]);
                                }
                            }
                        }
                    }
                }
                if let Some(c) = sow {
                    // Any crop's first stage: grows, stands on farmland, and nothing grows into it.
                    let starts: Vec<Block> = r.blocks.iter().filter(|d| d.x.on.contains(&farmland) && d.x.grows.is_some()).map(|d| crop_start(d.first)).collect();
                    if !starts.is_empty() {
                        return Some(Action::Set(c, starts[roll as usize % starts.len()]));
                    }
                }
                if tilled < 12 {
                    let (dx, dz) = ((roll % 7) as i32 - 3, ((roll >> 8) % 7) as i32 - 3);
                    for dy in (-2..=2).rev() {
                        let c = [home[0] + dx + 3, home[1] + dy, home[2] + dz + 3];
                        let b = world.get(c[0], c[1], c[2]);
                        if (b == r.wk.grass || b == r.wk.dirt) && world.get(c[0], c[1] + 1, c[2]) == AIR {
                            return Some(Action::Set(c, farmland));
                        }
                    }
                }
                None
            }
            "builder" => {
                // A hut, one block at a time: floor, walls with a doorway
                // and windows, then a roof.
                let (planks, glass) = (r.block("planks").unwrap_or(r.wk.stone), r.block("glass").unwrap_or(AIR));
                const W: i32 = 5;
                const H: i32 = 4;
                let total = (W * W * (H + 1)) as u32;
                let mut k = step;
                while k < total {
                    let (layer, rest) = ((k as i32) / (W * W), (k as i32) % (W * W));
                    let (ax, az) = (rest / W, rest % W);
                    let edge = ax == 0 || az == 0 || ax == W - 1 || az == W - 1;
                    let c = [home[0] - 8 + ax, home[1] - 1 + layer, home[2] - 8 + az];
                    let want = if layer == 0 || layer == H {
                        planks
                    } else if !edge || (az == 0 && ax == W / 2 && layer <= 2) {
                        AIR // inside, and the doorway
                    } else if layer == 2 && (ax == W / 2 || az == W / 2) {
                        glass
                    } else {
                        planks
                    };
                    // Not while that corner of the world is away: wait for it.
                    if !world.is_loaded(c[0], c[2]) {
                        return None;
                    }
                    k += 1;
                    if want != AIR && crate::interact::replaceable(world.get(c[0], c[1], c[2])) {
                        self.list[i].job_step = k;
                        return Some(Action::Set(c, want));
                    }
                }
                self.list[i].job_step = total;
                None
            }
            _ => None,
        }
    }

    /// Move every creature. `target` is where the player stands and `held`
    /// the item in their hand. Returns the damage dealt to the player.
    pub fn update(&mut self, dt: f32, world: &World, target: [f64; 3], time: f32, hostile_ok: bool, held: Option<usize>) -> i32 {
        let r = reg();
        self.spawn_timer -= dt;
        if self.spawn_timer <= 0.0 {
            self.spawn_timer = 1.5;
            if self.list.len() < CAP {
                self.try_spawn(world, target, time, hostile_ok);
            }
        }
        let day = !is_night(time);
        let solid = |x: i32, y: i32, z: i32| block::is_solid(world.get(x, y, z));
        // Who is where, so creatures can find their own kind and their prey.
        let others: Vec<(u32, usize, [f64; 3])> = self.list.iter().map(|m| (m.id, m.kind, m.body.pos)).collect();
        let mut damage = 0;
        let mut i = 0;
        while i < self.list.len() {
            let roll = self.rand();
            let m = &mut self.list[i];
            let d = &r.entities[m.kind];
            let t = d.x.traits;
            let (dx, dy, dz) = ((target[0] - m.body.pos[0]) as f32, (target[1] - m.body.pos[1]) as f32, (target[2] - m.body.pos[2]) as f32);
            let dist = sqrtf(dx * dx + dz * dz);
            // Gone: dead, far away, fallen out of the world, or a night
            // creature caught by the morning out of the player's sight.
            let burned = d.night && day && !d.x.cave && (dist > 20.0 || roll % 600 == 0);
            if m.tamed && m.health > 0 {
                // A pet is never lost: left far behind, it catches up.
                if (dist > 40.0 || m.body.pos[1] < -8.0) && !m.stay {
                    m.body.pos = [target[0] + 1.0, target[1] + 0.2, target[2] + 1.0];
                    m.body.vel = [0.0; 3];
                    m.path.clear();
                }
            } else if m.health <= 0 || dist > 96.0 || m.body.pos[1] < -8.0 || burned || (d.behavior == Behavior::Hostile && !hostile_ok) {
                if m.health <= 0 && self.died.len() < 64 {
                    self.died.push((m.id, m.kind, m.body.pos));
                }
                self.list.swap_remove(i);
                continue;
            }
            m.hurt = (m.hurt - dt).max(0.0);
            m.attack_cooldown -= dt;
            m.give_timer -= dt;
            m.anger = (m.anger - dt).max(0.0);
            m.clock += dt;
            if !m.fx.is_empty() || m.fx.changed {
                let delta = crate::game::effects::apply::tick_mob(&mut m.fx, dt, m.body.in_water, d.speed / 4.4, &mut m.body.speed_scale);
                if delta < 0 {
                    m.health += delta;
                    m.hurt = 0.35;
                } else if delta > 0 {
                    m.health = (m.health + delta).min(d.health.max(m.health));
                }
            }
            if m.young > 0.0 {
                m.young -= dt;
                if m.young <= 0.0 {
                    // Grown: full size and strength from now on.
                    m.young = 0.0;
                    let (pos, yaw) = (m.body.pos, m.body.yaw);
                    m.body = Player::with_size(pos, d.width as f64, d.height as f64);
                    m.body.yaw = yaw;
                    m.body.speed_scale = d.speed / 4.4;
                    m.health = spawn_health(d.health, roll);
                }
            }
            let (px, pz) = (floord(m.body.pos[0]), floord(m.body.pos[2]));
            if !world.is_loaded(px, pz) {
                // Frozen until its chunk comes back; but one left far outside what is
                // loaded is just removed (it would stand in the air for ever).
                if dist > 48.0 && !m.tamed {
                    self.list.swap_remove(i);
                } else {
                    i += 1;
                }
                continue;
            }
            // Ridden: it goes where its rider steers, and thinks of nothing else.
            if let Some((_, steer, yaw)) = self.rider.as_ref().filter(|r| r.0 == m.id) {
                let flyer = d.behavior == Behavior::Fly;
                m.body.yaw = *yaw;
                m.body.flying = flyer;
                let input = Input {
                    forward: steer.forward,
                    strafe: steer.strafe,
                    jump: steer.jump && (flyer || m.body.on_ground || m.body.in_water),
                    descend: flyer && steer.descend,
                    sprint: steer.sprint,
                };
                let before = m.body.pos;
                m.body.update(dt, &input, &|x, y, z| world.get_for_physics(x, y, z));
                let (mx, mz) = ((m.body.pos[0] - before[0]) as f32, (m.body.pos[2] - before[2]) as f32);
                let speed = if dt > 0.0 { sqrtf(mx * mx + mz * mz) / dt } else { 0.0 };
                m.stride += (if flyer { 1.0 } else { (speed / d.speed.max(0.1)).min(1.0) } - m.stride) * (dt * 8.0).min(1.0);
                m.phase += if flyer { dt * 12.0 } else { speed * dt * 3.2 };
                if m.phase > TAU {
                    m.phase -= TAU;
                }
                m.path.clear();
                i += 1;
                continue;
            }

            if d.behavior == Behavior::Swim {
                // Water creatures need no paths: they cruise, turn away from
                // anything that is not water, and stay under the surface.
                let water = r.wk.water;
                let wet = |dx: f32, dy: f32, dz: f32| {
                    world.get(floord(m.body.pos[0] + dx as f64), floord(m.body.pos[1] + 0.2 + dy as f64), floord(m.body.pos[2] + dz as f64)) == water
                };
                m.body.flying = wet(0.0, 0.0, 0.0);
                m.think -= dt;
                let (fx, fz) = (-sinf(m.body.yaw), -cosf(m.body.yaw));
                if m.think <= 0.0 || !wet(fx * 1.3, 0.0, fz * 1.3) {
                    m.think = 1.5 + (roll % 300) as f32 / 100.0;
                    m.body.yaw += 1.2 + (roll % 200) as f32 / 100.0;
                    if m.body.yaw > crate::math::PI {
                        m.body.yaw -= TAU;
                    }
                    m.idle = ((roll >> 9) % 3) as f32 - 1.0; // drift up, level or down
                }
                let input = Input {
                    forward: if m.body.flying { 1.0 } else { 0.0 },
                    strafe: 0.0,
                    jump: m.body.flying && wet(0.0, 1.0, 0.0) && (m.idle > 0.5 || !wet(0.0, -1.0, 0.0)),
                    descend: m.body.flying && (!wet(0.0, 1.0, 0.0) || (m.idle < -0.5 && wet(0.0, -1.0, 0.0))),
                    sprint: false,
                };
                m.body.update(dt, &input, &|x, y, z| world.get_for_physics(x, y, z));
                m.stride = 0.6;
                m.phase += dt * 6.0;
                if m.phase > TAU {
                    m.phase -= TAU;
                }
                i += 1;
                continue;
            }

            if d.behavior == Behavior::Fly {
                // Flyers hold a height above whatever is under them, turn
                // now and then, and swerve up and away from anything ahead.
                let open = |dx: f32, dy: f32, dz: f32| {
                    !block::is_solid(world.get(floord(m.body.pos[0] + dx as f64), floord(m.body.pos[1] + 0.2 + dy as f64), floord(m.body.pos[2] + dz as f64)))
                };
                m.body.flying = true;
                m.think -= dt;
                let (fx, fz) = (-sinf(m.body.yaw), -cosf(m.body.yaw));
                let blocked = !open(fx * 1.2, 0.0, fz * 1.2);
                if m.think <= 0.0 || blocked {
                    m.think = 0.8 + (roll % 250) as f32 / 100.0;
                    m.body.yaw += if blocked { 1.6 } else { (roll % 240) as f32 / 100.0 - 1.2 };
                    if m.body.yaw > crate::math::PI {
                        m.body.yaw -= TAU;
                    }
                    m.idle = ((roll >> 9) % 3) as f32 - 1.0;
                }
                // Flyers that shoot (dragons) circle the player and loose
                // at them from above.
                if d.x.shoots > 0 && hostile_ok && !m.tamed && dist < 40.0 {
                    if dist > 14.0 && roll % 30 == 0 {
                        m.body.yaw = atan2f(-dx, -dz);
                    }
                    if dist < 26.0 && m.attack_cooldown <= 0.0 {
                        m.attack_cooldown = 2.6;
                        let from = [m.body.pos[0], m.body.pos[1] + d.height as f64 * 0.5, m.body.pos[2]];
                        let aim = [dx, dy + 1.0 - d.height * 0.5, dz];
                        let len = sqrtf(aim[0] * aim[0] + aim[1] * aim[1] + aim[2] * aim[2]).max(0.01);
                        self.actions.push(Action::Shoot(from, [aim[0] / len, aim[1] / len, aim[2] / len], d.x.shoots, m.kind));
                    }
                }
                // Frightened flyers scatter upward.
                let scared = t & traits::FLEES != 0 && dist < 4.0;
                let low = !open(0.0, -1.0, 0.0) || !open(0.0, -2.0, 0.0);
                let high = open(0.0, -3.0, 0.0) && open(0.0, -4.0, 0.0) && open(0.0, -5.0, 0.0);
                let roofed = !open(0.0, 1.2, 0.0);
                let input = Input {
                    forward: 1.0,
                    strafe: 0.0,
                    jump: !roofed && (low || blocked || scared || (m.idle > 0.5 && !high)),
                    descend: roofed || (!low && (high || m.idle < -0.5)),
                    sprint: false,
                };
                m.body.update(dt, &input, &|x, y, z| world.get_for_physics(x, y, z));
                m.stride = 1.0;
                m.phase += dt * 14.0;
                if m.phase > TAU {
                    m.phase -= TAU;
                }
                i += 1;
                continue;
            }

            // What it is up to at the moment.
            let near_kin = |pos: [f64; 3], want: &dyn Fn(usize) -> bool, reach: f64| -> Option<(u32, [f64; 3])> {
                let mut best: Option<(f64, u32, [f64; 3])> = None;
                for &(id, kind, p) in &others {
                    let d2 = (p[0] - pos[0]) * (p[0] - pos[0]) + (p[2] - pos[2]) * (p[2] - pos[2]);
                    if want(kind) && d2 < reach * reach && d2 > 0.01 && best.map_or(true, |b| d2 < b.0) {
                        best = Some((d2, id, p));
                    }
                }
                best.map(|b| (b.1, b.2))
            };
            let angry = (t & (traits::TERRITORIAL | traits::PACK) != 0 && m.anger > 0.0) || (t & traits::TERRITORIAL != 0 && dist < 4.5 && m.young <= 0.0);
            let attacks = hostile_ok && !m.tamed && (d.behavior == Behavior::Hostile || angry);
            let flees = !attacks && !m.tamed && t & traits::FLEES != 0 && dist < 7.0;
            let guard = d.behavior == Behavior::Villager && d.x.job == "guard";
            let quarry = if m.tamed && !m.stay && d.damage > 0 {
                // Pets with teeth see off whatever threatens their owner.
                near_kin(m.body.pos, &|k| r.entities[k].behavior == Behavior::Hostile, 12.0)
            } else if m.tamed {
                None
            } else if t & traits::HUNTER != 0 && !attacks && m.young <= 0.0 {
                // Its own named quarry, or failing a list, anything that is prey.
                if d.x.hunts.is_empty() {
                    near_kin(m.body.pos, &|k| r.entities[k].x.traits & traits::PREY != 0, 16.0)
                } else {
                    near_kin(m.body.pos, &|k| d.x.hunts.iter().any(|h| *h == r.entities[k].id), 18.0)
                }
            } else if guard {
                near_kin(m.body.pos, &|k| r.entities[k].behavior == Behavior::Hostile, 18.0)
            } else {
                None
            };
            let tempted = (t & traits::FOLLOWS != 0 && held.is_some() && held == d.x.likes && dist < 10.0) || (m.tamed && !m.stay && dist > 3.0);
            if m.tamed && m.stay {
                // Waiting: no wandering off.
                m.path.clear();
                m.idle = 2.0;
            }
            let at_home = d.behavior == Behavior::Villager;
            let from_home = sqrtf(((m.home[0] - px) * (m.home[0] - px) + (m.home[2] - pz) * (m.home[2] - pz)) as f32);

            // Decide where to go.
            m.think -= dt;
            let mut hurry = attacks || flees || quarry.is_some();
            if m.think <= 0.0 {
                m.think = 0.6 + (roll % 40) as f32 / 100.0;
                let feet = [px, floord(m.body.pos[1] + 0.3), pz];
                let goal = if attacks && dist < 30.0 {
                    m.idle = 0.0;
                    Some([floord(target[0]), floord(target[1] + 0.3), floord(target[2])])
                } else if flees {
                    // Straight away from the player, as far as a path goes.
                    m.idle = 0.0;
                    let k = 9.0 / dist.max(0.5);
                    path::ground_near(&solid, feet[0] - (dx * k) as i32, feet[1], feet[2] - (dz * k) as i32, 4)
                } else if let Some((_, p)) = quarry {
                    m.idle = 0.0;
                    Some([floord(p[0]), floord(p[1] + 0.3), floord(p[2])])
                } else if (d.behavior == Behavior::Npc && dist < 12.0 && dist > 3.5) || (tempted && dist > 2.5) {
                    m.idle = 0.0;
                    Some([floord(target[0]), floord(target[1] + 0.3), floord(target[2])])
                } else if at_home && (from_home > 12.0 || (!day && from_home > 3.0)) {
                    // Villagers head home when they stray, and at dusk.
                    path::ground_near(&solid, m.home[0], m.home[1], m.home[2], 3)
                } else if m.step >= m.path.len() {
                    m.idle -= 1.0;
                    if m.idle <= 0.0 {
                        let lazy = if t & traits::LAZY != 0 { 4.0 } else { 1.0 };
                        m.idle = (2.0 + (roll % 5) as f32) * lazy;
                        let (ox, oz) = (((roll >> 8) % 17) as i32 - 8, ((roll >> 16) % 17) as i32 - 8);
                        // Herd animals drift back toward the others.
                        let kind = m.kind;
                        let anchor = if t & traits::HERD != 0 { near_kin(m.body.pos, &|k| k == kind, 40.0).map(|k| k.1) } else { None };
                        let (cx, cz) = match anchor {
                            Some(p) if roll % 3 != 0 => (floord(p[0]) + ox / 3, floord(p[2]) + oz / 3),
                            _ if at_home => (m.home[0] + ox, m.home[2] + oz),
                            _ => (feet[0] + ox, feet[2] + oz),
                        };
                        path::ground_near(&solid, cx, feet[1], cz, 4)
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(goal) = goal {
                    let start = if path::standable(&solid, feet) { Some(feet) } else { path::ground_near(&solid, feet[0], feet[1], feet[2], 2) };
                    m.path = start.and_then(|s| path::find_path(&solid, s, goal, 320)).unwrap_or_default();
                    m.step = 0;
                }
            }
            hurry &= !m.path.is_empty();

            // Follow the path.
            let mut input = Input::default();
            input.sprint = hurry;
            while m.step < m.path.len() {
                let wp = m.path[m.step];
                let (wx, wz) = ((wp[0] as f64 + 0.5 - m.body.pos[0]) as f32, (wp[2] as f64 + 0.5 - m.body.pos[2]) as f32);
                if wx * wx + wz * wz < 0.12 && m.body.pos[1] > wp[1] as f64 - 0.3 {
                    m.step += 1;
                    continue;
                }
                // Yaw 0 faces -z and turns toward -x as it grows.
                m.body.yaw = atan2f(-wx, -wz);
                input.forward = 1.0;
                input.jump = (wp[1] as f64 > m.body.pos[1] + 0.3 || m.body.in_water) && (m.body.on_ground || m.body.in_water);
                break;
            }
            if attacks && dist < 6.0 && m.step >= m.path.len() {
                // The path has run out: close in directly for the last steps.
                m.body.yaw = atan2f(-dx, -dz);
                if dist > 0.9 {
                    input.forward = 1.0;
                }
            }
            // Hoppers leave the ground with every step.
            if t & traits::JUMPS != 0 && input.forward > 0.0 && m.body.on_ground {
                input.jump = true;
            }
            // A lit fuse holds it still: it hisses, swells and waits.
            if m.fuse > 0.0 {
                input.forward = 0.0;
                input.strafe = 0.0;
                input.jump = false;
                input.sprint = false;
            }
            // Getting nowhere? Hop, and think again soon.
            if input.forward > 0.0 {
                m.stuck += dt;
                if m.stuck > 1.0 {
                    let (mx, mz) = ((m.body.pos[0] - m.last_pos[0]) as f32, (m.body.pos[2] - m.last_pos[2]) as f32);
                    if mx * mx + mz * mz < 0.04 {
                        input.jump = m.body.on_ground;
                        m.think = m.think.min(0.2);
                    }
                    m.stuck = 0.0;
                    m.last_pos = m.body.pos;
                }
            } else {
                m.stuck = 0.0;
                m.last_pos = m.body.pos;
            }
            let before = m.body.pos;
            m.body.update(dt, &input, &|x, y, z| world.get_for_physics(x, y, z));
            let (mx, mz) = ((m.body.pos[0] - before[0]) as f32, (m.body.pos[2] - before[2]) as f32);
            let speed = if dt > 0.0 { sqrtf(mx * mx + mz * mz) / dt } else { 0.0 };
            m.stride += ((speed / d.speed.max(0.1)).min(1.0) - m.stride) * (dt * 8.0).min(1.0);
            m.phase += speed * dt * 3.2;
            if m.phase > TAU {
                m.phase -= TAU;
            }

            if attacks && dist < 1.3 && dy > -2.0 && dy < 1.6 && m.attack_cooldown <= 0.0 {
                m.attack_cooldown = 1.0;
                damage += d.damage;
                self.last_biter = Some(m.kind);
            }
            // Creatures that explode: light the fuse near a player, put it out
            // when they get away, and blow up when it has burnt down.
            if let Some((power, fuse_len, range)) = d.x.explodes {
                let level = dy > -3.0 && dy < 3.0;
                let armed = hostile_ok && !m.tamed && m.young <= 0.0 && m.health > 0;
                let (fuse, event) = crate::entity_explode::step(m.fuse, dt, if level { dist } else { 1e9 }, range, fuse_len, armed);
                m.fuse = fuse;
                // Flashing is the red flash of a creature that was hit.
                if crate::entity_explode::flashing(fuse, fuse_len) {
                    m.hurt = m.hurt.max(0.06);
                }
                let centre = [m.body.pos[0], m.body.pos[1] + d.height as f64 * 0.5, m.body.pos[2]];
                match event {
                    crate::entity_explode::Event::Lit => self.actions.push(Action::Fuse(centre)),
                    crate::entity_explode::Event::Boom => {
                        self.actions.push(Action::Explode(centre, power, m.kind));
                        self.list.swap_remove(i);
                        continue;
                    }
                    crate::entity_explode::Event::Nothing => {}
                }
            }
            let m = &mut self.list[i];
            // Archers and spell-throwers loose at the player from a distance.
            if attacks && d.x.shoots > 0 && dist > 3.0 && dist < 20.0 && m.attack_cooldown <= 0.0 {
                m.attack_cooldown = 2.2;
                let from = [m.body.pos[0], m.body.pos[1] + d.height as f64 * 0.8, m.body.pos[2]];
                let aim = [dx, dy + 1.2 - d.height * 0.8 + dist * 0.06, dz];
                let len = sqrtf(aim[0] * aim[0] + aim[1] * aim[1] + aim[2] * aim[2]).max(0.01);
                self.actions.push(Action::Shoot(from, [aim[0] / len, aim[1] / len, aim[2] / len], d.x.shoots, m.kind));
            }
            // Hunters and guards strike what they caught up with.
            if let Some((id, p)) = quarry {
                let reach = ((p[0] - m.body.pos[0]) * (p[0] - m.body.pos[0]) + (p[2] - m.body.pos[2]) * (p[2] - m.body.pos[2])) as f32;
                if reach < 2.2 && m.attack_cooldown <= 0.0 {
                    m.attack_cooldown = 1.2;
                    let from = m.body.pos;
                    self.actions.push(Action::Bite(id, d.damage.max(2), from));
                }
            }
            // Villagers at work, by day, when they have nowhere to be.
            let m = &mut self.list[i];
            if at_home && day && !d.x.job.is_empty() && quarry.is_none() {
                m.job_timer -= dt;
                if m.job_timer <= 0.0 && from_home < 14.0 {
                    m.job_timer = 2.5 + (roll % 200) as f32 / 100.0;
                    if let Some(a) = self.work(world, i) {
                        self.actions.push(a);
                    }
                }
            }
            i += 1;
        }
        // Bites land after everyone has moved.
        let bites: Vec<(u32, i32, [f64; 3])> = self.actions.iter().filter_map(|a| if let Action::Bite(id, n, from) = a { Some((*id, *n, *from)) } else { None }).collect();
        self.actions.retain(|a| !matches!(a, Action::Bite(..)));
        for (id, n, from) in bites {
            if let Some(k) = self.list.iter().position(|m| m.id == id) {
                self.hurt_by(k, n, from, false);
            }
        }
        damage
    }

    /// Every creature as text, for the other players of a shared game.
    pub fn snapshot(&self) -> String {
        let r = reg();
        let mut out = String::new();
        for m in self.list.iter().take(64) {
            out.push_str(&alloc::format!(
                "{} {} {} {:.2} {:.2} {:.2} {:.2} {:.0} {} {}\n",
                m.id, r.entities[m.kind].id, m.variant, m.body.pos[0], m.body.pos[1], m.body.pos[2], m.body.yaw, m.young, m.health, m.tamed as i32
            ));
        }
        out
    }

    /// Take another player's [`Mobs::snapshot`] as the truth: creatures in
    /// it appear or move (smoothly, see [`Mobs::animate`]); the rest go.
    pub fn apply_snapshot(&mut self, text: &str) {
        let r = reg();
        let mut seen: Vec<u32> = Vec::new();
        // (A host sends at most 64; more is not believed, so a hostile one cannot fill the world.)
        for line in text.lines().take(64) {
            let w: Vec<&str> = line.split(' ').collect();
            let num = |k: usize| w.get(k).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite());
            let (id, kind) = match (w.first().and_then(|v| v.parse::<u32>().ok()), w.get(1).and_then(|id| r.entities.iter().position(|e| e.id == *id))) {
                (Some(id), Some(kind)) => (id, kind),
                _ => continue,
            };
            let (pos, yaw) = match (num(3), num(4), num(5), num(6)) {
                (Some(x), Some(y), Some(z), Some(yaw)) => ([x, y, z], yaw as f32),
                _ => continue,
            };
            let i = match self.list.iter().position(|m| m.id == id) {
                Some(i) if self.list[i].kind == kind => i,
                other => {
                    if let Some(i) = other {
                        self.list.swap_remove(i);
                    }
                    let i = self.spawn(kind, pos);
                    self.list[i].id = id;
                    i
                }
            };
            let m = &mut self.list[i];
            m.variant = (num(2).unwrap_or(0.0) as u8).min(r.entities[kind].x.variants.len() as u8);
            m.goal = Some(pos);
            m.body.yaw = yaw;
            m.young = num(7).unwrap_or(0.0) as f32;
            m.health = num(8).unwrap_or(1.0) as i32;
            m.tamed = num(9) == Some(1.0);
            seen.push(id);
        }
        self.list.retain(|m| seen.contains(&m.id));
        // What a snapshot brings is not a birth (the host raises those).
        self.born.clear();
    }

    /// Move creatures run by another player toward where they were last
    /// said to be, and keep their legs going.
    pub fn animate(&mut self, dt: f32) {
        for m in self.list.iter_mut() {
            m.hurt = (m.hurt - dt).max(0.0);
            m.clock += dt;
            if let Some(goal) = m.goal {
                let k = (dt * 10.0).min(1.0) as f64;
                let before = m.body.pos;
                for a in 0..3 {
                    m.body.pos[a] += (goal[a] - m.body.pos[a]) * k;
                }
                let (dx, dz) = ((m.body.pos[0] - before[0]) as f32, (m.body.pos[2] - before[2]) as f32);
                let speed = if dt > 0.0 { sqrtf(dx * dx + dz * dz) / dt } else { 0.0 };
                let top = reg().entities[m.kind].speed.max(0.1);
                m.stride += ((speed / top).min(1.0) - m.stride) * (dt * 8.0).min(1.0);
                m.phase += speed * dt * 3.2;
                if m.phase > TAU {
                    m.phase -= TAU;
                }
            }
        }
    }

    /// The player's pets as text, one per line, to keep with the world.
    pub fn save_pets(&self) -> String {
        let mut out = String::new();
        for m in self.list.iter().filter(|m| m.tamed && m.health > 0) {
            let id = &reg().entities[m.kind].id;
            out.push_str(&alloc::format!("{} {} {:.2} {:.2} {:.2} {} {}\n", id, m.variant, m.body.pos[0], m.body.pos[1], m.body.pos[2], m.stay as i32, m.health));
        }
        out
    }

    pub fn load_pets(&mut self, text: &str) {
        for line in text.lines() {
            let w: Vec<&str> = line.split(' ').collect();
            let num = |k: usize| w.get(k).and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite());
            if let (Some(kind), Some(x), Some(y), Some(z)) = (reg().entities.iter().position(|e| Some(&e.id.as_str()) == w.first()), num(2), num(3), num(4)) {
                let i = self.spawn(kind, [x, y, z]);
                let m = &mut self.list[i];
                m.variant = (num(1).unwrap_or(0.0) as u8).min(reg().entities[kind].x.variants.len() as u8);
                m.tamed = true;
                m.stay = num(5) == Some(1.0);
                m.health = (num(6).unwrap_or(1.0) as i32).clamp(1, reg().entities[kind].health * 5 / 4 + 1);
            }
        }
    }

    /// The creature a ray hits first, and how far along the ray.
    /// (Targets are never smaller than `MIN_TARGET` blocks.)
    pub fn hit_test(&self, origin: [f64; 3], dir: Vec3, reach: f32) -> Option<(usize, f32)> {
        let d = [dir.x as f64, dir.y as f64, dir.z as f64];
        let mut best: Option<(usize, f32)> = None;
        for (i, m) in self.list.iter().enumerate() {
            let (mut lo, mut hi) = m.body.aabb();
            // Tiny creatures (butterflies, bees, bats) are hard to aim at: the
            // target is at least this big on every side, around their middle.
            for a in 0..3 {
                let missing = MIN_TARGET - (hi[a] - lo[a]);
                if missing > 0.0 {
                    lo[a] -= missing * 0.5;
                    hi[a] += missing * 0.5;
                }
            }
            let (mut t0, mut t1) = (0.0f64, reach as f64);
            let mut hit = true;
            for a in 0..3 {
                if d[a] > -1e-9 && d[a] < 1e-9 {
                    if origin[a] < lo[a] || origin[a] > hi[a] {
                        hit = false;
                    }
                    continue;
                }
                let (mut a0, mut a1) = ((lo[a] - origin[a]) / d[a], (hi[a] - origin[a]) / d[a]);
                if a0 > a1 {
                    core::mem::swap(&mut a0, &mut a1);
                }
                t0 = t0.max(a0);
                t1 = t1.min(a1);
            }
            if hit && t0 <= t1 && best.map_or(true, |b| (t0 as f32) < b.1) {
                best = Some((i, t0 as f32));
            }
        }
        best
    }

    /// Hit creature `i` for `amount`, knocking it away from `from`.
    /// Returns true if that killed it.
    pub fn hurt(&mut self, i: usize, amount: i32, from: [f64; 3]) -> bool {
        self.hurt_by(i, amount, from, true)
    }

    /// As [`Mobs::hurt`]; `by_player` says whether to hold it against them.
    fn hurt_by(&mut self, i: usize, amount: i32, from: [f64; 3], by_player: bool) -> bool {
        if by_player {
            // The whole pack takes it personally.
            let (kind, pos) = match self.list.get(i) {
                Some(m) => (m.kind, m.body.pos),
                None => return false,
            };
            let pack = reg().entities[kind].x.traits & traits::PACK != 0;
            for o in self.list.iter_mut() {
                let (ox, oz) = (o.body.pos[0] - pos[0], o.body.pos[2] - pos[2]);
                let near = ox * ox + oz * oz < 18.0 * 18.0;
                if o.kind == kind && near && pack {
                    o.anger = 20.0;
                    o.think = 0.0;
                }
            }
            if let Some(m) = self.list.get_mut(i) {
                m.anger = 20.0;
            }
        }
        let m = match self.list.get_mut(i) {
            Some(m) => m,
            None => return false,
        };
        m.health -= amount;
        m.hurt = 0.35;
        let (dx, dz) = ((m.body.pos[0] - from[0]) as f32, (m.body.pos[2] - from[2]) as f32);
        let len = sqrtf(dx * dx + dz * dz).max(0.01);
        m.body.vel[0] += dx / len * 6.0;
        m.body.vel[2] += dz / len * 6.0;
        m.body.vel[1] = 5.0;
        // Startled: think again at once (animals bolt, hunters turn).
        m.think = 0.0;
        m.idle = 0.0;
        m.health <= 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::Pool;
    use crate::mesh::MeshData;
    use crate::world::{GpuMesh, MeshSink, NullStore};

    struct NoSink;
    impl MeshSink for NoSink {
        fn upload(&mut self, _: &MeshData) -> GpuMesh {
            GpuMesh::default()
        }
        fn free(&mut self, _: GpuMesh) {}
    }

    /// A loaded world with a big stone platform at y = 120 (feet at 121).
    fn arena() -> World {
        let mut w = World::new(5);
        let mut pool = Pool::new(0, 5);
        for _ in 0..6000 {
            w.update(0, 0, 2, &mut pool, &mut NoSink, &mut NullStore);
            if w.settled {
                break;
            }
        }
        assert!(w.settled);
        for x in -24..40 {
            for z in -24..40 {
                w.set_block(x, 120, z, reg().wk.stone);
            }
        }
        w
    }

    /// The boomer lives in the wildlife mod, which is not part of this repository:
    /// a test that needs it says so and passes when the mod is not there.
    fn no_boomer() -> bool {
        let none = !reg().entities.iter().any(|e| e.id == "boomer");
        if none {
            std::eprintln!("skipped: the wildlife mod (the boomer) is not built in");
        }
        none
    }

    fn kind(id: &str) -> usize {
        reg().entities.iter().position(|e| e.id == id).unwrap()
    }

    fn run(mobs: &mut Mobs, w: &World, target: [f64; 3], time: f32, seconds: f32) -> i32 {
        let mut dmg = 0;
        for _ in 0..(seconds * 30.0) as i32 {
            dmg += mobs.update(1.0 / 30.0, w, target, time, true, None);
        }
        dmg
    }

    #[test]
    fn base_data_defines_creatures_with_models() {
        if crate::native::skip_without_mods() {
            return;
        }
        let r = reg();
        assert!(r.entities.len() >= 3);
        let lurker = &r.entities[kind("lurker")];
        assert_eq!((lurker.behavior, lurker.night, lurker.damage), (Behavior::Hostile, true, 3));
        assert_eq!(r.models[lurker.model].id, "player");
        assert_eq!(r.models[r.entities[kind("cow")].model].id, "cow");
        for m in &r.models {
            crate::model::ModelDef::from_bbmodel(&m.id, &m.text).unwrap();
        }
    }

    #[test]
    fn a_hunter_finds_its_way_around_a_wall_and_attacks() {
        if crate::native::skip_without_mods() {
            return;
        }
        let mut w = arena();
        // A wall between hunter and player, with a way round at one end.
        for x in 0..12 {
            for y in 121..124 {
                w.set_block(x, y, 8, reg().wk.stone);
            }
        }
        let mut mobs = Mobs::new(7);
        mobs.spawn_timer = 1e9; // no extra arrivals in this test
        mobs.spawn(kind("lurker"), [4.5, 121.01, 4.5]);
        let player = [4.5, 121.0, 12.5];
        let dmg = run(&mut mobs, &w, player, 0.75, 14.0);
        let m = &mobs.list[0];
        let d = ((m.body.pos[0] - player[0]).powi(2) + (m.body.pos[2] - player[2]).powi(2)).sqrt();
        assert!(d < 2.0, "the hunter is still {d:.1} blocks away at {:?}", m.body.pos);
        assert!(dmg >= 3, "it never attacked");
    }

    #[test]
    fn animals_wander_and_night_creatures_leave_by_day() {
        if crate::native::skip_without_mods() {
            return;
        }
        let w = arena();
        let mut mobs = Mobs::new(3);
        mobs.spawn_timer = 1e9;
        mobs.spawn(kind("cow"), [8.5, 121.01, 8.5]);
        let far = [8.5, 121.0, 60.0];
        assert_eq!(run(&mut mobs, &w, far, 0.25, 20.0), 0);
        let m = &mobs.list[0];
        let moved = ((m.body.pos[0] - 8.5).powi(2) + (m.body.pos[2] - 8.5).powi(2)).sqrt();
        assert!(moved > 1.0, "the animal never moved");
        assert!((m.body.pos[1] - 121.0).abs() < 0.2, "it left the ground: {:?}", m.body.pos);
        // A hunter far from the player is gone once it is day.
        mobs.spawn(kind("lurker"), [0.5, 121.01, 0.5]);
        run(&mut mobs, &w, far, 0.25, 1.0);
        assert!(mobs.list.iter().all(|m| m.kind != kind("lurker")));
    }

    #[test]
    fn every_animal_of_a_kind_is_a_little_different() {
        if crate::native::skip_without_mods() {
            return;
        }
        let mut mobs = Mobs::new(9);
        mobs.spawn_timer = 1e9;
        for i in 0..24 {
            mobs.spawn(kind("cow"), [8.5 + i as f64 * 3.0, 121.01, 5.5]);
        }
        let base = reg().entities[kind("cow")].health;
        let healths: Vec<i32> = mobs.list.iter().map(|m| m.health).collect();
        assert!(healths.iter().all(|&h| h >= base * 3 / 4 && h <= base * 5 / 4 + 1), "{healths:?}");
        assert!(healths.iter().any(|&h| h != healths[0]), "all {} cows have {} health", healths.len(), healths[0]);
    }

    #[test]
    fn a_cow_takes_several_survival_blows_not_one() {
        if crate::native::skip_without_mods() {
            return;
        }
        // Survival hits with bare hands do 3; a cow has 10 health, so it
        // takes four blows. (Creative mode does 1000: one blow.)
        let mut mobs = Mobs::new(4);
        mobs.spawn_timer = 1e9;
        mobs.spawn(kind("cow"), [8.5, 121.01, 5.5]);
        let eye = [8.5, 122.0, 9.5];
        let (i, _) = mobs.hit_test(eye, Vec3::new(0.0, -0.05, -1.0).norm(), 6.0).expect("ray missed");
        // Each cow has its own health (about ten), so count its blows from that.
        let health = mobs.list[0].health;
        let blows = (health + 2) / 3;
        assert!(blows >= 2, "a cow has {health} health");
        for blow in 0..blows - 1 {
            assert!(!mobs.hurt(i, 3, eye), "dead after blow {}", blow + 1);
        }
        assert!(mobs.hurt(i, 3, eye), "still alive after {blows} blows");
    }

    #[test]
    fn creatures_can_be_hit_and_die() {
        if crate::native::skip_without_mods() {
            return;
        }
        let w = arena();
        let mut mobs = Mobs::new(9);
        mobs.spawn_timer = 1e9;
        mobs.spawn(kind("cow"), [8.5, 121.01, 5.5]);
        let eye = [8.5, 122.0, 9.5];
        // Yaw 0 looks toward -z, where the animal stands.
        let (i, t) = mobs.hit_test(eye, Vec3::new(0.0, -0.05, -1.0).norm(), 6.0).expect("ray missed");
        assert!(t > 3.0 && t < 4.2, "hit at {t}");
        assert!(mobs.hit_test(eye, Vec3::new(0.0, 0.0, 1.0), 6.0).is_none());
        assert!(mobs.hit_test(eye, Vec3::new(0.0, -0.05, -1.0).norm(), 2.0).is_none());
        assert!(!mobs.hurt(i, 4, eye));
        assert!(mobs.list[0].body.vel[2] < -3.0, "no knock-back");
        assert!(mobs.hurt(i, 1000, eye));
        run(&mut mobs, &w, eye, 0.25, 0.2);
        assert!(mobs.list.is_empty());
    }

    #[test]
    fn a_snapshot_cannot_fill_the_world_with_creatures() {
        if crate::native::skip_without_mods() {
            return;
        }
        let mut text = String::new();
        for id in 1..=300 {
            text.push_str(&alloc::format!("{} cow 0 {} 100 5 0 0 5 0\n", id, id));
        }
        let mut mobs = Mobs::new(5);
        mobs.apply_snapshot(&text);
        assert_eq!(mobs.list.len(), 64);
        // Lines that are not creatures are skipped, not trusted.
        mobs.apply_snapshot("x cow 0 1 2 3 4 0 5 0\n7 nosuchthing 0 1 2 3 4 0 5 0\n8 cow 0 nan 2 3 4 0 5 0\n");
        assert!(mobs.list.is_empty());
    }

    #[test]
    fn spawning_fills_up_to_the_cap_and_respects_the_rules() {
        if crate::native::skip_without_mods() {
            return;
        }
        let w = arena();
        let mut mobs = Mobs::new(11);
        let player = [8.5, 121.0, 8.5];
        // By day, with hunters allowed: only day creatures appear.
        for _ in 0..4000 {
            mobs.try_spawn(&w, player, 0.25, true);
        }
        assert!(!mobs.list.is_empty(), "nothing spawned");
        assert!(mobs.list.iter().all(|m| !reg().entities[m.kind].night));
        assert!(mobs.list.iter().all(|m| {
            let d = ((m.body.pos[0] - 8.5).powi(2) + (m.body.pos[2] - 8.5).powi(2)).sqrt();
            // The rest of a herd stands a step or two from the first.
            d > 13.0
        }));
        // At night in creative mode (no hunters): still none.
        let mut mobs = Mobs::new(12);
        for _ in 0..4000 {
            mobs.try_spawn(&w, player, 0.75, false);
        }
        assert!(mobs.list.iter().all(|m| reg().entities[m.kind].behavior != Behavior::Hostile));
        // At night in survival: hunters do come.
        let mut mobs = Mobs::new(13);
        for _ in 0..4000 {
            mobs.try_spawn(&w, player, 0.75, true);
        }
        assert!(mobs.list.iter().any(|m| reg().entities[m.kind].behavior == Behavior::Hostile));
    }

    #[test]
    fn a_boomer_hisses_then_blows_up_beside_a_player() {
        if no_boomer() {
            return;
        }
        let w = arena();
        let mut mobs = Mobs::new(21);
        mobs.spawn_timer = 1e9;
        let boomer = kind("boomer");
        mobs.spawn(boomer, [8.5, 121.01, 8.5]);
        let player = [8.5, 121.0, 10.5];
        let (mut hissed, mut boom, mut at_seconds) = (false, None, 0.0);
        for n in 0..150 {
            mobs.update(1.0 / 30.0, &w, player, 0.25, true, None);
            for a in mobs.actions.drain(..) {
                match a {
                    Action::Fuse(_) => hissed = true,
                    Action::Explode(_, power, k) => {
                        boom = Some((power, k));
                        at_seconds = n as f32 / 30.0;
                    }
                    _ => {}
                }
            }
            if boom.is_some() {
                break;
            }
        }
        assert!(hissed, "no hiss");
        assert_eq!(boom, Some((2.0, boomer)));
        assert!(at_seconds > 1.2 && at_seconds < 2.0, "blew up after {at_seconds}");
        assert!(mobs.list.is_empty(), "it is still there");
    }

    #[test]
    fn a_boomer_that_is_left_behind_puts_its_fuse_out() {
        if no_boomer() {
            return;
        }
        let w = arena();
        let mut mobs = Mobs::new(22);
        mobs.spawn_timer = 1e9;
        mobs.spawn(kind("boomer"), [8.5, 121.01, 8.5]);
        mobs.list[0].fuse = 1.0;
        // The player is far away now.
        mobs.update(1.0 / 30.0, &w, [8.5, 121.0, 30.5], 0.25, true, None);
        assert_eq!(mobs.list[0].fuse, 0.0);
        assert!(mobs.actions.iter().all(|a| !matches!(a, Action::Explode(..))));
        // Nothing happens in creative mode either: such creatures are simply gone.
        mobs.update(1.0 / 30.0, &w, [8.5, 121.0, 9.5], 0.25, false, None);
        assert!(mobs.actions.iter().all(|a| !matches!(a, Action::Explode(..))));
    }

    #[test]
    fn a_creature_appears_in_the_colours_of_its_biome() {
        if no_boomer() {
            return;
        }
        let mut mobs = Mobs::new(23);
        mobs.spawn_timer = 1e9;
        let boomer = kind("boomer");
        let d = &reg().entities[boomer];
        let desert = d.x.variants.iter().position(|v| v.name == "Desert").expect("a desert look") + 1;
        for _ in 0..12 {
            let i = mobs.spawn_in(boomer, [8.5, 121.01, 8.5], Some("desert"));
            assert_eq!(mobs.list[i].variant as usize, desert);
            assert!(mobs.list[i].title().starts_with("Desert "), "{}", mobs.list[i].title());
        }
    }
}
