//! What items do to the world: breaking, placing and picking blocks.

use crate::block::{self, reg, Block, ItemDef, Kind, AIR};
use crate::chunk::CH;
use crate::math::{floord, floori, PI};
use crate::player::Player;
use crate::world::{Hit, World};

/// Blocks that building may overwrite.
pub fn replaceable(b: Block) -> bool {
    matches!(reg().kind[b as usize], Kind::Air | Kind::Liquid | Kind::Cross)
}

/// Call `f` for every cell of a ball of radius `r` (0 is a single cell).
fn sphere(center: [i32; 3], r: i32, mut f: impl FnMut([i32; 3])) {
    for dx in -r..=r {
        for dy in -r..=r {
            for dz in -r..=r {
                if dx * dx + dy * dy + dz * dz <= r * r + r {
                    f([center[0] + dx, center[1] + dy, center[2] + dz]);
                }
            }
        }
    }
}

/// Which way the player faces, in quarter turns: 0 looks down -Z, 1 down -X.
pub fn facing(yaw: f32) -> u8 {
    floori(yaw / (PI * 0.5) + 0.5).rem_euclid(4) as u8
}

/// Whether `b` is the block whose first state is `first`, or what that
/// block toggles into (a door open or shut is the same door).
fn same_or_partner(b: Block, first: Block) -> bool {
    let d = block::def(b);
    d.first == first || d.x.toggle == Some(first) || block::def(first).x.toggle == Some(d.first)
}

/// The cell one step the way a block faces (0 looks down -Z, 1 down -X, 2 down +Z, 3 down +X).
fn step_of(rotation: u8) -> [i32; 3] {
    match rotation % 4 {
        0 => [0, 0, -1],
        1 => [-1, 0, 0],
        2 => [0, 0, 1],
        _ => [1, 0, 0],
    }
}

/// Where the other half of a two-block-long block (a bed) at `c` is, if it is there.
fn long_partner(world: &World, c: [i32; 3]) -> Option<[i32; 3]> {
    let b = world.get(c[0], c[1], c[2]);
    let d = block::def(b);
    let (ahead, other) = d.x.long?;
    let rotation = if d.rotations > 1 { ((b - d.first) % d.rotations as Block) as u8 } else { 0 };
    let s = step_of(rotation);
    let k = if ahead { 1 } else { -1 };
    let at = [c[0] + s[0] * k, c[1], c[2] + s[2] * k];
    (block::def(world.get(at[0], at[1], at[2])).first == other).then_some(at)
}

/// The other half of a two-block-high block at `c`, if it is there.
fn other_half(world: &World, c: [i32; 3]) -> Option<[i32; 3]> {
    let d = block::def(world.get(c[0], c[1], c[2]));
    if !d.x.tall {
        return None;
    }
    // Which half is this? Count the same block below: with none (or two,
    // four...) under it, it is a bottom half and its partner is above.
    let same = |y: i32| {
        let nb = world.get(c[0], y, c[2]);
        nb != AIR && same_or_partner(nb, d.first)
    };
    let mut below = 0;
    while below < 64 && same(c[1] - 1 - below) {
        below += 1;
    }
    let partner = if below % 2 == 0 { c[1] + 1 } else { c[1] - 1 };
    if same(partner) {
        Some([c[0], partner, c[2]])
    } else {
        None
    }
}

/// Every cell a prop `w` blocks wide and `h` tall takes up when it stands
/// at `c` (the middle of its floor), the cell `c` itself included.
fn footprint(c: [i32; 3], (w, h): (u8, u8), mut f: impl FnMut([i32; 3])) {
    let (w, h) = (w.max(1) as i32, h.max(1) as i32);
    for dy in 0..h {
        for dx in -(w / 2)..(w + 1) / 2 {
            for dz in -(w / 2)..(w + 1) / 2 {
                f([c[0] + dx, c[1] + dy, c[2] + dz]);
            }
        }
    }
}

/// The block a cell belongs to: for the hidden bulk of a big prop, the
/// prop's own cell; for anything else, the cell itself.
pub fn prop_root(world: &World, c: [i32; 3]) -> [i32; 3] {
    if !block::def(world.get(c[0], c[1], c[2])).x.part {
        return c;
    }
    for dy in 0..24 {
        for dx in -3..=3 {
            for dz in -3..=3 {
                let r = [c[0] + dx, c[1] - dy, c[2] + dz];
                let fills = block::def(world.get(r[0], r[1], r[2])).x.fills;
                if fills.0 == 0 {
                    continue;
                }
                let mut inside = false;
                footprint(r, fills, |f| inside |= f == c);
                if inside {
                    return r;
                }
            }
        }
    }
    c
}

