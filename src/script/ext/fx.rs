//! Effects: dropped items, explosions with flags, flying blocks, particles,
//! translucent quads and the sky of a dimension. All of the work (and the
//! limits) is in `game/fx/`; this file reads the arguments, checks them and
//! calls it. A call with a bad argument is an error (it switches that script
//! off, as everywhere); a call that is merely refused (a limit, not allowed on
//! a server) answers `false` or 0.
//!
//! `explode(x, y, z, power, flags)` is the basic call `explode` with a fifth
//! argument: it is routed from `host.rs` to [`explode_with_flags`].

use super::args::{finite, int, num, text, want};
use super::Ctx;
use crate::block;
use crate::game::fx::{self, blast, drops, falling, particles, quads, sky};
use crate::script::interp::{Interp, Res};
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("drop_item(x: f64, y: f64, z: f64, item: &str, count: i64, vx: f64, vy: f64, vz: f64) -> bool", "set an item down in the world with a velocity (blocks a second, up to 40 each way): it falls and can be picked up (`on_item_pickup`); `count` is held to 1 to 64; false if there is no such item, 128 already lie about, 32 were dropped this frame, or it is not allowed here"),
    ("spawn_falling_block(x: f64, y: f64, z: f64, block_id: &str, vx: f64, vy: f64, vz: f64) -> bool", "throw a block as a small cube with its picture: it falls under gravity and lands as the block if the cell it comes to rest in is free, else it breaks to dust; at most 64 at once; false if there is no such block, 64 fly already, or it is not allowed here"),
    ("spawn_particles(kind: &str, x: f64, y: f64, z: f64, count: i64, spread: f64, speed: f64, r: i64, g: i64, b: i64) -> i64", "make particles of a kind (spark, smoke, flame, bubble, dust, magic, heart, cloud) in a ball of radius `spread` (0 to 16) round a place, flying out at up to `speed` blocks a second (0 to 30), tinted by the colour; at most 64 a call; returns how many were made (fewer when 2000 are alive, none when the frame is slow or the place is over 96 blocks from the player)"),
    ("quad_create(id: i64, texture: &str, c0: [f64; 3], c1: [f64; 3], c2: [f64; 3], c3: [f64; 3], r: i64, g: i64, b: i64, a: i64, blend: &str) -> bool", "keep a translucent quad in the world until it is removed: four corners in order round it, the picture (`block:<id>`, the name of a picture the mod brought, or \"\" for a plain colour), a colour and opacity 0 to 255, and `\"alpha\"` or `\"additive\"`; the same id again replaces it; at most 256 a mod; false when that is used up"),
    ("quad_move(id: i64, c0: [f64; 3], c1: [f64; 3], c2: [f64; 3], c3: [f64; 3]) -> bool", "give one of this mod's quads new corners; false if it has none with that id"),
    ("quad_tint(id: i64, r: i64, g: i64, b: i64, a: i64) -> bool", "change the colour and opacity of one of this mod's quads; false if it has none with that id"),
    ("quad_remove(id: i64) -> bool", "take one of this mod's quads away; false if it has none with that id"),
    ("set_dimension_sky(dim: &str, key: &str, value: &str) -> bool", "change the sky of a dimension (\"\" or \"home\" is the home world) while playing: keys sky_colour, fog_colour, fog_distance, stars, clouds, sun, moon, ambient_light, time (see DATA.md); the value may also be a number, a bool or an (r, g, b) tuple; false if there is no such dimension; an error for an unknown key or a bad value"),
];

/// Whether `n` is a number of arguments a function takes besides the one its
/// table row says (for the lint): `explode` takes flags as a fifth.
pub fn extra_arity(name: &str, n: usize) -> bool {
    name == "explode" && n == 5
}

/// A point from `(x, y, z)` or `[x, y, z]`.
fn point(name: &str, args: &[Value], i: usize) -> Result<[f64; 3], String> {
    let list = match args.get(i) {
        Some(Value::Tuple(l)) | Some(Value::Vec(l)) => l,
        Some(other) => return Err(format!("`{}`: argument {} should be a point (x, y, z), found {}", name, i + 1, other.type_name())),
        None => return Err(format!("`{}` is missing argument {}", name, i + 1)),
    };
    if list.len() != 3 {
        return Err(format!("`{}`: argument {} should have 3 numbers, it has {}", name, i + 1, list.len()));
    }
    let mut out = [0.0; 3];
    for (k, v) in list.iter().enumerate() {
        out[k] = match v {
            Value::Float(f) => *f,
            Value::Int(n) => *n as f64,
            other => return Err(format!("`{}`: argument {} should hold numbers, found {}", name, i + 1, other.type_name())),
        };
        if !out[k].is_finite() {
            return Err(format!("`{}`: argument {} is not a finite point", name, i + 1));
        }
    }
    Ok(out)
}

