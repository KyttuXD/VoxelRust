//! Messages between the copies of a mod in a shared game: the host (or any
//! player) tells the mod's scripts on the other machines what happened, and
//! they answer with `on_message(channel, from, text)`. Rules and wire format
//! are in `net/modmsg.rs`; the work is `Session::send_mod_message`, which
//! native mods use too. (`net_send` / `on_net` of `net.rs` are the older, looser
//! pair: any key, 8000 bytes, no player to address.)

use super::args::{text, want};
use super::Ctx;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("send_message(channel: &str, text: &str) -> bool", "send a message to this mod's scripts on all the other players' games (`on_message`); `channel` is a name of your choosing (printable ASCII; with the mod's name at most 64 bytes), the text at most 512 bytes without control characters; at most 8 a second for each player (the server drops more); false when the game is not shared, the channel or text is not fit, or you are over the rate"),
    ("send_message_to(player: &str, channel: &str, text: &str) -> bool", "as `send_message`, but only to the one player with that name (any case); false if no such player is in the game"),
];

pub fn call(g: &mut Session, ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "send_message" => {
            want(name, args, 2)?;
            let (channel, body) = (text(name, args, 0)?, text(name, args, 1)?);
            Ok(Value::Bool(g.send_mod_message(&ctx.mod_name, channel, None, body)))
        }
        "send_message_to" => {
            want(name, args, 3)?;
            let (who, channel, body) = (text(name, args, 0)?, text(name, args, 1)?, text(name, args, 2)?);
            Ok(Value::Bool(g.send_mod_message(&ctx.mod_name, channel, Some(who), body)))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
