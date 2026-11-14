//! What distant terrain shows for a column: the surface block the real chunk
//! would have there (by the rules of `surface_rules`), with the changes
//! players made.
//!
//! * The far colour of a natural column is the average colour of its real top
//!   block (`block_colour`), so it matches the block you meet when you walk up.
//! * Edits (the world's overlay of other players' changes, and the top of
//!   every changed chunk that is saved or loaded) are folded in per column:
//!   the column's real top block and height are found by laying the edits over
//!   the terrain function's column. A tile samples one column per cell, so the
//!   edited columns of a cell are averaged into it (`CellAcc`) instead of being
//!   missed when they are not the sampled one.
//! * The cost is bounded: at most `MAX_EDITS` edits per tile are looked at, and
//!   a tile with none costs nothing extra.

use crate::block::{def, reg, Block, Kind, AIR};
use crate::block_colour::{is_foliage, BlockColours};
use crate::chunk::{ChunkData, CH, CW};
use crate::surface_rules::{natural_block, surface_blocks};
use crate::worldgen::{self, Column, SEA};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Edits looked at for one tile, at most.
pub const MAX_EDITS: usize = 6144;
/// A column of a saved chunk's summary that has no top.
pub const NO_TOP: u32 = u32::MAX;

/// A change at a world position: `x, y, z, block`.
pub type Edit = (i32, i32, i32, Block);

/// The real top of a column, as the chunk it is in says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Top {
    pub x: i32,
    pub z: i32,
    pub y: i32,
    pub block: Block,
}

/// What a tile's job is told about changes.
#[derive(Default, Clone)]
pub struct FarInput {
    /// Changes at world positions, later ones win.
    pub edits: Vec<Edit>,
    /// Columns of changed chunks, as they really are.
    pub tops: Vec<Top>,
}

impl FarInput {
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty() && self.tops.is_empty()
    }
}

/// Whether a block is skipped when looking for what a column shows: air, grass
/// tufts and flowers, and leaves.
fn passes(b: Block) -> bool {
    b == AIR || def(b).kind == Kind::Cross || is_foliage(b)
}

/// The block shown at the top of a chunk's column and its height, as a packed
/// number (`block | y << 16`), or `NO_TOP`.
pub fn column_top(data: &ChunkData, lx: i32, lz: i32) -> u32 {
    let base = ChunkData::idx(lx, 0, lz);
    let col = &data.blocks[base..base + CH as usize];
    let mut y = CH - 1;
    while y >= 0 {
        let b = col[y as usize];
        if !passes(b) {
            return b as u32 | ((y as u32) << 16);
        }
        y -= 1;
    }
    NO_TOP
}

/// The tops of all 256 columns of a chunk (index `lx * 16 + lz`): what a
/// distant tile needs to know of a changed chunk that is no longer loaded.
pub fn summarise(data: &ChunkData) -> Box<[u32]> {
    let mut out = alloc::vec![NO_TOP; (CW * CW) as usize];
    for lx in 0..CW {
        for lz in 0..CW {
            out[(lx * CW + lz) as usize] = column_top(data, lx, lz);
        }
    }
    out.into_boxed_slice()
}

/// A chunk's summary as tops at world positions.
pub fn summary_tops(cx: i32, cz: i32, summary: &[u32], out: &mut Vec<Top>) {
    for lx in 0..CW {
        for lz in 0..CW {
            let v = summary.get((lx * CW + lz) as usize).copied().unwrap_or(NO_TOP);
            if v != NO_TOP {
                out.push(Top { x: cx * CW + lx, z: cz * CW + lz, y: (v >> 16) as i32, block: (v & 0xFFFF) as Block });
            }
        }
    }
}

/// Put a chunk's tops into its blocks: the top block in place and nothing
/// above it. (Everything under the top stays as generated.)
pub fn apply_tops(data: &mut ChunkData, cx: i32, cz: i32, tops: &[Top]) {
    for t in tops {
        if t.x >> 4 != cx || t.z >> 4 != cz || !(0..CH).contains(&t.y) {
            continue;
        }
        let base = ChunkData::idx(t.x & 15, 0, t.z & 15);
        data.blocks[base + t.y as usize] = t.block;
        for y in (t.y + 1) as usize..CH as usize {
            data.blocks[base + y] = AIR;
        }
    }
}

