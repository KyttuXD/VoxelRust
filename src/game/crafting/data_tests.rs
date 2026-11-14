//! Tests of the recipes in the game's own data (data/base.txt) and of the
//! `craft`, `shaped` and `group` lines of a pack.

use super::check::{clashes, problem};
use super::matcher;
use super::recipe::Pattern;
use crate::block::{reg, Registry, BASE_DATA};

fn with_pack(pack: &str) -> Registry {
    Registry::from_sources(&[("base.txt", BASE_DATA), ("pack.txt", pack)], &mut |_| None)
}

#[test]
fn base_data_has_no_complaints_about_recipes() {
    let r = reg();
    let bad: alloc::vec::Vec<&String> = r.warnings.iter().filter(|w| w.contains("recipe") || w.contains("group")).collect();
    assert!(bad.is_empty(), "{:#?}", bad);
    assert!(r.grid_recipes.len() > 300, "{}", r.grid_recipes.len());
}

#[test]
fn every_recipe_makes_an_item_that_exists_from_items_that_exist() {
    let r = reg();
    let n = r.items.len();
    for rc in &r.grid_recipes {
        assert!((rc.out.0 as usize) < n, "{:?}", rc);
        assert!((1..=999).contains(&rc.out.1), "{:?}", rc);
        for ing in rc.ingredients() {
            assert!(ing.items.iter().all(|&i| (i as usize) < n), "{:?}", rc);
        }
        assert_eq!(problem(rc), None, "{} {:?}", r.items[rc.out.0 as usize].id, rc);
        // Nothing needs more of an item than one slot holds.
        for ing in rc.ingredients() {
            assert!(ing.items.iter().any(|&i| r.items[i as usize].stack >= ing.need), "{}", r.items[rc.out.0 as usize].id);
        }
    }
}

#[test]
fn every_shaped_recipe_is_well_formed_and_the_well_known_ones_are_there() {
    let r = reg();
    let shaped = r.grid_recipes.iter().filter(|rc| matches!(rc.pattern, Pattern::Shaped { .. })).count();
    assert!(shaped > 100, "{}", shaped);
    let of = |id: &str| r.grid_recipes.iter().find(|rc| r.items[rc.out.0 as usize].id == id).unwrap_or_else(|| panic!("no recipe for {}", id));
    for tool in ["wood_pick", "stone_axe", "iron_pick", "ruby_sword", "crystal_hoe", "copper_shovel"] {
        assert!(matches!(of(tool).pattern, Pattern::Shaped { .. }), "{}", tool);
    }
    // A pick is three heads over two sticks: it needs the table.
    assert!(of("iron_pick").needs_table());
    // Sticks and the table itself can be made by hand.
    assert!(!of("stick").needs_table());
    assert!(!of("crafting_table").needs_table());
    assert!(!of("planks").needs_table());
    // Any plank makes sticks: the group is real.
    let stick = of("stick");
    assert!(stick.ingredients()[0].items.len() >= 8);
}

#[test]
fn a_pick_is_made_from_its_shape_and_only_there() {
    let r = reg();
    let id = |s: &str| r.items.iter().position(|i| i.id == s).unwrap() as u16;
    let (iron, stick) = (id("iron_ore"), id("stick"));
    let cell = |item: u16| Some(crate::inventory::Stack { item, count: 1 });
    let mut cells = [None; 9];
    for i in [0, 1, 2] {
        cells[i] = cell(iron);
    }
    for i in [4, 7] {
        cells[i] = cell(stick);
    }
    let (found, _) = matcher::find(&r.grid_recipes, 3, 3, &cells).expect("no pick");
    assert_eq!(r.grid_recipes[found].out, (id("iron_pick"), 1));
    // An axe laid out the other way round is still an axe.
    let mut axe = [None; 9];
    for i in [1, 2] {
        axe[i] = cell(iron);
    }
    axe[4] = cell(stick);
    axe[5] = cell(iron);
    axe[7] = cell(stick);
    let (found, _) = matcher::find(&r.grid_recipes, 3, 3, &axe).expect("no axe");
    assert_eq!(r.grid_recipes[found].out, (id("iron_axe"), 1));
    // The pick with one head missing is not a pick (it is a hoe now).
    cells[2] = None;
    let missing = matcher::find(&r.grid_recipes, 3, 3, &cells).map(|(i, _)| r.grid_recipes[i].out.0);
    assert_ne!(missing, Some(id("iron_pick")));
    // Nor is the pick with a handle missing.
    cells[2] = cell(iron);
    cells[7] = None;
    assert!(matcher::find(&r.grid_recipes, 3, 3, &cells).is_none());
}

#[test]
fn no_two_recipes_make_different_things_from_the_same_grid() {
    let r = reg();
    let bad = clashes(&r.grid_recipes);
    let names: alloc::vec::Vec<String> = bad
        .iter()
        .map(|&(a, b)| format!("{} / {}", r.items[r.grid_recipes[a].out.0 as usize].id, r.items[r.grid_recipes[b].out.0 as usize].id))
        .collect();
    assert!(bad.is_empty(), "{:#?}", names);
}

#[test]
fn a_pack_may_add_craft_shaped_and_group_lines() {
    let r = with_pack(
        "group gems ruby crystal\n\
         shaped lamp 2 \"G.G\" \" S \" with G @gems S stick\n\
         shaped stone_slab 1 X.. .Y. with X stone Y dirt*3\n\
         shapeless glass 1 sand 1 coal 1\n\
         craft brick 1 clay 3\n\
         recipe sandstone 1 glass 1\n",
    );
    let id = |s: &str| r.items.iter().position(|i| i.id == s).unwrap() as u16;
    assert!(r.warnings.iter().all(|w| !w.contains("recipe")), "{:?}", r.warnings);
    let lamp = r.grid_recipes.iter().rev().find(|rc| rc.out == (id("lamp"), 2)).expect("no lamp");
    match &lamp.pattern {
        Pattern::Shaped { w, h, cells } => {
            assert_eq!((*w, *h), (3, 2));
            assert_eq!(cells[0].as_ref().unwrap().items, [id("ruby"), id("crystal")]);
            assert!(cells[1].is_none());
            assert_eq!(cells[4].as_ref().unwrap().items, [id("stick")]);
        }
        _ => panic!("not shaped"),
    }
    let slab = r.grid_recipes.iter().rev().find(|rc| rc.out == (id("stone_slab"), 1)).expect("no slab");
    assert_eq!(slab.ingredients()[1].need, 3);
    // The same lines with a mistake each are refused, one warning apiece.
    let bad = with_pack(
        "shaped lamp 2 \"GG\" \"S\" with G ruby S stick\n\
         shaped lamp 2 GG with G nothing_real\n\
         shaped lamp 2 GG with G @no_such_group\n\
         shaped lamp 2 GX with G ruby\n\
         shaped lamp 2 GG\n\
         craft lamp 1 ruby\n\
         craft lamp 1 ruby 1000\n",
    );
    assert_eq!(bad.warnings.len(), 7, "{:#?}", bad.warnings);
}
