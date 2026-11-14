//! Palettes: the colours a painted texture is made of.
//!
//! A texture line used to name one colour (`texture ruby_pick paint pick 226 60
//! 110`) and the painters made the dark and light tones from it by fixed ratios.
//! A palette holds up to six colours with roles (base, dark, light, accent,
//! highlight, outline), may spread them over the picture as flat bands
//! (a gradient: fire is red to yellow along the head of a tool), and may carry
//! more palettes (the gems of a "multi" ore, the wood of a tool's handle).
//!
//! ```text
//! palette ruby #e2406e #a82454 #ff9cb8          # base, dark, light
//! palette fire #d83a1c #f2a21c gradient vertical
//! texture ruby_ore paint ore palette ruby
//! texture fire_axe paint axe palette fire+oak   # head: fire; handle: oak
//! texture jade_sword paint sword colors #3aa66a #1e6a3e #8fe0b0 accent=#e8f8ee
//! texture old_style paint pick 226 60 110       # still works, same pixels as ever
//! ```
//!
//! The parts here are pure functions (parsing and sampling), so they are
//! tested without a game: see the tests at the foot of the file.

use crate::block::TileSrc;
use crate::painters::{dark, lift, scale, OUTLINE};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Most colours in one palette.
pub const MAX_COLORS: usize = 6;

/// How the colours of a palette are spread over a picture: in flat bands (no
/// blending, the OCD style has none), the first colour at the top, the left,
/// the top left or the middle, the last at the bottom, the right, the bottom
/// right or the rim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gradient {
    None,
    Vertical,
    Horizontal,
    Diagonal,
    Radial,
}

impl Gradient {
    pub fn parse(s: &str) -> Option<Gradient> {
        Some(match s {
            "none" => Gradient::None,
            "vertical" => Gradient::Vertical,
            "horizontal" => Gradient::Horizontal,
            "diagonal" => Gradient::Diagonal,
            "radial" => Gradient::Radial,
            _ => return None,
        })
    }
}

/// What a colour of a palette is for. The order is the order colours are
/// given in a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Base = 0,
    Dark = 1,
    Light = 2,
    Accent = 3,
    Highlight = 4,
    Outline = 5,
}

impl Role {
    pub fn parse(s: &str) -> Option<Role> {
        Some(match s {
            "base" => Role::Base,
            "dark" => Role::Dark,
            "light" => Role::Light,
            "accent" => Role::Accent,
            "highlight" => Role::Highlight,
            "outline" => Role::Outline,
            _ => return None,
        })
    }
}

/// Up to six colours, how they are spread, and (for ores and tools) further
/// palettes: the other gems of a multi-gem ore, the handle of a tool.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    /// The colours in the order given; the first is the base.
    colors: [[u8; 3]; MAX_COLORS],
    len: usize,
    /// Colours given by role name (`accent=#ffd0e0`); they win over the list.
    named: [Option<[u8; 3]>; MAX_COLORS],
    pub gradient: Gradient,
    /// Ores: the colour of the stone they sit in (default: stone).
    pub host: Option<[u8; 3]>,
    /// Ores: the further gems, used in turn after the first. Tools: the
    /// first of them is the handle (its base is the lit column, its dark the shaded one).
    pub more: Vec<Palette>,
}

impl Palette {
    /// The old one-colour palette: every tone is made from it by ratios, so
    /// the painters draw exactly what they always drew.
    pub fn from_rgb(rgb: [u8; 3]) -> Palette {
        let mut colors = [[0u8; 3]; MAX_COLORS];
        colors[0] = rgb;
        Palette { colors, len: 1, named: [None; MAX_COLORS], gradient: Gradient::None, host: None, more: Vec::new() }
    }

    /// A palette of the colours in the list (at most six are kept; an empty list is black).
    pub fn from_list(list: &[[u8; 3]], gradient: Gradient) -> Palette {
        let mut p = Palette::from_rgb(list.first().copied().unwrap_or([0, 0, 0]));
        for c in list.iter().skip(1).take(MAX_COLORS - 1) {
            p.colors[p.len] = *c;
            p.len += 1;
        }
        p.gradient = gradient;
        p
    }

