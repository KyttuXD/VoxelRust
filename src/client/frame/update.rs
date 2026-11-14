//! The per-frame update (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    /// Voice chat, once a frame: the microphone out, other players in.
    pub(super) fn update_voice(&mut self, dt: f32) {
        let key = (self.settings.key(Action::Voice) as usize).min(511);
        let ptt = self.focus == 0 && self.keys[key] && matches!(self.screen, Screen::Game | Screen::Inventory | Screen::Craft | Screen::Chest | Screen::Map | Screen::FullMap);
        let meter = self.screen == Screen::Settings && self.tab == 6;
        let wire = self.session.as_mut().and_then(|g| {
            let allowed = g.perms.map_or(true, |f| f & crate::proto::PERM_VOICE != 0);
            let (pos, yaw) = (g.player.pos, g.player.yaw);
            g.net.as_mut().map(|client| crate::client::voice::Wire { client, pos, yaw, allowed })
        });
        self.voice.update(dt, &self.settings, ptt, wire, &mut self.audio, meter);
    }

    pub(super) fn update(&mut self, r: &mut Renderer, dt: f32, real_dt: f32) {
        self.help_timer -= dt;
        self.ui_clock += real_dt;
        // In multiplayer the world never stops.
        let online = self.session.as_ref().map_or(false, |g| g.net.is_some());
        let playing = online || matches!(self.screen, Screen::Game | Screen::Inventory | Screen::Dead | Screen::Chat | Screen::Map | Screen::FullMap | Screen::Craft | Screen::Chest | Screen::Emote);
        let in_game = self.screen == Screen::Game;
        let k = |a: Action| in_game && self.keys[(self.settings.key(a) as usize).min(511)];
        let controls = Controls {
            input: Input {
                forward: k(Action::Forward) as i32 as f32 - k(Action::Back) as i32 as f32,
                strafe: k(Action::Right) as i32 as f32 - k(Action::Left) as i32 as f32,
                jump: k(Action::Jump),
                descend: k(Action::Descend),
                sprint: k(Action::Sprint),
            },
            break_held: in_game && self.buttons[1],
            place_held: in_game && self.buttons[3],
            pick: in_game && self.pick,
            toggle_fly: self.toggle_fly,
            leave: self.leave,
            drop: self.drop_item,
        };
        self.pick = false;
        self.toggle_fly = false;
        self.leave = false;
        self.drop_item = false;
        // What the memory budget allows (client/memory): the settings are left alone.
        let wanted = crate::budget::Wanted {
            distance: self.distance,
            far: self.settings.far_distance,
            lod_real: self.settings.lod_real,
            lod_trees: self.settings.lod_trees,
            lod_detail: self.settings.lod_detail,
        };
        // Skins on the card: all of them are counted, and what an HD skin costs beyond a
        // plain 64 x 64 raises the memory limit (the player chose it; see client/skins/budget).
        let plain = crate::client::skins::budget::gpu_bytes(64);
        let own = if self.skin_tex != 0 { crate::client::skins::budget::gpu_bytes(self.skin_tex_side) } else { 0 };
        r.skin_bytes = own + self.skins.gpu_bytes();
        crate::budget::set_allowance(crate::budget::Allow::Skins, own.saturating_sub(plain).saturating_add(self.skins.gpu_extra_bytes()));
        self.mem.tick(dt, r, self.session.as_ref(), &mut self.audio, &wanted);
        self.eff = self.mem.effective(&wanted);
        let distance = self.eff.distance;
        let mut died = false;
        if let Some(g) = self.session.as_mut() {
            // The player may move once the chunks right around them are in;
            // the rest of the world keeps streaming in behind them.
            if !g.playable {
                let (cx, cz) = crate::world::chunk_at(g.player.pos[0], g.player.pos[2]);
                let (done, total) = g.world.progress(cx, cz, 2, distance);
                if total > 0 && done == total {
                    g.playable = true;
                    if self.loading_since > 0.0 {
                        logf!("voxelrust: playable {:.2} s after the world started", sys::now() - self.loading_since);
                    }
                    self.loading_since = 0.0;
                } else if self.loading_since == 0.0 {
                    self.loading_since = sys::now();
                }
            }
            if playing && g.playable {
                g.set_lod(self.eff.lod_real, self.eff.lod_trees, self.eff.lod_detail);
                g.update(dt, &controls, distance, self.eff.far, r, &mut self.audio);
                if g.flags != self.settings.flags {
                    g.flags = self.settings.flags.clone();
                }
                g.weather_update(dt, self.settings.weather, &mut self.audio);
                died = g.dead;
            } else {
                // Paused: keep loading terrain, but nothing moves.
                let (cx, cz) = crate::world::chunk_at(g.player.pos[0], g.player.pos[2]);
                g.world.update(cx, cz, distance, &mut g.pool, r, &mut g.store);
            }
        }
        // A new name may mean another of the built-in skins.
        if self.settings.skin != "custom" && self.skin_for != self.settings.player_name {
            self.load_skin(r);
        }
        if let Some(g) = self.session.as_mut() {
            // Our skin goes out once it is packed (a big one is packed on a worker) and the server
            // will take one (not within five seconds of the last); until then this is asked again every frame.
            if let (Some(net), false) = (g.net.as_mut(), self.skin_sent) {
                if !self.skin_png.is_empty() && net.send_skins(&self.skin_png, self.skin_side, &self.skin_small, self.settings.slim_arms || self.skin_slim) {
                    self.skin_sent = true;
                }
            }
            if let Some(pack) = g.server_pack.take() {
                // Play with the server's blocks and items, not our own.
                if pack.as_slice() != self.pack_text.bytes(read_mod_file) {
                    let new = server_content(&pack);
                    logf!("voxelrust: using the server's content ({} bytes of packs, {} block states)", pack.len(), new.blocks.len());
                    for w in &new.warnings {
                        logf!("voxelrust: server data: {}", w);
                    }
                    g.reload_content(new, r);
                    r.rebuild_atlas();
                    r.load_models();
                    self.pack_text = crate::client::pack_text::PackText::ready(pack);
                    self.server_content = true;
                    self.mem.set_mods(true);
                    g.say("USING THE SERVER'S CONTENT");
                }
            }
            // Other players' skins: how far each is, so that near ones are sharp and far ones small
            // (see client/skins/budget.rs), and what arrived.
            let me = g.player.pos;
            let dists: BTreeMap<u16, f32> = g
                .net
                .as_ref()
                .map(|n| {
                    n.players
                        .iter()
                        .map(|(id, p)| {
                            let (dx, dy, dz) = (p.pos[0] - me[0], p.pos[1] - me[1], p.pos[2] - me[2]);
                            (*id, crate::math::sqrtf((dx * dx + dy * dy + dz * dz) as f32))
                        })
                        .collect()
                })
                .unwrap_or_default();
            for (id, png, slim) in g.skin_events.drain(..) {
                // (A server could name a skin for every player number there is: the hub keeps a few hundred.)
                self.remote_slim.insert(id, slim);
                self.skins.on_skin(id, png);
            }
            self.skins.plan(real_dt, &dists, r, &mut self.remote_skins);
        }
        self.skins.pump(r, &mut self.remote_skins);
        if let Some(own) = self.skins.take_own() {
            self.skin_png = own.hd;
            self.skin_side = own.side;
            self.skin_small = own.small;
            self.skin_sent = false;
        }
        if self.session.is_none() && !self.remote_skins.is_empty() {
            // The game was left: the other players' skins go back to the card.
            self.skins.forget_all(r, &mut self.remote_skins);
            self.remote_slim.clear();
        }
        self.audio.tick(real_dt);
        self.update_voice(real_dt);
        self.paint_minimap(r);
        if self.screen == Screen::FullMap {
            self.work_full_map(r);
        }
        if core::mem::take(&mut self.want_reload) {
            self.reload_packs(r);
        }
        if died && self.screen != Screen::Dead {
            self.set_screen(Screen::Dead);
        }
        if let Some(which) = self.session.as_mut().and_then(|g| g.opened_ext.take()) {
            self.chest_ext = Some(which);
            self.set_screen(Screen::Chest);
        }
        if let Some((what, at)) = self.session.as_mut().and_then(|g| g.opened.take()) {
            self.chest_ext = None;
            self.chest_at = at;
            self.inv_scroll = 0;
            self.at_oven = what == block::Usable::Oven;
            self.at_furnace = what == block::Usable::Furnace;
            self.craft.table_side = what.craft_side().unwrap_or(3);
            self.set_screen(if what == block::Usable::Chest { Screen::Chest } else { Screen::Craft });
        }
        // A script asked for the crafting grid or the pack (`open_crafting`, `open_inventory`).
        if let Some(ask) = self.session.as_mut().and_then(|g| crate::script::ext::screens::take_request(g)) {
            if self.screen == Screen::Game {
                use crate::script::ext::screens::ScreenRequest;
                match ask {
                    ScreenRequest::Craft(side) => {
                        self.chest_ext = None;
                        self.at_oven = false;
                        self.at_furnace = false;
                        self.craft.table_side = side;
                        self.set_screen(Screen::Craft);
                    }
                    ScreenRequest::Inventory => self.set_screen(Screen::Inventory),
                }
            }
        }
        // The chest being looked into has its lid up (game/chest_anim.rs).
        let looking = if self.screen == Screen::Chest && self.chest_ext.is_none() { Some(self.chest_at) } else { None };
        if let Some(g) = self.session.as_mut() {
            g.chest_view = looking;
        }
        if self.session.as_ref().map_or(false, |g| g.lost) {
            self.close_world(r);
            self.status = String::from("THE CONNECTION TO THE SERVER WAS LOST");
            self.set_screen(Screen::Join);
        }
        if !self.scripted {
            let every = self.settings.autosave_seconds as f32;
            if let Some(g) = self.session.as_mut() {
                if g.autosave_due(every) {
                    g.save();
                }
            }
            self.tune_distance(dt);
        }
    }

    /// Hold the target frame rate by trading view distance for speed.
    pub(super) fn tune_distance(&mut self, dt: f32) {
        let target = self.settings.target_fps as f32;
        if target <= 0.0 || self.screen != Screen::Game {
            return;
        }
        self.tune_timer += dt;
        if self.tune_timer < 1.5 {
            return;
        }
        self.tune_timer = 0.0;
        if self.fps < target * 0.93 && self.distance > 3 {
            self.distance -= 1;
        } else if self.fps > target * 1.3 && self.distance < self.settings.distance {
            self.distance += 1;
        }
    }
}
