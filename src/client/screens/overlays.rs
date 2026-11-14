//! The chat and emote wheel screens, drawn over the game (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    pub(super) fn screen_chat(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let (w, h) = (ui.w(), ui.h());
        let y = h - 40.0 * s;
        if ui.text_field_case(20, 4.0 * s, y, w * 0.6, 14.0 * s, &mut self.form_chat, 100, ui::any_char, ui::FieldCase::Cased) {
            let text = core::mem::take(&mut self.form_chat);
            let name = self.settings.player_name.clone();
            let online = self.session.as_ref().map_or(false, |g| g.net.is_some());
            let command = crate::game::aliases::expand(&text.trim().to_ascii_lowercase(), online);
            if command == "/reload" {
                self.want_reload = true;
            } else if command == "/help" {
                if let Some(g) = self.session.as_mut() {
                    g.push_chat(String::from("/RELOAD   /GIVE ITEM [N]   /TIME DAY|NIGHT   /TP X Y Z"));
                    g.push_chat(String::from("/SPAWN CREATURE [N] [YOUNG]   /MODE CREATIVE|SURVIVAL   /WEATHER CLEAR|RAIN|STORM   /SEED"));
                    g.push_chat(String::from("/SET X Y Z BLOCK [TURNS]   /DIM NAME|HOME   /SAY   /SOUND   /HEAL   /FEED   /HURT"));
                    g.push_chat(String::from("/LAUNCH UP [AHEAD]   /SHOOT DAMAGE [SPEED] [BLOCK]   /AIM BLOCK   /STRIKE   (~ = WHERE YOU STAND)"));
                    g.push_chat(String::from("/EFFECT GIVE ID [SECS] [LEVEL]   /EFFECT CLEAR [ID]   /EFFECT LIST   /EFFECT MOB ID"));
                    g.push_chat(String::from("/VAR NAME = N   /IF COND THEN CMD [ELSE CMD]   /AFTER SECS CMD   /REPEAT N CMD   /TAKE ITEM [N]   /TITLE TEXT   /BLAST R"));
                }
            } else if crate::net::accounts::client::is_account_command(&command) {
                // /register, /pin and /forgetpin hold a PIN: they are for the game alone (not the
                // server as chat, not the mods' scripts).
                if let Some(g) = self.session.as_mut() {
                    let reply = match g.net.as_mut() {
                        Some(n) => n.account_command(&command),
                        None => String::from("ACCOUNTS ARE FOR MULTIPLAYER SERVERS"),
                    };
                    g.push_chat(reply);
                }
            } else if let Some(g) = self.session.as_mut() {
                // Scripts hear `chat` and `command` first, and a mod's
                // `on_chat` may swallow the line (game/events.rs).
                let go_on = crate::game::events::chat_line(g, &text);
                // Commands are typed in lower case ids; anything else is talk.
                match if go_on && command.starts_with('/') { g.command(&command) } else { None } {
                    _ if !go_on => {}
                    Some(reply) if reply.is_empty() => {}
                    Some(reply) => g.push_chat(reply),
                    // On a server, what the game does not know the server may
                    // (/list, /op, /kick...): pass it on.
                    None if command.starts_with('/') && g.net.is_some() => {
                        if let Some(n) = g.net.as_mut() {
                            // (An alias goes as the full command.)
                            let sent = if command.starts_with("/gamemode") { command.clone() } else { text.clone() };
                            n.send_chat(&sent);
                        }
                    }
                    None if command.starts_with('/') => g.push_chat(String::from("NO SUCH COMMAND. TRY /HELP")),
                    None => g.send_chat(&name, &text),
                }
            }
            self.set_screen(Screen::Game);
        }
    }

    /// The emote wheel: let go of the key or click to play the emote the pointer is on.
    pub(super) fn screen_emote(&mut self, ui: &mut Ui) {
        let names: Vec<String> = reg().emotes.list.iter().map(|e| e.name.to_ascii_uppercase()).collect();
        let key = (self.settings.key(Action::Emote) as usize).min(511);
        let (held, pressed) = (self.keys[key], ui.inp.pressed(key as i32));
        match self.wheel.show(ui, &names, held, pressed) {
            crate::client::emote_wheel::Outcome::Open => {}
            crate::client::emote_wheel::Outcome::Cancel => self.set_screen(Screen::Game),
            crate::client::emote_wheel::Outcome::Play(i) => {
                if let Some(g) = self.session.as_mut() {
                    g.play_emote(i);
                }
                self.set_screen(Screen::Game);
            }
        }
    }
}
