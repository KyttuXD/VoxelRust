//! Mods talking to themselves across a shared game: `send_message` and
//! `on_message` for scripts (see docs/SCRIPTING.md) and the same for native
//! mods (`Session::send_mod_message`, `Native::on_message`). The wire rules
//! (key, size, rate) are in `net/modmsg.rs`; the server checks them again.
//!
//! A message goes from one machine to the same mod on the others (everyone
//! else, or one player by name). It is never delivered to the machine that
//! sent it. Outside a shared game nothing is sent.

use crate::net::modmsg;
use crate::session::Session;
use alloc::string::String;

impl Session {
    /// Send `text` on `channel` of the mod `mod_name` to the same mod on the
    /// other players' machines, or only to the player called `to`. `false`
    /// when the game is not shared, the player is not there, the channel or
    /// text is not fit to send (64 bytes of mod name and channel together,
    /// 512 bytes of text), or this player has sent 8 in the last second.
    pub fn send_mod_message(&mut self, mod_name: &str, channel: &str, to: Option<&str>, text: &str) -> bool {
        let Some(key) = modmsg::make_key(mod_name, channel) else { return false };
        if !modmsg::text_ok(text) {
            return false;
        }
        let target = match (to, self.net.as_ref()) {
            (_, None) => return false,
            (None, Some(_)) => modmsg::EVERYONE,
            (Some(name), Some(n)) => match n.players.iter().find(|p| p.1.name.eq_ignore_ascii_case(name)) {
                Some((id, _)) => *id,
                None => return false,
            },
        };
        let Some(data) = modmsg::pack(target, text) else { return false };
        let now = self.clock;
        if !crate::script::ext::data(self).msg_bucket.allow(now) {
            return false;
        }
        match self.net.as_mut() {
            Some(n) => {
                n.send_state(crate::proto::channel::MODMSG, false, &key, &data);
                true
            }
            None => false,
        }
    }

    /// A message of a mod arrived from player `from`: hand it to that mod's
    /// scripts (`on_message(channel, from, text)`) and native code
    /// (`Native::on_message`), if the mod is on here.
    pub(crate) fn receive_mod_message(&mut self, from: u16, key: &str, data: &[u8]) {
        let Some(net) = self.net.as_ref() else { return };
        let (Some((to, text)), Some((mod_name, channel))) = (modmsg::unpack(data), modmsg::split_key(key)) else { return };
        if (to != modmsg::EVERYONE && to != net.id) || from == net.id {
            return;
        }
        let Some(who) = net.players.get(&from).map(|p| p.name.clone()) else { return };
        let (mod_name, channel, text): (String, String, String) = (mod_name.into(), channel.into(), text.into());
        crate::script::bridge::mod_message(self, &mod_name, &channel, &who, &text);
        for n in crate::native::all().iter().filter(|n| n.name == mod_name && crate::block::reg().has(n.name)) {
            if let Some(f) = n.on_message {
                f(self, &channel, &who, &text);
            }
        }
    }
}
