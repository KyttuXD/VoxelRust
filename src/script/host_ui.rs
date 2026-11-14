//! The host functions for controls, the HUD and mod screens (the second
//! half of the host API: see docs/SCRIPTING.md, "Controls, HUD and screens").
//!
//! They are thin: each reads its arguments and calls the matching method of
//! [`ModApi`] (`client/modui/api.rs`), which holds all the limits. The calls
//! that read the game (`hunger`, `air`, `biome`, `heading`...) go through the
//! [`Host`](crate::script::host::Host) like the first half does.

use crate::client::modui::binds::key_scancode;
use crate::client::modui::ModApi;
use crate::script::host::{int, num, text, want};
use crate::script::interp::{Interp, Res};
use crate::script::value::Value;
use alloc::format;
use alloc::string::String;

/// Every function here: (signature, what it does). Kept in step with `call`
/// by a test, and with docs/SCRIPTING.md.
pub const UI_FUNCTIONS: &[(&str, &str)] = &[
    ("bind_key(id: &str, label: &str, group: &str, key: i64) -> bool", "in `on_register` only: add a key binding to Settings > Controls (`key` is a key code or a name like \"g\")"),
    ("add_switch(id: &str, label: &str, default: bool) -> bool", "in `on_register` only: add a row to the mod switches page"),
    ("key_scancode(name: &str) -> i64", "the key code of a key name (\"g\", \"f5\", \"space\", \"up\"...), 0 if unknown"),
    ("key_held(id: &str) -> bool", "whether the key bound to this binding is down now"),
    ("setting_on(id: &str) -> bool", "whether one of this mod's switches is on"),
    ("hud_text(anchor: &str, dx: f64, dy: f64, scale: f64, text: &str, colour: i64) -> bool", "in `on_hud`: draw text; false if over the frame's cap"),
    ("hud_rect(anchor: &str, dx: f64, dy: f64, w: f64, h: f64, colour: i64) -> bool", "in `on_hud`: draw a filled rectangle"),
    ("hud_outline(anchor: &str, dx: f64, dy: f64, w: f64, h: f64, thick: f64, colour: i64) -> bool", "in `on_hud`: draw a rectangle's outline"),
    ("hud_item(anchor: &str, dx: f64, dy: f64, size: f64, item_id: &str) -> bool", "in `on_hud`: draw an item's icon"),
    ("hud_block(anchor: &str, dx: f64, dy: f64, size: f64, block_id: &str) -> bool", "in `on_hud`: draw a block's icon"),
    ("hud_bar(anchor: &str, dx: f64, dy: f64, w: f64, h: f64, fraction: f64, colour: i64) -> bool", "in `on_hud`: draw a progress bar (0 to 1)"),
    ("hud_image(name: &str, x: f64, y: f64, w: f64, h: f64, alpha: f64) -> bool", "in `on_hud`: draw a picture of the mod (`name` is a png or tga file in the mod's folder, like \"images/scare.png\"), `w` x `h` interface pixels with its top-left corner `x` right and `y` down from the screen's; `alpha` 0 to 1; false if the picture cannot be read, the mod already has 16, or the frame is full"),
    ("hud_image_at(name: &str, anchor: &str, dx: f64, dy: f64, w: f64, h: f64, alpha: f64, tint: i64) -> bool", "in `on_hud`: as `hud_image` with an anchor like the other calls, and a tint (`0xRRGGBB`, `0xFFFFFF` for the picture as it is)"),
    ("hud_image_uv(name: &str, x: f64, y: f64, w: f64, h: f64, u0: f64, v0: f64, u1: f64, v1: f64, alpha: f64) -> bool", "in `on_hud`: draw a part of a picture, for sprite sheets and strips: (u0, v0) to (u1, v1) are fractions of the picture (0 to 1), (0, 0) its top-left corner"),
    ("hud_image_uv_at(name: &str, anchor: &str, dx: f64, dy: f64, w: f64, h: f64, u0: f64, v0: f64, u1: f64, v1: f64, alpha: f64, tint: i64) -> bool", "in `on_hud`: as `hud_image_uv` with an anchor and a tint"),
    ("hud_image_fit(name: &str, mode: &str, alpha: f64) -> bool", "in `on_hud`: fit a picture to the whole screen: `\"cover\"` fills it (and crops what does not fit), `\"contain\"` shows all of it with bars; an error for another mode"),
    ("hud_set(slot: &str, id: &str, text: &str, colour: i64) -> bool", "put a line in a HUD slot, or change the one with this id (it stays until removed)"),
    ("hud_set_icon(id: &str, item_id: &str) -> bool", "give a slot line an item icon (an empty id takes it away)"),
    ("hud_remove(id: &str) -> bool", "take a slot line away"),
    ("toast(text: &str, secs: f64) -> bool", "show a message at the top for a few seconds"),
    ("action_bar(text: &str, secs: f64) -> bool", "show a line above the hotbar for a few seconds"),
    ("hunger() -> i64", "the player's hunger (20 is full)"),
    ("air() -> f64", "the player's breath left (10 is full)"),
    ("biome() -> String", "the biome under the player, lower case"),
    ("fps() -> f64", "frames a second"),
    ("selected_slot() -> i64", "the hotbar slot chosen, 0 to 8"),
    ("heading() -> f64", "the way the player faces, in degrees: 0 north, 90 east, 180 south, 270 west"),
    ("ui_open(name: &str, title: &str) -> bool", "open a screen of the mod (empty; add widgets after it); one that was open closes"),
    ("ui_close() -> bool", "close the mod's screen"),
    ("ui_text(text: &str) -> bool", "add a line of text to the screen"),
    ("ui_button(id: &str, label: &str) -> bool", "add a button (`on_ui_click(id)`)"),
    ("ui_toggle(id: &str, label: &str, on: bool) -> bool", "add an on/off toggle (`on_ui_change(id, value)`, a bool)"),
    ("ui_stepper(id: &str, label: &str, value: i64, min: i64, max: i64) -> bool", "add a number you step with < > (`on_ui_change`, an integer)"),
    ("ui_input(id: &str, label: &str, value: &str) -> bool", "add a text box (`on_ui_change`, the text)"),
    ("ui_list(id: &str, items: &str, selected: i64) -> bool", "add a scrolling list of lines (`items` separated by newlines; `on_ui_change`, the line's number)"),
    ("ui_set_text(id: &str, text: &str) -> bool", "change a widget's label or text (a list: its items)"),
    ("ui_set_line(n: i64, text: &str) -> bool", "change the text of the nth widget when it is a text line"),
    ("ui_set_value(id: &str, n: i64) -> bool", "change a toggle, stepper or list's chosen line"),
    ("ui_get_text(id: &str) -> String", "an input's text, or the chosen line of a list (empty if none)"),
    ("ui_get_value(id: &str) -> i64", "a toggle's (0 or 1), stepper's or list's (-1 for none) value"),
];

