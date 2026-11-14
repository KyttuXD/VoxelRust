//! Pictures a script asks for: rectangles and text on the HUD, models and
//! specks in the world. A script does not draw while the frame is drawn: in
//! `on_update` (or `on_tick`) it says what should be seen, and the game
//! draws that every frame until the next `on_update` replaces it. Nothing
//! asked for in the frame before is kept.

use super::args::{finite, num, text, want};
use super::data::{data, DrawModel, HudRect, HudText, MAX_MODELS, MAX_RECTS, MAX_SPECKS, MAX_TEXTS};
use super::Ctx;
use crate::block;
use crate::game::hooks::HudSink;
use crate::render::Renderer;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("hud_frac_rect(x: f64, y: f64, w: f64, h: f64, r: i64, g: i64, b: i64, a: i64)", "fill a rectangle of the HUD this frame; x, y, w and h are fractions of the screen (0 to 1), the colour and opacity 0 to 255; at most 64 a frame"),
    ("hud_frac_text(x: f64, y: f64, scale: f64, text: &str, r: i64, g: i64, b: i64)", "write text on the HUD this frame; x and y are fractions of the screen, `scale` times the size of the game's own interface (0.5 to 8); at most 32 a frame"),
    ("draw_model(model: &str, x: f64, y: f64, z: f64, yaw: f64, height: f64, phase: f64, tint: f64)", "draw a model (a model id, or the id of a creature to use its model) in the world this frame, `height` blocks tall; `phase` is the walk or wheel cycle in radians; `tint` is 0 to 1 for a fixed brightness, or -1 for the light of the place; at most 200 a frame"),
    ("draw_model_own(model: &str, x: f64, y: f64, z: f64, yaw: f64, height: f64, phase: f64, tint: f64)", "as `draw_model`, for the thing the player sits in or wears: it is drawn only when the player sees themselves from outside (third person)"),
    ("draw_model_posed(model: &str, x: f64, y: f64, z: f64, yaw: f64, height: f64, phase: f64, stride: f64, head_pitch: f64, head_yaw: f64, clock: f64, tint: f64)", "as `draw_model` with the pose of a creature: `stride` 0 (still) to 1 (full stride), where the head looks relative to the body in degrees, and `clock`, the seconds it has been alive (tails wag and ears twitch on their own)"),
    ("draw_speck(x: f64, y: f64, z: f64, r: i64, g: i64, b: i64)", "draw one small opaque speck of colour in the world this frame; at most 300 a frame"),
];

fn byte(name: &str, args: &[Value], i: usize) -> Result<u8, String> {
    Ok(crate::script::host::int(name, args, i)?.clamp(0, 255) as u8)
}

/// A model by its id, or the model of the creature with that id.
fn model_index(id: &str) -> Option<usize> {
    let r = block::reg();
    r.models.iter().position(|m| m.id == id).or_else(|| r.entities.iter().find(|e| e.id == id).map(|e| e.model))
}