/// The colour of a column as the terrain makes it, nothing built or dug.
pub fn natural_colour(pal: &mut BlockColours, seed: u32, x: i32, z: i32, col: &Column) -> [u8; 3] {
    pal.colour(natural_top(seed, x, z, col))
}

/// The block seen from above at a natural column: water over a sea bed, ice on
/// cold seas, the water of a river, else the top block of the rules.
pub fn natural_top(seed: u32, x: i32, z: i32, col: &Column) -> Block {
    let wk = &reg().wk;
    let (top, fill) = surface_blocks(wk, seed, x, z, col);
    let y = col.height.max(SEA);
    natural_block(wk, col, top, fill, y)
}

/// What the column at `(x, z)` really shows from above with these changes
/// laid over the terrain: its top height and block. `ground` is the top the
/// column's chunk reports (if known), `edits` the changes at this column.
fn real_top(seed: u32, x: i32, z: i32, ground: Option<(i32, Block)>, edits: &[(i32, Block)]) -> Option<(i32, Block)> {
    let wk = &reg().wk;
    let col = worldgen::column(seed, x, z);
    let (top, fill) = surface_blocks(wk, seed, x, z, &col);
    let mut hi = col.height.max(SEA);
    for e in edits {
        hi = hi.max(e.0);
    }
    if let Some((gy, _)) = ground {
        hi = hi.max(gy);
    }
    let mut y = hi.min(CH - 1);
    while y >= 0 {
        let b = if let Some(e) = edits.iter().rev().find(|e| e.0 == y) {
            e.1
        } else if let Some((gy, gb)) = ground {
            if y > gy {
                AIR
            } else if y == gy {
                gb
            } else {
                natural_block(wk, &col, top, fill, y)
            }
        } else {
            natural_block(wk, &col, top, fill, y)
        };
        if !passes(b) {
            return Some((y, b));
        }
        y -= 1;
    }
    None
}

/// The columns of a tile that people changed, with what each really shows.
pub struct EditedColumns {
    /// World `(x, z)` to the top block's height and block.
    pub cols: BTreeMap<(i32, i32), (i32, Block)>,
}

impl EditedColumns {
    pub fn new() -> EditedColumns {
        EditedColumns { cols: BTreeMap::new() }
    }

    /// Work out the real tops of every column that `input` touches (at most
    /// `MAX_EDITS` edits and tops are looked at).
    pub fn build(seed: u32, input: &FarInput) -> EditedColumns {
        let mut out = EditedColumns::new();
        if input.is_empty() || worldgen::shape() >= 2 {
            return out;
        }
        let mut grounds: BTreeMap<(i32, i32), (i32, Block)> = BTreeMap::new();
        for t in input.tops.iter().take(MAX_EDITS) {
            grounds.insert((t.x, t.z), (t.y, t.block));
        }
        let mut edits: BTreeMap<(i32, i32), Vec<(i32, Block)>> = BTreeMap::new();
        for &(x, y, z, b) in input.edits.iter().take(MAX_EDITS) {
            if (0..CH).contains(&y) {
                edits.entry((x, z)).or_default().push((y, b));
            }
        }
        let mut keys: Vec<(i32, i32)> = grounds.keys().copied().collect();
        keys.extend(edits.keys().copied().filter(|k| !grounds.contains_key(k)));
        for k in keys {
            let list: &[(i32, Block)] = edits.get(&k).map_or(&[], |v| v.as_slice());
            if let Some(t) = real_top(seed, k.0, k.1, grounds.get(&k).copied(), list) {
                out.cols.insert(k, t);
            }
        }
        out
    }

