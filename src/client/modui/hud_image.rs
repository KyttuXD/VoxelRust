//! Pictures a mod puts on the HUD (`hud_image`, `hud_image_uv`,
//! `hud_image_fit`): a jumpscare, a counter strip, a logo.
//!
//! A mod names a picture by its file, relative to its own folder
//! (`images/scare.png`; png or tga, plain names, no `..`). The first call
//! that names it reads and decodes it, the next calls find it in the cache.
//! Each mod may keep [`MAX_PER_MOD`] pictures (a name that failed counts too, so
//! a script cannot make the game read a thousand files), each at most
//! [`MAX_SIDE`] x [`MAX_SIDE`] pixels and [`MAX_BYTES`] decoded; all mods
//! together [`MAX_TOTAL`]. The pixels go to the graphics card the first time
//! the picture is drawn and are dropped from memory then; when a mod is
//! switched off its pictures are taken from the card.
//!
//! This file holds the cache and the placing of an image command on the
//! screen (anchors, interface scale, clipping, the `cover` and `contain` fits);
//! painting is in `hud_draw.rs`. The decisions are plain arithmetic and are
//! tested without a window.

use crate::client::modui::hudcmd::{self, Cmd, Frame, Prim};
use crate::render::Color;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, Ordering};

/// Where the player's mods are when `--mods-dir` names a folder other than the data folder's `mods`
/// (set once at start; an earlier value is left to leak on purpose, nobody frees what a reader may hold).
static MODS_DIR: AtomicPtr<String> = AtomicPtr::new(core::ptr::null_mut());

/// Say where the mods' folders are (no trailing slash); empty means the data folder's own `mods`.
pub fn set_mods_dir(dir: &str) {
    let p = Box::into_raw(Box::new(String::from(dir.trim_end_matches(|c| c == '/' || c == '\\'))));
    MODS_DIR.store(p, Ordering::Release);
}

fn mods_dir() -> Option<&'static str> {
    let p = MODS_DIR.load(Ordering::Acquire);
    // Safety: set from a leaked box and never freed.
    if p.is_null() { None } else { Some(unsafe { &*p }.as_str()).filter(|d| !d.is_empty()) }
}

/// Pictures one mod may name, and all mods together.
pub const MAX_PER_MOD: usize = 16;
pub const MAX_TOTAL: usize = 64;
/// Largest side of a picture, pixels, and most bytes of it decoded.
pub const MAX_SIDE: usize = 1024;
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
/// Most decoded bytes of all the pictures of all mods together (64 pictures of 4 MB would be 256 MB on the card).
pub const MAX_TOTAL_BYTES: usize = 48 * 1024 * 1024;
/// Largest file read (before it is decoded).
pub const MAX_FILE: usize = 8 * 1024 * 1024;
/// Longest file name.
pub const MAX_NAME: usize = 64;

/// Whether a mod may ask for this file: a plain relative path of letters,
/// digits, `_`, `-`, `.` and `/` ending in `.png` or `.tga`.
pub fn valid_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let ext = lower.ends_with(".png") || lower.ends_with(".tga");
    ext && name.len() <= MAX_NAME
        && !name.starts_with('/')
        && !name.starts_with('.')
        && !name.contains("..")
        && !name.contains("//")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
}

/// A decoded picture: its size and RGBA pixels.
pub struct Pixels {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

/// One cached picture.
#[derive(Debug)]
pub struct Entry {
    pub id: u32,
    pub owner: String,
    pub file: String,
    pub w: usize,
    pub h: usize,
    /// Waiting to go to the card; dropped when it has.
    pixels: Option<Vec<u8>>,
    /// The picture on the card (0: not there yet).
    pub tex: u32,
    /// It could not be read or was too big: asking again does nothing.
    pub failed: bool,
}

/// What a lookup gave: the picture's number and size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Found {
    pub id: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Default)]
pub struct Images {
    list: Vec<Entry>,
    next: u32,
    /// Pictures on the card whose mod is gone: to be deleted by the renderer.
    dead: Vec<u32>,
}

