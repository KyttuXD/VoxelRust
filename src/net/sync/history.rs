//! The last lines of chat, told to a player who joins so that they are not
//! left wondering what everyone is talking about.

use alloc::collections::VecDeque;
use alloc::string::String;

/// How many lines are kept.
pub const KEEP: usize = 10;
/// Longest line kept, in bytes (a message line is at most 200).
const MAX_LINE: usize = 150;

#[derive(Default)]
pub struct Chatlog {
    lines: VecDeque<String>,
}

impl Chatlog {
    /// Remember a line (cut to a sensible length); the oldest goes after [`KEEP`].
    pub fn push(&mut self, line: &str) {
        let cut: String = line.chars().filter(|c| (' '..='~').contains(c)).take(MAX_LINE).collect();
        if cut.trim().is_empty() {
            return;
        }
        if self.lines.len() >= KEEP {
            self.lines.pop_front();
        }
        self.lines.push_back(cut);
    }

    pub fn lines(&self) -> impl Iterator<Item = &String> {
        self.lines.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_last_ten_lines_stay_and_odd_characters_go() {
        let mut c = Chatlog::default();
        assert!(c.is_empty());
        for i in 0..14 {
            c.push(&alloc::format!("<Ann> line {i}"));
        }
        c.push("   ");
        c.push("a\u{7}b\u{e9}c");
        let all: alloc::vec::Vec<&String> = c.lines().collect();
        assert_eq!(all.len(), KEEP);
        assert_eq!(all[0], "<Ann> line 5");
        assert_eq!(all[KEEP - 1], "abc");
    }
}
