//! What scripts may do to the game: the host API.
//!
//! The interpreter knows nothing of the game. It talks to a [`Host`], and
//! the functions below are the whole of what a script can reach: there is
//! no file, network or system access. The game's host is [`SessionHost`]
//! (a thin layer over `Session`); the tests use a mock. Every function is
//! listed in [`HOST_FUNCTIONS`] (the table in docs/SCRIPTING.md is written
//! from it, and a test keeps the two in step).

use crate::script::interp::{Interp, Res};
use crate::script::value::Value;
use alloc::format;
use alloc::string::String;

/// The game, as a script sees it. Methods that report success return
/// `false` when the thing could not be done (no such item, not allowed on a
/// server, the place is not loaded).
pub trait Host {
    fn say(&mut self, text: &str);
    fn give(&mut self, item: &str, n: i64) -> bool;
    fn spawn(&mut self, creature: &str, x: f64, y: f64, z: f64) -> bool;
    fn set_block(&mut self, x: i64, y: i64, z: i64, id: &str) -> bool;
    fn get_block(&mut self, x: i64, y: i64, z: i64) -> String;
    fn player_pos(&mut self) -> (f64, f64, f64);
    /// Time of day, 0 to 1 (0.25 is noon).
    fn time(&mut self) -> f64;
    fn set_time(&mut self, t: f64);
    fn explode(&mut self, x: f64, y: f64, z: f64, power: f64);
    fn effect(&mut self, id: &str, secs: f64, level: i64) -> bool;
    fn play_sound(&mut self, id: &str);
    /// Run a chat command as the player; what it answers.
    fn run_command(&mut self, text: &str) -> String;
    /// A number from 0 up to (not including) `n`.
    fn rand(&mut self, n: i64) -> i64;
    fn log(&mut self, text: &str);

    fn health(&mut self) -> i64 {
        0
    }
    fn heal(&mut self, _n: i64) {}
    fn hurt(&mut self, _n: i64) {}
    fn held_item(&mut self) -> String {
        String::new()
    }
    fn item_count(&mut self, _id: &str) -> i64 {
        0
    }
    fn take_item(&mut self, _id: &str, _n: i64) -> bool {
        false
    }
    fn teleport(&mut self, _x: f64, _y: f64, _z: f64) {}
    fn dimension(&mut self) -> String {
        String::new()
    }
    /// Give an effect to `who` (`"player"` or `"target"`, the creature in the
    /// crosshair); `secs` of 0 means the effect's own time.
    fn apply_effect(&mut self, _who: &str, _id: &str, _secs: f64, _level: i64) -> bool {
        false
    }
    /// Take an effect (all of them for an empty id) from `who`.
    fn remove_effect(&mut self, _who: &str, _id: &str) -> bool {
        false
    }
    /// The level of an effect on `who`; 0 when it is not active.
    fn effect_level(&mut self, _who: &str, _id: &str) -> i64 {
        0
    }
    /// Set a number the post-processing shader reads as `u_<name>` (see
    /// docs/SHADERS.md); false for a bad name or a value that is not a number.
    fn set_shader_uniform(&mut self, _name: &str, _value: f64) -> bool {
        false
    }
    /// The mod whose script is about to run (for the log).
    fn set_mod(&mut self, _name: &str) {}
    /// The running game, for the host functions of `script/ext/` (world
    /// queries, creatures, bodies, drawing...). `None` for a host that is
    /// not the game (the tests' mock): those functions then fail with a message.
    fn session(&mut self) -> Option<&mut crate::session::Session> {
        None
    }
    /// The game's data folder (ending with `/`), where a mod's files under
    /// `tmp/<mod>/` live; empty when there is none.
    fn data_dir(&mut self) -> String {
        String::new()
    }

    // ---- the interface (see `host_ui.rs`)

    /// What mods keep on the HUD, in their screens and for their keys.
    fn ui(&mut self) -> &mut crate::client::modui::ModUi;
    /// Hunger, 0 to 20.
    fn hunger(&mut self) -> i64 {
        0
    }
    /// Breath left, 0 to 10.
    fn air(&mut self) -> f64 {
        10.0
    }
    /// The biome under the player, lower case.
    fn biome(&mut self) -> String {
        String::new()
    }
    /// The hotbar slot chosen, 0 to 8.
    fn selected_slot(&mut self) -> i64 {
        0
    }
    /// The compass heading in degrees (0 north, 90 east).
    fn heading(&mut self) -> f64 {
        0.0
    }
    /// Whether the mod's switch `id` is on.
    fn setting_on(&mut self, _mod_name: &str, _id: &str) -> bool {
        false
    }

