//! What a mod's scripts keep between calls and between sessions:
//! `state_get(key)` / `state_set(key, value)`. Text to text, saved to the
//! mod's own data folder (`<data>/mods-data/<mod>/script-state.txt`) when the
//! world is saved and read back when the script is loaded.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

pub const MAX_KEYS: usize = 512;
pub const MAX_KEY: usize = 64;
pub const MAX_VALUE: usize = 4096;

#[derive(Default, Debug, Clone, PartialEq)]
pub struct ScriptState {
    pub map: BTreeMap<String, String>,
    /// Changed since it was last saved.
    pub dirty: bool,
}

fn escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '=' => out.push_str("\\e"),
            c => out.push(c),
        }
    }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('e') => out.push('='),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl ScriptState {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(|s| s.as_str())
    }

    /// Keep `value` under `key`. `Err` says why not.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        if key.is_empty() || key.len() > MAX_KEY {
            return Err(alloc::format!("a state key must be 1 to {} bytes long", MAX_KEY));
        }
        if value.len() > MAX_VALUE {
            return Err(alloc::format!("a state value may not be longer than {} bytes", MAX_VALUE));
        }
        if !self.map.contains_key(key) && self.map.len() >= MAX_KEYS {
            return Err(alloc::format!("a mod may keep at most {} state keys", MAX_KEYS));
        }
        if self.map.get(key).map(|s| s.as_str()) != Some(value) {
            self.map.insert(String::from(key), String::from(value));
            self.dirty = true;
        }
        Ok(())
    }

    pub fn remove(&mut self, key: &str) -> bool {
        let gone = self.map.remove(key).is_some();
        if gone {
            self.dirty = true;
        }
        gone
    }

    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (k, v) in &self.map {
            out.push_str(&escape(k));
            out.push('=');
            out.push_str(&escape(v));
            out.push('\n');
        }
        out
    }

    pub fn from_text(text: &str) -> ScriptState {
        let mut map = BTreeMap::new();
        let lines: Vec<&str> = text.lines().collect();
        for l in lines.into_iter().take(MAX_KEYS) {
            if let Some((k, v)) = l.split_once('=') {
                let k = unescape(k);
                if !k.is_empty() && k.len() <= MAX_KEY {
                    map.insert(k, unescape(v));
                }
            }
        }
        ScriptState { map, dirty: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get() {
        let mut s = ScriptState::default();
        assert_eq!(s.get("a"), None);
        s.set("a", "1").unwrap();
        assert_eq!(s.get("a"), Some("1"));
        assert!(s.dirty);
        s.dirty = false;
        s.set("a", "1").unwrap();
        assert!(!s.dirty, "setting the same value is not a change");
    }

    #[test]
    fn limits() {
        let mut s = ScriptState::default();
        assert!(s.set("", "x").is_err());
        assert!(s.set(&"k".repeat(65), "x").is_err());
        assert!(s.set("k", &"v".repeat(MAX_VALUE + 1)).is_err());
        for i in 0..MAX_KEYS {
            s.set(&alloc::format!("k{}", i), "v").unwrap();
        }
        assert!(s.set("one more", "v").is_err());
        assert!(s.set("k1", "changed").is_ok());
    }

    #[test]
    fn the_text_form_round_trips() {
        let mut s = ScriptState::default();
        s.set("plain", "value").unwrap();
        s.set("odd=key", "line one\nline two \\ with = signs").unwrap();
        let back = ScriptState::from_text(&s.to_text());
        assert_eq!(back.map, s.map);
        assert!(!back.dirty);
    }

    #[test]
    fn garbage_lines_are_skipped() {
        let s = ScriptState::from_text("no equals sign\n=empty key\nok=1\n");
        assert_eq!(s.map.len(), 1);
        assert_eq!(s.get("ok"), Some("1"));
    }
}
