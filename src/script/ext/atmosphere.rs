//! The look of the air: a fog over the world (`set_fog`), soft translucent
//! specks (`spawn_soft`) and a tint over the whole screen (`hud_tint`). A mod's
//! own drawing comes after the engine has set its fog, so for darkness that
//! blends with the sky the mod says what it wants and the renderer folds it in
//! at the next world pass; all the limits are in `game/fx/fog.rs` and
//! `game/fx/soft.rs` (the tint is bounded here and drawn by the HUD).

use super::args::{finite, int, text, want};
use super::data::data;
use super::Ctx;
use crate::game::fx::{self, quads::Blend, soft};
use crate::render::Renderer;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("set_fog(start: f64, end: f64, r: i64, g: i64, b: i64, density: f64) -> bool", "hold a fog over the world: it is mixed with the dimension's own fog by `density` (0 to 1; 0 gives the usual fog back) at the next world pass, `start` and `end` in blocks (0 to 512, the end at least a block past the start), the colour 0 to 255 each. It goes half a second after the last call, so call it every frame while it should last; nothing is drawn in water or lava (they have their own). false for a number that is not finite"),
    ("hud_tint(r: i64, g: i64, b: i64, a: i64) -> bool", "colour the whole screen this frame, over the world and under the HUD's own pictures: the colour 0 to 255 and `a` the opacity (0 to 255). Draw it again every frame; the last call of a frame is the one that shows"),
    ("spawn_soft(x: f64, y: f64, z: f64, vx: f64, vy: f64, vz: f64, life: f64, size0: f64, size1: f64, r: i64, g: i64, b: i64, a0: i64, a1: i64, blend: &str) -> bool", "make one soft round speck that flies straight at (vx, vy, vz) blocks a second (up to 40 each way) for `life` seconds (0.05 to 10); its half width goes from `size0` to `size1` blocks (0 to 8) and its opacity from `a0` to `a1` (0 to 255) over its life, tinted by the colour, `blend` \"alpha\" (laid over the view) or \"additive\" (glows). At most 512 alive and 256 for a mod; none when the place is over 96 blocks from the player or the frame is slow (as `spawn_particles`); false when none was made"),
];

fn byte(name: &str, args: &[Value], i: usize) -> Result<u8, String> {
    Ok(int(name, args, i)?.clamp(0, 255) as u8)
}

pub fn call(g: &mut Session, ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "set_fog" => {
            want(name, args, 6)?;
            let (start, end) = (super::args::num(name, args, 0)?, super::args::num(name, args, 1)?);
            let rgb = [byte(name, args, 2)?, byte(name, args, 3)?, byte(name, args, 4)?];
            let density = super::args::num(name, args, 5)?;
            Ok(Value::Bool(fx::state(g).fog.set(start, end, rgb, density)))
        }
        "hud_tint" => {
            want(name, args, 4)?;
            let rgba = [byte(name, args, 0)?, byte(name, args, 1)?, byte(name, args, 2)?, byte(name, args, 3)?];
            data(g).tint = if rgba[3] == 0 { None } else { Some(rgba) };
            Ok(Value::Bool(true))
        }
        "spawn_soft" => {
            want(name, args, 15)?;
            let blend_name = text(name, args, 14)?;
            let Some(blend) = Blend::from_name(blend_name) else {
                return Err(format!("`spawn_soft`: the blend mode `{}` should be \"alpha\" or \"additive\"", blend_name));
            };
            let req = soft::Request {
                at: [finite(name, args, 0)?, finite(name, args, 1)?, finite(name, args, 2)?],
                vel: [finite(name, args, 3)?, finite(name, args, 4)?, finite(name, args, 5)?],
                life: finite(name, args, 6)?,
                size: (finite(name, args, 7)?, finite(name, args, 8)?),
                rgb: [byte(name, args, 9)?, byte(name, args, 10)?, byte(name, args, 11)?],
                alpha: (int(name, args, 12)?, int(name, args, 13)?),
                additive: blend == Blend::Additive,
            };
            match soft::spawn(g, &ctx.mod_name, &req) {
                Ok(made) => Ok(Value::Bool(made)),
                Err(_) => Err(String::from("`spawn_soft`: the numbers must be finite and the life above 0")),
            }
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}

/// The game's side: the screen tint a script asked for this frame, over the
/// world and before the interface.
pub fn draw_tint(g: &Session, r: &mut Renderer, w: f32, h: f32) {
    let Some(d) = g.ext_ref::<super::ScriptData>(super::KEY) else { return };
    if let Some(t) = d.tint {
        r.rect(0.0, 0.0, w, h, t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_names_the_three_calls() {
        let names: alloc::vec::Vec<&str> = FUNCTIONS.iter().map(|f| super::super::name_of(f.0)).collect();
        assert_eq!(names, ["set_fog", "hud_tint", "spawn_soft"]);
        assert_eq!(super::super::arity_of(FUNCTIONS[0].0), 6);
        assert_eq!(super::super::arity_of(FUNCTIONS[1].0), 4);
        assert_eq!(super::super::arity_of(FUNCTIONS[2].0), 15);
    }

    #[test]
    fn bytes_are_held_to_a_colour() {
        let args = [Value::Int(-5), Value::Int(300)];
        assert_eq!(byte("f", &args, 0).unwrap(), 0);
        assert_eq!(byte("f", &args, 1).unwrap(), 255);
        assert!(byte("f", &args, 2).is_err());
    }
}