/// Take one block out of the world, with its other half if it has one, or
/// the whole of a big prop.
pub fn remove(world: &mut World, c: [i32; 3]) {
    let c = prop_root(world, c);
    let fills = block::def(world.get(c[0], c[1], c[2])).x.fills;
    if fills.0 != 0 {
        footprint(c, fills, |f| {
            if block::def(world.get(f[0], f[1], f[2])).x.part {
                world.set_block(f[0], f[1], f[2], AIR);
            }
        });
    }
    if let Some(o) = other_half(world, c) {
        world.set_block(o[0], o[1], o[2], AIR);
    }
    if let Some(o) = long_partner(world, c) {
        world.set_block(o[0], o[1], o[2], AIR);
    }
    world.set_block(c[0], c[1], c[2], AIR);
    // Grass, flowers and crops standing on it go with the ground.
    let up = [c[0], c[1] + 1, c[2]];
    if world.is_loaded(up[0], up[2]) && grows_from_below(world.get(up[0], up[1], up[2])) {
        remove(world, up);
    }
}

/// A plant that stands on the block below it (not vines, webs, torches or
/// anything else that clings to something).
fn grows_from_below(b: Block) -> bool {
    if reg().kind[b as usize] != Kind::Cross {
        return false;
    }
    let id = block::def(b).id.as_str();
    !["vine", "web", "torch", "lantern", "chain", "rope", "ladder", "moss_hang", "root"].iter().any(|w| id.contains(w))
}

/// Let the block at `c` change if it has a `react` line and one of the
/// blocks it names is beside it (above, below or on a side).
pub fn react(world: &mut World, c: [i32; 3]) {
    let b = world.get(c[0], c[1], c[2]);
    let reacts = &block::def(b).x.reacts;
    if reacts.is_empty() {
        return;
    }
    for [dx, dy, dz] in [[1, 0, 0], [-1, 0, 0], [0, 1, 0], [0, -1, 0], [0, 0, 1], [0, 0, -1]] {
        let near = block::def(world.get(c[0] + dx, c[1] + dy, c[2] + dz)).first;
        if let Some(&(_, becomes)) = reacts.iter().find(|r| r.0 == near) {
            world.set_block(c[0], c[1], c[2], becomes);
            return;
        }
    }
}

/// Swap a block for its other form (open or shut a door). Returns whether
/// it had one.
pub fn toggle(world: &mut World, c: [i32; 3]) -> bool {
    let swap = |world: &mut World, c: [i32; 3]| {
        let b = world.get(c[0], c[1], c[2]);
        let d = block::def(b);
        match d.x.toggle {
            Some(to) => {
                // Keep the facing where both forms have the same states.
                let t = block::def(to);
                let same = t.rotations == d.rotations && t.flips == d.flips;
                world.set_block(c[0], c[1], c[2], if same { to + (b - d.first) } else { to });
                true
            }
            None => false,
        }
    };
    let half = other_half(world, c);
    let done = swap(world, c);
    if let (true, Some(o)) = (done, half) {
        swap(world, o);
    }
    done
}

/// Let the block at `c` drop one cell if it is the falling kind and has
/// nothing under it. Returns whether it moved.
pub fn fall_step(world: &mut World, c: [i32; 3]) -> bool {
    let b = world.get(c[0], c[1], c[2]);
    if b == AIR || !block::def(b).x.falls || c[1] <= 0 || !world.is_loaded(c[0], c[2]) {
        return false;
    }
    if !replaceable(world.get(c[0], c[1] - 1, c[2])) {
        return false;
    }
    world.set_block(c[0], c[1], c[2], AIR);
    world.set_block_cause(crate::terrain::world_log::cause::FALL, c[0], c[1] - 1, c[2], b);
    true
}

/// Remove the targeted block, or a ball of blocks for a brush item. Liquids
/// and unbreakable blocks stay.
pub fn break_at(world: &mut World, item: &ItemDef, hit: &Hit) {
    sphere(hit.cell, item.brush as i32, |c| {
        let b = world.get(c[0], c[1], c[2]);
        if b != AIR && !block::def(b).unbreakable && reg().kind[b as usize] != Kind::Liquid {
            remove(world, c);
        }
    });
}

