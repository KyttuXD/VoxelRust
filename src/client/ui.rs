//! A small immediate-mode UI toolkit: every frame the screens call these
//! functions, which draw a widget and report whether it was used.

use crate::render::{Color, Renderer, WHITE};
use alloc::string::String;
use alloc::vec::Vec;

/// What the player did since the last frame.
#[derive(Default)]
pub struct UiInput {
    pub mouse: (f32, f32),
    /// Buttons pressed this frame: index 1 left, 2 middle, 3 right.
    pub clicked: [bool; 4],
    /// Buttons currently held down.
    pub down: [bool; 4],
    /// Mouse movement this frame, in pixels.
    pub moved: (f32, f32),
    pub wheel: i32,
    /// Characters typed this frame.
    pub text: String,
    /// Keys pressed this frame (SDL scancodes), repeats included.
    pub keys: Vec<i32>,
    /// Text pasted this frame (Ctrl or Cmd + V).
    pub paste: String,
}

impl UiInput {
    pub fn clear_frame(&mut self) {
        self.clicked = [false; 4];
        self.moved = (0.0, 0.0);
        self.wheel = 0;
        self.text.clear();
        self.keys.clear();
        self.paste.clear();
    }
    pub fn pressed(&self, scancode: i32) -> bool {
        self.keys.contains(&scancode)
    }
}

pub const KEY_RETURN: i32 = 40;
pub const KEY_ESCAPE: i32 = 41;
pub const KEY_BACKSPACE: i32 = 42;
pub const KEY_TAB: i32 = 43;
/// Not scancodes: what Ctrl or Cmd with A, C and X arrive as.
pub const KEY_SELECT_ALL: i32 = 1001;
pub const KEY_COPY: i32 = 1002;
pub const KEY_CUT: i32 = 1003;

pub const PANEL: Color = [18, 20, 28, 225];
pub const BUTTON: Color = [46, 50, 64, 235];
pub const BUTTON_HOT: Color = [72, 112, 196, 245];
pub const BUTTON_OFF: Color = [34, 36, 44, 200];
pub const DIM: Color = [170, 176, 192, 255];
pub const ACCENT: Color = [255, 222, 130, 255];

pub struct Ui<'a> {
    pub r: &'a mut Renderer,
    pub inp: &'a UiInput,
    /// Size of one UI pixel in screen pixels.
    pub s: f32,
    /// Which text field has the keyboard (0 = none).
    pub focus: &'a mut u32,
    /// Set when any widget was clicked, so the caller can play a sound.
    pub clicked_any: bool,
    /// Seconds since start, for labels that scroll.
    pub clock: f32,
    /// The focused text field is selected whole (Ctrl or Cmd + A): typing
    /// or pasting replaces it, Backspace empties it.
    pub selected: &'a mut bool,
}

/// The part of `label` to show in a space `chars` wide. Labels that fit are
/// returned whole; longer ones slide left to right and back, pausing at
/// each end, so every word can be read.
pub fn marquee(label: &str, chars: usize, clock: f32) -> &str {
    let len = label.len();
    if len <= chars || chars == 0 || !label.is_ascii() {
        return label;
    }
    let span = len - chars;
    // Two characters a second, with a second's pause at each end.
    let cycle = (span + 2) * 2;
    let t = (clock * 2.0) as usize % cycle;
    let pos = if t < span + 2 { t.saturating_sub(1).min(span) } else { span - (t - span - 2).saturating_sub(1).min(span) };
    &label[pos..pos + chars]
}