    /// Fold the edited columns of the tile at `(tx, tz)` into cells of `step`
    /// blocks, keyed by cell `(i, j)`.
    pub fn cells(&self, tx: i32, tz: i32, step: i32, pal: &mut BlockColours) -> BTreeMap<(usize, usize), CellAcc> {
        let mut out: BTreeMap<(usize, usize), CellAcc> = BTreeMap::new();
        let step = step.max(1);
        let (ox, oz) = (tx * crate::lod::TILE, tz * crate::lod::TILE);
        for (&(x, z), &(y, b)) in &self.cols {
            let (lx, lz) = (x - ox, z - oz);
            if !(0..crate::lod::TILE).contains(&lx) || !(0..crate::lod::TILE).contains(&lz) {
                continue;
            }
            let c = pal.colour(b);
            let acc = out.entry(((lx / step) as usize, (lz / step) as usize)).or_default();
            acc.n += 1;
            for k in 0..3 {
                acc.rgb[k] += c[k] as u32;
            }
            acc.top_sum += (y + 1) as i64;
        }
        out
    }
}

impl Default for EditedColumns {
    fn default() -> Self {
        EditedColumns::new()
    }
}

/// The edited columns that fall in one cell of a tile.
#[derive(Default, Clone, Debug)]
pub struct CellAcc {
    pub n: u32,
    pub rgb: [u32; 3],
    /// Sum of (top height + 1) of those columns.
    pub top_sum: i64,
}

impl CellAcc {
    /// The cell's colour: its edited columns and, for the rest of the
    /// `step` x `step` columns, `centre` (what the sampled column shows).
    pub fn blend(&self, step: i32, centre: [u8; 3]) -> [u8; 3] {
        let total = (step.max(1) * step.max(1)) as u64;
        let k = (self.n as u64).min(total).max(1);
        let rest = total - k;
        let mut out = [0u8; 3];
        for c in 0..3 {
            let edited = (self.rgb[c] as u64 * k) / (self.n as u64).max(1);
            out[c] = ((edited + rest * centre[c] as u64) / total).min(255) as u8;
        }
        out
    }

    /// The height of the cell's top when the changed columns are most of it:
    /// their average. Otherwise `None` (the sampled column's height stands).
    pub fn raised(&self, step: i32) -> Option<i32> {
        let total = (step.max(1) * step.max(1)) as u32;
        if self.n == 0 || self.n.saturating_mul(2) < total {
            return None;
        }
        let avg = (self.top_sum + (self.n / 2) as i64) / self.n as i64;
        Some(avg.clamp(0, CH as i64 + 1) as i32)
    }
}

