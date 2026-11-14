//! Talking to the same mod on the other players' machines in a shared
//! game. A message is a key and a text; it reaches `on_net(key, text)` of the
//! same mod's scripts on every other player's game (only if they have that
//! mod). Nothing is sent from a game that is not shared.

use super::args::{text, want};
use super::Ctx;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[("net_send(key: &str, text: &str) -> bool", "send a message to this mod's scripts on the other players' games (`on_net`); false when the game is not shared or the key is over 64 bytes or the text over 8000")];

/// Longest key and text.
const MAX_KEY: usize = 64;
const MAX_TEXT: usize = 8000;
/// Most messages a frame.
const MAX_PER_FRAME: u32 = 4;

pub fn call(g: &mut Session, ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "net_send" => {
            want(name, args, 2)?;
            let (key, body) = (text(name, args, 0)?, text(name, args, 1)?);
            if key.is_empty() || key.len() > MAX_KEY || body.len() > MAX_TEXT || key.contains('\u{1f}') || ctx.mod_name.contains('\u{1f}') {
                return Ok(Value::Bool(false));
            }
            if super::data(g).net_sent >= MAX_PER_FRAME {
                return Ok(Value::Bool(false));
            }
            super::data(g).net_sent += 1;
            match g.net.as_mut() {
                Some(n) => {
                    n.send_state(crate::proto::channel::SCRIPT, false, &format!("{}\u{1f}{}", ctx.mod_name, key), body.as_bytes());
                    Ok(Value::Bool(true))
                }
                None => Ok(Value::Bool(false)),
            }
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
