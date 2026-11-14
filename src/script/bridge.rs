//! Where the game's hooks reach the scripts.
//!
//! `game/hooks.rs` (the dispatchers the native mod code is called from) and a
//! few places in the session call the functions here after the native
//! hooks. Each one costs one flag test when no script is loaded
//! ([`ScriptHost::is_live`]). The hook names and what they get are listed in
//! [`crate::script::HOOKS`] and docs/SCRIPTING.md.

use crate::block::{self, Block};
use crate::explosion::ExplosionInfo;
use crate::script::host::SessionHost;
use crate::script::value::{clean_f32, Value};
use crate::script::{with_host, ScriptHost};
use crate::session::Session;
use alloc::string::String;
use alloc::vec::Vec;

/// Whether any script is loaded and working.
pub fn live() -> bool {
    ScriptHost::is_live()
}

pub(crate) fn run(g: &mut Session, hook: &str, args: Vec<Value>, mut on_result: impl FnMut(&Value) -> bool) {
    let world = g.name.clone();
    with_host(|h| {
        let mut host = SessionHost { g, rng: h.rng, mod_name: String::new(), data_dir: h.data_dir.clone() };
        h.call_hook(&mut host, &world, hook, &args, &mut on_result);
        h.rng = host.rng;
    });
}

/// As `run`, for one mod's scripts.
fn run_in(g: &mut Session, only: &dyn Fn(&str) -> bool, hook: &str, args: Vec<Value>, mut on_result: impl FnMut(&Value) -> bool) {
    let world = g.name.clone();
    with_host(|h| {
        let mut host = SessionHost { g, rng: h.rng, mod_name: String::new(), data_dir: h.data_dir.clone() };
        h.call_hook_in(&mut host, &world, Some(only), hook, &args, &mut on_result);
        h.rng = host.rng;
    });
}

fn is_no(v: &Value) -> bool {
    matches!(v, Value::Bool(false))
}

fn item_id(item: usize) -> String {
    block::reg().items.get(item).map(|d| d.id.clone()).unwrap_or_default()
}

fn entity_id(kind: usize) -> String {
    block::reg().entities.get(kind).map(|d| d.id.clone()).unwrap_or_default()
}

/// The world was opened: read the scripts' saved state, call `on_load`.
pub fn world_loaded(g: &mut Session) {
    if !live() {
        return;
    }
    run(g, "", Vec::new(), |_| false);
}

/// The world was saved: write the scripts' state, call `on_save`.
pub fn world_saved(g: &mut Session) {
    if !live() {
        return;
    }
    run(g, "on_save", Vec::new(), |_| false);
    let world = g.name.clone();
    with_host(|h| h.save_states(&world));
}

/// A block is about to be broken; `false` keeps it.
pub fn block_break(g: &mut Session, cell: [i32; 3], b: Block) -> bool {
    if !live() {
        return true;
    }
    let args = alloc::vec![Value::Int(cell[0] as i64), Value::Int(cell[1] as i64), Value::Int(cell[2] as i64), Value::string(block::def(b).id.clone())];
    let mut allow = true;
    run(g, "on_block_break", args, |r| {
        if is_no(r) {
            allow = false;
        }
        !allow
    });
    allow
}

/// A block is about to be placed; `false` stops it.
pub fn block_place(g: &mut Session, cell: [i32; 3], b: Block) -> bool {
    if !live() {
        return true;
    }
    let args = alloc::vec![Value::Int(cell[0] as i64), Value::Int(cell[1] as i64), Value::Int(cell[2] as i64), Value::string(block::def(b).id.clone())];
    let mut allow = true;
    run(g, "on_block_place", args, |r| {
        if is_no(r) {
            allow = false;
        }
        !allow
    });
    allow
}

/// A creature appeared; `false` takes it away.
pub fn entity_spawn(g: &mut Session, kind: usize) -> bool {
    if !live() {
        return true;
    }
    let mut allow = true;
    run(g, "on_entity_spawn", alloc::vec![Value::string(entity_id(kind))], |r| {
        if is_no(r) {
            allow = false;
        }
        !allow
    });
    allow
}

