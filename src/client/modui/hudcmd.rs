//! The HUD drawing commands a mod may give, and how they become pixels.
//!
//! A mod never draws itself. Each frame it adds [`Cmd`]s to a [`CmdBuf`]
//! (text, filled rectangle, outline, item icon, block icon, progress bar,
//! each at an [`Anchor`] of the screen); the buffer refuses anything past
//! its cap. Later `place` turns each command into [`Prim`]s in screen
//! pixels, scaled by the interface scale and clipped to the screen, and
//! `hud_draw.rs` paints those. Everything here is arithmetic on numbers, so
//! it is tested without a window.

use crate::client::modui::limits::*;
use crate::render::Color;
use alloc::string::String;
use alloc::vec::Vec;

/// A point of the screen that a command is placed against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Centre,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// `top-left`, `top`, `top-right`, `left`, `centre` (or `center`),
    /// `right`, `bottom-left`, `bottom`, `bottom-right`; case, `-`, `_` and
    /// spaces do not matter.
    pub fn parse(name: &str) -> Option<Anchor> {
        let n: String = name.chars().filter(|c| !matches!(c, '-' | '_' | ' ')).map(|c| c.to_ascii_lowercase()).collect();
        Some(match n.as_str() {
            "topleft" | "tl" => Anchor::TopLeft,
            "top" => Anchor::Top,
            "topright" | "tr" => Anchor::TopRight,
            "left" => Anchor::Left,
            "centre" | "center" | "middle" => Anchor::Centre,
            "right" => Anchor::Right,
            "bottomleft" | "bl" => Anchor::BottomLeft,
            "bottom" => Anchor::Bottom,
            "bottomright" | "br" => Anchor::BottomRight,
            _ => return None,
        })
    }

    /// How far across and down (0, 0.5 or 1) the anchor is, both for the
    /// screen and for the element placed on it.
    pub fn fractions(self) -> (f32, f32) {
        match self {
            Anchor::TopLeft => (0.0, 0.0),
            Anchor::Top => (0.5, 0.0),
            Anchor::TopRight => (1.0, 0.0),
            Anchor::Left => (0.0, 0.5),
            Anchor::Centre => (0.5, 0.5),
            Anchor::Right => (1.0, 0.5),
            Anchor::BottomLeft => (0.0, 1.0),
            Anchor::Bottom => (0.5, 1.0),
            Anchor::BottomRight => (1.0, 1.0),
        }
    }
}

/// One drawing command. Distances (`dx`, `dy`, sizes) are in interface
/// pixels: they are multiplied by the interface scale. The element is put
/// so that the point of it matching the anchor (its top-right corner for
/// `top-right`, its middle for `centre`...) is on the anchor's point of
/// the screen, and then moved `dx` right and `dy` down.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Text { at: Anchor, dx: f32, dy: f32, scale: f32, text: String, col: Color },
    Rect { at: Anchor, dx: f32, dy: f32, w: f32, h: f32, col: Color },
    Outline { at: Anchor, dx: f32, dy: f32, w: f32, h: f32, thick: f32, col: Color },
    Item { at: Anchor, dx: f32, dy: f32, size: f32, id: String },
    Block { at: Anchor, dx: f32, dy: f32, size: f32, id: String },
    Bar { at: Anchor, dx: f32, dy: f32, w: f32, h: f32, frac: f32, col: Color },
}

/// What is left to paint, in screen pixels.
#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    Rect { x: f32, y: f32, w: f32, h: f32, col: Color },
    /// `scale` is the final size of a font pixel (already times the interface scale).
    Text { x: f32, y: f32, scale: f32, text: String, col: Color },
    Item { x: f32, y: f32, size: f32, id: String },
    Block { x: f32, y: f32, size: f32, id: String },
}

/// The commands of one frame, with the cap.
#[derive(Debug, Default)]
pub struct CmdBuf {
    pub cmds: Vec<Cmd>,
    /// Who gave each command (parallel to `cmds`).
    owners: Vec<String>,
    /// Commands refused this frame (over a cap).
    pub refused: u32,
}

impl CmdBuf {
    /// Start a frame: forget everything.
    pub fn clear(&mut self) {
        self.cmds.clear();
        self.owners.clear();
        self.refused = 0;
    }

