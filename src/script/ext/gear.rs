//! Armour, durability, item data and item tags: what a mod needs to make
//! gear. Slots are numbered as `game/gear/session_gear.rs` says: 0..35 the
//! pack, -1 the item in hand, 100..126 the open chest's slots, 200..203 the
//! worn pieces.

use super::args::{int, text, want};
use super::Ctx;
use crate::block::reg;
use crate::game::gear::{armor, item_data, session_gear, wear};
use crate::inventory::Stack;
use crate::script::value::Value;
use crate::session::{GameMode, Session};
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("get_armor(slot: &str) -> String", "the id of the piece worn in `head`, `chest`, `legs` or `feet` (empty if none)"),
    ("set_armor(slot: &str, item: &str, count: i64) -> bool", "wear an item in a slot (a count of 0 or an empty id takes the piece off); what was worn goes to the pack or the ground; false if the item is not armour for that slot (`armor <slot>` in its data) or the slot is wrong"),
    ("armor_defense() -> f64", "the defense points the player has: worn pieces and the `armor` attribute (each point takes 4 percent off a creature's blow, a shot or a blast; at most 80 percent)"),
    ("armor_toughness() -> f64", "the toughness of what is worn (it helps against big blows)"),
    ("item_damage(slot: i64, amount: i64) -> bool", "wear the item in a slot by `amount` uses (0 to 60000); true if it broke (it is gone then); nothing wears in creative or for an item with no `durability`"),
    ("item_wear(slot: i64) -> i64", "uses spent of the item in a slot (0 when new or when it does not wear; -1 if the slot is empty or does not exist)"),
    ("item_durability(slot: i64) -> i64", "the uses the item in a slot has when new (0 if it does not wear or the slot is empty)"),
    ("repair_item(slot: i64, amount: i64) -> bool", "give back up to `amount` uses to the item in a slot; false if it was not worn"),
    ("item_get(slot: i64, key: &str) -> String", "a value kept on the item in a slot (empty if it has none under that key)"),
    ("item_set(slot: i64, key: &str, value: &str) -> bool", "keep a value on the item in a slot (at most 8 keys of 1 to 16 letters, digits, `_`, `-` or `.`; a value is at most 64 bytes); stacks with different data do not merge; false if the slot is empty or it is refused"),
    ("item_clear(slot: i64, key: &str) -> bool", "take a key off the item in a slot; false if it was not there"),
    ("item_keys(slot: i64) -> Vec<String>", "the keys kept on the item in a slot"),
    ("has_tag(item: &str, tag: &str) -> bool", "whether an item has the `tag <name>` mark in its data"),
    ("item_tag_value(item: &str, tag: &str) -> f64", "the number of an item's tag (1 if it was given none; 0 if the item lacks the tag)"),
];

fn slot_arg(name: &str, args: &[Value], i: usize) -> Result<i64, String> {
    int(name, args, i)
}

fn item_number(id: &str) -> Option<u16> {
    reg().items.iter().position(|it| it.id == id && it.brush == 0).filter(|&i| i <= u16::MAX as usize).map(|i| i as u16)
}

