//! The player's attributes (most health, walking speed, jump height, armour
//! points, damage dealt) and questions about status effects that a mod needs
//! to keep its own timers out of the way: how long is left, which are active.

use super::args::{finite, text, want};
use super::Ctx;
use crate::game::effects::script_api;
use crate::game::gear::attributes as attr;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("set_attribute(name: &str, value: f64) -> bool", "set the base of an attribute of the player: `max_health` (2 to 100, default 20), `walk_speed` (0.1 to 4, a multiplier, default 1), `jump_height` (0 to 4, a multiplier, default 1), `armor` (0 to 40 defense points, default 0) or `damage` (0 to 10, the multiplier of the player's blows, default 1); kept with the world; false if the name is wrong"),
    ("reset_attribute(name: &str) -> bool", "put an attribute's base back to its default (the modifiers stay)"),
    ("get_attribute(name: &str) -> f64", "an attribute's value now: base and modifiers together, held to its range"),
    ("add_attribute_modifier(id: &str, name: &str, amount: f64, mode: &str) -> bool", "add a modifier to an attribute under an id of the mod's own (1 to 32 letters, digits, `_`, `-`, `.`); `mode` is `add` or `multiply`; the value is (base + all `add`) x (1 + all `multiply`); the same id again replaces it; at most 32 modifiers; false when refused"),
    ("remove_attribute_modifier(id: &str) -> bool", "take a modifier away; false if there was none of that id"),
    ("player_max_health() -> i64", "the most health the player has now (20 unless an attribute changed it)"),
    ("effect_secs(who: &str, id: &str) -> f64", "seconds left of a status effect on \"player\" or \"target\" (0 if it is not active)"),
    ("list_effects(who: &str) -> Vec<String>", "the ids of the status effects active on \"player\" or \"target\""),
    ("effect_defined(id: &str) -> bool", "whether a status effect of that id exists (built in, or an `effect` of the data)"),
];

fn attr_named(name: &str, arg: &str) -> Result<usize, String> {
    attr::index(arg).ok_or_else(|| format!("`{}`: the attributes are max_health, walk_speed, jump_height, armor and damage, not `{}`", name, arg))
}

/// After a change of the most health, health above it is brought down.
fn settle(g: &mut Session) {
    let max = g.max_health();
    if g.health > max {
        g.health = max;
    }
}

pub fn call(g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "set_attribute" => {
            want(name, args, 2)?;
            let a = attr_named(name, text(name, args, 0)?)?;
            let ok = g.attrs.set_base(a, finite(name, args, 1)?);
            settle(g);
            Ok(Value::Bool(ok))
        }
        "reset_attribute" => {
            want(name, args, 1)?;
            let a = attr_named(name, text(name, args, 0)?)?;
            g.attrs.reset_base(a);
            settle(g);
            Ok(Value::Bool(true))
        }
        "get_attribute" => {
            want(name, args, 1)?;
            let a = attr_named(name, text(name, args, 0)?)?;
            Ok(Value::Float(g.attrs.value(a) as f64))
        }
        "add_attribute_modifier" => {
            want(name, args, 4)?;
            let id = text(name, args, 0)?;
            let a = attr_named(name, text(name, args, 1)?)?;
            let amount = finite(name, args, 2)?;
            let multiply = match text(name, args, 3)? {
                "add" => false,
                "multiply" => true,
                other => return Err(format!("`{}`: the mode is `add` or `multiply`, not `{}`", name, other)),
            };
            let ok = g.attrs.add_modifier(id, a, amount, multiply).is_ok();
            settle(g);
            Ok(Value::Bool(ok))
        }
        "remove_attribute_modifier" => {
            want(name, args, 1)?;
            let ok = g.attrs.remove_modifier(text(name, args, 0)?);
            settle(g);
            Ok(Value::Bool(ok))
        }
        "player_max_health" => {
            want(name, args, 0)?;
            Ok(Value::Int(g.max_health() as i64))
        }
        "effect_secs" => {
            want(name, args, 2)?;
            Ok(Value::Float(script_api::secs_left(g, text(name, args, 0)?, text(name, args, 1)?) as f64))
        }
        "list_effects" => {
            want(name, args, 1)?;
            Ok(Value::vec(script_api::active_ids(g, text(name, args, 0)?).iter().map(|s| Value::str(s)).collect()))
        }
        "effect_defined" => {
            want(name, args, 1)?;
            Ok(Value::Bool(crate::game::effects::apply::lookup(&text(name, args, 0)?.to_ascii_lowercase()).is_some()))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
