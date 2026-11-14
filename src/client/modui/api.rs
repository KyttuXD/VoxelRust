//! [`ModApi`]: what one mod may do to the interface.
//!
//! Scripts reach it through `script/host_ui.rs`, native code gets one as an
//! argument of its hooks (`on_hud`, `on_ui`). Every call is bounded: text and
//! ids are cut or refused, the per-frame drawing commands have a cap, the
//! number of key bindings, switches, HUD lines and widgets are capped, and
//! numbers that are NaN or enormous are brought into range. A call that is
//! over a cap returns `Ok(false)` (nothing was done); an unknown anchor or
//! slot name is an `Err` (the script has a typo; it is switched off with the
//! message).

use crate::client::modui::binds::KeyDecl;
use crate::client::modui::hud::Slot;
use crate::client::modui::hud_image;
use crate::client::modui::hudcmd::{self, Anchor, Cmd};
use crate::client::modui::limits::*;
use crate::client::modui::ModUi;
use alloc::format;
use alloc::string::String;

/// One mod's handle on the shared interface state.
pub struct ModApi<'a> {
    pub ui: &'a mut ModUi,
    pub mod_name: &'a str,
}

fn anchor(name: &str) -> Result<Anchor, String> {
    Anchor::parse(name).ok_or_else(|| format!("`{}` is not an anchor (top-left, top, top-right, left, centre, right, bottom-left, bottom, bottom-right)", name))
}

fn item_id(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_graphic()).take(MAX_ITEM_ID).collect()
}

