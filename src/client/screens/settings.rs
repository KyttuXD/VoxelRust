//! The settings screen and the reloading of packs (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    pub(super) fn screen_settings(&mut self, ui: &mut Ui) {
        let s = ui.s;
        ui.r.rect(0.0, 0.0, ui.w(), ui.h(), [8, 10, 18, 190]);
        let (_, y, _, bh, step) = Self::column(ui, 13);
        // Wider than the other menus: key names need the room.
        let bw = (360.0 * s).min(ui.w() - 8.0 * s);
        let x = (ui.w() - bw) * 0.5;
        let top = (y - 10.0 * s).max(24.0 * s);
        ui.centered(ui.w() * 0.5, top - 20.0 * s, 2.0, "SETTINGS", WHITE);
        let tw = bw / TABS.len() as f32;
        for (i, t) in TABS.iter().enumerate() {
            let tx = x + i as f32 * tw;
            if ui.button(tx, top, tw - 2.0 * s, bh, t) {
                self.tab = i;
                self.video_detail = 0;
                self.rebinding = None;
                *ui.focus = 0;
            }
            if i == self.tab {
                ui.r.rect(tx, top + bh, tw - 2.0 * s, 2.0 * s, ui::ACCENT);
            }
        }
        let y = top + bh + 8.0 * s;
        let row = |i: usize| y + i as f32 * step;
        let mut changed = false;
        let mut fullscreen_clicked = false;
        let mut mode_changed = false;
        let (mut skin_changed, mut skin_pick, mut skin_web) = (false, false, false);
        {
            let st = &mut self.settings;
            match self.tab {
                0 if self.video_detail == 1 => {
                    let half = (bw - 4.0 * s) * 0.5;
                    if ui.button(x, row(0), bw, bh, "< BACK TO VIDEO") {
                        self.video_detail = 0;
                    }
                    changed |= ui.stepper(x, row(1), bw, bh, "VIEW DISTANCE", &mut st.distance, 2, 48, 1);
                    changed |= ui.stepper(x, row(2), bw, bh, "DISTANT TERRAIN", &mut st.far_distance, 0, 256, 16);
                    let real = if st.lod_real == 0 { String::from("DISTANT BUILDS AND TREES  < OFF >") } else { alloc::format!("DISTANT BUILDS AND TREES  < {} TILES >", st.lod_real) };
                    changed |= ui.stepper_text(x, row(3), bw, bh, &real, &mut st.lod_real, 0, 8, 1);
                    let trees = ["NONE", "SOME", "ALL"][st.lod_trees.clamp(0, 2) as usize];
                    changed |= ui.stepper_text(x, row(4), half, bh, &alloc::format!("TREES  < {} >", trees), &mut st.lod_trees, 0, 2, 1);
                    let detail = ["COARSE", "MEDIUM", "FINE"][st.lod_detail.clamp(0, 2) as usize];
                    changed |= ui.stepper_text(x + half + 4.0 * s, row(4), half, bh, &alloc::format!("DETAIL  < {} >", detail), &mut st.lod_detail, 0, 2, 1);
                    let rs = alloc::format!("3D DETAIL  < {}% >", st.render_scale);
                    changed |= step_list(ui, x, row(5), bw, bh, &rs, &mut st.render_scale, &[33, 50, 67, 75, 100]);
                    changed |= ui.toggle(x, row(6), bw, bh, "CLOUDS", &mut st.clouds);
                }
                0 => {
                    if ui.button(x, row(0), bw, bh, "DETAIL AND DISTANCE...") {
                        self.video_detail = 1;
                    }
                    changed |= ui.stepper(x, row(1), bw, bh, "FIELD OF VIEW", &mut st.fov, 50, 110, 5);
                    let gs = if st.gui_scale == 0 { String::from("INTERFACE SIZE  < AUTO >") } else { alloc::format!("INTERFACE SIZE  < {} >", st.gui_scale) };
                    changed |= ui.stepper_text(x, row(2), bw, bh, &gs, &mut st.gui_scale, 0, 6, 1);
                    let tf = if st.target_fps == 0 { String::from("HOLD FRAME RATE  < OFF >") } else { alloc::format!("HOLD FRAME RATE  < {} >", st.target_fps) };
                    changed |= step_list(ui, x, row(3), bw, bh, &tf, &mut st.target_fps, &[0, 30, 60, 85, 120, 144, 250]);
                    let mf = if st.max_fps == 0 { String::from("FRAME RATE LIMIT  < NONE >") } else { alloc::format!("FRAME RATE LIMIT  < {} >", st.max_fps) };
                    changed |= step_list(ui, x, row(4), bw, bh, &mf, &mut st.max_fps, &[0, 30, 60, 85, 120, 144, 250, 500]);
                    changed |= ui.toggle(x, row(5), bw, bh, "VSYNC", &mut st.vsync);
                    changed |= ui.toggle(x, row(6), bw, bh, "SHOW FRAME RATE", &mut st.show_fps);
                    let half = (bw - 4.0 * s) * 0.5;
                    let mut fs = st.fullscreen;
                    fullscreen_clicked = ui.toggle(x, row(7), half, bh, "FULL SCREEN", &mut fs);
                    let (mw, mh) = crate::settings::FULLSCREEN_MODES[st.fullscreen_mode as usize];
                    let label = if mw == 0 { String::from("AT  < DESKTOP SIZE >") } else { alloc::format!("AT  < {}X{} >", mw, mh) };
                    mode_changed = ui.stepper_text(x + half + 4.0 * s, row(7), half, bh, &label, &mut st.fullscreen_mode, 0, crate::settings::FULLSCREEN_MODES.len() as i32 - 1, 1);
                }
                1 => {
                    changed |= ui.stepper(x, row(0), bw, bh, "MASTER VOLUME", &mut st.master_volume, 0, 10, 1);
                    changed |= ui.stepper(x, row(1), bw, bh, "EFFECTS", &mut st.effects_volume, 0, 10, 1);
                    changed |= ui.stepper(x, row(2), bw, bh, "FOOTSTEPS", &mut st.steps_volume, 0, 10, 1);
                    changed |= ui.stepper(x, row(3), bw, bh, "INTERFACE", &mut st.ui_volume, 0, 10, 1);
                    changed |= ui.toggle(x, row(5), bw, bh, "SUBTITLES FOR SOUNDS", &mut st.subtitles);
                    if !self.audio.available() {
                        ui.centered(ui.w() * 0.5, row(4) + 4.0 * s, 1.0, "NO SOUND DEVICE IS IN USE", ui::ACCENT);
                    }
                }
                2 if self.video_detail == 1 => {
                    let half = (bw - 4.0 * s) * 0.5;
                    if ui.button(x, row(0), bw, bh, "< BACK TO CONTROLS") {
                        self.video_detail = 0;
                    }
                    for (i, a) in ACTIONS.iter().enumerate() {
                        // Two columns of key bindings.
                        let (cx, cy) = (x + (i % 2) as f32 * (half + 4.0 * s), row(1 + i / 2));
                        let label = if self.rebinding == Some(i) {
                            String::from("PRESS A KEY")
                        } else {
                            alloc::format!("{}: {}", a.2, key_name(st.keys[i]))
                        };
                        if ui.button(cx, cy, half, bh, &label) {
                            self.rebinding = Some(i);
                        }
                    }
                    if ui.button(x, row(10), bw, bh, "RESET KEYS") {
                        st.keys = Settings::default().keys;
                    }
                }
                2 => {
                    let half = (bw - 4.0 * s) * 0.5;
                    changed |= ui.stepper(x, row(0), half, bh, "MOUSE SPEED", &mut st.sens, 1, 20, 1);
                    changed |= ui.toggle(x + half + 4.0 * s, row(0), half, bh, "INVERT MOUSE", &mut st.invert_y);
                    if ui.button(x, row(1), bw, bh, "KEY BINDINGS...") {
                        self.video_detail = 1;
                    }
                }
                4 => {
                    // A turning preview on the left, choices on the right.
                    let model = reg().model(arm_models(st.slim_arms).0);
                    ui.panel(x, row(0), 100.0 * s, 150.0 * s);
                    if let Some(model) = model {
                        let pose = crate::model::Pose { phase: ui.clock * 3.0, stride: 0.5, ..Default::default() };
                        ui.r.tex_override = self.skin_tex;
                        ui.r.draw_model_2d(model, &pose, x + 50.0 * s, row(0) + 138.0 * s, ui.clock * 0.6, 120.0 * s);
                    }
                    let (cx, cw) = (x + 108.0 * s, bw - 108.0 * s);
                    let list = crate::settings::SKINS;
                    let mut i = list.iter().position(|k| *k == st.skin).map_or(list.len() as i32, |i| i as i32);
                    let label = alloc::format!("LOOK  < {} >", st.skin.to_ascii_uppercase());
                    if ui.stepper_text(cx, row(0), cw, bh, &label, &mut i, 0, list.len() as i32, 1) {
                        st.skin = String::from(if (i as usize) < list.len() { list[i as usize] } else { "custom" });
                        skin_changed = true;
                    }
                    ui.text(cx, row(1) + 2.0 * s, "YOUR OWN: A 64 X 64 PICTURE IN", ui::DIM);
                    ui.text(cx, row(2) - 6.0 * s, "THE USUAL SKIN LAYOUT (64 X 32 TOO).", ui::DIM);
                    if ui.button(cx, row(3) - 4.0 * s, cw, bh, "CHOOSE A SKIN FILE...") {
                        skin_pick = true;
                    }
                    if ui.button(cx, row(4) - 2.0 * s, cw, bh, "IMPORT FROM WEB ADDRESS...") {
                        skin_web = true;
                    }
                    skin_changed |= ui.toggle(cx, row(5) + 4.0 * s, cw, bh, "SLIM ARMS (ALEX SKINS)", &mut st.slim_arms);
                }
                5 => {
                    // Every mod in the mods folder, with its version and a
                    // switch (changes take effect at once), then the ones
                    // the server sent for this session, which stay as they are.
                    let installed = all_mods(&self.pack_paths);
                    let sent: Vec<(String, String)> = self.session.as_ref().and_then(|g| g.mod_sync.as_ref()).map(|m| m.taken.iter().map(|t| (t.name.clone(), t.version.clone())).collect()).unwrap_or_default();
                    let list = crate::client::mods::rows::rows(&installed, &st.mods_off, &sent);
                    let hidden = list.len().saturating_sub(6) as i32;
                    self.mods_scroll = (self.mods_scroll - ui.inp.wheel).clamp(0, hidden);
                    ui.text(x, row(0) - 9.0 * s, if list.is_empty() { "NO MODS INSTALLED (SEE THE README IN THE MODS FOLDER)" } else { "MODS (PUT YOUR OWN IN THE MODS FOLDER)" }, ui::DIM);
                    for (k, m) in list.iter().skip(self.mods_scroll as usize).take(6).enumerate() {
                        let mut on = m.on;
                        let flipped = ui.toggle(x, row(k), bw, bh, &m.label(), &mut on);
                        if flipped && !m.from_server {
                            st.mods_off = crate::client::mods::rows::toggled(&st.mods_off, &m.name, on);
                            changed = true;
                            self.want_reload = true;
                        }
                    }
                    if hidden > 0 {
                        ui.text(x + bw - 70.0 * s, row(0) - 9.0 * s, "SCROLL FOR MORE", ui::DIM);
                    }
                    // Bring one in from elsewhere: a folder, a .zip or a pack file.
                    ui.text(x, row(7) - 9.0 * s, "ADD A MOD FROM A FOLDER OR .ZIP (TYPE ITS PATH)", ui::DIM);
                    ui.text_field(41, x, row(7), bw - 44.0 * s, bh, &mut self.form_mod_path, 200, ui::any_char);
                    if ui.button(x + bw - 41.0 * s, row(7), 41.0 * s, bh, "ADD") {
                        self.status = import_mod(&self.mods_dir, &self.form_mod_path);
                        if self.status.starts_with("ADDED") {
                            self.form_mod_path.clear();
                            self.want_reload = true;
                        }
                    }
                    let note = self.status.clone();
                    ui.text(x, row(8) + 2.0 * s, &note, ui::ACCENT);
                    changed |= ui.toggle(x, row(9), bw, bh, "UPDATE THE GAME AUTOMATICALLY", &mut st.auto_update);
                }
                6 => {
                    let at = ACTIONS.iter().position(|a| a.0 == Action::Voice).unwrap_or(0);
                    let label = if self.rebinding == Some(at) { String::from("PRESS A KEY") } else { key_name(st.keys[at]) };
                    let online = self.session.as_ref().map_or(false, |g| g.net.is_some());
                    let allowed = self.session.as_ref().map_or(false, |g| g.perms.map_or(true, |f| f & crate::proto::PERM_VOICE != 0));
                    let asks = crate::client::voice::menu::draw(ui, st, &mut self.voice, (x, row(0), bw, bh, step), &label, online, allowed);
                    changed |= asks.changed;
                    if asks.rebind {
                        self.rebinding = Some(at);
                    }
                }
                _ if self.video_detail == 1 => {
                    let half = (bw - 4.0 * s) * 0.5;
                    if ui.button(x, row(0), bw, bh, "< BACK TO GAME") {
                        self.video_detail = 0;
                    }
                    changed |= ui.toggle(x, row(1), half, bh, "VIEW BOBBING", &mut st.view_bobbing);
                    changed |= ui.toggle(x + half + 4.0 * s, row(1), half, bh, "WEATHER", &mut st.weather);
                    changed |= ui.toggle(x, row(2), bw, bh, "MINIMAP", &mut st.minimap);
                }
                _ if self.video_detail == 2 => {
                    let half = (bw - 4.0 * s) * 0.5;
                    if ui.button(x, row(0), bw, bh, "< BACK TO GAME") {
                        self.video_detail = 0;
                    }
                    if ui.button(x, row(1), bw, bh, "RELOAD MODS NOW") {
                        self.want_reload = true;
                    }
                    // Switches the mods add (`setting` lines in their packs)...
                    let toggles = &reg().toggles;
                    for (k, (id, default, label)) in toggles.iter().take(4).enumerate() {
                        let mut on = st.flags.iter().find(|f| &f.0 == id).map_or(*default, |f| f.1);
                        let wide = if toggles.len() == 1 { bw } else { half };
                        if ui.toggle(x + (k % 2) as f32 * (half + 4.0 * s), row(2 + k / 2), wide, bh, label, &mut on) {
                            st.flags.retain(|f| &f.0 != id);
                            st.flags.push((id.clone(), on));
                            changed = true;
                        }
                    }
                    // ...and the rows their own code adds.
                    let mut at = row(2 + (toggles.len().min(4) + 1) / 2);
                    for (_, n) in crate::native::active() {
                        if let Some(rows) = n.settings {
                            let (used, touched) = rows(ui, st, x, at, bw, bh, step);
                            at += used * step;
                            changed |= touched;
                        }
                    }
                }
                _ => {
                    ui.text(x, row(0) + 4.0 * s, "PLAYER NAME", ui::DIM);
                    ui.text_field(10, x + 80.0 * s, row(0), bw - 80.0 * s, bh, &mut st.player_name, 16, ui::name_char);
                    changed |= ui.stepper(x, row(1), bw, bh, "AUTOSAVE EVERY (SECONDS)", &mut st.autosave_seconds, 10, 600, 10);
                    if let Some(g) = self.session.as_mut() {
                        let label = alloc::format!("GAME MODE  {}", if g.mode == GameMode::Survival { "SURVIVAL" } else { "CREATIVE" });
                        if ui.button_lr(x, row(2), bw, bh, &label, true) != 0 {
                            g.set_mode(if g.mode == GameMode::Survival { GameMode::Creative } else { GameMode::Survival });
                        }
                    }
                    if ui.button(x, row(3), bw, bh, "VIEW, WEATHER AND MAP...") {
                        self.video_detail = 1;
                    }
                    if ui.button(x, row(4), bw, bh, "MOD SWITCHES AND RELOAD...") {
                        self.video_detail = 2;
                    }
                }
            }
        }
        if skin_pick {
            self.open_skin_picker();
        }
        if skin_web {
            self.open_skin_url();
        }
        if skin_changed {
            self.load_skin(ui.r);
        }
        if self.tab == 4 {
            let status = self.status.clone();
            ui.centered(ui.w() * 0.5, row(9), 1.0, &status, ui::ACCENT);
        }
        if fullscreen_clicked {
            self.toggle_fullscreen();
        } else if mode_changed && self.settings.fullscreen {
            self.apply_fullscreen();
        }
        if changed {
            self.apply_settings();
        }
        if ui.button(x, row(10) + 4.0 * s, bw, bh, "DONE") {
            self.leave_settings();
        }
    }

    pub(super) fn leave_settings(&mut self) {
        self.settings.player_name = crate::settings::clean_name(&self.settings.player_name, 16);
        if self.settings.player_name.is_empty() {
            self.settings.player_name = String::from("Player");
        }
        if !self.scripted {
            sys::write_file_atomic(&self.settings_path, self.settings.to_text().as_bytes());
        }
        self.set_screen(if self.session.is_some() { Screen::Pause } else { Screen::Title });
    }

    /// Read the pack files again and switch to them without restarting.
    pub(super) fn reload_packs(&mut self, r: &mut Renderer) {
        if self.session.as_ref().map_or(false, |g| g.net.is_some()) {
            if let Some(g) = self.session.as_mut() {
                g.say("ON A SERVER, THE SERVER DECIDES THE CONTENT");
            }
            return;
        }
        let (new, text) = load_content(&self.pack_paths, &self.settings.mods_off);
        for w in &new.warnings {
            logf!("voxelrust: data: {}", w);
        }
        let note = if new.warnings.is_empty() {
            alloc::format!("PACKS RELOADED: {} BLOCKS, {} ITEMS", new.blocks.len(), new.items.len())
        } else {
            alloc::format!("PACKS RELOADED, {} PROBLEMS (SEE THE LOG)", new.warnings.len())
        };
        if new.items.is_empty() {
            return;
        }
        match self.session.as_mut() {
            Some(g) => {
                g.reload_content(new, r);
                g.push_chat(note.clone());
            }
            None => block::replace(new),
        }
        r.rebuild_atlas();
        r.load_models();
        self.pack_text = text;
        self.server_content = false;
        self.status = note;
    }
}
