//! Distant terrain. Beyond the fully detailed chunks the world is drawn as
//! coarse height-field tiles made straight from the terrain function, with
//! no block data at all. A tile covers 64 x 64 blocks in a few dozen quads,
//! so the view can reach kilometres at a small cost.

use crate::block::reg;
use crate::mesh::{MeshData, Vertex};
use crate::worldgen::{self, Biome, SEA};
use alloc::vec::Vec;

/// Tile edge in blocks (four chunks).
pub const TILE: i32 = 64;
/// Chunks per tile edge.
pub const TILE_CHUNKS: i32 = 4;

pub fn colour(height: i32, biome: Biome) -> [u8; 3] {
    if height < SEA {
        return [44, 104, 198];
    }
    if worldgen::source().is_none() && height <= SEA + 1 && biome != Biome::Tundra && biome != Biome::Taiga {
        return [220, 204, 152];
    }
    match biome {
        Biome::Plains => [74, 150, 70],
        // Forest floor is earthier than the crowns on it, so the ground reads.
        Biome::Forest => [86, 104, 56],
        Biome::Desert => [220, 204, 152],
        Biome::Tundra => [236, 241, 248],
        Biome::Taiga => [150, 176, 160],
        Biome::Swamp => [70, 112, 70],
        Biome::Savanna => [150, 160, 76],
        Biome::Jungle => [40, 130, 52],
        Biome::Badlands => [196, 110, 66],
        Biome::Mountain => {
            if height > SEA + 54 {
                [236, 241, 248]
            } else {
                [126, 130, 138]
            }
        }
    }
}

/// Leaf colour of the trees that grow in a biome, or `None` for open country.
fn tree_colour(biome: Biome, roll: u32) -> Option<[u8; 3]> {
    Some(match biome {
        Biome::Forest => match roll % 10 {
            0..=2 => [96, 160, 70],   // birch
            3..=4 => [204, 96, 40],   // maple
            _ => [48, 124, 54],       // oak
        },
        Biome::Taiga => [30, 92, 66],
        Biome::Jungle => [58, 140, 52],
        Biome::Swamp => [58, 108, 58],
        Biome::Savanna if roll % 5 == 0 => [86, 132, 52],
        Biome::Plains if roll % 60 == 0 => [240, 150, 190],
        _ => return None,
    })
}

