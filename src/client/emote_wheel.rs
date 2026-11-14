//! The emote wheel: hold the emote key (B), push the mouse toward an emote,
//! and let go (or click) to play it. Escape closes it without playing.
//! A quick tap of the key leaves the wheel open until a click or Escape.
//!
//! The mouse is in relative mode while the wheel is open, so the pointer
//! is a small dot the wheel keeps itself (`cursor`, in pixels from the centre).

use crate::client::ui::{Ui, ACCENT, BUTTON, BUTTON_HOT, DIM, KEY_ESCAPE};
use crate::math::{atan2f, cosf, sinf, TAU};
use crate::render::WHITE;
use alloc::string::String;

/// Most emotes shown at once; the mouse wheel turns the pages.
pub const PAGE: usize = 8;
/// A tap shorter than this many seconds leaves the wheel open.
const TAP: f32 = 0.3;

#[derive(Default)]
pub struct Wheel {
    /// The pointer, in pixels from the middle of the wheel.
    pub cursor: (f32, f32),
    opened: Option<f32>,
    /// The key was let go quickly: wait for a click.
    sticky: bool,
    pub page: usize,
}

/// What the wheel decided this frame.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Open,
    Cancel,
    /// Play this emote (its place in the list the wheel was given).
    Play(usize),
}

/// Which of `n` slices (the first straight up, going round clockwise) the
/// pointer `(dx, dy)` (screen pixels, y down) is in, if it is out of the
/// dead zone of radius `dead`.
pub fn slice_at(dx: f32, dy: f32, n: usize, dead: f32) -> Option<usize> {
    if n == 0 || dx * dx + dy * dy < dead * dead {
        return None;
    }
    let mut a = atan2f(dx, -dy);
    if a < 0.0 {
        a += TAU;
    }
    let width = TAU / n as f32;
    Some(((a + width * 0.5) / width) as usize % n)
}

/// The middle of slice `i` of `n`, as an offset from the centre for a
/// wheel of radii `rx`, `ry`.
pub fn slice_centre(i: usize, n: usize, rx: f32, ry: f32) -> (f32, f32) {
    let a = i as f32 * TAU / n.max(1) as f32;
    (sinf(a) * rx, -cosf(a) * ry)
}

impl Wheel {
    pub fn open(&mut self) {
        *self = Wheel::default();
    }

    /// How many pages `total` emotes make.
    pub fn pages(total: usize) -> usize {
        ((total + PAGE - 1) / PAGE).max(1)
    }

