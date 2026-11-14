//! Physics bodies: a box that moves and collides with the world the way the
//! player and the creatures do (gravity, steps, water, flying), driven by
//! the script. This is what a vehicle, a drone or a thrown thing is made of;
//! the script says where it wants to go each frame, the game says where it
//! got to. A body has no look of its own: draw it with `draw_model`.

use super::args::{finite, flag, int, num, want, xyz};
use super::data::{data, MAX_BODIES};
use super::Ctx;
use crate::math::floord;
use crate::player::{Input, Player};
use crate::script::numeric::fabs;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("body_new(width: f64, height: f64, x: f64, y: f64, z: f64) -> i64", "make a physics body (a box on the ground at x, y, z, `width` by `height` blocks, up to 6 each) and give its id, or -1 if there are too many bodies (256 over all mods)"),
    ("body_free(id: i64)", "forget a body"),
    ("body_step(id: i64, dt: f64, forward: f64, strafe: f64, jump: bool, descend: bool, flying: bool, speed: f64)", "move a body for `dt` seconds (at most 0.1) as a walker would: `forward` and `strafe` -1..1 along its yaw, `jump`, `descend`, `flying` (no gravity: up on `jump`, down on `descend`), `speed` a multiplier of the walking speed (0 to 8; the player walks 4.8 blocks a second at 1, flies at 11); does nothing where the world is not loaded"),
    ("body_pos(id: i64) -> (f64, f64, f64)", "where a body's feet are"),
    ("body_exists(id: i64) -> bool", "whether a body with that id is there (bodies live only as long as the game session: a saved id is stale after the world is opened again)"),
    ("body_set_pos(id: i64, x: f64, y: f64, z: f64)", "put a body somewhere; its velocity is kept (`body_set_vel` stops it)"),
    ("body_vel(id: i64) -> (f64, f64, f64)", "a body's velocity in blocks a second"),
    ("body_set_vel(id: i64, x: f64, y: f64, z: f64)", "set a body's velocity"),
    ("body_yaw(id: i64) -> f64", "which way a body faces (yaw 0 faces -z)"),
    ("body_set_yaw(id: i64, yaw: f64)", "turn a body"),
    ("body_state(id: i64) -> (bool, bool)", "(on the ground, in water or lava)"),
    ("body_ray(id: i64, ox: f64, oy: f64, oz: f64, dx: f64, dy: f64, dz: f64, max: f64) -> f64", "how far along a ray (up to `max`, at most 50) the box of a body is hit, or -1 if it is missed"),
];

fn index_of(g: &mut Session, id: i64) -> Option<usize> {
    if id < 0 || id > u32::MAX as i64 {
        return None;
    }
    data(g).bodies.iter().position(|b| b.0 == id as u32)
}

