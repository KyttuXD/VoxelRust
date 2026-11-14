//! The interface hooks of mods: key bindings, HUD drawing, their own
//! screens and switches. Scripts (`script/host_ui.rs`) and native code
//! (`native::Native`: `bindings`, `switches`, `on_key`, `on_hud`, `on_ui`)
//! use the same [`ModApi`], so both have the same calls and the same limits.
//!
//! | file | what |
//! | --- | --- |
//! | `limits.rs` | every cap, and cleaning of text and ids |
//! | `binds.rs` | key bindings and switches a mod declares; keys, conflicts, list lines |
//! | `scroll.rs` | the arithmetic of scrolling lists |
//! | `hudcmd.rs` | the per-frame drawing commands, the cap, placing and clipping |
//! | `hud_image.rs` | a mod's pictures on the HUD: the cache and its limits, anchoring, `cover` and `contain` |
//! | `hud.rs` | HUD slots, toasts, the action bar, and their layout |
//! | `screens.rs` | a mod's own screen: widgets and the events they make |
//! | `info.rs` | what a mod may read about the player |
//! | `api.rs` | [`ModApi`]: the calls a mod makes |
//! | `keys.rs`, `dispatch.rs` | key presses and per-frame calls reaching the mods |
//! | `hud_draw.rs`, `screen_draw.rs`, `bind_page.rs`, `switch_page.rs` | painting (renderer and `Ui`) |
//!
//! The state is one [`ModUi`], shared by the window loop and the mods' calls
//! (a single thread uses it; see [`with_ui`]). A mod can only add to it through
//! [`ModApi`], which checks and caps everything.

pub mod api;
pub mod bind_page;
pub mod binds;
pub mod dispatch;
pub mod hud;
pub mod hud_draw;
pub mod hud_image;
pub mod hudcmd;
pub mod info;
pub mod keys;
pub mod limits;
pub mod screen_draw;
pub mod screens;
pub mod scroll;
pub mod switch_page;

pub use api::ModApi;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

/// Everything the mods keep on the interface.
#[derive(Debug, Default)]
pub struct ModUi {
    pub hud: hud::Hud,
    /// The pictures mods draw on the HUD (read when first named, freed with their mod).
    pub images: hud_image::Images,
    pub screens: screens::Screens,
    /// What a script declares while its `on_register` runs.
    pub decls: binds::Decls,
    /// Declarations are being collected (`on_register` is running).
    pub registering: bool,
    /// Key bindings whose key is down now, as (mod, id).
    pub held: Vec<(String, String)>,
    /// Frames a second, as counted by the window loop.
    pub fps: f32,
    /// First line shown on the key bindings page, and on the mod switches page.
    pub scroll_binds: i32,
    pub scroll_switches: i32,
    /// The default of each switch the scripts declared, by flag name (`mod.id`).
    pub switch_defaults: Vec<(String, bool)>,
}

impl ModUi {
    pub fn is_held(&self, mod_name: &str, id: &str) -> bool {
        self.held.iter().any(|h| h.0 == mod_name && h.1 == id)
    }

    /// Note a binding's key going down or up. Returns whether that is a change.
    pub fn set_held(&mut self, mod_name: &str, id: &str, down: bool) -> bool {
        let was = self.is_held(mod_name, id);
        if down == was {
            return false;
        }
        if down {
            if self.held.len() < limits::MAX_BINDS {
                self.held.push((String::from(mod_name), String::from(id)));
            }
        } else {
            self.held.retain(|h| !(h.0 == mod_name && h.1 == id));
        }
        true
    }

    /// Forget everything a mod put on the interface (it was switched off or reloaded).
    pub fn forget_mod(&mut self, mod_name: &str) {
        self.hud.remove_mod(mod_name);
        self.images.drop_mod(mod_name);
        self.screens.drop_mod(mod_name);
        self.held.retain(|h| h.0 != mod_name);
    }
}

static UI: AtomicPtr<ModUi> = AtomicPtr::new(core::ptr::null_mut());
static UI_BUSY: AtomicBool = AtomicBool::new(false);

/// The shared state's address (made on first use, never freed).
pub fn ui_ptr() -> *mut ModUi {
    let p = UI.load(Ordering::Acquire);
    if !p.is_null() {
        return p;
    }
    let fresh = Box::into_raw(Box::new(ModUi::default()));
    match UI.compare_exchange(core::ptr::null_mut(), fresh, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => fresh,
        Err(old) => {
            // Another got there first.
            drop(unsafe { Box::from_raw(fresh) });
            old
        }
    }
}

/// Run `f` on the shared state. `None` when it is in use already (a hook that
/// reaches here again from inside; nothing is lost, the call is just skipped).
/// Never call a mod's code from inside `f`.
pub fn with_ui<R>(f: impl FnOnce(&mut ModUi) -> R) -> Option<R> {
    if UI_BUSY.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_err() {
        return None;
    }
    // Safety: the state is never freed, and UI_BUSY lets one user in at a time.
    let r = f(unsafe { &mut *ui_ptr() });
    UI_BUSY.store(false, Ordering::Release);
    Some(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_keys_are_noted_once_and_forgotten_with_their_mod() {
        let mut u = ModUi::default();
        assert!(u.set_held("m", "a", true));
        assert!(!u.set_held("m", "a", true), "already down: no change");
        assert!(u.is_held("m", "a"));
        assert!(!u.is_held("n", "a"));
        assert!(u.set_held("n", "a", true));
        assert!(u.set_held("m", "a", false));
        assert!(!u.set_held("m", "a", false), "already up: no change");
        assert!(u.is_held("n", "a"));
        u.forget_mod("n");
        assert!(u.held.is_empty());
    }

    #[test]
    fn forgetting_a_mod_clears_its_hud_and_screen() {
        let mut u = ModUi::default();
        u.hud.set_element("m", hud::Slot::TopLeftList, "x", "t", [255; 4]);
        u.hud.set_element("n", hud::Slot::TopLeftList, "x", "t", [255; 4]);
        u.screens.open("m", "s", "S");
        u.forget_mod("m");
        assert_eq!(u.hud.elements.len(), 1);
        assert!(u.screens.open.is_none());
    }
}
