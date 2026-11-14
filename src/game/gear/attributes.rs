//! The player's attributes: numbers a mod can change that the game reads
//! (the most health the player has, walking speed, jump height, armour
//! points, damage dealt). Each has a base value (`set_attribute`) and
//! modifiers a mod adds under an id of its own (`add_attribute_modifier`):
//!
//! value = (base + sum of the `add` amounts) x (1 + sum of the `multiply` amounts)
//!
//! so two modifiers of +4 and +6 make +10 and two of x0.5 and x0.25 make x1.75,
//! whatever order they came in. A modifier added again under the same id
//! replaces the old one. The values are held to a range per attribute.
//! They are saved with the player (`attrs=` in the world's meta file).

use alloc::string::String;
use alloc::vec::Vec;

pub const MAX_HEALTH: usize = 0;
pub const WALK_SPEED: usize = 1;
pub const JUMP_HEIGHT: usize = 2;
pub const ARMOR: usize = 3;
pub const DAMAGE: usize = 4;
pub const NAMES: [&str; 5] = ["max_health", "walk_speed", "jump_height", "armor", "damage"];
/// What each is without any change.
pub const DEFAULTS: [f32; 5] = [20.0, 1.0, 1.0, 0.0, 1.0];
/// What each is held to (lowest, highest).
pub const RANGES: [(f32, f32); 5] = [(2.0, 100.0), (0.1, 4.0), (0.0, 4.0), (0.0, 40.0), (0.0, 10.0)];
pub const MAX_MODIFIERS: usize = 32;
pub const MAX_ID: usize = 32;

#[derive(Clone, PartialEq, Debug)]
pub struct Modifier {
    pub id: String,
    pub attr: u8,
    pub amount: f32,
    /// `multiply` rather than `add`.
    pub multiply: bool,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub struct Attributes {
    base: [Option<f32>; 5],
    pub mods: Vec<Modifier>,
}

pub fn index(name: &str) -> Option<usize> {
    NAMES.iter().position(|n| *n == name)
}

fn good_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

impl Attributes {
    pub fn is_default(&self) -> bool {
        self.mods.is_empty() && self.base.iter().all(|b| b.is_none())
    }

    /// Set what the attribute is before modifiers (a number that is not finite is refused).
    pub fn set_base(&mut self, attr: usize, value: f64) -> bool {
        if attr >= NAMES.len() || !value.is_finite() {
            return false;
        }
        self.base[attr] = Some(value.clamp(-1.0e6, 1.0e6) as f32);
        true
    }

    /// Back to the default base.
    pub fn reset_base(&mut self, attr: usize) {
        if let Some(b) = self.base.get_mut(attr) {
            *b = None;
        }
    }

    /// Add a modifier (or replace the one with this id). An error says what is wrong.
    pub fn add_modifier(&mut self, id: &str, attr: usize, amount: f64, multiply: bool) -> Result<(), String> {
        if !good_id(id) {
            return Err(String::from("a modifier id is 1 to 32 letters, digits, `_`, `-` or `.`"));
        }
        if attr >= NAMES.len() {
            return Err(String::from("unknown attribute (max_health, walk_speed, jump_height, armor, damage)"));
        }
        if !amount.is_finite() {
            return Err(String::from("a modifier amount must be a finite number"));
        }
        let m = Modifier { id: String::from(id), attr: attr as u8, amount: amount.clamp(-1.0e6, 1.0e6) as f32, multiply };
        match self.mods.iter_mut().find(|x| x.id == id) {
            Some(slot) => *slot = m,
            None => {
                if self.mods.len() >= MAX_MODIFIERS {
                    return Err(String::from("at most 32 attribute modifiers"));
                }
                self.mods.push(m);
            }
        }
        Ok(())
    }

    /// Take a modifier away; whether it was there.
    pub fn remove_modifier(&mut self, id: &str) -> bool {
        let before = self.mods.len();
        self.mods.retain(|m| m.id != id);
        self.mods.len() != before
    }

    /// The attribute's value now.
    pub fn value(&self, attr: usize) -> f32 {
        let Some(&(lo, hi)) = RANGES.get(attr) else { return 0.0 };
        let base = self.base[attr].unwrap_or(DEFAULTS[attr]);
        let (mut add, mut mul) = (0.0f32, 0.0f32);
        for m in self.mods.iter().filter(|m| m.attr as usize == attr) {
            if m.multiply {
                mul += m.amount;
            } else {
                add += m.amount;
            }
        }
        let v = (base + add) * (1.0 + mul);
        if v.is_finite() {
            v.clamp(lo, hi)
        } else {
            DEFAULTS[attr]
        }
    }

    /// The most health the player may have, whole points.
    pub fn max_health(&self) -> i32 {
        (self.value(MAX_HEALTH) + 0.5) as i32
    }