    /// Only a base colour, nothing else: the old syntax.
    pub fn is_plain(&self) -> bool {
        self.len == 1 && self.named.iter().all(|n| n.is_none()) && self.gradient == Gradient::None && self.host.is_none() && self.more.is_empty()
    }

    /// How many colours were listed.
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The colours as listed (the stops of a gradient).
    pub fn colors(&self) -> &[[u8; 3]] {
        &self.colors[..self.len]
    }

    /// Does the gradient do anything (two colours or more and a direction)?
    pub fn is_gradient(&self) -> bool {
        self.gradient != Gradient::None && self.len >= 2
    }

    /// The colour of a role if the palette gives one: by name, or by its place
    /// in the list when there is no gradient (with a gradient every listed
    /// colour is a stop of it, so only named colours are roles).
    pub fn role(&self, r: Role) -> Option<[u8; 3]> {
        let i = r as usize;
        if let Some(c) = self.named[i] {
            return Some(c);
        }
        if i == 0 {
            return Some(self.colors[0]);
        }
        if self.gradient == Gradient::None && i < self.len {
            Some(self.colors[i])
        } else {
            None
        }
    }

    /// The base colour (the first one).
    pub fn base(&self) -> [u8; 3] {
        self.role(Role::Base).unwrap_or(self.colors[0])
    }

    /// The accent colour, if the palette has one.
    pub fn accent(&self) -> Option<[u8; 3]> {
        self.role(Role::Accent)
    }

    /// The outline of items: the palette's own or the usual near-black.
    pub fn outline(&self) -> [u8; 3] {
        self.role(Role::Outline).unwrap_or(OUTLINE)
    }

    /// The flat colour at `(x, y)` of a `w` x `h` area whose top left is
    /// `(x0, y0)`: the base without a gradient, else the band the pixel is in.
    pub fn at_in(&self, x: i32, y: i32, x0: i32, y0: i32, w: i32, h: i32) -> [u8; 3] {
        if !self.is_gradient() {
            return self.base();
        }
        self.colors[self.band(x - x0, y - y0, w, h)]
    }

    /// The colour at `(x, y)` of a `size` x `size` picture.
    pub fn sample(&self, x: i32, y: i32, size: i32) -> [u8; 3] {
        self.at_in(x, y, 0, 0, size, size)
    }

    /// The colour at `(x, y)` of a 16 x 16 tile.
    pub fn at(&self, x: i32, y: i32) -> [u8; 3] {
        self.sample(x, y, 16)
    }

    /// Which of the colours the pixel `(px, py)` of a `w` x `h` area is in.
    fn band(&self, px: i32, py: i32, w: i32, h: i32) -> usize {
        let n = self.len as i32;
        let (w, h) = (w.max(1), h.max(1));
        let (px, py) = (px.clamp(0, w - 1), py.clamp(0, h - 1));
        let i = match self.gradient {
            Gradient::None => 0,
            Gradient::Vertical => py * n / h,
            Gradient::Horizontal => px * n / w,
            Gradient::Diagonal => (px + py) * n / (w + h - 1),
            Gradient::Radial => {
                // Distance from the middle in half pixels, as a fraction of the distance to a corner.
                let (dx, dy) = (2 * px - (w - 1), 2 * py - (h - 1));
                let far = (w - 1) * (w - 1) + (h - 1) * (h - 1);
                if far == 0 {
                    0
                } else {
                    let t = crate::math::sqrtf((dx * dx + dy * dy) as f32 / far as f32);
                    ((t * n as f32) as i32).min(n - 1)
                }
            }
        };
        i.clamp(0, n - 1) as usize
    }

    /// The lit tone of `c`: the palette's light colour, else a fifth of the
    /// way to white (the old rule). With a gradient and no light colour, `c`
    /// itself: the bands do the shading, so the picture keeps few colours.
    pub fn light_of(&self, c: [u8; 3]) -> [u8; 3] {
        match self.role(Role::Light) {
            Some(l) => l,
            None if self.is_gradient() => c,
            None => lift(c),
        }
    }

