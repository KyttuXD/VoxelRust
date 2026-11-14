//! The PIN screen: asked for when a server says the player's name has an account (or that it needs
//! one). Digits only, shown as dots, 4 or 6 long (methods of `App`).

#[allow(unused_imports)]
use super::*;
use crate::net::accounts::client::{ConnectError, JoinAuth, RememberAt};

/// What the PIN screen is asking and what has been typed so far.
#[derive(Default)]
pub(super) struct PinPrompt {
    address: String,
    name: String,
    /// The name has no account: the PIN is a new one (typed twice).
    register: bool,
    tries: u8,
    /// Why the PIN is asked again (empty the first time).
    note: String,
    weak_ok: bool,
    pin: String,
    again: String,
    /// Typing the second copy of a new PIN.
    stage: u8,
    remember: bool,
}

impl PinPrompt {
    pub(super) fn new(address: &str, name: &str, register: bool, tries: u8, note: String, weak_ok: bool, remember: bool) -> PinPrompt {
        PinPrompt { address: String::from(address), name: String::from(name), register, tries, note, weak_ok, remember, ..Default::default() }
    }
}

impl App {
    /// Join `address`, with a PIN or without; the PIN screen opens if the server wants one.
    /// True if this screen is left (in the game, or on the PIN screen); false with `self.status` set if not.
    pub(super) fn join_with_auth(&mut self, r: &mut Renderer, address: &str, name: &str, auth: &JoinAuth) -> bool {
        match Session::join_with(&self.base, address, name, self.threads, auth) {
            Ok(mut g) => {
                self.close_world(r);
                g.who = crate::settings::clean_name(&self.settings.player_name, 16);
                self.session = Some(g);
                self.distance = self.settings.distance;
                self.status.clear();
                self.pin = PinPrompt::default();
                self.set_screen(Screen::Game);
                true
            }
            Err(ConnectError::NeedPin { register, tries, note, weak_ok }) => {
                let keep = self.pin.remember || auth.remember;
                self.pin = PinPrompt::new(address, name, register, tries, note, weak_ok, keep);
                self.set_screen(Screen::Pin);
                true
            }
            Err(ConnectError::Failed(e)) => {
                self.status = e;
                false
            }
        }
    }

    pub(super) fn screen_pin(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let (x, y, bw, bh, step) = Self::column(ui, 8);
        let cx = ui.w() * 0.5;
        // What was typed (digits only; copy and paste are left out on purpose).
        let stage = self.pin.stage;
        let submit = ui.inp.pressed(ui::KEY_RETURN);
        {
            let p = &mut self.pin;
            let field = if stage == 0 { &mut p.pin } else { &mut p.again };
            for c in ui.inp.text.chars() {
                if c.is_ascii_digit() && field.len() < 6 {
                    field.push(c);
                }
            }
            for &k in &ui.inp.keys {
                if k == ui::KEY_BACKSPACE {
                    field.pop();
                }
            }
        }
        ui.centered(cx, y - 30.0 * s, 2.0, "ACCOUNT", WHITE);
        let who = self.pin.name.to_ascii_uppercase();
        if self.pin.register {
            ui.centered(cx, y - 8.0 * s, 1.0, &alloc::format!("THIS SERVER NEEDS AN ACCOUNT FOR {}", who), ui::DIM);
            ui.centered(cx, y + 2.0 * s, 1.0, if stage == 0 { "CHOOSE A PIN OF 4 OR 6 DIGITS" } else { "TYPE THE SAME PIN AGAIN" }, ui::DIM);
        } else {
            ui.centered(cx, y - 8.0 * s, 1.0, &alloc::format!("THE NAME {} HAS AN ACCOUNT", who), ui::DIM);
            ui.centered(cx, y + 2.0 * s, 1.0, "TYPE ITS PIN", ui::DIM);
        }
        // The PIN as dots in a box.
        let typed = if stage == 0 { self.pin.pin.len() } else { self.pin.again.len() };
        let shown: String = "*".repeat(typed) + if (ui.clock * 2.0) as i32 % 2 == 0 { "_" } else { " " };
        ui.r.rect(x, y + step * 1.2, bw, bh, [8, 8, 12, 240]);
        ui.r.frame(x, y + step * 1.2, bw, bh, s, ui::ACCENT);
        ui.r.label(x + 4.0 * s, y + step * 1.2 + (bh - 7.0 * s) * 0.5, s, &shown, WHITE);
        if !self.pin.note.is_empty() {
            ui.centered(cx, y + step * 2.4, 1.0, &self.pin.note, ui::ACCENT);
        } else if !self.pin.register && (1..5).contains(&self.pin.tries) {
            // (Wrong tries from before, on this account or from this address, still count.)
            ui.centered(cx, y + step * 2.4, 1.0, &alloc::format!("{} TRIES LEFT BEFORE A LOCKOUT", self.pin.tries), ui::ACCENT);
        }
        if !self.status.is_empty() {
            ui.centered(cx, y + step * 2.4, 1.0, &self.status, ui::ACCENT);
        }
        let mut remember = self.pin.remember;
        ui.toggle(x, y + step * 3.4, bw, bh, "REMEMBER THE PIN", &mut remember);
        self.pin.remember = remember;
        ui.centered(cx, y + step * 4.4, 1.0, "KEPT AS PLAIN TEXT IN YOUR DATA FOLDER", ui::DIM);
        let half = bw * 0.5 - 2.0 * s;
        let ok = ui.button(x, y + step * 5.2, half, bh, if self.pin.register && stage == 0 { "NEXT" } else if self.pin.register { "MAKE ACCOUNT" } else { "JOIN" });
        let cancel = ui.button(x + half + 4.0 * s, y + step * 5.2, half, bh, "CANCEL");
        ui.centered(cx, y + step * 6.4, 1.0, "A PIN STOPS PLAYERS USING YOUR NAME; IT IS NOT STRONG SECRET", ui::DIM);
        if cancel {
            self.pin = PinPrompt::default();
            self.status.clear();
            self.set_screen(Screen::Join);
            return;
        }
        if !(ok || submit) {
            return;
        }
        self.status.clear();
        let (len_ok, text) = {
            let f = if stage == 0 { &self.pin.pin } else { &self.pin.again };
            (f.len() == 4 || f.len() == 6, f.clone())
        };
        if !len_ok {
            self.status = String::from("A PIN IS 4 OR 6 DIGITS");
            return;
        }
        if self.pin.register && stage == 0 {
            if let Err(e) = crate::net::accounts::pin::check(&text, self.pin.weak_ok) {
                self.status = e.to_ascii_uppercase();
                self.pin.pin.clear();
                return;
            }
            self.pin.stage = 1;
            return;
        }
        if self.pin.register && self.pin.pin != self.pin.again {
            self.status = String::from("THE TWO PINS DIFFER: START AGAIN");
            self.pin.pin.clear();
            self.pin.again.clear();
            self.pin.stage = 0;
            return;
        }
        let (address, name) = (self.pin.address.clone(), self.pin.name.clone());
        let auth = JoinAuth { at: Some(RememberAt { base: self.base.clone(), address: address.clone() }), pin: Some(self.pin.pin.clone()), register: self.pin.register, remember: self.pin.remember };
        // The PIN is typed again if the server does not take it.
        if !self.join_with_auth(ui.r, &address, &name, &auth) {
            self.pin = PinPrompt::default();
            self.set_screen(Screen::Join);
        }
    }
}