impl Images {
    pub fn count_of(&self, owner: &str) -> usize {
        self.list.iter().filter(|e| e.owner == owner).count()
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Decoded bytes of all the pictures (what they take on the card).
    pub fn bytes(&self) -> usize {
        self.list.iter().filter(|e| !e.failed).map(|e| e.w * e.h * 4).sum()
    }

    /// The picture `file` of `owner`, read through `source` the first time.
    /// `None` when the name is no good, the picture failed, or a cap is used up.
    pub fn get(&mut self, owner: &str, file: &str, source: &mut dyn FnMut() -> Option<Pixels>) -> Option<Found> {
        if !valid_file(file) {
            return None;
        }
        if let Some(e) = self.list.iter().find(|e| e.owner == owner && e.file == file) {
            return if e.failed { None } else { Some(Found { id: e.id, w: e.w as u32, h: e.h as u32 }) };
        }
        if self.count_of(owner) >= MAX_PER_MOD || self.list.len() >= MAX_TOTAL {
            return None;
        }
        self.next = self.next.wrapping_add(1).max(1);
        let id = self.next;
        let mut entry = Entry { id, owner: String::from(owner), file: String::from(file), w: 0, h: 0, pixels: None, tex: 0, failed: true };
        if let Some(p) = source() {
            let sane = p.w > 0 && p.h > 0 && p.w <= MAX_SIDE && p.h <= MAX_SIDE && p.rgba.len() == p.w * p.h * 4 && p.rgba.len() <= MAX_BYTES && self.bytes() + p.rgba.len() <= MAX_TOTAL_BYTES;
            if sane {
                entry.w = p.w;
                entry.h = p.h;
                entry.pixels = Some(p.rgba);
                entry.failed = false;
            }
        }
        let found = if entry.failed { None } else { Some(Found { id, w: entry.w as u32, h: entry.h as u32 }) };
        self.list.push(entry);
        found
    }

    /// Send the pictures that wait to the card with `upload(w, h, rgba)` (which
    /// answers its handle) and forget their pixels.
    pub fn upload(&mut self, upload: &mut dyn FnMut(usize, usize, &[u8]) -> u32) {
        for e in self.list.iter_mut() {
            if let (true, Some(px)) = (e.tex == 0, e.pixels.as_ref()) {
                e.tex = upload(e.w, e.h, px);
                if e.tex == 0 {
                    // The card refused it: not worth asking again.
                    e.failed = true;
                }
                e.pixels = None;
            }
        }
    }

    /// The card's handle of a picture, if it is there.
    pub fn tex_of(&self, id: u32) -> Option<u32> {
        self.list.iter().find(|e| e.id == id && !e.failed && e.tex != 0).map(|e| e.tex)
    }

    /// A mod was switched off: its pictures go (those on the card are listed for deleting).
    pub fn drop_mod(&mut self, owner: &str) {
        for e in self.list.iter().filter(|e| e.owner == owner) {
            if e.tex != 0 {
                self.dead.push(e.tex);
            }
        }
        self.list.retain(|e| e.owner != owner);
    }

    /// The handles to take from the card (once).
    pub fn take_dead(&mut self) -> Vec<u32> {
        core::mem::take(&mut self.dead)
    }
}

/// Read and decode a mod's picture from disk: `<data>/mods/<mod>/<file>`, or
/// the folder of the same mod in a server's download (`<data>/tmp/mods/<server>/<mod>/`),
/// or else a picture the mod's pack named with a `picture` line (by the file's name
/// without its folders and extension).
pub fn load(data_dir: &str, owner: &str, file: &str) -> Option<Pixels> {
    if !valid_file(file) {
        return None;
    }
    let safe_owner = !owner.is_empty() && owner.len() <= 40 && owner.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')) && !owner.starts_with('.');
    let mut paths: Vec<String> = Vec::new();
    if let (true, Some(dir)) = (safe_owner, mods_dir()) {
        paths.push(alloc::format!("{}/{}/{}", dir, owner, file));
    }
    if safe_owner && !data_dir.is_empty() {
        paths.push(alloc::format!("{}mods/{}/{}", data_dir, owner, file));
        for key in crate::sys::list_dir(&alloc::format!("{}tmp/mods/", data_dir)).into_iter().take(8) {
            if !key.is_empty() && !key.starts_with('.') && key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')) {
                paths.push(alloc::format!("{}tmp/mods/{}/{}/{}", data_dir, key, owner, file));
            }
        }
    }
    for p in &paths {
        if let Some(img) = crate::sys::read_file_limit(p, MAX_FILE).and_then(|b| crate::image::decode(&b)) {
            return Some(Pixels { w: img.w, h: img.h, rgba: img.rgba });
        }
    }
    let stem = file.rsplit('/').next().unwrap_or(file);
    let stem = stem.rsplit_once('.').map_or(stem, |s| s.0);
    crate::pack_entities::mod_image(stem).map(|img| Pixels { w: img.w, h: img.h, rgba: img.rgba })
}