    /// Add a command from `owner`. `false` when the frame's cap, or the
    /// owner's share of it, is used up.
    pub fn push(&mut self, owner: &str, cmd: Cmd) -> bool {
        if self.cmds.len() >= MAX_HUD_CMDS || self.owners.iter().filter(|o| o.as_str() == owner).count() >= MAX_HUD_CMDS_PER_MOD {
            self.refused = self.refused.saturating_add(1);
            return false;
        }
        self.cmds.push(cmd);
        self.owners.push(String::from(owner));
        true
    }

    pub fn len(&self) -> usize {
        self.cmds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }
}

/// A colour from a script's number: `0xRRGGBB` is opaque, `0xAARRGGBB`
/// (a number above `0xFFFFFF`) carries its own alpha.
pub fn colour(n: i64) -> Color {
    let n = n.clamp(0, 0xFFFF_FFFF) as u32;
    let a = if n > 0x00FF_FFFF { (n >> 24) as u8 } else { 255 };
    [(n >> 16) as u8, (n >> 8) as u8, n as u8, a]
}

/// A number made safe to use as a size or distance: finite, within +-`lim`.
pub fn num(v: f64, lim: f32) -> f32 {
    if v != v {
        return 0.0;
    }
    let v = if v > lim as f64 { lim as f64 } else if v < -(lim as f64) { -(lim as f64) } else { v };
    v as f32
}

/// The screen and the interface scale commands are placed against, and
/// how wide a piece of text is at a scale (`measure(text, final_scale)`).
pub struct Frame<'a> {
    pub w: f32,
    pub h: f32,
    pub s: f32,
    pub measure: &'a dyn Fn(&str, f32) -> f32,
}

/// The part of a rectangle inside the screen, if any.
pub fn clip_rect(x: f32, y: f32, w: f32, h: f32, sw: f32, sh: f32) -> Option<(f32, f32, f32, f32)> {
    let (x0, y0) = (if x < 0.0 { 0.0 } else { x }, if y < 0.0 { 0.0 } else { y });
    let (x1, y1) = (if x + w > sw { sw } else { x + w }, if y + h > sh { sh } else { y + h });
    if w <= 0.0 || h <= 0.0 || x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0, y0, x1 - x0, y1 - y0))
}

fn rect_prim(out: &mut Vec<Prim>, f: &Frame, x: f32, y: f32, w: f32, h: f32, col: Color) {
    if col[3] == 0 {
        return;
    }
    if let Some((x, y, w, h)) = clip_rect(x, y, w, h, f.w, f.h) {
        out.push(Prim::Rect { x, y, w, h, col });
    }
}

/// The top-left of an element of size (`w`, `h`) put at `at` of the screen,
/// moved by (`dx`, `dy`) interface pixels.
pub fn origin(f: &Frame, at: Anchor, dx: f32, dy: f32, w: f32, h: f32) -> (f32, f32) {
    let (fx, fy) = at.fractions();
    (f.w * fx - w * fx + dx * f.s, f.h * fy - h * fy + dy * f.s)
}