    // The calls of host_qol.rs: facing, biome, block facts, the screen line,
    // the pack's slots and chests.
    fn block_light(&mut self, _id: &str) -> i64 {
        0
    }
    fn block_drop(&mut self, _id: &str) -> String {
        String::new()
    }
    fn survival(&mut self) -> bool {
        true
    }
    fn sneaking(&mut self) -> bool {
        false
    }
    fn hud(&mut self, _text: &str) {}
    fn slot_item(&mut self, _i: i64) -> String {
        String::new()
    }
    fn slot_count(&mut self, _i: i64) -> i64 {
        0
    }
    fn move_slot(&mut self, _from: i64, _to: i64) -> bool {
        false
    }
    fn stack_to_chest(&mut self, _x: i64, _y: i64, _z: i64) -> i64 {
        0
    }
}

/// Every host function: (signature, what it does). Kept in step with
/// `call_host_fn` by a test, and with docs/SCRIPTING.md.
pub const HOST_FUNCTIONS: &[(&str, &str)] = &[
    ("say(text: &str)", "show a line in the chat"),
    ("log(text: &str)", "write a line to the game's log"),
    ("give(item_id: &str, n: i64) -> bool", "put items in the player's pack; false if there is no such item or no room"),
    ("take_item(item_id: &str, n: i64) -> bool", "remove items from the pack; false if the player has fewer"),
    ("item_count(item_id: &str) -> i64", "how many of an item the player carries"),
    ("held_item() -> String", "the id of the item in the player's hand (empty if none)"),
    ("spawn(creature_id: &str, x: f64, y: f64, z: f64) -> bool", "make a creature appear"),
    ("set_block(x: i64, y: i64, z: i64, block_id: &str) -> bool", "put a block somewhere (false if that is not loaded or not allowed)"),
    ("get_block(x: i64, y: i64, z: i64) -> String", "the id of the block at a place (\"air\" where nothing is loaded)"),
    ("player_pos() -> (f64, f64, f64)", "where the player is"),
    ("tp(x: f64, y: f64, z: f64)", "move the player"),
    ("dimension() -> String", "the dimension the player is in (empty for the home world)"),
    ("time() -> f64", "the time of day, 0 to 1 (0 is sunrise, 0.25 noon)"),
    ("set_time(t: f64)", "set the time of day"),
    ("explode(x: f64, y: f64, z: f64, power: f64)", "set off an explosion (power 0.5 to 8)"),
    ("effect(id: &str, secs: f64, level: i64) -> bool", "give the player a status effect"),
    ("apply_effect(who: &str, id: &str, secs: f64, level: i64) -> bool", "give a status effect to \"player\" or \"target\" (the creature in the crosshair); 0 seconds is the effect's own time"),
    ("remove_effect(who: &str, id: &str) -> bool", "take a status effect (all of them for an empty id) from \"player\" or \"target\"; false if it was not there"),
    ("has_effect(who: &str, id: &str) -> bool", "whether \"player\" or \"target\" has that status effect"),
    ("effect_level(who: &str, id: &str) -> i64", "the level of a status effect on \"player\" or \"target\"; 0 if it is not active"),
    ("play_sound(id: &str)", "play a sound"),
    ("run_command(text: &str) -> String", "carry out a chat command as the player (with or without the `/`); returns what it answers"),
    ("rand(n: i64) -> i64", "a random whole number from 0 up to n (not including n)"),
    ("player_health() -> i64", "the player's health (20 is full)"),
    ("heal(n: i64)", "give health back"),
    ("hurt(n: i64)", "hurt the player (survival; armour and hooks apply)"),
    ("state_get(key: &str) -> String", "what this mod saved under a key (empty if nothing)"),
    ("state_set(key: &str, value: &str)", "save text under a key (kept per world, in the mod's data folder)"),
    ("state_get_int(key: &str, default: i64) -> i64", "a saved whole number (the default if there is none)"),
    ("state_set_int(key: &str, value: i64)", "save a whole number"),
    ("mod_name() -> String", "the name of the mod this script belongs to"),
    ("set_shader_uniform(name: &str, value: f64) -> bool", "set the number `u_<name>` that the player's post-processing shader reads (see SHADERS.md)"),
    ("block_light(block_id: &str) -> i64", "how much light a block gives off, 0 to 15"),
    ("block_drop(block_id: &str) -> String", "the id of the item a block gives when broken in survival (empty if none)"),
    ("survival() -> bool", "whether the player plays in survival mode"),
    ("sneaking() -> bool", "whether the player is crouching (the sneak key is held)"),
    ("hud(text: &str)", "show a line of text at the top left of the screen for a moment (call it again within a second to keep it; empty text takes it away); one line per mod"),
    ("slot_item(slot: i64) -> String", "the id of the item in a pack slot (0 to 35, the first nine are the hotbar; empty if none)"),
    ("slot_count(slot: i64) -> i64", "how many items are in a pack slot"),
    ("move_slot(from: i64, to: i64) -> bool", "move a pack slot's items to another slot: into an empty one, onto the same item as far as it fits, else the two swap; false if nothing moved"),
    ("stack_to_chest(x: i64, y: i64, z: i64) -> i64", "put the items of the pack (not the hotbar) that the chest at that block already holds into it; how many moved (0 if no chest, or in a shared game)"),
];