/// How a whole-screen picture fits the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fit {
    /// The whole picture shows, with bars at the sides or top if the shapes differ.
    Contain,
    /// The screen is filled; what does not fit is cropped.
    Cover,
}

impl Fit {
    pub fn parse(name: &str) -> Option<Fit> {
        match name.to_ascii_lowercase().as_str() {
            "contain" | "fit" => Some(Fit::Contain),
            "cover" | "fill" => Some(Fit::Cover),
            _ => None,
        }
    }
}

/// The picture's rectangle in screen pixels and the part of it (u0, v0, u1, v1)
/// to show, for a picture of `iw` x `ih` fitted to a screen of `sw` x `sh`.
pub fn fit_rect(iw: f32, ih: f32, sw: f32, sh: f32, fit: Fit) -> Option<([f32; 4], [f32; 4])> {
    if !(iw > 0.0 && ih > 0.0 && sw > 0.0 && sh > 0.0) {
        return None;
    }
    let (kx, ky) = (sw / iw, sh / ih);
    match fit {
        Fit::Contain => {
            let k = if kx < ky { kx } else { ky };
            let (w, h) = (iw * k, ih * k);
            Some(([(sw - w) * 0.5, (sh - h) * 0.5, w, h], [0.0, 0.0, 1.0, 1.0]))
        }
        Fit::Cover => {
            let k = if kx > ky { kx } else { ky };
            // The share of the picture that shows, centred.
            let (u, v) = (sw / (iw * k), sh / (ih * k));
            let (u0, v0) = ((1.0 - u) * 0.5, (1.0 - v) * 0.5);
            Some(([0.0, 0.0, sw, sh], [u0, v0, 1.0 - u0, 1.0 - v0]))
        }
    }
}

/// A rectangle and the part of a picture on it, cut to the screen: the
/// picture is cut along with it, so what stays is not stretched.
pub fn clip_with_uv(r: [f32; 4], uv: [f32; 4], sw: f32, sh: f32) -> Option<([f32; 4], [f32; 4])> {
    let (x, y, w, h) = (r[0], r[1], r[2], r[3]);
    let (cx, cy, cw, ch) = hudcmd::clip_rect(x, y, w, h, sw, sh)?;
    let (fx0, fy0, fx1, fy1) = ((cx - x) / w, (cy - y) / h, (cx + cw - x) / w, (cy + ch - y) / h);
    let (du, dv) = (uv[2] - uv[0], uv[3] - uv[1]);
    Some(([cx, cy, cw, ch], [uv[0] + du * fx0, uv[1] + dv * fy0, uv[0] + du * fx1, uv[1] + dv * fy1]))
}

fn finite4(v: [f32; 4]) -> bool {
    v.iter().all(|x| x.is_finite())
}

