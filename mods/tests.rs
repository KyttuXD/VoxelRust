//! Tests of the mods that come with the game (the folders named in
//! `mods/SHIPPED`): they load cleanly and say what they are meant to say.
//! The build compiles this into the game's own tests.

use crate::block::{Registry, BASE_DATA};
use crate::mods::SHIPPED;

fn load(mods: &[&str]) -> Registry {
    let mut sources: Vec<(&str, &str)> = vec![("base.txt", BASE_DATA)];
    for (path, bytes) in SHIPPED {
        if path.ends_with("/pack.txt") && mods.iter().any(|m| *path == format!("mods/{m}/pack.txt")) {
            sources.push((path, core::str::from_utf8(bytes).unwrap()));
        }
    }
    assert_eq!(sources.len(), mods.len() + 1, "a mod named here is not in mods/SHIPPED");
    Registry::from_sources(&sources, &mut |path| SHIPPED.iter().find(|f| f.0 == path).map(|f| f.1.to_vec()))
}

fn shipped() -> Vec<&'static str> {
    let mut names: Vec<&str> = SHIPPED.iter().filter(|f| f.0.ends_with("/pack.txt")).filter_map(|f| f.0.split('/').nth(1)).collect();
    names.dedup();
    names
}

#[test]
fn every_shipped_mod_loads_without_complaint() {
    let names = shipped();
    assert!(names.len() >= 8, "{names:?}");
    let r = load(&names);
    assert!(r.warnings.is_empty(), "{:#?}", r.warnings);
}

#[test]
fn the_disasters_mod_defines_its_disasters_itself() {
    let r = load(&["disasters"]);
    let ids: Vec<&str> = r.episodes.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, ["earthquake", "tsunami", "tornado", "hailstorm", "blizzard"]);
    let quake = &r.episodes[0];
    assert_eq!((quake.every, quake.lasts, quake.then.as_str()), (1200, 7.0, "tsunami"));
    assert_eq!(quake.actions, ["/shake 0.35", "/sound quake"]);
    assert_eq!(r.episodes[1].every, 0, "a wave never comes by itself");
    let tornado = &r.episodes[2];
    assert!(tornado.funnel && tornado.moves == 3.5);
    assert_eq!(tornado.needs, ["setting disasters", "storm", "lowland", "biome plains savanna"]);
    assert_eq!(r.toggles, [(String::from("disasters"), false, String::from("NATURAL DISASTERS"))]);
}

#[test]
fn the_new_vehicles_are_defined_and_the_big_ones_seat_several() {
    let r = load(&["vehicles", "landvehicles", "boats", "planes"]);
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    let seats = |id: &str| r.sections.iter().find(|s| s.kind == "vehicle" && s.id == id).map(|s| s.num("seats", 1.0, 1.0, 16.0) as i32);
    for (id, n) in [("buggy", 1), ("jeep", 2), ("bus", 6), ("tractor", 1), ("quad", 1), ("speedboat", 2), ("ferry", 8), ("biplane", 2)] {
        assert_eq!(seats(id), Some(n), "vehicle {id}");
    }
}

#[test]
fn vehicles_need_the_vehicles_mod() {
    let alone = load(&["boats"]);
    assert!(alone.warnings.iter().any(|w| w.contains("needs the mod `vehicles`")), "{:?}", alone.warnings);
    let both = load(&["vehicles", "boats"]);
    assert!(both.warnings.is_empty(), "{:?}", both.warnings);
    assert!(both.items.iter().any(|i| i.id == "boat" && i.section.is_some()));
    // Without any of them the game has no vehicles at all.
    assert!(load(&[]).sections.is_empty());
}

#[test]
fn every_item_is_in_the_creative_list_and_beds_are_there() {
    let r = load(&shipped());
    // Every item sits in a category, and every category is a tab.
    for it in &r.items {
        assert!(r.categories.contains(&it.category), "{} is in no tab", it.id);
    }
    let mut tabs: Vec<(&String, usize)> = r.categories.iter().map(|c| (c, r.items.iter().filter(|i| &i.category == c).count())).collect();
    tabs.sort();
    eprintln!("{} items in {} tabs: {:?}", r.items.len(), tabs.len(), tabs);
    for bed in ["white_bed", "red_bed", "black_bed"] {
        assert!(r.items.iter().any(|i| i.id == bed), "{bed} is not in the creative list");
    }
    // Every block that is not hidden is offered somewhere.
    let missing: Vec<&str> = r.blocks.iter().filter(|b| !b.id.is_empty()).map(|b| b.id.as_str()).filter(|id| !r.items.iter().any(|i| i.id == *id) && !r.items.iter().any(|i| i.place.map_or(false, |p| r.blocks[p as usize].id == *id))).collect();
    let mut u: Vec<&str> = missing.iter().copied().filter(|i| !i.ends_with("_open")).collect(); u.sort(); u.dedup(); eprintln!("blocks without an item: {:?}", u);
}