// ---- argument helpers (also used by the built-in library)

pub(crate) fn want(name: &str, args: &[Value], n: usize) -> Result<(), String> {
    if args.len() == n {
        Ok(())
    } else {
        Err(format!("`{}` takes {} argument(s) but {} were given", name, n, args.len()))
    }
}

pub(crate) fn num(name: &str, args: &[Value], i: usize) -> Result<f64, String> {
    match args.get(i) {
        Some(Value::Float(f)) => Ok(*f),
        Some(Value::Int(n)) => Ok(*n as f64),
        Some(other) => Err(format!("`{}`: argument {} should be a number, found {}", name, i + 1, other.type_name())),
        None => Err(format!("`{}` is missing argument {}", name, i + 1)),
    }
}

/// A whole number; a float is rounded down.
pub(crate) fn int(name: &str, args: &[Value], i: usize) -> Result<i64, String> {
    match args.get(i) {
        Some(Value::Int(n)) => Ok(*n),
        Some(Value::Float(f)) => Ok(crate::script::numeric::floor(*f) as i64),
        Some(other) => Err(format!("`{}`: argument {} should be a whole number, found {}", name, i + 1, other.type_name())),
        None => Err(format!("`{}` is missing argument {}", name, i + 1)),
    }
}

pub(crate) fn text<'v>(name: &str, args: &'v [Value], i: usize) -> Result<&'v str, String> {
    match args.get(i) {
        Some(Value::Str(s)) => Ok(s.as_str()),
        Some(other) => Err(format!("`{}`: argument {} should be a string, found {}", name, i + 1, other.type_name())),
        None => Err(format!("`{}` is missing argument {}", name, i + 1)),
    }
}