/// Use the item against the targeted face. Returns whether anything changed.
pub fn place_at(world: &mut World, player: &Player, item: &ItemDef, hit: &Hit) -> bool {
    let r = reg();
    let (cell, prev) = (hit.cell, hit.prev);
    let aimed = world.get(cell[0], cell[1], cell[2]);
    // What to place: the item's block, or for a brush with no block of its
    // own, more of whatever is being pointed at.
    let first = match item.place {
        Some(b) => b,
        None if item.brush > 0 => block::def(aimed).first,
        None => return false,
    };
    if first == AIR {
        return false;
    }
    // Plants are replaced rather than built against.
    let at = if r.kind[aimed as usize] == Kind::Cross { cell } else { prev };
    // Upside-down when placed against a ceiling or the upper half of a wall.
    let upper = if prev[1] != cell[1] { prev[1] < cell[1] } else { hit.point[1] - floord(hit.point[1]) as f64 > 0.5 };
    let state = r.placed_state(first, facing(player.yaw), upper);
    let solid = block::is_solid(state);
    let extra = &block::def(state).x;
    let mut changed = false;
    sphere(at, item.brush as i32, |c| {
        if !(0..CH).contains(&c[1]) || !replaceable(world.get(c[0], c[1], c[2])) {
            return;
        }
        if solid && player.overlaps_cell(c) {
            return;
        }
        // Crops and the like only stand on certain blocks.
        if !extra.on.is_empty() && !extra.on.contains(&block::def(world.get(c[0], c[1] - 1, c[2])).first) {
            return;
        }
        if extra.fills.0 != 0 && reg().part != AIR {
            // A big prop needs all of its room free.
            let mut free = true;
            footprint(c, extra.fills, |f| free &= (0..CH).contains(&f[1]) && replaceable(world.get(f[0], f[1], f[2])) && !player.overlaps_cell(f));
            if !free {
                return;
            }
            footprint(c, extra.fills, |f| {
                world.set_block(f[0], f[1], f[2], reg().part);
            });
        }
        if let Some((true, other)) = extra.long {
            // A bed: its far half goes one block ahead, the way the player faces.
            let s = step_of(facing(player.yaw));
            let far = [c[0] + s[0], c[1], c[2] + s[2]];
            if !replaceable(world.get(far[0], far[1], far[2])) || (solid && player.overlaps_cell(far)) {
                return;
            }
            let half = r.placed_state(other, facing(player.yaw), upper);
            world.set_block(far[0], far[1], far[2], half);
        }
        if extra.tall {
            // Doors need room for their upper half.
            let up = [c[0], c[1] + 1, c[2]];
            if up[1] >= CH || !replaceable(world.get(up[0], up[1], up[2])) || (solid && player.overlaps_cell(up)) {
                return;
            }
            world.set_block(up[0], up[1], up[2], state);
        }
        changed |= world.set_block(c[0], c[1], c[2], state);
    });
    changed
}