pub fn call(g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "body_new" => {
            want(name, args, 5)?;
            let (w, h) = (finite(name, args, 0)?.clamp(0.1, 6.0), finite(name, args, 1)?.clamp(0.1, 6.0));
            let at = [finite(name, args, 2)?, finite(name, args, 3)?.clamp(-64.0, crate::chunk::CH as f64 + 64.0), finite(name, args, 4)?];
            let d = data(g);
            if d.bodies.len() >= MAX_BODIES || fabs(at[0]) > 3.0e7 || fabs(at[2]) > 3.0e7 {
                return Ok(Value::Int(-1));
            }
            d.next_body = d.next_body.wrapping_add(1).max(1);
            let id = d.next_body;
            d.bodies.push((id, Player::with_size(at, w, h)));
            Ok(Value::Int(id as i64))
        }
        "body_exists" => {
            want(name, args, 1)?;
            Ok(Value::Bool(index_of(g, int(name, args, 0)?).is_some()))
        }
        "body_free" => {
            want(name, args, 1)?;
            if let Some(i) = index_of(g, int(name, args, 0)?) {
                data(g).bodies.swap_remove(i);
            }
            Ok(Value::Unit)
        }
        "body_step" => {
            want(name, args, 8)?;
            let dt = finite(name, args, 1)?.clamp(0.0, 0.1) as f32;
            let input = Input { forward: num(name, args, 2)?.clamp(-1.0, 1.0) as f32, strafe: num(name, args, 3)?.clamp(-1.0, 1.0) as f32, jump: flag(name, args, 4)?, descend: flag(name, args, 5)?, sprint: false };
            let flying = flag(name, args, 6)?;
            let speed = finite(name, args, 7)?.clamp(0.0, 8.0) as f32;
            let Some(i) = index_of(g, int(name, args, 0)?) else { return Ok(Value::Unit) };
            // Taken out so the world can be read while it moves.
            let mut body = core::mem::replace(&mut data(g).bodies[i].1, Player::new([0.0; 3]));
            if g.world.is_loaded(floord(body.pos[0]), floord(body.pos[2])) {
                body.flying = flying;
                body.speed_scale = speed;
                let world = &g.world;
                body.update(dt, &input, &|x, y, z| world.get_for_physics(x, y, z));
            }
            data(g).bodies[i].1 = body;
            Ok(Value::Unit)
        }
        "body_pos" => {
            want(name, args, 1)?;
            let p = index_of(g, int(name, args, 0)?).map_or([0.0; 3], |i| data(g).bodies[i].1.pos);
            Ok(xyz(p[0], p[1], p[2]))
        }
        "body_set_pos" => {
            want(name, args, 4)?;
            let at = [finite(name, args, 1)?, finite(name, args, 2)?.clamp(-64.0, crate::chunk::CH as f64 + 64.0), finite(name, args, 3)?];
            if let (Some(i), true) = (index_of(g, int(name, args, 0)?), fabs(at[0]) < 3.0e7 && fabs(at[2]) < 3.0e7) {
                data(g).bodies[i].1.pos = at;
            }
            Ok(Value::Unit)
        }
        "body_vel" => {
            want(name, args, 1)?;
            let v = index_of(g, int(name, args, 0)?).map_or([0.0; 3], |i| data(g).bodies[i].1.vel);
            Ok(xyz(v[0] as f64, v[1] as f64, v[2] as f64))
        }
        "body_set_vel" => {
            want(name, args, 4)?;
            let v = [finite(name, args, 1)?.clamp(-100.0, 100.0) as f32, finite(name, args, 2)?.clamp(-100.0, 100.0) as f32, finite(name, args, 3)?.clamp(-100.0, 100.0) as f32];
            if let Some(i) = index_of(g, int(name, args, 0)?) {
                data(g).bodies[i].1.vel = v;
            }
            Ok(Value::Unit)
        }
        "body_yaw" => {
            want(name, args, 1)?;
            Ok(Value::Float(index_of(g, int(name, args, 0)?).map_or(0.0, |i| data(g).bodies[i].1.yaw as f64)))
        }
        "body_set_yaw" => {
            want(name, args, 2)?;
            let yaw = finite(name, args, 1)? as f32;
            if let Some(i) = index_of(g, int(name, args, 0)?) {
                data(g).bodies[i].1.yaw = yaw;
            }
            Ok(Value::Unit)
        }
        "body_state" => {
            want(name, args, 1)?;
            let (ground, water) = index_of(g, int(name, args, 0)?).map_or((false, false), |i| {
                let b = &data(g).bodies[i].1;
                (b.on_ground, b.in_water)
            });
            Ok(Value::tuple(alloc::vec![Value::Bool(ground), Value::Bool(water)]))
        }
        "body_ray" => {
            want(name, args, 8)?;
            let o = [finite(name, args, 1)?, finite(name, args, 2)?, finite(name, args, 3)?];
            let d = [finite(name, args, 4)?, finite(name, args, 5)?, finite(name, args, 6)?];
            let reach = finite(name, args, 7)?.clamp(0.0, 50.0);
            let Some(i) = index_of(g, int(name, args, 0)?) else { return Ok(Value::Float(-1.0)) };
            let (lo, hi) = data(g).bodies[i].1.aabb();
            let (mut t0, mut t1) = (0.0f64, reach);
            for a in 0..3 {
                if d[a] > -1e-9 && d[a] < 1e-9 {
                    if o[a] < lo[a] || o[a] > hi[a] {
                        return Ok(Value::Float(-1.0));
                    }
                    continue;
                }
                let (mut a0, mut a1) = ((lo[a] - o[a]) / d[a], (hi[a] - o[a]) / d[a]);
                if a0 > a1 {
                    core::mem::swap(&mut a0, &mut a1);
                }
                t0 = t0.max(a0);
                t1 = t1.min(a1);
            }
            // (The direction is taken as a unit vector by the caller.)
            Ok(Value::Float(if t0 <= t1 { t0 } else { -1.0 }))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