impl<'a> ModApi<'a> {
    pub fn new(ui: &'a mut ModUi, mod_name: &'a str) -> ModApi<'a> {
        ModApi { ui, mod_name }
    }

    // ---- declaring (only while the script loads, or from a native mod's tables)

    /// Declare a key binding; only allowed while `on_register` runs.
    pub fn bind_key(&mut self, id: &str, label: &str, group: &str, key: i32) -> Result<bool, String> {
        if !self.ui.registering {
            return Err(String::from("`bind_key` may only be called from `on_register`"));
        }
        self.ui.decls.add_bind(self.mod_name, id, label, group, key).map(|_| true)
    }

    /// Declare a row of the mod switches page; only allowed while `on_register` runs.
    pub fn add_switch(&mut self, id: &str, label: &str, default: bool) -> Result<bool, String> {
        if !self.ui.registering {
            return Err(String::from("`add_switch` may only be called from `on_register`"));
        }
        self.ui.decls.add_switch(self.mod_name, id, label, default).map(|_| true)
    }

    /// Whether this mod's binding `id` has its key down now.
    pub fn key_held(&self, id: &str) -> bool {
        self.ui.is_held(&clean_mod(self.mod_name), id)
    }

    // ---- HUD drawing, once a frame

    fn cmd(&mut self, cmd: Cmd) -> bool {
        self.ui.hud.buf.push(self.mod_name, cmd)
    }

    pub fn hud_text(&mut self, at: &str, dx: f64, dy: f64, scale: f64, text: &str, col: i64) -> Result<bool, String> {
        let at = anchor(at)?;
        let text = clean_text(text, MAX_TEXT);
        if text.is_empty() {
            return Ok(false);
        }
        Ok(self.cmd(Cmd::Text { at, dx: hudcmd::num(dx, 8192.0), dy: hudcmd::num(dy, 8192.0), scale: hudcmd::num(scale, 8.0), text, col: hudcmd::colour(col) }))
    }

    pub fn hud_rect(&mut self, at: &str, dx: f64, dy: f64, w: f64, h: f64, col: i64) -> Result<bool, String> {
        let at = anchor(at)?;
        Ok(self.cmd(Cmd::Rect { at, dx: hudcmd::num(dx, 8192.0), dy: hudcmd::num(dy, 8192.0), w: hudcmd::num(w, 8192.0), h: hudcmd::num(h, 8192.0), col: hudcmd::colour(col) }))
    }

    pub fn hud_outline(&mut self, at: &str, dx: f64, dy: f64, w: f64, h: f64, thick: f64, col: i64) -> Result<bool, String> {
        let at = anchor(at)?;
        Ok(self.cmd(Cmd::Outline { at, dx: hudcmd::num(dx, 8192.0), dy: hudcmd::num(dy, 8192.0), w: hudcmd::num(w, 8192.0), h: hudcmd::num(h, 8192.0), thick: hudcmd::num(thick, 16.0), col: hudcmd::colour(col) }))
    }

    pub fn hud_item(&mut self, at: &str, dx: f64, dy: f64, size: f64, id: &str) -> Result<bool, String> {
        let at = anchor(at)?;
        let id = item_id(id);
        if id.is_empty() {
            return Ok(false);
        }
        Ok(self.cmd(Cmd::Item { at, dx: hudcmd::num(dx, 8192.0), dy: hudcmd::num(dy, 8192.0), size: hudcmd::num(size, 256.0), id }))
    }

    pub fn hud_block(&mut self, at: &str, dx: f64, dy: f64, size: f64, id: &str) -> Result<bool, String> {
        let at = anchor(at)?;
        let id = item_id(id);
        if id.is_empty() {
            return Ok(false);
        }
        Ok(self.cmd(Cmd::Block { at, dx: hudcmd::num(dx, 8192.0), dy: hudcmd::num(dy, 8192.0), size: hudcmd::num(size, 256.0), id }))
    }

    pub fn hud_bar(&mut self, at: &str, dx: f64, dy: f64, w: f64, h: f64, frac: f64, col: i64) -> Result<bool, String> {
        let at = anchor(at)?;
        Ok(self.cmd(Cmd::Bar { at, dx: hudcmd::num(dx, 8192.0), dy: hudcmd::num(dy, 8192.0), w: hudcmd::num(w, 8192.0), h: hudcmd::num(h, 8192.0), frac: hudcmd::num(frac, 1.0e6), col: hudcmd::colour(col) }))
    }

    // ---- pictures (see `hud_image.rs`)

    /// The mod's picture `file` (read from its folder the first time), or `None`
    /// when the name is not allowed, it cannot be read, or the mod's 16 are used up.
    fn picture(&mut self, data_dir: &str, file: &str) -> Option<hud_image::Found> {
        let owner = self.mod_name;
        self.ui.images.get(owner, file, &mut || hud_image::load(data_dir, owner, file))
    }

    /// Draw a picture, or the part `uv` (u0, v0, u1, v1: 0 to 1 across the picture) of it, `w` x `h`
    /// interface pixels at the anchor moved by (`dx`, `dy`); `tint` is `0xRRGGBB` (white is as it is)
    /// and `alpha` the opacity, 0 to 1. `false` when the picture is not there or the frame is full.
    pub fn hud_image(&mut self, data_dir: &str, file: &str, at: &str, dx: f64, dy: f64, w: f64, h: f64, uv: [f64; 4], alpha: f64, tint: i64) -> Result<bool, String> {
        let at = anchor(at)?;
        let Some(found) = self.picture(data_dir, file) else { return Ok(false) };
        let uv = [hudcmd::num(uv[0], 1.0e4), hudcmd::num(uv[1], 1.0e4), hudcmd::num(uv[2], 1.0e4), hudcmd::num(uv[3], 1.0e4)];
        Ok(self.cmd(Cmd::Image { at, dx: hudcmd::num(dx, 8192.0), dy: hudcmd::num(dy, 8192.0), w: hudcmd::num(w, 8192.0), h: hudcmd::num(h, 8192.0), uv, col: hud_image::tinted(tint, alpha), img: found.id }))
    }

    /// A picture at a place and size given as fractions of the screen (0 to 1; from `hud_frac_image`).
    pub fn hud_image_frac(&mut self, data_dir: &str, file: &str, x: f32, y: f32, w: f32, h: f32, alpha: f32) -> bool {
        let Some(found) = self.picture(data_dir, file) else { return false };
        self.cmd(Cmd::ImageFrac { x, y, w, h, col: hud_image::tinted(0xFFFFFF, alpha as f64), img: found.id })
    }

    /// Fit a picture to the whole screen: `"cover"` fills it (and crops), `"contain"` shows all of it.
    pub fn hud_image_fit(&mut self, data_dir: &str, file: &str, mode: &str, alpha: f64) -> Result<bool, String> {
        let Some(fit) = hud_image::Fit::parse(mode) else { return Err(format!("`{}` is not a fit (\"cover\" or \"contain\")", mode)) };
        let Some(found) = self.picture(data_dir, file) else { return Ok(false) };
        Ok(self.cmd(Cmd::Fit { img: found.id, iw: found.w, ih: found.h, fit, col: hud_image::tinted(0xFFFFFF, alpha) }))
    }

    // ---- HUD elements, toasts, the action bar

    /// Put a line in a named slot (`above_hotbar`, `beside_hearts`,
    /// `top_left_list`, `under_crosshair`), or change the line with this id.
    pub fn hud_set(&mut self, slot: &str, id: &str, text: &str, col: i64) -> Result<bool, String> {
        let Some(slot) = Slot::parse(slot) else { return Err(format!("`{}` is not a HUD slot (above_hotbar, beside_hearts, top_left_list, under_crosshair)", slot)) };
        Ok(self.ui.hud.set_element(self.mod_name, slot, id, text, hudcmd::colour(col)))
    }

    pub fn hud_set_icon(&mut self, id: &str, item: &str) -> bool {
        self.ui.hud.set_icon(self.mod_name, id, item)
    }

    pub fn hud_remove(&mut self, id: &str) -> bool {
        self.ui.hud.remove_element(self.mod_name, id)
    }

    pub fn toast(&mut self, text: &str, secs: f64) -> bool {
        self.ui.hud.toast(text, hudcmd::num(secs, 1.0e6))
    }

    pub fn action_bar(&mut self, text: &str, secs: f64) -> bool {
        self.ui.hud.action_bar(text, hudcmd::num(secs, 1.0e6))
    }

    // ---- screens

    pub fn ui_open(&mut self, name: &str, title: &str) -> bool {
        self.ui.screens.open(self.mod_name, name, title)
    }

    pub fn ui_close(&mut self) -> bool {
        self.ui.screens.close_for(self.mod_name)
    }

    pub fn ui_text(&mut self, text: &str) -> bool {
        self.ui.screens.text(self.mod_name, text)
    }

    pub fn ui_button(&mut self, id: &str, label: &str) -> bool {
        self.ui.screens.button(self.mod_name, id, label)
    }

    pub fn ui_toggle(&mut self, id: &str, label: &str, on: bool) -> bool {
        self.ui.screens.toggle(self.mod_name, id, label, on)
    }

    pub fn ui_stepper(&mut self, id: &str, label: &str, value: i64, min: i64, max: i64) -> bool {
        self.ui.screens.stepper(self.mod_name, id, label, value, min, max)
    }

    pub fn ui_input(&mut self, id: &str, label: &str, value: &str) -> bool {
        self.ui.screens.input(self.mod_name, id, label, value)
    }

    pub fn ui_list(&mut self, id: &str, items: &str, selected: i64) -> bool {
        self.ui.screens.list(self.mod_name, id, items, selected)
    }

    pub fn ui_set_text(&mut self, id: &str, text: &str) -> bool {
        self.ui.screens.set_text(self.mod_name, id, text)
    }

    pub fn ui_set_line(&mut self, n: i64, text: &str) -> bool {
        n >= 0 && self.ui.screens.set_line(self.mod_name, n as usize, text)
    }

    pub fn ui_set_value(&mut self, id: &str, n: i64) -> bool {
        self.ui.screens.set_value(self.mod_name, id, n)
    }

    pub fn ui_get_text(&mut self, id: &str) -> String {
        self.ui.screens.get_text(self.mod_name, id).unwrap_or_default()
    }

    pub fn ui_get_value(&mut self, id: &str) -> i64 {
        self.ui.screens.get_value(self.mod_name, id).unwrap_or(0)
    }

    /// Frames a second, as counted by the window loop.
    pub fn fps(&self) -> f64 {
        let f = self.ui.fps;
        if f.is_finite() {
            f as f64
        } else {
            0.0
        }
    }
}

