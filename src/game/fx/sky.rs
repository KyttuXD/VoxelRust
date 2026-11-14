//! The sky of the dimension the player is in: what the pack's `dimension`
//! line says (`content/dim_sky.rs`) with whatever a script changed on top
//! (`set_dimension_sky`). The renderer asks [`current`] once a frame.

use crate::dim_sky::DimSky;
use crate::session::Session;
use alloc::string::String;
use alloc::vec::Vec;

/// Most dimensions whose skies scripts may change (a few more than a pack can declare).
pub const MAX_CHANGED: usize = 32;

/// The name a dimension goes by here: the home world is the empty text.
pub fn clean_name(dim: &str) -> String {
    let d = dim.trim().to_ascii_lowercase();
    if d == "home" {
        String::new()
    } else {
        d
    }
}

/// Whether the home world or a dimension of that name exists.
pub fn dimension_exists(dim: &str) -> bool {
    dim.is_empty() || crate::block::reg().dimensions.iter().any(|d| d.0 == dim)
}

/// The sky of `dim` as the pack declared it.
pub fn declared(dim: &str) -> DimSky {
    crate::block::reg().dim_skies.iter().find(|d| d.0 == dim).map_or(DimSky::DEFAULT, |d| d.1)
}

/// Pick a dimension's sky from a list of changes, else the declared one.
pub fn pick(changed: &[(String, DimSky)], dim: &str) -> DimSky {
    changed.iter().find(|c| c.0 == dim).map_or_else(|| declared(dim), |c| c.1)
}

/// The sky of the dimension the player is in.
pub fn current(g: &Session) -> DimSky {
    match super::state_ref(g) {
        Some(s) if !s.skies.is_empty() => pick(&s.skies, &g.dim),
        _ => declared(&g.dim),
    }
}

/// Change one property of a dimension's sky. `Ok(false)`: no such dimension.
/// An error: no such property, or a value that makes no sense (nothing changes).
pub fn set(g: &mut Session, dim: &str, key: &str, value: &str) -> Result<bool, String> {
    let dim = clean_name(dim);
    if !dimension_exists(&dim) {
        return Ok(false);
    }
    let s = super::state(g);
    set_in(&mut s.skies, &dim, key, value, declared(&dim))?;
    Ok(true)
}

/// The list side of [`set`]: change `key` of the sky kept for `dim` (starting
/// from `base` when there is none yet).
pub fn set_in(changed: &mut Vec<(String, DimSky)>, dim: &str, key: &str, value: &str, base: DimSky) -> Result<(), String> {
    let at = changed.iter().position(|c| c.0 == dim);
    let mut sky = at.map_or(base, |i| changed[i].1);
    sky.set(key, value)?;
    match at {
        Some(i) => changed[i].1 = sky,
        None => {
            if changed.len() >= MAX_CHANGED {
                return Err(String::from("too many dimensions have their sky changed"));
            }
            changed.push((String::from(dim), sky));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_tidied() {
        assert_eq!(clean_name("home"), "");
        assert_eq!(clean_name(" Home "), "");
        assert_eq!(clean_name(""), "");
        assert_eq!(clean_name("Sky"), "sky");
    }

    #[test]
    fn a_change_starts_from_the_base_and_is_kept() {
        let mut list = Vec::new();
        set_in(&mut list, "sky", "stars", "no", DimSky::DEFAULT).unwrap();
        assert_eq!(list.len(), 1);
        assert!(!list[0].1.stars && list[0].1.clouds);
        // A second change adds to the first.
        set_in(&mut list, "sky", "clouds", "no", DimSky::DEFAULT).unwrap();
        assert_eq!(list.len(), 1);
        assert!(!list[0].1.stars && !list[0].1.clouds);
        assert!(!pick(&list, "sky").stars);
        // Another dimension is untouched.
        assert_eq!(pick(&list, "void_that_does_not_exist"), DimSky::DEFAULT);
    }

    #[test]
    fn a_bad_value_changes_nothing() {
        let mut list = Vec::new();
        assert!(set_in(&mut list, "sky", "stars", "perhaps", DimSky::DEFAULT).is_err());
        assert!(set_in(&mut list, "sky", "nonsense", "1", DimSky::DEFAULT).is_err());
        assert!(list.is_empty());
        set_in(&mut list, "sky", "ambient_light", "5", DimSky::DEFAULT).unwrap();
        assert!(set_in(&mut list, "sky", "ambient_light", "lots", DimSky::DEFAULT).is_err());
        assert_eq!(list[0].1.ambient, 5);
    }

    #[test]
    fn only_so_many_skies_may_be_changed() {
        let mut list = Vec::new();
        for i in 0..MAX_CHANGED {
            set_in(&mut list, &alloc::format!("d{}", i), "sun", "no", DimSky::DEFAULT).unwrap();
        }
        assert!(set_in(&mut list, "one_more", "sun", "no", DimSky::DEFAULT).is_err());
        // One already there can still change.
        set_in(&mut list, "d0", "moon", "no", DimSky::DEFAULT).unwrap();
        assert!(!list[0].1.moon);
    }

    #[test]
    fn the_home_world_always_exists() {
        assert!(dimension_exists(""));
        assert!(!dimension_exists("no_such_dimension_anywhere"));
    }
}
