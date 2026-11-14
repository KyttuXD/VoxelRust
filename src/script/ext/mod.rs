//! The host functions beyond the basics of `script/host.rs`: what a mod that
//! used to need compiled code (a stalker, a rail layer, a vehicle) asks of the
//! game. Each file is one subject; every one is bounded (counts, sizes,
//! distances), works only in a running game, and on a server is refused the
//! same way `set_block` is unless the player may change the world there.
//!
//! | file | subject |
//! | --- | --- |
//! | `world.rs` | is a column loaded, ground height, biome, solid, block tags, scanning a box, rays |
//! | `mobs.rs` | find, spawn, move, resize, hurt and remove creatures |
//! | `player.rs` | look, input, camera hold, sleep, flashes, aim, cooldowns |
//! | `draw.rs` | HUD rectangles and text, models and specks drawn in the world |
//! | `bodies.rs` | physics bodies (the game's own movement and collision) for things that move |
//! | `storage.rs` | chest-sized storage kept by a mod, opened on the chest screen |
//! | `pack.rs` | reading the pack lines and sections a mod claims (`claims` in pack.txt) |
//! | `files.rs` | small text files in `<data>/tmp/<mod>/` |
//! | `net.rs` | messages to the same mod on the other players' machines |
//! | `effects.rs` | shots in flight, a forced sky, pushes |
//! | `gear.rs` | armour slots, durability, item data and tags |
//! | `attrs.rs` | the player's attributes, effect timers and lists |
//! | `mobs_model.rs` | giant creatures, per-bone model overrides, tilt, state text |
//! | `modmsg.rs` | messages to the same mod on the other players' machines (`on_message`) |
//! | `structures.rs` | `rand_chunk` and `world_seed`, for structures placed as chunks are generated |
//! | `screens.rs` | opening the crafting grid and the inventory |
//! | `fx.rs` | dropped items, explosions with flags, flying blocks, particles, translucent quads, the sky of a dimension |
//! | `data.rs` | what the session keeps for all of this |
//!
//! The functions are listed in each file's `FUNCTIONS` and in the table of
//! docs/SCRIPTING.md; a test keeps the two in step.

pub mod args;
pub mod attrs;
pub mod bodies;
pub mod data;
pub mod draw;
pub mod effects;
pub mod files;
pub mod gear;
pub mod fx;
pub mod mobs;
pub mod mobs_model;
pub mod modmsg;
pub mod net;
pub mod pack;
pub mod player;
pub mod screens;
pub mod storage;
pub mod structures;
pub mod world;

#[cfg(test)]
mod tests;

pub use data::{data, new_frame, ScriptData, KEY, STORAGE_KEY};

use crate::script::interp::{Interp, Res};
use crate::script::value::Value;
use alloc::format;
use alloc::string::String;

/// What each call may know besides the game: which mod it is, and where the data folder is.
pub struct Ctx {
    pub mod_name: String,
    pub data_dir: String,
}

/// Every function of every file, in the order the docs list them.
pub const MODULES: &[&[(&str, &str)]] = &[world::FUNCTIONS, mobs::FUNCTIONS, player::FUNCTIONS, draw::FUNCTIONS, bodies::FUNCTIONS, storage::FUNCTIONS, pack::FUNCTIONS, files::FUNCTIONS, net::FUNCTIONS, effects::FUNCTIONS, gear::FUNCTIONS, attrs::FUNCTIONS, mobs_model::FUNCTIONS, modmsg::FUNCTIONS, structures::FUNCTIONS, screens::FUNCTIONS, fx::FUNCTIONS];

/// All the (signature, what it does) rows.
pub fn all_functions() -> impl Iterator<Item = &'static (&'static str, &'static str)> {
    MODULES.iter().flat_map(|m| m.iter())
}

/// The name in a signature row.
pub fn name_of(sig: &str) -> &str {
    sig.split('(').next().unwrap_or("")
}

/// How many parameters a signature row declares.
pub fn arity_of(sig: &str) -> usize {
    let inner = sig.split_once('(').map(|x| x.1).unwrap_or("");
    let inner = inner.split(')').next().unwrap_or("");
    if inner.trim().is_empty() {
        0
    } else {
        inner.matches(',').count() + 1
    }
}

/// Call the host function `name` of one of the files above, if there is one.
/// `None`: no such function.
pub(crate) fn call(it: &mut Interp, name: &str, args: &[Value]) -> Option<Res> {
    let m = MODULES.iter().position(|m| m.iter().any(|f| name_of(f.0) == name))?;
    let ctx = Ctx { mod_name: String::from(it.mod_name), data_dir: it.host.data_dir() };
    let r: Result<Value, String> = match it.host.session() {
        None => Err(format!("`{}` works only in a running game", name)),
        Some(g) => match m {
            0 => world::call(g, &ctx, name, args),
            1 => mobs::call(g, &ctx, name, args),
            2 => player::call(g, &ctx, name, args),
            3 => draw::call(g, &ctx, name, args),
            4 => bodies::call(g, &ctx, name, args),
            5 => storage::call(g, &ctx, name, args),
            6 => pack::call(g, &ctx, name, args),
            7 => files::call(g, &ctx, name, args),
            8 => net::call(g, &ctx, name, args),
            9 => effects::call(g, &ctx, name, args),
            10 => gear::call(g, &ctx, name, args),
            11 => attrs::call(g, &ctx, name, args),
            12 => mobs_model::call(g, &ctx, name, args),
            13 => modmsg::call(g, &ctx, name, args),
            14 => structures::call(g, &ctx, name, args),
            15 => screens::call(g, &ctx, name, args),
            _ => fx::call(g, &ctx, name, args),
        },
    };
    Some(match r {
        Ok(v) => Ok(v),
        Err(msg) => it.fail(msg),
    })
}
