//! Finding every key binding of every mod (scripts and native code), and
//! sending key presses to the mod whose binding it is.

use crate::client::modui::api::native_binds;
use crate::client::modui::binds::{self, Bind, Row, Switch};
use crate::client::modui::with_ui;
use crate::session::Session;
use crate::settings::Settings;
use alloc::string::String;
use alloc::vec::Vec;

/// The bindings the scripts declared.
pub fn script_binds() -> Vec<Bind> {
    crate::script::with_host(|h| h.decls.binds.clone()).unwrap_or_default()
}

/// The bindings of every switched-on native mod, then of the scripts.
pub fn all_binds() -> Vec<Bind> {
    let mut v: Vec<Bind> = Vec::new();
    for (_, n) in crate::native::active() {
        v.extend(native_binds(n.name, n.bindings));
    }
    v.extend(script_binds());
    v
}

/// Every binding with the key it has now.
pub fn rows(st: &Settings) -> Vec<Row> {
    binds::resolve(&all_binds(), st)
}

/// The switch rows mods declared, natives first.
pub fn all_switches() -> Vec<Switch> {
    let mut v: Vec<Switch> = Vec::new();
    for (_, n) in crate::native::active() {
        let mut d = binds::Decls::default();
        for s in n.switches {
            let _ = d.add_switch(n.name, s.id, s.label, s.default);
        }
        v.extend(d.switches);
    }
    v.extend(crate::script::with_host(|h| h.decls.switches.clone()).unwrap_or_default());
    v
}

/// Tell one mod that its binding `id` went down or up.
pub fn fire(g: &mut Session, mod_name: &str, id: &str, down: bool) {
    for (_, n) in crate::native::active().filter(|(_, n)| crate::client::modui::limits::clean_mod(n.name) == mod_name) {
        if let Some(f) = n.on_key {
            f(g, id, down);
        }
    }
    crate::script::bridge::key(g, mod_name, id, down);
}

/// A key went down (`down`) or up. Every binding on that key whose state
/// changes is told to its mod. Pass only presses made while playing; releases
/// of a held binding are always passed on.
pub fn press(g: &mut Session, st: &Settings, scancode: i32, down: bool) {
    // Nothing is bound and nothing is held: leave at once (the usual case).
    let any_held = with_ui(|u| !u.held.is_empty()).unwrap_or(false);
    if !down && !any_held {
        return;
    }
    let rows = rows(st);
    for (m, id) in binds::on_key(&rows, scancode) {
        let changed = with_ui(|u| u.set_held(&m, &id, down)).unwrap_or(false);
        if changed {
            fire(g, &m, &id, down);
        }
    }
}

/// Every held binding is let go (the window lost the keyboard, the game closed).
pub fn release_all(g: &mut Session) {
    let held: Vec<(String, String)> = with_ui(|u| core::mem::take(&mut u.held)).unwrap_or_default();
    for (m, id) in held {
        fire(g, &m, &id, false);
    }
}

/// Whether a mod's switch is on: what the player set on the mod switches
/// page, else the default the mod declared.
pub fn switch_on(g: &Session, mod_name: &str, id: &str) -> bool {
    let name = binds::switch_flag(mod_name, id);
    if let Some(f) = g.flags.iter().find(|f| f.0 == name) {
        return f.1;
    }
    let clean = crate::client::modui::limits::clean_mod(mod_name);
    for (_, n) in crate::native::active().filter(|(_, n)| crate::client::modui::limits::clean_mod(n.name) == clean) {
        if let Some(s) = n.switches.iter().find(|s| s.id == id) {
            return s.default;
        }
    }
    with_ui(|u| u.switch_defaults.iter().find(|d| d.0 == name).map(|d| d.1)).flatten().unwrap_or(false)
}
