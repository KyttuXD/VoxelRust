//! The title, world list, new and edit world, pause and death screens (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    pub(super) fn screen_title(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let (x, _, bw, bh, step) = Self::column(ui, 3);
        let cx = ui.w() * 0.5;
        let y = ui.h() * 0.5 + 20.0 * s;
        // The game's emblem above its name.
        let size = 64.0 * s;
        ui.r.emblem(cx - size * 0.5, y - 132.0 * s, size);
        ui.centered(cx, y - 60.0 * s, 4.0, "VOXELRUST", WHITE);
        if ui.button(x, y, bw, bh, "SINGLEPLAYER") {
            self.worlds = session::list_worlds(&self.base);
            self.status.clear();
            self.set_screen(Screen::Worlds);
        }
        if ui.button(x, y + step, bw, bh, "MULTIPLAYER") {
            self.status.clear();
            self.set_screen(Screen::Join);
            self.focus = 30;
        }
        if ui.button(x, y + step * 2.0, bw, bh, "SETTINGS") {
            self.set_screen(Screen::Settings);
        }
        if ui.button(x, y + step * 3.0, bw, bh, "QUIT") {
            self.quit = true;
        }
        let version = alloc::format!("VERSION {}  {}  {}", env!("CARGO_PKG_VERSION"), BUILD.to_ascii_uppercase(), crate::update::COMMIT.to_ascii_uppercase());
        ui.centered(cx, ui.h() - 12.0 * s, 1.0, &version, ui::DIM);
        // What the look for a newer version found.
        ui.centered(cx, ui.h() - 23.0 * s, 1.0, crate::update::message(), ui::ACCENT);
        // A credit in the bottom right corner, with a pixel heart (the
        // font has no emoji).
        let (a, b) = ("MADE WITH", "BY KYTTU");
        let (wa, wb) = (Renderer::text_width(a, s), Renderer::text_width(b, s));
        let heart = 7.0 * s;
        let total = wa + 4.0 * s + heart + 4.0 * s + wb;
        let x0 = ui.w() - 8.0 * s - total;
        let y0 = ui.h() - 12.0 * s;
        ui.text(x0, y0, a, ui::DIM);
        let hx = x0 + wa + 4.0 * s;
        // 7 x 6 pixels, one `s` square each.
        const HEART: [&str; 6] = [".XX.XX.", "XXXXXXX", "XXXXXXX", ".XXXXX.", "..XXX..", "...X..."];
        for (row, line) in HEART.iter().enumerate() {
            for (col, c) in line.chars().enumerate() {
                if c == 'X' {
                    ui.r.rect(hx + col as f32 * s, y0 - 0.5 * s + row as f32 * s, s, s, [228, 48, 78, 255]);
                }
            }
        }
        ui.text(hx + heart + 4.0 * s, y0, b, ui::DIM);
    }

    /// One world's options: export it, rename it, copy it, or delete it.
    pub(super) fn screen_edit_world(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let name = self.edit_world.clone();
        let (x, y, bw, bh, _step) = Self::column(ui, 9);
        let side = 46.0 * s;
        ui.centered(ui.w() * 0.5, y - 26.0 * s, 2.0, &name.to_ascii_uppercase(), WHITE);
        if ui.button(x, y, bw, bh, "EXPORT TO A FILE") {
            // One file holding the whole world, to copy to another machine.
            sys::make_dir(&alloc::format!("{}exports/", self.base));
            let path = alloc::format!("{}exports/{}.vxworld", self.base, name);
            self.status = match crate::save::export_world(&mut crate::save::DiskIo, &self.base, &name) {
                Some(data) if sys::write_file(&path, &data) => alloc::format!("WROTE {}", path.to_ascii_uppercase()),
                _ => String::from("COULD NOT EXPORT THAT WORLD"),
            };
        }
        // Each field has its label above it and room below, so nothing overlaps.
        ui.text(x, y + 46.0 * s, "NEW NAME (RENAME)", ui::DIM);
        ui.text_field(42, x, y + 56.0 * s, bw - side - 3.0 * s, bh, &mut self.edit_name, 24, ui::world_char);
        if ui.button(x + bw - side, y + 56.0 * s, side, bh, "RENAME") {
            let to = crate::settings::clean_world_name(&self.edit_name, 24);
            if to.is_empty() || to == name {
                self.status = String::from("TYPE A DIFFERENT NAME");
            } else if crate::session::world_exists(&self.base, &to) {
                self.status = String::from("A WORLD WITH THAT NAME EXISTS");
            } else if crate::session::rename_world(&self.base, &name, &to) {
                self.worlds = crate::session::list_worlds(&self.base);
                self.edit_world = to.clone();
                self.status = alloc::format!("RENAMED TO {}", to.to_ascii_uppercase());
            } else {
                self.status = String::from("COULD NOT RENAME THAT WORLD");
            }
        }
        ui.text(x, y + 92.0 * s, "COPY AS", ui::DIM);
        ui.text_field(43, x, y + 102.0 * s, bw - side - 3.0 * s, bh, &mut self.form_import, 24, ui::world_char);
        if ui.button(x + bw - side, y + 102.0 * s, side, bh, "COPY") {
            let to = crate::settings::clean_world_name(&self.form_import, 24);
            if to.is_empty() || to == name {
                self.status = String::from("TYPE A DIFFERENT NAME");
            } else if crate::session::copy_world(&self.base, &name, &to) {
                self.worlds = crate::session::list_worlds(&self.base);
                self.status = alloc::format!("COPIED AS {}", to.to_ascii_uppercase());
            } else {
                self.status = String::from("COULD NOT COPY THAT WORLD (THE NAME MAY BE TAKEN)");
            }
        }
        // Deleting takes two clicks, so one slip does not lose a world.
        let label = if self.edit_confirm { "CLICK AGAIN TO DELETE" } else { "DELETE THIS WORLD" };
        if ui.button(x, y + 140.0 * s, bw, bh, label) {
            if self.edit_confirm {
                if crate::session::delete_world(&self.base, &name) {
                    self.worlds = crate::session::list_worlds(&self.base);
                    self.status = alloc::format!("DELETED {}", name.to_ascii_uppercase());
                    self.set_screen(Screen::Worlds);
                    return;
                }
                self.status = String::from("COULD NOT DELETE THAT WORLD");
                self.edit_confirm = false;
            } else {
                self.edit_confirm = true;
                self.status = String::from("CLICK AGAIN TO DELETE IT FOR GOOD");
            }
        }
        if ui.button(x, y + 166.0 * s, bw, bh, "BACK") {
            self.edit_confirm = false;
            self.worlds = crate::session::list_worlds(&self.base);
            self.set_screen(Screen::Worlds);
        }
        ui.centered(ui.w() * 0.5, y + 192.0 * s, 1.0, &self.status, ui::ACCENT);
    }

    pub(super) fn screen_worlds(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let shown = self.worlds.len().min(8);
        let (x, y, bw, bh, step) = Self::column(ui, shown.max(1) + 6);
        ui.centered(ui.w() * 0.5, y - 26.0 * s, 2.0, "WORLDS", WHITE);
        let mut open = None;
        let mut edit = None;
        let side = 46.0 * s;
        // More worlds than fit: the wheel scrolls the list.
        let hidden = self.worlds.len().saturating_sub(8) as i32;
        self.worlds_scroll = (self.worlds_scroll - ui.inp.wheel).clamp(0, hidden);
        if hidden > 0 {
            ui.centered(ui.w() * 0.5, y - 9.0 * s, 1.0, "SCROLL FOR MORE", ui::DIM);
        }
        for (i, name) in self.worlds.iter().skip(self.worlds_scroll as usize).take(8).enumerate() {
            if ui.button(x, y + i as f32 * step, bw - side - 3.0 * s, bh, &name.to_ascii_uppercase()) {
                open = Some(name.clone());
            }
            if ui.button(x + bw - side, y + i as f32 * step, side, bh, "EDIT") {
                edit = Some(name.clone());
            }
        }
        if let Some(name) = edit {
            self.edit_name = name.clone();
            self.edit_world = name;
            self.edit_confirm = false;
            self.status.clear();
            self.set_screen(Screen::EditWorld);
            return;
        }
        if self.worlds.is_empty() {
            ui.centered(ui.w() * 0.5, y + 4.0 * s, 1.0, "NO WORLDS YET", ui::DIM);
        }
        let by = y + shown.max(1) as f32 * step + 6.0 * s;
        if ui.button(x, by, bw, bh, "CREATE A NEW WORLD") {
            self.form_name = alloc::format!("world{}", self.worlds.len() + 1);
            self.form_seed.clear();
            self.status.clear();
            self.set_screen(Screen::NewWorld);
        }
        ui.text(x, by + step + 5.0 * s, "IMPORT A .VXWORLD FILE (PATH)", ui::DIM);
        ui.text_field(30, x, by + step * 2.0, bw - side - 3.0 * s, bh, &mut self.form_import, 200, ui::any_char);
        if ui.button(x + bw - side, by + step * 2.0, side, bh, "IMPORT") {
            let path = String::from(self.form_import.trim());
            // The world is named after the file.
            let stem = path.rsplit(|c| c == '/' || c == '\\').next().unwrap_or("").trim_end_matches(".vxworld");
            let mut name = crate::settings::clean_world_name(stem, 24);
            if name.is_empty() {
                name = String::from("imported");
            }
            let mut n = 2;
            let first = name.clone();
            while crate::session::world_exists(&self.base, &name) {
                name = alloc::format!("{}-{}", first, n);
                n += 1;
            }
            // A bare name means a file in the data folder.
            let data = sys::read_file(&path).or_else(|| sys::read_file(&alloc::format!("{}{}", self.base, path))).or_else(|| sys::read_file(&alloc::format!("{}{}.vxworld", self.base, path)));
            self.status = match data {
                Some(d) if crate::save::import_world(&mut crate::save::DiskIo, &self.base, &name, &d) => {
                    crate::session::remember_world(&self.base, &name);
                    self.worlds = crate::session::list_worlds(&self.base);
                    alloc::format!("IMPORTED AS {}", name.to_ascii_uppercase())
                }
                Some(_) => String::from("THAT IS NOT A VOXELRUST WORLD FILE"),
                None => String::from("CANNOT READ THAT FILE"),
            };
        }
        if ui.button(x, by + step * 3.0, bw, bh, "BACK") {
            self.set_screen(Screen::Title);
        }
        let status = self.status.clone();
        ui.centered(ui.w() * 0.5, by + step * 4.2, 1.0, &status, ui::ACCENT);
        if let Some(name) = open {
            // The saved world decides its own seed, mode and kind.
            let new = NewWorld { seed: 0, mode: GameMode::Survival, kind: WorldKind::Normal };
            self.start_world(ui.r, &name, &new);
        }
    }

    pub(super) fn screen_new_world(&mut self, ui: &mut Ui) {
        let s = ui.s;
        let (x, y, bw, bh, step) = Self::column(ui, 7);
        ui.centered(ui.w() * 0.5, y - 30.0 * s, 2.0, "NEW WORLD", WHITE);
        ui.text(x, y - 9.0 * s, "NAME", ui::DIM);
        ui.text_field(1, x, y, bw, bh, &mut self.form_name, 24, ui::world_char);
        ui.text(x, y + step * 1.5 - 9.0 * s, "SEED (EMPTY = RANDOM)", ui::DIM);
        ui.text_field(2, x, y + step * 1.5, bw, bh, &mut self.form_seed, 24, ui::any_char);
        let mode = alloc::format!("GAME MODE  {}", if self.form_mode == GameMode::Survival { "SURVIVAL" } else { "CREATIVE" });
        if ui.button_lr(x, y + step * 2.7, bw, bh, &mode, true) != 0 {
            self.form_mode = if self.form_mode == GameMode::Survival { GameMode::Creative } else { GameMode::Survival };
        }
        // Normal, flat, and whatever terrain the mods' own code offers.
        let offered: Vec<(usize, &'static str)> = crate::native::active().filter_map(|(k, n)| n.terrain_label.and_then(|label| label()).map(|l| (k, l))).collect();
        let chosen = self.form_native.and_then(|k| offered.iter().position(|o| o.0 == k));
        if chosen.is_none() {
            self.form_native = None;
        }
        let kind = alloc::format!("TERRAIN  {}", chosen.map_or(if self.form_flat { "FLAT" } else { "NORMAL" }, |c| offered[c].1));
        let turn = ui.button_lr(x, y + step * 3.7, bw, bh, &kind, true);
        if turn != 0 {
            let now = chosen.map_or(self.form_flat as i32, |c| 2 + c as i32);
            let next = (now + turn).rem_euclid(2 + offered.len() as i32);
            self.form_flat = next == 1;
            self.form_native = if next >= 2 { Some(offered[next as usize - 2].0) } else { None };
        }
        // That code's own options for it, under the button.
        let form = self.form_native.and_then(|k| crate::native::all()[k].terrain_form);
        let rows = form.map_or(0.0, |form| form(ui, x, y + step * 4.7, bw, bh, step));
        let y = y + step * rows;
        if ui.button(x, y + step * 5.0, bw, bh, "CREATE") {
            let name = crate::settings::clean_world_name(&self.form_name, 24);
            if name.is_empty() {
                self.status = String::from("GIVE THE WORLD A NAME");
            } else if session::world_exists(&self.base, &name) {
                self.status = String::from("A WORLD WITH THAT NAME EXISTS");
            } else {
                let seed = if self.form_seed.trim().is_empty() {
                    (unsafe { (sdl::sdl().SDL_GetPerformanceCounter)() } as u32) ^ 0x5EED_1234
                } else {
                    seed_from(self.form_seed.trim())
                };
                let custom = self.form_native.and_then(|k| crate::native::all()[k].terrain_chosen).map(|chosen| chosen());
                let kind = if let Some(kind) = custom.and_then(|text| WorldKind::parse(&text)) {
                    kind
                } else if self.form_flat {
                    WorldKind::Flat
                } else {
                    WorldKind::Normal
                };
                self.start_world(ui.r, &name, &NewWorld { seed, mode: self.form_mode, kind });
                return;
            }
        }
        if ui.button(x, y + step * 6.0, bw, bh, "BACK") {
            self.set_screen(Screen::Worlds);
        }
        ui.centered(ui.w() * 0.5, y + step * 7.2, 1.0, &self.status, ui::ACCENT);
    }

    pub(super) fn screen_pause(&mut self, ui: &mut Ui) {
        let s = ui.s;
        ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [8, 10, 18, 150]);
        let (x, y, bw, bh, step) = Self::column(ui, 3);
        ui.centered(ui.w() * 0.5, y - 26.0 * s, 2.0, "PAUSED", WHITE);
        if ui.button(x, y, bw, bh, "BACK TO THE GAME") {
            self.set_screen(Screen::Game);
        }
        if ui.button(x, y + step, bw, bh, "SETTINGS") {
            self.set_screen(Screen::Settings);
        }
        if ui.button(x, y + step * 2.0, bw, bh, "SAVE AND QUIT TO TITLE") {
            self.close_world(ui.r);
            self.set_screen(Screen::Title);
        }
    }

    pub(super) fn screen_dead(&mut self, ui: &mut Ui) {
        let s = ui.s;
        ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [90, 8, 8, 150]);
        let (x, y, bw, bh, step) = Self::column(ui, 2);
        ui.centered(ui.w() * 0.5, y - 34.0 * s, 3.0, "YOU DIED", WHITE);
        let respawn = ui.button(x, y, bw, bh, "RESPAWN");
        let leave = ui.button(x, y + step, bw, bh, "SAVE AND QUIT TO TITLE");
        if respawn || leave {
            if let Some(g) = self.session.as_mut() {
                g.respawn();
            }
            if leave {
                self.close_world(ui.r);
            }
            self.set_screen(if leave { Screen::Title } else { Screen::Game });
        }
    }

    /// A centred column of full-width rows: (x, first y, width, row height, step).
    pub(super) fn column(ui: &Ui, rows: usize) -> (f32, f32, f32, f32, f32) {
        let s = ui.s;
        // Below 1000 pixels wide the buttons widen, so their labels (and
        // the halves of rows) still fit at the small interface size.
        let wide = if ui.w() < 1000.0 { 1.6 } else { 1.0 };
        let (bw, bh, step) = (200.0 * s * wide, 16.0 * s, 19.0 * s);
        ((ui.w() - bw) * 0.5, ((ui.h() - rows as f32 * step) * 0.5 + 14.0 * s).max(40.0 * s), bw, bh, step)
    }
}
