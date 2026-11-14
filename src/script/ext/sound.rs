//! Sound and music: sounds in the world, panned and pitched, held by a
//! handle so they can be stopped, faded and moved; looping sounds; and one
//! channel of streamed music. What the game does with them is in
//! `game/mod_sound.rs` (handles and limits) and `client/audio/` (the mixer);
//! the limits and an example of each call are in docs/SCRIPTING.md ("Sound and
//! music"). The older `play_sound(id)` of `script/host.rs` still works, and
//! with four arguments (`play_sound(name, volume, pitch, pan)`) it comes here.
//!
//! A call never waits for the sound device and a name that does not exist is
//! no error: the sound is not played, the call returns 0 (or false) and the
//! log says so once. Wrong argument types, numbers that are not numbers, and
//! the like, are errors, as for every host function.

use super::args::{finite, flag, int, text, want};
use super::Ctx;
use crate::game::mod_sound as sound;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("play_sound_at(name: &str, x: f64, y: f64, z: f64, volume: f64, pitch: f64) -> i64", "play a sound from a place in the world: full volume within 4 blocks, quieter and duller with the distance (inaudible past 64), from the side it is on; volume 0 to 2, pitch 0.25 to 4 (the playback rate); returns a handle (0 if the sound has none or does not exist)"),
    ("loop_sound(name: &str, volume: f64, pitch: f64, pan: f64) -> i64", "play a sound over and over until `stop_sound`, with its end crossfaded into its start; pan -1 (left) to 1 (right); returns the handle, or 0 if the mod already has 4 loops (or 8 sounds with handles) or the sound does not exist"),
    ("loop_sound_at(name: &str, x: f64, y: f64, z: f64, volume: f64, pitch: f64) -> i64", "a looping sound placed in the world (move it with `set_sound`)"),
    ("stop_sound(handle: i64)", "stop a sound of this mod: it goes quiet in a few milliseconds; a handle that is stale or not this mod's does nothing"),
    ("fade_sound(handle: i64, volume: f64, seconds: f64)", "move a sound's volume (0 to 2) to a new value over some seconds; a volume of 0 stops the sound when it gets there"),
    ("set_sound(handle: i64, volume: f64, pitch: f64, pan: f64, x: f64, y: f64, z: f64)", "change a playing sound: its volume, pitch and pan; a sound placed with a position follows `x, y, z` (and ignores `pan`), one without a position ignores them"),
    ("is_playing(handle: i64) -> bool", "whether a sound of this mod is still playing (false for a stale handle or one that is not this mod's)"),
    ("play_music(name: &str, volume: f64, fade_in: f64, looping: bool) -> bool", "start a music track from the mod's `music/` (or `sounds/`) folder, streamed (Ogg Vorbis, MP3 or WAV); a track playing already crossfades into it over `fade_in` seconds (at least a quarter of one); the same name again only changes its volume; there is one music channel per world; false if the file is not there or cannot be read"),
    ("stop_music(fade_out: f64)", "fade this mod's music out over some seconds (0 is at once)"),
    ("set_music_volume(volume: f64)", "set the volume (0 to 1) of this mod's music; the player's MUSIC VOLUME setting applies on top"),
    ("music_playing() -> bool", "whether the world's music channel has a track"),
];

/// A pan or a position of a sound.
fn place(name: &str, args: &[Value], i: usize) -> Result<[f64; 3], String> {
    Ok([finite(name, args, i)?, finite(name, args, i + 1)?, finite(name, args, i + 2)?])
}

/// A handle, as the whole number the script holds (anything out of range is no handle).
fn handle(name: &str, args: &[Value], i: usize) -> Result<u32, String> {
    let h = int(name, args, i)?;
    Ok(if h > 0 && h <= u32::MAX as i64 { h as u32 } else { 0 })
}

/// A sound's name: too long ones are not names of anything.
fn sound_name<'a>(name: &str, args: &'a [Value]) -> Result<Option<&'a str>, String> {
    let n = text(name, args, 0)?;
    Ok(if n.is_empty() || n.len() > 64 { None } else { Some(n) })
}