/// Declare a native mod's tables the way a script's `on_register` would
/// (done when the key bindings page or a key press asks for them).
pub fn native_binds(mod_name: &str, list: &[KeyDecl]) -> alloc::vec::Vec<crate::client::modui::binds::Bind> {
    let mut d = crate::client::modui::binds::Decls::default();
    for k in list {
        let _ = d.add_bind(mod_name, k.id, k.label, k.group, k.key);
    }
    d.binds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::modui::hudcmd::Cmd;

    fn api(ui: &mut ModUi) -> ModApi<'_> {
        ModApi::new(ui, "m")
    }

    #[test]
    fn keys_can_only_be_declared_while_registering() {
        let mut ui = ModUi::default();
        assert!(api(&mut ui).bind_key("a", "A", "", 11).is_err());
        assert!(api(&mut ui).add_switch("s", "S", false).is_err());
        ui.registering = true;
        assert_eq!(api(&mut ui).bind_key("a", "A", "g", 11), Ok(true));
        assert_eq!(api(&mut ui).add_switch("s", "S", true), Ok(true));
        assert!(api(&mut ui).bind_key("a b", "A", "", 11).is_err());
        assert_eq!((ui.decls.binds.len(), ui.decls.switches.len()), (1, 1));
        assert_eq!(ui.decls.binds[0].mod_name, "m");
    }

    #[test]
    fn drawing_calls_make_commands_until_the_cap() {
        let mut ui = ModUi::default();
        assert_eq!(api(&mut ui).hud_text("top-left", 1.0, 2.0, 1.0, "hello", 0xFFFFFF), Ok(true));
        assert_eq!(api(&mut ui).hud_rect("centre", 0.0, 0.0, 10.0, 5.0, 0x80000000), Ok(true));
        assert_eq!(api(&mut ui).hud_outline("bottom", 0.0, 0.0, 10.0, 5.0, 1.0, 0xFF0000), Ok(true));
        assert_eq!(api(&mut ui).hud_item("right", 0.0, 0.0, 16.0, "diamond"), Ok(true));
        assert_eq!(api(&mut ui).hud_block("left", 0.0, 0.0, 16.0, "stone"), Ok(true));
        assert_eq!(api(&mut ui).hud_bar("top", 0.0, 0.0, 40.0, 4.0, 0.5, 0x00FF00), Ok(true));
        assert_eq!(ui.hud.buf.len(), 6);
        match &ui.hud.buf.cmds[0] {
            Cmd::Text { text, col, .. } => assert_eq!((text.as_str(), *col), ("HELLO", [255, 255, 255, 255])),
            c => panic!("{:?}", c),
        }
        // Empty text and empty ids are not commands.
        assert_eq!(api(&mut ui).hud_text("top", 0.0, 0.0, 1.0, "", 0), Ok(false));
        assert_eq!(api(&mut ui).hud_item("top", 0.0, 0.0, 1.0, ""), Ok(false));
        // Past the per-mod share: refused, not an error.
        let mut ok = 0;
        for _ in 0..1000 {
            if api(&mut ui).hud_rect("top", 0.0, 0.0, 1.0, 1.0, 0) == Ok(true) {
                ok += 1;
            }
        }
        assert_eq!(ok, MAX_HUD_CMDS_PER_MOD - 6);
        assert_eq!(ui.hud.buf.len(), MAX_HUD_CMDS_PER_MOD);
        // The next frame starts empty.
        ui.hud.frame(0.016);
        assert_eq!(api(&mut ui).hud_rect("top", 0.0, 0.0, 1.0, 1.0, 0), Ok(true));
    }

    #[test]
    fn pictures_are_drawn_from_the_cache_with_the_limits() {
        let mut ui = ModUi::default();
        // (No such file, and no picture of that name in a pack: nothing is drawn, and it is not an error.)
        assert_eq!(api(&mut ui).hud_image("", "nothing-here.png", "top-left", 0.0, 0.0, 10.0, 10.0, [0.0, 0.0, 1.0, 1.0], 1.0, 0xFFFFFF), Ok(false));
        assert!(ui.hud.buf.is_empty());
        // A name that is not allowed.
        assert_eq!(api(&mut ui).hud_image("", "../x.png", "top-left", 0.0, 0.0, 10.0, 10.0, [0.0, 0.0, 1.0, 1.0], 1.0, 0xFFFFFF), Ok(false));
        // A wrong anchor or fit is the script's typo.
        assert!(api(&mut ui).hud_image("", "a.png", "sideways", 0.0, 0.0, 1.0, 1.0, [0.0, 0.0, 1.0, 1.0], 1.0, 0).is_err());
        assert!(api(&mut ui).hud_image_fit("", "a.png", "stretch", 1.0).is_err());
        // A picture that is cached is drawn.
        let id = ui.images.get("m", "a.png", &mut || Some(hud_image::Pixels { w: 4, h: 2, rgba: alloc::vec![255; 32] })).unwrap().id;
        assert_eq!(api(&mut ui).hud_image("", "a.png", "centre", 1.0, 2.0, 40.0, 20.0, [0.0, 0.0, 0.5, 1.0], 0.5, 0xFF0000), Ok(true));
        assert_eq!(api(&mut ui).hud_image_fit("", "a.png", "Cover", 1.0), Ok(true));
        match &ui.hud.buf.cmds[0] {
            Cmd::Image { img, uv, col, w, .. } => assert_eq!((*img, *uv, *col, *w), (id, [0.0, 0.0, 0.5, 1.0], [255, 0, 0, 127], 40.0)),
            c => panic!("{:?}", c),
        }
        assert!(matches!(&ui.hud.buf.cmds[1], Cmd::Fit { iw: 4, ih: 2, .. }));
        // Another mod does not see it as its own picture.
        assert_eq!(ModApi::new(&mut ui, "n").hud_image("", "a.png", "top", 0.0, 0.0, 1.0, 1.0, [0.0, 0.0, 1.0, 1.0], 1.0, 0), Ok(false));
        // Switching the mod off takes its picture away.
        ui.forget_mod("m");
        assert_eq!(ui.images.len(), 1 /* the failed one of mod n */);
    }

    #[test]
    fn a_wrong_anchor_or_slot_is_an_error_with_the_names() {
        let mut ui = ModUi::default();
        let e = api(&mut ui).hud_text("sideways", 0.0, 0.0, 1.0, "x", 0).unwrap_err();
        assert!(e.contains("sideways") && e.contains("top-left"), "{}", e);
        let e = api(&mut ui).hud_set("nowhere", "id", "x", 0).unwrap_err();
        assert!(e.contains("above_hotbar"), "{}", e);
        assert!(ui.hud.buf.is_empty() && ui.hud.elements.is_empty());
    }

    #[test]
    fn strange_numbers_are_brought_into_range() {
        let mut ui = ModUi::default();
        api(&mut ui).hud_rect("top", f64::NAN, f64::INFINITY, 1.0e300, -5.0, -1).unwrap();
        match &ui.hud.buf.cmds[0] {
            Cmd::Rect { dx, dy, w, h, col, .. } => {
                assert_eq!((*dx, *dy, *w, *h), (0.0, 8192.0, 8192.0, -5.0));
                assert_eq!(*col, [0, 0, 0, 255]);
            }
            c => panic!("{:?}", c),
        }
    }

    #[test]
    fn elements_toasts_and_screens_go_through_the_api() {
        let mut ui = ModUi::default();
        assert_eq!(api(&mut ui).hud_set("above_hotbar", "c", "N 90", 0xFFFFFF), Ok(true));
        assert!(api(&mut ui).hud_set_icon("c", "compass"));
        assert!(api(&mut ui).hud_remove("c"));
        assert!(!api(&mut ui).hud_remove("c"));
        assert!(api(&mut ui).toast("hi", 2.0));
        assert!(api(&mut ui).action_bar("bar", 2.0));
        assert!(!api(&mut ui).ui_text("no screen yet"));
        assert!(api(&mut ui).ui_open("s", "Title"));
        assert!(api(&mut ui).ui_text("hello"));
        assert!(api(&mut ui).ui_button("b", "press"));
        assert!(api(&mut ui).ui_input("i", "name", "x"));
        assert_eq!(api(&mut ui).ui_get_text("i"), "x");
        assert!(api(&mut ui).ui_toggle("t", "t", true));
        assert_eq!(api(&mut ui).ui_get_value("t"), 1);
        assert_eq!(api(&mut ui).ui_get_value("nope"), 0);
        assert!(api(&mut ui).ui_close());
        assert!(!api(&mut ui).ui_close());
        let mut other = ModApi::new(&mut ui, "n");
        assert!(!other.ui_text("not mine"));
    }

    #[test]
    fn held_keys_are_per_mod() {
        let mut ui = ModUi::default();
        ui.set_held("m", "a", true);
        assert!(api(&mut ui).key_held("a"));
        assert!(!ModApi::new(&mut ui, "n").key_held("a"));
        assert!(!api(&mut ui).key_held("b"));
    }

    #[test]
    fn native_tables_become_bindings() {
        let list = [KeyDecl { id: "go", label: "go", group: "", key: 9 }, KeyDecl { id: "bad id", label: "x", group: "", key: 9 }];
        let b = native_binds("nat", &list);
        assert_eq!(b.len(), 1);
        assert_eq!((b[0].mod_name.as_str(), b[0].id.as_str(), b[0].default_key), ("nat", "go", 9));
    }
}
