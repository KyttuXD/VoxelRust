//! Lines of text a script puts on the screen (the host call `hud`): one line
//! per mod, shown for a moment and then gone, so a script that stops, is
//! switched off or is unloaded takes its line away with it.

use alloc::string::String;
use alloc::vec::Vec;

/// How long a line stays after the script last set it, in seconds. A script
/// sets its line every half second or so.
pub const LIFE: f32 = 1.5;
/// Most lines at once.
pub const MAX_LINES: usize = 6;
/// Longest line, in characters.
pub const MAX_CHARS: usize = 48;

#[derive(Default, Clone)]
pub struct HudLines {
    lines: Vec<(String, String, f32)>,
}

impl HudLines {
    /// Set the line of `key` (the mod's name). Empty text takes it away.
    pub fn set(&mut self, key: &str, text: &str) {
        let shown: String = text.chars().filter(|c| c.is_ascii_graphic() || *c == ' ').take(MAX_CHARS).collect::<String>().to_ascii_uppercase();
        let shown = String::from(shown.trim());
        if shown.is_empty() {
            self.lines.retain(|l| l.0 != key);
            return;
        }
        if let Some(i) = self.lines.iter().position(|l| l.0 == key) {
            self.lines[i].1 = shown;
            self.lines[i].2 = LIFE;
        } else if self.lines.len() < MAX_LINES {
            self.lines.push((String::from(key), shown, LIFE));
        }
    }

    /// Let `dt` seconds pass; lines that were not renewed go.
    pub fn tick(&mut self, dt: f32) {
        if self.lines.is_empty() {
            return;
        }
        for l in self.lines.iter_mut() {
            l.2 -= dt;
        }
        self.lines.retain(|l| l.2 > 0.0);
    }

    /// The lines to draw, top to bottom.
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(|l| l.1.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_kept_renewed_and_dropped() {
        let mut h = HudLines::default();
        h.set("a", "x 1 y 2");
        h.set("b", "noon");
        assert_eq!(h.texts().collect::<Vec<_>>(), ["X 1 Y 2", "NOON"]);
        h.tick(1.0);
        h.set("a", "x 3");
        h.tick(0.6);
        assert_eq!(h.texts().collect::<Vec<_>>(), ["X 3"]);
        h.set("a", "");
        assert!(h.is_empty());
    }

    #[test]
    fn lines_are_few_and_short() {
        let mut h = HudLines::default();
        for i in 0..20 {
            h.set(&alloc::format!("m{}", i), "hello");
        }
        assert_eq!(h.texts().count(), MAX_LINES);
        h.set("m0", &"w".repeat(200));
        assert!(h.texts().next().unwrap().len() <= MAX_CHARS);
    }
}