/// Call the host function `name`, if there is one. `None`: no such function.
pub(crate) fn call_host_fn(it: &mut Interp, name: &str, args: &[Value]) -> Option<Res> {
    let r: Result<Value, String> = match name {
        "say" => want(name, args, 1).and_then(|_| text(name, args, 0)).map(|t| {
            it.host.say(t);
            Value::Unit
        }),
        "log" => want(name, args, 1).and_then(|_| text(name, args, 0)).map(|t| {
            it.host.log(t);
            Value::Unit
        }),
        "give" => want(name, args, 2).and_then(|_| Ok((text(name, args, 0)?, int(name, args, 1)?))).map(|(id, n)| Value::Bool(it.host.give(id, n))),
        "take_item" => want(name, args, 2).and_then(|_| Ok((text(name, args, 0)?, int(name, args, 1)?))).map(|(id, n)| Value::Bool(it.host.take_item(id, n))),
        "item_count" => want(name, args, 1).and_then(|_| text(name, args, 0)).map(|id| Value::Int(it.host.item_count(id))),
        "held_item" => want(name, args, 0).map(|_| Value::string(it.host.held_item())),
        "spawn" => want(name, args, 4).and_then(|_| Ok((text(name, args, 0)?, num(name, args, 1)?, num(name, args, 2)?, num(name, args, 3)?))).map(|(id, x, y, z)| Value::Bool(it.host.spawn(id, x, y, z))),
        "set_block" => want(name, args, 4).and_then(|_| Ok((int(name, args, 0)?, int(name, args, 1)?, int(name, args, 2)?, text(name, args, 3)?))).map(|(x, y, z, id)| Value::Bool(it.host.set_block(x, y, z, id))),
        "get_block" => want(name, args, 3).and_then(|_| Ok((int(name, args, 0)?, int(name, args, 1)?, int(name, args, 2)?))).map(|(x, y, z)| Value::string(it.host.get_block(x, y, z))),
        "player_pos" => want(name, args, 0).map(|_| {
            let (x, y, z) = it.host.player_pos();
            Value::tuple(alloc::vec![Value::Float(x), Value::Float(y), Value::Float(z)])
        }),
        "tp" => want(name, args, 3).and_then(|_| Ok((num(name, args, 0)?, num(name, args, 1)?, num(name, args, 2)?))).map(|(x, y, z)| {
            it.host.teleport(x, y, z);
            Value::Unit
        }),
        "dimension" => want(name, args, 0).map(|_| Value::string(it.host.dimension())),
        "time" => want(name, args, 0).map(|_| Value::Float(it.host.time())),
        "set_time" => want(name, args, 1).and_then(|_| num(name, args, 0)).map(|t| {
            it.host.set_time(t);
            Value::Unit
        }),
        "explode" => want(name, args, 4).and_then(|_| Ok((num(name, args, 0)?, num(name, args, 1)?, num(name, args, 2)?, num(name, args, 3)?))).map(|(x, y, z, p)| {
            it.host.explode(x, y, z, p);
            Value::Unit
        }),
        "effect" => want(name, args, 3).and_then(|_| Ok((text(name, args, 0)?, num(name, args, 1)?, int(name, args, 2)?))).map(|(id, secs, level)| Value::Bool(it.host.effect(id, secs, level))),
        "apply_effect" => want(name, args, 4).and_then(|_| Ok((text(name, args, 0)?, text(name, args, 1)?, num(name, args, 2)?, int(name, args, 3)?))).map(|(who, id, secs, level)| Value::Bool(it.host.apply_effect(who, id, secs, level))),
        "remove_effect" => want(name, args, 2).and_then(|_| Ok((text(name, args, 0)?, text(name, args, 1)?))).map(|(who, id)| Value::Bool(it.host.remove_effect(who, id))),
        "has_effect" => want(name, args, 2).and_then(|_| Ok((text(name, args, 0)?, text(name, args, 1)?))).map(|(who, id)| Value::Bool(it.host.effect_level(who, id) > 0)),
        "effect_level" => want(name, args, 2).and_then(|_| Ok((text(name, args, 0)?, text(name, args, 1)?))).map(|(who, id)| Value::Int(it.host.effect_level(who, id))),
        "play_sound" => want(name, args, 1).and_then(|_| text(name, args, 0)).map(|id| {
            it.host.play_sound(id);
            Value::Unit
        }),
        "run_command" => want(name, args, 1).and_then(|_| text(name, args, 0)).map(|t| Value::string(it.host.run_command(t))),
        "rand" => want(name, args, 1).and_then(|_| int(name, args, 0)).map(|n| Value::Int(if n <= 0 { 0 } else { it.host.rand(n) })),
        "player_health" => want(name, args, 0).map(|_| Value::Int(it.host.health())),
        "heal" => want(name, args, 1).and_then(|_| int(name, args, 0)).map(|n| {
            it.host.heal(n);
            Value::Unit
        }),
        "hurt" => want(name, args, 1).and_then(|_| int(name, args, 0)).map(|n| {
            it.host.hurt(n);
            Value::Unit
        }),
        "state_get" => want(name, args, 1).and_then(|_| text(name, args, 0)).map(|k| Value::str(it.state.get(k).unwrap_or(""))),
        "state_set" => want(name, args, 2).and_then(|_| Ok((text(name, args, 0)?, text(name, args, 1)?))).and_then(|(k, v)| it.state.set(k, v)).map(|_| Value::Unit),
        "state_get_int" => want(name, args, 2).and_then(|_| Ok((text(name, args, 0)?, int(name, args, 1)?))).map(|(k, d)| Value::Int(it.state.get(k).and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(d))),
        "state_set_int" => want(name, args, 2).and_then(|_| Ok((text(name, args, 0)?, int(name, args, 1)?))).and_then(|(k, v)| it.state.set(k, &format!("{}", v))).map(|_| Value::Unit),
        "mod_name" => want(name, args, 0).map(|_| Value::str(it.mod_name)),
        "set_shader_uniform" => want(name, args, 2).and_then(|_| Ok((text(name, args, 0)?, num(name, args, 1)?))).map(|(n, v)| Value::Bool(it.host.set_shader_uniform(n, v))),
        "block_light" => want(name, args, 1).and_then(|_| text(name, args, 0)).map(|id| Value::Int(it.host.block_light(id))),
        "block_drop" => want(name, args, 1).and_then(|_| text(name, args, 0)).map(|id| Value::string(it.host.block_drop(id))),
        "survival" => want(name, args, 0).map(|_| Value::Bool(it.host.survival())),
        "sneaking" => want(name, args, 0).map(|_| Value::Bool(it.host.sneaking())),
        "hud" => want(name, args, 1).and_then(|_| text(name, args, 0)).map(|t| {
            it.host.hud(t);
            Value::Unit
        }),
        "slot_item" => want(name, args, 1).and_then(|_| int(name, args, 0)).map(|i| Value::string(it.host.slot_item(i))),
        "slot_count" => want(name, args, 1).and_then(|_| int(name, args, 0)).map(|i| Value::Int(it.host.slot_count(i))),
        "move_slot" => want(name, args, 2).and_then(|_| Ok((int(name, args, 0)?, int(name, args, 1)?))).map(|(a, b)| Value::Bool(it.host.move_slot(a, b))),
        "stack_to_chest" => want(name, args, 3).and_then(|_| Ok((int(name, args, 0)?, int(name, args, 1)?, int(name, args, 2)?))).map(|(x, y, z)| Value::Int(it.host.stack_to_chest(x, y, z))),
        // The rest of the host functions are in script/host_ui.rs and script/ext/.
        _ => return crate::script::host_ui::call(it, name, args).or_else(|| crate::script::ext::call(it, name, args)),
    };
    Some(match r {
        Ok(v) => Ok(v),
        Err(m) => it.fail(m),
    })
}

