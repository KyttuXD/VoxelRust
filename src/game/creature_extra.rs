//! What a script (or a mod's own code) can add to one creature beyond its
//! kind: its own size, its own health limit, per-bone model overrides, a tilt
//! of the whole model, and a short state text. A creature that has none of
//! these carries nothing: `Mob::extra` is `None`, so ordinary creatures cost
//! no memory and no work per frame.
//!
//! In a shared game the host's snapshot carries all of it to the other
//! players (see `creature_wire.rs`), so big creatures are seen at their size
//! and a mod's state shows on every machine.

use crate::bone_pose::BoneTable;
use alloc::string::String;
use alloc::vec::Vec;

/// Longest state text, in bytes.
pub const MAX_STATE: usize = 64;
/// Most creatures that may have bone overrides at once.
pub const MAX_WITH_BONES: usize = 256;
/// Most models whose bone names are remembered, and most unknown bone names
/// logged (so a script that asks every frame cannot fill the log).
pub const MAX_NAME_CACHE: usize = 32;

/// The fields a snapshot from another player has set (`Extra::remote`): when
/// a later snapshot no longer carries one, it is cleared again.
pub mod field {
    pub const SIZE: u8 = 1;
    pub const MAX_HP: u8 = 2;
    pub const ORIENT: u8 = 4;
    pub const BONES: u8 = 8;
    pub const STATE: u8 = 16;
    pub const LOOK: u8 = 32;
}

#[derive(Clone, Debug)]
pub struct Extra {
    /// Width and height in blocks, when a script set them.
    pub size: Option<(f32, f32)>,
    /// The most health it can have, when a script set it.
    pub max_hp: Option<i32>,
    /// Per-bone overrides.
    pub bones: BoneTable,
    /// Tilt of the whole model, tenths of a degree.
    pub pitch: i16,
    pub roll: i16,
    /// Where a script forces the head to look (yaw, pitch; tenths of a degree, relative to the body).
    pub look: Option<(i16, i16)>,
    /// What the mod wants shown or remembered about this creature.
    pub state: String,
    /// Which fields came from another player's snapshot (`field::*`).
    pub remote: u8,
    /// Goes up whenever something a snapshot carries changes.
    pub rev: u32,
    /// The `rev` the host last sent in full.
    pub sent_rev: u32,
    wire: String,
    wire_rev: u32,
}

impl Extra {
    pub fn new() -> Extra {
        Extra { size: None, max_hp: None, bones: BoneTable::default(), pitch: 0, roll: 0, look: None, state: String::new(), remote: 0, rev: 1, sent_rev: 0, wire: String::new(), wire_rev: 0 }
    }

    /// Something changed: the next snapshot sends it.
    pub fn touch(&mut self) {
        self.rev = self.rev.wrapping_add(1).max(1);
    }

    /// Nothing is set: the creature does not need an `Extra` at all.
    pub fn is_plain(&self) -> bool {
        self.size.is_none() && self.max_hp.is_none() && self.bones.is_empty() && self.pitch == 0 && self.roll == 0 && self.look.is_none() && self.state.is_empty()
    }

    /// (pitch, roll) in degrees, when the model is tilted.
    pub fn orient_deg(&self) -> Option<(f32, f32)> {
        if self.pitch == 0 && self.roll == 0 {
            None
        } else {
            Some((self.pitch as f32 / 10.0, self.roll as f32 / 10.0))
        }
    }

    /// The forced head look in degrees (yaw, pitch), if a script set one.
    pub fn look_deg(&self) -> Option<(f32, f32)> {
        self.look.map(|(y, p)| (y as f32 / 10.0, p as f32 / 10.0))
    }

    /// The snapshot text of this extra (see `creature_wire::encode`), made
    /// again only when something changed since it was last made.
    pub fn wire(&mut self) -> &str {
        if self.wire_rev != self.rev || self.wire_rev == 0 {
            self.wire = crate::creature_wire::encode(self);
            self.wire_rev = self.rev;
        }
        &self.wire
    }
}

impl Default for Extra {
    fn default() -> Extra {
        Extra::new()
    }
}

/// A state text as the game keeps it: no control characters, at most
/// [`MAX_STATE`] bytes (cut at a character).
pub fn clean_state(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_control() {
            continue;
        }
        if out.len() + c.len_utf8() > MAX_STATE {
            break;
        }
        out.push(c);
    }
    out
}

/// The bone names of models, read from their files the first time they are
/// asked for, and the unknown names already complained about.
#[derive(Default)]
pub struct BoneNames {
    cache: Vec<(usize, Vec<String>)>,
    warned: Vec<String>,
}

impl BoneNames {
    /// The bone names of registry model `model`, as the file gives them
    /// (empty if it cannot be read).
    pub fn names(&mut self, model: usize) -> &[String] {
        let at = match self.cache.iter().position(|c| c.0 == model) {
            Some(i) => i,
            None => {
                let names = crate::block::reg()
                    .models
                    .get(model)
                    .and_then(|spec| spec.source())
                    .and_then(|text| crate::bbmodel::load(&text).ok())
                    .map(|m| m.bones.iter().map(|b| b.name.clone()).collect())
                    .unwrap_or_default();
                if self.cache.len() >= MAX_NAME_CACHE {
                    self.cache.remove(0);
                }
                self.cache.push((model, names));
                self.cache.len() - 1
            }
        };
        &self.cache[at].1
    }

    /// The place of the bone called `name` (any case) in model `model`.
    pub fn index(&mut self, model: usize, name: &str) -> Option<usize> {
        self.names(model).iter().position(|n| n.eq_ignore_ascii_case(name))
    }

    /// Say once, in the log, that a model has no bone of this name.
    pub fn complain(&mut self, model: usize, name: &str) {
        let key = alloc::format!("{}/{}", model, name);
        if self.warned.contains(&key) || self.warned.len() >= MAX_NAME_CACHE {
            return;
        }
        let model_id = crate::block::reg().models.get(model).map(|m| m.id.clone()).unwrap_or_default();
        logf!("voxelrust: set_bone: the model {} has no bone called {} (ignored)", model_id, name);
        self.warned.push(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_extra_is_plain_until_something_is_set() {
        let mut e = Extra::new();
        assert!(e.is_plain());
        assert_eq!(e.orient_deg(), None);
        e.pitch = 150;
        assert!(!e.is_plain());
        assert_eq!(e.orient_deg(), Some((15.0, 0.0)));
        e.pitch = 0;
        e.state.push_str("x");
        assert!(!e.is_plain());
    }

    #[test]
    fn the_wire_text_is_made_only_when_something_changed() {
        let mut e = Extra::new();
        e.state.push_str("angry");
        let first = e.wire().to_string();
        assert!(first.contains("angry"));
        // Changed without a touch: the old text is still what is held.
        e.state.push_str("er");
        assert_eq!(e.wire(), first);
        e.touch();
        assert!(e.wire().contains("angryer"));
    }

    #[test]
    fn state_texts_are_cleaned_and_cut() {
        assert_eq!(clean_state("a\nb\tc\u{0}d"), "abcd");
        let long = "x".repeat(200);
        assert_eq!(clean_state(&long).len(), MAX_STATE);
        // Cut at a character, never in the middle of one.
        let wide = "\u{e9}".repeat(100);
        let cut = clean_state(&wide);
        assert!(cut.len() <= MAX_STATE && cut.chars().all(|c| c == '\u{e9}') && cut.len() % 2 == 0);
    }
}
