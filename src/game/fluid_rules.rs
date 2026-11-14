//! The rules of how water and lava treat other blocks, as plain functions
//! (no world needed, so they are tested on their own): what a liquid may
//! flow into, what it washes away, what lava and water make where they
//! meet, and how deep water softens falls and blasts.
//!
//! Meeting table (the lava is what hardens; the water is never changed):
//!
//! | lava | water touches it | result |
//! |------|------------------|--------|
//! | source | from above or beside | obsidian (stone if the game has none) |
//! | flow | from above or beside | cobblestone |
//! | flow, with water below it | (lava falling or running onto water) | stone |
//! | source, with water only below it | | stays (lava floats on the water) |

use crate::block::{self, reg, Block, Kind, AIR};

/// Whether a block of this kind is swept away by running liquid when the
/// data does not say (`washable yes|no`): anything with no collision that is
/// not a liquid, not a thing that clings to a wall (ladders, vines, ropes,
/// chains), not rail track, and not something used (a door, bed, seat...).
/// Plants, torches, signs, carpets, pressure plates and crops follow.
pub fn default_washable(kind: Kind, solid: bool, id: &str, track: bool, used: bool) -> bool {
    if solid || track || used || matches!(kind, Kind::Air | Kind::Liquid) {
        return false;
    }
    !["ladder", "vine", "rope", "chain"].iter().any(|w| id.contains(w))
}

/// Whether a liquid may take the cell: air, and the blocks that are washed
/// away. Everything with a collision shape (full blocks, slabs, stairs,
/// doors, fences, panes, trapdoors, chests, beds) stops it, and so does the
/// other kind of liquid.
pub fn can_flow_into(b: Block) -> bool {
    b == AIR || (block::fluid_of(b).is_none() && !block::def(b).solid && block::def(b).x.washable)
}

/// Whether a block is washed away (not air, not a liquid).
pub fn is_washed(b: Block) -> bool {
    b != AIR && can_flow_into(b)
}

/// What a cell of lava turns into when water touches it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hard {
    Obsidian,
    Cobble,
    Stone,
}

/// Where the water that touches the lava is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Touch {
    /// Water above the lava, or beside it.
    AboveOrBeside,
    /// Water only below it.
    Below,
}

/// What lava of this `level` (8 is a source) becomes when water touches it
/// as `touch`, if anything (the table at the top of this file).
pub fn lava_sets(level: u8, touch: Touch) -> Option<Hard> {
    match (level >= 8, touch) {
        (true, Touch::AboveOrBeside) => Some(Hard::Obsidian),
        (false, Touch::AboveOrBeside) => Some(Hard::Cobble),
        (false, Touch::Below) => Some(Hard::Stone),
        (true, Touch::Below) => None,
    }
}

/// The block for a [`Hard`] result in this game: obsidian and cobblestone
/// if packs define them, stone if not.
pub fn hard_block(h: Hard) -> Block {
    let r = reg();
    match h {
        Hard::Obsidian => r.block("obsidian").unwrap_or(r.wk.stone),
        Hard::Cobble => r.block("cobble").or_else(|| r.block("cobblestone")).unwrap_or(r.wk.stone),
        Hard::Stone => r.wk.stone,
    }
}

/// The power an explosion keeps for breaking blocks when it goes off in
/// water: none (the water takes the blast), as in the old game.
pub fn underwater_block_power(_power: f32) -> f32 {
    0.0
}

/// The share of a blast's strength that still hurts creatures when it goes
/// off under water.
pub const UNDERWATER_HURT: f32 = 0.6;

/// How a fall is softened by the water it ends in: `depth` is the number of
/// liquid blocks in the column where the body lands. Two or more cancel the
/// fall; one halves it.
pub fn fall_after_water(fell: f32, depth: i32) -> f32 {
    if depth >= 2 {
        0.0
    } else if depth == 1 {
        fell * 0.5
    } else {
        fell
    }
}

