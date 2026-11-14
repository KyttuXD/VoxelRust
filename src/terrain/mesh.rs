//! Lighting and meshing.
//!
//! A chunk's mesh is built from the chunk and its eight neighbours and from
//! nothing else, so mesh jobs can run on any thread. Light is recomputed for
//! that 48 x 48 block area each time; light travels at most 15 blocks, so
//! the 16-block margin makes the result exact for the chunk in the middle.

use crate::block::{reg, Block, Bx, Kind};
use crate::chunk::{ChunkData, CH, CW};
use alloc::vec::Vec;

/// One vertex, 20 bytes. Positions are in 1/16ths of a block relative to the
/// chunk; texture coordinates in 1/16ths of an atlas pixel; light
/// coordinates index the 16 x 16 light map (block light, sky light).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub pos: [i16; 4],
    pub col: [u8; 4],
    pub uv: [i16; 2],
    pub luv: [i16; 2],
}

#[derive(Default)]
pub struct MeshData {
    /// Opaque and cut-out geometry, four vertices per quad.
    pub opaque: Vec<Vertex>,
    /// Translucent geometry (water), drawn after everything else.
    pub water: Vec<Vertex>,
    /// See-through solid blocks (stained glass, ice): drawn with the water,
    /// but they do not rise and fall with it.
    pub glass: Vec<Vertex>,
    /// Vertical extent of the geometry in blocks, for culling.
    pub ymin: i32,
    pub ymax: i32,
    /// Blocks drawn as models rather than as part of the mesh: where in
    /// the chunk each is, and which block.
    pub props: Vec<([u8; 3], crate::block::Block)>,
    /// Where each section's vertices start, its bounds, and which of its
    /// faces see each other (see `terrain/sections.rs`).
    pub sections: crate::sections::SectionTables,
}

const R: usize = 48;
const H: usize = CH as usize;

#[inline]
fn ri(x: usize, y: usize, z: usize) -> usize {
    (x * R + z) * H + y
}

struct Face {
    n: [i32; 3],
    /// Corner of the unit cube where the quad starts.
    o: [i32; 3],
    /// The quad's two edges; `u x v = n`, and `v` is up for side faces.
    u: [i32; 3],
    v: [i32; 3],
    shade: u32,
    /// Which of the block's three textures: top, side, bottom.
    tex: usize,
}

const FACES: [Face; 6] = [
    Face { n: [1, 0, 0], o: [1, 0, 1], u: [0, 0, -1], v: [0, 1, 0], shade: 160, tex: 1 },
    Face { n: [-1, 0, 0], o: [0, 0, 0], u: [0, 0, 1], v: [0, 1, 0], shade: 160, tex: 1 },
    Face { n: [0, 1, 0], o: [0, 1, 1], u: [1, 0, 0], v: [0, 0, -1], shade: 255, tex: 0 },
    Face { n: [0, -1, 0], o: [0, 0, 0], u: [1, 0, 0], v: [0, 0, 1], shade: 128, tex: 2 },
    Face { n: [0, 0, 1], o: [0, 0, 1], u: [1, 0, 0], v: [0, 1, 0], shade: 205, tex: 1 },
    Face { n: [0, 0, -1], o: [1, 0, 0], u: [-1, 0, 0], v: [0, 1, 0], shade: 205, tex: 1 },
];

const CORNERS: [(i32, i32); 4] = [(0, 0), (1, 0), (1, 1), (0, 1)];
/// Brightness for 0..=3 occluding neighbours at a vertex.
const AO: [u32; 4] = [256, 200, 156, 116];

struct Region {
    blk: Vec<Block>,
    /// Sky light in the high nibble, block light in the low nibble.
    light: Vec<u8>,
}

impl Region {
    /// Block and light at region coordinates; above the world is open sky,
    /// below it is solid dark.
    #[inline]
    fn cell(&self, x: i32, y: i32, z: i32) -> (Block, u8) {
        if y >= CH {
            (0, 0xF0)
        } else if y < 0 {
            (u16::MAX, 0)
        } else {
            let i = ri(x as usize, y as usize, z as usize);
            (self.blk[i], self.light[i])
        }
    }
}

