//! Key bindings and switches that mods declare.
//!
//! A mod says, once, which keys it listens to ([`Bind`]: an id, a label, a
//! group and a default key) and which on/off rows it wants on the mod
//! switches page ([`Switch`]). Scripts do it in `on_register()`
//! (`bind_key`, `add_switch`); native code lists them in `Native::bindings`
//! and `Native::switches`. This file is the pure part: checking what was
//! declared, working out the key each binding has now (what the player
//! chose, saved in `settings.txt`, or the default), finding two bindings on
//! one key, and laying the Settings > CONTROLS > KEY BINDINGS list out in
//! lines. The drawing is in `bind_page.rs`.

use crate::client::modui::limits::*;
use crate::settings::Settings;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

/// A key binding as native code declares it (static data).
#[derive(Clone, Copy, Debug)]
pub struct KeyDecl {
    pub id: &'static str,
    pub label: &'static str,
    /// The heading it is listed under (after the mod's name); may be empty.
    pub group: &'static str,
    /// The SDL scancode it starts on (see `key_scancode`).
    pub key: i32,
}

/// A switch as native code declares it (static data).
#[derive(Clone, Copy, Debug)]
pub struct SwitchDecl {
    pub id: &'static str,
    pub label: &'static str,
    pub default: bool,
}

/// One key binding a mod declared.
#[derive(Clone, Debug, PartialEq)]
pub struct Bind {
    pub mod_name: String,
    pub id: String,
    pub label: String,
    pub group: String,
    pub default_key: i32,
}

/// One switch row a mod declared.
#[derive(Clone, Debug, PartialEq)]
pub struct Switch {
    pub mod_name: String,
    pub id: String,
    pub label: String,
    pub default: bool,
}

/// Everything declared by the scripts (or by one script, while it loads).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decls {
    pub binds: Vec<Bind>,
    pub switches: Vec<Switch>,
}

impl Decls {
    /// Declare a key binding. The same (mod, id) again replaces the first
    /// declaration. `Err` says why it was refused.
    pub fn add_bind(&mut self, mod_name: &str, id: &str, label: &str, group: &str, key: i32) -> Result<(), String> {
        let Some(id) = clean_id(id) else { return Err(format!("`{}` is not a usable key id (letters, digits, _ and -, up to {})", id, MAX_ID)) };
        if !(1..=511).contains(&key) {
            return Err(format!("{} is not a key code (1 to 511)", key));
        }
        let mod_name = clean_mod(mod_name);
        let mut label = clean_text(label, MAX_LABEL);
        if label.is_empty() {
            label = clean_text(&id, MAX_LABEL);
        }
        let bind = Bind { mod_name: mod_name.clone(), id: id.clone(), label, group: clean_text(group, 24), default_key: key };
        if let Some(old) = self.binds.iter_mut().find(|b| b.mod_name == mod_name && b.id == id) {
            *old = bind;
            return Ok(());
        }
        if self.binds.iter().filter(|b| b.mod_name == mod_name).count() >= MAX_BINDS_PER_MOD || self.binds.len() >= MAX_BINDS {
            return Err(format!("too many key bindings (at most {} per mod)", MAX_BINDS_PER_MOD));
        }
        self.binds.push(bind);
        Ok(())
    }

    /// Declare a switch row (`flag` name is `<mod>.<id>`, see [`switch_flag`]).
    pub fn add_switch(&mut self, mod_name: &str, id: &str, label: &str, default: bool) -> Result<(), String> {
        let Some(id) = clean_id(id) else { return Err(format!("`{}` is not a usable switch id (letters, digits, _ and -, up to {})", id, MAX_ID)) };
        let mod_name = clean_mod(mod_name);
        let mut label = clean_text(label, MAX_LABEL);
        if label.is_empty() {
            label = clean_text(&id, MAX_LABEL);
        }
        let sw = Switch { mod_name: mod_name.clone(), id: id.clone(), label, default };
        if let Some(old) = self.switches.iter_mut().find(|s| s.mod_name == mod_name && s.id == id) {
            *old = sw;
            return Ok(());
        }
        if self.switches.iter().filter(|s| s.mod_name == mod_name).count() >= MAX_SWITCHES_PER_MOD || self.switches.len() >= MAX_SWITCHES {
            return Err(format!("too many switches (at most {} per mod)", MAX_SWITCHES_PER_MOD));
        }
        self.switches.push(sw);
        Ok(())
    }