/// How many liquid cells deep the liquid is at (x, z) around height `y`:
/// the run of liquid cells (of any kind) through `y`, counting up and down.
pub fn depth_at(get: &dyn Fn(i32, i32, i32) -> Block, x: i32, y: i32, z: i32) -> i32 {
    if block::fluid_of(get(x, y, z)).is_none() {
        return 0;
    }
    let mut n = 1;
    let mut k = 1;
    while k < 8 && block::fluid_of(get(x, y + k, z)).is_some() {
        n += 1;
        k += 1;
    }
    k = 1;
    while k < 8 && block::fluid_of(get(x, y - k, z)).is_some() {
        n += 1;
        k += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_wash_light_things_and_keep_the_rest() {
        assert!(default_washable(Kind::Cross, false, "flower_red", false, false));
        assert!(default_washable(Kind::Cross, false, "torch", false, false));
        assert!(default_washable(Kind::Boxes, false, "carpet_red", false, false));
        assert!(default_washable(Kind::Boxes, false, "pressure_plate", false, false));
        // Not ladders and vines, not rails, not doors or beds.
        assert!(!default_washable(Kind::Cross, false, "ladder", false, false));
        assert!(!default_washable(Kind::Cross, false, "jungle_vine", false, false));
        assert!(!default_washable(Kind::Boxes, false, "rail", true, false));
        assert!(!default_washable(Kind::Boxes, false, "door_open", false, true));
        // Solid things and liquids are never washed.
        assert!(!default_washable(Kind::Cube, true, "stone", false, false));
        assert!(!default_washable(Kind::Boxes, true, "stone_slab", false, false));
        assert!(!default_washable(Kind::Liquid, false, "water", false, false));
        assert!(!default_washable(Kind::Air, false, "air", false, false));
    }

    #[test]
    fn the_real_blocks_follow_the_rules() {
        let r = reg();
        assert!(can_flow_into(AIR));
        assert!(can_flow_into(r.wk.tall_grass));
        assert!(can_flow_into(r.wk.flower_red));
        assert!(!can_flow_into(r.wk.stone));
        assert!(!can_flow_into(r.wk.water));
        assert!(!can_flow_into(r.wk.lava));
        assert!(!can_flow_into(r.wk.log));
        assert!(!can_flow_into(r.wk.leaves));
        assert!(is_washed(r.wk.tall_grass));
        assert!(!is_washed(AIR));
        // Any block with a collision box stops a liquid, however it is shaped.
        for (i, d) in r.blocks.iter().enumerate() {
            if d.solid {
                assert!(!can_flow_into(i as Block), "{} lets liquid through", d.id);
            }
            if d.kind == Kind::Liquid {
                assert!(!can_flow_into(i as Block), "{} is a liquid", d.id);
            }
        }
        if let Some(s) = r.block("stone_slab") {
            assert!(!can_flow_into(s));
        }
    }

    #[test]
    fn lava_and_water_meet_by_the_table() {
        assert_eq!(lava_sets(8, Touch::AboveOrBeside), Some(Hard::Obsidian));
        assert_eq!(lava_sets(5, Touch::AboveOrBeside), Some(Hard::Cobble));
        assert_eq!(lava_sets(7, Touch::Below), Some(Hard::Stone));
        assert_eq!(lava_sets(8, Touch::Below), None);
        assert_ne!(hard_block(Hard::Stone), AIR);
        assert_ne!(hard_block(Hard::Cobble), AIR);
        assert_ne!(hard_block(Hard::Obsidian), AIR);
    }

    #[test]
    fn deep_water_cancels_a_fall_and_shallow_halves_it() {
        assert_eq!(fall_after_water(10.0, 3), 0.0);
        assert_eq!(fall_after_water(10.0, 2), 0.0);
        assert_eq!(fall_after_water(10.0, 1), 5.0);
        assert_eq!(fall_after_water(10.0, 0), 10.0);
    }

    #[test]
    fn depth_counts_the_whole_column() {
        let water = reg().wk.water;
        let get = move |_x: i32, y: i32, _z: i32| if (10..13).contains(&y) { water } else { AIR };
        assert_eq!(depth_at(&get, 0, 11, 0), 3);
        assert_eq!(depth_at(&get, 0, 10, 0), 3);
        assert_eq!(depth_at(&get, 0, 14, 0), 0);
    }

    #[test]
    fn an_underwater_blast_breaks_nothing() {
        assert_eq!(underwater_block_power(2.5), 0.0);
        assert!(UNDERWATER_HURT > 0.0 && UNDERWATER_HURT < 1.0);
    }
}
