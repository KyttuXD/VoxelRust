//! The emotes of the content packs: `emote <id> <name> <seconds> [loop]`
//! followed by keyframe lines, kept in the registry as an [`EmoteTable`]
//! (see docs/DATA.md, "Emotes").
//!
//! ```text
//! emote wave Wave 2.4
//!   key 0.0 R-Arm 0 0 0
//!   key 0.4 R-Arm 0 0 150
//!   key 2.0 R-Arm 0 0 150
//! ```
//!
//! Every `key <time> <bone> <x> <y> <z>` gives the rotation, in degrees, a
//! bone of the model has added to its own at that time. Between keys the
//! rotation eases from one to the next; before a bone's first key and after
//! its last the emote eases to and from no rotation at all (or, in a looping
//! emote, from the last key round to the first).

use alloc::string::String;
use alloc::vec::Vec;

/// Most emotes, tracks in one, and keys on one track.
pub const MAX_EMOTES: usize = 64;
pub const MAX_TRACKS: usize = 24;
pub const MAX_KEYS: usize = 64;
/// Longest an emote may run, in seconds.
pub const MAX_SECS: f32 = 60.0;
/// The most any one rotation is allowed to be, degrees.
const MAX_DEGREES: f32 = 720.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub t: f32,
    pub rot: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    /// The bone, in lower case.
    pub bone: String,
    /// In order of time.
    pub keys: Vec<Key>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Emote {
    pub id: String,
    pub name: String,
    pub secs: f32,
    pub looped: bool,
    pub tracks: Vec<Track>,
}

#[derive(Clone, Debug, Default)]
pub struct EmoteTable {
    pub list: Vec<Emote>,
}

impl EmoteTable {
    pub fn find(&self, id: &str) -> Option<usize> {
        self.list.iter().position(|e| e.id == id)
    }