    /// Forget what a mod declared.
    pub fn remove_mod(&mut self, mod_name: &str) {
        let m = clean_mod(mod_name);
        self.binds.retain(|b| b.mod_name != m);
        self.switches.retain(|s| s.mod_name != m);
    }

    /// Take over everything `other` declared (its mods replace ours of the same name).
    pub fn absorb(&mut self, other: Decls) {
        let mut names: Vec<String> = Vec::new();
        for n in other.binds.iter().map(|b| &b.mod_name).chain(other.switches.iter().map(|s| &s.mod_name)) {
            if !names.contains(n) {
                names.push(n.clone());
            }
        }
        for n in &names {
            self.remove_mod(n);
        }
        for b in other.binds {
            if self.binds.len() < MAX_BINDS {
                self.binds.push(b);
            }
        }
        for s in other.switches {
            if self.switches.len() < MAX_SWITCHES {
                self.switches.push(s);
            }
        }
    }
}

/// The name a switch is saved under in the settings' flags.
pub fn switch_flag(mod_name: &str, id: &str) -> String {
    format!("{}.{}", clean_mod(mod_name), id)
}

/// Whether a switch is on: what the player set, else its default.
pub fn switch_on(flags: &[(String, bool)], mod_name: &str, id: &str, default: bool) -> bool {
    let name = switch_flag(mod_name, id);
    flags.iter().find(|f| f.0 == name).map_or(default, |f| f.1)
}

/// A binding with the key it has now.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub mod_name: String,
    pub id: String,
    pub label: String,
    pub group: String,
    pub default_key: i32,
    pub key: i32,
}

/// What a mod's binding is called in the settings file.
pub fn setting_name(mod_name: &str, id: &str) -> String {
    format!("{}.{}", clean_mod(mod_name), id)
}

/// The bindings with their current keys (what the player chose, else the default).
pub fn resolve(binds: &[Bind], st: &Settings) -> Vec<Row> {
    binds
        .iter()
        .map(|b| {
            let name = setting_name(&b.mod_name, &b.id);
            let key = st.mod_keys.iter().find(|k| k.0 == name).map_or(b.default_key, |k| k.1);
            Row { mod_name: b.mod_name.clone(), id: b.id.clone(), label: b.label.clone(), group: b.group.clone(), default_key: b.default_key, key }
        })
        .collect()
}

/// Give a binding a key. Back on its default, nothing is kept.
pub fn set_key(st: &mut Settings, row: &Row, key: i32) {
    let name = setting_name(&row.mod_name, &row.id);
    st.mod_keys.retain(|k| k.0 != name);
    let key = key.clamp(1, 511);
    if key != row.default_key && st.mod_keys.len() < MAX_BINDS {
        st.mod_keys.push((name, key));
    }
}

/// Every mod binding back on its default key.
pub fn reset_all(st: &mut Settings) {
    st.mod_keys.clear();
}

/// The bindings on `scancode`, as (mod, id).
pub fn on_key(rows: &[Row], scancode: i32) -> Vec<(String, String)> {
    rows.iter().filter(|r| r.key == scancode).map(|r| (r.mod_name.clone(), r.id.clone())).collect()
}

/// For each key of a list (the game's own, then the mods'): whether another
/// one is on the same key. Key 0 means unbound and never conflicts.
pub fn conflicts(keys: &[i32]) -> Vec<bool> {
    keys.iter().enumerate().map(|(i, &k)| k != 0 && keys.iter().enumerate().any(|(j, &o)| j != i && o == k)).collect()
}

/// A run of bindings under one heading.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    /// `MOD NAME` or `MOD NAME: GROUP`.
    pub heading: String,
    /// Indexes into the rows, in order.
    pub rows: Vec<usize>,
}