    /// The shaded tone of `c`: the palette's dark colour, else the old ratio.
    pub fn dark_of(&self, c: [u8; 3]) -> [u8; 3] {
        match self.role(Role::Dark) {
            Some(d) => d,
            None if self.is_gradient() => c,
            None => dark(c),
        }
    }

    /// The darker tone of `c` a painter wants at `pct` percent (a frame, a
    /// gap, a rim): the palette's dark colour if it has one, else `c` scaled.
    pub fn shade(&self, c: [u8; 3], pct: i32) -> [u8; 3] {
        self.role(Role::Dark).unwrap_or_else(|| scale(c, pct))
    }

    /// The `i`th gem of an ore: the palette itself, then the `more`, over and over.
    pub fn nth(&self, i: usize) -> &Palette {
        let k = i % (1 + self.more.len());
        if k == 0 {
            self
        } else {
            &self.more[k - 1]
        }
    }

    /// The handle of a tool as (lit column, shaded column): the first of the
    /// `more` palettes, else `default`.
    pub fn handle_or(&self, default: ([u8; 3], [u8; 3])) -> ([u8; 3], [u8; 3]) {
        match self.more.first() {
            Some(h) => {
                let b = h.base();
                (b, h.role(Role::Dark).unwrap_or_else(|| dark(b)))
            }
            None => default,
        }
    }
}

// ---- Parsing.

/// `#rrggbb` (or `#rgb`) as a colour.
pub fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    let digit = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    let b = h.as_bytes();
    match b.len() {
        6 => {
            let mut out = [0u8; 3];
            for i in 0..3 {
                out[i] = digit(b[2 * i])? * 16 + digit(b[2 * i + 1])?;
            }
            Some(out)
        }
        3 => {
            let mut out = [0u8; 3];
            for i in 0..3 {
                out[i] = digit(b[i])? * 17;
            }
            Some(out)
        }
        _ => None,
    }
}

/// Where the comment of a data line starts. `#` starts a comment, except in
/// a `texture` or `palette` line where `#rrggbb` is a colour.
pub fn comment_start(raw: &str) -> Option<usize> {
    let first = raw.split_whitespace().next().unwrap_or("");
    let colours_allowed = first == "texture" || first == "palette";
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            if !colours_allowed {
                return Some(i);
            }
            // A colour: a `#` after a space, an `=` or the start, then six hex digits and the end of the word.
            let end = (i + 7).min(bytes.len());
            let word = raw.get(i..end).unwrap_or("");
            let before_ok = i == 0 || bytes[i - 1].is_ascii_whitespace() || bytes[i - 1] == b'=';
            let after_ok = end == bytes.len() || bytes[end].is_ascii_whitespace();
            if !(before_ok && word.len() == 7 && after_ok && parse_hex(word).is_some()) {
                return Some(i);
            }
            i = end;
            continue;
        }
        i += 1;
    }
    None
}

/// Words that are not colours.
fn is_keyword(w: &str) -> bool {
    matches!(w, "palette" | "colors" | "colours" | "gradient" | "host")
}

/// The stones an ore can sit in, by name.
fn host_named(name: &str) -> Option<[u8; 3]> {
    Some(match name {
        "stone" => crate::painters::stone::STONE,
        "deepstone" => crate::painters::stone::DEEPSTONE,
        "sandstone" => [232, 214, 160],
        "dirt" => crate::painters::blocks::DIRT,
        "gravel" => [110, 108, 118],
        "netherrack" => [110, 54, 52],
        _ => return None,
    })
}

/// The palettes a pack has declared so far, by name.
pub type Palettes = Vec<(String, Palette)>;

fn find<'a>(known: &'a Palettes, name: &str) -> Option<&'a Palette> {
    known.iter().rev().find(|(n, _)| n == name).map(|(_, p)| p)
}