/// Turn a command into what is to be painted, scaled and clipped. Text is
/// shortened to what fits before the screen's right edge and dropped when it
/// is off the screen; icons are painted whole or not at all.
pub fn place(cmd: &Cmd, f: &Frame, out: &mut Vec<Prim>) {
    match cmd {
        Cmd::Text { at, dx, dy, scale, text, col } => {
            let sc = scale.clamp(0.5, 4.0) * f.s;
            let full = (f.measure)(text, sc);
            let (x, y) = origin(f, *at, *dx, *dy, full, 7.0 * sc);
            if col[3] == 0 || text.is_empty() || y + 7.0 * sc <= 0.0 || y >= f.h || x >= f.w || x + full <= 0.0 {
                return;
            }
            // Cut at the right edge (a character at a time).
            let mut shown = String::from(text.as_str());
            while !shown.is_empty() && x + (f.measure)(&shown, sc) > f.w {
                shown.pop();
            }
            if !shown.is_empty() {
                out.push(Prim::Text { x, y, scale: sc, text: shown, col: *col });
            }
        }
        Cmd::Rect { at, dx, dy, w, h, col } => {
            let (w, h) = (w.clamp(0.0, 4096.0) * f.s, h.clamp(0.0, 4096.0) * f.s);
            let (x, y) = origin(f, *at, *dx, *dy, w, h);
            rect_prim(out, f, x, y, w, h, *col);
        }
        Cmd::Outline { at, dx, dy, w, h, thick, col } => {
            let (w, h) = (w.clamp(0.0, 4096.0) * f.s, h.clamp(0.0, 4096.0) * f.s);
            let t = (thick.clamp(1.0, 16.0) * f.s).min(w * 0.5).min(h * 0.5);
            let (x, y) = origin(f, *at, *dx, *dy, w, h);
            if w <= 0.0 || h <= 0.0 {
                return;
            }
            rect_prim(out, f, x, y, w, t, *col);
            rect_prim(out, f, x, y + h - t, w, t, *col);
            rect_prim(out, f, x, y + t, t, h - 2.0 * t, *col);
            rect_prim(out, f, x + w - t, y + t, t, h - 2.0 * t, *col);
        }
        Cmd::Item { at, dx, dy, size, id } | Cmd::Block { at, dx, dy, size, id } => {
            let size = size.clamp(4.0, 256.0) * f.s;
            let (x, y) = origin(f, *at, *dx, *dy, size, size);
            // A picture cannot be cut: it is drawn if it is wholly on the screen.
            if x < 0.0 || y < 0.0 || x + size > f.w || y + size > f.h {
                return;
            }
            if matches!(cmd, Cmd::Item { .. }) {
                out.push(Prim::Item { x, y, size, id: id.clone() });
            } else {
                out.push(Prim::Block { x, y, size, id: id.clone() });
            }
        }
        Cmd::Bar { at, dx, dy, w, h, frac, col } => {
            let (w, h) = (w.clamp(0.0, 4096.0) * f.s, h.clamp(0.0, 4096.0) * f.s);
            let (x, y) = origin(f, *at, *dx, *dy, w, h);
            let frac = if *frac != *frac { 0.0 } else { frac.clamp(0.0, 1.0) };
            // A dark well with the fill inside it.
            rect_prim(out, f, x, y, w, h, [0, 0, 0, 160]);
            let m = f.s.min(w * 0.25).min(h * 0.25);
            rect_prim(out, f, x + m, y + m, (w - 2.0 * m) * frac, h - 2.0 * m, *col);
        }
    }
}

