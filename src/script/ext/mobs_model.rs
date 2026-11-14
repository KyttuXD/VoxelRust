//! Big creatures and their looks: a creature's own size and health, per-bone
//! model overrides, a tilt of the whole model, and a short state text that
//! shared games show on every machine. All of them go through
//! `game/creature_api.rs`, which native mods use too, so the limits are one set:
//! size up to 32 wide and 64 tall, health up to 1,000,000, 64 bone overrides a
//! creature and 256 creatures with them, a state of 64 bytes.
//!
//! Size, health and state are the host's say (refused for a player who does not
//! run the creatures in a shared game, as the other `mob_` calls are). Bone
//! overrides and the tilt only change how a creature is drawn on this machine
//! (a host's snapshot, if it has bones for that creature, replaces them).

use super::args::{finite, int, text, want};
use super::Ctx;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("set_creature_size(id: i64, width: f64, height: f64) -> bool", "give a creature its own size in blocks (width 0.1 to 32, height 0.1 to 64; it collides and is drawn at it, steps up ledges in proportion to its width, stays alive and drawn from further off the bigger it is); false if there is no such creature or this player does not run the creatures"),
    ("set_creature_health(id: i64, hp: i64, max_hp: i64) -> bool", "set a creature's health and its limit (the limit 1 to 1,000,000, the health 0 to the limit: 0 kills it); the creature's own regrowth never goes over the limit; false if there is no such creature or this player does not run the creatures"),
    ("set_bone(id: i64, bone: &str, rx: f64, ry: f64, rz: f64, ox: f64, oy: f64, oz: f64, scale: f64) -> bool", "on top of the creature's animation, turn the model bone called `bone` by (rx, ry, rz) degrees about its pivot, move it by (ox, oy, oz) model units and scale it (0.01 to 100, 1 is as it is); its child bones follow. Call it every frame to move a bone. false for an unknown bone name (logged once), no such creature, 64 bones or 256 creatures already, or too many calls in a frame (4096); it is seen on this machine, and in a shared game on every machine if this player runs the creatures"),
    ("clear_bones(id: i64) -> bool", "take all of a creature's bone overrides away; false if there is no such creature"),
    ("set_creature_orientation(id: i64, pitch: f64, roll: f64) -> bool", "tilt the whole model of a creature about its centre, in degrees: pitch up lifts the nose, roll leans the top to the creature's left (-3000 to 3000; 0, 0 puts it upright); false if there is no such creature"),
    ("model_bones(model_id: &str) -> Vec<String>", "the names of the bones of a model (a model id, or a creature kind to use its model), in the order of the model file; empty if there is no such model"),
    ("set_creature_state(id: i64, text: &str) -> bool", "keep a short text (64 bytes at most, no control characters) on a creature for the mod; a shared game sends it to every player, who read it with `creature_state`; false if there is no such creature or this player does not run the creatures"),
    ("creature_state(id: i64) -> String", "the text `set_creature_state` left on a creature (empty if none, or no such creature)"),
];

/// Longest bone name a script may ask for.
const MAX_BONE_NAME: usize = 64;

fn creature(g: &Session, name: &str, args: &[Value]) -> Result<Option<u32>, String> {
    let id = int(name, args, 0)?;
    Ok(if id < 0 || id > u32::MAX as i64 { None } else { g.mobs.index_of(id as u32).map(|_| id as u32) })
}

/// Whether this player decides about the creatures (a game that is not shared, or the one that runs them).
fn runs(g: &Session) -> bool {
    g.net.is_none() || g.runs_the_world()
}

pub fn call(g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "set_creature_size" => {
            want(name, args, 3)?;
            let (w, h) = (finite(name, args, 1)?, finite(name, args, 2)?);
            Ok(Value::Bool(match creature(g, name, args)? {
                Some(id) if runs(g) => g.mobs.set_size(id, w, h),
                _ => false,
            }))
        }
        "set_creature_health" => {
            want(name, args, 3)?;
            let (hp, max) = (int(name, args, 1)?, int(name, args, 2)?);
            Ok(Value::Bool(match creature(g, name, args)? {
                Some(id) if runs(g) => g.mobs.set_health(id, hp, max),
                _ => false,
            }))
        }
        "set_bone" => {
            want(name, args, 9)?;
            let bone = text(name, args, 1)?;
            let mut v = [0.0f64; 7];
            for (k, slot) in v.iter_mut().enumerate() {
                *slot = finite(name, args, k + 2)?;
            }
            let Some(id) = creature(g, name, args)? else { return Ok(Value::Bool(false)) };
            if bone.len() > MAX_BONE_NAME || super::data(g).bone_calls >= super::data::MAX_BONE_CALLS {
                return Ok(Value::Bool(false));
            }
            super::data(g).bone_calls += 1;
            let done = g.mobs.set_bone(id, bone, [v[0], v[1], v[2]], [v[3], v[4], v[5]], v[6]);
            Ok(Value::Bool(done == crate::creature_api::BoneResult::Set))
        }
        "clear_bones" => {
            want(name, args, 1)?;
            Ok(Value::Bool(match creature(g, name, args)? {
                Some(id) => g.mobs.clear_bones(id),
                None => false,
            }))
        }
        "set_creature_orientation" => {
            want(name, args, 3)?;
            let (pitch, roll) = (finite(name, args, 1)?, finite(name, args, 2)?);
            Ok(Value::Bool(match creature(g, name, args)? {
                Some(id) => g.mobs.set_orientation(id, pitch, roll),
                None => false,
            }))
        }
        "model_bones" => {
            want(name, args, 1)?;
            let id = text(name, args, 0)?;
            let names: Vec<Value> = crate::creature_api::model_bone_names(&mut g.mobs, id).unwrap_or_default().into_iter().map(Value::string).collect();
            Ok(Value::vec(names))
        }
        "set_creature_state" => {
            want(name, args, 2)?;
            let t = text(name, args, 1)?;
            Ok(Value::Bool(match creature(g, name, args)? {
                Some(id) if runs(g) => g.mobs.set_state(id, t),
                _ => false,
            }))
        }
        "creature_state" => {
            want(name, args, 1)?;
            Ok(Value::string(match creature(g, name, args)? {
                Some(id) => String::from(g.mobs.state_of(id)),
                None => String::new(),
            }))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