/// Whether `name` is one of the functions here.
fn is_ui_fn(name: &str) -> bool {
    matches!(
        name,
        "bind_key"
            | "add_switch"
            | "key_scancode"
            | "key_held"
            | "setting_on"
            | "hud_text"
            | "hud_rect"
            | "hud_outline"
            | "hud_item"
            | "hud_block"
            | "hud_bar"
            | "hud_image"
            | "hud_image_at"
            | "hud_image_uv"
            | "hud_image_uv_at"
            | "hud_image_fit"
            | "hud_set"
            | "hud_set_icon"
            | "hud_remove"
            | "toast"
            | "action_bar"
            | "hunger"
            | "air"
            | "biome"
            | "fps"
            | "selected_slot"
            | "heading"
            | "ui_open"
            | "ui_close"
            | "ui_text"
            | "ui_button"
            | "ui_toggle"
            | "ui_stepper"
            | "ui_input"
            | "ui_list"
            | "ui_set_text"
            | "ui_set_line"
            | "ui_set_value"
            | "ui_get_text"
            | "ui_get_value"
    )
}

fn flag(name: &str, args: &[Value], i: usize) -> Result<bool, String> {
    match args.get(i) {
        Some(Value::Bool(b)) => Ok(*b),
        Some(Value::Int(n)) => Ok(*n != 0),
        Some(other) => Err(format!("`{}`: argument {} should be true or false, found {}", name, i + 1, other.type_name())),
        None => Err(format!("`{}` is missing argument {}", name, i + 1)),
    }
}

/// A key: a code, or a name that `key_scancode` knows.
fn key(name: &str, args: &[Value], i: usize) -> Result<i32, String> {
    match args.get(i) {
        Some(Value::Int(n)) => Ok((*n).clamp(0, 100_000) as i32),
        Some(Value::Str(s)) => match key_scancode(s.as_str()) {
            0 => Err(format!("`{}`: `{}` is not a key name (a letter or digit, f1 to f12, space, tab, enter, up, down, left, right, shift, ctrl, alt...)", name, s.as_str())),
            k => Ok(k),
        },
        Some(other) => Err(format!("`{}`: argument {} should be a key code or a key name, found {}", name, i + 1, other.type_name())),
        None => Err(format!("`{}` is missing argument {}", name, i + 1)),
    }
}