impl Ui<'_> {
    pub fn w(&self) -> f32 {
        self.r.w as f32
    }
    pub fn h(&self) -> f32 {
        self.r.h as f32
    }

    pub fn hot(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        let (mx, my) = self.inp.mouse;
        mx >= x && mx < x + w && my >= y && my < y + h
    }

    /// A scroll bar down the side of a list: `pos` is the first row shown
    /// of `total`, `shown` at a time. Drag it, or click the track, to move;
    /// the wheel is the list's own business. Draws nothing if all fits.
    pub fn scrollbar(&mut self, x: f32, y: f32, w: f32, h: f32, pos: &mut i32, shown: usize, total: usize) {
        if total <= shown || h <= 0.0 {
            return;
        }
        let max = (total - shown) as i32;
        self.r.rect(x, y, w, h, [18, 20, 28, 220]);
        let thumb = (h * shown as f32 / total as f32).max(8.0 * self.s);
        // Held anywhere on the track, the thumb goes to the pointer.
        let (mx, my) = self.inp.mouse;
        if self.inp.down[1] && mx >= x - 3.0 * self.s && mx < x + w + 3.0 * self.s && my >= y && my < y + h {
            let t = ((my - y - thumb * 0.5) / (h - thumb).max(1.0)).clamp(0.0, 1.0);
            *pos = (t * max as f32 + 0.5) as i32;
            self.clicked_any = true;
        }
        *pos = (*pos).clamp(0, max);
        let ty = y + (h - thumb) * (*pos as f32 / max as f32);
        let hot = self.hot(x - 3.0 * self.s, y, w + 6.0 * self.s, h);
        self.r.rect(x, ty, w, thumb, if hot { [150, 170, 230, 255] } else { [104, 112, 140, 255] });
        self.r.rect(x, ty, w, self.s, [190, 200, 230, 255]);
    }

    pub fn panel(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let s = self.s;
        // A soft shadow under it, a dark rim, a lighter inner line, and a
        // slightly lit top: a panel that sits on the screen.
        self.r.rect(x + 2.0 * s, y + 3.0 * s, w, h, [0, 0, 0, 70]);
        self.r.rect(x, y, w, h, PANEL);
        self.r.rect(x, y, w, 10.0 * s, [40, 44, 60, 90]);
        self.r.frame(x, y, w, h, s, [10, 11, 16, 255]);
        self.r.frame(x + s, y + s, w - 2.0 * s, h - 2.0 * s, s, [96, 104, 130, 255]);
    }

    pub fn text(&mut self, x: f32, y: f32, text: &str, col: Color) {
        self.r.label(x, y, self.s, text, col);
    }

    /// Text centred on `cx`, at `scale` times the normal size.
    pub fn centered(&mut self, cx: f32, y: f32, scale: f32, text: &str, col: Color) {
        let tw = Renderer::text_width(text, self.s * scale);
        self.r.label(cx - tw * 0.5, y, self.s * scale, text, col);
    }

    /// A button. Returns 1 for a left click, -1 for a right click, else 0.
    pub fn button_lr(&mut self, x: f32, y: f32, w: f32, h: f32, label: &str, enabled: bool) -> i32 {
        let hot = enabled && self.hot(x, y, w, h);
        let held = hot && (self.inp.down[1] || self.inp.down[3]);
        let s = self.s;
        // A raised plate: dark rim, lit top edge, shaded foot; it sinks a
        // little when pressed and glows when the pointer is on it.
        let face = if !enabled { BUTTON_OFF } else if held { [56, 90, 164, 250] } else if hot { BUTTON_HOT } else { BUTTON };
        let lift = |c: Color, d: i32| [(c[0] as i32 + d).clamp(0, 255) as u8, (c[1] as i32 + d).clamp(0, 255) as u8, (c[2] as i32 + d).clamp(0, 255) as u8, c[3]];
        self.r.rect(x, y, w, h, [12, 13, 18, 240]);
        self.r.rect(x + s, y + s, w - 2.0 * s, h - 2.0 * s, face);
        if enabled {
            self.r.rect(x + s, y + s, w - 2.0 * s, s, lift(face, if held { -18 } else { 34 }));
            self.r.rect(x + s, y + h - 2.0 * s, w - 2.0 * s, s, lift(face, if held { 20 } else { -22 }));
            self.r.rect(x + s, y + s, s, h - 2.0 * s, lift(face, 14));
            self.r.rect(x + w - 2.0 * s, y + s, s, h - 2.0 * s, lift(face, -14));
        }
        if hot {
            self.r.rect(x + 2.0 * s, y + h - 2.0 * s, w - 4.0 * s, s, ACCENT);
        }
        // Labels too long for the button scroll instead of spilling out.
        let room = ((w - 4.0 * self.s) / (crate::font::ADVANCE * self.s)) as usize;
        let label = marquee(label, room, self.clock);
        let tw = Renderer::text_width(label, self.s);
        let col = if enabled { WHITE } else { [130, 134, 146, 255] };
        let nudge = if held { self.s * 0.5 } else { 0.0 };
        self.r.label(x + (w - tw) * 0.5, y + (h - 7.0 * self.s) * 0.5 + nudge, self.s, label, col);
        if hot && (self.inp.clicked[1] || self.inp.clicked[3]) {
            self.clicked_any = true;
            return if self.inp.clicked[1] { 1 } else { -1 };
        }
        0
    }

    pub fn button(&mut self, x: f32, y: f32, w: f32, h: f32, label: &str) -> bool {
        self.button_lr(x, y, w, h, label, true) == 1
    }

    /// A value changed with the left and right mouse buttons or the wheel.
    /// Returns whether it changed.
    pub fn stepper(&mut self, x: f32, y: f32, w: f32, h: f32, label: &str, v: &mut i32, min: i32, max: i32, step: i32) -> bool {
        let shown = alloc::format!("{}  < {} >", label, *v);
        self.stepper_text(x, y, w, h, &shown, v, min, max, step)
    }

    /// Like [`Ui::stepper`], but the caller supplies the whole label.
    pub fn stepper_text(&mut self, x: f32, y: f32, w: f32, h: f32, shown: &str, v: &mut i32, min: i32, max: i32, step: i32) -> bool {
        let mut d = self.button_lr(x, y, w, h, shown, true);
        if d == 0 && self.hot(x, y, w, h) {
            d = self.inp.wheel.signum();
        }
        let nv = (*v + d * step).clamp(min, max);
        let changed = nv != *v;
        *v = nv;
        changed
    }

    pub fn toggle(&mut self, x: f32, y: f32, w: f32, h: f32, label: &str, v: &mut bool) -> bool {
        let shown = alloc::format!("{}  {}", label, if *v { "ON" } else { "OFF" });
        if self.button_lr(x, y, w, h, &shown, true) != 0 {
            *v = !*v;
            return true;
        }
        false
    }

    /// A one-line text box. Click to focus; returns true when Enter is
    /// pressed while it has the focus.
    pub fn text_field(&mut self, id: u32, x: f32, y: f32, w: f32, h: f32, value: &mut String, max: usize, allow: fn(char) -> bool) -> bool {
        if self.inp.clicked[1] {
            if self.hot(x, y, w, h) {
                *self.focus = id;
                self.clicked_any = true;
            } else if *self.focus == id {
                *self.focus = 0;
            }
        }
        let focused = *self.focus == id;
        let mut entered = false;
        if focused {
            let typed: String = self.inp.text.chars().chain(self.inp.paste.chars().filter(|c| *c != '\n' && *c != '\r')).collect();
            for c in typed.chars() {
                if !allow(c) {
                    continue;
                }
                if *self.selected {
                    value.clear();
                    *self.selected = false;
                }
                if value.chars().count() < max {
                    value.push(c);
                }
            }
            for &k in &self.inp.keys {
                match k {
                    KEY_BACKSPACE if *self.selected => {
                        value.clear();
                        *self.selected = false;
                    }
                    KEY_BACKSPACE => {
                        value.pop();
                    }
                    KEY_RETURN => entered = true,
                    KEY_SELECT_ALL => *self.selected = !value.is_empty(),
                    // With nothing selected, copy and cut take the whole box.
                    KEY_COPY => crate::sys::set_clipboard(value),
                    KEY_CUT => {
                        crate::sys::set_clipboard(value);
                        value.clear();
                        *self.selected = false;
                    }
                    _ => {}
                }
            }
        }
        // Sunk into the page: dark well, shaded top, a rim that lights
        // up when it has the keyboard.
        self.r.rect(x, y, w, h, [8, 8, 12, 240]);
        self.r.rect(x, y, w, 2.0 * self.s, [0, 0, 0, 120]);
        self.r.frame(x, y, w, h, self.s, if focused { ACCENT } else { [84, 90, 112, 255] });
        let mut shown = value.clone();
        if focused && *self.selected {
            let tw = Renderer::text_width(&shown, self.s);
            self.r.rect(x + 3.0 * self.s, y + 2.0 * self.s, tw + 2.0 * self.s, h - 4.0 * self.s, [60, 100, 190, 255]);
        } else if focused {
            shown.push('_');
        }
        self.r.label(x + 4.0 * self.s, y + (h - 7.0 * self.s) * 0.5, self.s, &shown, WHITE);
        entered
    }
}

#[cfg(test)]
mod tests {
    use super::marquee;

    #[test]
    fn long_labels_scroll_through_every_character_and_short_ones_stay() {
        assert_eq!(marquee("SHORT", 10, 3.0), "SHORT");
        let label = "JUMP / FLY UP: LEFT SHIFT";
        let mut seen_start = false;
        let mut seen_end = false;
        for i in 0..400 {
            let part = marquee(label, 12, i as f32 * 0.1);
            assert_eq!(part.len(), 12);
            assert!(label.contains(part));
            seen_start |= label.starts_with(part);
            seen_end |= label.ends_with(part);
        }
        assert!(seen_start && seen_end);
        // It rests at the start for a moment.
        assert_eq!(marquee(label, 12, 0.0), marquee(label, 12, 0.4));
    }
}

/// Characters allowed in names of worlds and players.
pub fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Characters allowed in free text such as seeds, addresses and chat.
pub fn any_char(c: char) -> bool {
    c.is_ascii_graphic() || c == ' '
}