/// The rows grouped by mod, then by group, each in the order first seen.
pub fn sections(rows: &[Row]) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut order: Vec<(String, String)> = Vec::new();
    for r in rows {
        let key = (r.mod_name.clone(), r.group.clone());
        if !order.contains(&key) {
            order.push(key);
        }
    }
    // Mods in the order first seen; a mod's groups stay together.
    let mut mods: Vec<String> = Vec::new();
    for (m, _) in &order {
        if !mods.contains(m) {
            mods.push(m.clone());
        }
    }
    for m in &mods {
        for (om, g) in order.iter().filter(|o| &o.0 == m) {
            let heading = if g.is_empty() { clean_text(om, 40) } else { format!("{}: {}", clean_text(om, 30), g) };
            let idx: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| &r.mod_name == om && &r.group == g).map(|(i, _)| i).collect();
            out.push(Section { heading, rows: idx });
        }
    }
    out
}

/// One line of the key bindings list.
#[derive(Clone, Debug, PartialEq)]
pub enum Line {
    Heading(String),
    /// One or two bindings side by side, as indexes into the flat list:
    /// `0..core` are the game's own, `core + i` is mod row `i`.
    Pair(usize, Option<usize>),
}

/// The whole list as lines: the game's own bindings first, two to a line,
/// then each section of the mods' under its heading.
pub fn page_lines(core: usize, sections: &[Section]) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut i = 0;
    while i < core {
        lines.push(Line::Pair(i, if i + 1 < core { Some(i + 1) } else { None }));
        i += 2;
    }
    for s in sections {
        if s.rows.is_empty() {
            continue;
        }
        lines.push(Line::Heading(s.heading.clone()));
        for pair in s.rows.chunks(2) {
            lines.push(Line::Pair(core + pair[0], pair.get(1).map(|b| core + b)));
        }
    }
    lines
}

/// Switch rows as lines in the same way (a heading per mod, two to a line;
/// the indexes are into `switches`).
pub fn switch_lines(switches: &[Switch]) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut mods: Vec<&String> = Vec::new();
    for s in switches {
        if !mods.contains(&&s.mod_name) {
            mods.push(&s.mod_name);
        }
    }
    for m in mods {
        lines.push(Line::Heading(clean_text(m, 40)));
        let idx: Vec<usize> = switches.iter().enumerate().filter(|(_, s)| &s.mod_name == m).map(|(i, _)| i).collect();
        for pair in idx.chunks(2) {
            lines.push(Line::Pair(pair[0], pair.get(1).copied()));
        }
    }
    lines
}

