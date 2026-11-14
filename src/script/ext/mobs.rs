//! Creatures: finding them, placing them, moving them, hurting them. A
//! creature is named by its number (`id`), which stays the same while it
//! lives. Changes are refused in a shared game by a player who does not run
//! the creatures (their copy of the world is only a picture of the host's).

use super::args::{finite, int, num, text, want};
use super::Ctx;
use crate::block;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("mobs_near(x: f64, y: f64, z: f64, radius: f64, kind: &str) -> Vec<(i64, f64, f64, f64)>", "the living creatures of one kind (empty: any) within a radius, nearest first, as (id, x, y, z); at most 64"),
    ("spawn_mob(kind: &str, x: f64, y: f64, z: f64) -> i64", "make a creature appear and give its id, or -1 (no such kind, not allowed here, or the world is full of creatures: 300 at most)"),
    ("mob_info(id: i64) -> (String, i64, f64, f64, f64)", "(kind, health, x, y, z) of a creature; the kind is empty if it is gone or dying"),
    ("mob_set_pos(id: i64, x: f64, y: f64, z: f64)", "put a creature somewhere and stop it"),
    ("mob_set_yaw(id: i64, yaw: f64)", "turn a creature (yaw 0 faces -z; its forward is (-sin yaw, -cos yaw))"),
    ("mob_set_size(id: i64, width: f64, height: f64)", "change the box a creature collides with (0.1 to 8 blocks); what it is drawn as does not change"),
    ("mob_remove(id: i64)", "make a creature vanish at once: nothing drops, no `on_entity_death`"),
    ("mob_hurt(id: i64, n: i64)", "hurt a creature as the player's blow would (drops and `on_entity_death` follow if it dies)"),
    ("mob_in_view(max: f64) -> i64", "the id of the first creature on the player's look ray within `max` blocks (at most 100) with no block in the way, or -1"),
];

/// Most creatures `spawn_mob` allows in the world.
const MOST: usize = 300;
const NEAR_MOST: usize = 64;

fn index_of(g: &Session, id: i64) -> Option<usize> {
    if id < 0 || id > u32::MAX as i64 {
        return None;
    }
    g.mobs.list.iter().position(|m| m.id == id as u32)
}

fn may_change(g: &Session) -> bool {
    g.net.is_none() || g.runs_the_world()
}

fn kind_of(id: &str) -> Option<usize> {
    block::reg().entities.iter().position(|e| e.id == id)
}

pub fn call(g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "mobs_near" => {
            want(name, args, 5)?;
            let p = [finite(name, args, 0)?, finite(name, args, 1)?, finite(name, args, 2)?];
            let r = num(name, args, 3)?.clamp(0.0, 500.0);
            let kind = text(name, args, 4)?;
            let want_kind = if kind.is_empty() { None } else { kind_of(kind) };
            if !kind.is_empty() && want_kind.is_none() {
                return Ok(Value::vec(Vec::new()));
            }
            let mut found: Vec<(f64, u32, [f64; 3])> = Vec::new();
            for m in g.mobs.list.iter().filter(|m| m.health > 0 && want_kind.map_or(true, |k| m.kind == k)) {
                let (dx, dy, dz) = (m.body.pos[0] - p[0], m.body.pos[1] - p[1], m.body.pos[2] - p[2]);
                let d2 = dx * dx + dy * dy + dz * dz;
                if d2 <= r * r {
                    found.push((d2, m.id, m.body.pos));
                }
            }
            found.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));
            found.truncate(NEAR_MOST);
            Ok(Value::vec(found.into_iter().map(|(_, id, at)| Value::tuple(alloc::vec![Value::Int(id as i64), Value::Float(at[0]), Value::Float(at[1]), Value::Float(at[2])])).collect()))
        }
        "spawn_mob" => {
            want(name, args, 4)?;
            let kind = text(name, args, 0)?;
            let at = [finite(name, args, 1)?, finite(name, args, 2)?, finite(name, args, 3)?];
            let Some(k) = kind_of(kind) else { return Ok(Value::Int(-1)) };
            if !may_change(g) || g.mobs.list.len() >= MOST {
                return Ok(Value::Int(-1));
            }
            let at = [at[0], at[1].clamp(1.0, crate::chunk::CH as f64 + 20.0), at[2]];
            let i = g.mobs.spawn(k, at);
            Ok(Value::Int(g.mobs.list[i].id as i64))
        }
        "mob_info" => {
            want(name, args, 1)?;
            let found = index_of(g, int(name, args, 0)?).map(|i| &g.mobs.list[i]).filter(|m| m.health > 0);
            Ok(match found {
                Some(m) => Value::tuple(alloc::vec![Value::string(block::reg().entities.get(m.kind).map(|e| e.id.clone()).unwrap_or_default()), Value::Int(m.health as i64), Value::Float(m.body.pos[0]), Value::Float(m.body.pos[1]), Value::Float(m.body.pos[2])]),
                None => Value::tuple(alloc::vec![Value::str(""), Value::Int(0), Value::Float(0.0), Value::Float(0.0), Value::Float(0.0)]),
            })
        }
        "mob_set_pos" => {
            want(name, args, 4)?;
            let at = [finite(name, args, 1)?, finite(name, args, 2)?, finite(name, args, 3)?];
            if let (true, Some(i)) = (may_change(g), index_of(g, int(name, args, 0)?)) {
                let b = &mut g.mobs.list[i].body;
                b.pos = [at[0], at[1].clamp(-64.0, crate::chunk::CH as f64 + 64.0), at[2]];
                b.vel = [0.0; 3];
            }
            Ok(Value::Unit)
        }
        "mob_set_yaw" => {
            want(name, args, 2)?;
            let yaw = finite(name, args, 1)?;
            if let (true, Some(i)) = (may_change(g), index_of(g, int(name, args, 0)?)) {
                g.mobs.list[i].body.yaw = yaw as f32;
            }
            Ok(Value::Unit)
        }
        "mob_set_size" => {
            want(name, args, 3)?;
            let (w, h) = (finite(name, args, 1)?.clamp(0.1, 8.0), finite(name, args, 2)?.clamp(0.1, 8.0));
            if let (true, Some(i)) = (may_change(g), index_of(g, int(name, args, 0)?)) {
                let b = &mut g.mobs.list[i].body;
                b.half_width = w * 0.5;
                b.height = h;
            }
            Ok(Value::Unit)
        }
        "mob_remove" => {
            want(name, args, 1)?;
            if let (true, Some(i)) = (may_change(g), index_of(g, int(name, args, 0)?)) {
                g.mobs.list[i].health = 0;
            }
            Ok(Value::Unit)
        }
        "mob_hurt" => {
            want(name, args, 2)?;
            let n = int(name, args, 1)?.clamp(0, 1000) as i32;
            if let (true, Some(i)) = (may_change(g), index_of(g, int(name, args, 0)?)) {
                let from = g.mobs.list[i].body.pos;
                g.mobs.hurt(i, n, [from[0], from[1], from[2] + 1.0]);
            }
            Ok(Value::Unit)
        }
        "mob_in_view" => {
            want(name, args, 1)?;
            let max = num(name, args, 0)?.clamp(0.0, 100.0) as f32;
            let eye = g.player.eye();
            let dir = crate::math::forward(g.player.yaw, g.player.pitch);
            let Some((i, d)) = g.mobs.hit_test(eye, dir, max) else { return Ok(Value::Int(-1)) };
            let wall = g.world.raycast(eye, dir, max).map_or(max, |h| h.dist);
            Ok(Value::Int(if d > wall { -1 } else { g.mobs.list[i].id as i64 }))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
