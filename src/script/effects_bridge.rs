//! Where the status effects reach the scripts: `on_effect_start`,
//! `on_effect_tick` and `on_effect_end` (see `game/effects/apply.rs`, which
//! calls these after the native hooks). Each costs one flag test when no
//! script is loaded.

use crate::script::bridge::{live, run};
use crate::script::value::{clean_f32, Value};
use crate::session::Session;
use alloc::vec::Vec;

/// An effect started on the player.
pub fn start(g: &mut Session, id: &str, level: i32) {
    if !live() {
        return;
    }
    run(g, "on_effect_start", alloc::vec![Value::str(id), Value::Int(level as i64)], |_| false);
}

/// About once a second for each effect that is running.
pub fn tick(g: &mut Session, id: &str, secs_left: f32, level: i32) {
    if !live() {
        return;
    }
    run(g, "on_effect_tick", alloc::vec![Value::str(id), Value::Float(clean_f32(secs_left)), Value::Int(level as i64)], |_| false);
}

/// An effect ended (ran out, was put out or was cleared).
pub fn end(g: &mut Session, id: &str) {
    if !live() {
        return;
    }
    let args: Vec<Value> = alloc::vec![Value::str(id)];
    run(g, "on_effect_end", args, |_| false);
}