// ---- the game's host

/// The real host: a `Session`.
pub struct SessionHost<'g> {
    pub g: &'g mut crate::session::Session,
    pub rng: u32,
    pub mod_name: String,
    /// The data folder, for `tmp_read` and `tmp_write`.
    pub data_dir: String,
}

impl SessionHost<'_> {
    fn may_change_world(&self) -> bool {
        self.g.net.is_none() || self.g.runs_the_world()
    }

    fn item(id: &str) -> Option<u16> {
        crate::block::reg().items.iter().position(|it| it.id == id).map(|i| i as u16)
    }
}

impl Host for SessionHost<'_> {
    fn say(&mut self, text: &str) {
        self.g.push_chat(text.to_ascii_uppercase());
    }

    fn give(&mut self, item: &str, n: i64) -> bool {
        match Self::item(item) {
            Some(i) => {
                // What does not fit lies at the player's feet rather than vanishing.
                self.g.give_or_drop(i, n.clamp(1, 999) as u16);
                true
            }
            None => false,
        }
    }

    fn spawn(&mut self, creature: &str, x: f64, y: f64, z: f64) -> bool {
        let Some(kind) = crate::block::reg().entities.iter().position(|e| e.id == creature) else { return false };
        if !self.may_change_world() || !(x.is_finite() && y.is_finite() && z.is_finite()) {
            return false;
        }
        // A script cannot fill the world with creatures (the natural cap is far lower).
        if self.g.mobs.list.len() >= crate::entity::SCRIPT_CAP {
            return false;
        }
        self.g.mobs.spawn(kind, [x, y.clamp(1.0, crate::chunk::CH as f64 + 20.0), z]);
        true
    }

    fn set_block(&mut self, x: i64, y: i64, z: i64, id: &str) -> bool {
        let Some(b) = crate::block::reg().block(id) else { return false };
        const EDGE: i64 = 30_000_000;
        if !self.may_change_world() || x.abs() > EDGE || z.abs() > EDGE || !(0..crate::chunk::CH as i64).contains(&y) {
            return false;
        }
        // `on_chunk_generated` may write only so many blocks in one call.
        if let Some(left) = self.g.ev.block_budget {
            if left == 0 {
                return false;
            }
            self.g.ev.block_budget = Some(left - 1);
        }
        self.g.world.set_block_cause(crate::terrain::world_log::cause::SCRIPT, x as i32, y as i32, z as i32, b)
    }

    fn get_block(&mut self, x: i64, y: i64, z: i64) -> String {
        const EDGE: i64 = 30_000_000;
        if x.abs() > EDGE || z.abs() > EDGE {
            return String::from("air");
        }
        let b = self.g.world.get(x as i32, y.clamp(-1, crate::chunk::CH as i64) as i32, z as i32);
        crate::block::def(b).id.clone()
    }

    fn player_pos(&mut self) -> (f64, f64, f64) {
        let p = self.g.player.pos;
        (p[0], p[1], p[2])
    }

    fn time(&mut self) -> f64 {
        crate::script::value::clean_f32(self.g.time)
    }

    fn set_time(&mut self, t: f64) {
        if self.g.net.is_none() && t >= 0.0 && t < 1.0 {
            self.g.time = t as f32;
        }
    }

    fn explode(&mut self, x: f64, y: f64, z: f64, power: f64) {
        if self.may_change_world() && power > 0.0 {
            crate::explosion::explode(self.g, [x, y, z], power as f32, crate::explosion::ExplosionSource::Mod(0x5C21));
        }
    }

    fn effect(&mut self, id: &str, secs: f64, level: i64) -> bool {
        crate::game::effects::apply::give(self.g, id, secs as f32, level.clamp(1, 255) as i32).is_ok()
    }

    fn apply_effect(&mut self, who: &str, id: &str, secs: f64, level: i64) -> bool {
        crate::game::effects::script_api::give(self.g, who, id, secs, level)
    }

    fn remove_effect(&mut self, who: &str, id: &str) -> bool {
        crate::game::effects::script_api::remove(self.g, who, id)
    }

    fn effect_level(&mut self, who: &str, id: &str) -> i64 {
        crate::game::effects::script_api::level(self.g, who, id)
    }

    fn play_sound(&mut self, id: &str) {
        if self.g.sound_queue.len() < 32 {
            self.g.sound_queue.push(String::from(id));
        }
    }

    fn run_command(&mut self, text: &str) -> String {
        let line = if text.starts_with('/') { String::from(text) } else { format!("/{}", text) };
        self.g.command(&line).unwrap_or_default()
    }

    fn rand(&mut self, n: i64) -> i64 {
        // xorshift32
        let mut x = if self.rng == 0 { 0x9E37_79B9 } else { self.rng };
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        ((x as u64) % (n.max(1) as u64)) as i64
    }

    fn log(&mut self, text: &str) {
        crate::sys::log(&format!("script {}: {}", self.mod_name, text));
    }

    fn health(&mut self) -> i64 {
        self.g.health as i64
    }

    fn heal(&mut self, n: i64) {
        if n > 0 && self.g.health > 0 {
            self.g.health = (self.g.health as i64 + n).min(crate::session::MAX_HEALTH as i64) as i32;
        }
    }

    fn hurt(&mut self, n: i64) {
        self.g.hurt_later(n.clamp(0, 100) as i32);
    }

    fn held_item(&mut self) -> String {
        self.g.held().map(|h| h.1.id.clone()).unwrap_or_default()
    }

    fn item_count(&mut self, id: &str) -> i64 {
        Self::item(id).map_or(0, |i| self.g.inv.count(i) as i64)
    }

    fn take_item(&mut self, id: &str, n: i64) -> bool {
        match Self::item(id) {
            Some(i) => self.g.inv.remove(i, n.clamp(1, 999) as u16),
            None => false,
        }
    }

    fn teleport(&mut self, x: f64, y: f64, z: f64) {
        if x.is_finite() && y.is_finite() && z.is_finite() {
            self.g.command(&format!("/tp {} {} {}", x, y, z));
        }
    }

    fn dimension(&mut self) -> String {
        self.g.dim.clone()
    }

    fn set_shader_uniform(&mut self, name: &str, value: f64) -> bool {
        self.g.shader_uniforms.set(name, value)
    }

    fn set_mod(&mut self, name: &str) {
        self.mod_name = String::from(name);
    }

    fn session(&mut self) -> Option<&mut crate::session::Session> {
        Some(&mut *self.g)
    }

    fn data_dir(&mut self) -> String {
        self.data_dir.clone()
    }

    fn ui(&mut self) -> &mut crate::client::modui::ModUi {
        // Safety: the state is never freed; no code of the interface calls a
        // script while it holds the state (see `modui::with_ui`).
        unsafe { &mut *crate::client::modui::ui_ptr() }
    }

    fn hunger(&mut self) -> i64 {
        self.g.hunger as i64
    }

    fn air(&mut self) -> f64 {
        crate::script::value::clean_f32(self.g.air)
    }

    fn biome(&mut self) -> String {
        crate::client::modui::info::biome_name(self.g)
    }

    fn selected_slot(&mut self) -> i64 {
        self.g.selected as i64
    }

    fn heading(&mut self) -> f64 {
        crate::client::modui::info::heading_degrees(self.g.player.yaw)
    }

    fn setting_on(&mut self, mod_name: &str, id: &str) -> bool {
        crate::client::modui::keys::switch_on(self.g, mod_name, id)
    }

    fn block_light(&mut self, id: &str) -> i64 {
        crate::script::host_qol::block_light(id)
    }

    fn block_drop(&mut self, id: &str) -> String {
        crate::script::host_qol::block_drop(id)
    }

    fn survival(&mut self) -> bool {
        crate::script::host_qol::survival(self.g)
    }

    fn sneaking(&mut self) -> bool {
        crate::script::host_qol::sneaking(self.g)
    }

    fn hud(&mut self, text: &str) {
        let key = self.mod_name.clone();
        self.g.script_hud.set(&key, text);
    }

    fn slot_item(&mut self, i: i64) -> String {
        crate::script::host_qol::slot_item(self.g, i)
    }

    fn slot_count(&mut self, i: i64) -> i64 {
        crate::script::host_qol::slot_count(self.g, i)
    }

    fn move_slot(&mut self, from: i64, to: i64) -> bool {
        crate::script::host_qol::move_slot(self.g, from, to)
    }

    fn stack_to_chest(&mut self, x: i64, y: i64, z: i64) -> i64 {
        crate::script::host_qol::stack_to_chest(self.g, x, y, z)
    }
}