/// Build the mesh of tile (tx, tz), sampling the terrain every `step`
/// blocks (4, 8 or 16).
///
/// The tile is drawn the way the world is built: as flat-topped columns
/// with vertical sides, not as a smooth sheet, so distant land keeps the
/// same blocky look as the land nearby. Wooded country gets a blocky tree
/// on each column that would have trees.
pub fn build(seed: u32, tx: i32, tz: i32, step: i32, trees_setting: u8) -> MeshData {
    let step = step.clamp(2, 32);
    // Empty space and sky islands have no far-off ground to draw.
    if worldgen::shape() >= 2 {
        return MeshData::default();
    }
    let n = (TILE / step) as usize;
    let (ox, oz) = (tx * TILE, tz * TILE);
    // One sample per cell, taken at its middle.
    let mut h = alloc::vec![0i32; n * n];
    let mut c = alloc::vec![[0u8; 3]; n * n];
    let mut trees: Vec<(usize, usize, [u8; 3])> = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let (x, z) = (ox + i as i32 * step + step / 2, oz + j as i32 * step + step / 2);
            let col = worldgen::column(seed, x, z);
            let dry = col.height >= SEA && !col.river;
            // Water is drawn as a flat sheet at sea level.
            h[i * n + j] = col.height.max(SEA) + 1 + col.building as i32;
            c[i * n + j] = if col.river {
                [44, 104, 198]
            } else if col.building > 0 {
                [170, 168, 164]
            } else if col.road && col.height >= SEA {
                [86, 88, 94]
            } else if col.farm && col.height >= SEA {
                // Fields in a patchwork of ripe and green.
                match crate::noise::hash2(seed ^ 0xFA53, x >> 4, z >> 4) % 7 {
                    0 | 2 => [206, 184, 96],
                    5 | 6 => [112, 168, 72],
                    _ => [128, 150, 70],
                }
            } else if col.paved && col.height >= SEA {
                [128, 124, 122]
            } else {
                colour(col.height, col.biome)
            };
            // (Only the world's own terrain has woods: flat land and the other
            // dimensions are drawn without far-off trees.)
            if trees_setting > 0 && dry && step <= 8 && !col.paved && col.building == 0 && !col.farm && worldgen::shape() == 0 {
                let roll = crate::noise::hash2(seed ^ 0x7EE5, x, z);
                // Denser cover in forest and jungle than at their edges.
                let chance = match col.biome {
                    Biome::Forest | Biome::Jungle => 70,
                    Biome::Taiga | Biome::Swamp => 50,
                    _ => 100,
                };
                if roll % 100 < chance && (trees_setting >= 2 || roll % 2 == 0) {
                    if let Some(leaf) = tree_colour(col.biome, roll >> 8) {
                        trees.push((i, j, leaf));
                    }
                }
            }
        }
    }
    // Plain white texel, full sky light: colour comes from the vertices and
    // brightness from the time of day, like everything else.
    let r = reg();
    let white = r.tiles.iter().position(|t| t.id == "white").unwrap_or(0) as i32;
    let uv = [((white % 16) * 256 + 128) as i16, ((white / 16) * 256 + 128) as i16];
    let luv = [8i16, 15 * 16 + 8];
    let mut out = MeshData { opaque: Vec::with_capacity(n * n * 12), water: Vec::new(), glass: Vec::new(), ymin: i32::MAX, ymax: 0, props: Vec::new() };
    // A vertex at block offset (x, z) within the tile, in 1/16ths.
    let vert = |x: i32, y: i32, z: i32, col: [u8; 3], shade: i32| Vertex {
        pos: [x as i16, (y * 16) as i16, z as i16, 0],
        col: [
            (col[0] as i32 * shade / 256).clamp(0, 255) as u8,
            (col[1] as i32 * shade / 256).clamp(0, 255) as u8,
            (col[2] as i32 * shade / 256).clamp(0, 255) as u8,
            255,
        ],
        uv,
        luv,
    };
    // An upright box from y0 to y1 over the square (x0, z0)..(x1, z1), in
    // 1/16ths: its top, and each side down to `floor(side)`.
    let pillar = |x0: i32, z0: i32, x1: i32, z1: i32, y1: i32, col: [u8; 3], sides: [i32; 4], out: &mut MeshData| {
        out.opaque.push(vert(x0, y1, z1, col, 256));
        out.opaque.push(vert(x1, y1, z1, col, 256));
        out.opaque.push(vert(x1, y1, z0, col, 256));
        out.opaque.push(vert(x0, y1, z0, col, 256));
        // Sides in the order -x, +x, -z, +z, with the same shading as blocks.
        let faces = [
            ([x0, z0, x0, z1], 160),
            ([x1, z1, x1, z0], 160),
            ([x1, z0, x0, z0], 205),
            ([x0, z1, x1, z1], 205),
        ];
        for (k, (e, shade)) in faces.iter().enumerate() {
            let y0 = sides[k];
            if y0 < y1 {
                out.opaque.push(vert(e[0], y0, e[1], col, *shade));
                out.opaque.push(vert(e[2], y0, e[3], col, *shade));
                out.opaque.push(vert(e[2], y1, e[3], col, *shade));
                out.opaque.push(vert(e[0], y1, e[1], col, *shade));
                out.ymin = out.ymin.min(y0);
            }
        }
        out.ymax = out.ymax.max(y1);
        out.ymin = out.ymin.min(y1);
    };
    let s16 = step * 16;
    // At the tile's edge a curtain hangs down, hiding cracks against the
    // neighbouring tile (which may be sampled more coarsely).
    let drop = 12;
    for i in 0..n {
        for j in 0..n {
            let here = h[i * n + j];
            let side = |di: i32, dj: i32| -> i32 {
                let (a, b) = (i as i32 + di, j as i32 + dj);
                if a < 0 || b < 0 || a >= n as i32 || b >= n as i32 {
                    here - drop
                } else {
                    h[a as usize * n + b as usize]
                }
            };
            let (x0, z0) = (i as i32 * s16, j as i32 * s16);
            pillar(x0, z0, x0 + s16, z0 + s16, here, c[i * n + j], [side(-1, 0), side(1, 0), side(0, -1), side(0, 1)], &mut out);
        }
    }
    for (i, j, leaf) in trees {
        // A crown on a short gap of trunk, sized to the sampling step.
        let ground = h[i * n + j];
        let (cx, cz) = (i as i32 * s16 + s16 / 2, j as i32 * s16 + s16 / 2);
        let half = (s16 * 3 / 8).min(40);
        let top = ground + 4 + (leaf[0] as i32 + i as i32) % 3;
        let base = ground + 2;
        // A trunk up to the crown, so the tree stands on the ground.
        let trunk = (s16 / 12).clamp(2, 6);
        pillar(cx - trunk, cz - trunk, cx + trunk, cz + trunk, base, [96, 70, 44], [ground; 4], &mut out);
        pillar(cx - half, cz - half, cx + half, cz + half, top, leaf, [base; 4], &mut out);
    }
    if out.ymin > out.ymax {
        out.ymin = 0;
    }
    out
}

