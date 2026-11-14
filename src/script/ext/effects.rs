//! Things that happen in the world and to bodies: shots in flight, a storm in
//! the sky, a push. For mods whose creatures attack from afar or throw
//! things about. Refused in a shared game by a player who does not run the
//! world, as `set_block` is.

use super::args::{finite, flag, int, num, want};
use super::Ctx;
use crate::block;
use crate::script::numeric::fabs;
use crate::script::value::Value;
use crate::session::{Session, Shot};
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("shoot(x: f64, y: f64, z: f64, dx: f64, dy: f64, dz: f64, speed: f64, damage: i64, hostile: bool, r: i64, g: i64, b: i64) -> bool", "send a shot flying from a place along a direction (blocks a second, up to 60; it hurts what it hits for `damage`, up to 100, and the player too when `hostile`), drawn in the colour; false if there are too many in flight (200) or it is not allowed here"),
    ("force_sky(rain: f64)", "force the sky to a fixed weather, 0 (clear) to 1 (storm), as `/weather` does; a negative number gives the sky back to the weather"),
    ("mob_set_vel(id: i64, x: f64, y: f64, z: f64)", "set a creature's velocity in blocks a second (up to 60 in each direction): a push or a leap"),
    ("clear_effect(id: &str) -> bool", "take a status effect off the player (a cure); false if they did not have it"),
    ("push_player(x: f64, y: f64, z: f64)", "add to the player's velocity in blocks a second (up to 40 in each direction)"),
];

fn may_change(g: &Session) -> bool {
    g.net.is_none() || g.runs_the_world()
}

pub fn call(g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "shoot" => {
            want(name, args, 12)?;
            let at = [finite(name, args, 0)?, finite(name, args, 1)?, finite(name, args, 2)?];
            let d = [finite(name, args, 3)?, finite(name, args, 4)?, finite(name, args, 5)?];
            let speed = num(name, args, 6)?.clamp(0.0, 60.0);
            let damage = int(name, args, 7)?.clamp(0, 100) as i32;
            let hostile = flag(name, args, 8)?;
            let col = [int(name, args, 9)?.clamp(0, 255) as u8, int(name, args, 10)?.clamp(0, 255) as u8, int(name, args, 11)?.clamp(0, 255) as u8];
            let len = crate::script::numeric::sqrt(d[0] * d[0] + d[1] * d[1] + d[2] * d[2]);
            if !may_change(g) || g.shots.len() >= 200 || len < 1e-9 || fabs(at[0]) > 3.0e7 || fabs(at[2]) > 3.0e7 {
                return Ok(Value::Bool(false));
            }
            let k = speed / len;
            g.shots.push(Shot { pos: at, vel: [(d[0] * k) as f32, (d[1] * k) as f32, (d[2] * k) as f32], damage, life: 6.0, leaves: block::AIR, colour: col, hostile, shooter: None });
            Ok(Value::Bool(true))
        }
        "force_sky" => {
            want(name, args, 1)?;
            let v = num(name, args, 0)?;
            if g.net.is_none() {
                g.forced_sky = if v < 0.0 { None } else { Some(v.clamp(0.0, 1.0) as f32) };
            }
            Ok(Value::Unit)
        }
        "mob_set_vel" => {
            want(name, args, 4)?;
            let v = [finite(name, args, 1)?.clamp(-60.0, 60.0) as f32, finite(name, args, 2)?.clamp(-60.0, 60.0) as f32, finite(name, args, 3)?.clamp(-60.0, 60.0) as f32];
            let id = int(name, args, 0)?;
            if may_change(g) && id >= 0 && id <= u32::MAX as i64 {
                if let Some(m) = g.mobs.list.iter_mut().find(|m| m.id == id as u32) {
                    m.body.vel = v;
                }
            }
            Ok(Value::Unit)
        }
        "clear_effect" => {
            want(name, args, 1)?;
            let id = super::args::text(name, args, 0)?;
            Ok(Value::Bool(crate::game::effects::apply::clear(g, id)))
        }
        "push_player" => {
            want(name, args, 3)?;
            for a in 0..3 {
                let add = finite(name, args, a)?.clamp(-40.0, 40.0) as f32;
                g.player.vel[a] = (g.player.vel[a] + add).clamp(-80.0, 80.0);
            }
            Ok(Value::Unit)
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
