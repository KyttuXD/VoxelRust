//! Looking at the world: what is loaded, how high the ground is, which
//! biome, what a box of it holds. Reading only; changing blocks is
//! `set_block` in `script/host.rs`.

use super::args::{coord, finite, int, num, text, want, EDGE};
use super::Ctx;
use crate::block;
use crate::chunk::CH;
use crate::math::Vec3;
use crate::session::Session;
use crate::script::value::Value;
use alloc::format;
use alloc::string::String;
use crate::script::numeric::fabs;
use alloc::vec::Vec;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("is_loaded(x: i64, z: i64) -> bool", "whether the chunk column holding that place is loaded (`get_block` says \"air\" for places that are not)"),
    ("surface_y(x: i64, z: i64) -> i64", "the height a creature stands at on that column of the world's natural ground (one above its top block); works where nothing is loaded"),
    ("biome(x: i64, z: i64) -> String", "the natural biome of that column, lower case (`plains`, `forest`, `desert`, `tundra`, `taiga`, `mountain`, `swamp`, `savanna`, `jungle`, `badlands`)"),
    ("is_solid(x: i64, y: i64, z: i64) -> bool", "whether the block there stops a body"),
    ("block_tag(block_id: &str, tag: &str) -> f64", "the number after `tag <name>` in a block's pack lines (1 if none was given), or -1 if the block does not have the tag"),
    ("scan_blocks(x0: i64, y0: i64, z0: i64, x1: i64, y1: i64, z1: i64, prefix: &str) -> Vec<(i64, i64, i64, String)>", "every loaded block in the box whose id starts with `prefix` (empty: any but air), as (x, y, z, id), up to 2000 of them, in x then z then y order; the box may hold at most 40 000 cells"),
    ("block_ray(ox: f64, oy: f64, oz: f64, dx: f64, dy: f64, dz: f64, max: f64) -> f64", "how far along a ray the first solid block is (up to `max`, at most 200), or -1 if none is in the way"),
    ("find_path(sx: f64, sy: f64, sz: f64, gx: f64, gy: f64, gz: f64) -> Vec<(i64, i64, i64)>", "a walkable way over the loaded blocks from the place a walker stands (feet at sx, sy, sz) toward a goal, as the cells to step on in order (up to 64; the search looks at 320 cells at most, so a far goal gives the way to the nearest point reached); empty if there is none; both ends within 150 blocks of each other"),
];

/// The biomes' names.
fn biome_name(b: crate::worldgen::Biome) -> &'static str {
    use crate::worldgen::Biome::*;
    match b {
        Plains => "plains",
        Forest => "forest",
        Desert => "desert",
        Tundra => "tundra",
        Taiga => "taiga",
        Mountain => "mountain",
        Swamp => "swamp",
        Savanna => "savanna",
        Jungle => "jungle",
        Badlands => "badlands",
    }
}

const MAX_CELLS: i64 = 40_000;
const MAX_FOUND: usize = 2000;