/// How coarsely to sample a tile `dist` tiles away when the finest sampling
/// is the one `detail` (0 coarse, 1 medium, 2 fine) gives.
pub fn step_with(dist: i32, detail: i32) -> i32 {
    let fine = [8, 4, 2][detail.clamp(0, 2) as usize];
    let tier = if dist <= 6 { 0 } else if dist <= 16 { 1 } else { 2 };
    (fine << tier).clamp(2, 32)
}

/// How coarsely to sample a tile `dist` tiles away.
pub fn step_for(dist: i32) -> i32 {
    if dist <= 6 {
        4
    } else if dist <= 16 {
        8
    } else {
        16
    }
}

/// The colour a block shows from a distance: the average of its top picture's
/// solid pixels. `None` for air and for blocks that are not seen as ground.
struct Palette {
    cols: Vec<Option<[u8; 3]>>,
}

impl Palette {
    fn new() -> Palette {
        Palette { cols: alloc::vec![None; reg().blocks.len()] }
    }

    fn colour(&mut self, b: crate::block::Block) -> [u8; 3] {
        if let Some(Some(c)) = self.cols.get(b as usize) {
            return *c;
        }
        let r = reg();
        let d = &r.blocks[b as usize];
        let col = if d.kind == crate::block::Kind::Liquid {
            if d.x.fluid.map_or(false, |f| f.0) { [226, 98, 24] } else { [44, 104, 198] }
        } else {
            let tile = d.tex[0] as usize;
            match r.tiles.get(tile) {
                Some(t) => {
                    let px = crate::textures::described_pixels(tile, t);
                    let (mut sum, mut n) = ([0u32; 3], 0u32);
                    for p in px.chunks(4) {
                        if p[3] > 128 {
                            sum = [sum[0] + p[0] as u32, sum[1] + p[1] as u32, sum[2] + p[2] as u32];
                            n += 1;
                        }
                    }
                    if n == 0 { [128, 128, 128] } else { [(sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8] }
                }
                None => [128, 128, 128],
            }
        };
        if let Some(slot) = self.cols.get_mut(b as usize) {
            *slot = Some(col);
        }
        col
    }
}

fn is_foliage(b: crate::block::Block) -> bool {
    let d = crate::block::def(b);
    d.kind == crate::block::Kind::Cutout && (d.id.contains("leaves") || d.id.contains("leaf") || d.id.contains("needle"))
}

fn close(a: [u8; 3], b: [u8; 3]) -> bool {
    (a[0] as i32 - b[0] as i32).abs() + (a[1] as i32 - b[1] as i32).abs() + (a[2] as i32 - b[2] as i32).abs() < 54
}

/// What a distant column looks like, from its real blocks.
struct Cell {
    /// Top of the ground (in blocks), and its colour.
    ground: i32,
    gcol: [u8; 3],
    /// Something standing on the ground: a stem or walls from `ground` up to
    /// `crown_from`, and above that a crown or roof up to `top`.
    stem: Option<[u8; 3]>,
    crown_from: i32,
    crown: Option<([u8; 3], bool)>,
    top: i32,
}

/// Like [`build`], but made from the world's real blocks: every chunk of the
/// tile is generated (and the player's `edits` put back), so the builds,
/// roofs and trees seen from afar are the ones standing there.
pub fn build_real(seed: u32, tx: i32, tz: i32, step: i32, trees_setting: u8, edits: &[Vec<(i32, i32, i32, crate::block::Block)>]) -> MeshData {
    use crate::chunk::{ChunkData, CH};
    let step = step.clamp(2, 32);
    if worldgen::shape() >= 2 {
        return MeshData::default();
    }
    // The tile's chunks, with what players changed in them.
    let mut chunks: Vec<ChunkData> = Vec::with_capacity(16);
    for i in 0..TILE_CHUNKS {
        for j in 0..TILE_CHUNKS {
            let mut c = worldgen::generate(seed, tx * TILE_CHUNKS + i, tz * TILE_CHUNKS + j);
            if let Some(list) = edits.get((i * TILE_CHUNKS + j) as usize) {
                for &(x, y, z, b) in list {
                    if (0..CH).contains(&y) {
                        c.blocks[ChunkData::idx(x.rem_euclid(16), y, z.rem_euclid(16))] = b;
                    }
                }
            }
            chunks.push(c);
        }
    }
    let n = (TILE / step) as usize;
    let (ox, oz) = (tx * TILE, tz * TILE);
    let mut pal = Palette::new();
    let mut cells: Vec<Cell> = Vec::with_capacity(n * n);
    for i in 0..n {
        for j in 0..n {
            let (x, z) = (ox + i as i32 * step + step / 2, oz + j as i32 * step + step / 2);
            let (lx, lz) = (x - ox, z - oz);
            let chunk = &chunks[((lx / 16) * TILE_CHUNKS + lz / 16) as usize];
            let base = ChunkData::idx(lx % 16, 0, lz % 16);
            let col = &chunk.blocks[base..base + CH as usize];
            let info = worldgen::column(seed, x, z);
            let ground_y = info.height.max(SEA);
            // Which foliage is left out (trees setting), column by column.
            let keep_trees = trees_setting >= 2 || (trees_setting == 1 && crate::noise::hash2(seed ^ 0x7EE5, x, z) % 2 == 0);
            let skip = |b: crate::block::Block| -> bool {
                b == crate::block::AIR || crate::block::def(b).kind == crate::block::Kind::Cross || (!keep_trees && is_foliage(b))
            };
            // The topmost thing there.
            let mut y = CH - 1;
            while y >= 0 && skip(col[y as usize]) {
                y -= 1;
            }
            if y < 0 {
                cells.push(Cell { ground: ground_y + 1, gcol: colour(info.height, info.biome), stem: None, crown_from: 0, crown: None, top: ground_y + 1 });
                continue;
            }
            let b = col[y as usize];
            let top_col = pal.colour(b);
            // Water, or ground at or below the natural surface: one flat top.
            if crate::block::def(b).kind == crate::block::Kind::Liquid || y <= ground_y {
                let surface = if crate::block::def(b).kind == crate::block::Kind::Liquid { y.max(SEA) + 1 } else { y + 1 };
                cells.push(Cell { ground: surface, gcol: top_col, stem: None, crown_from: 0, crown: None, top: surface });
                continue;
            }
            // Something stands above the ground: a crown or roof of one
            // material on top, and what holds it up of another.
            let mut r = y;
            while r > ground_y + 1 && !skip(col[(r - 1) as usize]) && close(pal.colour(col[(r - 1) as usize]), top_col) {
                r -= 1;
            }
            let gb = col[ground_y as usize];
            let gcol = if gb == crate::block::AIR { colour(info.height, info.biome) } else { pal.colour(gb) };
            let stem = if r > ground_y + 1 && !skip(col[(r - 1) as usize]) { Some(pal.colour(col[(r - 1) as usize])) } else { None };
            let foliage = is_foliage(b);
            if r <= ground_y + 1 {
                // The material goes all the way down: walls or a mound.
                cells.push(Cell { ground: ground_y + 1, gcol, stem: Some(top_col), crown_from: y + 1, crown: None, top: y + 1 });
            } else {
                cells.push(Cell { ground: ground_y + 1, gcol, stem, crown_from: r, crown: Some((top_col, foliage)), top: y + 1 });
            }
        }
    }
    // Quads, as in `build`: boxes from the ground up.
    let r = reg();
    let white = r.tiles.iter().position(|t| t.id == "white").unwrap_or(0) as i32;
    let uv = [((white % 16) * 256 + 128) as i16, ((white / 16) * 256 + 128) as i16];
    let luv = [8i16, 15 * 16 + 8];
    let mut out = MeshData { opaque: Vec::with_capacity(n * n * 16), water: Vec::new(), glass: Vec::new(), ymin: i32::MAX, ymax: 0, props: Vec::new() };
    let vert = |x: i32, y: i32, z: i32, col: [u8; 3], shade: i32| Vertex {
        pos: [x as i16, (y * 16) as i16, z as i16, 0],
        col: [
            (col[0] as i32 * shade / 256).clamp(0, 255) as u8,
            (col[1] as i32 * shade / 256).clamp(0, 255) as u8,
            (col[2] as i32 * shade / 256).clamp(0, 255) as u8,
            255,
        ],
        uv,
        luv,
    };
    let pillar = |x0: i32, z0: i32, x1: i32, z1: i32, y1: i32, col: [u8; 3], sides: [i32; 4], out: &mut MeshData| {
        out.opaque.push(vert(x0, y1, z1, col, 256));
        out.opaque.push(vert(x1, y1, z1, col, 256));
        out.opaque.push(vert(x1, y1, z0, col, 256));
        out.opaque.push(vert(x0, y1, z0, col, 256));
        let faces = [([x0, z0, x0, z1], 160), ([x1, z1, x1, z0], 160), ([x1, z0, x0, z0], 205), ([x0, z1, x1, z1], 205)];
        for (k, (e, shade)) in faces.iter().enumerate() {
            let y0 = sides[k];
            if y0 < y1 {
                out.opaque.push(vert(e[0], y0, e[1], col, *shade));
                out.opaque.push(vert(e[2], y0, e[3], col, *shade));
                out.opaque.push(vert(e[2], y1, e[3], col, *shade));
                out.opaque.push(vert(e[0], y1, e[1], col, *shade));
                out.ymin = out.ymin.min(y0);
            }
        }
        out.ymax = out.ymax.max(y1);
        out.ymin = out.ymin.min(y1);
    };
    let s16 = step * 16;
    let drop = 12;
    for i in 0..n {
        for j in 0..n {
            let cell = &cells[i * n + j];
            let side = |di: i32, dj: i32| -> i32 {
                let (a, b) = (i as i32 + di, j as i32 + dj);
                if a < 0 || b < 0 || a >= n as i32 || b >= n as i32 {
                    cell.ground - drop
                } else {
                    cells[a as usize * n + b as usize].ground
                }
            };
            let (x0, z0) = (i as i32 * s16, j as i32 * s16);
            pillar(x0, z0, x0 + s16, z0 + s16, cell.ground, cell.gcol, [side(-1, 0), side(1, 0), side(0, -1), side(0, 1)], &mut out);
            let (cx, cz) = (x0 + s16 / 2, z0 + s16 / 2);
            match (cell.stem, cell.crown) {
                // Walls, or a mound: the whole cell, up to the top.
                (Some(col), None) => pillar(x0, z0, x0 + s16, z0 + s16, cell.top, col, [cell.ground; 4], &mut out),
                (stem, Some((col, foliage))) => {
                    if let Some(sc) = stem {
                        // A tree's trunk is thin; walls under a roof fill the cell.
                        let w = if foliage { (s16 / 8).clamp(4, 24) } else { s16 / 2 };
                        pillar(cx - w, cz - w, cx + w, cz + w, cell.crown_from, sc, [cell.ground; 4], &mut out);
                    }
                    let inset = if foliage { s16 / 10 } else { 0 };
                    pillar(x0 + inset, z0 + inset, x0 + s16 - inset, z0 + s16 - inset, cell.top, col, [cell.crown_from; 4], &mut out);
                }
                _ => {}
            }
        }
    }
    if out.ymin > out.ymax {
        out.ymin = 0;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_are_blocky_and_get_cheaper_with_distance() {
        let fine = build(7, 3, -2, 4, 2);
        let coarse = build(7, 3, -2, 16, 2);
        assert!(fine.opaque.len() % 4 == 0 && coarse.opaque.len() % 4 == 0);
        // At least a top per cell, and far fewer quads when sampled coarsely.
        assert!(fine.opaque.len() >= 16 * 16 * 4);
        assert!(coarse.opaque.len() >= 4 * 4 * 4 && coarse.opaque.len() * 6 < fine.opaque.len());
        // Every quad is flat or upright: all four corners share x, y or z.
        for q in fine.opaque.chunks(4) {
            let same = |k: usize| q.iter().all(|v| v.pos[k] == q[0].pos[k]);
            assert!(same(0) || same(1) || same(2), "a slanted quad: {q:?}");
        }
        // The first cell's top is at the terrain height there (never below the sea).
        let want = worldgen::column(7, 3 * TILE + 2, -2 * TILE + 2).height.max(SEA) + 1;
        assert!(fine.opaque[..4].iter().all(|v| v.pos[1] as i32 == want * 16));
        assert!(fine.ymin < fine.ymax);
        assert_eq!((step_for(1), step_for(11), step_for(40)), (4, 8, 16));
    }

    #[test]
    fn forests_show_trees_in_the_distance() {
        // Find a wooded tile and an open one; the wooded one has more boxes
        // than its cells alone account for, standing above the ground.
        let mut found = false;
        for t in 0..60 {
            let (tx, tz) = (t * 3, t * 5 - 40);
            let col = worldgen::column(7, tx * TILE + 32, tz * TILE + 32);
            if col.biome == Biome::Forest && col.height > SEA + 2 {
                let m = build(7, tx, tz, 4, 2);
                // More top faces than cells: the extra ones are tree crowns.
                let tops = m.opaque.chunks(4).filter(|q| q.iter().all(|v| v.pos[1] == q[0].pos[1])).count();
                assert!(tops > 16 * 16, "no trees on a forest tile");
                found = true;
                break;
            }
        }
        assert!(found, "no forest tile found to test");
    }

    #[test]
    fn real_tiles_show_what_players_built() {
        let (tx, tz) = (3, -2);
        let plain = build_real(7, tx, tz, 4, 2, &[]);
        assert!(plain.opaque.len() % 4 == 0 && plain.opaque.len() >= 16 * 16 * 4);
        // A tower of stone, forty blocks high, in the cell at (202, -122).
        let ground = worldgen::column(7, 202, -122).height.max(SEA);
        let stone = crate::block::reg().wk.stone;
        let mut edits: Vec<Vec<(i32, i32, i32, crate::block::Block)>> = alloc::vec![Vec::new(); 16];
        edits[1] = (ground + 1..ground + 41).map(|y| (202, y, -122, stone)).collect();
        let with = build_real(7, tx, tz, 4, 2, &edits);
        assert!(with.ymax >= ground + 40 && plain.ymax < ground + 38, "{} {} {}", with.ymax, plain.ymax, ground);
        // Every quad is flat or upright.
        for q in with.opaque.chunks(4) {
            let same = |k: usize| q.iter().all(|v| v.pos[k] == q[0].pos[k]);
            assert!(same(0) || same(1) || same(2), "a slanted quad: {q:?}");
        }
    }

    #[test]
    fn the_trees_setting_thins_the_distant_woods() {
        for t in 0..60 {
            let (tx, tz) = (t * 3, t * 5 - 40);
            let col = worldgen::column(7, tx * TILE + 32, tz * TILE + 32);
            if col.biome == Biome::Forest && col.height > SEA + 2 {
                let (none, some, all) = (build(7, tx, tz, 4, 0).opaque.len(), build(7, tx, tz, 4, 1).opaque.len(), build(7, tx, tz, 4, 2).opaque.len());
                assert!(none < some && some < all, "{none} {some} {all}");
                assert_eq!((step_with(1, 2), step_with(1, 1), step_with(1, 0), step_with(10, 1), step_with(30, 1)), (2, 4, 8, 8, 16));
                return;
            }
        }
        panic!("no forest tile found");
    }
}