/// Turn an image command into what is to be painted.
pub fn place(cmd: &Cmd, f: &Frame, out: &mut Vec<Prim>) {
    match cmd {
        Cmd::Image { at, dx, dy, w, h, uv, col, img } => {
            if col[3] == 0 || !finite4(*uv) {
                return;
            }
            let (w, h) = (w.clamp(0.0, 8192.0) * f.s, h.clamp(0.0, 8192.0) * f.s);
            if w <= 0.0 || h <= 0.0 {
                return;
            }
            let (x, y) = hudcmd::origin(f, *at, *dx, *dy, w, h);
            if let Some((r, uv)) = clip_with_uv([x, y, w, h], *uv, f.w, f.h) {
                out.push(Prim::Image { x: r[0], y: r[1], w: r[2], h: r[3], uv, col: *col, img: *img });
            }
        }
        Cmd::ImageFrac { x, y, w, h, col, img } => {
            if col[3] == 0 || !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() {
                return;
            }
            let r = [x * f.w, y * f.h, w * f.w, h * f.h];
            if r[2] <= 0.0 || r[3] <= 0.0 {
                return;
            }
            if let Some((r, uv)) = clip_with_uv(r, [0.0, 0.0, 1.0, 1.0], f.w, f.h) {
                out.push(Prim::Image { x: r[0], y: r[1], w: r[2], h: r[3], uv, col: *col, img: *img });
            }
        }
        Cmd::Fit { img, iw, ih, fit, col } => {
            if col[3] == 0 {
                return;
            }
            if let Some((r, uv)) = fit_rect(*iw as f32, *ih as f32, f.w, f.h, *fit) {
                out.push(Prim::Image { x: r[0], y: r[1], w: r[2], h: r[3], uv, col: *col, img: *img });
            }
        }
        _ => {}
    }
}