/// All the commands of a frame, placed.
pub fn place_all(cmds: &[Cmd], f: &Frame) -> Vec<Prim> {
    let mut out = Vec::new();
    for c in cmds {
        place(c, f, &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Six font pixels a character.
    fn m(t: &str, sc: f32) -> f32 {
        t.len() as f32 * 6.0 * sc
    }

    fn frame<'a>(measure: &'a dyn Fn(&str, f32) -> f32) -> Frame<'a> {
        Frame { w: 400.0, h: 200.0, s: 2.0, measure }
    }

    #[test]
    fn anchors_parse_in_any_spelling() {
        assert_eq!(Anchor::parse("top-left"), Some(Anchor::TopLeft));
        assert_eq!(Anchor::parse("TOP_RIGHT"), Some(Anchor::TopRight));
        assert_eq!(Anchor::parse("Center"), Some(Anchor::Centre));
        assert_eq!(Anchor::parse("centre"), Some(Anchor::Centre));
        assert_eq!(Anchor::parse("bottom right"), Some(Anchor::BottomRight));
        assert_eq!(Anchor::parse("bottom"), Some(Anchor::Bottom));
        assert_eq!(Anchor::parse("left"), Some(Anchor::Left));
        assert_eq!(Anchor::parse("sideways"), None);
        assert_eq!(Anchor::parse(""), None);
    }

    #[test]
    fn an_element_meets_its_own_corner_at_the_anchor() {
        let mm = m;
        let f = frame(&mm);
        // 10 x 4 interface pixels = 20 x 8 screen pixels.
        assert_eq!(origin(&f, Anchor::TopLeft, 0.0, 0.0, 20.0, 8.0), (0.0, 0.0));
        assert_eq!(origin(&f, Anchor::TopRight, 0.0, 0.0, 20.0, 8.0), (380.0, 0.0));
        assert_eq!(origin(&f, Anchor::BottomRight, -1.0, -1.0, 20.0, 8.0), (378.0, 190.0));
        assert_eq!(origin(&f, Anchor::Centre, 0.0, 0.0, 20.0, 8.0), (190.0, 96.0));
        assert_eq!(origin(&f, Anchor::Bottom, 5.0, 0.0, 20.0, 8.0), (200.0, 192.0));
        assert_eq!(origin(&f, Anchor::Left, 3.0, 0.0, 20.0, 8.0), (6.0, 96.0));
    }

    #[test]
    fn a_rectangle_is_scaled_and_clipped_to_the_screen() {
        let mm = m;
        let f = frame(&mm);
        let mut out = Vec::new();
        place(&Cmd::Rect { at: Anchor::TopLeft, dx: 5.0, dy: 5.0, w: 10.0, h: 10.0, col: [1, 2, 3, 255] }, &f, &mut out);
        assert_eq!(out, [Prim::Rect { x: 10.0, y: 10.0, w: 20.0, h: 20.0, col: [1, 2, 3, 255] }]);
        // Half off the left edge: the part that is on.
        out.clear();
        place(&Cmd::Rect { at: Anchor::TopLeft, dx: -5.0, dy: 0.0, w: 10.0, h: 10.0, col: [1, 2, 3, 255] }, &f, &mut out);
        assert_eq!(out, [Prim::Rect { x: 0.0, y: 0.0, w: 10.0, h: 20.0, col: [1, 2, 3, 255] }]);
        // Wholly off the screen, empty, or invisible: nothing.
        for (dx, w, a) in [(500.0, 10.0, 255u8), (0.0, 0.0, 255), (0.0, -3.0, 255), (0.0, 10.0, 0)] {
            out.clear();
            place(&Cmd::Rect { at: Anchor::TopLeft, dx, dy: 0.0, w, h: 10.0, col: [1, 2, 3, a] }, &f, &mut out);
            assert!(out.is_empty(), "{} {} {}", dx, w, a);
        }
        // A huge size is cut to the screen, not trusted.
        out.clear();
        place(&Cmd::Rect { at: Anchor::Centre, dx: 0.0, dy: 0.0, w: 1.0e30, h: 1.0e30, col: [1, 2, 3, 255] }, &f, &mut out);
        assert_eq!(out, [Prim::Rect { x: 0.0, y: 0.0, w: 400.0, h: 200.0, col: [1, 2, 3, 255] }]);
    }

    #[test]
    fn an_outline_is_four_thin_rectangles() {
        let mm = m;
        let f = frame(&mm);
        let mut out = Vec::new();
        place(&Cmd::Outline { at: Anchor::TopLeft, dx: 0.0, dy: 0.0, w: 10.0, h: 10.0, thick: 1.0, col: [9, 9, 9, 255] }, &f, &mut out);
        assert_eq!(out.len(), 4);
        // The four cover 20 x 20 minus the 16 x 16 hole.
        let area: f32 = out.iter().map(|p| if let Prim::Rect { w, h, .. } = p { w * h } else { 0.0 }).sum();
        assert_eq!(area, 20.0 * 20.0 - 16.0 * 16.0);
        // Too thick for its size: it becomes a filled box without overlap.
        out.clear();
        place(&Cmd::Outline { at: Anchor::TopLeft, dx: 0.0, dy: 0.0, w: 4.0, h: 4.0, thick: 16.0, col: [9, 9, 9, 255] }, &f, &mut out);
        let area: f32 = out.iter().map(|p| if let Prim::Rect { w, h, .. } = p { w * h } else { 0.0 }).sum();
        assert_eq!(area, 8.0 * 8.0);
    }

    #[test]
    fn text_is_placed_scaled_and_shortened_at_the_edge() {
        let mm = m;
        let f = frame(&mm);
        let mut out = Vec::new();
        // 5 characters at scale 1 * s 2: 6 * 2 = 12 a character, 60 wide, 14 high.
        place(&Cmd::Text { at: Anchor::TopRight, dx: -2.0, dy: 1.0, scale: 1.0, text: String::from("HELLO"), col: [255, 255, 255, 255] }, &f, &mut out);
        assert_eq!(out, [Prim::Text { x: 400.0 - 60.0 - 4.0, y: 2.0, scale: 2.0, text: String::from("HELLO"), col: [255, 255, 255, 255] }]);
        // Starting near the right edge, only what fits is kept.
        out.clear();
        place(&Cmd::Text { at: Anchor::TopLeft, dx: 185.0, dy: 0.0, scale: 1.0, text: String::from("HELLO"), col: [255, 255, 255, 255] }, &f, &mut out);
        match &out[..] {
            [Prim::Text { text, .. }] => assert_eq!(text, "HE"),
            other => panic!("{:?}", other),
        }
        // Off the screen: nothing. Empty text: nothing.
        for (dx, dy, t) in [(500.0, 0.0, "X"), (0.0, 500.0, "X"), (0.0, -50.0, "X"), (0.0, 0.0, "")] {
            out.clear();
            place(&Cmd::Text { at: Anchor::TopLeft, dx, dy, scale: 1.0, text: String::from(t), col: [255, 255, 255, 255] }, &f, &mut out);
            assert!(out.is_empty(), "{} {} {}", dx, dy, t);
        }
        // The scale is bounded.
        out.clear();
        place(&Cmd::Text { at: Anchor::TopLeft, dx: 0.0, dy: 0.0, scale: 1000.0, text: String::from("A"), col: [255, 255, 255, 255] }, &f, &mut out);
        match &out[..] {
            [Prim::Text { scale, .. }] => assert_eq!(*scale, 8.0),
            other => panic!("{:?}", other),
        }
    }

    #[test]
    fn icons_are_drawn_whole_or_not_at_all() {
        let mm = m;
        let f = frame(&mm);
        let mut out = Vec::new();
        place(&Cmd::Item { at: Anchor::Centre, dx: 0.0, dy: 0.0, size: 16.0, id: String::from("stick") }, &f, &mut out);
        assert_eq!(out, [Prim::Item { x: 184.0, y: 84.0, size: 32.0, id: String::from("stick") }]);
        out.clear();
        place(&Cmd::Block { at: Anchor::TopLeft, dx: -4.0, dy: 0.0, size: 16.0, id: String::from("stone") }, &f, &mut out);
        assert!(out.is_empty());
        place(&Cmd::Block { at: Anchor::TopLeft, dx: 0.0, dy: 0.0, size: 16.0, id: String::from("stone") }, &f, &mut out);
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0], Prim::Block { .. }));
    }

    #[test]
    fn a_bar_fills_by_its_fraction_and_survives_bad_numbers() {
        let mm = m;
        let f = frame(&mm);
        let bar = |frac: f32| Cmd::Bar { at: Anchor::TopLeft, dx: 0.0, dy: 0.0, w: 50.0, h: 5.0, frac, col: [0, 255, 0, 255] };
        let fill = |frac: f32| {
            let mut out = Vec::new();
            place(&bar(frac), &f, &mut out);
            out.iter().filter_map(|p| if let Prim::Rect { w, col, .. } = p { if col[1] == 255 { Some(*w) } else { None } } else { None }).next()
        };
        // 100 screen pixels wide, a 2 pixel rim each side.
        assert_eq!(fill(1.0), Some(96.0));
        assert_eq!(fill(0.5), Some(48.0));
        assert_eq!(fill(0.0), None);
        assert_eq!(fill(7.0), Some(96.0));
        assert_eq!(fill(-1.0), None);
        assert_eq!(fill(f32::NAN), None);
    }

    #[test]
    fn the_buffer_stops_at_its_cap_and_gives_each_mod_a_share() {
        let mut b = CmdBuf::default();
        let c = || Cmd::Rect { at: Anchor::TopLeft, dx: 0.0, dy: 0.0, w: 1.0, h: 1.0, col: [0, 0, 0, 255] };
        for _ in 0..MAX_HUD_CMDS_PER_MOD {
            assert!(b.push("a", c()));
        }
        // "a" has had its share...
        assert!(!b.push("a", c()));
        // ...but "b" still gets its own.
        for _ in 0..MAX_HUD_CMDS - MAX_HUD_CMDS_PER_MOD {
            assert!(b.push("b", c()));
        }
        // The frame is full for everybody.
        assert!(!b.push("b", c()));
        assert!(!b.push("c", c()));
        assert_eq!(b.len(), MAX_HUD_CMDS);
        assert_eq!(b.refused, 3);
        b.clear();
        assert!(b.is_empty());
        assert_eq!(b.refused, 0);
        assert!(b.push("a", c()));
    }

    #[test]
    fn colours_and_numbers_are_made_safe() {
        assert_eq!(colour(0xFF8000), [255, 128, 0, 255]);
        assert_eq!(colour(0x80FF0000), [255, 0, 0, 128]);
        assert_eq!(colour(0x00FF0000 + 0x1000000), [255, 0, 0, 1]);
        assert_eq!(colour(-5), [0, 0, 0, 255]);
        assert_eq!(colour(i64::MAX), [255, 255, 255, 255]);
        assert_eq!(num(f64::NAN, 100.0), 0.0);
        assert_eq!(num(1.0e300, 100.0), 100.0);
        assert_eq!(num(f64::NEG_INFINITY, 100.0), -100.0);
        assert_eq!(num(2.5, 100.0), 2.5);
    }
}
