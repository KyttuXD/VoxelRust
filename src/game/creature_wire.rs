//! How a creature's [`Extra`] travels in the creature snapshot of a shared
//! game (`Mobs::snapshot`, protocol version 13).
//!
//! A snapshot line is the old ten fields, then, only if the creature has
//! something extra, `|`-separated segments straight after the last field:
//!
//! | segment | meaning |
//! | --- | --- |
//! | `S<w>,<h>` | size in blocks (two decimals) |
//! | `M<n>` | most health |
//! | `O<pitch>,<roll>` | tilt in tenths of a degree |
//! | `B<table>` | bone overrides (see `BoneTable::encode`) |
//! | `L<yaw>,<pitch>` | the head forced to look this way, tenths of a degree relative to the body |
//! | `T<text>` | state text, `%XX` for `%`, `|` and bytes outside printable ASCII |
//! | `=` | (alone) nothing changed since the last full line: keep what you have |
//!
//! Everything is bounded when it is read: a hostile host cannot make a client
//! hold a bigger creature than the limits allow, more than 64 bones, or a
//! state over 64 bytes.

use crate::bone_pose::BoneTable;
use crate::creature_extra::{clean_state, Extra, MAX_STATE};
use crate::creature_size::{clamp_health, clamp_size};
use alloc::string::String;
use alloc::vec::Vec;

/// A full snapshot of every creature's extras is sent at least this often
/// (counted in snapshots, which go out five times a second), so a player who
/// joined late catches up; in between only a creature whose extras changed
/// repeats them, and the others say `=`.
pub const FULL_EVERY: u32 = 5;

/// What one snapshot line said about the extras.
#[derive(Debug, PartialEq)]
pub enum Wire {
    /// `=`: keep what you have.
    Same,
    /// The segments that were there (and sound).
    Fields(Decoded),
}

#[derive(Debug, Default, PartialEq)]
pub struct Decoded {
    pub size: Option<(f32, f32)>,
    pub max_hp: Option<i32>,
    pub orient: Option<(i16, i16)>,
    pub bones: Option<BoneTable>,
    pub state: Option<String>,
    pub look: Option<(i16, i16)>,
}