pub fn call(g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "get_armor" => {
            want(name, args, 1)?;
            let slot = armor::slot_named(text(name, args, 0)?).ok_or_else(|| String::from("`get_armor`: the slot is head, chest, legs or feet"))?;
            Ok(Value::string(g.inv.worn[slot].and_then(|s| reg().items.get(s.item as usize)).map(|i| i.id.clone()).unwrap_or_default()))
        }
        "set_armor" => {
            want(name, args, 3)?;
            let slot = armor::slot_named(text(name, args, 0)?).ok_or_else(|| String::from("`set_armor`: the slot is head, chest, legs or feet"))?;
            let id = text(name, args, 1)?;
            let count = int(name, args, 2)?;
            if id.is_empty() || count <= 0 {
                return Ok(Value::Bool(g.set_worn(slot, None)));
            }
            Ok(Value::Bool(match item_number(id) {
                Some(item) => g.set_worn(slot, Some(Stack::new(item, 1))),
                None => false,
            }))
        }
        "armor_defense" => {
            want(name, args, 0)?;
            Ok(Value::Float(g.armor_points().0 as f64))
        }
        "armor_toughness" => {
            want(name, args, 0)?;
            Ok(Value::Float(g.armor_points().1 as f64))
        }
        "item_damage" => {
            want(name, args, 2)?;
            let n = slot_arg(name, args, 0)?;
            let amount = int(name, args, 1)?.clamp(0, 60_000) as u32;
            if g.mode == GameMode::Creative {
                return Ok(Value::Bool(false));
            }
            let broke_slot = n;
            let Some(slot) = g.slot_mut(n) else { return Ok(Value::Bool(false)) };
            let broke = wear::wear_slot(slot, amount) == wear::Outcome::Broke;
            if broke && (session_gear::WORN..session_gear::WORN + 4).contains(&broke_slot) {
                crate::script::gear_bridge::armor_changed(g, (broke_slot - session_gear::WORN) as usize);
            }
            Ok(Value::Bool(broke))
        }
        "item_wear" => {
            want(name, args, 1)?;
            let n = slot_arg(name, args, 0)?;
            Ok(Value::Int(match g.slot_mut(n).and_then(|s| *s) {
                Some(s) if wear::durability_of(s.item) > 0 => s.wear as i64,
                Some(_) => 0,
                None => -1,
            }))
        }
        "item_durability" => {
            want(name, args, 1)?;
            let n = slot_arg(name, args, 0)?;
            Ok(Value::Int(g.slot_mut(n).and_then(|s| *s).map_or(0, |s| wear::durability_of(s.item) as i64)))
        }
        "repair_item" => {
            want(name, args, 2)?;
            let n = slot_arg(name, args, 0)?;
            let amount = int(name, args, 1)?.clamp(0, 60_000) as u32;
            Ok(Value::Bool(match g.slot_mut(n) {
                Some(slot) => wear::repair(slot, amount),
                None => false,
            }))
        }
        "item_get" => {
            want(name, args, 2)?;
            let n = slot_arg(name, args, 0)?;
            let key = text(name, args, 1)?;
            Ok(Value::string(g.slot_mut(n).and_then(|s| *s).and_then(|s| item_data::get(s.data, key)).unwrap_or_default()))
        }
        "item_set" => {
            want(name, args, 3)?;
            let n = slot_arg(name, args, 0)?;
            let (key, value) = (text(name, args, 1)?, text(name, args, 2)?);
            // A bad key or value is a mistake in the script: it is told so.
            item_data::check_key(key).map_err(|e| format!("`item_set`: {}", e))?;
            item_data::check_value(value).map_err(|e| format!("`item_set`: {}", e))?;
            let Some(Some(s)) = g.slot_mut(n) else { return Ok(Value::Bool(false)) };
            match item_data::with_key(s.data, key, value) {
                Ok(h) => {
                    s.data = h;
                    Ok(Value::Bool(true))
                }
                // Too many keys, or the table of maps is full: the item is left as it was.
                Err(_) => Ok(Value::Bool(false)),
            }
        }
        "item_clear" => {
            want(name, args, 2)?;
            let n = slot_arg(name, args, 0)?;
            let key = text(name, args, 1)?;
            let Some(Some(s)) = g.slot_mut(n) else { return Ok(Value::Bool(false)) };
            if item_data::get(s.data, key).is_none() {
                return Ok(Value::Bool(false));
            }
            s.data = item_data::without_key(s.data, key);
            Ok(Value::Bool(true))
        }
        "item_keys" => {
            want(name, args, 1)?;
            let n = slot_arg(name, args, 0)?;
            let keys = g.slot_mut(n).and_then(|s| *s).map(|s| item_data::map_of(s.data).0).unwrap_or_default();
            Ok(Value::vec(keys.iter().map(|k| Value::str(&k.0)).collect()))
        }
        "has_tag" => {
            want(name, args, 2)?;
            let (id, tag) = (text(name, args, 0)?, text(name, args, 1)?.to_ascii_lowercase());
            Ok(Value::Bool(item_number(id).map_or(false, |i| reg().items[i as usize].x.tags.iter().any(|t| t.0 == tag))))
        }
        "item_tag_value" => {
            want(name, args, 2)?;
            let (id, tag) = (text(name, args, 0)?, text(name, args, 1)?.to_ascii_lowercase());
            Ok(Value::Float(item_number(id).and_then(|i| reg().items[i as usize].x.tags.iter().find(|t| t.0 == tag).map(|t| t.1 as f64)).unwrap_or(0.0)))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