/// Read colours and `gradient <dir>` from `words`: `#rrggbb`, `r g b`
/// (three numbers), `role=#rrggbb`. Returns the palette.
fn read_colors(words: &[&str]) -> Result<Palette, String> {
    let mut list: Vec<[u8; 3]> = Vec::new();
    let mut named: [Option<[u8; 3]>; MAX_COLORS] = [None; MAX_COLORS];
    let mut gradient = Gradient::None;
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        if w == "gradient" {
            let dir = words.get(i + 1).ok_or_else(|| String::from("gradient needs vertical, horizontal, diagonal or radial"))?;
            gradient = Gradient::parse(dir).ok_or_else(|| alloc::format!("unknown gradient `{}` (vertical, horizontal, diagonal, radial)", dir))?;
            i += 2;
        } else if let Some((role, col)) = w.split_once('=') {
            let r = Role::parse(role).ok_or_else(|| alloc::format!("unknown colour role `{}` (base, dark, light, accent, highlight, outline)", role))?;
            let c = parse_hex(col).ok_or_else(|| alloc::format!("`{}` is not a colour like #aabbcc", col))?;
            named[r as usize] = Some(c);
            i += 1;
        } else if w.starts_with('#') {
            let c = parse_hex(w).ok_or_else(|| alloc::format!("`{}` is not a colour like #aabbcc", w))?;
            list.push(c);
            i += 1;
        } else if let Ok(r) = w.parse::<u8>() {
            let g = words.get(i + 1).and_then(|s| s.parse::<u8>().ok());
            let b = words.get(i + 2).and_then(|s| s.parse::<u8>().ok());
            match (g, b) {
                (Some(g), Some(b)) => list.push([r, g, b]),
                _ => return Err(String::from("a colour is #rrggbb or three numbers 0..255")),
            }
            i += 3;
        } else {
            return Err(alloc::format!("`{}` is not a colour (use #rrggbb or r g b)", w));
        }
        if list.len() > MAX_COLORS {
            return Err(alloc::format!("a palette has at most {} colours", MAX_COLORS));
        }
    }
    if list.is_empty() {
        // Only named colours: they stand in the list in role order.
        for c in named.iter().flatten() {
            list.push(*c);
        }
    }
    if list.is_empty() {
        return Err(String::from("a palette needs at least one colour"));
    }
    let mut p = Palette::from_list(&list, gradient);
    p.named = named;
    Ok(p)
}

/// A `palette <name> <colours...> [gradient <dir>]` line (the words after `palette`).
pub fn declare(known: &mut Palettes, args: &[&str]) -> Result<(), String> {
    let name = args.first().ok_or_else(|| String::from("palette needs: <name> <colours...> [gradient <dir>]"))?;
    if name.starts_with('#') || name.parse::<u8>().is_ok() || name.contains('+') || name.contains('=') {
        return Err(alloc::format!("`{}` is not a good palette name", name));
    }
    let p = read_colors(&args[1..])?;
    known.push((name.to_string(), p));
    Ok(())
}

/// Read the colour part of a `texture <id> paint <style> ...` line (the words
/// after the style): `r g b`; or `palette <name>[+<more>...]`; or colours
/// inline, optionally after the word `colors`; with `gradient <dir>` and
/// `host <stone|#rrggbb|palette>` as extras.
pub fn parse_spec(words: &[&str], known: &Palettes) -> Result<Palette, String> {
    let mut base: Option<Palette> = None;
    let mut host = None;
    let mut gradient = None;
    let mut inline: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        match words[i] {
            "palette" => {
                let names = words.get(i + 1).ok_or_else(|| String::from("palette needs a name"))?;
                let mut parts = names.split('+');
                let first = parts.next().unwrap_or("");
                let mut p = find(known, first).cloned().ok_or_else(|| alloc::format!("no palette called `{}` (declare it with a `palette` line first)", first))?;
                for n in parts {
                    let m = find(known, n).cloned().ok_or_else(|| alloc::format!("no palette called `{}`", n))?;
                    p.more.push(m);
                }
                base = Some(p);
                i += 2;
            }
            "colors" | "colours" => i += 1,
            "gradient" => {
                let d = words.get(i + 1).ok_or_else(|| String::from("gradient needs a direction"))?;
                gradient = Some(Gradient::parse(d).ok_or_else(|| alloc::format!("unknown gradient `{}` (vertical, horizontal, diagonal, radial)", d))?);
                i += 2;
            }
            "host" => {
                let h = words.get(i + 1).ok_or_else(|| String::from("host needs a stone name, a palette or #rrggbb"))?;
                let c = if let Some(c) = parse_hex(h) {
                    c
                } else if let Some(c) = host_named(h) {
                    c
                } else if let Some(p) = find(known, h) {
                    p.base()
                } else {
                    return Err(alloc::format!("unknown host `{}` (stone, deepstone, sandstone, dirt, gravel, netherrack, a palette or #rrggbb)", h));
                };
                host = Some(c);
                i += 2;
            }
            _ => {
                // A colour; take all of them up to the next keyword.
                let mut j = i;
                while j < words.len() && !is_keyword(words[j]) {
                    j += 1;
                }
                inline.extend_from_slice(&words[i..j]);
                i = j;
            }
        }
    }
    let mut p = if !inline.is_empty() {
        let mut p = read_colors(&inline)?;
        if let Some(b) = base {
            // Colours given as well as a palette: the colours win, the palette's extras are kept.
            p.more = b.more;
        }
        p
    } else {
        base.ok_or_else(|| String::from("paint needs colours: r g b, #rrggbb ..., or palette <name>"))?
    };
    if let Some(g) = gradient {
        p.gradient = g;
    }
    if host.is_some() {
        p.host = host;
    }
    Ok(p)
}