pub fn call(g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "hud_frac_rect" => {
            want(name, args, 8)?;
            let rect = HudRect {
                x: finite(name, args, 0)?.clamp(-1.0, 2.0) as f32,
                y: finite(name, args, 1)?.clamp(-1.0, 2.0) as f32,
                w: finite(name, args, 2)?.clamp(0.0, 2.0) as f32,
                h: finite(name, args, 3)?.clamp(0.0, 2.0) as f32,
                col: [byte(name, args, 4)?, byte(name, args, 5)?, byte(name, args, 6)?, byte(name, args, 7)?],
            };
            let d = data(g);
            if d.rects.len() < MAX_RECTS {
                d.rects.push(rect);
            }
            Ok(Value::Unit)
        }
        "hud_frac_text" => {
            want(name, args, 7)?;
            let t = HudText {
                x: finite(name, args, 0)?.clamp(-1.0, 2.0) as f32,
                y: finite(name, args, 1)?.clamp(-1.0, 2.0) as f32,
                scale: finite(name, args, 2)?.clamp(0.5, 8.0) as f32,
                text: String::from(text(name, args, 3)?).chars().take(120).collect(),
                col: [byte(name, args, 4)?, byte(name, args, 5)?, byte(name, args, 6)?, 255],
            };
            let d = data(g);
            if d.texts.len() < MAX_TEXTS {
                d.texts.push(t);
            }
            Ok(Value::Unit)
        }
        "draw_model_posed" => {
            want(name, args, 12)?;
            let Some(model) = model_index(text(name, args, 0)?) else { return Ok(Value::Unit) };
            let m = DrawModel {
                model,
                at: [finite(name, args, 1)?, finite(name, args, 2)?, finite(name, args, 3)?],
                yaw: finite(name, args, 4)? as f32,
                height: finite(name, args, 5)?.clamp(0.05, 64.0) as f32,
                phase: finite(name, args, 6)? as f32,
                stride: finite(name, args, 7)?.clamp(0.0, 1.0) as f32,
                head_pitch: finite(name, args, 8)?.clamp(-90.0, 90.0) as f32,
                head_yaw: finite(name, args, 9)?.clamp(-90.0, 90.0) as f32,
                clock: finite(name, args, 10)?.clamp(0.0, 1.0e6) as f32,
                tint: num(name, args, 11)?.clamp(-1.0, 1.0) as f32,
                third_only: false,
            };
            let d = data(g);
            if d.models.len() < MAX_MODELS {
                d.models.push(m);
            }
            Ok(Value::Unit)
        }
        "draw_model" | "draw_model_own" => {
            want(name, args, 8)?;
            let Some(model) = model_index(text(name, args, 0)?) else { return Ok(Value::Unit) };
            let m = DrawModel {
                model,
                at: [finite(name, args, 1)?, finite(name, args, 2)?, finite(name, args, 3)?],
                yaw: finite(name, args, 4)? as f32,
                height: finite(name, args, 5)?.clamp(0.05, 64.0) as f32,
                phase: finite(name, args, 6)? as f32,
                tint: num(name, args, 7)?.clamp(-1.0, 1.0) as f32,
                third_only: name == "draw_model_own",
                stride: 0.0,
                head_pitch: 0.0,
                head_yaw: 0.0,
                clock: 0.0,
            };
            let d = data(g);
            if d.models.len() < MAX_MODELS {
                d.models.push(m);
            }
            Ok(Value::Unit)
        }
        "draw_speck" => {
            want(name, args, 6)?;
            let at = [finite(name, args, 0)?, finite(name, args, 1)?, finite(name, args, 2)?];
            let col = [byte(name, args, 3)?, byte(name, args, 4)?, byte(name, args, 5)?];
            let d = data(g);
            if d.specks.len() < MAX_SPECKS {
                d.specks.push((at, col));
            }
            Ok(Value::Unit)
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}

/// The game's side: draw what the scripts asked for into the world. `eye` is
/// the camera and `third_person` whether it is outside the player; `light` the brightness (0..1) at a place.
pub fn draw_world(g: &Session, r: &mut Renderer, eye: [f64; 3], third_person: bool, light: &dyn Fn([f64; 3]) -> f32) {
    let Some(d) = g.ext_ref::<super::ScriptData>(super::KEY) else { return };
    for m in d.models.iter().filter(|m| third_person || !m.third_only) {
        let rel = [(m.at[0] - eye[0]) as f32, (m.at[1] - eye[1]) as f32, (m.at[2] - eye[2]) as f32];
        if rel[0] * rel[0] + rel[2] * rel[2] > 96.0 * 96.0 {
            continue;
        }
        let l = if m.tint >= 0.0 { m.tint } else { light(m.at) };
        let v = (l.clamp(0.0, 1.0) * 255.0) as u8;
        let pose = crate::model::Pose { phase: m.phase, stride: m.stride, head_pitch: m.head_pitch, head_yaw: m.head_yaw, clock: m.clock, ..Default::default() };
        r.draw_model(m.model, &pose, rel, m.yaw, m.height, [v, v, v]);
    }
    if !d.specks.is_empty() {
        let sparks: Vec<([f32; 3], [u8; 3])> = d.specks.iter().map(|(at, col)| ([(at[0] - eye[0]) as f32, (at[1] - eye[1]) as f32, (at[2] - eye[2]) as f32], *col)).collect();
        r.draw_sparks(&sparks);
    }
}

/// The game's side: draw the HUD the scripts asked for.
pub fn draw_hud(g: &Session, hud: &mut dyn HudSink) {
    let Some(d) = g.ext_ref::<super::ScriptData>(super::KEY) else { return };
    if d.rects.is_empty() && d.texts.is_empty() {
        return;
    }
    let (w, h) = hud.size();
    for r in &d.rects {
        hud.rect(r.x * w, r.y * h, r.w * w, r.h * h, r.col);
    }
    for t in &d.texts {
        hud.text(t.x * w, t.y * h, t.scale, &t.text, t.col);
    }
}
