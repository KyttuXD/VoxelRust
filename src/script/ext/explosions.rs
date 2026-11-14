//! Explosions seen from a mod: changing the blast in `before_explode`, and
//! blocks that blasts never destroy. The hooks themselves (`before_explode`,
//! `explode_block`, `after_explode`) are in `script/bridge.rs`; the rules
//! and limits are in `game/explosion_hooks/`.
//!
//! `set_immune_blocks` is called once, from `on_load` (it is not allowed in
//! `on_register`, which runs before any world exists); the list is kept per
//! mod for as long as the world is open, and a mod's second call replaces its
//! first.

use super::args::{text, want};
use super::Ctx;
use crate::game::explosion_hooks::{self, immune};
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("set_explosion_radius(radius: f64) -> bool", "inside `before_explode` only: change the radius of the blast about to happen (held to 0 to 16 blocks; 0 cancels it); false outside the hook, where it does nothing"),
    ("set_immune_blocks(blocks: Vec<String>) -> i64", "declare the blocks (ids, or block numbers) that no explosion destroys, whatever its radius or cause, without a call for each block: call it once from `on_load`; it replaces this mod's earlier list; at most 4096 entries (the rest are left out); a name that is no block is logged once and ignored; returns how many entries were taken"),
    ("clear_immune_blocks()", "drop this mod's immune list"),
    ("is_blast_immune(block_id: &str) -> bool", "whether explosions leave this block alone: the game's own rule (water and lava, unbreakable blocks, `tag blastproof`, obsidian and bedrock) or any mod's immune list; for a mod that clears blocks itself and wants to agree with the engine"),
];

/// The entries of a list argument: block ids as text, or block numbers.
fn specs(name: &str, args: &[Value], i: usize) -> Result<Vec<immune::Spec>, String> {
    let list = match args.get(i) {
        Some(Value::Vec(l)) | Some(Value::Tuple(l)) => l,
        Some(other) => return Err(format!("`{}`: argument {} should be a list of block ids, found {}", name, i + 1, other.type_name())),
        None => return Err(format!("`{}` is missing argument {}", name, i + 1)),
    };
    let mut out = Vec::new();
    // Entries past the cap are not even read (the registry would drop them).
    for v in list.iter().take(immune::MAX_ENTRIES + 1) {
        out.push(match v {
            Value::Str(s) => immune::Spec::Name(String::from(s.as_str())),
            Value::Int(n) => immune::Spec::Number(*n),
            other => return Err(format!("`{}`: the list should hold block ids (text) or block numbers, found {}", name, other.type_name())),
        });
    }
    Ok(out)
}

pub fn call(g: &mut Session, ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    let owner = if ctx.mod_name.is_empty() { "script" } else { ctx.mod_name.as_str() };
    match name {
        "set_explosion_radius" => {
            want(name, args, 1)?;
            let r = super::args::num(name, args, 0)?;
            if r.is_nan() {
                return Err(format!("`{}`: the radius is not a number", name));
            }
            Ok(Value::Bool(explosion_hooks::state(g).set_radius(r)))
        }
        "set_immune_blocks" => {
            want(name, args, 1)?;
            let list = specs(name, args, 0)?;
            let blocks = crate::block::reg().blocks.len();
            let out = explosion_hooks::state(g).immune.set(owner, &list, blocks, &explosion_hooks::resolve);
            for n in &out.unknown {
                crate::sys::log(&format!("voxelrust: {}: set_immune_blocks: there is no block `{}`, left out", owner, n));
            }
            if out.truncated {
                crate::sys::log(&format!("voxelrust: {}: set_immune_blocks: more than {} entries, the rest are left out", owner, immune::MAX_ENTRIES));
            }
            Ok(Value::Int(out.accepted as i64))
        }
        "clear_immune_blocks" => {
            want(name, args, 0)?;
            explosion_hooks::state(g).immune.clear(owner);
            Ok(Value::Unit)
        }
        "is_blast_immune" => {
            want(name, args, 1)?;
            let id = text(name, args, 0)?;
            Ok(Value::Bool(match crate::block::reg().block(id) {
                Some(b) => crate::explosion::resists(b) || explosion_hooks::is_immune(g, b),
                None => false,
            }))
        }
        other => Err(format!("`{}` is not an explosion function", other)),
    }
}