    /// As text for the save file: `B:name:value` and `M:id:name:amount:add|multiply`, comma-separated.
    pub fn save(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (i, b) in self.base.iter().enumerate() {
            if let Some(v) = b {
                parts.push(alloc::format!("B:{}:{}", NAMES[i], v));
            }
        }
        for m in &self.mods {
            parts.push(alloc::format!("M:{}:{}:{}:{}", m.id, NAMES[m.attr as usize], m.amount, if m.multiply { "multiply" } else { "add" }));
        }
        parts.join(",")
    }

    /// Inverse of [`Attributes::save`]; what is not understood is skipped.
    pub fn load(text: &str) -> Attributes {
        let mut a = Attributes::default();
        for part in text.split(',').take(5 + MAX_MODIFIERS) {
            let w: Vec<&str> = part.split(':').collect();
            match (w.first().copied(), w.len()) {
                (Some("B"), 3) => {
                    if let (Some(i), Ok(v)) = (index(w[1]), w[2].parse::<f64>()) {
                        a.set_base(i, v);
                    }
                }
                (Some("M"), 5) => {
                    if let (Some(i), Ok(v)) = (index(w[2]), w[3].parse::<f64>()) {
                        let _ = a.add_modifier(w[1], i, v, w[4] == "multiply");
                    }
                }
                _ => {}
            }
        }
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_changed_gives_the_defaults() {
        let a = Attributes::default();
        assert_eq!(a.max_health(), 20);
        assert_eq!(a.value(WALK_SPEED), 1.0);
        assert_eq!(a.value(ARMOR), 0.0);
        assert!(a.is_default());
        assert_eq!(a.value(99), 0.0);
    }

    #[test]
    fn modifiers_stack_in_any_order() {
        let mut a = Attributes::default();
        a.add_modifier("a", MAX_HEALTH, 4.0, false).unwrap();
        a.add_modifier("b", MAX_HEALTH, 6.0, false).unwrap();
        assert_eq!(a.max_health(), 30);
        a.add_modifier("c", MAX_HEALTH, 0.5, true).unwrap();
        a.add_modifier("d", MAX_HEALTH, 0.25, true).unwrap();
        // (20 + 10) x 1.75 = 52.5
        assert_eq!(a.max_health(), 53);
        let mut b = Attributes::default();
        for (id, amount, mul) in [("d", 0.25, true), ("b", 6.0, false), ("c", 0.5, true), ("a", 4.0, false)] {
            b.add_modifier(id, MAX_HEALTH, amount, mul).unwrap();
        }
        assert_eq!(a.value(MAX_HEALTH), b.value(MAX_HEALTH));
    }

    #[test]
    fn the_same_id_replaces_and_remove_takes_away() {
        let mut a = Attributes::default();
        a.add_modifier("boost", WALK_SPEED, 0.5, false).unwrap();
        a.add_modifier("boost", WALK_SPEED, 0.2, false).unwrap();
        assert_eq!(a.mods.len(), 1);
        assert!((a.value(WALK_SPEED) - 1.2).abs() < 1e-6);
        assert!(a.remove_modifier("boost"));
        assert!(!a.remove_modifier("boost"));
        assert_eq!(a.value(WALK_SPEED), 1.0);
    }

    #[test]
    fn values_are_held_to_their_range() {
        let mut a = Attributes::default();
        a.set_base(MAX_HEALTH, 5000.0);
        assert_eq!(a.max_health(), 100);
        a.set_base(MAX_HEALTH, -3.0);
        assert_eq!(a.max_health(), 2);
        a.set_base(WALK_SPEED, 50.0);
        assert_eq!(a.value(WALK_SPEED), 4.0);
        assert!(!a.set_base(WALK_SPEED, f64::NAN));
        assert!(!a.set_base(17, 1.0));
    }

    #[test]
    fn bad_modifiers_are_refused() {
        let mut a = Attributes::default();
        assert!(a.add_modifier("", MAX_HEALTH, 1.0, false).is_err());
        assert!(a.add_modifier("has space", MAX_HEALTH, 1.0, false).is_err());
        assert!(a.add_modifier("x", 9, 1.0, false).is_err());
        assert!(a.add_modifier("x", MAX_HEALTH, f64::INFINITY, false).is_err());
        for i in 0..MAX_MODIFIERS {
            a.add_modifier(&alloc::format!("m{}", i), ARMOR, 0.1, false).unwrap();
        }
        assert!(a.add_modifier("one_more", ARMOR, 1.0, false).is_err());
        assert!(a.add_modifier("m3", ARMOR, 1.0, false).is_ok());
    }

    #[test]
    fn they_save_and_load() {
        let mut a = Attributes::default();
        a.set_base(MAX_HEALTH, 30.0);
        a.add_modifier("ring.1", DAMAGE, 0.5, true).unwrap();
        a.add_modifier("belt", ARMOR, 3.0, false).unwrap();
        let b = Attributes::load(&a.save());
        assert_eq!(a, b);
        assert_eq!(Attributes::load(""), Attributes::default());
        assert_eq!(Attributes::load("junk,B:max_health:x,M:a:b"), Attributes::default());
    }
}
