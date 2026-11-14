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

pub const FUNCTIONS: &[(&str, &str)] = &[("net_send(key: &str, text: &str) -> bool", "send a message to this mod's scripts on the other players' games (`on_net`); false when the game is not shared or the key is over 64 bytes (the mod's name and the key together) or the text over 8000")];

/// Longest key and text.
const MAX_KEY: usize = 64;
const MAX_TEXT: usize = 8000;
/// Whether `key` of the mod `mod_name` makes a key that goes over the wire: the
/// wire key is `mod\x1fkey`, and a longer one than [`MAX_KEY`] bytes is not a message
/// (the receiver would drop the connection).
fn key_fits(mod_name: &str, key: &str) -> bool {
    !key.is_empty() && mod_name.len() + 1 + key.len() <= MAX_KEY
}

/// Most messages a frame.
const MAX_PER_FRAME: u32 = 4;

pub fn call(g: &mut Session, ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "net_send" => {
            want(name, args, 2)?;
            let (key, body) = (text(name, args, 0)?, text(name, args, 1)?);
            // (The key on the wire is `mod\x1fkey`, and a longer one than 64 bytes is not a message.)
            if !key_fits(&ctx.mod_name, key) || body.len() > MAX_TEXT || key.contains('\u{1f}') || ctx.mod_name.contains('\u{1f}') {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mod_name_counts_towards_the_key_limit() {
        assert!(key_fits("earth", "score"));
        assert!(!key_fits("earth", ""));
        assert!(key_fits("m", &"k".repeat(62)));
        assert!(!key_fits("m", &"k".repeat(63)));
        assert!(!key_fits(&"m".repeat(40), &"k".repeat(30)));
    }
}