pub fn call(g: &mut Session, ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    let owner = ctx.mod_name.as_str();
    // A script that is not a mod has no name to hold sounds under.
    let owner = if owner.is_empty() { "script" } else { owner };
    match name {
        "play_sound" | "loop_sound" => {
            want(name, args, 4)?;
            let n = sound_name(name, args)?;
            let (vol, pitch, pan) = (finite(name, args, 1)? as f32, finite(name, args, 2)? as f32, finite(name, args, 3)? as f32);
            Ok(Value::Int(match n {
                Some(n) => sound::play(g, owner, n, vol, pitch, pan, None, name == "loop_sound") as i64,
                None => 0,
            }))
        }
        "play_sound_at" | "loop_sound_at" => {
            want(name, args, 6)?;
            let n = sound_name(name, args)?;
            let at = place(name, args, 1)?;
            let (vol, pitch) = (finite(name, args, 4)? as f32, finite(name, args, 5)? as f32);
            Ok(Value::Int(match n {
                Some(n) => sound::play(g, owner, n, vol, pitch, 0.0, Some(at), name == "loop_sound_at") as i64,
                None => 0,
            }))
        }
        "stop_sound" => {
            want(name, args, 1)?;
            sound::stop(g, owner, handle(name, args, 0)?);
            Ok(Value::Unit)
        }
        "fade_sound" => {
            want(name, args, 3)?;
            let h = handle(name, args, 0)?;
            sound::fade(g, owner, h, finite(name, args, 1)? as f32, finite(name, args, 2)? as f32);
            Ok(Value::Unit)
        }
        "set_sound" => {
            want(name, args, 7)?;
            let h = handle(name, args, 0)?;
            let (vol, pitch, pan) = (finite(name, args, 1)? as f32, finite(name, args, 2)? as f32, finite(name, args, 3)? as f32);
            let at = place(name, args, 4)?;
            sound::set(g, owner, h, vol, pitch, pan, Some(at));
            Ok(Value::Unit)
        }
        "is_playing" => {
            want(name, args, 1)?;
            Ok(Value::Bool(sound::is_playing(g, owner, handle(name, args, 0)?)))
        }
        "play_music" => {
            want(name, args, 4)?;
            let n = sound_name(name, args)?;
            let (vol, fade) = (finite(name, args, 1)? as f32, finite(name, args, 2)? as f32);
            let looping = flag(name, args, 3)?;
            Ok(Value::Bool(match n {
                Some(n) => sound::play_music(g, owner, n, vol, fade, looping),
                None => false,
            }))
        }
        "stop_music" => {
            want(name, args, 1)?;
            sound::stop_music(g, owner, finite(name, args, 0)? as f32);
            Ok(Value::Unit)
        }
        "set_music_volume" => {
            want(name, args, 1)?;
            sound::set_music_volume(g, owner, finite(name, args, 0)? as f32);
            Ok(Value::Unit)
        }
        "music_playing" => {
            want(name, args, 0)?;
            Ok(Value::Bool(sound::music_playing(g)))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::check::check_source;

    fn mistakes(src: &str) -> alloc::vec::Vec<String> {
        check_source("t.rs", src).unwrap().mistakes
    }

    #[test]
    fn handles_out_of_range_are_no_handle() {
        let h = |v: i64| handle("stop_sound", &[Value::Int(v)], 0).unwrap();
        assert_eq!((h(0), h(-5), h(i64::MAX), h(u32::MAX as i64 + 1)), (0, 0, 0, 0));
        assert_eq!(h(257), 257);
        assert_eq!(handle("stop_sound", &[Value::Float(258.7)], 0).unwrap(), 258);
        assert!(handle("stop_sound", &[Value::Bool(true)], 0).is_err());
        assert!(handle("stop_sound", &[], 0).is_err());
    }

    #[test]
    fn names_are_text_and_not_absurd() {
        let n = |v: Value| sound_name("play_sound_at", &[v]).map(|o| o.map(String::from));
        assert_eq!(n(Value::str("roar")).unwrap(), Some(String::from("roar")));
        assert_eq!(n(Value::str("")).unwrap(), None);
        assert_eq!(n(Value::string("x".repeat(65))).unwrap(), None);
        assert!(n(Value::Int(3)).is_err());
    }

    #[test]
    fn positions_and_numbers_must_be_numbers() {
        let args = [Value::str("a"), Value::Float(1.0), Value::Int(2), Value::Float(f64::NAN)];
        assert!(place("play_sound_at", &args, 1).is_err(), "NaN is not a place");
        let ok = [Value::str("a"), Value::Float(1.0), Value::Int(2), Value::Float(3.5)];
        assert_eq!(place("play_sound_at", &ok, 1).unwrap(), [1.0, 2.0, 3.5]);
    }

    #[test]
    fn the_lint_knows_the_arities_of_the_sound_calls() {
        let ok = "fn on_update(dt: f32) { play_sound(\"a\"); let h = play_sound(\"a\", 1.0, 1.0, 0.0); let g = play_sound_at(\"a\", 1.0, 2.0, 3.0, 1.0, 1.0); loop_sound(\"a\", 1.0, 1.0, 0.0); loop_sound_at(\"a\", 1.0, 2.0, 3.0, 1.0, 1.0); stop_sound(h); fade_sound(g, 0.0, 1.0); set_sound(g, 1.0, 1.0, 0.0, 1.0, 2.0, 3.0); is_playing(h); play_music(\"m\", 1.0, 1.0, true); stop_music(1.0); set_music_volume(0.5); music_playing(); }";
        assert!(mistakes(ok).is_empty(), "{:?}", mistakes(ok));
        assert!(!mistakes("fn on_update(dt: f32) { play_sound(\"a\", 1.0); }").is_empty());
        assert!(!mistakes("fn on_update(dt: f32) { play_sound_at(\"a\", 1.0); }").is_empty());
        assert!(!mistakes("fn on_update(dt: f32) { stop_music(); }").is_empty());
    }

    #[test]
    fn the_table_lists_each_call_once_with_its_arity() {
        let names: alloc::vec::Vec<&str> = FUNCTIONS.iter().map(|f| super::super::name_of(f.0)).collect();
        for n in ["play_sound_at", "loop_sound", "loop_sound_at", "stop_sound", "fade_sound", "set_sound", "is_playing", "play_music", "stop_music", "set_music_volume", "music_playing"] {
            assert_eq!(names.iter().filter(|x| **x == n).count(), 1, "{n}");
        }
        assert_eq!(super::super::arity_of(FUNCTIONS[0].0), 6);
        assert_eq!(super::super::arity_of("set_sound(handle: i64, volume: f64, pitch: f64, pan: f64, x: f64, y: f64, z: f64)"), 7);
    }
}
