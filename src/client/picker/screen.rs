//! Drawing the file picker and handling its input.

use super::listing::parent;
use super::{jump_to_letter, path_tail, preview_pixels, scroll_to_show, Look, Outcome, Picker};
use crate::render::{Renderer, WHITE};
use crate::ui::{self, Ui};
use alloc::string::String;
use alloc::vec::Vec;

impl Picker {
    /// Read the picture under the cursor into the preview when it changed.
    fn refresh_preview(&mut self, ui: &mut Ui) {
        let path = self.selected_file();
        if path == self.shown {
            return;
        }
        self.shown = path.clone();
        self.look = Look::None;
        let path = match path {
            Some(p) => p,
            None => return,
        };
        let img = crate::sys::read_file(&path).and_then(|b| crate::image::decode(&b));
        let img = match img {
            Some(i) => i,
            None => {
                self.look = Look::Unreadable;
                return;
            }
        };
        let (px, _, _) = preview_pixels(&img);
        if self.tex == 0 {
            self.tex = ui.r.add_texture(64, 64, &px);
        } else {
            ui.r.update_texture(self.tex, 64, 64, &px);
        }
        self.look = Look::Size(img.w, img.h);
    }

    /// Draw the picker and handle this frame's input.
    pub fn show(&mut self, ui: &mut Ui) -> Outcome {
        let s = ui.s;
        let (sw, sh) = (ui.w(), ui.h());
        let m = 6.0 * s;
        let (px, py, pw, ph) = (m, m, sw - 2.0 * m, sh - 2.0 * m);
        ui.panel(px, py, pw, ph);
        let ix = px + 6.0 * s;
        let iw = pw - 12.0 * s;
        let adv = crate::font::ADVANCE * s;

        // Title and the path bar.
        ui.text(ix, py + 6.0 * s, &self.title, WHITE);
        let bar_y = py + 17.0 * s;
        ui.r.rect(ix, bar_y, iw, 13.0 * s, [8, 8, 12, 240]);
        ui.r.frame(ix, bar_y, iw, 13.0 * s, s, [84, 90, 112, 255]);
        let shown_path = if self.dir.is_empty() { String::from("(CURRENT FOLDER)") } else { self.dir.clone() };
        let fit = ((iw - 8.0 * s) / adv) as usize;
        ui.text(ix + 4.0 * s, bar_y + 3.0 * s, &path_tail(&shown_path, fit), ui::ACCENT);

        // UP and the shortcuts, sharing the row.
        let row_y = bar_y + 17.0 * s;
        let n = 1 + self.shortcuts.len();
        let gap = 3.0 * s;
        let bw = (iw - gap * (n as f32 - 1.0)) / n as f32;
        let can_up = parent(&self.dir).is_some();
        if ui.button_lr(ix, row_y, bw, 14.0 * s, "UP", can_up) == 1 {
            self.up();
        }
        let mut goto: Option<String> = None;
        for (k, (label, path)) in self.shortcuts.iter().enumerate() {
            // Short names when the full ones do not fit the button.
            let fits = Renderer::text_width(label, s) + 4.0 * s <= bw;
            let label = if fits { label.as_str() } else { label.get(..4).unwrap_or(label.as_str()) };
            if ui.button(ix + (k + 1) as f32 * (bw + gap), row_y, bw, 14.0 * s, label) {
                goto = Some(path.clone());
            }
        }
        if let Some(p) = goto {
            self.go(&p, None);
        }

        // Layout of the list, the preview beside it, and the bottom row.
        let wide = sw / s >= 330.0;
        let pv_w = if wide { 88.0 * s } else { 0.0 };
        let list_x = ix;
        let list_y = row_y + 19.0 * s;
        let bottom_h = 20.0 * s + if wide { 0.0 } else { 11.0 * s };
        let list_h = (py + ph - 6.0 * s - bottom_h - list_y).max(11.0 * s);
        let list_w = iw - pv_w - if wide { 4.0 * s } else { 0.0 };
        let row_h = 11.0 * s;
        let rows = ((list_h / row_h) as usize).max(1);
        let bar_w = 6.0 * s;

        // Keyboard.
        let mut outcome = Outcome::Open;
        let keys = ui.inp.keys.clone();
        for k in keys {
            match k {
                82 => self.move_sel(-1, rows),
                81 => self.move_sel(1, rows),
                75 => self.move_sel(-(rows as i32), rows),
                78 => self.move_sel(rows as i32, rows),
                74 => self.move_sel(-(self.entries.len() as i32), rows),
                77 => self.move_sel(self.entries.len() as i32, rows),
                ui::KEY_BACKSPACE | 80 => self.up(),
                ui::KEY_RETURN | 79 => {
                    if k == 79 && !self.sel.and_then(|i| self.entries.get(i)).map_or(false, |e| e.dir) {
                        continue;
                    }
                    let o = self.activate();
                    if o != Outcome::Open {
                        outcome = o;
                    }
                }
                _ => {}
            }
        }
        // Typing a letter jumps to the next name that starts with it.
        let typed: Vec<char> = ui.inp.text.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        for c in typed {
            if let Some(i) = jump_to_letter(&self.entries, self.sel, c) {
                self.sel = Some(i);
                self.scroll = scroll_to_show(self.scroll, i, rows, self.entries.len());
            }
        }

        // The list: a sunken well, rows, wheel and scroll bar.
        let total = self.entries.len();
        ui.r.rect(list_x, list_y, list_w, list_h, [8, 8, 12, 240]);
        ui.r.frame(list_x, list_y, list_w, list_h, s, [84, 90, 112, 255]);
        if ui.hot(list_x, list_y, list_w, list_h) && ui.inp.wheel != 0 {
            self.scroll = (self.scroll as i32 - ui.inp.wheel * 3).clamp(0, total.saturating_sub(rows) as i32) as usize;
        }
        self.scroll = self.scroll.min(total.saturating_sub(rows));
        let text_w = list_w - 8.0 * s - if total > rows { bar_w } else { 0.0 };
        let chars = (text_w / adv) as usize;
        let mut clicked_row: Option<usize> = None;
        for k in 0..rows {
            let i = self.scroll + k;
            if i >= total {
                break;
            }
            let ry = list_y + s + k as f32 * row_h;
            let rw = list_w - 2.0 * s - if total > rows { bar_w } else { 0.0 };
            let hot = ui.hot(list_x + s, ry, rw, row_h);
            if self.sel == Some(i) {
                ui.r.rect(list_x + s, ry, rw, row_h, [60, 100, 190, 255]);
            } else if hot {
                ui.r.rect(list_x + s, ry, rw, row_h, [40, 46, 66, 255]);
            }
            if hot && ui.inp.clicked[1] {
                clicked_row = Some(i);
            }
            let e = &self.entries[i];
            let label = if e.dir { alloc::format!("> {}", e.name) } else { e.name.clone() };
            let label = if self.sel == Some(i) { String::from(ui::marquee(&label, chars, ui.clock)) } else { label.chars().take(chars).collect() };
            ui.text(list_x + 4.0 * s, ry + 2.0 * s, &label, if e.dir { ui::ACCENT } else { WHITE });
        }
        if total == 0 {
            let msg = if self.note.is_empty() { "NOTHING TO CHOOSE HERE" } else { self.note.as_str() };
            ui.text(list_x + 6.0 * s, list_y + 6.0 * s, msg, ui::DIM);
        }
        if total > rows {
            let mut pos = self.scroll as i32;
            ui.scrollbar(list_x + list_w - bar_w - s, list_y + s, bar_w, list_h - 2.0 * s, &mut pos, rows, total);
            self.scroll = pos as usize;
        }
        if let Some(i) = clicked_row {
            let now = ui.clock;
            let double = matches!(self.last_click, Some((j, t)) if j == i && now - t < 0.45);
            self.sel = Some(i);
            ui.clicked_any = true;
            if double {
                self.last_click = None;
                let o = self.activate();
                if o != Outcome::Open {
                    outcome = o;
                }
            } else {
                self.last_click = Some((i, now));
            }
        }

        self.refresh_preview(ui);

        // The preview and what is known about the picture.
        let info_line = match (self.look, self.sel_name()) {
            (Look::Size(w, h), Some(_)) => alloc::format!("{} X {}", w, h),
            (Look::Unreadable, Some(_)) => String::from("CANNOT READ THIS PICTURE"),
            _ => String::new(),
        };
        let size_bad = match (self.look, self.size_ok) {
            (Look::Size(w, h), Some(f)) => !f(w, h),
            (Look::Unreadable, _) => true,
            _ => false,
        };
        let info_col = if size_bad { [255, 120, 110, 255] } else { [150, 230, 140, 255] };
        if wide {
            let bx = list_x + list_w + 4.0 * s;
            ui.r.rect(bx, list_y, pv_w, list_h, [8, 8, 12, 240]);
            ui.r.frame(bx, list_y, pv_w, list_h, s, [84, 90, 112, 255]);
            let room = (pv_w - 8.0 * s).min(list_h - 30.0 * s).max(8.0 * s);
            // A whole number of screen pixels per picture pixel when it fits.
            let side = if room >= 64.0 { ((room / 64.0) as i32).max(1) as f32 * 64.0 } else { room };
            let (ix2, iy2) = (bx + (pv_w - side) * 0.5, list_y + 4.0 * s);
            if matches!(self.look, Look::Size(..)) && self.tex != 0 {
                // A chequer behind it shows what is see-through.
                let cell = side / 8.0;
                for cy in 0..8 {
                    for cx in 0..8 {
                        let c = if (cx + cy) % 2 == 0 { [44, 48, 60, 255] } else { [60, 64, 78, 255] };
                        ui.r.rect(ix2 + cx as f32 * cell, iy2 + cy as f32 * cell, cell, cell, c);
                    }
                }
                ui.r.image(self.tex, ix2, iy2, side, side);
            } else {
                ui.r.frame(ix2, iy2, side, side, s, [60, 64, 78, 255]);
            }
            let ty = iy2 + side + 4.0 * s;
            let cw = ((pv_w - 6.0 * s) / adv) as usize;
            if let Some(name) = self.sel_name() {
                ui.text(bx + 3.0 * s, ty, ui::marquee(&name, cw, ui.clock), WHITE);
            }
            if !info_line.is_empty() {
                ui.text(bx + 3.0 * s, ty + 9.0 * s, &info_line, info_col);
            }
            if !self.hint.is_empty() && ty + 27.0 * s < list_y + list_h {
                ui.text(bx + 3.0 * s, ty + 18.0 * s, ui::marquee(&self.hint, cw, ui.clock), ui::DIM);
            }
        }

        // The bottom row: CANCEL and SELECT, with the picture's size when
        // there is no room beside the list for the preview.
        let by = py + ph - 6.0 * s - 16.0 * s;
        if !wide {
            let name = self.sel_name().unwrap_or_default();
            let line = if name.is_empty() { self.hint.clone() } else { alloc::format!("{}  {}", name, info_line) };
            let cw = (iw / adv) as usize;
            ui.text(ix, by - 11.0 * s, ui::marquee(&line, cw, ui.clock), if size_bad { info_col } else { ui::DIM });
        }
        let half = (iw - 4.0 * s) * 0.5;
        if ui.button(ix, by, half, 16.0 * s, "CANCEL") {
            outcome = Outcome::Cancelled;
        }
        let file = self.selected_file();
        let on_dir = self.sel.and_then(|i| self.entries.get(i)).map_or(false, |e| e.dir);
        let label = if on_dir { "OPEN FOLDER" } else { "SELECT" };
        if ui.button_lr(ix + half + 4.0 * s, by, half, 16.0 * s, label, file.is_some() || on_dir) == 1 {
            let o = self.activate();
            if o != Outcome::Open {
                outcome = o;
            }
        }
        outcome
    }

    fn sel_name(&self) -> Option<String> {
        let e = self.entries.get(self.sel?)?;
        if e.dir { None } else { Some(e.name.clone()) }
    }
}
