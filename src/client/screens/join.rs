//! The multiplayer screens: the server list, joining, adding a server, hosting (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    pub(super) fn save_servers(&self) {
        let text: String = self.servers.iter().map(|(n, a)| alloc::format!("{} {}\n", n, a)).collect();
        sys::write_file_atomic(&alloc::format!("{}servers.txt", self.base), text.as_bytes());
    }

    pub(super) fn join_address(&mut self, r: &mut Renderer, address: &str) -> bool {
        let name = crate::settings::clean_name(&self.settings.player_name, 16);
        match Session::join(&self.base, address, if name.is_empty() { "Player" } else { &name }, self.threads) {
            Ok(g) => {
                self.close_world(r);
                let mut g = g;
                g.who = crate::settings::clean_name(&self.settings.player_name, 16);
                self.session = Some(g);
                self.distance = self.settings.distance;
                self.set_screen(Screen::Game);
                true
            }
            Err(e) => {
                self.status = e;
                false
            }
        }
    }

    /// The multiplayer screen: the servers you have added, as a list.
    pub(super) fn screen_join(&mut self, ui: &mut Ui) {
        let s = ui.s;
        if !self.servers_loaded {
            // Saved servers: one "name address" per line.
            self.servers_loaded = true;
            self.servers = sys::read_file(&alloc::format!("{}servers.txt", self.base))
                .map(|b| String::from_utf8_lossy(&b).lines().filter_map(|l| l.trim().rsplit_once(' ').map(|(n, a)| (String::from(n.trim()), String::from(a.trim()))))
                    .filter(|(_, a)| !a.is_empty() && a.len() <= 120)
                    .take(64)
                    .collect())
                .unwrap_or_default();
        }
        const ROWS: usize = 6;
        let (bw, bh, step) = (260.0 * s, 16.0 * s, 19.0 * s);
        let x = (ui.w() - bw) * 0.5;
        let y0 = ((ui.h() - (ROWS as f32 + 5.5) * step) * 0.5 + 14.0 * s).max(40.0 * s);
        ui.centered(ui.w() * 0.5, y0 - 30.0 * s, 2.0, "MULTIPLAYER", WHITE);
        ui.panel(x - 4.0 * s, y0 - 4.0 * s, bw + 8.0 * s, ROWS as f32 * step + 5.0 * s);
        let total = self.servers.len();
        if ui.hot(x, y0, bw, ROWS as f32 * step) {
            self.server_scroll -= ui.inp.wheel;
        }
        self.server_scroll = self.server_scroll.clamp(0, total.saturating_sub(ROWS) as i32);
        if total == 0 {
            ui.centered(ui.w() * 0.5, y0 + 2.2 * step, 1.0, "NO SERVERS YET", ui::DIM);
            ui.centered(ui.w() * 0.5, y0 + 3.0 * step, 1.0, "ADD ONE BELOW, OR HOST YOUR OWN", ui::DIM);
        }
        let (edit_w, del_w, bar) = (34.0 * s, 16.0 * s, 5.0 * s);
        let row_w = bw - edit_w - del_w - bar - 9.0 * s;
        let mut join: Option<String> = None;
        for row in 0..ROWS.min(total) {
            let k = row + self.server_scroll as usize;
            if k >= self.servers.len() {
                break;
            }
            let (name, address) = self.servers[k].clone();
            let ry = y0 + row as f32 * step;
            if ui.button(x, ry, row_w, bh, &alloc::format!("{}   {}", name, address).to_ascii_uppercase()) {
                join = Some(address.clone());
            }
            if ui.button(x + row_w + 3.0 * s, ry, edit_w, bh, "EDIT") {
                self.form_server = name;
                self.form_address = address;
                self.server_edit = Some(k);
                self.status.clear();
                self.set_screen(Screen::AddServer);
                self.focus = 30;
                return;
            }
            if ui.button(x + row_w + edit_w + 6.0 * s, ry, del_w, bh, "X") {
                self.servers.remove(k);
                self.save_servers();
                break;
            }
        }
        ui.scrollbar(x + bw - bar, y0, bar, ROWS as f32 * step - 3.0 * s, &mut self.server_scroll, ROWS, total);
        if let Some(address) = join {
            if self.join_address(ui.r, &address) {
                return;
            }
        }
        let y = y0 + ROWS as f32 * step + 12.0 * s;
        let half = bw * 0.5 - 2.0 * s;
        if ui.button(x, y, half, bh, "ADD SERVER") {
            self.form_server.clear();
            self.form_address.clear();
            self.server_edit = None;
            self.status.clear();
            self.set_screen(Screen::AddServer);
            self.focus = 33;
            return;
        }
        ui.text(x + half + 4.0 * s, y - 9.0 * s, "YOUR NAME", ui::DIM);
        ui.text_field_case(31, x + half + 4.0 * s, y, half, bh, &mut self.settings.player_name, 16, ui::name_char, ui::FieldCase::Upper);
        // Hosting: a server runs inside the game for as long as you play.
        ui.text(x + half + 4.0 * s, y + step * 1.5 - 9.0 * s, "SHARED WORLD NAME", ui::DIM);
        ui.text_field(32, x + half + 4.0 * s, y + step * 1.5, half, bh, &mut self.form_host, 24, ui::world_char);
        if ui.button(x, y + step * 1.5, half, bh, "HOST A GAME") {
            self.status.clear();
            self.host_game(ui.r);
            if self.session.is_some() {
                return;
            }
        }
        if ui.button(x, y + step * 2.7, bw, bh, "BACK") {
            self.set_screen(Screen::Title);
        }
        // The project's own server, underneath everything else.
        if ui.button(x, y + step * 3.5, bw, bh, "PUBLIC SERVER") {
            self.status.clear();
            if self.join_address(ui.r, PUBLIC_SERVER) {
                return;
            }
        }
        ui.centered(ui.w() * 0.5, y + step * 4.4, 1.0, &self.status, ui::ACCENT);
        let mine = self.my_address.get_or_insert_with(sys::net::local_address).as_ref().map(|a| alloc::format!("YOUR ADDRESS ON THIS NETWORK: {}", a)).unwrap_or_default();
        ui.centered(ui.w() * 0.5, y + step * 5.2, 1.0, &mine, ui::DIM);
    }

    /// Add a server to the list, or change one already there.
    pub(super) fn screen_add_server(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let (x, y, bw, bh, step) = Self::column(ui, 7);
        ui.centered(ui.w() * 0.5, y - 30.0 * s, 2.0, if self.server_edit.is_some() { "EDIT SERVER" } else { "ADD SERVER" }, WHITE);
        ui.text(x, y - 9.0 * s, "SERVER NAME", ui::DIM);
        ui.text_field(33, x, y, bw, bh, &mut self.form_server, 24, ui::world_char);
        ui.text(x, y + step * 1.5 - 9.0 * s, "SERVER ADDRESS (HOST OR HOST:PORT)", ui::DIM);
        let enter = ui.text_field(30, x, y + step * 1.5, bw, bh, &mut self.form_address, 60, ui::any_char);
        if ui.inp.pressed(ui::KEY_TAB) {
            *ui.focus = if *ui.focus == 33 { 30 } else { 33 };
        }
        let half = bw * 0.5 - 2.0 * s;
        let save = ui.button(x, y + step * 2.8, half, bh, "SAVE") || enter;
        let save_join = ui.button(x + half + 4.0 * s, y + step * 2.8, half, bh, "SAVE AND JOIN");
        if save || save_join {
            let address: String = self.form_address.chars().filter(|c| !c.is_whitespace()).collect();
            if address.is_empty() {
                self.status = String::from("TYPE THE SERVER'S ADDRESS");
            } else {
                let name = if self.form_server.is_empty() { alloc::format!("server{}", self.servers.len() + 1) } else { self.form_server.clone() };
                match self.server_edit {
                    Some(k) if k < self.servers.len() => self.servers[k] = (name, address.clone()),
                    _ if self.servers.len() < 64 => self.servers.push((name, address.clone())),
                    _ => {}
                }
                self.save_servers();
                self.status.clear();
                self.set_screen(Screen::Join);
                if save_join {
                    self.join_address(ui.r, &address);
                }
                return;
            }
        }
        if ui.button(x, y + step * 4.0, bw, bh, "CANCEL") {
            self.status.clear();
            self.set_screen(Screen::Join);
        }
        ui.centered(ui.w() * 0.5, y + step * 5.2, 1.0, &self.status, ui::ACCENT);
        ui.centered(ui.w() * 0.5, y + step * 6.0, 1.0, "CTRL+V PASTES AN ADDRESS", ui::DIM);
    }

    /// Start a server inside this program and join it.
    pub(super) fn host_game(&mut self, r: &mut Renderer) {
        self.close_world(r);
        let name = crate::settings::clean_world_name(&self.form_host, 24);
        let port = crate::proto::DEFAULT_PORT;
        let (server, edits) = match open_server(if name.is_empty() { "shared" } else { &name }, None, None, port, 16, &self.base, self.pack_text.bytes(read_mod_file), crate::client::mods::serving::served(&self.pack_paths, &self.settings.mods_off, self.serve_mods.as_deref()), &self.settings) {
            Ok(v) => v,
            Err(e) => {
                self.status = e.to_ascii_uppercase();
                return;
            }
        };
        let port = server.port();
        // The server holds raw sockets; it is handed to its thread whole
        // and never touched from here again.
        struct Parcel(crate::server::Server, String);
        unsafe impl Send for Parcel {}
        let parcel = Parcel(server, edits);
        self.host = sys::thread::spawn(alloc::boxed::Box::new(move || {
            let p = parcel;
            let _ = server_loop(p.0, p.1, true);
        }));
        if self.host.is_none() {
            self.status = String::from("COULD NOT START THE SERVER");
            return;
        }
        let player = crate::settings::clean_name(&self.settings.player_name, 16);
        match Session::join(&self.base, &alloc::format!("127.0.0.1:{}", port), if player.is_empty() { "Player" } else { &player }, self.threads) {
            Ok(mut g) => {
                let address = sys::net::local_address().unwrap_or_else(|| String::from("YOUR ADDRESS"));
                g.push_chat(alloc::format!("HOSTING. OTHERS JOIN WITH  {}:{}", address, port));
                g.push_chat(String::from("FROM OUTSIDE YOUR NETWORK, FORWARD THAT PORT ON YOUR ROUTER."));
                let mut g = g;
                g.who = crate::settings::clean_name(&self.settings.player_name, 16);
                self.session = Some(g);
                self.distance = self.settings.distance;
                self.set_screen(Screen::Game);
            }
            Err(e) => {
                self.status = e.to_ascii_uppercase();
                self.close_world(r);
            }
        }
    }
}