/// The colour distant terrain gives the column `(x, z)` with these changes
/// (a column of its own: for `/blockat` and the tests).
pub fn column_colour(seed: u32, x: i32, z: i32, input: &FarInput) -> [u8; 3] {
    if worldgen::shape() >= 2 {
        return crate::block_colour::GREY;
    }
    let mut pal = BlockColours::new();
    let ed = EditedColumns::build(seed, input);
    match ed.cols.get(&(x, z)) {
        Some(&(_, b)) => pal.colour(b),
        None => natural_colour(&mut pal, seed, x, z, &worldgen::column(seed, x, z)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_colour::of_block;

    fn close(a: [u8; 3], b: [u8; 3], tol: i32) -> bool {
        (0..3).map(|k| (a[k] as i32 - b[k] as i32).abs()).sum::<i32>() <= tol
    }

    #[test]
    fn far_colours_are_the_average_of_the_real_surface_block() {
        let (mut same, mut total) = (0, 0);
        for seed in [3u32, 11, 29, 42, 77] {
            let mut pal = BlockColours::new();
            for n in 0..14 {
                let x = crate::noise::hash2(seed ^ 0x51, n, 1) as i32 % 3000;
                let z = crate::noise::hash2(seed ^ 0x52, n, 2) as i32 % 3000;
                let (cx, cz) = (x >> 4, z >> 4);
                let data = worldgen::generate(seed, cx, cz);
                for k in 0..6 {
                    let (lx, lz) = ((k * 5 + n) % 16, (k * 7 + 3 * n) % 16);
                    let (wx, wz) = (cx * 16 + lx, cz * 16 + lz);
                    let col = worldgen::column(seed, wx, wz);
                    let t = column_top(&data, lx, lz);
                    if t == NO_TOP {
                        continue;
                    }
                    let (ty, tb) = ((t >> 16) as i32, (t & 0xFFFF) as Block);
                    // Anything standing on the ground (trunks, rocks, builds) is left out of the comparison.
                    if ty > col.height.max(SEA) || col.river || col.building > 0 {
                        continue;
                    }
                    total += 1;
                    if close(natural_colour(&mut pal, seed, wx, wz, &col), of_block(tb), 12) {
                        same += 1;
                    }
                }
            }
        }
        assert!(total >= 200, "only {total} columns compared");
        // (A cave mouth or a pond can open a column differently.)
        assert!(same * 100 >= total * 88, "{same} of {total} far colours match the real surface");
    }

    #[test]
    fn an_edit_changes_the_far_colour_of_its_column() {
        let r = reg();
        let clay = r.block("red_terracotta").or_else(|| r.block("brick")).unwrap_or(r.wk.stone);
        let seed = 7;
        // A column whose natural colour is not the clay's.
        let (mut x, z) = (202, -122);
        let mut pal = BlockColours::new();
        while close(natural_colour(&mut pal, seed, x, z, &worldgen::column(seed, x, z)), of_block(clay), 12) {
            x += 1;
        }
        let h = worldgen::column(seed, x, z).height.max(SEA);
        let mut input = FarInput::default();
        assert!(!close(column_colour(seed, x, z, &input), of_block(clay), 12));
        input.edits.push((x, h + 3, z, clay));
        assert!(close(column_colour(seed, x, z, &input), of_block(clay), 0));
        // A whole cell of clay colours the cell and raises it.
        let (tx, tz) = (x.div_euclid(crate::lod::TILE), z.div_euclid(crate::lod::TILE));
        let step = 4;
        let (cx0, cz0) = (x.div_euclid(step) * step, z.div_euclid(step) * step);
        let mut cell = FarInput::default();
        for dx in 0..step {
            for dz in 0..step {
                let g = worldgen::column(seed, cx0 + dx, cz0 + dz).height.max(SEA);
                cell.edits.push((cx0 + dx, g + 6, cz0 + dz, clay));
            }
        }
        let ed = EditedColumns::build(seed, &cell);
        let cells = ed.cells(tx, tz, step, &mut pal);
        let acc = cells.get(&(((cx0 - tx * 64) / step) as usize, ((cz0 - tz * 64) / step) as usize)).expect("the cell is edited");
        assert_eq!(acc.n, 16);
        assert!(close(acc.blend(step, [0, 0, 0]), of_block(clay), 3));
        assert!(acc.raised(step).unwrap() >= SEA + 7);
        // One column in sixteen does not take the cell over.
        let few = EditedColumns::build(seed, &FarInput { edits: cell.edits[..1].to_vec(), tops: Vec::new() });
        let acc = few.cells(tx, tz, step, &mut pal).into_values().next().unwrap();
        assert!(acc.raised(step).is_none());
        assert!(close(acc.blend(step, of_block(clay)), of_block(clay), 3));
    }

    #[test]
    fn summaries_round_trip_and_patch_chunks() {
        let seed = 5;
        let mut data = worldgen::generate(seed, 2, -3);
        let stone = reg().wk.stone;
        let h = worldgen::column(seed, 2 * 16 + 4, -3 * 16 + 9).height;
        data.set(4, h + 10, 9, stone);
        let s = summarise(&data);
        let mut tops = Vec::new();
        summary_tops(2, -3, &s, &mut tops);
        let mine = tops.iter().find(|t| (t.x, t.z) == (2 * 16 + 4, -3 * 16 + 9)).unwrap();
        assert_eq!((mine.y, mine.block), (h + 10, stone));
        // Putting the tops into a fresh chunk makes that column match.
        let mut fresh = worldgen::generate(seed, 2, -3);
        apply_tops(&mut fresh, 2, -3, &tops);
        assert_eq!(fresh.get(4, h + 10, 9), stone);
        assert_eq!(column_top(&fresh, 4, 9), column_top(&data, 4, 9));
    }
}