/// A creature died.
pub fn entity_death(g: &mut Session, kind: usize, at: [f64; 3]) {
    if !live() {
        return;
    }
    run(g, "on_entity_death", alloc::vec![Value::string(entity_id(kind)), Value::Float(at[0]), Value::Float(at[1]), Value::Float(at[2])], |_| false);
}

/// The player hit a creature.
pub fn entity_hit(g: &mut Session, id: u32, kind: usize, at: [f64; 3]) {
    if !live() {
        return;
    }
    run(g, "on_entity_hit", alloc::vec![Value::Int(id as i64), Value::string(entity_id(kind)), Value::Float(at[0]), Value::Float(at[1]), Value::Float(at[2])], |_| false);
}

/// The player is about to take damage. The first script to answer with a
/// number (the amount to take instead) or `false` (none) decides.
pub fn player_hurt(g: &mut Session, amount: &mut i32, cause: &str) {
    if !live() {
        return;
    }
    let mut now = *amount as i64;
    run(g, "on_player_hurt", alloc::vec![Value::Int(now), Value::str(cause)], |r| match r {
        Value::Bool(false) => {
            now = 0;
            true
        }
        Value::Int(n) => {
            now = *n;
            true
        }
        Value::Float(f) => {
            now = *f as i64;
            true
        }
        _ => false,
    });
    *amount = now.clamp(0, 1000) as i32;
}

/// The player typed a line; `false` swallows it.
pub fn chat(g: &mut Session, line: &str) -> bool {
    if !live() {
        return true;
    }
    let mut allow = true;
    run(g, "on_chat", alloc::vec![Value::str(line)], |r| {
        if is_no(r) {
            allow = false;
        }
        !allow
    });
    allow
}

/// The player used the item in hand; `false` stops it.
pub fn item_use(g: &mut Session, item: usize) -> bool {
    if !live() {
        return true;
    }
    let mut allow = true;
    run(g, "on_item_use", alloc::vec![Value::string(item_id(item))], |r| {
        if is_no(r) {
            allow = false;
        }
        !allow
    });
    allow
}

/// The player crafted something.
pub fn craft(g: &mut Session, out: usize) {
    if !live() {
        return;
    }
    run(g, "on_craft", alloc::vec![Value::string(item_id(out))], |_| false);
}

/// Once a frame.
pub fn tick(g: &mut Session, dt: f32) {
    if !live() {
        return;
    }
    run(g, "on_tick", alloc::vec![Value::Float(clean_f32(dt))], |_| false);
}

/// A script event was raised (see `Session::fire`).
pub fn event(g: &mut Session, name: &str, what: &str) {
    if !live() {
        return;
    }
    run(g, "on_event", alloc::vec![Value::str(name), Value::str(what)], |_| false);
}

/// A chat command the game does not know: a script may take it. `words`
/// are what was typed, the first with its `/`. A script that answers `true`
/// (or a string, which is shown) has handled it.
pub fn command(g: &mut Session, words: &[&str]) -> Option<String> {
    if !live() || words.is_empty() {
        return None;
    }
    let name = words[0].trim_start_matches('/').to_ascii_lowercase();
    let rest = words[1..].join(" ");
    let mut reply: Option<String> = None;
    run(g, "on_command", alloc::vec![Value::string(name), Value::string(rest)], |r| match r {
        Value::Bool(true) => {
            reply = Some(String::new());
            true
        }
        Value::Str(s) => {
            reply = Some(s.to_ascii_uppercase());
            true
        }
        _ => false,
    });
    reply
}

/// An explosion is about to happen: a script may cancel it (`false`) or
/// change its power (a number). Returns whether it goes ahead.
pub fn explode(g: &mut Session, info: &mut ExplosionInfo) -> bool {
    if !live() {
        return true;
    }
    let args = alloc::vec![Value::Float(info.at[0]), Value::Float(info.at[1]), Value::Float(info.at[2]), Value::Float(clean_f32(info.power)), Value::string(info.source.name())];
    let mut go = true;
    let mut power = info.power;
    run(g, "on_explode", args, |r| match r {
        Value::Bool(false) => {
            go = false;
            true
        }
        Value::Float(f) => {
            power = *f as f32;
            false
        }
        Value::Int(n) => {
            power = *n as f32;
            false
        }
        _ => false,
    });
    if power > 0.0 && power.is_finite() {
        info.power = power;
    } else {
        go = false;
    }
    go
}

