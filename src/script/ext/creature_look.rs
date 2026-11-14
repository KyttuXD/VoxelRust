//! Where a creature's head looks: `set_creature_look` and `clear_creature_look`.
//! By default a creature with a `head` bone looks at the nearest player within
//! 12 blocks (see `game/creature_look.rs`); these take the head over, or give it back.
//! A shared game shows a forced look on every machine; a player who does not run
//! the creatures cannot force one (the host's wins).

use super::args::{finite, int, want};
use super::Ctx;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("set_creature_look(id: i64, yaw: f64, pitch: f64) -> bool", "make the head of a creature (the bone named `head`) look this way, in degrees relative to its body: yaw positive to its left (-90 to 90), pitch positive up (-60 to 60); the head turns smoothly and stays so until `clear_creature_look`; shown on every machine in a shared game; false if there is no such creature, a number is not finite, or this player does not run the creatures"),
    ("clear_creature_look(id: i64) -> bool", "give a creature's head back to the game, which turns it toward the nearest player within 12 blocks (48 for a creature with the trait `watches`); false if there is no such creature or this player does not run the creatures"),
];

pub fn call(g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    let runs = g.net.is_none() || g.runs_the_world();
    match name {
        "set_creature_look" => {
            want(name, args, 3)?;
            let id = int(name, args, 0)?;
            let (yaw, pitch) = (finite(name, args, 1)?, finite(name, args, 2)?);
            Ok(Value::Bool(runs && id >= 0 && id <= u32::MAX as i64 && g.mobs.set_look(id as u32, yaw, pitch)))
        }
        "clear_creature_look" => {
            want(name, args, 1)?;
            let id = int(name, args, 0)?;
            Ok(Value::Bool(runs && id >= 0 && id <= u32::MAX as i64 && g.mobs.clear_look(id as u32)))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
