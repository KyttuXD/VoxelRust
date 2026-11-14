//! What mods keep on the HUD between frames, and where it goes.
//!
//! Three kinds of thing besides the per-frame drawing commands
//! (`hudcmd.rs`): **elements** in named slots (a line of text with an
//! optional icon that stays until the mod changes or removes it),
//! **toasts** (a boxed message at the top that fades after a few seconds)
//! and the **action bar** (one line above the hotbar). `layout` turns all
//! of them into paintable pieces for the geometry of this frame.

use crate::client::modui::hudcmd::{Frame, Prim};
use crate::client::modui::limits::*;
use crate::render::Color;
use alloc::string::String;
use alloc::vec::Vec;

/// A place on the HUD that lines of text can be added to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// Centred above the hotbar, growing upward.
    AboveHotbar,
    /// To the left of the hearts, growing upward.
    BesideHearts,
    /// A list down the top left, under the frame rate.
    TopLeftList,
    /// Centred under the crosshair, growing downward.
    UnderCrosshair,
}

impl Slot {
    /// `above_hotbar`, `beside_hearts`, `top_left_list`, `under_crosshair`
    /// (case, `-` and spaces do not matter).
    pub fn parse(name: &str) -> Option<Slot> {
        let n: String = name.chars().filter(|c| !matches!(c, '-' | '_' | ' ')).map(|c| c.to_ascii_lowercase()).collect();
        Some(match n.as_str() {
            "abovehotbar" => Slot::AboveHotbar,
            "besidehearts" => Slot::BesideHearts,
            "topleftlist" | "topleft" => Slot::TopLeftList,
            "undercrosshair" => Slot::UnderCrosshair,
            _ => return None,
        })
    }
}

/// A line in a slot.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub mod_name: String,
    pub id: String,
    pub slot: Slot,
    pub text: String,
    pub col: Color,
    /// An item id drawn before the text (empty: none).
    pub icon: String,
}

/// A message that shows for a while.
#[derive(Clone, Debug, PartialEq)]
pub struct Toast {
    pub text: String,
    pub left: f32,
    pub total: f32,
}

#[derive(Debug, Default)]
pub struct Hud {
    pub buf: crate::client::modui::hudcmd::CmdBuf,
    pub elements: Vec<Element>,
    pub toasts: Vec<Toast>,
    pub action: Option<Toast>,
}

impl Hud {
    /// A new frame: the drawing commands start over and the timers run.
    pub fn frame(&mut self, dt: f32) {
        self.buf.clear();
        let dt = if dt.is_finite() { dt.clamp(0.0, 1.0) } else { 0.0 };
        for t in self.toasts.iter_mut() {
            t.left -= dt;
        }
        self.toasts.retain(|t| t.left > 0.0);
        if let Some(a) = self.action.as_mut() {
            a.left -= dt;
        }
        if self.action.as_ref().map_or(false, |a| a.left <= 0.0) {
            self.action = None;
        }
    }