/// The texture source for a `paint` line: `words` are the style and what
/// follows. Three numbers keep the old, plain `Paint`; anything else is a palette.
pub fn paint_source(words: &[&str], known: &Palettes) -> Result<TileSrc, String> {
    let style = *words.first().ok_or_else(|| String::from("paint needs a style"))?;
    if crate::textures::painted(style, [0, 0, 0], 0, 0).is_none() {
        return Err(alloc::format!("unknown paint style `{}`", style));
    }
    let rest = &words[1..];
    if rest.len() == 3 {
        let n: Vec<Option<u8>> = rest.iter().map(|s| s.parse::<u8>().ok()).collect();
        if let (Some(r), Some(g), Some(b)) = (n[0], n[1], n[2]) {
            return Ok(TileSrc::Paint { style: style.to_string(), rgb: [r, g, b] });
        }
    }
    let pal = parse_spec(rest, known)?;
    if pal.is_plain() {
        return Ok(TileSrc::Paint { style: style.to_string(), rgb: pal.base() });
    }
    Ok(TileSrc::Palette { style: style.to_string(), pal })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pals() -> Palettes {
        let mut k = Palettes::new();
        declare(&mut k, &["ruby", "#e2406e", "#a82454", "#ff9cb8"]).unwrap();
        declare(&mut k, &["fire", "#d83a1c", "#f2a21c", "gradient", "vertical"]).unwrap();
        declare(&mut k, &["oak", "160", "122", "74", "116", "86", "52"]).unwrap();
        k
    }

    #[test]
    fn hex_colours() {
        assert_eq!(parse_hex("#e2406e"), Some([0xe2, 0x40, 0x6e]));
        assert_eq!(parse_hex("#FFF"), Some([255, 255, 255]));
        assert_eq!(parse_hex("#12345"), None);
        assert_eq!(parse_hex("e2406e"), None);
        assert_eq!(parse_hex("#gg0000"), None);
    }

    #[test]
    fn hash_is_a_comment_except_for_colours_in_texture_and_palette_lines() {
        assert_eq!(comment_start("block stone # hard"), Some(12));
        assert_eq!(comment_start("block stone #abcdef"), Some(12));
        assert_eq!(comment_start("palette ruby #e2406e #a82454"), None);
        assert_eq!(comment_start("palette ruby #e2406e # the red"), Some(21));
        assert_eq!(comment_start("texture a paint gem colors #e2406e accent=#ffffff"), None);
        assert_eq!(comment_start("texture a paint gem 1 2 3 # note"), Some(26));
        assert_eq!(comment_start("# all comment"), Some(0));
        assert_eq!(comment_start("texture a paint gem #zzzzzz"), Some(20));
    }

    #[test]
    fn declared_palettes_have_roles_in_order() {
        let k = pals();
        let ruby = find(&k, "ruby").unwrap();
        assert_eq!(ruby.len(), 3);
        assert_eq!(ruby.base(), [0xe2, 0x40, 0x6e]);
        assert_eq!(ruby.role(Role::Dark), Some([0xa8, 0x24, 0x54]));
        assert_eq!(ruby.role(Role::Light), Some([0xff, 0x9c, 0xb8]));
        assert_eq!(ruby.role(Role::Accent), None);
        assert_eq!(ruby.outline(), OUTLINE);
        let oak = find(&k, "oak").unwrap();
        assert_eq!(oak.colors(), &[[160, 122, 74], [116, 86, 52]]);
    }

    #[test]
    fn named_roles_and_errors() {
        let p = parse_spec(&["colors", "#102030", "accent=#ffeeaa", "outline=#000000"], &pals()).unwrap();
        assert_eq!(p.base(), [0x10, 0x20, 0x30]);
        assert_eq!(p.accent(), Some([0xff, 0xee, 0xaa]));
        assert_eq!(p.outline(), [0, 0, 0]);
        assert!(parse_spec(&["palette", "nope"], &pals()).is_err());
        assert!(parse_spec(&["#12345g"], &pals()).is_err());
        assert!(parse_spec(&["gradient", "sideways", "#112233"], &pals()).is_err());
        assert!(parse_spec(&["#111111", "#222222", "#333333", "#444444", "#555555", "#666666", "#777777"], &pals()).is_err());
        assert!(parse_spec(&[], &pals()).is_err());
        assert!(declare(&mut pals(), &["#fff", "#000000"]).is_err());
    }

    #[test]
    fn old_rgb_lines_stay_plain() {
        let k = pals();
        match paint_source(&["pick", "226", "60", "110"], &k).unwrap() {
            TileSrc::Paint { style, rgb } => assert_eq!((style.as_str(), rgb), ("pick", [226, 60, 110])),
            _ => panic!("old syntax is a plain Paint"),
        }
        // One hex colour is the same thing.
        assert!(matches!(paint_source(&["pick", "#e23c6e"], &k).unwrap(), TileSrc::Paint { rgb: [0xe2, 0x3c, 0x6e], .. }));
        assert!(matches!(paint_source(&["pick", "palette", "ruby"], &k).unwrap(), TileSrc::Palette { .. }));
        assert!(paint_source(&["no_such_style", "1", "2", "3"], &k).is_err());
    }

    #[test]
    fn palette_names_chain_and_hosts() {
        let k = pals();
        let p = parse_spec(&["palette", "fire+oak", "host", "deepstone"], &k).unwrap();
        assert_eq!(p.gradient, Gradient::Vertical);
        assert_eq!(p.more.len(), 1);
        assert_eq!(p.handle_or(([0; 3], [0; 3])), ([160, 122, 74], [116, 86, 52]));
        assert_eq!(p.host, Some(crate::painters::stone::DEEPSTONE));
        let q = parse_spec(&["palette", "ruby", "host", "#102030"], &k).unwrap();
        assert_eq!(q.host, Some([16, 32, 48]));
        // A gradient word on the line overrides the palette's.
        let r = parse_spec(&["palette", "fire", "gradient", "radial"], &k).unwrap();
        assert_eq!(r.gradient, Gradient::Radial);
    }

    #[test]
    fn plain_palettes_make_the_old_tones() {
        let p = Palette::from_rgb([150, 110, 70]);
        let c = p.at(3, 4);
        assert_eq!(c, [150, 110, 70]);
        assert_eq!(p.light_of(c), lift(c));
        assert_eq!(p.dark_of(c), dark(c));
        assert_eq!(p.shade(c, 60), scale(c, 60));
        assert_eq!(p.at_in(15, 15, 0, 0, 16, 16), c);
        assert!(p.is_plain());
        assert_eq!(p.handle_or(([1; 3], [2; 3])), ([1; 3], [2; 3]));
    }

    #[test]
    fn explicit_roles_win() {
        let p = Palette::from_list(&[[200, 50, 50], [100, 20, 20], [250, 150, 150]], Gradient::None);
        assert_eq!(p.dark_of([1, 2, 3]), [100, 20, 20]);
        assert_eq!(p.light_of([1, 2, 3]), [250, 150, 150]);
        assert_eq!(p.shade([1, 2, 3], 50), [100, 20, 20]);
    }

    #[test]
    fn vertical_and_horizontal_gradients_are_flat_bands() {
        let p = Palette::from_list(&[[255, 0, 0], [255, 128, 0], [255, 255, 0]], Gradient::Vertical);
        // Sixteen rows in three bands: 0..=5, 5..=10, 10..=15 (by y * 3 / 16).
        let col = |y: i32| p.at(8, y);
        assert_eq!(col(0), [255, 0, 0]);
        assert_eq!(col(5), [255, 0, 0]);
        assert_eq!(col(6), [255, 128, 0]);
        assert_eq!(col(15), [255, 255, 0]);
        // Same colour along a row.
        assert_eq!(p.at(0, 7), p.at(15, 7));
        let h = Palette::from_list(&[[0, 0, 255], [255, 255, 255]], Gradient::Horizontal);
        assert_eq!(h.at(0, 9), [0, 0, 255]);
        assert_eq!(h.at(7, 9), [0, 0, 255]);
        assert_eq!(h.at(8, 9), [255, 255, 255]);
        assert_eq!(h.at(15, 0), [255, 255, 255]);
    }

    #[test]
    fn gradient_over_a_part_of_the_tile() {
        let p = Palette::from_list(&[[1, 1, 1], [2, 2, 2]], Gradient::Vertical);
        // The head of a tool: rows 2..=4 of the tile.
        assert_eq!(p.at_in(5, 2, 0, 2, 16, 3), [1, 1, 1]);
        assert_eq!(p.at_in(5, 4, 0, 2, 16, 3), [2, 2, 2]);
        // Outside the area it holds the ends.
        assert_eq!(p.at_in(5, 0, 0, 2, 16, 3), [1, 1, 1]);
        assert_eq!(p.at_in(5, 15, 0, 2, 16, 3), [2, 2, 2]);
    }

    #[test]
    fn diagonal_and_radial_gradients() {
        let d = Palette::from_list(&[[1, 0, 0], [2, 0, 0]], Gradient::Diagonal);
        assert_eq!(d.at(0, 0), [1, 0, 0]);
        assert_eq!(d.at(15, 15), [2, 0, 0]);
        assert_eq!(d.at(15, 0), d.at(0, 15));
        let r = Palette::from_list(&[[1, 0, 0], [2, 0, 0], [3, 0, 0]], Gradient::Radial);
        assert_eq!(r.at(7, 7), [1, 0, 0]);
        assert_eq!(r.at(8, 8), [1, 0, 0]);
        assert_eq!(r.at(0, 0), [3, 0, 0]);
        assert_eq!(r.at(15, 15), [3, 0, 0]);
        assert_eq!(r.at(0, 15), [3, 0, 0]);
    }

    #[test]
    fn a_gradient_with_one_colour_or_no_direction_is_flat() {
        let one = Palette::from_list(&[[9, 9, 9]], Gradient::Vertical);
        assert_eq!(one.at(3, 14), [9, 9, 9]);
        let none = Palette::from_list(&[[9, 9, 9], [1, 1, 1]], Gradient::None);
        assert_eq!(none.at(3, 14), [9, 9, 9]);
        // With a gradient the bands do the shading.
        let g = Palette::from_list(&[[9, 9, 9], [1, 1, 1]], Gradient::Vertical);
        assert_eq!(g.light_of([5, 5, 5]), [5, 5, 5]);
        assert_eq!(g.dark_of([5, 5, 5]), [5, 5, 5]);
    }

    #[test]
    fn ores_take_their_gems_in_turn() {
        let mut p = Palette::from_rgb([1, 1, 1]);
        p.more.push(Palette::from_rgb([2, 2, 2]));
        assert_eq!(p.nth(0).base(), [1, 1, 1]);
        assert_eq!(p.nth(1).base(), [2, 2, 2]);
        assert_eq!(p.nth(2).base(), [1, 1, 1]);
    }
}
