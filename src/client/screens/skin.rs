//! Skins: loading, the file picker, dropped files and the web address panel (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    /// (Re)build the player's skin texture from the settings. The picture goes
    /// to the graphics card here; packing it for the network (a big one takes
    /// seconds) is left to a worker (`client/skins`), whose answer is taken in
    /// the frame update.
    pub(super) fn load_skin(&mut self, r: &mut Renderer) {
        self.skin_for = self.settings.player_name.clone();
        let who = crate::settings::clean_name(&self.settings.player_name, 16);
        // Any custom skin still being unpacked from before is forgotten.
        self.skins.cancel_load();
        // A big custom skin is unpacked on a worker (a 4096 one takes about 0.4 s: the first
        // frame must not wait for it); the built-in skin is shown until it is ready.
        let mut wait_for_custom = false;
        if self.settings.skin == "custom" {
            if let Some(bytes) = sys::read_file_limit(&alloc::format!("{}skin.png", self.base), crate::image::MAX_BYTES) {
                let side = crate::image::header::header(&bytes).map_or(0, |h| h.w as usize);
                if side > crate::client::skins::work::INLINE_SIDE {
                    self.skins.load_custom(side, bytes);
                    wait_for_custom = true;
                }
            }
        }
        let (side, px) = match if wait_for_custom { skin_pixels(&self.base, "default", &who) } else { skin_pixels(&self.base, &self.settings.skin, &who) } {
            Some(found) => found,
            None => {
                self.status = String::from("NO USABLE SKIN.PNG (A SQUARE MULTIPLE OF 64, UP TO 4096) IN THE DATA FOLDER");
                self.settings.skin = String::from("default");
                match skin_pixels(&self.base, "default", &who) {
                    Some(found) => found,
                    None => return,
                }
            }
        };
        self.show_skin(r, side, px);
    }

    /// The custom skin a worker unpacked (see `load_skin`), once it is: shown and packed for the network.
    pub(super) fn take_loaded_skin(&mut self, r: &mut Renderer) {
        match self.skins.take_loaded() {
            Some(Some((side, px))) => self.show_skin(r, side, px),
            Some(None) => {
                self.status = String::from("NO USABLE SKIN.PNG (A SQUARE MULTIPLE OF 64, UP TO 4096) IN THE DATA FOLDER");
                self.settings.skin = String::from("default");
                self.load_skin(r);
            }
            None => {}
        }
    }

    /// Put a skin on the card (over the one made before) and have it packed for sending.
    fn show_skin(&mut self, r: &mut Renderer, side: usize, px: Vec<u8>) {
        // The texture made the last time is written over, not left on the card (a different size makes a new one).
        if self.skin_tex == 0 {
            self.skin_tex = r.add_skin_texture(side, &px);
        } else if self.skin_tex_side == side {
            r.update_skin_texture(self.skin_tex, side, &px);
        } else {
            r.delete_textures(&[self.skin_tex]);
            self.skin_tex = r.add_skin_texture(side, &px);
        }
        self.skin_tex_side = side;
        self.skin_slim = crate::client::skinfile::is_slim(&px, side, side);
        // The pixels are handed on and not kept: only the card has them from here.
        self.skin_png.clear();
        self.skin_small.clear();
        self.skin_sent = false;
        self.skins.pack_own(side, px);
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
        self.picker.open("CHOOSE A SKIN: A SQUARE PICTURE, 64 UP TO 4096", &start, &fallbacks, SKIN_EXTS, &self.base);
        self.picker.size_ok = Some(crate::client::skinfile::size_ok);
        self.picker.hint = String::from("SKINS: 64 TO 4096, MULTIPLES OF 64 (OR 64 X 32)");
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
        ui.text(x, y - 9.0 * s, "ADDRESS OF A SKIN PICTURE (UP TO 4096 X 4096)", ui::DIM);
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