    pub fn get(&self, i: usize) -> Option<&Emote> {
        self.list.get(i)
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

fn good_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 24 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn number(s: &str) -> Option<f32> {
    s.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// Start (or restart, redefining the same id) an emote from the words after
/// `emote`: `<id> <name...> <seconds> [loop]`. Returns its place in the table.
pub fn begin(table: &mut EmoteTable, args: &[&str]) -> Result<usize, String> {
    let mut words: Vec<&str> = args.to_vec();
    let looped = words.last() == Some(&"loop");
    if looped {
        words.pop();
    }
    let secs = words.last().and_then(|s| number(s));
    if words.len() < 3 || secs.is_none() {
        return Err(String::from("emote needs: <id> <name> <seconds> [loop]"));
    }
    let secs = secs.unwrap_or(1.0);
    let id = words[0].to_ascii_lowercase();
    if !good_id(&id) {
        return Err(alloc::format!("emote ids are letters, digits and _ (24 at most), not `{}`", words[0]));
    }
    if !(0.2..=MAX_SECS).contains(&secs) {
        return Err(alloc::format!("an emote lasts 0.2 to {} seconds", MAX_SECS as i32));
    }
    let name: String = words[1..words.len() - 1].join(" ").chars().filter(|c| c.is_ascii_graphic() || *c == ' ').take(16).collect();
    let fresh = Emote { id: id.clone(), name, secs, looped, tracks: Vec::new() };
    match table.find(&id) {
        Some(i) => {
            table.list[i] = fresh;
            Ok(i)
        }
        None if table.list.len() >= MAX_EMOTES => Err(alloc::format!("at most {} emotes", MAX_EMOTES)),
        None => {
            table.list.push(fresh);
            Ok(table.list.len() - 1)
        }
    }
}

/// Read one line of the emote `i`: `key <time> <bone> <x> <y> <z>`.
pub fn line(table: &mut EmoteTable, i: usize, key: &str, args: &[&str]) -> Result<(), String> {
    if key != "key" {
        return Err(alloc::format!("unknown emote line `{}` (there are only `key` lines)", key));
    }
    let e = table.list.get_mut(i).ok_or_else(|| String::from("key outside an emote"))?;
    let nums: Vec<Option<f32>> = args.iter().skip(2).map(|s| number(s)).collect();
    let (t, bone) = match (args.first().and_then(|s| number(s)), args.get(1)) {
        (Some(t), Some(b)) if args.len() == 5 && nums.iter().all(|n| n.is_some()) => (t, b.to_ascii_lowercase()),
        _ => return Err(String::from("key needs: <time> <bone> <x> <y> <z>, in seconds and degrees")),
    };
    if t < 0.0 || t > e.secs {
        return Err(alloc::format!("a key time must be 0..{} (the emote's length)", e.secs));
    }
    let rot = [0, 1, 2].map(|k| nums[k].unwrap_or(0.0).clamp(-MAX_DEGREES, MAX_DEGREES));
    let at = match e.tracks.iter().position(|tr| tr.bone == bone) {
        Some(at) => at,
        None if e.tracks.len() >= MAX_TRACKS => return Err(alloc::format!("an emote may move {} bones at most", MAX_TRACKS)),
        None => {
            e.tracks.push(Track { bone, keys: Vec::new() });
            e.tracks.len() - 1
        }
    };
    let keys = &mut e.tracks[at].keys;
    if keys.len() >= MAX_KEYS {
        return Err(alloc::format!("a bone may have {} keys at most", MAX_KEYS));
    }
    // A key at the same time replaces the earlier one.
    keys.retain(|k| k.t != t);
    let place = keys.iter().position(|k| k.t > t).unwrap_or(keys.len());
    keys.insert(place, Key { t, rot });
    Ok(())
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * k)
}

/// The rotation of a track at time `t` (the keys in order), for an emote
/// `secs` long. Outside the keys it eases to and from nothing, or round
/// the loop.
pub fn sample(keys: &[Key], secs: f32, looped: bool, t: f32) -> [f32; 3] {
    let (first, last) = match (keys.first(), keys.last()) {
        (Some(f), Some(l)) => (f, l),
        _ => return [0.0; 3],
    };
    let t = if looped && secs > 0.0 { t - secs * ((t / secs) as i32 as f32) } else { t.clamp(0.0, secs) };
    if t < first.t {
        // Before the first key: from the end of the loop, or from rest.
        return if looped {
            let from = Key { t: last.t - secs, rot: last.rot };
            between(&from, first, t)
        } else {
            between(&Key { t: 0.0, rot: [0.0; 3] }, first, t)
        };
    }
    if t >= last.t {
        return if looped {
            let to = Key { t: first.t + secs, rot: first.rot };
            between(last, &to, t)
        } else {
            between(last, &Key { t: secs.max(last.t), rot: [0.0; 3] }, t)
        };
    }
    let i = keys.iter().rposition(|k| k.t <= t).unwrap_or(0);
    between(&keys[i], &keys[i + 1], t)
}

fn between(a: &Key, b: &Key, t: f32) -> [f32; 3] {
    let span = b.t - a.t;
    if span <= 1e-6 {
        return b.rot;
    }
    mix(a.rot, b.rot, smooth((t - a.t) / span))
}

impl Emote {
    /// The time into the emote for `t` seconds since it began, or `None`
    /// when it is over (looping ones never are).
    pub fn at(&self, t: f32) -> Option<f32> {
        if self.looped || t < self.secs {
            Some(t)
        } else {
            None
        }
    }

    /// The rotation of the bone called `bone` (lower case) at time `t`.
    pub fn rotation(&self, bone: &str, t: f32) -> Option<[f32; 3]> {
        self.tracks.iter().find(|tr| tr.bone == bone).map(|tr| sample(&tr.keys, self.secs, self.looped, t))
    }

    /// Whether the emote moves an arm or a hand, which the first-person
    /// view then shows (and nothing else of the body).
    pub fn uses_arm(&self) -> bool {
        self.tracks.iter().any(|t| t.bone.contains("arm") || t.bone.contains("hand"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> (EmoteTable, Vec<String>) {
        let mut table = EmoteTable::default();
        let mut warnings = Vec::new();
        let mut cur = None;
        for l in text.lines() {
            let words: Vec<&str> = l.split_whitespace().collect();
            let (key, args) = match words.split_first() {
                Some(x) => x,
                None => continue,
            };
            if *key == "emote" {
                match begin(&mut table, args) {
                    Ok(i) => cur = Some(i),
                    Err(m) => {
                        warnings.push(m);
                        cur = None;
                    }
                }
            } else if let Some(i) = cur {
                if let Err(m) = line(&mut table, i, key, args) {
                    warnings.push(m);
                }
            }
        }
        (table, warnings)
    }

    #[test]
    fn an_emote_is_read_with_its_keys_in_order() {
        let (t, w) = parse("emote wave Wave 2.5\n  key 1.0 R-Arm 0 0 150\n  key 0 R-Arm 0 0 0\n  key 0.5 L-Arm 10 0 0\n");
        assert!(w.is_empty(), "{w:?}");
        let e = &t.list[0];
        assert_eq!((e.id.as_str(), e.name.as_str(), e.secs, e.looped), ("wave", "Wave", 2.5, false));
        assert_eq!(e.tracks.len(), 2);
        assert_eq!(e.tracks[0].bone, "r-arm");
        assert_eq!(e.tracks[0].keys.iter().map(|k| k.t).collect::<Vec<_>>(), [0.0, 1.0]);
        assert!(e.uses_arm());
    }

    #[test]
    fn names_may_have_several_words_and_emotes_may_loop() {
        let (t, w) = parse("emote dance Happy Dance 4 loop\n  key 0 Head 0 0 0\n");
        assert!(w.is_empty(), "{w:?}");
        assert_eq!((t.list[0].name.as_str(), t.list[0].looped, t.list[0].secs), ("Happy Dance", true, 4.0));
        assert!(!t.list[0].uses_arm());
    }

    #[test]
    fn bad_lines_are_reported_and_skipped() {
        let (t, w) = parse(
            "emote x\nemote Bad-Id Name 2\nemote slow Slow 99\nemote ok Ok 2\n  key 3 Head 0 0 0\n  key 1 Head 0 0\n  key one Head 0 0 0\n  key 1 Head 0 0 nan\n  pose 1\n  key 1 Head 5 0 0\n",
        );
        assert_eq!(w.len(), 8, "{w:?}");
        assert_eq!(t.list.len(), 1);
        assert_eq!(t.list[0].tracks[0].keys.len(), 1);
    }

    #[test]
    fn redefining_an_emote_replaces_it() {
        let (t, _) = parse("emote a A 1\n  key 0 Head 1 0 0\nemote a A 2\n");
        assert_eq!(t.list.len(), 1);
        assert!(t.list[0].tracks.is_empty() && t.list[0].secs == 2.0);
    }

    fn keys() -> Vec<Key> {
        alloc::vec![Key { t: 0.5, rot: [0.0, 0.0, 0.0] }, Key { t: 1.5, rot: [100.0, 0.0, 0.0] }]
    }

    #[test]
    fn keys_are_interpolated_smoothly() {
        let k = keys();
        assert_eq!(sample(&k, 3.0, false, 0.5), [0.0; 3]);
        assert_eq!(sample(&k, 3.0, false, 1.5), [100.0, 0.0, 0.0]);
        let mid = sample(&k, 3.0, false, 1.0);
        assert!((mid[0] - 50.0).abs() < 0.01);
        // Eased: slower than linear near the ends.
        assert!(sample(&k, 3.0, false, 0.6)[0] < 10.0);
        // Never outside the two keys it lies between.
        for i in 0..=30 {
            let v = sample(&k, 3.0, false, 0.5 + i as f32 / 30.0)[0];
            assert!((0.0..=100.0).contains(&v));
        }
    }

    #[test]
    fn before_and_after_the_keys_the_emote_returns_to_rest() {
        let k = alloc::vec![Key { t: 1.0, rot: [80.0, 0.0, 0.0] }];
        assert_eq!(sample(&k, 2.0, false, 0.0), [0.0; 3]);
        assert_eq!(sample(&k, 2.0, false, 1.0), [80.0, 0.0, 0.0]);
        assert_eq!(sample(&k, 2.0, false, 2.0), [0.0; 3]);
        assert!(sample(&k, 2.0, false, 0.5)[0] > 10.0 && sample(&k, 2.0, false, 0.5)[0] < 70.0);
        assert_eq!(sample(&[], 2.0, false, 1.0), [0.0; 3]);
    }

    #[test]
    fn a_looping_emote_is_continuous_round_the_loop() {
        let k = alloc::vec![Key { t: 0.0, rot: [0.0; 3] }, Key { t: 1.0, rot: [90.0, 0.0, 0.0] }];
        let (end, start) = (sample(&k, 2.0, true, 1.999), sample(&k, 2.0, true, 2.0));
        assert!((end[0] - start[0]).abs() < 1.0, "{end:?} {start:?}");
        assert!((sample(&k, 2.0, true, 2.5)[0] - sample(&k, 2.0, true, 0.5)[0]).abs() < 1e-3, "time wraps");
    }

    #[test]
    fn an_emote_ends_unless_it_loops() {
        let (t, _) = parse("emote a A 1\nemote b B 1 loop\n");
        assert!(t.list[0].at(0.5).is_some() && t.list[0].at(1.0).is_none());
        assert!(t.list[1].at(100.0).is_some());
        assert_eq!(t.list[0].rotation("head", 0.0), None);
    }
}
