//! The calls the game makes into mods for their interface, once a frame
//! and after a mod's screen was used: the HUD hook, and the screen's events.

use crate::client::modui::screens::UiEvent;
use crate::client::modui::{with_ui, ModApi};
use crate::session::Session;
use alloc::vec::Vec;

/// Once a frame, while playing: the HUD's timers run, the last frame's
/// drawing commands are dropped, and every mod's HUD hook (`on_hud`) draws
/// the new frame's. Called from `game/events.rs` after the mods' tick.
pub fn frame(g: &mut Session, dt: f32) {
    if with_ui(|u| u.hud.frame(dt)).is_none() {
        return;
    }
    for (_, n) in crate::native::active() {
        if let Some(f) = n.on_hud {
            with_ui(|u| f(&*g, &mut ModApi::new(u, n.name)));
        }
    }
    crate::script::bridge::hud(g, dt);
}

/// The window loop's frame rate, for mods to read.
pub fn set_fps(fps: f32) {
    with_ui(|u| u.fps = fps);
}

/// A mod opened a screen that the game has not switched to yet (true once).
pub fn take_opened() -> bool {
    with_ui(|u| core::mem::take(&mut u.screens.opened_now)).unwrap_or(false)
}

/// Whether a mod's screen is open.
pub fn is_open() -> bool {
    with_ui(|u| u.screens.open.is_some()).unwrap_or(false)
}

/// The player closed the mod screen: its mod hears of it.
pub fn closed_by_player() {
    with_ui(|u| u.screens.close());
}

/// Hand what the player did on a mod's screen to the mods. Called after the
/// screen was drawn, outside of any use of the shared state.
pub fn ui_events(g: &mut Session) {
    let events: Vec<UiEvent> = with_ui(|u| u.screens.take_events()).unwrap_or_default();
    for ev in events {
        for (_, n) in crate::native::active().filter(|(_, n)| n.name == ev.mod_name) {
            if let Some(f) = n.on_ui {
                with_ui(|u| f(g, &mut ModApi::new(u, n.name), &ev));
            }
        }
        crate::script::bridge::ui_event(g, &ev);
    }
}

/// A mod was switched off or reloaded: whatever it put on the interface goes.
pub fn forget(mod_name: &str) {
    with_ui(|u| u.forget_mod(mod_name));
}

/// The scripts' switches changed (a mod loaded or went): keep their defaults
/// where `switch_on` can find them.
pub fn publish_defaults(switches: &[crate::client::modui::binds::Switch]) {
    with_ui(|u| u.switch_defaults = switches.iter().map(|s| (crate::client::modui::binds::switch_flag(&s.mod_name, &s.id), s.default)).collect());
}
