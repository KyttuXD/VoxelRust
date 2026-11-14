//! The rows of Settings > MODS: the mods installed in the mods folder with
//! their version and an on/off switch, then the ones the server sent for
//! this session, which cannot be switched.

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub name: String,
    pub version: String,
    /// Switched on (always true for a mod from the server).
    pub on: bool,
    /// Sent by the server for this session: read only.
    pub from_server: bool,
}

impl Row {
    /// What the list shows: `NAME 1.2` (or `NAME 1.2 - SERVER`).
    pub fn label(&self) -> String {
        let mut s = self.name.to_ascii_uppercase().replace('_', " ");
        if !self.version.is_empty() {
            s.push(' ');
            s.push_str(&self.version.to_ascii_uppercase());
        }
        if self.from_server {
            s.push_str("  (FROM SERVER)");
        }
        s
    }
}

/// `installed` and `from_server` are (name, version); `off` is the
/// settings' comma-separated list of switched-off mods.
pub fn rows(installed: &[(String, String)], off: &str, from_server: &[(String, String)]) -> Vec<Row> {
    let mut out: Vec<Row> = installed.iter().map(|(n, v)| Row { name: n.clone(), version: v.clone(), on: !off.split(',').any(|o| o == n), from_server: false }).collect();
    for (n, v) in from_server {
        out.push(Row { name: n.clone(), version: v.clone(), on: true, from_server: true });
    }
    out
}

/// `off` with `name` switched on or off.
pub fn toggled(off: &str, name: &str, on: bool) -> String {
    let mut list: Vec<&str> = off.split(',').filter(|o| !o.is_empty() && *o != name).collect();
    if !on {
        list.push(name);
    }
    list.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(n: &str, v: &str) -> (String, String) {
        (String::from(n), String::from(v))
    }

    #[test]
    fn local_mods_have_switches_and_server_mods_are_read_only() {
        let r = rows(&[p("earth", "2.0"), p("holiday_pack", "")], "holiday_pack", &[p("remote", "1")]);
        assert_eq!(r.len(), 3);
        assert!(r[0].on && !r[0].from_server);
        assert!(!r[1].on);
        assert!(r[2].on && r[2].from_server);
        assert_eq!(r[0].label(), "EARTH 2.0");
        assert_eq!(r[1].label(), "HOLIDAY PACK");
        assert_eq!(r[2].label(), "REMOTE 1  (FROM SERVER)");
    }

    #[test]
    fn switching_edits_the_off_list() {
        assert_eq!(toggled("", "a", false), "a");
        assert_eq!(toggled("a", "b", false), "a,b");
        assert_eq!(toggled("a,b", "a", true), "b");
        assert_eq!(toggled("a", "a", false), "a");
        assert_eq!(toggled("a", "a", true), "");
    }
}
