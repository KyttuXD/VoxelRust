//! The player: where they look, what they press, being held in place
//! (riding), sleeping, the screen flash, what they aim at, who they are.

use super::args::{finite, num, want, xyz};
use super::data::data;
use super::Ctx;
use crate::script::numeric::fabs;
use crate::script::value::Value;
use crate::session::{GameMode, Session};
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("player_look() -> (f64, f64)", "(yaw, pitch) in radians; yaw 0 faces -z, positive pitch looks up"),
    ("look_dir() -> (f64, f64, f64)", "the unit vector the player looks along"),
    ("set_look(yaw: f64, pitch: f64)", "turn the player's view (pitch is kept within 1.5 radians of level)"),
    ("player_eye() -> (f64, f64, f64)", "where the player's eyes are"),
    ("place_player(x: f64, y: f64, z: f64)", "put the player at a place (the feet), stopped"),
    ("hold_player(x: f64, y: f64, z: f64)", "carry the player along: for this frame their eyes are at that place and they do not walk (set it again every frame, from `on_update`; this is how riding works)"),
    ("player_mode() -> String", "`survival` or `creative`"),
    ("player_dead() -> bool", "whether the player is dead (on the death screen)"),
    ("player_sleeping() -> bool", "whether the player is in bed"),
    ("wake()", "get the player out of bed"),
    ("flash(secs: f64)", "flash the screen white for a moment (up to 1 second)"),
    ("shake(amount: f64)", "shake the camera (0 to 3)"),
    ("input() -> (f64, f64, bool, bool, bool, bool, bool)", "what the player presses this frame, in `on_update` and `on_interact`: (forward -1..1, strafe -1..1, jump, descend, leave, use, attack)"),
    ("aim() -> (i64, i64, i64, i64, i64, i64, f64)", "the block the player looks at: (x, y, z, x, y, z of the cell in front of it, distance); the distance is -1 when nothing is in reach"),
    ("use_ready() -> bool", "whether the use button's cooldown is over (it is set again by `use_cooldown`)"),
    ("use_cooldown(secs: f64)", "start the use button's cooldown"),
    ("attack_ready() -> bool", "whether the attack button's cooldown is over"),
    ("attack_cooldown(secs: f64)", "start the attack button's cooldown"),
    ("take_held() -> bool", "use up one of the item in hand (nothing is used up in creative); false if the hand is empty"),
    ("who() -> String", "the name the player goes by"),
    ("is_host() -> bool", "whether this copy of the game runs the creatures and the like (always true alone; one player in a shared game)"),
    ("is_shared() -> bool", "whether this is a shared game (a server)"),
];

pub fn call(g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "player_look" => {
            want(name, args, 0)?;
            Ok(Value::tuple(alloc::vec![Value::Float(g.player.yaw as f64), Value::Float(g.player.pitch as f64)]))
        }
        "look_dir" => {
            want(name, args, 0)?;
            let d = crate::math::forward(g.player.yaw, g.player.pitch);
            Ok(xyz(d.x as f64, d.y as f64, d.z as f64))
        }
        "set_look" => {
            want(name, args, 2)?;
            let (yaw, pitch) = (finite(name, args, 0)?, finite(name, args, 1)?);
            g.player.yaw = yaw as f32;
            g.player.pitch = pitch.clamp(-1.5, 1.5) as f32;
            Ok(Value::Unit)
        }
        "player_eye" => {
            want(name, args, 0)?;
            let e = g.player.eye();
            Ok(xyz(e[0], e[1], e[2]))
        }
        "place_player" => {
            want(name, args, 3)?;
            let at = [finite(name, args, 0)?, finite(name, args, 1)?, finite(name, args, 2)?];
            if fabs(at[0]) < 3.0e7 && fabs(at[2]) < 3.0e7 {
                g.player.pos = [at[0], at[1].clamp(-64.0, crate::chunk::CH as f64 + 64.0), at[2]];
                g.player.vel = [0.0; 3];
            }
            Ok(Value::Unit)
        }
        "hold_player" => {
            want(name, args, 3)?;
            g.seat = Some([finite(name, args, 0)?, finite(name, args, 1)?, finite(name, args, 2)?]);
            Ok(Value::Unit)
        }
        "player_mode" => {
            want(name, args, 0)?;
            Ok(Value::str(if g.mode == GameMode::Survival { "survival" } else { "creative" }))
        }
        "player_dead" => {
            want(name, args, 0)?;
            Ok(Value::Bool(g.dead))
        }
        "player_sleeping" => {
            want(name, args, 0)?;
            Ok(Value::Bool(g.sleeping))
        }
        "wake" => {
            want(name, args, 0)?;
            g.sleeping = false;
            Ok(Value::Unit)
        }
        "flash" => {
            want(name, args, 1)?;
            g.flash = num(name, args, 0)?.clamp(0.0, 1.0) as f32;
            Ok(Value::Unit)
        }
        "shake" => {
            want(name, args, 1)?;
            g.shake = num(name, args, 0)?.clamp(0.0, 3.0) as f32;
            Ok(Value::Unit)
        }
        "input" => {
            want(name, args, 0)?;
            let c = data(g).controls;
            Ok(Value::tuple(alloc::vec![
                Value::Float(c.input.forward as f64),
                Value::Float(c.input.strafe as f64),
                Value::Bool(c.input.jump),
                Value::Bool(c.input.descend),
                Value::Bool(c.leave),
                Value::Bool(c.place_held),
                Value::Bool(c.break_held),
            ]))
        }
        "aim" => {
            want(name, args, 0)?;
            Ok(match &g.target {
                Some(h) => Value::tuple(alloc::vec![
                    Value::Int(h.cell[0] as i64),
                    Value::Int(h.cell[1] as i64),
                    Value::Int(h.cell[2] as i64),
                    Value::Int(h.prev[0] as i64),
                    Value::Int(h.prev[1] as i64),
                    Value::Int(h.prev[2] as i64),
                    Value::Float(h.dist as f64),
                ]),
                None => Value::tuple(alloc::vec![Value::Int(0), Value::Int(0), Value::Int(0), Value::Int(0), Value::Int(0), Value::Int(0), Value::Float(-1.0)]),
            })
        }
        "use_ready" => {
            want(name, args, 0)?;
            Ok(Value::Bool(g.cooldown[1] <= 0.0))
        }
        "use_cooldown" => {
            want(name, args, 1)?;
            g.cooldown[1] = num(name, args, 0)?.clamp(0.0, 10.0) as f32;
            Ok(Value::Unit)
        }
        "attack_ready" => {
            want(name, args, 0)?;
            Ok(Value::Bool(g.cooldown[0] <= 0.0))
        }
        "attack_cooldown" => {
            want(name, args, 1)?;
            g.cooldown[0] = num(name, args, 0)?.clamp(0.0, 10.0) as f32;
            Ok(Value::Unit)
        }
        "take_held" => {
            want(name, args, 0)?;
            if g.held().is_none() {
                return Ok(Value::Bool(false));
            }
            if g.mode != GameMode::Creative {
                let slot = g.selected;
                g.inv.take_one(slot);
            }
            Ok(Value::Bool(true))
        }
        "who" => {
            want(name, args, 0)?;
            Ok(Value::string(g.who.clone()))
        }
        "is_host" => {
            want(name, args, 0)?;
            Ok(Value::Bool(g.net.is_none() || g.runs_the_world()))
        }
        "is_shared" => {
            want(name, args, 0)?;
            Ok(Value::Bool(g.net.is_some()))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