fn compute_light(blk: &[Block]) -> Vec<u8> {
    let r = reg();
    let (opaque, emit, loss) = (&r.opaque, &r.emit, &r.sky_loss);
    let mut light = alloc::vec![0u8; R * R * H];
    let mut skytop = alloc::vec![H as u8; R * R];
    let mut sq: Vec<u32> = Vec::with_capacity(4096);
    let mut bq: Vec<u32> = Vec::with_capacity(256);

    // Sunlight falls straight down each column until something stops it.
    for x in 0..R {
        for z in 0..R {
            let base = ri(x, 0, z);
            let mut l = 15u8;
            let mut top = H;
            for y in (0..H).rev() {
                let b = blk[base + y] as usize;
                let e = emit[b];
                if e > 0 {
                    light[base + y] |= e;
                    bq.push((base + y) as u32);
                }
                if l > 0 {
                    if opaque[b] != 0 {
                        l = 0;
                    } else {
                        l = l.saturating_sub(loss[b]);
                        if l == 15 {
                            top = y;
                        } else if l > 0 {
                            sq.push((base + y) as u32);
                        }
                        light[base + y] |= l << 4;
                    }
                }
            }
            skytop[x * R + z] = top as u8;
        }
    }
    // Fully lit cells only need to spread sideways where a neighbouring
    // column is still dark at that height.
    for x in 0..R {
        for z in 0..R {
            let t = skytop[x * R + z] as usize;
            let mut m = 0usize;
            if x > 0 {
                m = m.max(skytop[(x - 1) * R + z] as usize);
            }
            if x + 1 < R {
                m = m.max(skytop[(x + 1) * R + z] as usize);
            }
            if z > 0 {
                m = m.max(skytop[x * R + z - 1] as usize);
            }
            if z + 1 < R {
                m = m.max(skytop[x * R + z + 1] as usize);
            }
            for y in t..m.min(H) {
                sq.push(ri(x, y, z) as u32);
            }
        }
    }

    for (queue, shift) in [(&mut sq, 4u32), (&mut bq, 0u32)] {
        let mut head = 0;
        while head < queue.len() {
            let i = queue[head] as usize;
            head += 1;
            let l = (light[i] >> shift) & 15;
            if l <= 1 {
                continue;
            }
            let (x, z, y) = (i / (R * H), (i / H) % R, i % H);
            let visit = |j: usize, light: &mut Vec<u8>, queue: &mut Vec<u32>| {
                let b = blk[j] as usize;
                if opaque[b] != 0 {
                    return;
                }
                let nl = (l - 1).saturating_sub(loss[b].min(1));
                if (light[j] >> shift) & 15 < nl {
                    light[j] = (light[j] & !(15 << shift)) | (nl << shift);
                    queue.push(j as u32);
                }
            };
            if x > 0 {
                visit(i - R * H, &mut light, queue);
            }
            if x + 1 < R {
                visit(i + R * H, &mut light, queue);
            }
            if z > 0 {
                visit(i - H, &mut light, queue);
            }
            if z + 1 < R {
                visit(i + H, &mut light, queue);
            }
            if y > 0 {
                visit(i - 1, &mut light, queue);
            }
            if y + 1 < H {
                visit(i + 1, &mut light, queue);
            }
        }
    }
    light
}

#[inline]
fn tile_uv(tile: u16, px_u: i32, px_v: i32) -> [i16; 2] {
    // Pixel offsets 0 and 16 are pulled in by 1/16 pixel so a neighbouring
    // tile never bleeds in.
    let inset = |p: i32| (p * 16).clamp(1, 255);
    let n = crate::textures::PER_ROW as i32;
    [((tile as i32 % n) * 256 + inset(px_u)) as i16, ((tile as i32 / n) * 256 + inset(px_v)) as i16]
}

#[inline]
fn light_uv(l: u8) -> [i16; 2] {
    [((l & 15) as i16) * 16 + 8, ((l >> 4) as i16) * 16 + 8]
}

