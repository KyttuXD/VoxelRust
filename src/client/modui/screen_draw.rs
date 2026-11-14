//! Painting a mod's own screen with the widgets the settings use, and turning
//! what the player does into [`UiEvent`]s. The model and its bounds are in
//! `screens.rs`; the scrolling arithmetic in `scroll.rs`.

use crate::client::modui::scroll;
use crate::client::modui::screens::{Open, Screens, UiEvent, UiKind, UiValue, Widget, INPUT_MAX, LIST_ROWS};
use crate::client::ui::{self, Ui};
use crate::render::WHITE;
use alloc::string::String;
use alloc::vec::Vec;

/// Text fields of mod screens have the focus ids `FOCUS_BASE + widget number`.
const FOCUS_BASE: u32 = 300;

/// Draw the open screen. Returns `true` when the player pressed CLOSE.
/// (Escape is handled by the window loop.)
pub fn draw(ui: &mut Ui, screens: &mut Screens) -> bool {
    let Some(o) = screens.open.as_mut() else { return true };
    let mut out: Vec<UiEvent> = Vec::new();
    let closed = draw_open(ui, o, &mut out);
    for e in out {
        screens.push_event(e);
    }
    closed
}

fn draw_open(ui: &mut Ui, o: &mut Open, out: &mut Vec<UiEvent>) -> bool {
    let s = ui.s;
    ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [8, 10, 18, 190]);
    let bw = (300.0 * s).min(ui.w() - 24.0 * s).max(60.0 * s);
    let x = (ui.w() - bw) * 0.5;
    let (bh, step) = (16.0 * s, 19.0 * s);
    let y0 = 54.0 * s;
    let title = o.title.clone();
    ui.centered(ui.w() * 0.5, 24.0 * s, 2.0, &title, WHITE);
    // Rows that fit between the title and the CLOSE button.
    let room = (((ui.h() - y0 - 40.0 * s) / step) as usize).max(2);
    let units: Vec<usize> = o.widgets.iter().map(|w| w.rows()).collect();
    let max_first = scroll::max_first(&units, room);
    let mut first = (o.scroll.max(0) as usize).min(max_first);
    let shown = scroll::fit(&units, first, room);
    ui.panel(x - 6.0 * s, y0 - 6.0 * s, bw + 12.0 * s, room as f32 * step + 8.0 * s);
    let more = o.widgets.len() > shown || first > 0;
    let bar = if more { 6.0 * s } else { 0.0 };
    let w = bw - if more { bar + 3.0 * s } else { 0.0 };

    let mut y = y0;
    let mut inner_wheel = false;
    for i in first..first + shown {
        let id = FOCUS_BASE + i as u32;
        let (mod_name, screen) = (o.mod_name.clone(), o.name.clone());
        let mut emit = |wid: &str, kind: UiKind| out.push(UiEvent { mod_name: mod_name.clone(), screen: screen.clone(), id: String::from(wid), kind });
        let rows = o.widgets[i].rows();
        match &mut o.widgets[i] {
            Widget::Text { text } => {
                let chars = (w / (crate::font::ADVANCE * s)) as usize;
                let t = ui::marquee(text, chars, ui.clock);
                ui.text(x + 2.0 * s, y + 4.0 * s, t, ui::DIM);
            }
            Widget::Button { id: wid, label } => {
                if ui.button(x, y, w, bh, label) {
                    emit(wid, UiKind::Click);
                }
            }
            Widget::Toggle { id: wid, label, value } => {
                if ui.toggle(x, y, w, bh, label, value) {
                    emit(wid, UiKind::Change(UiValue::Bool(*value)));
                }
            }
            Widget::Stepper { id: wid, label, value, min, max } => {
                if ui.stepper(x, y, w, bh, label, value, *min, *max, 1) {
                    emit(wid, UiKind::Change(UiValue::Int(*value as i64)));
                }
            }
            Widget::Input { id: wid, label, value } => {
                let lw = (crate::font::ADVANCE * s * (label.len() as f32 + 1.0)).min(w * 0.4);
                ui.text(x + 2.0 * s, y + 4.0 * s, label, ui::DIM);
                let before = value.clone();
                ui.text_field(id, x + lw, y, w - lw, bh, value, INPUT_MAX, ui::any_char);
                if *value != before {
                    emit(wid, UiKind::Change(UiValue::Text(value.clone())));
                }
            }
            Widget::List { id: wid, items, selected, scroll: pos } => {
                let total = items.len();
                let lshown = rows.min(LIST_ROWS);
                let hot = ui.hot(x, y, w + bar + 3.0 * s, lshown as f32 * step);
                if hot && total > lshown {
                    *pos = scroll::wheel(*pos, ui.inp.wheel, 1, total, lshown);
                    inner_wheel = true;
                }
                *pos = scroll::clamp(*pos, total, lshown);
                let lbar = if total > lshown { 6.0 * s } else { 0.0 };
                let lw = w - if lbar > 0.0 { lbar + 3.0 * s } else { 0.0 };
                if total == 0 {
                    ui.text(x + 2.0 * s, y + 4.0 * s, "(NOTHING IN THE LIST)", ui::DIM);
                }
                for k in 0..lshown {
                    let n = *pos as usize + k;
                    if n >= total {
                        break;
                    }
                    let ly = y + k as f32 * step;
                    if ui.button(x, ly, lw, bh, &items[n]) {
                        *selected = n as i32;
                        emit(wid, UiKind::Change(UiValue::Int(n as i64)));
                    }
                    if *selected == n as i32 {
                        ui.r.frame(x, ly, lw, bh, s, ui::ACCENT);
                    }
                }
                if lbar > 0.0 {
                    ui.scrollbar(x + w - lbar, y, lbar, lshown as f32 * step - 3.0 * s, pos, lshown, total);
                    *pos = scroll::clamp(*pos, total, lshown);
                }
            }
        }
        y += rows as f32 * step;
    }

    // The screen's own scrolling: the wheel over it, or its bar.
    if !inner_wheel && ui.hot(x - 6.0 * s, y0 - 6.0 * s, bw + 12.0 * s, room as f32 * step + 8.0 * s) {
        let moved = first as i32 - ui.inp.wheel.clamp(-20, 20);
        first = (moved.max(0) as usize).min(max_first);
    }
    if more {
        let mut pos = first as i32;
        ui.scrollbar(x + bw - bar, y0, bar, room as f32 * step - 3.0 * s, &mut pos, shown, o.widgets.len());
        first = (pos.max(0) as usize).min(max_first);
    }
    o.scroll = first as i32;
    ui.button(x, ui.h() - 30.0 * s, bw, bh, "CLOSE")
}
