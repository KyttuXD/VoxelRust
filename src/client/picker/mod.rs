//! A file picker that works everywhere without outside tools: the player
//! browses folders in the game and chooses a file.
//!
//! - `listing`: reading a folder, sorting and filtering it, path helpers
//! - this file: the picker's state and the pure logic (selection, scrolling,
//!   the preview picture)
//! - `screen`: drawing it and handling input with the interface toolkit
//!
//! To use it for something else (textures, sounds), call [`Picker::open`]
//! with a title and the extensions wanted, call [`Picker::show`] every frame
//! while it is on screen, and act on [`Outcome::Chosen`] (the full path).

pub mod listing;
mod screen;

pub use listing::{file_name, home_dir, join, normalize, parent, Entry};
use crate::png::Image;
use alloc::string::String;
use alloc::vec::Vec;
use listing::{build_entries, shortcuts};

/// What the picker wants to tell its owner this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Still open.
    Open,
    /// A file was chosen: its full path.
    Chosen(String),
    /// Cancelled (Escape or the CANCEL button).
    Cancelled,
}

/// The selection after moving it by `delta` rows in a list of `len`. With
/// nothing selected, down picks the first row and up the last.
pub fn step(sel: Option<usize>, len: usize, delta: i32) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(match sel {
        None => {
            if delta >= 0 {
                0
            } else {
                len - 1
            }
        }
        Some(i) => (i as i32 + delta).clamp(0, len as i32 - 1) as usize,
    })
}

/// The first row to show so that row `sel` is visible, `shown` rows at a
/// time, changing `scroll` as little as possible.
pub fn scroll_to_show(scroll: usize, sel: usize, shown: usize, len: usize) -> usize {
    let max = len.saturating_sub(shown);
    let s = if sel < scroll {
        sel
    } else if shown > 0 && sel >= scroll + shown {
        sel + 1 - shown
    } else {
        scroll
    };
    s.min(max)
}

/// The next row after `from` (wrapping round) whose name starts with `c`.
pub fn jump_to_letter(entries: &[Entry], from: Option<usize>, c: char) -> Option<usize> {
    let c = c.to_ascii_lowercase();
    let n = entries.len();
    (1..=n).map(|k| (from.map_or(n.saturating_sub(1), |f| f) + k) % n.max(1)).find(|&i| entries[i].name.chars().next().map_or(false, |f| f.to_ascii_lowercase() == c))
}

/// The end of a path that fits in `chars` characters, with `...` in front
/// when the start had to go.
pub fn path_tail(path: &str, chars: usize) -> String {
    let len = path.chars().count();
    if len <= chars || chars < 4 {
        return String::from(path);
    }
    let keep = chars - 3;
    let tail: String = path.chars().skip(len - keep).collect();
    alloc::format!("...{}", tail)
}

/// A picture as 64 x 64 RGBA for the preview: shrunk to fit (averaging), or
/// enlarged by a whole number (hard pixels), centred, with the rest clear.
/// Also gives the size it was drawn at.
pub fn preview_pixels(img: &Image) -> (Vec<u8>, usize, usize) {
    const N: usize = 64;
    let mut out = alloc::vec![0u8; N * N * 4];
    if img.w == 0 || img.h == 0 || img.rgba.len() < img.w * img.h * 4 {
        return (out, 0, 0);
    }
    let big = img.w.max(img.h);
    // (width, height) to draw at.
    let (tw, th) = if big >= N {
        ((img.w * N / big).max(1), (img.h * N / big).max(1))
    } else {
        let k = N / big;
        (img.w * k, img.h * k)
    };
    let (ox, oy) = ((N - tw) / 2, (N - th) / 2);
    for y in 0..th {
        let (y0, y1) = (y * img.h / th, ((y + 1) * img.h / th).max(y * img.h / th + 1).min(img.h));
        for x in 0..tw {
            let (x0, x1) = (x * img.w / tw, ((x + 1) * img.w / tw).max(x * img.w / tw + 1).min(img.w));
            let (mut sum, mut a_sum, mut n) = ([0u32; 3], 0u32, 0u32);
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let p = &img.rgba[(sy * img.w + sx) * 4..][..4];
                    for c in 0..3 {
                        sum[c] += p[c] as u32 * p[3] as u32;
                    }
                    a_sum += p[3] as u32;
                    n += 1;
                }
            }
            let o = ((oy + y) * N + ox + x) * 4;
            if a_sum > 0 {
                for c in 0..3 {
                    out[o + c] = (sum[c] / a_sum) as u8;
                }
            }
            out[o + 3] = if n > 0 { (a_sum / n) as u8 } else { 0 };
        }
    }
    (out, tw, th)
}

