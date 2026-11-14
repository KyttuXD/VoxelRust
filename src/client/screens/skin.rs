//! Skins: loading, the file picker, dropped files and the web address panel (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    /// (Re)build the player's skin texture from the settings.
    pub(super) fn load_skin(&mut self, r: &mut Renderer) {
        self.skin_for = self.settings.player_name.clone();
        let who = crate::settings::clean_name(&self.settings.player_name, 16);
        let px = match skin_pixels(&self.base, &self.settings.skin, &who) {
            Some(px) => px,
            None => {
                self.status = String::from("NO USABLE SKIN.PNG (64 X 64) IN THE DATA FOLDER");
                self.settings.skin = String::from("default");
                match skin_pixels(&self.base, "default", &who) {
                    Some(px) => px,
                    None => return,
                }
            }
        };
        // The texture made the last time is written over, not left on the card.
        if self.skin_tex == 0 {
            self.skin_tex = r.add_texture(64, 64, &px);
        } else {
            r.update_texture(self.skin_tex, 64, 64, &px);
        }
        self.skin_slim = crate::client::skinfile::is_slim(&px, 64, 64);
        self.skin_png = crate::png::encode(64, 64, &px);
        self.skin_sent = false;
    }

    /// Open the file browser to choose a skin picture.
    pub(super) fn open_skin_picker(&mut self) {
        let mut fallbacks: Vec<String> = Vec::new();
        if let Some(h) = crate::client::picker::home_dir() {
            fallbacks.push(crate::client::picker::join(&h, "Downloads"));
            fallbacks.push(h);
        }
        fallbacks.push(self.base.clone());
        let start = self.settings.picker_dir.clone();
        self.picker.open("CHOOSE A SKIN: A 64 X 64 OR 64 X 32 PICTURE", &start, &fallbacks, SKIN_EXTS, &self.base);
        self.picker.size_ok = Some(crate::client::skinfile::size_ok);
        self.picker.hint = String::from("SKINS: 64 X 64 OR 64 X 32");
        self.set_screen(Screen::Picker);
    }

    /// The file browser is done: remember where it was, and use the file if one was chosen.
    pub(super) fn close_picker(&mut self, chosen: Option<String>) -> bool {
        self.settings.picker_dir = self.picker.dir.clone();
        if !self.scripted {
            sys::write_file_atomic(&self.settings_path, self.settings.to_text().as_bytes());
        }
        self.set_screen(Screen::Settings);
        if let Some(path) = chosen {
            match import_skin(&self.base, &path) {
                Ok(()) => {
                    self.settings.skin = String::from("custom");
                    self.status = String::from("SKIN LOADED");
                    return true;
                }
                Err(e) => self.status = e,
            }
        }
        false
    }

    pub(super) fn screen_picker(&mut self, ui: &mut Ui) {
        ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [8, 9, 14, 235]);
        match self.picker.show(ui) {
            crate::client::picker::Outcome::Open => {}
            crate::client::picker::Outcome::Cancelled => {
                self.close_picker(None);
            }
            crate::client::picker::Outcome::Chosen(p) => {
                if self.close_picker(Some(p)) {
                    self.load_skin(ui.r);
                }
            }
        }
    }

    /// A file was dropped on the window: on the skin settings (or the web
    /// address panel) a picture becomes the custom skin.
    pub(super) fn skin_dropped(&mut self, r: &mut Renderer, path: &str) {
        if !(self.screen == Screen::SkinUrl || (self.screen == Screen::Settings && self.tab == 4)) {
            return;
        }
        let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        if !SKIN_EXTS.contains(&ext.as_str()) {
            self.status = String::from("NOT A SKIN PICTURE");
        } else {
            match import_skin(&self.base, path) {
                Ok(()) => {
                    self.settings.skin = String::from("custom");
                    self.status = String::from("SKIN LOADED");
                    self.load_skin(r);
                }
                Err(e) => self.status = e,
            }
        }
        if self.screen == Screen::SkinUrl {
            self.set_screen(Screen::Settings);
        }
    }

    /// Open the panel to type a web address for a skin picture.
    pub(super) fn open_skin_url(&mut self) {
        crate::client::skinweb::reset();
        self.status.clear();
        self.set_screen(Screen::SkinUrl);
        self.focus = 40;
    }

    /// The panel itself: a text box, GO and BACK, and what the download is doing.
    pub(super) fn screen_skin_url(&mut self, ui: &mut Ui) {
        use crate::client::skinweb as web;
        let s = ui.s;
        ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [8, 10, 18, 190]);
        let (x, y, bw, bh, step) = Self::column(ui, 7);
        ui.centered(ui.w() * 0.5, y - 30.0 * s, 2.0, "SKIN FROM A WEB ADDRESS", WHITE);
        ui.text(x, y - 9.0 * s, "ADDRESS OF A 64 X 64 OR 64 X 32 PICTURE", ui::DIM);
        let busy = web::state() == web::DOWNLOADING;
        let enter = ui.text_field(40, x, y, bw, bh, &mut self.skin_url, web::MAX_URL, ui::any_char);
        let half = bw * 0.5 - 2.0 * s;
        let go = ui.button(x, y + step * 1.5, half, bh, "DOWNLOAD") || enter;
        if ui.button(x + half + 4.0 * s, y + step * 1.5, half, bh, "BACK") {
            self.set_screen(Screen::Settings);
            return;
        }
        if go && !busy {
            web::start(&self.skin_url, &self.base, BUILD);
        }
        if web::take_ok() {
            let file = web::file(&self.base);
            let result = import_skin(&self.base, &file);
            sys::remove_file(&file);
            match result {
                Ok(()) => {
                    self.settings.skin = String::from("custom");
                    self.status = String::from("IMPORTED");
                    self.set_screen(Screen::Settings);
                    self.load_skin(ui.r);
                    return;
                }
                Err(e) => self.status = e,
            }
        }
        let st = web::state();
        let text = if st == web::IDLE { self.status.clone() } else { String::from(web::message(st)) };
        let colour = if web::is_failure(st) { [255, 120, 100, 255] } else { ui::ACCENT };
        ui.centered(ui.w() * 0.5, y + step * 3.0, 1.0, &text, colour);
        ui.centered(ui.w() * 0.5, y + step * 4.2, 1.0, "PASTE WITH CTRL+V (CMD+V ON A MAC)", ui::DIM);
        ui.centered(ui.w() * 0.5, y + step * 5.0, 1.0, "A DIRECT LINK TO THE PICTURE, HTTP OR HTTPS", ui::DIM);
    }
}