pub fn call(g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "is_loaded" => {
            want(name, args, 2)?;
            Ok(Value::Bool(g.world.is_loaded(coord(name, args, 0)?, coord(name, args, 1)?)))
        }
        "surface_y" => {
            want(name, args, 2)?;
            let col = crate::worldgen::column(g.world.seed, coord(name, args, 0)?, coord(name, args, 1)?);
            Ok(Value::Int(col.height as i64 + 1))
        }
        "biome" => {
            want(name, args, 2)?;
            let col = crate::worldgen::column(g.world.seed, coord(name, args, 0)?, coord(name, args, 1)?);
            Ok(Value::str(biome_name(col.biome)))
        }
        "is_solid" => {
            want(name, args, 3)?;
            let y = int(name, args, 1)?;
            if !(0..CH as i64).contains(&y) {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(block::is_solid(g.world.get(coord(name, args, 0)?, y as i32, coord(name, args, 2)?))))
        }
        "block_tag" => {
            want(name, args, 2)?;
            let (id, tag) = (text(name, args, 0)?, text(name, args, 1)?);
            let tagged = block::reg().block(id).and_then(|b| block::def(b).x.tag(tag));
            Ok(Value::Float(tagged.map_or(-1.0, |t| t as f64)))
        }
        "scan_blocks" => {
            want(name, args, 7)?;
            let mut lo = [int(name, args, 0)?, int(name, args, 1)?, int(name, args, 2)?];
            let mut hi = [int(name, args, 3)?, int(name, args, 4)?, int(name, args, 5)?];
            let prefix = text(name, args, 6)?;
            for a in 0..3 {
                if lo[a] > hi[a] {
                    core::mem::swap(&mut lo[a], &mut hi[a]);
                }
            }
            lo[0] = lo[0].max(-EDGE);
            lo[2] = lo[2].max(-EDGE);
            hi[0] = hi[0].min(EDGE);
            hi[2] = hi[2].min(EDGE);
            lo[1] = lo[1].max(0);
            hi[1] = hi[1].min(CH as i64 - 1);
            if lo[0] > hi[0] || lo[1] > hi[1] || lo[2] > hi[2] {
                return Ok(Value::vec(Vec::new()));
            }
            let cells = (hi[0] - lo[0] + 1) * (hi[1] - lo[1] + 1) * (hi[2] - lo[2] + 1);
            if cells > MAX_CELLS {
                return Err(format!("`scan_blocks`: the box holds {} cells, at most {} are allowed", cells, MAX_CELLS));
            }
            let mut out: Vec<Value> = Vec::new();
            'scan: for x in lo[0]..=hi[0] {
                for z in lo[2]..=hi[2] {
                    if !g.world.is_loaded(x as i32, z as i32) {
                        continue;
                    }
                    for y in lo[1]..=hi[1] {
                        let b = g.world.get(x as i32, y as i32, z as i32);
                        if b == block::AIR {
                            continue;
                        }
                        let id = &block::def(b).id;
                        if !id.starts_with(prefix) {
                            continue;
                        }
                        out.push(Value::tuple(alloc::vec![Value::Int(x), Value::Int(y), Value::Int(z), Value::str(id)]));
                        if out.len() >= MAX_FOUND {
                            break 'scan;
                        }
                    }
                }
            }
            Ok(Value::vec(out))
        }
        "block_ray" => {
            want(name, args, 7)?;
            let o = [finite(name, args, 0)?, finite(name, args, 1)?, finite(name, args, 2)?];
            let d = [finite(name, args, 3)?, finite(name, args, 4)?, finite(name, args, 5)?];
            let max = num(name, args, 6)?.clamp(0.0, 200.0);
            let len = crate::script::numeric::sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
            if len < 1e-9 {
                return Ok(Value::Float(-1.0));
            }
            let dir = Vec3 { x: (d[0] / len) as f32, y: (d[1] / len) as f32, z: (d[2] / len) as f32 };
            Ok(Value::Float(g.world.raycast(o, dir, max as f32).map_or(-1.0, |h| h.dist as f64)))
        }
        "find_path" => {
            want(name, args, 6)?;
            let s = [finite(name, args, 0)?, finite(name, args, 1)?, finite(name, args, 2)?];
            let t = [finite(name, args, 3)?, finite(name, args, 4)?, finite(name, args, 5)?];
            if fabs(s[0] - t[0]) > 150.0 || fabs(s[2] - t[2]) > 150.0 || fabs(s[0]) > 3.0e7 || fabs(s[2]) > 3.0e7 || fabs(t[0]) > 3.0e7 || fabs(t[2]) > 3.0e7 {
                return Ok(Value::vec(Vec::new()));
            }
            let world = &g.world;
            let solid = |x: i32, y: i32, z: i32| block::is_solid(world.get(x, y, z));
            let (sx, sy, sz) = (crate::math::floord(s[0]), crate::math::floord(s[1]), crate::math::floord(s[2]));
            let (tx, ty, tz) = (crate::math::floord(t[0]), crate::math::floord(t[1]), crate::math::floord(t[2]));
            let start = crate::path::ground_near(&solid, sx, sy, sz, 2);
            let end = crate::path::ground_near(&solid, tx, ty, tz, 4);
            let cells = match (start, end) {
                (Some(a), Some(b)) => crate::path::find_path(&solid, a, b, 320).unwrap_or_default(),
                _ => Vec::new(),
            };
            Ok(Value::vec(cells.iter().take(64).map(|c| Value::tuple(alloc::vec![Value::Int(c[0] as i64), Value::Int(c[1] as i64), Value::Int(c[2] as i64)])).collect()))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