    /// Whether there is anything to draw at all.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty() && self.elements.is_empty() && self.toasts.is_empty() && self.action.is_none()
    }

    /// Put a line in a slot, or change the one with this id. `false` when
    /// the id is no good or the mod (or the game) has its share already.
    pub fn set_element(&mut self, mod_name: &str, slot: Slot, id: &str, text: &str, col: Color) -> bool {
        let Some(id) = clean_id(id) else { return false };
        let text = clean_text(text, MAX_TEXT);
        if let Some(e) = self.elements.iter_mut().find(|e| e.mod_name == mod_name && e.id == id) {
            e.slot = slot;
            e.text = text;
            e.col = col;
            return true;
        }
        if self.elements.len() >= MAX_ELEMENTS || self.elements.iter().filter(|e| e.mod_name == mod_name).count() >= MAX_ELEMENTS_PER_MOD {
            return false;
        }
        self.elements.push(Element { mod_name: String::from(mod_name), id, slot, text, col, icon: String::new() });
        true
    }

    /// Give an element an item icon (an empty id takes it away).
    pub fn set_icon(&mut self, mod_name: &str, id: &str, item: &str) -> bool {
        match self.elements.iter_mut().find(|e| e.mod_name == mod_name && e.id == id) {
            Some(e) => {
                e.icon = item.chars().filter(|c| c.is_ascii_graphic()).take(MAX_ITEM_ID).collect();
                true
            }
            None => false,
        }
    }

    pub fn remove_element(&mut self, mod_name: &str, id: &str) -> bool {
        let n = self.elements.len();
        self.elements.retain(|e| !(e.mod_name == mod_name && e.id == id));
        self.elements.len() != n
    }

    /// Everything of one mod (it was switched off).
    pub fn remove_mod(&mut self, mod_name: &str) {
        self.elements.retain(|e| e.mod_name != mod_name);
    }

    /// Show a toast for `secs` seconds (0.5 to [`MAX_SECS`]). When the queue is full the oldest goes.
    pub fn toast(&mut self, text: &str, secs: f32) -> bool {
        let text = clean_text(text, MAX_TEXT);
        if text.is_empty() {
            return false;
        }
        let secs = clamp_secs(secs);
        if self.toasts.len() >= MAX_TOASTS {
            self.toasts.remove(0);
        }
        self.toasts.push(Toast { text, left: secs, total: secs });
        true
    }

    /// Replace the action bar's line.
    pub fn action_bar(&mut self, text: &str, secs: f32) -> bool {
        let text = clean_text(text, MAX_TEXT);
        if text.is_empty() {
            self.action = None;
            return false;
        }
        let secs = clamp_secs(secs);
        self.action = Some(Toast { text, left: secs, total: secs });
        true
    }
}

fn clamp_secs(secs: f32) -> f32 {
    if secs.is_finite() {
        secs.clamp(0.5, MAX_SECS)
    } else {
        3.0
    }
}

/// How opaque a message with `left` of `total` seconds to go is: a quick
/// fade in, a half-second fade out.
pub fn fade(left: f32, total: f32) -> f32 {
    let into = total - left;
    let a_in = if into < 0.15 { (into / 0.15).clamp(0.0, 1.0) } else { 1.0 };
    let a_out = (left / 0.5).clamp(0.0, 1.0);
    if a_in < a_out {
        a_in
    } else {
        a_out
    }
}

/// Where the game's own HUD parts are this frame, in screen pixels.
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub hotbar_x: f32,
    pub hotbar_y: f32,
    pub survival: bool,
}

/// Height of one line of text in a slot, in interface pixels.
const LINE: f32 = 10.0;

fn with_alpha(col: Color, a: f32) -> Color {
    [col[0], col[1], col[2], (col[3] as f32 * a.clamp(0.0, 1.0)) as u8]
}

/// A line of text with a dark backing, its icon if it has one, at (x, y)
/// (the top-left of the line); `right` puts the line's right end at x.
fn line(out: &mut Vec<Prim>, f: &Frame, x: f32, y: f32, e_text: &str, icon: &str, col: Color, align: i8, alpha: f32) {
    let s = f.s;
    let tw = (f.measure)(e_text, s);
    let iw = if icon.is_empty() { 0.0 } else { 9.0 * s };
    let total = tw + iw;
    let x0 = match align {
        0 => x,
        1 => x - total * 0.5,
        _ => x - total,
    };
    if x0 + total < 0.0 || x0 > f.w || y + 8.0 * s < 0.0 || y > f.h {
        return;
    }
    if let Some((rx, ry, rw, rh)) = crate::client::modui::hudcmd::clip_rect(x0 - 2.0 * s, y - s, total + 4.0 * s, 9.0 * s, f.w, f.h) {
        out.push(Prim::Rect { x: rx, y: ry, w: rw, h: rh, col: with_alpha([0, 0, 0, 120], alpha) });
    }
    if !icon.is_empty() && x0 >= 0.0 && y >= 0.0 && x0 + 8.0 * s <= f.w && y + 8.0 * s <= f.h {
        out.push(Prim::Item { x: x0, y: y - 0.5 * s, size: 8.0 * s, id: String::from(icon) });
    }
    let tx = x0 + iw;
    let mut shown = String::from(e_text);
    while !shown.is_empty() && tx + (f.measure)(&shown, s) > f.w {
        shown.pop();
    }
    if !shown.is_empty() && col[3] > 0 {
        out.push(Prim::Text { x: tx, y, scale: s, text: shown, col: with_alpha(col, alpha) });
    }
}