/// The key a name stands for, as an SDL scancode: a letter or digit, `F1`
/// to `F12`, `space`, `tab`, `enter`, `escape`, the arrows, `shift`, `ctrl`,
/// `alt` and the punctuation keys. 0 when the name is not known.
pub fn key_scancode(name: &str) -> i32 {
    let n = name.trim().to_ascii_lowercase();
    let b = n.as_bytes();
    if b.len() == 1 {
        match b[0] {
            c @ b'a'..=b'z' => return 4 + (c - b'a') as i32,
            c @ b'1'..=b'9' => return 30 + (c - b'1') as i32,
            b'0' => return 39,
            b'-' => return 45,
            b'=' => return 46,
            b'[' => return 47,
            b']' => return 48,
            b'\\' => return 49,
            b';' => return 51,
            b'\'' => return 52,
            b'`' => return 53,
            b',' => return 54,
            b'.' => return 55,
            b'/' => return 56,
            _ => {}
        }
    }
    if let Some(f) = n.strip_prefix('f') {
        if let Ok(k) = f.parse::<i32>() {
            if (1..=12).contains(&k) {
                return 57 + k;
            }
        }
    }
    match n.as_str() {
        "enter" | "return" => 40,
        "escape" | "esc" => 41,
        "backspace" => 42,
        "tab" => 43,
        "space" => 44,
        "insert" => 73,
        "home" => 74,
        "pageup" => 75,
        "delete" => 76,
        "end" => 77,
        "pagedown" => 78,
        "right" => 79,
        "left" => 80,
        "down" => 81,
        "up" => 82,
        "ctrl" | "lctrl" => 224,
        "shift" | "lshift" => 225,
        "alt" | "lalt" => 226,
        "rctrl" => 228,
        "rshift" => 229,
        "ralt" => 230,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> Settings {
        Settings::default()
    }

    #[test]
    fn declaring_checks_ids_keys_and_counts() {
        let mut d = Decls::default();
        assert!(d.add_bind("m", "compass", "toggle compass", "hud", 11).is_ok());
        assert_eq!(d.binds[0].label, "TOGGLE COMPASS");
        assert!(d.add_bind("m", "bad id", "x", "", 11).is_err());
        assert!(d.add_bind("m", "k", "x", "", 0).is_err());
        assert!(d.add_bind("m", "k", "x", "", 512).is_err());
        // Again: replaces.
        assert!(d.add_bind("m", "compass", "other", "", 12).is_ok());
        assert_eq!((d.binds.len(), d.binds[0].default_key, d.binds[0].label.as_str()), (1, 12, "OTHER"));
        for i in 1..MAX_BINDS_PER_MOD {
            assert!(d.add_bind("m", &format!("k{}", i), "", "", 20).is_ok(), "{}", i);
        }
        assert!(d.add_bind("m", "one-too-many", "", "", 20).is_err());
        assert!(d.add_bind("other", "fine", "", "", 20).is_ok());
        // An empty label becomes the id.
        assert_eq!(d.binds.iter().find(|b| b.id == "k1").unwrap().label, "K1");
    }

    #[test]
    fn the_total_of_bindings_is_capped() {
        let mut d = Decls::default();
        let mut refused = 0;
        for m in 0..10 {
            for k in 0..MAX_BINDS_PER_MOD {
                if d.add_bind(&format!("mod{}", m), &format!("k{}", k), "", "", 20).is_err() {
                    refused += 1;
                }
            }
        }
        assert_eq!(d.binds.len(), MAX_BINDS);
        assert_eq!(refused, 10 * MAX_BINDS_PER_MOD - MAX_BINDS);
    }

    #[test]
    fn switches_replace_cap_and_read_back() {
        let mut d = Decls::default();
        d.add_switch("m", "fast", "fast mode", false).unwrap();
        d.add_switch("m", "fast", "faster mode", true).unwrap();
        assert_eq!(d.switches.len(), 1);
        assert_eq!(d.switches[0].label, "FASTER MODE");
        for i in 1..MAX_SWITCHES_PER_MOD {
            d.add_switch("m", &format!("s{}", i), "", false).unwrap();
        }
        assert!(d.add_switch("m", "more", "", false).is_err());
        let flags = alloc::vec![(String::from("m.fast"), false)];
        assert!(!switch_on(&flags, "m", "fast", true));
        assert!(switch_on(&flags, "m", "other", true));
        assert!(!switch_on(&[], "m", "other", false));
    }

    #[test]
    fn a_chosen_key_is_kept_and_the_default_is_not() {
        let mut d = Decls::default();
        d.add_bind("m", "a", "A", "", 11).unwrap();
        let mut s = st();
        let rows = resolve(&d.binds, &s);
        assert_eq!(rows[0].key, 11);
        set_key(&mut s, &rows[0], 20);
        assert_eq!(resolve(&d.binds, &s)[0].key, 20);
        // It survives the settings file.
        let back = Settings::parse(&s.to_text());
        assert_eq!(resolve(&d.binds, &back)[0].key, 20);
        assert_eq!(back.mod_keys, s.mod_keys);
        // Back on the default: nothing is stored.
        set_key(&mut s, &rows[0], 11);
        assert!(s.mod_keys.is_empty());
        // Out of range keys are brought in.
        set_key(&mut s, &rows[0], 9999);
        assert_eq!(s.mod_keys[0].1, 511);
        reset_all(&mut s);
        assert!(s.mod_keys.is_empty());
    }

    #[test]
    fn keys_find_their_bindings_and_conflicts_are_marked_not_refused() {
        let mut d = Decls::default();
        d.add_bind("m", "a", "A", "", 11).unwrap();
        d.add_bind("m", "b", "B", "", 11).unwrap();
        d.add_bind("n", "c", "C", "", 12).unwrap();
        let rows = resolve(&d.binds, &st());
        // Both bindings on the key fire.
        assert_eq!(on_key(&rows, 11), [(String::from("m"), String::from("a")), (String::from("m"), String::from("b"))]);
        assert!(on_key(&rows, 99).is_empty());
        // The game's own keys count too.
        let mut keys: Vec<i32> = alloc::vec![26, 22, 12];
        keys.extend(rows.iter().map(|r| r.key));
        assert_eq!(conflicts(&keys), [false, false, true, true, true, true]);
        assert_eq!(conflicts(&[0, 0, 5]), [false, false, false]);
        assert!(conflicts(&[]).is_empty());
    }

    #[test]
    fn the_list_has_the_game_first_then_a_heading_per_mod_and_group() {
        let mut d = Decls::default();
        d.add_bind("alpha", "a", "A", "", 1).unwrap();
        d.add_bind("beta", "b", "B", "tools", 2).unwrap();
        d.add_bind("alpha", "c", "C", "", 3).unwrap();
        d.add_bind("alpha", "d", "D", "extra", 4).unwrap();
        d.add_bind("beta", "e", "E", "tools", 5).unwrap();
        d.add_bind("beta", "f", "F", "tools", 6).unwrap();
        let rows = resolve(&d.binds, &st());
        let secs = sections(&rows);
        let heads: Vec<&str> = secs.iter().map(|s| s.heading.as_str()).collect();
        assert_eq!(heads, ["ALPHA", "ALPHA: EXTRA", "BETA: TOOLS"]);
        let lines = page_lines(3, &secs);
        assert_eq!(
            lines,
            [
                Line::Pair(0, Some(1)),
                Line::Pair(2, None),
                Line::Heading(String::from("ALPHA")),
                Line::Pair(3, Some(5)),
                Line::Heading(String::from("ALPHA: EXTRA")),
                Line::Pair(6, None),
                Line::Heading(String::from("BETA: TOOLS")),
                Line::Pair(4, Some(7)),
                Line::Pair(8, None),
            ]
        );
        // Every binding appears exactly once.
        let mut seen = Vec::new();
        for l in &lines {
            if let Line::Pair(a, b) = l {
                seen.push(*a);
                seen.extend(*b);
            }
        }
        seen.sort();
        assert_eq!(seen, (0..9).collect::<Vec<_>>());
        // No mods: just the game's own.
        assert_eq!(page_lines(2, &[]), [Line::Pair(0, Some(1))]);
        assert!(page_lines(0, &[]).is_empty());
    }

    #[test]
    fn switch_lines_group_by_mod() {
        let mut d = Decls::default();
        d.add_switch("a", "x", "X", false).unwrap();
        d.add_switch("b", "y", "Y", false).unwrap();
        d.add_switch("a", "z", "Z", false).unwrap();
        assert_eq!(
            switch_lines(&d.switches),
            [Line::Heading(String::from("A")), Line::Pair(0, Some(2)), Line::Heading(String::from("B")), Line::Pair(1, None)]
        );
    }

    #[test]
    fn forgetting_and_absorbing_a_mod() {
        let mut d = Decls::default();
        d.add_bind("a", "x", "", "", 5).unwrap();
        d.add_switch("a", "s", "", false).unwrap();
        d.add_bind("b", "y", "", "", 6).unwrap();
        d.remove_mod("a");
        assert_eq!((d.binds.len(), d.switches.len()), (1, 0));
        let mut fresh = Decls::default();
        fresh.add_bind("b", "z", "", "", 7).unwrap();
        d.absorb(fresh);
        assert_eq!(d.binds.len(), 1);
        assert_eq!(d.binds[0].id, "z");
    }

    #[test]
    fn key_names_make_scancodes() {
        assert_eq!(key_scancode("a"), 4);
        assert_eq!(key_scancode("Z"), 29);
        assert_eq!(key_scancode("1"), 30);
        assert_eq!(key_scancode("0"), 39);
        assert_eq!(key_scancode("F1"), 58);
        assert_eq!(key_scancode("f12"), 69);
        assert_eq!(key_scancode("f13"), 0);
        assert_eq!(key_scancode("space"), 44);
        assert_eq!(key_scancode("up"), 82);
        assert_eq!(key_scancode("nonsense"), 0);
        assert_eq!(key_scancode(""), 0);
    }
}