/// What is known about the picture under the cursor.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Look {
    None,
    Unreadable,
    Size(usize, usize),
}

pub struct Picker {
    pub title: String,
    pub dir: String,
    pub entries: Vec<Entry>,
    pub sel: Option<usize>,
    pub scroll: usize,
    /// Wanted extensions, lower case without the dot (empty: all files).
    pub exts: Vec<String>,
    /// Which picture sizes are fine, and a line saying so.
    pub size_ok: Option<fn(usize, usize) -> bool>,
    pub hint: String,
    pub shortcuts: Vec<(String, String)>,
    /// Row and time of the last click, for double clicks.
    last_click: Option<(usize, f32)>,
    /// The preview texture (made on first use) and what it shows.
    tex: u32,
    shown: Option<String>,
    look: Look,
    note: String,
}

impl Picker {
    pub fn new() -> Picker {
        Picker {
            title: String::new(),
            dir: String::new(),
            entries: Vec::new(),
            sel: None,
            scroll: 0,
            exts: Vec::new(),
            size_ok: None,
            hint: String::new(),
            shortcuts: Vec::new(),
            last_click: None,
            tex: 0,
            shown: None,
            look: Look::None,
            note: String::new(),
        }
    }

    /// Start browsing at `dir` (or the first of `fallbacks` that is a
    /// folder), showing folders and files ending in `exts`.
    pub fn open(&mut self, title: &str, dir: &str, fallbacks: &[String], exts: &[&str], data: &str) {
        self.title = String::from(title);
        self.exts = exts.iter().map(|e| String::from(*e)).collect();
        self.shortcuts = shortcuts(data);
        self.size_ok = None;
        self.hint.clear();
        self.note.clear();
        self.last_click = None;
        let start = core::iter::once(&String::from(dir)).chain(fallbacks.iter()).map(|d| normalize(d)).find(|d| !d.is_empty() && crate::sys::is_dir(d)).unwrap_or_else(|| String::from("/"));
        self.go(&start, None);
    }

    /// Go to a folder, selecting `select` in it if it is listed.
    pub fn go(&mut self, dir: &str, select: Option<&str>) {
        self.dir = normalize(dir);
        let names = crate::sys::list_dir(&self.dir);
        let dir_path = self.dir.clone();
        let exts: Vec<&str> = self.exts.iter().map(|e| e.as_str()).collect();
        self.entries = build_entries(&names, |n| crate::sys::is_dir(&join(&dir_path, n)), &exts);
        self.sel = select.and_then(|n| self.entries.iter().position(|e| e.name == n));
        self.scroll = 0;
        self.last_click = None;
        self.shown = None;
        self.look = Look::None;
        self.note = if names.is_empty() && !crate::sys::is_dir(&self.dir) { String::from("CANNOT OPEN THIS FOLDER") } else { String::new() };
    }

    pub fn up(&mut self) {
        if let Some(p) = parent(&self.dir) {
            let came_from = String::from(file_name(&self.dir));
            self.go(&p, Some(&came_from));
        }
    }

    /// The path of the selected file, if a file (not a folder) is selected.
    pub fn selected_file(&self) -> Option<String> {
        let e = self.entries.get(self.sel?)?;
        if e.dir { None } else { Some(join(&self.dir, &e.name)) }
    }

