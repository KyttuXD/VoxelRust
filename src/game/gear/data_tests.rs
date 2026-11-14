//! Tests of the item lines this folder reads (`armor`, `durability`, `tag`)
//! and of the order spawn eggs are made in, through a pack read on top of the
//! base data.

use crate::block::{Registry, BASE_DATA};

fn with_packs(packs: &[&str]) -> Registry {
    let names: alloc::vec::Vec<alloc::string::String> = (0..packs.len()).map(|i| alloc::format!("mods/p{}/pack.txt", i)).collect();
    let mut sources: alloc::vec::Vec<(&str, &str)> = alloc::vec![("base.txt", BASE_DATA)];
    for (n, p) in names.iter().zip(packs) {
        sources.push((n.as_str(), p));
    }
    Registry::from_sources(&sources, &mut |_| None)
}

const RECIPE: &str = "craft stone 1 zorp_egg 1\n";
const CREATURE: &str = "entity zorp\n  name Zorp\n  model grazer\n  height 1\n  width 1\n  speed 2\n  health 10\n  behavior wander\n  weight 1\n  trade zorp_egg 1 stone 1\n";

fn item_index(r: &Registry, id: &str) -> usize {
    r.items.iter().position(|i| i.id == id).unwrap_or_else(|| panic!("no item {}", id))
}

fn recipe_uses_the_egg(r: &Registry) -> bool {
    let egg = item_index(r, "zorp_egg") as u16;
    r.grid_recipes.iter().any(|rc| rc.ingredients().iter().any(|ing| ing.items.contains(&egg)))
}

#[test]
fn a_recipe_may_name_the_egg_of_a_creature_of_a_pack_read_after_it() {
    let r = with_packs(&[RECIPE, CREATURE]);
    assert!(r.warnings.iter().all(|w| !w.contains("zorp")), "{:#?}", r.warnings);
    assert!(recipe_uses_the_egg(&r));
    // The creature's own trade names its egg too.
    let zorp = r.entities.iter().find(|e| e.id == "zorp").unwrap();
    assert_eq!(zorp.x.trades.len(), 1);
    // The egg hatches that creature.
    let k = r.entities.iter().position(|e| e.id == "zorp").unwrap();
    assert_eq!(r.items[item_index(&r, "zorp_egg")].x.egg, Some(k));
}

#[test]
fn the_order_of_the_packs_does_not_matter() {
    let a = with_packs(&[RECIPE, CREATURE]);
    let b = with_packs(&[CREATURE, RECIPE]);
    let c = with_packs(&[&[CREATURE, RECIPE].concat()]);
    for r in [&a, &b, &c] {
        assert!(r.warnings.iter().all(|w| !w.contains("zorp")), "{:#?}", r.warnings);
        assert!(recipe_uses_the_egg(r));
    }
}

#[test]
fn an_egg_of_a_creature_that_does_not_exist_is_said_so() {
    let r = with_packs(&["craft stone 1 ghost_egg 1\n"]);
    assert!(r.warnings.iter().any(|w| w.contains("ghost_egg") && w.contains("no creature `ghost`")), "{:#?}", r.warnings);
    assert!(!r.items.iter().any(|i| i.id == "ghost_egg"));
}

#[test]
fn armour_durability_and_tags_are_read() {
    let r = with_packs(&["item test_helm\n  name Test Helm\n  icon stone\n  armor head 3 1.5\n  armor_tint 10 20 30\n  armor_tex testhelm\n  durability 100\n  tag metal 2\n  tag shiny\n\nitem bad_hat\n  name Bad\n  icon stone\n  armor hat 3\n\nitem big_stack\n  name Big\n  icon stone\n  stack 16\n  durability 5\n\nitem too_hard\n  name Hard\n  icon stone\n  armor chest 99\n"]);
    let h = &r.items[item_index(&r, "test_helm")];
    assert_eq!(h.x.armor, Some((0, 3, 1.5)));
    assert_eq!(h.x.armor_tint, Some([10, 20, 30]));
    assert_eq!(h.x.armor_tex, "testhelm");
    assert_eq!(h.x.durability, 100);
    assert_eq!(h.stack, 1);
    assert_eq!(h.x.tags, alloc::vec![(alloc::string::String::from("metal"), 2.0), (alloc::string::String::from("shiny"), 1.0)]);
    // An item that wears is a stack of one.
    assert_eq!(r.items[item_index(&r, "big_stack")].stack, 1);
    // Bad armour lines are refused with a warning and leave the item plain.
    assert_eq!(r.items[item_index(&r, "bad_hat")].x.armor, None);
    assert_eq!(r.items[item_index(&r, "too_hard")].x.armor, None);
    assert!(r.warnings.iter().filter(|w| w.contains("armor needs")).count() >= 2, "{:#?}", r.warnings);
}

#[test]
fn the_base_tools_wear_out() {
    let r = crate::block::reg();
    let pick = &r.items[item_index(r, "wood_pick")];
    assert_eq!(pick.x.durability, 60);
    assert!(r.items[item_index(r, "bow")].x.durability > 0);
    assert_eq!(r.items[item_index(r, "stone")].x.durability, 0);
}
