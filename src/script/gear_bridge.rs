//! Where armour reaches the scripts: `on_armor_change(slot)` (see
//! `game/gear/session_gear.rs`, which calls this when a piece is put on,
//! taken off, set by a script or broken). Costs one flag test when no script
//! is loaded.

use crate::script::bridge::{live, run};
use crate::script::value::Value;
use crate::session::Session;

/// The worn piece in `slot` (0 head, 1 chest, 2 legs, 3 feet) changed.
pub fn armor_changed(g: &mut Session, slot: usize) {
    g.natives(|n, g| {
        if let Some(f) = n.on_armor_change {
            f(g, slot);
        }
    });
    if !live() {
        return;
    }
    run(g, "on_armor_change", alloc::vec![Value::Int(slot as i64)], |_| false);
}