    /// Draw the wheel over the game and read what the player does with it.
    /// `names` are all the emotes; `key_held` whether the emote key is still
    /// down, `key_pressed` whether it went down again this frame.
    pub fn show(&mut self, ui: &mut Ui, names: &[String], key_held: bool, key_pressed: bool) -> Outcome {
        let s = ui.s;
        let opened = *self.opened.get_or_insert(ui.clock);
        let pages = Wheel::pages(names.len());
        if ui.inp.wheel != 0 {
            self.page = (self.page as i32 + ui.inp.wheel.signum()).rem_euclid(pages as i32) as usize;
        }
        self.page = self.page.min(pages - 1);
        let first = self.page * PAGE;
        let shown = &names[first.min(names.len())..(first + PAGE).min(names.len())];
        let n = shown.len();

        // The pointer moves with the mouse and stays near the wheel.
        let reach = 78.0 * s;
        self.cursor.0 += ui.inp.moved.0;
        self.cursor.1 += ui.inp.moved.1;
        let len = crate::math::sqrtf(self.cursor.0 * self.cursor.0 + self.cursor.1 * self.cursor.1);
        if len > reach {
            self.cursor = (self.cursor.0 / len * reach, self.cursor.1 / len * reach);
        }
        let hovered = slice_at(self.cursor.0, self.cursor.1, n, 16.0 * s);

        let (cx, cy) = (ui.w() * 0.5, ui.h() * 0.5);
        ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [6, 8, 14, 120]);
        let (rx, ry) = (92.0 * s, 62.0 * s);
        for (i, name) in shown.iter().enumerate() {
            let (ox, oy) = slice_centre(i, n, rx, ry);
            let tw = crate::render::Renderer::text_width(name, s);
            let (w, h) = (tw + 10.0 * s, 13.0 * s);
            let col = if hovered == Some(i) { BUTTON_HOT } else { BUTTON };
            ui.r.rect(cx + ox - w * 0.5, cy + oy - h * 0.5, w, h, col);
            ui.r.frame(cx + ox - w * 0.5, cy + oy - h * 0.5, w, h, s, [10, 11, 16, 255]);
            ui.text(cx + ox - tw * 0.5, cy + oy - 3.5 * s, name, if hovered == Some(i) { WHITE } else { DIM });
        }
        // The middle: what is chosen, or what to do.
        let title = match (hovered, n) {
            (_, 0) => String::from("NO EMOTES"),
            (Some(i), _) => shown[i].clone(),
            (None, _) => String::from("EMOTES"),
        };
        ui.centered(cx, cy - 4.0 * s, 1.0, &title, ACCENT);
        if pages > 1 {
            ui.centered(cx, cy + 8.0 * s, 1.0, &alloc::format!("PAGE {} OF {}  (MOUSE WHEEL)", self.page + 1, pages), DIM);
        }
        ui.centered(cx, cy + ry + 26.0 * s, 1.0, "LET GO OR CLICK TO PLAY  -  ESCAPE TO CLOSE", DIM);
        // The pointer.
        let (px, py) = (cx + self.cursor.0, cy + self.cursor.1);
        ui.r.rect(px - 2.0 * s, py - 2.0 * s, 4.0 * s, 4.0 * s, WHITE);

        if ui.inp.pressed(KEY_ESCAPE) || (self.sticky && key_pressed) {
            return Outcome::Cancel;
        }
        if ui.inp.clicked[1] {
            if let Some(i) = hovered {
                return Outcome::Play(first + i);
            }
        }
        if !key_held && !self.sticky {
            if let Some(i) = hovered {
                return Outcome::Play(first + i);
            }
            if ui.clock - opened < TAP {
                self.sticky = true;
            } else {
                return Outcome::Cancel;
            }
        }
        Outcome::Open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slices_go_round_clockwise_from_the_top() {
        // Four slices: up, right, down, left.
        assert_eq!(slice_at(0.0, -50.0, 4, 10.0), Some(0));
        assert_eq!(slice_at(50.0, 0.0, 4, 10.0), Some(1));
        assert_eq!(slice_at(0.0, 50.0, 4, 10.0), Some(2));
        assert_eq!(slice_at(-50.0, 0.0, 4, 10.0), Some(3));
        // Slightly left of straight up is still the first slice (it spans both sides).
        assert_eq!(slice_at(-10.0, -50.0, 4, 10.0), Some(0));
        // Eight slices: the diagonal is the second.
        assert_eq!(slice_at(40.0, -40.0, 8, 10.0), Some(1));
    }

    #[test]
    fn the_middle_selects_nothing() {
        assert_eq!(slice_at(3.0, -2.0, 6, 10.0), None);
        assert_eq!(slice_at(30.0, 0.0, 0, 10.0), None);
    }

    #[test]
    fn every_slice_is_found_where_it_is_drawn() {
        for n in 1..=PAGE {
            for i in 0..n {
                let (x, y) = slice_centre(i, n, 90.0, 90.0);
                assert_eq!(slice_at(x, y, n, 5.0), Some(i), "{i} of {n}");
            }
        }
    }

    #[test]
    fn emotes_come_in_pages() {
        assert_eq!(Wheel::pages(0), 1);
        assert_eq!(Wheel::pages(8), 1);
        assert_eq!(Wheel::pages(9), 2);
        assert_eq!(Wheel::pages(17), 3);
    }
}
