//! The page Settings > CONTROLS > KEY BINDINGS: the game's own bindings first,
//! then each mod's under a heading with its name. It scrolls (mouse wheel and
//! a scroll bar) when the list is longer than the room, and two bindings on one
//! key are outlined in red (allowed, not refused).
//!
//! The lines and the conflicts are worked out in `binds.rs`, the scrolling in
//! `scroll.rs`; this file lays them out with the settings' own widgets.

use crate::client::modui::binds::{self, Line, Row};
use crate::client::modui::{keys, scroll, with_ui};
use crate::client::ui::{self, Ui};
use crate::settings::{Settings, ACTIONS};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// The colour that outlines a binding that shares its key.
const CONFLICT: [u8; 4] = [235, 84, 72, 255];

/// Where the list goes: its left edge, the top of its first line, its
/// width, a button's height, a line's step, and how many lines fit.
#[derive(Clone, Copy)]
pub struct Room {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub bh: f32,
    pub step: f32,
    pub lines: usize,
}

/// Give a mod binding (row `index` of the flat list, counted from the end of
/// the game's own bindings) the key `scancode`.
pub fn rebind_row(st: &mut Settings, rows: &[Row], index: usize, scancode: i32) -> bool {
    match rows.get(index) {
        Some(r) => {
            binds::set_key(st, r, scancode);
            true
        }
        None => false,
    }
}

/// `rebinding` was an index of the flat list (the game's `ACTIONS` first, then
/// the mods' rows): give a key to the right one.
pub fn rebind(st: &mut Settings, rebinding: usize, scancode: i32) -> bool {
    if rebinding < ACTIONS.len() {
        st.keys[rebinding] = scancode;
        return true;
    }
    let rows = keys::rows(st);
    rebind_row(st, &rows, rebinding - ACTIONS.len(), scancode)
}

/// Every key back on its default (the game's and the mods').
pub fn reset(st: &mut Settings) {
    st.keys = Settings::default().keys;
    binds::reset_all(st);
}

/// Draw the list and handle clicks (a click starts rebinding: `rebinding`
/// is set to the flat index) and the wheel.
pub fn draw(ui: &mut Ui, st: &Settings, rebinding: &mut Option<usize>, key_name: fn(i32) -> String, room: Room) {
    let s = ui.s;
    let rows = keys::rows(st);
    let secs = binds::sections(&rows);
    let core = ACTIONS.len();
    let lines = binds::page_lines(core, &secs);
    let mut all: Vec<i32> = st.keys.to_vec();
    all.extend(rows.iter().map(|r| r.key));
    let clash = binds::conflicts(&all);

    let shown = room.lines.max(1);
    let mut pos = with_ui(|u| u.scroll_binds).unwrap_or(0);
    if ui.hot(room.x, room.y, room.w, shown as f32 * room.step) {
        pos = scroll::wheel(pos, ui.inp.wheel, 2, lines.len(), shown);
    }
    pos = scroll::clamp(pos, lines.len(), shown);

    let bar = if lines.len() > shown { 6.0 * s } else { 0.0 };
    let list_w = room.w - if bar > 0.0 { bar + 3.0 * s } else { 0.0 };
    let half = (list_w - 4.0 * s) * 0.5;
    for k in 0..shown {
        let Some(line) = lines.get(pos as usize + k) else { break };
        let ly = room.y + k as f32 * room.step;
        match line {
            Line::Heading(text) => {
                ui.text(room.x + 2.0 * s, ly + 5.0 * s, text, ui::ACCENT);
                ui.r.rect(room.x, ly + room.bh - s, list_w, s, [96, 104, 130, 200]);
            }
            Line::Pair(a, b) => {
                for (col, idx) in [Some(*a), *b].iter().enumerate() {
                    let Some(idx) = *idx else { continue };
                    let cx = room.x + col as f32 * (half + 4.0 * s);
                    let (label, key) = if idx < core { (String::from(ACTIONS[idx].2), st.keys[idx]) } else { (rows[idx - core].label.clone(), rows[idx - core].key) };
                    let text = if *rebinding == Some(idx) { String::from("PRESS A KEY") } else { format!("{}: {}", label, key_name(key)) };
                    if ui.button(cx, ly, half, room.bh, &text) {
                        *rebinding = Some(idx);
                    }
                    if clash.get(idx).copied().unwrap_or(false) {
                        ui.r.frame(cx, ly, half, room.bh, s, CONFLICT);
                    }
                }
            }
        }
    }
    if bar > 0.0 {
        ui.scrollbar(room.x + room.w - bar, room.y, bar, shown as f32 * room.step - 3.0 * s, &mut pos, shown, lines.len());
        pos = scroll::clamp(pos, lines.len(), shown);
    }
    with_ui(|u| u.scroll_binds = pos);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::modui::binds::Decls;

    #[test]
    fn a_mod_binding_is_rebound_by_its_place_in_the_list() {
        let mut d = Decls::default();
        d.add_bind("m", "a", "A", "", 11).unwrap();
        d.add_bind("m", "b", "B", "", 12).unwrap();
        let mut st = Settings::default();
        let rows = binds::resolve(&d.binds, &st);
        assert!(rebind_row(&mut st, &rows, 1, 30));
        assert_eq!(binds::resolve(&d.binds, &st).iter().map(|r| r.key).collect::<Vec<_>>(), [11, 30]);
        assert!(!rebind_row(&mut st, &rows, 5, 30));
    }

    #[test]
    fn the_game_s_own_keys_are_rebound_directly_and_reset_clears_everything() {
        let mut st = Settings::default();
        assert!(rebind(&mut st, 0, 99));
        assert_eq!(st.keys[0], 99);
        st.mod_keys.push((String::from("m.a"), 30));
        reset(&mut st);
        assert_eq!(st.keys, Settings::default().keys);
        assert!(st.mod_keys.is_empty());
    }
}