    /// Open the selected folder, or choose the selected file.
    fn activate(&mut self) -> Outcome {
        let e = match self.sel.and_then(|i| self.entries.get(i)) {
            Some(e) => e.clone(),
            None => return Outcome::Open,
        };
        if e.dir {
            let to = join(&self.dir, &e.name);
            self.go(&to, None);
            self.sel = if self.entries.is_empty() { None } else { Some(0) };
            Outcome::Open
        } else {
            Outcome::Chosen(join(&self.dir, &e.name))
        }
    }

    /// Move the selection and keep it in view.
    fn move_sel(&mut self, delta: i32, shown: usize) {
        self.sel = step(self.sel, self.entries.len(), delta);
        if let Some(i) = self.sel {
            self.scroll = scroll_to_show(self.scroll, i, shown, self.entries.len());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_moves_and_stays_in_range() {
        assert_eq!(step(None, 5, 1), Some(0));
        assert_eq!(step(None, 5, -1), Some(4));
        assert_eq!(step(Some(2), 5, 1), Some(3));
        assert_eq!(step(Some(4), 5, 1), Some(4));
        assert_eq!(step(Some(1), 5, -9), Some(0));
        assert_eq!(step(Some(1), 5, 99), Some(4));
        assert_eq!(step(Some(0), 0, 1), None);
    }

    #[test]
    fn scrolling_keeps_the_selected_row_in_view() {
        // 20 rows, 5 shown.
        assert_eq!(scroll_to_show(0, 3, 5, 20), 0);
        assert_eq!(scroll_to_show(0, 5, 5, 20), 1);
        assert_eq!(scroll_to_show(10, 4, 5, 20), 4);
        assert_eq!(scroll_to_show(10, 12, 5, 20), 10);
        assert_eq!(scroll_to_show(18, 19, 5, 20), 15);
        assert_eq!(scroll_to_show(3, 0, 5, 3), 0);
    }

    #[test]
    fn typing_a_letter_jumps_to_the_next_name_with_it() {
        let e: Vec<Entry> = ["apple", "Bob.png", "banana.png", "cat.png"].iter().map(|n| Entry { name: String::from(*n), dir: false }).collect();
        assert_eq!(jump_to_letter(&e, None, 'b'), Some(1));
        assert_eq!(jump_to_letter(&e, Some(1), 'b'), Some(2));
        assert_eq!(jump_to_letter(&e, Some(2), 'B'), Some(1));
        assert_eq!(jump_to_letter(&e, None, 'z'), None);
        assert_eq!(jump_to_letter(&[], None, 'a'), None);
    }

    #[test]
    fn long_paths_keep_their_end() {
        assert_eq!(path_tail("/a/b", 10), "/a/b");
        assert_eq!(path_tail("/home/user/Downloads/skins", 12), "...ads/skins");
    }

    #[test]
    fn preview_fits_pictures_into_the_square() {
        // A 64 x 32 skin keeps its shape, centred.
        let img = Image { w: 64, h: 32, rgba: [10u8, 20, 30, 255].repeat(64 * 32) };
        let (px, w, h) = preview_pixels(&img);
        assert_eq!((w, h), (64, 32));
        assert_eq!(px[(16 * 64) * 4 + 3], 255);
        assert_eq!(px[3], 0);
        // A 16 x 16 one is enlarged four times.
        let img = Image { w: 16, h: 16, rgba: [200u8, 0, 0, 255].repeat(256) };
        let (px, w, h) = preview_pixels(&img);
        assert_eq!((w, h), (64, 64));
        assert_eq!(&px[..4], &[200, 0, 0, 255]);
        // A big picture is averaged down.
        let img = Image { w: 256, h: 128, rgba: [0u8, 100, 0, 255].repeat(256 * 128) };
        let (px, w, h) = preview_pixels(&img);
        assert_eq!((w, h), (64, 32));
        assert_eq!(px[(20 * 64 + 5) * 4 + 1], 100);
        // A broken picture gives nothing.
        let (_, w, h) = preview_pixels(&Image { w: 0, h: 0, rgba: Vec::new() });
        assert_eq!((w, h), (0, 0));
    }
}