/// The retained parts of the HUD: the slots, the action bar, the toasts.
pub fn layout(hud: &Hud, g: &Geometry, f: &Frame) -> Vec<Prim> {
    let s = f.s;
    let mut out = Vec::new();
    let in_slot = |slot: Slot| hud.elements.iter().filter(move |e| e.slot == slot);
    // Down the top left, under the frame rate.
    for (i, e) in in_slot(Slot::TopLeftList).enumerate() {
        line(&mut out, f, 4.0 * s, (16.0 + i as f32 * LINE) * s, &e.text, &e.icon, e.col, 0, 1.0);
    }
    // Under the crosshair, centred.
    for (i, e) in in_slot(Slot::UnderCrosshair).enumerate() {
        line(&mut out, f, f.w * 0.5, f.h * 0.5 + (22.0 + i as f32 * LINE) * s, &e.text, &e.icon, e.col, 1, 1.0);
    }
    // Above the hotbar (and what the game puts there): upward from a base line.
    let base = g.hotbar_y - (if g.survival { 48.0 } else { 38.0 }) * s;
    for (i, e) in in_slot(Slot::AboveHotbar).enumerate() {
        line(&mut out, f, f.w * 0.5, base - i as f32 * LINE * s, &e.text, &e.icon, e.col, 1, 1.0);
    }
    // Left of the hearts, on the hearts' line, upward.
    let hy = g.hotbar_y - if g.survival { 12.0 * s } else { -6.0 * s };
    for (i, e) in in_slot(Slot::BesideHearts).enumerate() {
        line(&mut out, f, g.hotbar_x - 6.0 * s, hy - i as f32 * LINE * s, &e.text, &e.icon, e.col, 2, 1.0);
    }
    if let Some(a) = &hud.action {
        let y = g.hotbar_y - (if g.survival { 36.0 } else { 26.0 }) * s;
        line(&mut out, f, f.w * 0.5, y, &a.text, "", [255, 240, 200, 255], 1, fade(a.left, a.total));
    }
    for (i, t) in hud.toasts.iter().enumerate() {
        let a = fade(t.left, t.total);
        let tw = (f.measure)(&t.text, s);
        let (bw, bh) = (tw + 12.0 * s, 13.0 * s);
        let x = (f.w - bw) * 0.5;
        let y = (8.0 + i as f32 * 15.0) * s;
        if y > f.h {
            break;
        }
        if let Some((rx, ry, rw, rh)) = crate::client::modui::hudcmd::clip_rect(x, y, bw, bh, f.w, f.h) {
            out.push(Prim::Rect { x: rx, y: ry, w: rw, h: rh, col: with_alpha([16, 18, 24, 215], a) });
            if let Some((fx, fy, fw, fh)) = crate::client::modui::hudcmd::clip_rect(x, y, bw, s, f.w, f.h) {
                out.push(Prim::Rect { x: fx, y: fy, w: fw, h: fh, col: with_alpha([255, 222, 130, 255], a) });
            }
        }
        line(&mut out, f, f.w * 0.5, y + 3.0 * s, &t.text, "", [255, 255, 255, 255], 1, a);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(t: &str, sc: f32) -> f32 {
        t.len() as f32 * 6.0 * sc
    }

    const W: Color = [255, 255, 255, 255];

    #[test]
    fn slots_parse_in_any_spelling() {
        assert_eq!(Slot::parse("above_hotbar"), Some(Slot::AboveHotbar));
        assert_eq!(Slot::parse("Beside-Hearts"), Some(Slot::BesideHearts));
        assert_eq!(Slot::parse("top left list"), Some(Slot::TopLeftList));
        assert_eq!(Slot::parse("under_crosshair"), Some(Slot::UnderCrosshair));
        assert_eq!(Slot::parse("nowhere"), None);
    }

    #[test]
    fn elements_are_added_changed_capped_and_removed() {
        let mut h = Hud::default();
        assert!(h.set_element("a", Slot::TopLeftList, "x", "hello", W));
        assert_eq!(h.elements[0].text, "HELLO");
        // The same id changes the line, even in another slot.
        assert!(h.set_element("a", Slot::AboveHotbar, "x", "bye", W));
        assert_eq!((h.elements.len(), h.elements[0].slot), (1, Slot::AboveHotbar));
        // Another mod can use the same id.
        assert!(h.set_element("b", Slot::AboveHotbar, "x", "other", W));
        assert_eq!(h.elements.len(), 2);
        assert!(!h.set_element("a", Slot::AboveHotbar, "bad id", "t", W));
        assert!(h.set_icon("a", "x", "diamond"));
        assert_eq!(h.elements[0].icon, "diamond");
        assert!(!h.set_icon("a", "nope", "diamond"));
        for i in 1..MAX_ELEMENTS_PER_MOD {
            assert!(h.set_element("a", Slot::TopLeftList, &format!("e{}", i), "t", W));
        }
        assert!(!h.set_element("a", Slot::TopLeftList, "one-more", "t", W));
        assert!(h.remove_element("a", "e1"));
        assert!(!h.remove_element("a", "e1"));
        assert!(h.set_element("a", Slot::TopLeftList, "one-more", "t", W));
        h.remove_mod("a");
        assert_eq!(h.elements.len(), 1);
        // Long text is cut.
        h.set_element("b", Slot::TopLeftList, "x", &"y".repeat(500), W);
        assert_eq!(h.elements[0].text.len(), MAX_TEXT);
    }

    #[test]
    fn all_mods_together_cannot_pass_the_elements_cap() {
        let mut h = Hud::default();
        let mut ok = 0;
        for m in 0..10 {
            for i in 0..MAX_ELEMENTS_PER_MOD {
                if h.set_element(&format!("m{}", m), Slot::TopLeftList, &format!("e{}", i), "t", W) {
                    ok += 1;
                }
            }
        }
        assert_eq!(ok, MAX_ELEMENTS);
    }

    #[test]
    fn toasts_expire_queue_up_and_are_bounded() {
        let mut h = Hud::default();
        assert!(h.toast("one", 2.0));
        assert!(!h.toast("", 2.0));
        h.frame(1.0);
        assert_eq!(h.toasts.len(), 1);
        h.frame(1.5);
        assert!(h.toasts.is_empty());
        for i in 0..20 {
            h.toast(&format!("t{}", i), 1000.0);
        }
        assert_eq!(h.toasts.len(), MAX_TOASTS);
        assert_eq!(h.toasts[0].text, "T12", "the oldest were dropped");
        assert_eq!(h.toasts[0].total, MAX_SECS);
        h.toast("nan", f32::NAN);
        assert_eq!(h.toasts.last().unwrap().total, 3.0);
        // A huge or negative frame time cannot clear everything at once... only a second.
        let before = h.toasts.len();
        h.frame(-5.0);
        h.frame(f32::INFINITY);
        assert_eq!(h.toasts.len(), before);
    }

    #[test]
    fn the_action_bar_is_one_line_that_expires() {
        let mut h = Hud::default();
        assert!(h.action_bar("saved", 2.0));
        assert!(h.action_bar("replaced", 1.0));
        assert_eq!(h.action.as_ref().unwrap().text, "REPLACED");
        h.frame(0.5);
        assert!(h.action.is_some());
        h.frame(0.6);
        assert!(h.action.is_none());
        h.action_bar("x", 5.0);
        assert!(!h.action_bar("", 5.0), "an empty line clears it");
        assert!(h.action.is_none());
        assert!(h.is_empty());
    }

    #[test]
    fn a_message_fades_in_and_out() {
        assert_eq!(fade(3.0, 3.0), 0.0);
        assert!(fade(2.95, 3.0) > 0.0 && fade(2.95, 3.0) < 1.0);
        assert_eq!(fade(2.0, 3.0), 1.0);
        assert_eq!(fade(0.25, 3.0), 0.5);
        assert_eq!(fade(0.0, 3.0), 0.0);
        assert_eq!(fade(-1.0, 3.0), 0.0);
    }

    #[test]
    fn slot_lines_stack_the_right_way_and_stay_on_screen() {
        let mm = m;
        let f = Frame { w: 400.0, h: 200.0, s: 1.0, measure: &mm };
        let g = Geometry { hotbar_x: 100.0, hotbar_y: 170.0, survival: true };
        let mut h = Hud::default();
        h.set_element("a", Slot::TopLeftList, "one", "first", W);
        h.set_element("a", Slot::TopLeftList, "two", "second", W);
        h.set_element("a", Slot::UnderCrosshair, "xh", "aim", W);
        h.set_element("a", Slot::AboveHotbar, "up1", "low", W);
        h.set_element("a", Slot::AboveHotbar, "up2", "high", W);
        h.set_element("a", Slot::BesideHearts, "hp", "left", W);
        let out = layout(&h, &g, &f);
        let text_y = |t: &str| -> (f32, f32) {
            out.iter()
                .find_map(|p| match p {
                    Prim::Text { x, y, text, .. } if text == t => Some((*x, *y)),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no {}", t))
        };
        // Top left list goes down; the first is under the frame rate.
        assert_eq!(text_y("FIRST"), (4.0, 16.0));
        assert_eq!(text_y("SECOND"), (4.0, 26.0));
        // Under the crosshair: centred, below the middle.
        let (x, y) = text_y("AIM");
        assert_eq!((x, y), (200.0 - 9.0, 122.0));
        // Above the hotbar: the second is higher than the first.
        assert!(text_y("HIGH").1 < text_y("LOW").1);
        assert!(text_y("LOW").1 < g.hotbar_y);
        // Beside the hearts: it ends left of the hotbar.
        let (x, _) = text_y("LEFT");
        assert_eq!(x + 4.0 * 6.0, 94.0);
        // Everything is inside the screen.
        for p in &out {
            if let Prim::Rect { x, y, w, h, .. } = p {
                assert!(*x >= 0.0 && *y >= 0.0 && x + w <= 400.0 && y + h <= 200.0, "{:?}", p);
            }
        }
    }

    #[test]
    fn lines_off_the_screen_and_toasts_are_handled() {
        let mm = m;
        let f = Frame { w: 100.0, h: 60.0, s: 1.0, measure: &mm };
        let g = Geometry { hotbar_x: 10.0, hotbar_y: 40.0, survival: false };
        let mut h = Hud::default();
        // A very long line is cut at the screen's edge.
        h.set_element("a", Slot::TopLeftList, "long", &"x".repeat(100), W);
        h.toast("a toast that is wider than the whole little screen", 3.0);
        h.action_bar("act", 3.0);
        h.frame(0.5);
        let out = layout(&h, &g, &f);
        assert!(!out.is_empty());
        for p in &out {
            match p {
                Prim::Text { x, text, .. } => assert!(x + text.len() as f32 * 6.0 <= 100.0 + 0.001, "{} {}", x, text),
                Prim::Rect { x, y, w, h, .. } => assert!(*x >= 0.0 && *y >= 0.0 && x + w <= 100.0 && y + h <= 60.0, "{:?}", p),
                _ => {}
            }
        }
        // Nothing at all to draw: nothing made.
        assert!(layout(&Hud::default(), &g, &f).is_empty());
    }
}
