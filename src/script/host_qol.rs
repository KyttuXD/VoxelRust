//! The host calls added for the quality-of-life mods: where the player
//! faces, the biome, light and drops of a block, a line on the screen, the
//! pack's slots and putting items into a chest. `SessionHost` (host.rs)
//! calls these; each is small and refuses what it cannot do (returns
//! `false`, `0` or an empty string).

use crate::block::{self, reg, Usable};
use crate::inventory::{stack_limit, Stack, HOTBAR, SLOTS};
use crate::math::floord;
use crate::session::{GameMode, Session};
use alloc::string::String;

/// The compass heading the player looks along, in degrees clockwise from
/// north (-z): 0 north, 90 east (+x), 180 south, 270 west.
pub fn heading(g: &Session) -> f64 {
    let d = -(g.player.yaw as f64) * 180.0 / core::f64::consts::PI;
    if !d.is_finite() {
        return 0.0;
    }
    d - (floord(d / 360.0) as f64) * 360.0
}

/// The biome of the column the player stands in, lower case (`plains`, `desert`...).
pub fn biome(g: &Session) -> String {
    let c = crate::worldgen::column(g.world.seed, floord(g.player.pos[0]), floord(g.player.pos[2]));
    alloc::format!("{:?}", c.biome).to_ascii_lowercase()
}

/// How much light a block gives off (0 to 15; 0 for an id nobody knows).
pub fn block_light(id: &str) -> i64 {
    reg().block(id).map_or(0, |b| block::def(b).emit as i64)
}

/// The id of the item a block gives when broken in survival ("" if none).
pub fn block_drop(id: &str) -> String {
    let r = reg();
    r.block(id).and_then(|b| block::def(b).drop).and_then(|i| r.items.get(i)).map(|it| it.id.clone()).unwrap_or_default()
}

pub fn survival(g: &Session) -> bool {
    g.mode == GameMode::Survival
}

pub fn sneaking(g: &Session) -> bool {
    g.player.crouch > 0.5
}

/// The item in pack slot `i` (0 to 35; the first nine are the hotbar), "" if empty.
pub fn slot_item(g: &Session, i: i64) -> String {
    match usize::try_from(i).ok().and_then(|i| g.inv.slots.get(i).copied().flatten()) {
        Some(s) => reg().items.get(s.item as usize).map(|it| it.id.clone()).unwrap_or_default(),
        None => String::new(),
    }
}

pub fn slot_count(g: &Session, i: i64) -> i64 {
    usize::try_from(i).ok().and_then(|i| g.inv.slots.get(i).copied().flatten()).map_or(0, |s| s.count as i64)
}

/// Move what is in slot `from` to slot `to`: into an empty slot, onto a
/// stack of the same item (as much as fits; the rest stays), else the two
/// swap places. `false` when nothing moved.
pub fn move_slot(g: &mut Session, from: i64, to: i64) -> bool {
    let (Ok(a), Ok(b)) = (usize::try_from(from), usize::try_from(to)) else { return false };
    if a >= SLOTS || b >= SLOTS || a == b {
        return false;
    }
    let Some(src) = g.inv.slots[a] else { return false };
    match g.inv.slots[b] {
        None => {
            g.inv.slots[b] = Some(src);
            g.inv.slots[a] = None;
        }
        Some(dst) if dst.item == src.item => {
            let room = stack_limit(dst.item).saturating_sub(dst.count);
            let n = room.min(src.count);
            if n == 0 {
                return false;
            }
            g.inv.slots[b] = Some(Stack { item: dst.item, count: dst.count + n });
            g.inv.slots[a] = if src.count == n { None } else { Some(Stack { item: src.item, count: src.count - n }) };
        }
        Some(_) => g.inv.slots.swap(a, b),
    }
    true
}

/// Put the items of the pack (not the hotbar) that the chest at the cell
/// already holds into it. Returns how many items moved; 0 if there is no
/// chest there, it is a shared game, or nothing matched.
pub fn stack_to_chest(g: &mut Session, x: i64, y: i64, z: i64) -> i64 {
    const EDGE: i64 = 30_000_000;
    if g.net.is_some() || x.abs() > EDGE || z.abs() > EDGE || !(0..crate::chunk::CH as i64).contains(&y) {
        return 0;
    }
    let cell = [x as i32, y as i32, z as i32];
    if block::def(g.world.get(cell[0], cell[1], cell[2])).usable != Usable::Chest {
        return 0;
    }
    let Some(chest) = g.chests.get_mut(&cell) else { return 0 };
    let mut moved = 0i64;
    for i in HOTBAR..SLOTS {
        let Some(s) = g.inv.slots[i] else { continue };
        if chest.count(s.item) == 0 {
            continue;
        }
        let left = chest.add(s.item, s.count);
        moved += (s.count - left) as i64;
        g.inv.slots[i] = if left == 0 { None } else { Some(Stack { item: s.item, count: left }) };
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_ids_are_harmless() {
        assert_eq!(block_light("no_such_block"), 0);
        assert_eq!(block_drop("no_such_block"), "");
    }
}