/// Call the function `name` if it is one of these. `None`: it is not.
pub(crate) fn call(it: &mut Interp, name: &str, args: &[Value]) -> Option<Res> {
    if !is_ui_fn(name) {
        return None;
    }
    let mod_name = it.mod_name;
    let r = run(it, mod_name, name, args);
    Some(match r {
        Ok(v) => Ok(v),
        Err(m) => it.fail(m),
    })
}

fn run(it: &mut Interp, mod_name: &str, name: &str, args: &[Value]) -> Result<Value, String> {
    let b = |ok: bool| Ok(Value::Bool(ok));
    match name {
        // ---- reading the game
        "hunger" => want(name, args, 0).map(|_| Value::Int(it.host.hunger())),
        "air" => want(name, args, 0).map(|_| Value::Float(it.host.air())),
        "biome" => want(name, args, 0).map(|_| Value::string(it.host.biome())),
        "selected_slot" => want(name, args, 0).map(|_| Value::Int(it.host.selected_slot())),
        "heading" => want(name, args, 0).map(|_| Value::Float(it.host.heading())),
        "setting_on" => {
            want(name, args, 1)?;
            let id = text(name, args, 0)?;
            Ok(Value::Bool(it.host.setting_on(mod_name, id)))
        }
        "key_scancode" => {
            want(name, args, 1)?;
            Ok(Value::Int(key_scancode(text(name, args, 0)?) as i64))
        }
        // ---- pictures: they need the data folder to find the mod's files
        "hud_image" | "hud_image_at" | "hud_image_uv" | "hud_image_uv_at" | "hud_image_fit" => {
            let dir = it.host.data_dir();
            let mut api = ModApi::new(it.host.ui(), mod_name);
            let full = [0.0, 0.0, 1.0, 1.0];
            match name {
                "hud_image" => {
                    want(name, args, 6)?;
                    b(api.hud_image(&dir, text(name, args, 0)?, "top-left", num(name, args, 1)?, num(name, args, 2)?, num(name, args, 3)?, num(name, args, 4)?, full, num(name, args, 5)?, 0xFFFFFF)?)
                }
                "hud_image_at" => {
                    want(name, args, 8)?;
                    b(api.hud_image(&dir, text(name, args, 0)?, text(name, args, 1)?, num(name, args, 2)?, num(name, args, 3)?, num(name, args, 4)?, num(name, args, 5)?, full, num(name, args, 6)?, int(name, args, 7)?)?)
                }
                "hud_image_uv" => {
                    want(name, args, 10)?;
                    let uv = [num(name, args, 5)?, num(name, args, 6)?, num(name, args, 7)?, num(name, args, 8)?];
                    b(api.hud_image(&dir, text(name, args, 0)?, "top-left", num(name, args, 1)?, num(name, args, 2)?, num(name, args, 3)?, num(name, args, 4)?, uv, num(name, args, 9)?, 0xFFFFFF)?)
                }
                "hud_image_uv_at" => {
                    want(name, args, 12)?;
                    let uv = [num(name, args, 6)?, num(name, args, 7)?, num(name, args, 8)?, num(name, args, 9)?];
                    b(api.hud_image(&dir, text(name, args, 0)?, text(name, args, 1)?, num(name, args, 2)?, num(name, args, 3)?, num(name, args, 4)?, num(name, args, 5)?, uv, num(name, args, 10)?, int(name, args, 11)?)?)
                }
                _ => {
                    want(name, args, 3)?;
                    b(api.hud_image_fit(&dir, text(name, args, 0)?, text(name, args, 1)?, num(name, args, 2)?)?)
                }
            }
        }
        // ---- the rest goes through the mod's handle
        _ => {
            let mut api = ModApi::new(it.host.ui(), mod_name);
            match name {
                "bind_key" => {
                    want(name, args, 4)?;
                    let k = key(name, args, 3)?;
                    b(api.bind_key(text(name, args, 0)?, text(name, args, 1)?, text(name, args, 2)?, k)?)
                }
                "add_switch" => {
                    want(name, args, 3)?;
                    b(api.add_switch(text(name, args, 0)?, text(name, args, 1)?, flag(name, args, 2)?)?)
                }
                "key_held" => {
                    want(name, args, 1)?;
                    b(api.key_held(text(name, args, 0)?))
                }
                "hud_text" => {
                    want(name, args, 6)?;
                    b(api.hud_text(text(name, args, 0)?, num(name, args, 1)?, num(name, args, 2)?, num(name, args, 3)?, text(name, args, 4)?, int(name, args, 5)?)?)
                }
                "hud_rect" => {
                    want(name, args, 6)?;
                    b(api.hud_rect(text(name, args, 0)?, num(name, args, 1)?, num(name, args, 2)?, num(name, args, 3)?, num(name, args, 4)?, int(name, args, 5)?)?)
                }
                "hud_outline" => {
                    want(name, args, 7)?;
                    b(api.hud_outline(text(name, args, 0)?, num(name, args, 1)?, num(name, args, 2)?, num(name, args, 3)?, num(name, args, 4)?, num(name, args, 5)?, int(name, args, 6)?)?)
                }
                "hud_item" => {
                    want(name, args, 5)?;
                    b(api.hud_item(text(name, args, 0)?, num(name, args, 1)?, num(name, args, 2)?, num(name, args, 3)?, text(name, args, 4)?)?)
                }
                "hud_block" => {
                    want(name, args, 5)?;
                    b(api.hud_block(text(name, args, 0)?, num(name, args, 1)?, num(name, args, 2)?, num(name, args, 3)?, text(name, args, 4)?)?)
                }
                "hud_bar" => {
                    want(name, args, 7)?;
                    b(api.hud_bar(text(name, args, 0)?, num(name, args, 1)?, num(name, args, 2)?, num(name, args, 3)?, num(name, args, 4)?, num(name, args, 5)?, int(name, args, 6)?)?)
                }
                "hud_set" => {
                    want(name, args, 4)?;
                    b(api.hud_set(text(name, args, 0)?, text(name, args, 1)?, text(name, args, 2)?, int(name, args, 3)?)?)
                }
                "hud_set_icon" => {
                    want(name, args, 2)?;
                    b(api.hud_set_icon(text(name, args, 0)?, text(name, args, 1)?))
                }
                "hud_remove" => {
                    want(name, args, 1)?;
                    b(api.hud_remove(text(name, args, 0)?))
                }
                "toast" => {
                    want(name, args, 2)?;
                    b(api.toast(text(name, args, 0)?, num(name, args, 1)?))
                }
                "action_bar" => {
                    want(name, args, 2)?;
                    b(api.action_bar(text(name, args, 0)?, num(name, args, 1)?))
                }
                "fps" => {
                    want(name, args, 0)?;
                    Ok(Value::Float(api.fps()))
                }
                "ui_open" => {
                    want(name, args, 2)?;
                    b(api.ui_open(text(name, args, 0)?, text(name, args, 1)?))
                }
                "ui_close" => {
                    want(name, args, 0)?;
                    b(api.ui_close())
                }
                "ui_text" => {
                    want(name, args, 1)?;
                    b(api.ui_text(text(name, args, 0)?))
                }
                "ui_button" => {
                    want(name, args, 2)?;
                    b(api.ui_button(text(name, args, 0)?, text(name, args, 1)?))
                }
                "ui_toggle" => {
                    want(name, args, 3)?;
                    b(api.ui_toggle(text(name, args, 0)?, text(name, args, 1)?, flag(name, args, 2)?))
                }
                "ui_stepper" => {
                    want(name, args, 5)?;
                    b(api.ui_stepper(text(name, args, 0)?, text(name, args, 1)?, int(name, args, 2)?, int(name, args, 3)?, int(name, args, 4)?))
                }
                "ui_input" => {
                    want(name, args, 3)?;
                    b(api.ui_input(text(name, args, 0)?, text(name, args, 1)?, text(name, args, 2)?))
                }
                "ui_list" => {
                    want(name, args, 3)?;
                    b(api.ui_list(text(name, args, 0)?, text(name, args, 1)?, int(name, args, 2)?))
                }
                "ui_set_text" => {
                    want(name, args, 2)?;
                    b(api.ui_set_text(text(name, args, 0)?, text(name, args, 1)?))
                }
                "ui_set_line" => {
                    want(name, args, 2)?;
                    b(api.ui_set_line(int(name, args, 0)?, text(name, args, 1)?))
                }
                "ui_set_value" => {
                    want(name, args, 2)?;
                    b(api.ui_set_value(text(name, args, 0)?, int(name, args, 1)?))
                }
                "ui_get_text" => {
                    want(name, args, 1)?;
                    Ok(Value::string(api.ui_get_text(text(name, args, 0)?)))
                }
                "ui_get_value" => {
                    want(name, args, 1)?;
                    Ok(Value::Int(api.ui_get_value(text(name, args, 0)?)))
                }
                other => Err(format!("`{}` is not implemented", other)),
            }
        }
    }
}
