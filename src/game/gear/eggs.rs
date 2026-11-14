//! Spawn eggs, `<creature id>_egg`: one item for every creature. They are
//! made after every pack has been read and before anything that names items
//! is resolved (block drops, groups, recipes, creatures' loot and trades), so
//! a recipe, a trade or a loot line of any pack can name the egg of a
//! creature of any other pack, whichever was read first. This file holds the
//! planning (which eggs, which names) so it can be tested without a registry.

use alloc::string::String;
use alloc::vec::Vec;

/// The end of a spawn egg's id.
pub const SUFFIX: &str = "_egg";

/// One egg to make.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Egg {
    pub id: String,
    pub name: String,
    /// Index of the creature among the creatures that were made.
    pub creature: usize,
}

/// The eggs for creatures given as `(id, name)` in the order they are made
/// (a creature whose model is missing is left out of the list before this).
/// `taken` says whether an item of that id exists (an egg a pack made by
/// hand wins); `items_now` is how many items there are and `limit` how many
/// there may be.
pub fn plan(creatures: &[(&str, &str)], taken: &dyn Fn(&str) -> bool, items_now: usize, limit: usize) -> Vec<Egg> {
    let mut out: Vec<Egg> = Vec::new();
    for (k, (id, name)) in creatures.iter().enumerate() {
        let egg = alloc::format!("{}{}", id, SUFFIX);
        if taken(&egg) || out.iter().any(|e| e.id == egg) || items_now + out.len() >= limit {
            continue;
        }
        out.push(Egg { id: egg, name: alloc::format!("{} Egg", name), creature: k });
    }
    out
}

/// The creature a spawn egg id names (`pig_egg` gives `pig`).
pub fn creature_of(item_id: &str) -> Option<&str> {
    item_id.strip_suffix(SUFFIX).filter(|c| !c.is_empty())
}

/// Make a recipe's or a trade's "unknown item" message say what is wrong
/// when the item is a spawn egg: that there is no such creature.
pub fn explain(why: &str, creatures: &dyn Fn(&str) -> bool) -> String {
    // The message names the item in backticks: unknown item `thing_egg`.
    let named = why.split('`').nth(1).and_then(creature_of);
    match named {
        Some(c) if !creatures(c) && why.starts_with("unknown item") => alloc::format!("{} (there is no creature `{}`, so no spawn egg of that name)", why, c),
        _ => String::from(why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_creature_gets_an_egg_in_order() {
        let eggs = plan(&[("pig", "Pig"), ("cow", "Cow")], &|_| false, 10, 100);
        assert_eq!(eggs.len(), 2);
        assert_eq!((eggs[0].id.as_str(), eggs[0].name.as_str(), eggs[0].creature), ("pig_egg", "Pig Egg", 0));
        assert_eq!(eggs[1].creature, 1);
    }

    #[test]
    fn an_egg_made_by_hand_or_twice_is_not_made_again() {
        let eggs = plan(&[("pig", "Pig"), ("cow", "Cow"), ("pig", "Pig")], &|id| id == "cow_egg", 0, 100);
        assert_eq!(eggs.len(), 1);
        assert_eq!(eggs[0].id, "pig_egg");
    }

    #[test]
    fn the_item_limit_is_kept() {
        let eggs = plan(&[("a", "A"), ("b", "B"), ("c", "C")], &|_| false, 59_999, 60_000);
        assert_eq!(eggs.len(), 1);
    }

    #[test]
    fn a_missing_creature_is_explained() {
        let msg = explain("unknown item `dragon_egg`", &|c| c == "pig");
        assert!(msg.contains("no creature `dragon`"));
        assert_eq!(explain("unknown item `pig_egg`", &|c| c == "pig"), "unknown item `pig_egg`");
        assert_eq!(explain("unknown item `stone`", &|_| false), "unknown item `stone`");
        assert_eq!(explain("needs pairs", &|_| false), "needs pairs");
        assert_eq!(creature_of("_egg"), None);
        assert_eq!(creature_of("pig_egg"), Some("pig"));
    }
}
