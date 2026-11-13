//! Which look a new creature is given. A look (a `variant` in the data) may
//! name the biomes it is for (`variant Desert model x biomes desert savanna`):
//! a creature appearing in one of them gets one of those looks, so it blends
//! into the ground it stands on. Anywhere else it gets a look that names no
//! biome, the creature's own plain look included.

use crate::block::Variant;

/// The index to store in `Mob::variant` (0 = the creature's own look, n =
/// its n-th variant) for a creature appearing in `biome`, given any number
/// `roll`. The choice is even among the looks that suit the place.
pub fn pick(variants: &[Variant], biome: &str, roll: u32) -> u8 {
    let suits = |v: &Variant| v.biomes.iter().any(|b| b == biome);
    let matching = variants.iter().filter(|v| suits(v)).count();
    if matching > 0 {
        let mut left = (roll % matching as u32) as usize;
        for (i, v) in variants.iter().enumerate() {
            if suits(v) {
                if left == 0 {
                    return (i + 1).min(255) as u8;
                }
                left -= 1;
            }
        }
    }
    // Looks for anywhere: the plain one (0) and the variants without biomes.
    let generic = 1 + variants.iter().filter(|v| v.biomes.is_empty()).count();
    let mut left = (roll % generic as u32) as usize;
    if left == 0 {
        return 0;
    }
    for (i, v) in variants.iter().enumerate() {
        if v.biomes.is_empty() {
            left -= 1;
            if left == 0 {
                return (i + 1).min(255) as u8;
            }
        }
    }
    0
}

/// Any look at all, for places with no biome (a spawn egg, `/spawn`).
pub fn any(variants: &[Variant], roll: u32) -> u8 {
    (roll % (variants.len() as u32 + 1)) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    fn look(name: &str, biomes: &[&str]) -> Variant {
        Variant { name: name.to_string(), model: 0, tint: [255; 3], size: 1.0, tex: String::new(), biomes: biomes.iter().map(|b| b.to_string()).collect() }
    }

    #[test]
    fn a_biome_gets_only_the_looks_made_for_it() {
        let v = [look("Desert", &["desert", "savanna"]), look("Snow", &["tundra"]), look("Sand", &["desert"])];
        let mut seen: Vec<u8> = (0..40).map(|r| pick(&v, "desert", r)).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen, [1, 3]);
        assert!((0..40).all(|r| pick(&v, "tundra", r) == 2));
        assert!((0..40).all(|r| pick(&v, "savanna", r) == 1));
    }

    #[test]
    fn elsewhere_it_has_the_plain_look_or_a_look_for_anywhere() {
        let v = [look("Desert", &["desert"]), look("Spotted", &[])];
        let mut seen: Vec<u8> = (0..40).map(|r| pick(&v, "plains", r)).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen, [0, 2]);
        // No looks at all: always the plain one.
        assert!((0..10).all(|r| pick(&[], "plains", r) == 0));
        // Only looks for places: the plain one everywhere else.
        let only = [look("Desert", &["desert"])];
        assert!((0..10).all(|r| pick(&only, "forest", r) == 0));
    }

    #[test]
    fn looks_without_biomes_are_even_as_before() {
        let v = [look("A", &[]), look("B", &[]), look("C", &[])];
        let mut seen: Vec<u8> = (0..40).map(|r| pick(&v, "forest", r)).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen, [0, 1, 2, 3]);
        assert_eq!(any(&v, 5), 1);
    }
}