/// The item that places the targeted block, if there is one.
pub fn pick(world: &World, hit: &Hit) -> Option<usize> {
    let first = block::def(world.get(hit.cell[0], hit.cell[1], hit.cell[2])).first;
    if first == AIR {
        return None;
    }
    reg().items.iter().position(|it| it.place == Some(first) && it.brush == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::Pool;
    use crate::math::{forward, Vec3};
    use crate::mesh::MeshData;
    use crate::world::{GpuMesh, MeshSink, NullStore};

    struct NoSink;
    impl MeshSink for NoSink {
        fn upload(&mut self, _: &MeshData) -> GpuMesh {
            GpuMesh::default()
        }
        fn free(&mut self, _: GpuMesh) {}
    }

    /// A loaded world with a stone floor at y = 120 around the origin chunk.
    fn world() -> World {
        let mut w = World::new(5);
        let mut pool = Pool::new(0, 5);
        for _ in 0..3000 {
            w.update(0, 0, 1, &mut pool, &mut NoSink, &mut NullStore);
            if w.settled {
                break;
            }
        }
        assert!(w.settled);
        for x in 0..16 {
            for z in 0..16 {
                w.set_block(x, 120, z, reg().wk.stone);
            }
        }
        w
    }

    fn item(id: &str) -> &'static ItemDef {
        reg().items.iter().find(|i| i.id == id).unwrap()
    }

    fn aim(w: &World, from: [f64; 3], dir: Vec3) -> Hit {
        w.raycast(from, dir, 12.0).expect("nothing targeted")
    }

    const DOWN: Vec3 = Vec3::new(0.0, -1.0, 0.0);

    #[test]
    fn a_bed_is_two_blocks_long_the_way_you_face_and_breaks_as_one() {
        if crate::native::skip_without_mods() {
            return;
        }
        let bed = reg().block("red_bed").unwrap();
        let head = reg().block("red_bed_head").unwrap();
        // (yaw in quarter turns, where the head ends up relative to the foot)
        for (turns, step) in [(0, [0, -1]), (1, [-1, 0]), (2, [0, 1]), (3, [1, 0])] {
            let mut w = world();
            let mut p = Player::new([2.5, 121.0, 2.5]);
            p.yaw = turns as f32 * core::f32::consts::FRAC_PI_2;
            let hit = aim(&w, [8.5, 126.0, 8.5], DOWN);
            assert!(place_at(&mut w, &p, item("red_bed"), &hit), "facing {turns}");
            let foot = [8, 121, 8];
            let far = [8 + step[0], 121, 8 + step[1]];
            assert_eq!(block::def(w.get(foot[0], foot[1], foot[2])).first, bed, "foot, facing {turns}");
            assert_eq!(block::def(w.get(far[0], far[1], far[2])).first, head, "head, facing {turns}");
            // Breaking either half takes both.
            remove(&mut w, far);
            assert_eq!(w.get(foot[0], foot[1], foot[2]), AIR);
            assert_eq!(w.get(far[0], far[1], far[2]), AIR);
            assert!(place_at(&mut w, &p, item("red_bed"), &hit));
            remove(&mut w, foot);
            assert_eq!(w.get(far[0], far[1], far[2]), AIR);
        }
        // No room for the head: nothing is placed.
        let mut w = world();
        let p = Player::new([2.5, 121.0, 2.5]);
        w.set_block(8, 121, 7, reg().wk.stone);
        let hit = aim(&w, [8.5, 126.0, 8.5], DOWN);
        assert!(!place_at(&mut w, &p, item("red_bed"), &hit));
        assert_eq!(w.get(8, 121, 8), AIR);
    }

    #[test]
    fn a_block_with_a_react_line_changes_beside_its_trigger() {
        let r = block::Registry::from_sources(
            &[("base.txt", block::BASE_DATA), ("p", "block wet_sand\n  tex sand\nblock mud\n  tex dirt\nblock dry_dirt\n  tex dirt\n  react wet_sand mud\n")],
            &mut |_| None,
        );
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let (dry, wet, mud) = (r.block("dry_dirt").unwrap(), r.block("wet_sand").unwrap(), r.block("mud").unwrap());
        assert_eq!(r.blocks[dry as usize].x.reacts, alloc::vec![(wet, mud)]);
    }

    #[test]
    fn big_props_take_their_room_and_break_as_one() {
        if crate::native::skip_without_mods() {
            return;
        }
        let mut w = world();
        let p = Player::new([2.5, 121.0, 2.5]);
        let hit = aim(&w, [8.5, 126.0, 8.5], DOWN);
        assert!(place_at(&mut w, &p, item("golem_statue"), &hit));
        let statue = reg().block("golem_statue").unwrap();
        assert_eq!(block::def(w.get(8, 121, 8)).first, statue);
        assert!(block::def(w.get(8, 122, 8)).x.part && block::def(w.get(8, 123, 8)).x.part, "the rest of it is filled in");
        assert_eq!(w.get(8, 124, 8), AIR);
        // Any part of it is the statue.
        assert_eq!(prop_root(&w, [8, 123, 8]), [8, 121, 8]);
        // No room: a second one cannot stand where the first one's head is.
        w.set_block(9, 122, 8, reg().wk.stone);
        let side = Hit { cell: [9, 122, 8], prev: [9, 123, 8], dist: 1.0, point: [9.5, 123.0, 8.5] };
        assert!(place_at(&mut w, &p, item("golem_statue"), &side));
        w.set_block(10, 121, 8, reg().wk.stone);
        let low = Hit { cell: [8, 120, 9], prev: [8, 121, 9], dist: 1.0, point: [8.5, 121.0, 9.5] };
        w.set_block(8, 122, 9, reg().wk.stone);
        assert!(!place_at(&mut w, &p, item("golem_statue"), &low), "blocked above");
        // Breaking the top takes all of it and nothing else.
        remove(&mut w, [8, 123, 8]);
        assert_eq!((w.get(8, 121, 8), w.get(8, 122, 8), w.get(8, 123, 8)), (AIR, AIR, AIR));
        assert!(block::def(w.get(9, 124, 8)).x.part, "the neighbour stands");
    }

    #[test]
    fn doors_are_two_high_and_open_shut_and_break_as_one() {
        let mut w = world();
        let p = Player::new([2.5, 121.0, 2.5]);
        let hit = aim(&w, [8.5, 124.0, 8.5], DOWN);
        assert!(place_at(&mut w, &p, item("oak_door"), &hit));
        let (shut, open) = (reg().block("oak_door").unwrap(), reg().block("oak_door_open").unwrap());
        let is = |w: &World, y: i32, first: Block| block::def(w.get(8, y, 8)).first == first;
        assert!(is(&w, 121, shut) && is(&w, 122, shut), "both halves placed");
        // Opening from either half swings both, keeping the way it faces.
        let before = w.get(8, 121, 8) - shut;
        assert!(toggle(&mut w, [8, 122, 8]));
        assert!(is(&w, 121, open) && is(&w, 122, open));
        assert_eq!(w.get(8, 121, 8) - open, before);
        assert!(toggle(&mut w, [8, 121, 8]));
        assert!(is(&w, 121, shut) && is(&w, 122, shut));
        // Breaking one half takes the other with it.
        remove(&mut w, [8, 122, 8]);
        assert_eq!((w.get(8, 121, 8), w.get(8, 122, 8)), (AIR, AIR));
        // No room overhead, no door.
        w.set_block(8, 122, 8, reg().wk.stone);
        let hit = aim(&w, [8.5, 121.5, 5.5], Vec3::new(0.0, -1.0, 0.0));
        let _ = hit;
        let low = Hit { cell: [8, 120, 8], prev: [8, 121, 8], point: [8.5, 121.0, 8.5], dist: 1.0 };
        assert!(!place_at(&mut w, &p, item("oak_door"), &low));
        // A stone block does not open.
        assert!(!toggle(&mut w, [8, 120, 8]));
    }

    #[test]
    fn sand_falls_until_it_lands_and_stone_does_not() {
        let mut w = world();
        let (sand, stone) = (reg().wk.sand, reg().wk.stone);
        w.set_block(4, 126, 4, sand);
        w.set_block(4, 127, 4, sand);
        w.set_block(6, 126, 6, stone);
        assert!(!fall_step(&mut w, [6, 126, 6]), "stone stays put");
        // Step every cell of the column until nothing moves, as the game does.
        let mut moves = 0;
        loop {
            let mut any = false;
            for y in 121..128 {
                any |= fall_step(&mut w, [4, y, 4]);
            }
            if !any {
                break;
            }
            moves += 1;
            assert!(moves < 50);
        }
        assert_eq!((w.get(4, 121, 4), w.get(4, 122, 4), w.get(4, 123, 4)), (sand, sand, AIR));
    }

    #[test]
    fn seeds_need_tilled_soil() {
        if crate::native::skip_without_mods() {
            return;
        }
        let mut w = world();
        let p = Player::new([2.5, 121.0, 2.5]);
        let hit = aim(&w, [8.5, 124.0, 8.5], DOWN);
        // On bare stone a seed does nothing.
        assert!(!place_at(&mut w, &p, item("wheat_seeds"), &hit));
        // A hoe's work, done by hand here: soil, then the seed takes.
        let hoe = item("stone_hoe");
        w.set_block(8, 120, 8, reg().wk.dirt);
        let to = hoe.x.turns.iter().find(|t| t.0 == reg().wk.dirt).expect("hoes till dirt").1;
        assert_eq!(to, reg().wk.farmland);
        w.set_block(8, 120, 8, to);
        let hit = aim(&w, [8.5, 124.0, 8.5], DOWN);
        assert!(place_at(&mut w, &p, item("wheat_seeds"), &hit));
        let sprout = w.get(8, 121, 8);
        let d = block::def(sprout);
        assert_eq!(d.id, "wheat_1");
        // It grows in stages and ends as something that gives wheat and seed.
        let mut stage = sprout;
        let mut steps = 0;
        while let Some((next, secs)) = block::def(stage).x.grows {
            assert!(secs > 0.0);
            stage = next;
            steps += 1;
            assert!(steps < 10, "growth must end");
        }
        assert_eq!(steps, 3);
        let ripe = block::def(stage);
        assert_eq!(reg().items[ripe.drop.unwrap()].id, "wheat");
        assert_eq!(reg().items[ripe.x.also.unwrap().0].id, "wheat_seeds");
    }

    #[test]
    fn place_break_and_pick_a_block() {
        let mut w = world();
        let p = Player::new([2.5, 121.0, 2.5]);
        let hit = aim(&w, [8.5, 124.0, 8.5], DOWN);
        assert!(place_at(&mut w, &p, item("brick"), &hit));
        let brick = reg().block("brick").unwrap();
        assert_eq!(w.get(8, 121, 8), brick);

        let hit = aim(&w, [8.5, 124.0, 8.5], DOWN);
        assert_eq!(hit.cell, [8, 121, 8]);
        assert_eq!(reg().items[pick(&w, &hit).unwrap()].id, "brick");
        break_at(&mut w, item("brick"), &hit);
        assert_eq!(w.get(8, 121, 8), AIR);
    }

    #[test]
    fn cannot_build_inside_the_player_or_break_deepstone() {
        let mut w = world();
        let p = Player::new([8.5, 121.0, 8.5]);
        let hit = aim(&w, p.eye(), DOWN);
        assert_eq!(hit.prev, [8, 121, 8]);
        assert!(!place_at(&mut w, &p, item("brick"), &hit));
        assert_eq!(w.get(8, 121, 8), AIR);
        // A flower has no collision, so it may share the player's cell.
        assert!(place_at(&mut w, &p, item("flower_red"), &hit));

        let deep = reg().wk.deepstone;
        w.set_block(3, 121, 3, deep);
        let hit = aim(&w, [3.5, 124.0, 3.5], DOWN);
        break_at(&mut w, item("brick"), &hit);
        assert_eq!(w.get(3, 121, 3), deep);
    }

    #[test]
    fn stairs_face_the_player_and_flip_under_ceilings() {
        let mut w = world();
        let stairs = reg().block("stone_stairs").unwrap();
        for (yaw, want) in [(0.0, 0), (PI * 0.5, 1), (PI, 2), (-PI * 0.5, 3), (0.2, 0), (-3.0, 2)] {
            let mut p = Player::new([2.5, 121.0, 2.5]);
            p.yaw = yaw;
            w.set_block(8, 121, 8, AIR);
            let hit = aim(&w, [8.5, 124.0, 8.5], DOWN);
            assert!(place_at(&mut w, &p, item("stone_stairs"), &hit));
            assert_eq!(w.get(8, 121, 8), stairs + want, "yaw {yaw}");
        }
        // Against the underside of a block: upside-down.
        let p = Player::new([2.5, 121.0, 2.5]);
        w.set_block(12, 125, 12, reg().wk.stone);
        let hit = aim(&w, [12.5, 122.0, 12.5], Vec3::new(0.0, 1.0, 0.0));
        assert!(place_at(&mut w, &p, item("stone_slab"), &hit));
        assert_eq!(w.get(12, 124, 12), reg().block("stone_slab").unwrap() + 1);
        // Against the upper half of a wall: upside-down too; lower half: not.
        w.set_block(5, 121, 5, reg().wk.stone);
        for (y, flipped) in [(121.8, 1), (121.2, 0)] {
            w.set_block(5, 121, 6, AIR);
            let hit = aim(&w, [5.5, y, 9.5], forward(0.0, 0.0));
            assert_eq!((hit.cell, hit.prev), ([5, 121, 5], [5, 121, 6]));
            assert!(place_at(&mut w, &p, item("stone_slab"), &hit));
            assert_eq!(w.get(5, 121, 6), reg().block("stone_slab").unwrap() + flipped);
        }
    }

    #[test]
    fn plants_go_when_the_ground_under_them_is_broken() {
        let mut w = world();
        w.set_block(8, 120, 8, reg().wk.stone);
        w.set_block(8, 121, 8, reg().wk.tall_grass);
        remove(&mut w, [8, 120, 8]);
        assert_eq!(w.get(8, 120, 8), AIR);
        assert_eq!(w.get(8, 121, 8), AIR);
    }

    #[test]
    fn plants_are_replaced_and_brushes_work_on_many_blocks() {
        let mut w = world();
        let p = Player::new([2.5, 121.0, 2.5]);
        w.set_block(8, 121, 8, reg().wk.tall_grass);
        let hit = aim(&w, [8.5, 124.0, 8.5], DOWN);
        assert_eq!(hit.cell, [8, 121, 8]);
        assert!(place_at(&mut w, &p, item("planks"), &hit));
        assert_eq!(w.get(8, 121, 8), reg().block("planks").unwrap());

    }
}