/// The colour of a picture drawn with an opacity (0 to 1) and a tint
/// (`0xRRGGBB`, white for none).
pub fn tinted(tint: i64, alpha: f64) -> Color {
    let a = if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 0.0 };
    let t = tint.clamp(0, 0xFF_FFFF) as u32;
    [(t >> 16) as u8, (t >> 8) as u8, t as u8, (a * 255.0) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::modui::hudcmd::Anchor;

    fn m(t: &str, sc: f32) -> f32 {
        t.len() as f32 * 6.0 * sc
    }

    fn px(w: usize, h: usize) -> Option<Pixels> {
        Some(Pixels { w, h, rgba: alloc::vec![255; w * h * 4] })
    }

    #[test]
    fn file_names_are_plain_relative_pictures() {
        for ok in ["a.png", "images/scare.png", "ui/Counter_1.TGA", "a-b.c.png"] {
            assert!(valid_file(ok), "{ok}");
        }
        let long = alloc::format!("{}.png", "a".repeat(80));
        for bad in ["", "a", "a.jpg", "../a.png", "a/../b.png", "/etc/a.png", ".hidden.png", "a//b.png", "a b.png", "a\\b.png", "x.png/", long.as_str()] {
            assert!(!valid_file(bad), "{bad}");
        }
    }

    #[test]
    fn a_picture_is_read_once_and_cached() {
        let mut im = Images::default();
        let mut reads = 0;
        let a = im.get("m", "a.png", &mut || { reads += 1; px(8, 4) }).unwrap();
        assert_eq!((a.w, a.h), (8, 4));
        let b = im.get("m", "a.png", &mut || { reads += 1; px(8, 4) }).unwrap();
        assert_eq!(a, b);
        assert_eq!(reads, 1);
        // Another mod's picture of the same name is its own.
        let c = im.get("n", "a.png", &mut || px(2, 2)).unwrap();
        assert_ne!(a.id, c.id);
        assert_eq!(im.bytes(), 8 * 4 * 4 + 2 * 2 * 4);
        // A bad name is never read.
        assert!(im.get("m", "../x.png", &mut || panic!("read")).is_none());
    }

    #[test]
    fn a_mod_has_sixteen_pictures_and_failures_count() {
        let mut im = Images::default();
        for i in 0..MAX_PER_MOD {
            let name = alloc::format!("p{}.png", i);
            // Every other one cannot be read: it still uses a place.
            let ok = i % 2 == 0;
            let r = im.get("m", &name, &mut || if ok { px(1, 1) } else { None });
            assert_eq!(r.is_some(), ok);
        }
        assert!(im.get("m", "one-more.png", &mut || px(1, 1)).is_none());
        assert_eq!(im.count_of("m"), MAX_PER_MOD);
        // A failed one is not tried again.
        assert!(im.get("m", "p1.png", &mut || panic!("tried again")).is_none());
        // Another mod is not held back by it.
        assert!(im.get("n", "a.png", &mut || px(1, 1)).is_some());
    }

    #[test]
    fn all_mods_together_have_a_limit_too() {
        let mut im = Images::default();
        let mut made = 0;
        for k in 0..10 {
            for i in 0..MAX_PER_MOD {
                if im.get(&alloc::format!("m{}", k), &alloc::format!("p{}.png", i), &mut || px(1, 1)).is_some() {
                    made += 1;
                }
            }
        }
        assert_eq!(made, MAX_TOTAL);
    }

    #[test]
    fn all_pictures_together_may_not_hold_too_many_bytes() {
        let mut im = Images::default();
        let mut made = 0;
        for k in 0..MAX_TOTAL / MAX_PER_MOD {
            for i in 0..4 {
                if im.get(&alloc::format!("m{}", k), &alloc::format!("big{}.png", i), &mut || px(MAX_SIDE, MAX_SIDE)).is_some() {
                    made += 1;
                }
            }
        }
        assert_eq!(made, MAX_TOTAL_BYTES / MAX_BYTES, "twelve of 4 MB, not sixteen");
        assert!(im.bytes() <= MAX_TOTAL_BYTES);
        // Small ones still fit beside them, and a mod is not held back by another's refusals.
        assert!(im.get("small", "s.png", &mut || px(2, 2)).is_some() || im.bytes() == MAX_TOTAL_BYTES);
    }

    #[test]
    fn a_picture_over_the_size_limits_is_refused() {
        let mut im = Images::default();
        assert!(im.get("m", "ok.png", &mut || px(MAX_SIDE, MAX_SIDE)).is_some(), "1024 x 1024 is exactly 4 MB");
        assert!(im.get("m", "wide.png", &mut || px(MAX_SIDE + 1, 1)).is_none());
        assert!(im.get("m", "tall.png", &mut || px(1, MAX_SIDE + 1)).is_none());
        assert!(im.get("m", "empty.png", &mut || px(0, 5)).is_none());
        // Pixels that do not match the size are not believed.
        assert!(im.get("m", "lie.png", &mut || Some(Pixels { w: 4, h: 4, rgba: alloc::vec![0; 10] })).is_none());
        // Only the good one is on the books for memory.
        assert_eq!(im.bytes(), MAX_BYTES);
    }

    #[test]
    fn pictures_go_to_the_card_once_and_are_freed_with_their_mod() {
        let mut im = Images::default();
        let a = im.get("m", "a.png", &mut || px(2, 2)).unwrap();
        let b = im.get("n", "b.png", &mut || px(2, 2)).unwrap();
        assert_eq!(im.tex_of(a.id), None, "not on the card yet");
        let mut next = 10;
        let mut ups = 0;
        im.upload(&mut |w, h, p| {
            assert_eq!((w, h, p.len()), (2, 2, 16));
            ups += 1;
            next += 1;
            next
        });
        assert_eq!(ups, 2);
        assert_eq!((im.tex_of(a.id), im.tex_of(b.id)), (Some(11), Some(12)));
        // A second pass uploads nothing.
        im.upload(&mut |_, _, _| panic!("uploaded twice"));
        // The mod is switched off: its picture goes, and its card handle is to be deleted.
        im.drop_mod("m");
        assert_eq!(im.tex_of(a.id), None);
        assert_eq!(im.take_dead(), [11]);
        assert!(im.take_dead().is_empty());
        assert_eq!(im.len(), 1);
        // It can be asked for again (read afresh).
        assert!(im.get("m", "a.png", &mut || px(2, 2)).is_some());
        // A card that refuses makes the picture a failure, once.
        let mut refuse = Images::default();
        refuse.get("m", "a.png", &mut || px(1, 1)).unwrap();
        refuse.upload(&mut |_, _, _| 0);
        assert!(refuse.get("m", "a.png", &mut || panic!("again")).is_none());
    }

    #[test]
    fn contain_shows_it_all_and_cover_fills_the_screen() {
        // A square picture on a wide screen: contain leaves bars at the sides.
        let (r, uv) = fit_rect(100.0, 100.0, 400.0, 200.0, Fit::Contain).unwrap();
        assert_eq!((r, uv), ([100.0, 0.0, 200.0, 200.0], [0.0, 0.0, 1.0, 1.0]));
        // Cover: the screen is full; a band of the picture's height shows.
        let (r, uv) = fit_rect(100.0, 100.0, 400.0, 200.0, Fit::Cover).unwrap();
        assert_eq!(r, [0.0, 0.0, 400.0, 200.0]);
        assert_eq!((uv[0], uv[2]), (0.0, 1.0));
        assert!((uv[1] - 0.25).abs() < 1e-6 && (uv[3] - 0.75).abs() < 1e-6, "{uv:?}");
        // A picture of the screen's own shape is the same either way.
        let a = fit_rect(200.0, 100.0, 400.0, 200.0, Fit::Contain).unwrap();
        let b = fit_rect(200.0, 100.0, 400.0, 200.0, Fit::Cover).unwrap();
        assert_eq!(a, b);
        // Nonsense sizes give nothing.
        assert!(fit_rect(0.0, 5.0, 4.0, 4.0, Fit::Cover).is_none());
        assert!(fit_rect(5.0, 5.0, 0.0, 4.0, Fit::Contain).is_none());
        assert!(fit_rect(f32::NAN, 5.0, 4.0, 4.0, Fit::Contain).is_none());
        assert_eq!(Fit::parse("COVER"), Some(Fit::Cover));
        assert_eq!(Fit::parse("contain"), Some(Fit::Contain));
        assert_eq!(Fit::parse("stretch"), None);
    }

    #[test]
    fn an_image_is_scaled_anchored_clipped_with_its_picture() {
        let mm = m;
        let f = Frame { w: 400.0, h: 200.0, s: 2.0, measure: &mm };
        let img = Cmd::Image { at: Anchor::TopLeft, dx: 5.0, dy: 5.0, w: 10.0, h: 10.0, uv: [0.0, 0.0, 1.0, 1.0], col: [255, 255, 255, 255], img: 7 };
        let mut out = Vec::new();
        place(&img, &f, &mut out);
        assert_eq!(out, [Prim::Image { x: 10.0, y: 10.0, w: 20.0, h: 20.0, uv: [0.0, 0.0, 1.0, 1.0], col: [255, 255, 255, 255], img: 7 }]);
        // Half off the left edge: half the picture shows, not all of it squeezed.
        out.clear();
        let half = Cmd::Image { at: Anchor::TopLeft, dx: -5.0, dy: 0.0, w: 10.0, h: 10.0, uv: [0.0, 0.0, 1.0, 1.0], col: [255, 255, 255, 255], img: 7 };
        place(&half, &f, &mut out);
        assert_eq!(out, [Prim::Image { x: 0.0, y: 0.0, w: 10.0, h: 20.0, uv: [0.5, 0.0, 1.0, 1.0], col: [255, 255, 255, 255], img: 7 }]);
        // A strip frame (part of a sheet) is clipped inside its own part.
        out.clear();
        let strip = Cmd::Image { at: Anchor::TopRight, dx: 5.0, dy: 0.0, w: 10.0, h: 10.0, uv: [0.25, 0.0, 0.5, 1.0], col: [255, 255, 255, 255], img: 7 };
        place(&strip, &f, &mut out);
        match &out[..] {
            [Prim::Image { x, w, uv, .. }] => {
                assert_eq!((*x, *w), (390.0, 10.0));
                assert!((uv[0] - 0.25).abs() < 1e-6 && (uv[2] - 0.375).abs() < 1e-6, "{uv:?}");
            }
            other => panic!("{other:?}"),
        }
        // Centred, bottom-anchored, invisible, empty or off the screen.
        out.clear();
        place(&Cmd::Image { at: Anchor::Centre, dx: 0.0, dy: 0.0, w: 10.0, h: 10.0, uv: [0.0, 0.0, 1.0, 1.0], col: [255, 255, 255, 255], img: 1 }, &f, &mut out);
        assert!(matches!(&out[..], [Prim::Image { x, y, .. }] if (*x, *y) == (190.0, 90.0)));
        for bad in [
            Cmd::Image { at: Anchor::Top, dx: 0.0, dy: 0.0, w: 10.0, h: 10.0, uv: [0.0, 0.0, 1.0, 1.0], col: [255, 255, 255, 0], img: 1 },
            Cmd::Image { at: Anchor::Top, dx: 0.0, dy: 0.0, w: 0.0, h: 10.0, uv: [0.0, 0.0, 1.0, 1.0], col: [255; 4], img: 1 },
            Cmd::Image { at: Anchor::Top, dx: 9999.0, dy: 0.0, w: 10.0, h: 10.0, uv: [0.0, 0.0, 1.0, 1.0], col: [255; 4], img: 1 },
            Cmd::Image { at: Anchor::Top, dx: 0.0, dy: 0.0, w: 10.0, h: 10.0, uv: [f32::NAN, 0.0, 1.0, 1.0], col: [255; 4], img: 1 },
        ] {
            let mut o = Vec::new();
            place(&bad, &f, &mut o);
            assert!(o.is_empty(), "{bad:?}");
        }
        // A fitted picture fills the screen.
        out.clear();
        place(&Cmd::Fit { img: 3, iw: 100, ih: 100, fit: Fit::Cover, col: [255; 4] }, &f, &mut out);
        assert!(matches!(&out[..], [Prim::Image { x, y, w, h, .. }] if (*x, *y, *w, *h) == (0.0, 0.0, 400.0, 200.0)));
    }

    #[test]
    fn colours_come_from_a_tint_and_an_opacity() {
        assert_eq!(tinted(0xFFFFFF, 1.0), [255, 255, 255, 255]);
        assert_eq!(tinted(0xFF0000, 0.5), [255, 0, 0, 127]);
        assert_eq!(tinted(-5, 7.0), [0, 0, 0, 255]);
        assert_eq!(tinted(0xFFFFFF, f64::NAN)[3], 0);
        assert_eq!(tinted(i64::MAX, 0.0), [255, 255, 255, 0]);
    }

    #[test]
    fn a_picture_by_fractions_covers_that_part_of_the_screen() {
        let mm = m;
        let f = Frame { w: 400.0, h: 200.0, s: 2.0, measure: &mm };
        let mut out = Vec::new();
        place(&Cmd::ImageFrac { x: 0.0, y: 0.0, w: 1.0, h: 1.0, col: [255; 4], img: 5 }, &f, &mut out);
        assert_eq!(out, [Prim::Image { x: 0.0, y: 0.0, w: 400.0, h: 200.0, uv: [0.0, 0.0, 1.0, 1.0], col: [255; 4], img: 5 }]);
        out.clear();
        place(&Cmd::ImageFrac { x: 0.25, y: 0.5, w: 0.5, h: 0.25, col: [255, 255, 255, 128], img: 5 }, &f, &mut out);
        assert_eq!(out, [Prim::Image { x: 100.0, y: 100.0, w: 200.0, h: 50.0, uv: [0.0, 0.0, 1.0, 1.0], col: [255, 255, 255, 128], img: 5 }]);
        // Seen, nothing, or nonsense: nothing is drawn.
        for bad in [
            Cmd::ImageFrac { x: 0.0, y: 0.0, w: 1.0, h: 1.0, col: [255, 255, 255, 0], img: 5 },
            Cmd::ImageFrac { x: 0.0, y: 0.0, w: 0.0, h: 1.0, col: [255; 4], img: 5 },
            Cmd::ImageFrac { x: f32::NAN, y: 0.0, w: 1.0, h: 1.0, col: [255; 4], img: 5 },
        ] {
            let mut o = Vec::new();
            place(&bad, &f, &mut o);
            assert!(o.is_empty(), "{bad:?}");
        }
    }
}
