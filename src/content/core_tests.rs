//! Tests of the game's core content alone: `data/base.txt` and what it includes,
//! with no mod loaded. Everything else (colours, extra woods, furniture, farming,
//! cooking, creatures...) is a mod of the VoxelRust-Mods folder; its tests are
//! there (`tests.rs`). These tests need no mods folder and must stay that way.

use crate::block::{Registry, AIR, BASE_DATA};

fn core() -> Registry {
    Registry::from_sources(&[("base.txt", BASE_DATA)], &mut |_| None)
}

fn has_block(r: &Registry, id: &str) -> bool {
    r.block(id).is_some()
}

fn has_item(r: &Registry, id: &str) -> bool {
    r.items.iter().any(|i| i.id == id)
}

#[test]
fn the_core_loads_cleanly_with_no_mods() {
    let r = core();
    assert!(r.warnings.is_empty(), "{:#?}", r.warnings);
    // A small core: the creatures, colours, furniture and the rest are mods.
    assert!(r.blocks.len() < 400, "{} block states", r.blocks.len());
    assert!(r.entities.is_empty(), "the game has no creatures of its own: {:?}", r.entities.iter().map(|e| &e.id).collect::<alloc::vec::Vec<_>>());
}

#[test]
fn the_core_has_the_world_the_tools_and_the_crafting_table() {
    let r = core();
    for id in ["stone", "dirt", "grass", "sand", "gravel", "water", "lava", "snow", "ice", "sandstone", "clay", "deepstone", "cactus", "cobble", "brick", "stone_bricks",
               "glass", "lamp", "coal_ore", "iron_ore", "copper_ore", "crystal_ore", "ruby_ore", "log", "leaves", "planks", "pine_log", "pine_leaves", "pine_planks",
               "crafting_table", "furnace", "oak_chest", "oak_door", "stone_stairs", "planks_slab", "planks_fence", "flower_red", "tall_grass"] {
        assert!(has_block(&r, id), "the core has no block {id}");
    }
    for id in ["stick", "coal", "ruby", "crystal", "iron_ingot", "copper_ingot", "string", "apple", "berries", "raw_meat", "cooked_meat", "raw_fish", "cooked_fish", "arrow", "bow",
               "wood_pick", "stone_axe", "iron_shovel", "wood_sword", "iron_sword", "golden_apple", "rotten_flesh"] {
        assert!(has_item(&r, id), "the core has no item {id}");
    }
    // What moved out is not here.
    for id in ["red_wool", "white_concrete", "birch_log", "oak_gate", "marble", "chair", "wheat_1", "oven", "tnt", "moss", "farmland"] {
        assert!(!has_block(&r, id), "{id} should be a mod's block");
    }
    for id in ["bread", "wheat", "copper_pick", "ruby_sword", "crossbow", "gunpowder", "iron_hoe"] {
        assert!(!has_item(&r, id), "{id} should be a mod's item");
    }
    // The basic tools can still be made.
    let makes = |id: &str| r.grid_recipes.iter().any(|rc| r.items[rc.out.0 as usize].id == id);
    for id in ["planks", "stick", "crafting_table", "wood_pick", "stone_pick", "iron_axe", "furnace", "oak_chest", "bow", "arrow", "iron_sword"] {
        assert!(makes(id), "no recipe for {id}");
    }
    assert!(r.recipes.iter().any(|rc| rc.furnace), "the furnace smelts nothing");
}

