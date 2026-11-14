//! The rows mods add to Settings > GAME > MOD SWITCHES AND RELOAD: a toggle
//! for each switch a mod declared (`add_switch` in a script, `Native::switches`),
//! under a heading with the mod's name, scrolling when there are more than fit.
//! A switch is kept in the settings as the flag `<mod>.<id>`.

use crate::client::modui::binds::{self, Line};
use crate::client::modui::{keys, scroll, with_ui};
use crate::client::ui::{self, Ui};
use crate::settings::Settings;
use alloc::string::String;

/// Draw the switches in `lines` lines of room starting at (`x`, `y`), `w` wide.
/// Returns whether the player changed one.
pub fn draw(ui: &mut Ui, st: &mut Settings, x: f32, y: f32, w: f32, bh: f32, step: f32, lines: usize) -> bool {
    let s = ui.s;
    let switches = keys::all_switches();
    if switches.is_empty() || lines == 0 {
        return false;
    }
    let all = binds::switch_lines(&switches);
    let mut pos = with_ui(|u| u.scroll_switches).unwrap_or(0);
    if ui.hot(x, y, w, lines as f32 * step) {
        pos = scroll::wheel(pos, ui.inp.wheel, 1, all.len(), lines);
    }
    pos = scroll::clamp(pos, all.len(), lines);
    let bar = if all.len() > lines { 6.0 * s } else { 0.0 };
    let list_w = w - if bar > 0.0 { bar + 3.0 * s } else { 0.0 };
    let half = (list_w - 4.0 * s) * 0.5;
    let mut changed = false;
    for k in 0..lines {
        let Some(line) = all.get(pos as usize + k) else { break };
        let ly = y + k as f32 * step;
        match line {
            Line::Heading(text) => {
                ui.text(x + 2.0 * s, ly + 5.0 * s, text, ui::ACCENT);
            }
            Line::Pair(a, b) => {
                for (col, idx) in [Some(*a), *b].iter().enumerate() {
                    let Some(sw) = idx.and_then(|i| switches.get(i)) else { continue };
                    let cx = x + col as f32 * (half + 4.0 * s);
                    let mut on = binds::switch_on(&st.flags, &sw.mod_name, &sw.id, sw.default);
                    // A lone switch on its line is as wide as the list.
                    let width = if b.is_none() { list_w } else { half };
                    if ui.toggle(cx, ly, width, bh, &sw.label, &mut on) {
                        let name: String = binds::switch_flag(&sw.mod_name, &sw.id);
                        st.flags.retain(|f| f.0 != name);
                        st.flags.push((name, on));
                        changed = true;
                    }
                }
            }
        }
    }
    if bar > 0.0 {
        ui.scrollbar(x + w - bar, y, bar, lines as f32 * step - 3.0 * s, &mut pos, lines, all.len());
        pos = scroll::clamp(pos, all.len(), lines);
    }
    with_ui(|u| u.scroll_switches = pos);
    changed
}