/// Once a frame, right after the mods' native code has moved things and
/// before the player moves: what the player presses is handed over, last
/// frame's pictures are dropped, and `on_update` runs.
pub fn update(g: &mut Session, dt: f32, c: &crate::session::Controls) {
    if !live() {
        return;
    }
    crate::script::ext::new_frame(g);
    crate::script::ext::data(g).controls = *c;
    run(g, "on_update", alloc::vec![Value::Float(clean_f32(dt))], |_| false);
}

/// The player's hands: called before the game's own handling of a click.
/// `true` if a script used the click up.
pub fn interact(g: &mut Session, c: &crate::session::Controls, reach: f32) -> bool {
    if !live() {
        return false;
    }
    crate::script::ext::data(g).controls = *c;
    let mut used = false;
    run(g, "on_interact", alloc::vec![Value::Float(clean_f32(reach))], |r| {
        if matches!(r, Value::Bool(true)) {
            used = true;
        }
        used
    });
    used
}

/// A message another player's copy of a mod sent with `net_send` (the key
/// carries the mod's name before a `\x1f`).
pub fn net_message(g: &mut Session, key: &str, text: &str) {
    if !live() {
        return;
    }
    let Some((mod_name, key)) = key.split_once('\u{1f}') else { return };
    run_in(g, &|m| m == mod_name, "on_net", alloc::vec![Value::str(key), Value::str(text)], |_| false);
}

/// An `/if` condition the game does not know: a script may answer it. `words`
/// are the words of the condition (the first is its name). The first script
/// to return `true` or `false` decides; one that returns nothing does not know it.
pub fn condition(g: &mut Session, words: &[&str]) -> Option<bool> {
    if !live() || words.is_empty() {
        return None;
    }
    let name = words[0].to_ascii_lowercase();
    let rest = words[1..].join(" ");
    let mut answer: Option<bool> = None;
    run(g, "on_condition", alloc::vec![Value::string(name), Value::string(rest)], |r| match r {
        Value::Bool(b) => {
            answer = Some(*b);
            true
        }
        _ => false,
    });
    answer
}

// ---- controls, HUD and screens (see script/host_ui.rs, client/modui)

/// One of a mod's key bindings went down or up (`mod_name` is the cleaned
/// name the bindings carry).
pub fn key(g: &mut Session, mod_name: &str, id: &str, down: bool) {
    if !live() {
        return;
    }
    let hook = if down { "on_key_down" } else { "on_key_up" };
    run_in(g, &|m| crate::client::modui::limits::clean_mod(m) == mod_name, hook, alloc::vec![Value::str(id)], |_| false);
}

/// Once a frame while playing: the scripts draw their HUD.
pub fn hud(g: &mut Session, dt: f32) {
    if !live() {
        return;
    }
    run(g, "on_hud", alloc::vec![Value::Float(clean_f32(dt))], |_| false);
}

/// The player used a mod's screen: its scripts hear of it.
pub fn ui_event(g: &mut Session, ev: &crate::client::modui::screens::UiEvent) {
    use crate::client::modui::screens::{UiKind, UiValue};
    if !live() {
        return;
    }
    let (hook, args) = match &ev.kind {
        UiKind::Click => ("on_ui_click", alloc::vec![Value::str(&ev.id)]),
        UiKind::Change(v) => {
            let value = match v {
                UiValue::Bool(b) => Value::Bool(*b),
                UiValue::Int(n) => Value::Int(*n),
                UiValue::Text(t) => Value::str(t),
            };
            ("on_ui_change", alloc::vec![Value::str(&ev.id), value])
        }
        UiKind::Closed => ("on_ui_close", alloc::vec![Value::str(&ev.screen)]),
    };
    run_in(g, &|m| m == ev.mod_name, hook, args, |_| false);
}