fn byte(name: &str, args: &[Value], i: usize) -> Result<u8, String> {
    Ok(int(name, args, i)?.clamp(0, 255) as u8)
}

/// What a script passed as the value of a sky property, as text.
fn value_text(name: &str, args: &[Value], i: usize) -> Result<String, String> {
    match args.get(i) {
        Some(Value::Str(s)) => Ok(String::from(s.as_str())),
        Some(Value::Int(n)) => Ok(format!("{}", n)),
        Some(Value::Float(f)) if f.is_finite() => Ok(format!("{}", f)),
        Some(Value::Float(_)) => Err(format!("`{}`: argument {} is not a finite number", name, i + 1)),
        Some(Value::Bool(b)) => Ok(String::from(if *b { "yes" } else { "no" })),
        Some(Value::Tuple(_)) | Some(Value::Vec(_)) => {
            let p = point(name, args, i)?;
            let c = |v: f64| v.clamp(0.0, 255.0) as i64;
            Ok(format!("{},{},{}", c(p[0]), c(p[1]), c(p[2])))
        }
        Some(other) => Err(format!("`{}`: argument {} should be text, a number, a bool or a colour, found {}", name, i + 1, other.type_name())),
        None => Err(format!("`{}` is missing argument {}", name, i + 1)),
    }
}

/// `explode(x, y, z, power, flags)`: the number of blocks destroyed.
fn explode_call(g: &mut Session, args: &[Value]) -> Result<Value, String> {
    let name = "explode";
    want(name, args, 5)?;
    let at = [finite(name, args, 0)?, finite(name, args, 1)?, finite(name, args, 2)?];
    let power = num(name, args, 3)?;
    let flags = match args.get(4) {
        Some(Value::Int(bits)) => blast::Flags::from_bits(*bits)?,
        Some(Value::Str(s)) => blast::Flags::parse(s)?,
        Some(other) => return Err(format!("`explode`: argument 5 should be flags (text like \"fire, silent\" or a number), found {}", other.type_name())),
        None => blast::Flags::NONE,
    };
    Ok(Value::Int(blast::run(g, at, power, flags) as i64))
}

/// The five-argument `explode`, routed here from `host.rs`.
pub(crate) fn explode_with_flags(it: &mut Interp, args: &[Value]) -> Res {
    let r = match it.host.session() {
        None => Err(String::from("`explode` with flags works only in a running game")),
        Some(g) => explode_call(g, args),
    };
    match r {
        Ok(v) => Ok(v),
        Err(m) => it.fail(m),
    }
}

/// A texture name a quad may use: valid text, and for `block:<id>` a block that exists.
fn texture_ok(name: &str, tex: &str) -> Result<(), String> {
    if !quads::name_ok(tex) {
        return Err(format!("`{}`: the picture name `{}` is not allowed (letters, digits and _ - : . only, up to {})", name, tex, quads::MAX_NAME));
    }
    if let Some(id) = tex.strip_prefix("block:") {
        if block::reg().block(id).is_none() {
            return Err(format!("`{}`: there is no block `{}`", name, id));
        }
    }
    Ok(())
}

fn corners(name: &str, args: &[Value], first: usize) -> Result<[[f64; 3]; 4], String> {
    let c = [point(name, args, first)?, point(name, args, first + 1)?, point(name, args, first + 2)?, point(name, args, first + 3)?];
    if !quads::corners_ok(&c) {
        return Err(format!("`{}`: the corners are too far apart (over {} blocks) or outside the world", name, quads::MAX_SIZE));
    }
    Ok(c)
}