/// A host that records what it was asked, for the tests.
#[cfg(test)]
pub mod mock {
    use super::*;
    use alloc::vec::Vec;

    #[derive(Default)]
    pub struct MockHost {
        pub calls: Vec<String>,
        pub pos: (f64, f64, f64),
        pub time: f64,
        pub blocks: Vec<((i64, i64, i64), String)>,
        pub commands: Vec<String>,
        pub reply: String,
        pub rng: u32,
        pub health: i64,
        pub items: Vec<(String, i64)>,
        pub ui: crate::client::modui::ModUi,
        pub hunger: i64,
        pub air: f64,
        pub biome: String,
        pub selected: i64,
        pub heading: f64,
        pub switches: Vec<(String, bool)>,
        /// For the calls of host_qol.rs.
        pub held: String,
        pub sneak: bool,
        /// The pack: (item id, count), an empty id for an empty slot.
        pub slots: Vec<(String, i64)>,
    }

    impl Host for MockHost {
        fn say(&mut self, text: &str) {
            self.calls.push(format!("say {}", text));
        }
        fn give(&mut self, item: &str, n: i64) -> bool {
            self.calls.push(format!("give {} {}", item, n));
            item != "nothing"
        }
        fn spawn(&mut self, creature: &str, x: f64, y: f64, z: f64) -> bool {
            self.calls.push(format!("spawn {} {} {} {}", creature, x, y, z));
            true
        }
        fn set_block(&mut self, x: i64, y: i64, z: i64, id: &str) -> bool {
            self.calls.push(format!("set_block {} {} {} {}", x, y, z, id));
            self.blocks.push(((x, y, z), String::from(id)));
            true
        }
        fn get_block(&mut self, x: i64, y: i64, z: i64) -> String {
            self.blocks.iter().rev().find(|b| b.0 == (x, y, z)).map_or_else(|| String::from("air"), |b| b.1.clone())
        }
        fn player_pos(&mut self) -> (f64, f64, f64) {
            self.pos
        }
        fn time(&mut self) -> f64 {
            self.time
        }
        fn set_time(&mut self, t: f64) {
            self.calls.push(format!("set_time {}", t));
            self.time = t;
        }
        fn explode(&mut self, x: f64, y: f64, z: f64, power: f64) {
            self.calls.push(format!("explode {} {} {} {}", x, y, z, power));
        }
        fn effect(&mut self, id: &str, secs: f64, level: i64) -> bool {
            self.calls.push(format!("effect {} {} {}", id, secs, level));
            true
        }
        fn apply_effect(&mut self, who: &str, id: &str, secs: f64, level: i64) -> bool {
            self.calls.push(format!("apply_effect {} {} {} {}", who, id, secs, level));
            true
        }
        fn remove_effect(&mut self, who: &str, id: &str) -> bool {
            self.calls.push(format!("remove_effect {} {}", who, id));
            true
        }
        fn effect_level(&mut self, who: &str, id: &str) -> i64 {
            if who == "player" && id == "poison" {
                2
            } else {
                0
            }
        }
        fn play_sound(&mut self, id: &str) {
            self.calls.push(format!("sound {}", id));
        }
        fn run_command(&mut self, text: &str) -> String {
            self.commands.push(String::from(text));
            self.reply.clone()
        }
        fn rand(&mut self, n: i64) -> i64 {
            self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            ((self.rng >> 8) as i64) % n
        }
        fn log(&mut self, text: &str) {
            self.calls.push(format!("log {}", text));
        }
        fn health(&mut self) -> i64 {
            self.health
        }
        fn heal(&mut self, n: i64) {
            self.health = (self.health + n).min(20);
        }
        fn hurt(&mut self, n: i64) {
            self.health -= n;
        }
        fn item_count(&mut self, id: &str) -> i64 {
            self.items.iter().find(|i| i.0 == id).map_or(0, |i| i.1)
        }
        fn held_item(&mut self) -> String {
            if self.held.is_empty() {
                String::from("stick")
            } else {
                self.held.clone()
            }
        }
        fn block_light(&mut self, id: &str) -> i64 {
            if id == "lamp" {
                15
            } else {
                0
            }
        }
        fn block_drop(&mut self, id: &str) -> String {
            String::from(id)
        }
        fn sneaking(&mut self) -> bool {
            self.sneak
        }
        fn hud(&mut self, text: &str) {
            self.calls.push(format!("hud {}", text));
        }
        fn slot_item(&mut self, i: i64) -> String {
            self.slots.get(i as usize).map_or_else(String::new, |s| s.0.clone())
        }
        fn slot_count(&mut self, i: i64) -> i64 {
            self.slots.get(i as usize).map_or(0, |s| s.1)
        }
        fn move_slot(&mut self, from: i64, to: i64) -> bool {
            let (a, b) = (from as usize, to as usize);
            if a >= self.slots.len() || b >= self.slots.len() || a == b || self.slots[a].0.is_empty() {
                return false;
            }
            if self.slots[b].0.is_empty() {
                self.slots.swap(a, b);
            } else if self.slots[b].0 == self.slots[a].0 {
                let n = (64 - self.slots[b].1).min(self.slots[a].1);
                if n == 0 {
                    return false;
                }
                self.slots[b].1 += n;
                self.slots[a].1 -= n;
                if self.slots[a].1 == 0 {
                    self.slots[a] = (String::new(), 0);
                }
            } else {
                self.slots.swap(a, b);
            }
            true
        }
        fn stack_to_chest(&mut self, x: i64, y: i64, z: i64) -> i64 {
            self.calls.push(format!("stack_to_chest {} {} {}", x, y, z));
            5
        }
        fn ui(&mut self) -> &mut crate::client::modui::ModUi {
            &mut self.ui
        }
        fn hunger(&mut self) -> i64 {
            self.hunger
        }
        fn air(&mut self) -> f64 {
            self.air
        }
        fn biome(&mut self) -> String {
            self.biome.clone()
        }
        fn selected_slot(&mut self) -> i64 {
            self.selected
        }
        fn heading(&mut self) -> f64 {
            self.heading
        }
        fn setting_on(&mut self, mod_name: &str, id: &str) -> bool {
            let name = alloc::format!("{}.{}", mod_name, id);
            self.switches.iter().find(|s| s.0 == name).map_or(false, |s| s.1)
        }
        fn set_shader_uniform(&mut self, name: &str, value: f64) -> bool {
            self.calls.push(format!("shader_uniform {} {}", name, value));
            crate::client::shaders::names::valid_user_name(name) && value.is_finite()
        }
    }
}