fn escape(s: &str, out: &mut String) {
    for b in s.bytes() {
        if b == b'%' || b == b'|' || !(32..127).contains(&b) {
            out.push_str(&alloc::format!("%{:02X}", b));
        } else {
            out.push(b as char);
        }
    }
}

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut raw: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < b.len() && raw.len() < MAX_STATE * 4 {
        if b[i] == b'%' {
            let hex = |k: usize| b.get(k).and_then(|c| (*c as char).to_digit(16));
            if let (Some(h), Some(l)) = (hex(i + 1), hex(i + 2)) {
                raw.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        raw.push(b[i]);
        i += 1;
    }
    clean_state(&String::from_utf8_lossy(&raw))
}

/// The segments of `e` (each starting with `|`), or an empty text when
/// nothing is set.
pub fn encode(e: &Extra) -> String {
    let mut out = String::new();
    if let Some((w, h)) = e.size {
        out.push_str(&alloc::format!("|S{:.2},{:.2}", w, h));
    }
    if let Some(n) = e.max_hp {
        out.push_str(&alloc::format!("|M{}", n));
    }
    if e.pitch != 0 || e.roll != 0 {
        out.push_str(&alloc::format!("|O{},{}", e.pitch, e.roll));
    }
    if !e.bones.is_empty() {
        out.push_str("|B");
        out.push_str(&e.bones.encode());
    }
    if let Some((y, p)) = e.look {
        out.push_str(&alloc::format!("|L{},{}", y, p));
    }
    if !e.state.is_empty() {
        out.push_str("|T");
        escape(&e.state, &mut out);
    }
    out
}

/// Read what follows the tenth field of a snapshot line (starting with `|`).
pub fn decode(tail: &str) -> Wire {
    let mut d = Decoded::default();
    for seg in tail.split('|').take(8) {
        let Some(tag) = seg.chars().next() else { continue };
        let body = &seg[tag.len_utf8()..];
        match tag {
            '=' => return Wire::Same,
            'S' => {
                let mut v = body.split(',').map(|x| x.parse::<f64>().ok().filter(|f| f.is_finite()));
                if let (Some(Some(w)), Some(Some(h))) = (v.next(), v.next()) {
                    d.size = Some(clamp_size(w, h));
                }
            }
            'M' => {
                if let Ok(n) = body.parse::<i64>() {
                    d.max_hp = Some(clamp_health(n));
                }
            }
            'O' => {
                let mut v = body.split(',').map(|x| x.parse::<i64>().ok());
                if let (Some(Some(p)), Some(Some(r))) = (v.next(), v.next()) {
                    let lim = (crate::bone_pose::MAX_ROT * 10.0) as i64;
                    d.orient = Some((p.clamp(-lim, lim) as i16, r.clamp(-lim, lim) as i16));
                }
            }
            'L' => {
                let mut v = body.split(',').map(|x| x.parse::<i64>().ok());
                if let (Some(Some(y)), Some(Some(p))) = (v.next(), v.next()) {
                    d.look = Some((y.clamp(-900, 900) as i16, p.clamp(-600, 600) as i16));
                }
            }
            'B' => d.bones = Some(BoneTable::decode(body)),
            'T' => d.state = Some(unescape(body)),
            _ => {}
        }
    }
    Wire::Fields(d)
}

impl Extra {
    /// Take what a snapshot said. Fields it carries are set (and marked as
    /// from the host); a field it does not carry but an earlier one did is
    /// cleared; what a local script set itself stays. Returns whether the
    /// size changed (the caller resizes the body).
    pub fn apply_wire(&mut self, w: Wire) -> bool {
        use crate::creature_extra::field::*;
        let d = match w {
            Wire::Same => return false,
            Wire::Fields(d) => d,
        };
        let before = self.size;
        let take = |has: bool, bit: u8, remote: &mut u8| {
            if has {
                *remote |= bit;
                true
            } else if *remote & bit != 0 {
                *remote &= !bit;
                true // cleared
            } else {
                false
            }
        };
        let mut remote = self.remote;
        if take(d.size.is_some(), SIZE, &mut remote) {
            self.size = d.size;
        }
        if take(d.max_hp.is_some(), MAX_HP, &mut remote) {
            self.max_hp = d.max_hp;
        }
        if take(d.orient.is_some(), ORIENT, &mut remote) {
            let (p, r) = d.orient.unwrap_or((0, 0));
            self.pitch = p;
            self.roll = r;
        }
        if take(d.bones.is_some(), BONES, &mut remote) {
            self.bones = d.bones.unwrap_or_default();
        }
        if take(d.look.is_some(), LOOK, &mut remote) {
            self.look = d.look;
        }
        if take(d.state.is_some(), STATE, &mut remote) {
            self.state = d.state.unwrap_or_default();
        }
        self.remote = remote;
        before != self.size
    }

    /// A snapshot line carried no extras at all: clear what an earlier one set.
    pub fn apply_none(&mut self) -> bool {
        self.apply_wire(Wire::Fields(Decoded::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bone_pose::BoneEntry;

    fn sample() -> Extra {
        let mut e = Extra::new();
        e.size = Some((12.5, 40.0));
        e.max_hp = Some(250_000);
        e.pitch = -150;
        e.roll = 300;
        e.bones.set(BoneEntry::new(2, [10.0, 0.0, -5.5], [0.0, 1.25, 0.0], 2.0).unwrap());
        e.state = String::from("phase=2|rage 50%");
        e
    }

    #[test]
    fn extras_survive_the_wire() {
        let e = sample();
        let text = encode(&e);
        assert!(!text.contains('\n') && text.starts_with('|'));
        // The state's own `|` is escaped, so the segments still split cleanly.
        assert_eq!(text.matches('|').count(), 5, "{text}");
        let Wire::Fields(d) = decode(&text) else { panic!("fields") };
        assert_eq!(d.size, Some((12.5, 40.0)));
        assert_eq!(d.max_hp, Some(250_000));
        assert_eq!(d.orient, Some((-150, 300)));
        assert_eq!(d.bones, Some(e.bones.clone()));
        assert_eq!(d.state.as_deref(), Some("phase=2|rage 50%"));
    }

    #[test]
    fn a_plain_creature_sends_nothing() {
        assert_eq!(encode(&Extra::new()), "");
    }

    #[test]
    fn same_means_keep_and_fields_replace() {
        assert_eq!(decode("|="), Wire::Same);
        let mut e = Extra::new();
        assert!(e.apply_wire(decode(&encode(&sample()))));
        assert_eq!(e.size, Some((12.5, 40.0)));
        assert_eq!(e.state, "phase=2|rage 50%");
        assert_eq!(e.bones.len(), 1);
        // `=` changes nothing.
        e.apply_wire(Wire::Same);
        assert_eq!(e.bones.len(), 1);
        // A line with only a state: the host cleared everything else, so it goes here too.
        let changed = e.apply_wire(decode("|Tcalm"));
        assert!(changed, "the size went");
        assert_eq!((e.size, e.max_hp, e.pitch, e.state.as_str()), (None, None, 0, "calm"));
        assert!(e.bones.is_empty());
        // A line without extras clears the rest.
        e.apply_none();
        assert!(e.is_plain());
    }

    #[test]
    fn what_a_local_script_set_survives_a_snapshot_without_extras() {
        let mut e = Extra::new();
        e.pitch = 100;
        e.apply_none();
        assert_eq!(e.pitch, 100);
    }

    #[test]
    fn hostile_text_is_kept_inside_the_limits() {
        let Wire::Fields(d) = decode("|S1e9,1e9|M99999999999|O999999,-999999|T%00%FF%zz") else { panic!("fields") };
        assert_eq!(d.size, Some((crate::creature_size::MAX_WIDTH, crate::creature_size::MAX_HEIGHT)));
        assert_eq!(d.max_hp, Some(crate::creature_size::MAX_HEALTH));
        assert_eq!(d.orient, Some((30000, -30000)));
        assert!(d.state.unwrap().len() <= MAX_STATE);
        let Wire::Fields(d) = decode("|Snan,3|Sx|Mx|O1|B|Zjunk|S") else { panic!("fields") };
        assert_eq!((d.size, d.max_hp, d.orient), (None, None, None));
        assert_eq!(d.bones, Some(BoneTable::default()));
        let long = alloc::format!("|T{}", "a".repeat(5000));
        let Wire::Fields(d) = decode(&long) else { panic!("fields") };
        assert_eq!(d.state.unwrap().len(), MAX_STATE);
        let many = "|S1,1".repeat(100);
        assert!(matches!(decode(&many), Wire::Fields(_)));
    }

    #[test]
    fn a_forced_look_travels_and_is_bounded() {
        let mut e = Extra::new();
        e.look = Some((-450, 120));
        let text = encode(&e);
        assert_eq!(text, "|L-450,120");
        let mut other = Extra::new();
        other.apply_wire(decode(&text));
        assert_eq!(other.look, Some((-450, 120)));
        assert_eq!(other.look_deg(), Some((-45.0, 12.0)));
        // The host cleared it: so it goes here too.
        other.apply_none();
        assert!(other.look.is_none() && other.is_plain());
        let Wire::Fields(d) = decode("|L99999,-99999|Lx,1") else { panic!("fields") };
        assert_eq!(d.look, Some((900, -600)));
        let mut e = Extra::new();
        e.look = Some((1, 1));
        assert!(!e.is_plain());
    }
}