/// Build the mesh of the centre chunk. `n` holds the 3 x 3 neighbourhood,
/// indexed `(dz + 1) * 3 + (dx + 1)`.
pub fn build(n: &[&ChunkData; 9]) -> MeshData {
    let r = reg();
    let mut blk = alloc::vec![0 as Block; R * R * H];
    for czi in 0..3 {
        for cxi in 0..3 {
            let c = n[czi * 3 + cxi];
            for x in 0..16 {
                for z in 0..16 {
                    let s = (x * 16 + z) * H;
                    let d = ri(cxi * 16 + x, 0, czi * 16 + z);
                    blk[d..d + H].copy_from_slice(&c.blocks[s..s + H]);
                }
            }
        }
    }
    // Unknown state numbers (from a save made with other data) count as air.
    let states = r.blocks.len() as Block;
    for b in blk.iter_mut() {
        if *b >= states {
            *b = 0;
        }
    }
    let light = compute_light(&blk);
    let reg_ = Region { blk, light };
    let opaque = |b: Block| b == u16::MAX || r.opaque[b as usize] != 0;

    let mut out = MeshData { opaque: Vec::new(), water: Vec::new(), glass: Vec::new(), ymin: CH, ymax: 0, props: Vec::new(), sections: Default::default() };
    let mut shape: Vec<Bx> = Vec::new();
    let mut tabs = crate::sections::SectionTables { valid: true, ..Default::default() };

    // The geometry is made section by section (16 blocks of height each) so
    // that each section's vertices lie together in the buffers.
    for sy in 0..crate::sections::SECTIONS {
        tabs.opaque[sy] = out.opaque.len() as u32;
        tabs.water[sy] = out.water.len() as u32;
        tabs.glass[sy] = out.glass.len() as u32;
        tabs.conn[sy] = section_connectivity(&reg_, sy);
        // Bounds of the cells that gave geometry: x0 y0 z0 x1 y1 z1.
        let mut bb = [255u8, 255, 255, 0, 0, 0];
        for lx in 0..CW {
        for lz in 0..CW {
            let (x, z) = (lx + 16, lz + 16);
            let base = ri(x as usize, 0, z as usize);
            for y in sy as i32 * 16..sy as i32 * 16 + 16 {
                let b = reg_.blk[base + y as usize];
                if b == 0 {
                    continue;
                }
                let d = &r.blocks[b as usize];
                let before = out.opaque.len() + out.water.len() + out.glass.len();
                let origin = [lx * 16, y * 16, lz * 16];
                match d.kind {
                    Kind::Air => {}
                    Kind::Cube | Kind::Cutout | Kind::Liquid | Kind::Translucent => {
                        let liquid = d.kind == Kind::Liquid;
                        let blended = liquid || d.kind == Kind::Translucent;
                        // Where a liquid's four top corners stand (terrain/liquid_mesh.rs).
                        let cells = |cx: i32, cy: i32, cz: i32| reg_.cell(cx, cy, cz).0;
                        for f in &FACES {
                            let (nx, ny, nz) = (x + f.n[0], y + f.n[1], z + f.n[2]);
                            let nb = reg_.cell(nx, ny, nz).0;
                            // Faces between two blocks of the same see-through
                            // kind (water, glass, and leaves inside a canopy)
                            // are never seen: skipping them saves a great deal
                            // of drawing in forests.
                            // A liquid's top and bottom against the same liquid are hidden too.
                            if opaque(nb) || (nb == b && d.kind != Kind::Cube) || (liquid && crate::liquid_mesh::face_hidden(b, nb)) {
                                continue;
                            }
                            let mut quad = [Vertex { pos: [0; 4], col: [0; 4], uv: [0; 2], luv: [0; 2] }; 4];
                            let mut bright = [0u32; 4];
                            for (k, &(cu, cv)) in CORNERS.iter().enumerate() {
                                let su = if cu == 1 { 1 } else { -1 };
                                let sv = if cv == 1 { 1 } else { -1 };
                                let side = |a: i32, c: i32| {
                                    reg_.cell(nx + f.u[0] * a + f.v[0] * c, ny + f.u[1] * a + f.v[1] * c, nz + f.u[2] * a + f.v[2] * c)
                                };
                                let (_, l0) = side(0, 0);
                                let (b1, l1) = side(su, 0);
                                let (b2, l2) = side(0, sv);
                                let (b3, l3) = side(su, sv);
                                let (o1, o2) = (opaque(b1), opaque(b2));
                                let o3 = opaque(b3) || (o1 && o2);
                                let (mut sky, mut blo, mut cnt) = ((l0 >> 4) as i32, (l0 & 15) as i32, 1);
                                for (o, l) in [(o1, l1), (o2, l2), (o3, l3)] {
                                    if !o {
                                        sky += (l >> 4) as i32;
                                        blo += (l & 15) as i32;
                                        cnt += 1;
                                    }
                                }
                                // What gives off light is itself at full glow (lava, lamps), not as
                                // dim as the air beside it.
                                if r.emit[b as usize] >= 8 {
                                    blo = 15 * cnt;
                                }
                                let occ = o1 as usize + o2 as usize + o3 as usize;
                                let c = (f.shade * AO[occ] / 256).min(255) as u8;
                                let mut p = [
                                    origin[0] + (f.o[0] + f.u[0] * cu + f.v[0] * cv) * 16,
                                    origin[1] + (f.o[1] + f.u[1] * cu + f.v[1] * cv) * 16,
                                    origin[2] + (f.o[2] + f.u[2] * cu + f.v[2] * cv) * 16,
                                ];
                                if liquid && p[1] == origin[1] + 16 {
                                    let (tx, tz) = (if p[0] == origin[0] + 16 { 1 } else { -1 }, if p[2] == origin[2] + 16 { 1 } else { -1 });
                                    p[1] = origin[1] + crate::liquid_mesh::corner_top(&cells, b, x, y, z, tx, tz);
                                }
                                quad[k] = Vertex {
                                    pos: [p[0] as i16, p[1] as i16, p[2] as i16, 0],
                                    col: [c, c, c, 255],
                                    uv: tile_uv(d.tex[f.tex], cu * 16, (1 - cv) * 16),
                                    luv: [(blo * 16 / cnt + 8) as i16, (sky * 16 / cnt + 8) as i16],
                                };
                                bright[k] = c as u32 * 4 + (sky.max(blo) * 16 / cnt) as u32;
                            }
                            // Split the quad along its brighter diagonal so
                            // shadows in corners come out symmetric.
                            let order = if bright[0] + bright[2] >= bright[1] + bright[3] { [0, 1, 2, 3] } else { [1, 2, 3, 0] };
                            let dst = if liquid { &mut out.water } else if blended { &mut out.glass } else { &mut out.opaque };
                            for i in order {
                                dst.push(quad[i]);
                            }
                            // The surface of water is visible from below too.
                            if liquid && f.n[1] == 1 {
                                for i in [order[3], order[2], order[1], order[0]] {
                                    dst.push(quad[i]);
                                }
                            }
                        }
                    }
                    Kind::Cross => {
                        let luv = light_uv(reg_.cell(x, y, z).1);
                        let t = d.tex[1];
                        // Two diagonals, each drawn from both sides.
                        for (a, bq) in [([0, 0], [16, 16]), ([0, 16], [16, 0])] {
                            let p = [
                                ([a[0], 0, a[1]], [0, 16]),
                                ([bq[0], 0, bq[1]], [16, 16]),
                                ([bq[0], 16, bq[1]], [16, 0]),
                                ([a[0], 16, a[1]], [0, 0]),
                            ];
                            for order in [[0, 1, 2, 3], [3, 2, 1, 0]] {
                                for i in order {
                                    let (q, uv) = p[i];
                                    out.opaque.push(Vertex {
                                        pos: [(origin[0] + q[0]) as i16, (origin[1] + q[1]) as i16, (origin[2] + q[2]) as i16, 0],
                                        col: [235, 235, 235, 255],
                                        uv: tile_uv(t, uv[0], uv[1]),
                                        luv,
                                    });
                                }
                            }
                        }
                    }
                    // The unseen bulk of a big prop.
                    Kind::Boxes if d.x.part => {}
                    Kind::Boxes if d.x.prop.is_some() => {
                        // Drawn as a model each frame; nothing to mesh.
                        if out.props.len() < 4096 {
                            out.props.push(([lx as u8, y as u8, lz as u8], b));
                        }
                    }
                    Kind::Boxes => {
                        let own = reg_.cell(x, y, z).1;
                        let mask = crate::block::connect_mask(b, &|dx, dz| {
                            let nb = reg_.cell(x + dx, y, z + dz).0;
                            if nb == u16::MAX {
                                0
                            } else {
                                nb
                            }
                        });
                        crate::block::shape(b, mask, &mut shape);
                        for bx in &shape {
                            emit_box(&mut out.opaque, &reg_, &opaque, bx, &shape, (d.tex, d.front), origin, [x, y, z], own);
                        }
                    }
                }
                if out.opaque.len() + out.water.len() + out.glass.len() != before {
                    out.ymin = out.ymin.min(y);
                    out.ymax = out.ymax.max(y + 1);
                    let ry = (y - sy as i32 * 16) as u8;
                    let (cx, cz) = (lx as u8, lz as u8);
                    bb = [bb[0].min(cx), bb[1].min(ry), bb[2].min(cz), bb[3].max(cx + 1), bb[4].max(ry + 1), bb[5].max(cz + 1)];
                }
            }
        }
        }
        tabs.bounds[sy] = if bb[3] == 0 { [0; 6] } else { bb };
    }
    let end = crate::sections::SECTIONS;
    tabs.opaque[end] = out.opaque.len() as u32;
    tabs.water[end] = out.water.len() as u32;
    tabs.glass[end] = out.glass.len() as u32;
    out.sections = tabs;
    if out.ymin > out.ymax {
        out.ymin = 0;
        out.ymax = 0;
    }
    out
}

