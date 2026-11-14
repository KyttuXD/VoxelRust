//! Lines and sections of a pack that are for scripts.
//!
//! The game keeps a top-level pack line or an indented section it does not
//! know only if something claimed it: a mod's compiled code (see
//! `native::reads_line`) or, here, a pack that says
//!
//! ```text
//! claims line manfromthefog
//! claims section vehicle
//! ```
//!
//! The claim holds for every pack read in the same loading, whichever comes
//! first, so a pack may use another pack's sections (`vehicle` sections in the
//! boats pack, read by the vehicles mod's script). Scripts read what was kept
//! with `pack_lines` and `pack_sections` (`script/ext/pack.rs`).

use alloc::string::String;
use alloc::vec::Vec;

/// Read the `claims` lines of one pack file into the two lists.
pub fn scan(text: &str, lines: &mut Vec<String>, sections: &mut Vec<String>) {
    for raw in text.lines() {
        if raw.starts_with(char::is_whitespace) {
            continue;
        }
        let line = match crate::palette::comment_start(raw) {
            Some(i) => &raw[..i],
            None => raw,
        };
        let mut w = line.split_whitespace();
        if w.next() != Some("claims") {
            continue;
        }
        let list = match w.next() {
            Some("line") => &mut *lines,
            Some("section") => &mut *sections,
            _ => continue,
        };
        for word in w {
            // Words the game itself reads cannot be taken over.
            if !list.iter().any(|k| k == word) && list.len() < 64 && !RESERVED.contains(&word) {
                list.push(String::from(word));
            }
        }
    }
}

/// Pack keywords a claim may not take (a claim must not change what a pack means).
const RESERVED: &[&str] = &["texture", "block", "item", "entity", "model", "recipe", "craft", "shapeless", "shaped", "bake", "smelt", "group", "include", "ore", "plant", "sound", "command", "when", "dimension", "feature", "episode", "setting", "effect", "emote", "picture", "palette", "version", "client", "claims"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_are_read_from_the_top_level_only() {
        let (mut l, mut s) = (Vec::new(), Vec::new());
        scan("claims line a b # note\nclaims section vehicle\n  claims line hidden\nblock x\nclaims line a texture\nclaims what nothing", &mut l, &mut s);
        assert_eq!(l, ["a", "b"]);
        assert_eq!(s, ["vehicle"]);
    }
}
