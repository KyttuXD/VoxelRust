//! Storage a mod keeps: a chest-sized inventory under a name (a vehicle's
//! hold, a machine's bin). It opens on the chest screen, and the mod saves
//! it as text (`storage_dump`) in its own saved state. The names are the
//! mod's own: two mods can use the same one.

use super::args::{text, want};
use super::data::{data, Storage, MAX_STORAGES};
use super::Ctx;
use crate::inventory::Inventory;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("storage_open(key: &str) -> bool", "open this mod's storage of that name on the chest screen (made empty if it is new); false if there are too many storages (128 over all mods) or the key is empty or longer than 64 bytes"),
    ("storage_dump(key: &str) -> String", "what a storage holds, as text to keep with `state_set` (empty if it holds nothing or there is none)"),
    ("storage_load(key: &str, text: &str)", "fill a storage from text made by `storage_dump` (made if it is new)"),
    ("storage_count(key: &str) -> i64", "how many items a storage holds"),
    ("storage_to_player(key: &str) -> i64", "move everything a storage holds into the player's pack and give how many items did not fit (they stay in the storage)"),
    ("storage_drop(key: &str)", "forget a storage and what it holds"),
];

/// The mod's key made unique among the mods.
fn full(ctx: &Ctx, key: &str) -> Result<String, String> {
    if key.is_empty() || key.len() > 64 {
        return Err(String::from("a storage key is 1 to 64 bytes"));
    }
    Ok(format!("{}\u{1f}{}", ctx.mod_name, key))
}

fn find(g: &mut Session, full_key: &str) -> Option<usize> {
    data(g).storages.iter().position(|s| s.key == full_key)
}

/// A storage by key, made if there is none yet. `None` when there is no room for another.
fn find_or_make(g: &mut Session, full_key: &str) -> Option<usize> {
    if let Some(i) = find(g, full_key) {
        return Some(i);
    }
    let d = data(g);
    if let Some(i) = d.storages.iter().position(|s| s.key.is_empty()) {
        d.storages[i] = Storage { key: String::from(full_key), inv: Inventory::new() };
        return Some(i);
    }
    if d.storages.len() >= MAX_STORAGES {
        return None;
    }
    d.storages.push(Storage { key: String::from(full_key), inv: Inventory::new() });
    Some(d.storages.len() - 1)
}

/// The storage the chest screen was asked to show (`Session::opened_ext` names it).
pub fn pointer(g: &mut Session, which: usize) -> Option<*mut Inventory> {
    data(g).storages.get_mut(which).filter(|s| !s.key.is_empty()).map(|s| &mut s.inv as *mut Inventory)
}

pub fn call(g: &mut Session, ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "storage_open" => {
            want(name, args, 1)?;
            let Ok(key) = full(ctx, text(name, args, 0)?) else { return Ok(Value::Bool(false)) };
            match find_or_make(g, &key) {
                Some(i) => {
                    g.opened_ext = Some((super::STORAGE_KEY, i));
                    Ok(Value::Bool(true))
                }
                None => Ok(Value::Bool(false)),
            }
        }
        "storage_dump" => {
            want(name, args, 1)?;
            let key = full(ctx, text(name, args, 0)?)?;
            let dump = match find(g, &key) {
                Some(i) => {
                    let inv = &data(g).storages[i].inv;
                    if inv.slots.iter().all(|s| s.is_none()) {
                        String::new()
                    } else {
                        inv.save()
                    }
                }
                None => String::new(),
            };
            Ok(Value::string(dump))
        }
        "storage_load" => {
            want(name, args, 2)?;
            let key = full(ctx, text(name, args, 0)?)?;
            let body = String::from(text(name, args, 1)?);
            if let Some(i) = find_or_make(g, &key) {
                data(g).storages[i].inv = Inventory::load(&body);
            }
            Ok(Value::Unit)
        }
        "storage_count" => {
            want(name, args, 1)?;
            let key = full(ctx, text(name, args, 0)?)?;
            let n = find(g, &key).map_or(0, |i| data(g).storages[i].inv.slots.iter().flatten().map(|s| s.count as i64).sum());
            Ok(Value::Int(n))
        }
        "storage_to_player" => {
            want(name, args, 1)?;
            let key = full(ctx, text(name, args, 0)?)?;
            let Some(i) = find(g, &key) else { return Ok(Value::Int(0)) };
            let mut left = 0i64;
            for s in 0..data(g).storages[i].inv.slots.len() {
                let Some(stack) = data(g).storages[i].inv.slots[s] else { continue };
                let rest = g.inv.add(stack.item, stack.count);
                data(g).storages[i].inv.slots[s] = if rest == 0 { None } else { Some(crate::inventory::Stack { item: stack.item, count: rest }) };
                left += rest as i64;
            }
            Ok(Value::Int(left))
        }
        "storage_drop" => {
            want(name, args, 1)?;
            let key = full(ctx, text(name, args, 0)?)?;
            if let Some(i) = find(g, &key) {
                let s = &mut data(g).storages[i];
                s.key = String::new();
                s.inv = Inventory::new();
            }
            Ok(Value::Unit)
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