pub fn call(g: &mut Session, ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "drop_item" => {
            want(name, args, 8)?;
            let at = [finite(name, args, 0)?, finite(name, args, 1)?, finite(name, args, 2)?];
            let id = text(name, args, 3)?;
            let count = int(name, args, 4)?;
            let vel = [finite(name, args, 5)?, finite(name, args, 6)?, finite(name, args, 7)?];
            let Some(item) = block::reg().items.iter().position(|it| it.id == id) else { return Ok(Value::Bool(false)) };
            Ok(Value::Bool(drops::spawn(g, at, vel, item, count)))
        }
        "spawn_falling_block" => {
            want(name, args, 7)?;
            let at = [finite(name, args, 0)?, finite(name, args, 1)?, finite(name, args, 2)?];
            let id = text(name, args, 3)?;
            let vel = [finite(name, args, 4)?, finite(name, args, 5)?, finite(name, args, 6)?];
            let Some(b) = block::reg().block(id) else { return Ok(Value::Bool(false)) };
            if !fx::may_change(g) || at[0] < -quads::EDGE || at[0] > quads::EDGE || at[2] < -quads::EDGE || at[2] > quads::EDGE {
                return Ok(Value::Bool(false));
            }
            let at = [at[0], at[1].clamp(-8.0, crate::chunk::CH as f64 + 64.0), at[2]];
            let v = [vel[0].clamp(-60.0, 60.0) as f32, vel[1].clamp(-60.0, 60.0) as f32, vel[2].clamp(-60.0, 60.0) as f32];
            Ok(Value::Bool(falling::launch(&mut fx::state(g).falling, b, at, v)))
        }
        "spawn_particles" => {
            want(name, args, 10)?;
            let kind_name = text(name, args, 0)?;
            let Some(kind) = particles::Kind::from_name(kind_name) else {
                let known: Vec<&str> = particles::Kind::ALL.iter().map(|k| k.name()).collect();
                return Err(format!("`spawn_particles`: no particle kind `{}` (known: {})", kind_name, known.join(", ")));
            };
            let req = particles::Request {
                kind,
                at: [finite(name, args, 1)?, finite(name, args, 2)?, finite(name, args, 3)?],
                count: int(name, args, 4)?,
                spread: finite(name, args, 5)?,
                speed: finite(name, args, 6)?,
                colour: [byte(name, args, 7)?, byte(name, args, 8)?, byte(name, args, 9)?],
            };
            Ok(Value::Int(particles::spawn(g, &req) as i64))
        }
        "quad_create" => {
            want(name, args, 11)?;
            let id = int(name, args, 0)?;
            let tex = text(name, args, 1)?;
            texture_ok(name, tex)?;
            let c = corners(name, args, 2)?;
            let rgba = [byte(name, args, 6)?, byte(name, args, 7)?, byte(name, args, 8)?, byte(name, args, 9)?];
            let blend_name = text(name, args, 10)?;
            let Some(blend) = quads::Blend::from_name(blend_name) else {
                return Err(format!("`quad_create`: the blend mode `{}` should be \"alpha\" or \"additive\"", blend_name));
            };
            let q = quads::FxQuad { owner: ctx.mod_name.clone(), id, tex: String::from(tex), corners: c, rgba, blend };
            Ok(Value::Bool(fx::state(g).quads.create(q).is_ok()))
        }
        "quad_move" => {
            want(name, args, 5)?;
            let id = int(name, args, 0)?;
            let c = corners(name, args, 1)?;
            Ok(Value::Bool(fx::state(g).quads.move_to(&ctx.mod_name, id, c)))
        }
        "quad_tint" => {
            want(name, args, 5)?;
            let id = int(name, args, 0)?;
            let rgba = [byte(name, args, 1)?, byte(name, args, 2)?, byte(name, args, 3)?, byte(name, args, 4)?];
            Ok(Value::Bool(fx::state(g).quads.tint(&ctx.mod_name, id, rgba)))
        }
        "quad_remove" => {
            want(name, args, 1)?;
            let id = int(name, args, 0)?;
            Ok(Value::Bool(fx::state(g).quads.remove(&ctx.mod_name, id)))
        }
        "set_dimension_sky" => {
            want(name, args, 3)?;
            let dim = text(name, args, 0)?;
            let key = text(name, args, 1)?;
            let value = value_text(name, args, 2)?;
            match sky::set(g, dim, key, &value) {
                Ok(found) => Ok(Value::Bool(found)),
                Err(e) => Err(format!("`set_dimension_sky`: {}", e)),
            }
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::host::call_host_fn;
    use crate::script::host::mock::MockHost;
    use crate::script::limits::Limits;
    use crate::script::state::ScriptState;
    use crate::script::Script;

    fn p(x: f64, y: f64, z: f64) -> Value {
        Value::tuple(alloc::vec![Value::Float(x), Value::Float(y), Value::Float(z)])
    }

    #[test]
    fn points_come_from_tuples_and_lists() {
        assert_eq!(point("f", &[p(1.0, 2.0, 3.0)], 0).unwrap(), [1.0, 2.0, 3.0]);
        let v = Value::vec(alloc::vec![Value::Int(1), Value::Float(2.5), Value::Int(-3)]);
        assert_eq!(point("f", &[v], 0).unwrap(), [1.0, 2.5, -3.0]);
        assert!(point("f", &[Value::Int(1)], 0).is_err());
        assert!(point("f", &[], 0).is_err());
        assert!(point("f", &[Value::tuple(alloc::vec![Value::Int(1), Value::Int(2)])], 0).is_err());
        assert!(point("f", &[Value::tuple(alloc::vec![Value::Int(1), Value::Int(2), Value::str("z")])], 0).is_err());
        assert!(point("f", &[p(f64::NAN, 0.0, 0.0)], 0).is_err());
        assert!(point("f", &[p(0.0, f64::INFINITY, 0.0)], 0).is_err());
    }

    #[test]
    fn corners_must_be_close_enough() {
        let ok = [p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(1.0, 1.0, 0.0), p(0.0, 1.0, 0.0)];
        assert!(corners("quad_move", &ok, 0).is_ok());
        let far = [p(0.0, 0.0, 0.0), p(5000.0, 0.0, 0.0), p(1.0, 1.0, 0.0), p(0.0, 1.0, 0.0)];
        assert!(corners("quad_move", &far, 0).is_err());
        assert!(corners("quad_move", &ok[..3], 0).is_err());
    }

    #[test]
    fn a_sky_value_may_be_text_a_number_a_bool_or_a_colour() {
        assert_eq!(value_text("f", &[Value::str("#fff")], 0).unwrap(), "#fff");
        assert_eq!(value_text("f", &[Value::Int(8)], 0).unwrap(), "8");
        assert_eq!(value_text("f", &[Value::Float(0.5)], 0).unwrap(), "0.5");
        assert_eq!(value_text("f", &[Value::Bool(true)], 0).unwrap(), "yes");
        assert_eq!(value_text("f", &[Value::Bool(false)], 0).unwrap(), "no");
        assert_eq!(value_text("f", &[p(255.0, 10.0, 300.0)], 0).unwrap(), "255,10,255");
        assert!(value_text("f", &[Value::Float(f64::NAN)], 0).is_err());
        assert!(value_text("f", &[Value::Unit], 0).is_err());
        assert!(value_text("f", &[], 0).is_err());
    }

    #[test]
    fn textures_are_checked() {
        assert!(texture_ok("quad_create", "").is_ok());
        assert!(texture_ok("quad_create", "storm_cloud").is_ok());
        assert!(texture_ok("quad_create", "../x").is_err());
        assert!(texture_ok("quad_create", "block:no_such_block_anywhere").is_err());
    }

    #[test]
    fn explode_takes_a_fifth_argument_and_nothing_else_does() {
        assert!(extra_arity("explode", 5));
        assert!(!extra_arity("explode", 6));
        assert!(!extra_arity("explode", 3));
        assert!(!extra_arity("give", 5));
    }

    #[test]
    fn the_lint_knows_the_new_calls_and_their_arity() {
        let good = "fn f() { explode(1.0, 2.0, 3.0, 4.0); explode(1.0, 2.0, 3.0, 4.0, \"fire\"); quad_remove(3); drop_item(0.0, 0.0, 0.0, \"coal\", 1, 0.0, 1.0, 0.0); }";
        let prog = Script::load("t.rs", good).unwrap().prog;
        assert!(crate::script::lint::lint(&prog).is_empty(), "{:?}", crate::script::lint::lint(&prog));
        for bad in ["fn f() { explode(1.0, 2.0, 3.0); }", "fn f() { explode(1.0, 2.0, 3.0, 4.0, 5, 6); }", "fn f() { quad_remove(); }", "fn f() { spawn_particles(\"spark\", 0.0, 0.0, 0.0); }", "fn f() { set_dimension_sky(\"a\", \"b\"); }"] {
            let prog = Script::load("t.rs", bad).unwrap().prog;
            assert_eq!(crate::script::lint::lint(&prog).len(), 1, "{}", bad);
        }
    }

    #[test]
    fn without_a_game_every_call_fails_politely() {
        let script = Script::load("t.rs", "fn t() {}").unwrap();
        let Script { prog, mut consts, .. } = script;
        let mut state = ScriptState::default();
        let mut host = MockHost::default();
        let mut it = Interp::new(&prog, &mut consts, &mut host, &mut state, "demo", Limits::DEFAULT);
        let args = [Value::Float(0.0), Value::Float(0.0), Value::Float(0.0), Value::Float(2.0), Value::str("fire")];
        assert!(matches!(call_host_fn(&mut it, "explode", &args), Some(Err(_))));
        // The four-argument explosion still works with the mock.
        assert!(matches!(call_host_fn(&mut it, "explode", &args[..4]), Some(Ok(_))));
        for sig in FUNCTIONS {
            assert!(matches!(call_host_fn(&mut it, crate::script::ext::name_of(sig.0), &[]), Some(Err(_))), "{}", sig.0);
        }
    }
}