/// Which faces of section `sy` of the middle chunk see each other through
/// cells that do not block sight (see `terrain/sections.rs`).
fn section_connectivity(region: &Region, sy: usize) -> u16 {
    let r = reg();
    let mut open = [0u8; 4096];
    let mut any_open = false;
    let mut any_shut = false;
    for x in 0..16usize {
        for z in 0..16usize {
            // A column's cells are contiguous in the region: read it along y.
            let base = ri(x + 16, sy * 16, z + 16);
            for y in 0..16usize {
                let b = region.blk[base + y];
                let o = r.opaque[b as usize] == 0;
                open[x + 16 * z + 256 * y] = o as u8;
                any_open |= o;
                any_shut |= !o;
            }
        }
    }
    if !any_open {
        return 0;
    }
    if !any_shut {
        return crate::sections::CONN_ALL;
    }
    crate::sections::connectivity(&open)
}

/// Emit the six faces of one box of a `Boxes` block. A face is skipped when
/// it lies on the cell boundary against an opaque neighbour, or when another
/// box of the same block covers it completely.
#[allow(clippy::too_many_arguments)]
fn emit_box(
    dst: &mut Vec<Vertex>,
    region: &Region,
    opaque: &dyn Fn(Block) -> bool,
    bx: &Bx,
    all: &[Bx],
    (tex, front): ([u16; 3], Option<(u8, u16)>),
    origin: [i32; 3],
    cell: [i32; 3],
    own_light: u8,
) {
    let (mn, mx) = (bx.min.map(|v| v as i32), bx.max.map(|v| v as i32));
    for (face, f) in FACES.iter().enumerate() {
        // A block's `front` picture goes on the face that looks its way.
        let tile = match front {
            Some((n, t)) if n as usize == face => t,
            _ => tex[f.tex],
        };
        let axis = if f.n[0] != 0 {
            0
        } else if f.n[1] != 0 {
            1
        } else {
            2
        };
        let positive = f.n[axis] > 0;
        let plane = if positive { mx[axis] } else { mn[axis] };
        let on_boundary = plane == if positive { 16 } else { 0 };
        let (ta, tb) = ((axis + 1) % 3, (axis + 2) % 3);
        let mut l = own_light;
        if on_boundary {
            let (nb, nl) = region.cell(cell[0] + f.n[0], cell[1] + f.n[1], cell[2] + f.n[2]);
            if opaque(nb) {
                continue;
            }
            // Use the brighter of the two cells per channel.
            l = (l >> 4).max(nl >> 4) << 4 | (l & 15).max(nl & 15);
        } else if all.iter().any(|o| {
            let (omn, omx) = (o.min.map(|v| v as i32), o.max.map(|v| v as i32));
            let touching = if positive { omn[axis] == plane } else { omx[axis] == plane };
            touching && omn[ta] <= mn[ta] && omx[ta] >= mx[ta] && omn[tb] <= mn[tb] && omx[tb] >= mx[tb]
        }) {
            continue;
        }
        let c = f.shade.min(255) as u8;
        let luv = light_uv(l);
        for &(cu, cv) in &CORNERS {
            // Start from the face's corner of the unit cube, then clamp each
            // coordinate into the box.
            let mut p = [0i32; 3];
            for a in 0..3 {
                let unit = f.o[a] + f.u[a] * cu + f.v[a] * cv;
                p[a] = if a == axis {
                    plane
                } else if unit == 1 {
                    mx[a]
                } else {
                    mn[a]
                };
            }
            // Texture coordinates follow the position inside the cell, so
            // neighbouring partial blocks line up.
            let along = |e: &[i32; 3]| -> i32 {
                let a = if e[0] != 0 {
                    0
                } else if e[1] != 0 {
                    1
                } else {
                    2
                };
                if e[a] > 0 {
                    p[a]
                } else {
                    16 - p[a]
                }
            };
            dst.push(Vertex {
                pos: [(origin[0] + p[0]) as i16, (origin[1] + p[1]) as i16, (origin[2] + p[2]) as i16, 0],
                col: [c, c, c, 255],
                uv: tile_uv(tile, along(&f.u), 16 - along(&f.v)),
                luv,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::AIR;

    fn hood(centre: &ChunkData, other: &ChunkData) -> MeshData {
        let n = [other, other, other, other, centre, other, other, other, other];
        build(&n)
    }

    #[test]
    fn empty_chunk_has_no_geometry() {
        let e = ChunkData::new();
        let m = hood(&e, &e);
        assert!(m.opaque.is_empty() && m.water.is_empty());
    }

    #[test]
    fn single_block_has_six_fully_lit_faces() {
        let e = ChunkData::new();
        let mut c = ChunkData::new();
        c.set(8, 60, 8, reg().wk.stone);
        let m = hood(&c, &e);
        assert_eq!(m.opaque.len(), 24);
        assert_eq!((m.ymin, m.ymax), (60, 61));
        // In open air everything gets full sky light and no block light.
        // In open air there is no block light, and full sky light everywhere
        // except underneath, where the block casts a faint shadow.
        assert!(m.opaque.iter().all(|v| v.luv[0] == 8 && v.luv[1] >= 14 * 16 + 8));
        assert!(m.opaque.iter().filter(|v| v.pos[1] == 61 * 16).all(|v| v.luv[1] == 15 * 16 + 8));
        assert!(m.opaque.iter().any(|v| v.luv[1] < 15 * 16 + 8));
    }

    fn floor_chunk() -> ChunkData {
        let mut c = ChunkData::new();
        for x in 0..16 {
            for z in 0..16 {
                for y in 0..70 {
                    c.set(x, y, z, reg().wk.stone);
                }
            }
        }
        c
    }

    #[test]
    fn a_solid_floor_draws_only_its_top_even_at_the_chunk_borders() {
        // Faces between blocks, against solid neighbouring chunks and against the
        // world's floor are never made: only the 256 tops remain.
        let f = floor_chunk();
        let n = [&f, &f, &f, &f, &f, &f, &f, &f, &f];
        let m = build(&n);
        assert_eq!(m.opaque.len(), 256 * 4);
        assert!(m.opaque.iter().all(|v| v.pos[1] == 70 * 16));
    }

    #[test]
    fn a_buried_block_makes_no_faces() {
        let e = ChunkData::new();
        let mut c = ChunkData::new();
        let s = reg().wk.stone;
        for x in 6..9 {
            for y in 60..63 {
                for z in 6..9 {
                    c.set(x, y, z, s);
                }
            }
        }
        // A 3 x 3 x 3 cube: nine faces on each side and nothing from the middle block.
        assert_eq!(hood(&c, &e).opaque.len(), 6 * 9 * 4);
    }

    #[test]
    fn sections_split_the_buffers_by_height() {
        let e = ChunkData::new();
        let mut c = ChunkData::new();
        c.set(8, 60, 8, reg().wk.stone);
        c.set(2, 200, 3, reg().wk.stone);
        let m = hood(&c, &e);
        let t = &m.sections;
        assert!(t.valid);
        assert_eq!(t.opaque[16] as usize, m.opaque.len());
        assert_eq!(m.opaque.len(), 48);
        for s in 0..16 {
            assert_eq!(t.has_geometry(s), s == 3 || s == 12, "section {}", s);
        }
        // Section 3 holds y 48..64 and comes first.
        assert_eq!((t.opaque[3], t.opaque[4]), (0, 24));
        assert_eq!((t.opaque[12], t.opaque[13]), (24, 48));
        assert!(m.opaque[..24].iter().all(|v| (60 * 16..=61 * 16).contains(&v.pos[1])));
        assert!(m.opaque[24..].iter().all(|v| (200 * 16..=201 * 16).contains(&v.pos[1])));
        // The bounds are those of the block, y from the section's floor.
        assert_eq!(t.bounds[3], [8, 12, 8, 9, 13, 9]);
        assert_eq!(t.bounds[12], [2, 8, 3, 3, 9, 4]);
        // Open air all through.
        assert!(t.conn.iter().all(|&c| c == crate::sections::CONN_ALL));
    }

    #[test]
    fn solid_sections_see_nothing_and_a_floor_cuts_the_underside_off() {
        use crate::sections::{connected, FACE_NX, FACE_NY, FACE_PX, FACE_PY};
        let f = floor_chunk();
        let n = [&f, &f, &f, &f, &f, &f, &f, &f, &f];
        let t = build(&n).sections;
        // Sections 0..=3 (y < 64) are solid.
        for s in 0..4 {
            assert_eq!(t.conn[s], 0, "section {}", s);
        }
        // Section 4 has the floor's top at 70: air above it joins the sides and the top,
        // but not the bottom, which is rock.
        assert!(connected(t.conn[4], FACE_NX, FACE_PX));
        assert!(connected(t.conn[4], FACE_NX, FACE_PY));
        assert!(!connected(t.conn[4], FACE_NY, FACE_PY));
        for s in 5..16 {
            assert_eq!(t.conn[s], crate::sections::CONN_ALL);
        }
    }

    #[test]
    fn hidden_faces_are_culled() {
        let e = ChunkData::new();
        let mut c = ChunkData::new();
        let s = reg().wk.stone;
        c.set(8, 60, 8, s);
        c.set(9, 60, 8, s);
        // Two touching cubes: 12 faces minus the two that touch.
        assert_eq!(hood(&c, &e).opaque.len(), 40);
    }

    #[test]
    fn roof_blocks_sky_and_lamp_lights_the_room() {
        let wk = &reg().wk;
        let e = ChunkData::new();
        let mut full = ChunkData::new();
        // A solid slab two blocks thick over the whole neighbourhood.
        for x in 0..16 {
            for z in 0..16 {
                full.set(x, 80, z, wk.stone);
                full.set(x, 81, z, wk.stone);
                full.set(x, 0, z, wk.stone);
            }
        }
        let mut c = full.clone();
        c.set(8, 40, 8, wk.stone);
        let n = [&full, &full, &full, &full, &c, &full, &full, &full, &full];
        let dark = build(&n);
        // Faces of the block at y=40 are far from any sky: only block light 0.
        let probe: Vec<_> = dark.opaque.iter().filter(|v| (640..=656).contains(&v.pos[1])).collect();
        assert_eq!(probe.len(), 24);
        assert!(probe.iter().all(|v| v.luv == [8, 8]), "{:?}", probe[0]);

        c.set(8, 42, 8, reg().block("lamp").unwrap());
        let n = [&full, &full, &full, &full, &c, &full, &full, &full, &full];
        let lit = build(&n);
        let top: Vec<_> = lit.opaque.iter().filter(|v| v.pos[1] == 656 && v.pos[0] >= 128 && v.pos[0] <= 144).collect();
        assert!(top.iter().any(|v| v.luv[0] > 12 * 16), "lamp light did not reach the block below");
        let _ = (e, AIR);
    }

    #[test]
    fn stairs_mesh_as_two_boxes_without_the_hidden_face() {
        let e = ChunkData::new();
        let mut c = ChunkData::new();
        c.set(8, 60, 8, reg().block("stone_stairs").unwrap());
        // Two boxes = 12 faces; the top box's underside rests on the slab.
        assert_eq!(hood(&c, &e).opaque.len(), 11 * 4);
        let mut c = ChunkData::new();
        c.set(8, 60, 8, reg().block("stone_slab").unwrap());
        c.set(8, 59, 8, reg().wk.stone);
        // Slab: 6 faces, bottom hidden by the cube; cube: 6 faces, all shown
        // (a slab is not opaque).
        let m = hood(&c, &e);
        assert_eq!(m.opaque.len(), 5 * 4 + 6 * 4);
        // The slab's top is at half height.
        assert!(m.opaque.iter().any(|v| v.pos[1] == 60 * 16 + 8));
    }

    #[test]
    fn water_goes_to_the_translucent_mesh() {
        let e = ChunkData::new();
        let mut c = ChunkData::new();
        c.set(8, 60, 8, reg().wk.water);
        let m = hood(&c, &e);
        assert!(m.opaque.is_empty());
        // Six faces plus the underside of the surface.
        assert_eq!(m.water.len(), 28);
    }

    #[test]
    fn a_flow_is_as_tall_as_its_level_and_a_column_is_full() {
        let r = reg();
        let e = ChunkData::new();
        // A lone flow at level 3 would stand 14 * 3 / 8 = 5 sixteenths high flat, but its
        // corners fall away toward the open air around it: between 1 and 5.
        let mut c = ChunkData::new();
        c.set(8, 60, 8, r.flow[0][3]);
        let lone = hood(&c, &e);
        assert!(lone.water.iter().all(|v| (60 * 16..=60 * 16 + 5).contains(&v.pos[1])), "{:?}", lone.water.iter().map(|v| v.pos[1]).collect::<Vec<_>>());
        // A falling column: the cell at the bottom has the same liquid above it,
        // so its surface is full height and its sides reach the top of the cell
        // above (61 * 16). The top of the column is a level-7 flow, 12 high.
        let mut col = ChunkData::new();
        col.set(8, 60, 8, r.flow[0][7]);
        col.set(8, 61, 8, r.flow[0][7]);
        let stacked = hood(&col, &e);
        let ys: Vec<i16> = stacked.water.iter().map(|v| v.pos[1]).collect();
        assert!(ys.contains(&(61 * 16)), "the bottom of the column is not full height");
        assert!(ys.iter().all(|&y| y <= 61 * 16 + 12), "a column is taller than its top: {:?}", ys.iter().max());
        // No faces between the two cells of the column: fewer than two single cells' worth.
        let single = hood(&c, &e).water.len();
        assert!(stacked.water.len() < 2 * single + 4, "{} vs {}", stacked.water.len(), single);
    }

    #[test]
    fn no_walls_show_inside_a_body_of_water() {
        // A lake with a hole where a block was broken, now filled with a flow:
        // it stands as tall as the water around it, so nothing may be drawn between them.
        let r = reg();
        let e = ChunkData::new();
        let mut lake = ChunkData::new();
        for x in 6..11 {
            for z in 6..11 {
                for y in 58..62 {
                    lake.set(x, y, z, r.wk.water);
                }
            }
        }
        let whole = hood(&lake, &e).water.len();
        let mut hole = lake.clone();
        hole.set(8, 60, 8, r.flow[0][7]);
        let with_hole = hood(&hole, &e).water.len();
        assert_eq!(whole, with_hole, "walls are drawn around a flow inside the water");
    }

    #[test]
    fn lava_lights_the_air_around_it() {
        let r = reg();
        let mut blk = alloc::vec![0 as Block; R * R * H];
        // A roof over the whole area (so only block light is in play) and a lava cell below it.
        for x in 0..R {
            for z in 0..R {
                blk[ri(x, 200, z)] = r.wk.stone;
            }
        }
        let at = (R / 2, 100, R / 2);
        blk[ri(at.0, at.1, at.2)] = r.wk.lava;
        let light = compute_light(&blk);
        let next = light[ri(at.0 + 1, at.1, at.2)] & 15;
        let far = light[ri(at.0 + 5, at.1, at.2)] & 15;
        assert!(next >= 10, "air beside lava is lit {next} of 15");
        assert!(far >= 5 && far < next, "the light fades with distance: {far}");
        let flow = r.flow[1][3];
        blk[ri(at.0, at.1, at.2)] = flow;
        let light = compute_light(&blk);
        assert!(light[ri(at.0 + 1, at.1, at.2)] & 15 >= 8, "a lava flow does not light its surroundings");
    }
}