#[test]
fn the_world_generator_falls_back_to_core_blocks_for_what_mods_bring() {
    let r = core();
    let wk = &r.wk;
    // Trees of the extra woods grow oak, caves have no decoration, there are no fields.
    assert_eq!((wk.birch_log, wk.birch_leaves), (wk.log, wk.leaves));
    assert_eq!((wk.palm_log, wk.blossom_leaves), (wk.log, wk.leaves));
    assert_eq!((wk.moss, wk.glow_mushroom, wk.mushroom, wk.crystal_cluster, wk.icicle, wk.magma, wk.cave_vine, wk.stalagmite, wk.web), (AIR, AIR, AIR, AIR, AIR, AIR, AIR, AIR, AIR));
    assert_eq!((wk.farmland, wk.crops), (AIR, [AIR; 4]));
    // Rocks and building materials of terrain made from a map come out of what the core has.
    assert_eq!(wk.basalt, wk.stone);
    assert_eq!(wk.asphalt, wk.gravel);
    assert_eq!(wk.walls[0], wk.stone);
    assert_eq!(wk.window, r.block("glass").unwrap());
    assert_eq!(wk.floor, r.block("planks").unwrap());
    // And what the core itself must have is there.
    for b in [wk.stone, wk.dirt, wk.grass, wk.sand, wk.gravel, wk.water, wk.lava, wk.log, wk.leaves, wk.coal_ore, wk.iron_ore, wk.copper_ore, wk.crystal_ore, wk.snow, wk.ice,
              wk.sandstone, wk.cactus, wk.pine_log, wk.pine_leaves, wk.deepstone, wk.clay, wk.ruby_ore] {
        assert_ne!(b, AIR);
    }
}

#[test]
fn the_core_has_no_creatures_but_has_the_models_it_draws_itself() {
    let r = core();
    for id in ["player", "player_slim", "hand", "hand_slim"] {
        assert!(r.models.iter().any(|m| m.id == id), "no model {id}");
    }
    assert!(r.entities.is_empty());
}

#[test]
fn a_pack_may_bring_a_creature_picture_and_the_game_finds_it() {
    // A one-pixel-wide PNG would do; any valid picture will, so use the game's own player skin.
    let png = crate::block::embedded::file("textures/entity/player.png").expect("the player's picture").to_vec();
    let pack = "picture my_creature file textures/my_creature.png\npicture lost file textures/lost.png\n";
    let r = Registry::from_sources(&[("base.txt", BASE_DATA), ("mods/x/pack.txt", pack)], &mut |p| if p == "mods/x/textures/my_creature.png" { Some(png.clone()) } else { None });
    assert_eq!(r.pictures.len(), 1, "{:?}", r.pictures.iter().map(|p| &p.0).collect::<alloc::vec::Vec<_>>());
    assert_eq!(r.pictures[0].0, "my_creature");
    assert!(r.warnings.iter().any(|w| w.contains("lost.png")), "{:?}", r.warnings);
    assert!(crate::png::decode(&r.pictures[0].1).is_some());
}

#[test]
fn creatures_may_be_up_to_64_tall_32_wide_and_a_million_health_and_no_more() {
    // (A pack may define an entity; its model is only looked up when it is drawn.)
    let pack = "entity giant\n  name Giant\n  model player\n  height 64\n  width 32\n  health 1000000\nentity small\n  name Small\n  model player\n  height 1.8\n  width 0.6\n  health 20\nentity toobig\n  name Too Big\n  model player\n  height 65\n  width 33\n  health 1000001\n";
    let r = Registry::from_sources(&[("base.txt", BASE_DATA), ("mods/x/pack.txt", pack)], &mut |_| None);
    let giant = r.entities.iter().find(|e| e.id == "giant").expect("the giant is defined");
    assert_eq!((giant.height, giant.width, giant.health), (64.0, 32.0, 1_000_000));
    let small = r.entities.iter().find(|e| e.id == "small").expect("an ordinary creature is unchanged");
    assert_eq!((small.height, small.width, small.health), (1.8, 0.6, 20));
    // Past the limits each line is refused with a warning and the default stays.
    let toobig = r.entities.iter().find(|e| e.id == "toobig").expect("defined, with defaults");
    assert_eq!((toobig.height, toobig.width, toobig.health), (1.8, 0.6, 10));
    for what in ["height must be 0.1..64", "width must be 0.1..32", "health must be 1..1000000"] {
        assert!(r.warnings.iter().any(|w| w.contains(what)), "no warning `{what}`: {:?}", r.warnings);
    }
}
