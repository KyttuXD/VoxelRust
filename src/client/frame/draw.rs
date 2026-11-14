//! Drawing a frame (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    pub(super) fn draw(&mut self, r: &mut Renderer, inp: &UiInput) {
        r.render_scale = self.settings.render_scale;
        r.culler.occlusion = !self.no_occlusion;
        crate::client::shaders::hook::sync(r, &self.settings.shader, self.session.as_ref().map(|g| &g.shader_uniforms));
        let mut underwater = false;
        let mut in_lava = false;
        let mut new_tags = Vec::new();
        if let Some(g) = &self.session {
            let mut eye = g.player.eye();
            // Asleep: the view is from the eyes of the body lying in the bed, looking up.
            let lying = g.posing.lying.filter(|_| g.sleeping);
            if let Some(l) = lying {
                eye = crate::content::pose::sleep::eye_at(l.middle, l.yaw);
            }
            if self.settings.view_bobbing && g.player.on_ground && !g.player.flying && lying.is_none() {
                let speed = g.player.vel[0] * g.player.vel[0] + g.player.vel[2] * g.player.vel[2];
                if speed > 1.0 {
                    eye[1] += (sinf(g.posing.gait * 1.1) * 0.035) as f64;
                }
            }
            if g.shake > 0.0 {
                // A `/shake` rattles the view.
                eye[0] += (sinf(g.clock * 47.0) * g.shake * 0.25) as f64;
                eye[1] += (sinf(g.clock * 61.0) * g.shake * 0.18) as f64;
            }
            if self.third_person {
                // Pull the camera back (or, from the front, out ahead), but not through walls.
                let back = crate::math::forward(g.player.yaw, g.player.pitch) * if self.third_front { 1.0 } else { -1.0 };
                let room = g.world.raycast(eye, back, 4.5).map_or(4.0, |hit| (hit.dist - 0.4).clamp(0.3, 4.0));
                eye = [eye[0] + (back.x * room) as f64, eye[1] + (back.y * room) as f64, eye[2] + (back.z * room) as f64];
            }
            // From the front the camera turns round to face the player.
            let (cam_yaw, cam_pitch) = if self.third_person && self.third_front {
                (g.player.yaw + PI, -g.player.pitch)
            } else if let (Some(l), false) = (lying, self.third_person) {
                (l.yaw, crate::content::pose::sleep::LOOK_UP)
            } else {
                (g.player.yaw, g.player.pitch)
            };
            let cam = Camera { pos: eye, yaw: cam_yaw, pitch: cam_pitch, fov: self.settings.fov as f32 };
            let head = g.world.get(floord(eye[0]), floord(eye[1]), floord(eye[2]));
            in_lava = matches!(crate::block::fluid_of(head), Some((true, _)));
            underwater = crate::block::is_water(head) || in_lava;
            // The sky of this dimension (a pack's `dimension` line, or a script's `set_dimension_sky`).
            let look = crate::game::fx::sky::current(g);
            let env = Env {
                time: look.time_of(g.time),
                sky: look,
                distance: self.eff.distance,
                clouds: self.settings.clouds && look.clouds,
                underwater,
                lava: in_lava,
                clock: g.clock,
                far_distance: if self.eff.far > self.eff.distance { self.eff.far } else { 0 },
                rain: g.sky.rain,
                flash: g.flash,
                vision: crate::game::effects::apply::night_vision(&g.fx, &crate::game::effects::apply::lookup),
                blind: crate::game::effects::body::blindness(&g.fx, &crate::game::effects::apply::lookup),
                fog_override: crate::game::fx::state_ref(g).and_then(|s| s.fog.get()),
            };
            r.draw_world(&g.world, &cam, &env);
            let mut tags = Vec::new();
            self.draw_creatures(r, g, &cam, &mut tags);
            new_tags = tags;
            if let (Some(hit), true) = (&g.target, (!self.scripted || self.show_hud) && self.screen == Screen::Game) {
                let c = hit.cell;
                let b = g.world.get(c[0], c[1], c[2]);
                let mask = block::connect_mask(b, &|dx, dz| g.world.get(c[0] + dx, c[1], c[2] + dz));
                let mut shape = Vec::new();
                block::shape(b, mask, &mut shape);
                r.outline(&cam, c, &shape);
                if let Some((cell, p)) = g.mining.filter(|m| m.0 == c) {
                    r.crack(&cam, cell, &shape, p);
                }
            }
            if !underwater {
                r.draw_precipitation(&cam, g.sky.rain, g.sky.snow, g.clock);
            }
            if let Some(crate::session::Episode { x, z, .. }) = g.episode.filter(|d| reg().episodes.get(d.kind).map_or(false, |k| k.funnel)) {
                let ground = crate::worldgen::column(g.world.seed, x as i32, z as i32).height as f64;
                r.draw_funnel([(x as f64 - cam.pos[0]) as f32, (ground - cam.pos[1]) as f32, (z as f64 - cam.pos[2]) as f32], g.clock);
            }
            r.finish_world(&cam, &env);
            // Particles and translucent quads of mods, over the finished world.
            r.draw_effects(g, &cam, &env);
            r.draw_soft_particles(g, &cam, &env);
            // The hand and what it holds, in their own close-up pass.
            // (While emoting only the arm shows, and only for emotes that move it; the item is put away.)
            let emoting = g.posing.emote.is_some();
            let emote_arm = !emoting || crate::content::pose::emote::shows_arm(&g.posing.body(g.activity()));
            if !self.third_person && self.screen == Screen::Game && !g.dead && !g.carried && lying.is_none() && emote_arm {
                let body = body_for(g, own_id(g), self.slim());
                if let Some(hand) = reg().model(arm_models(&body, self.slim()).1) {
                    let v = g.player.vel;
                    let speed = crate::math::sqrtf(v[0] * v[0] + v[2] * v[2]);
                    let held = if emoting { crate::client::hand::Held::Nothing } else { crate::client::hand::held_of(g.held().map(|h| h.0)) };
                    let mut view = hand_view_of(g, held, speed);
                    if let Some(f) = self.show_eat {
                        view.eat = f;
                        view.swing = 0.0;
                    }
                    if let Some((_, t)) = g.posing.emote {
                        // A wave: the hand swings to and fro.
                        view.swing = (0.4 + 0.35 * sinf(t * 9.0)).max(0.001);
                    }
                    let mut items = Vec::new();
                    crate::client::hand::first_person_view(held, &view, &mut items);
                    let to_view = |p: [f32; 3]| crate::client::hand::arm_point_view(p, &view, held);
                    // Lit by the sky, or by lava and lamps close by.
                    let light = Self::hand_light(g, eye);
                    crate::client::block_light::scale_quads(&mut items, light);
                    r.draw_first_person(hand, self.skin_tex, cam.fov, 0.24, &to_view, &items, light);
                }
            }
        } else {
            r.clear([0.36, 0.56, 0.86]);
            crate::game::bow::show(None);
        }

        self.tags = new_tags;
        r.begin_2d();
        let (w, h) = (r.w as f32, r.h as f32);
        if self.session.is_none() {
            // Menus stand on a wall of the game's own blocks: stone below,
            // soil and turf above, dimmed so the buttons stand out, and
            // drifting slowly sideways.
            let tile = (h / 11.0).max(32.0);
            let wk = &reg().wk;
            let slid = self.ui_clock * 6.0;
            let drift = slid - ((slid / tile) as i32) as f32 * tile;
            let rows = (h / tile) as i32 + 1;
            let cols = (w / tile) as i32 + 2;
            for row in 0..rows {
                let (b, face) = match row {
                    0..=2 => (block::AIR, 0),
                    3 => (wk.grass, 1),
                    4 | 5 => (wk.dirt, 1),
                    _ => (wk.stone, 1),
                };
                if b == block::AIR {
                    continue;
                }
                let shade = (200 - row * 8).max(120) as u8;
                for col in 0..cols {
                    // A few ores in the rock, the same ones every time.
                    let roll = crate::noise::hash2(77, col + (self.ui_clock * 6.0 / tile) as i32, row);
                    let block = if row >= 6 && roll % 13 == 0 { [wk.coal_ore, wk.iron_ore, wk.copper_ore, wk.crystal_ore][(roll >> 8) as usize % 4] } else { b };
                    r.icon(col as f32 * tile - drift, row as f32 * tile, tile + 0.5, block::def(block).tex[face], [shade, shade, shade, 255]);
                }
            }
            // Evening light from above, and the corners drawn in.
            r.rect(0.0, 0.0, w, tile * 3.0, [255, 190, 120, 26]);
            r.rect(0.0, h * 0.55, w, h * 0.45, [0, 0, 10, 50]);
            r.rect(0.0, 0.0, w, h, [8, 10, 24, 60]);
        }
        let s = self.ui_scale(r);
        // Thick, bright lava over the view, filling in quickly and draining slowly.
        let lava_dt = (self.ui_clock - self.lava_stepped).clamp(0.0, 0.25);
        self.lava_stepped = self.ui_clock;
        self.lava_fade = crate::lava_look::overlay_step(self.lava_fade, in_lava, lava_dt);
        if self.lava_fade > 0.0 {
            r.rect(0.0, 0.0, w, h, crate::lava_look::overlay_colour(self.lava_fade));
        } else if underwater {
            r.rect(0.0, 0.0, w, h, [20, 70, 150, 90]);
        }
        if self.session.is_some() && (!self.scripted || self.show_hud) && !(self.hud_hidden && self.screen == Screen::Game) {
            self.draw_hud(r, w, h, s);
        }
        // A loading panel, but only when the wait is long enough to notice.
        if let Some(g) = self.session.as_ref() {
            if !g.playable && self.screen == Screen::Game && self.loading_since > 0.0 && sys::now() - self.loading_since > 0.35 {
                draw_loading(r, w, h, s, g, self.distance);
            }
            // The server's mods are still arriving.
            if let Some((done, total)) = g.mod_sync.as_ref().and_then(|m| m.progress()) {
                crate::client::mods::progress::draw(r, w, h, s, done, total, !g.playable);
            }
        }
        self.mod_screens();
        let before = self.screen;
        let mut focus = self.focus;
        let mut selected = self.text_selected;
        let clicked;
        {
            let mut ui = Ui { r: &mut *r, inp, s, focus: &mut focus, clicked_any: false, clock: self.ui_clock, selected: &mut selected };
            match self.screen {
                Screen::Game => {}
                Screen::Title => self.screen_title(&mut ui),
                Screen::Worlds => self.screen_worlds(&mut ui),
                Screen::EditWorld => self.screen_edit_world(&mut ui),
                Screen::NewWorld => self.screen_new_world(&mut ui),
                Screen::Settings => self.screen_settings(&mut ui),
                Screen::Pause => self.screen_pause(&mut ui),
                Screen::Inventory | Screen::Craft | Screen::Chest => self.screen_inventory(&mut ui),
                Screen::Dead => self.screen_dead(&mut ui),
                Screen::Map => self.screen_map(&mut ui),
                Screen::FullMap => self.screen_full_map(&mut ui),
                Screen::Join => self.screen_join(&mut ui),
                Screen::AddServer => self.screen_add_server(&mut ui),
                Screen::Pin => self.screen_pin(&mut ui),
                Screen::Chat => self.screen_chat(&mut ui),
                Screen::Picker => self.screen_picker(&mut ui),
                Screen::Emote => self.screen_emote(&mut ui),
                Screen::SkinUrl => self.screen_skin_url(&mut ui),
                Screen::ModUi => self.screen_mod_ui(&mut ui),
            }
            clicked = ui.clicked_any;
        }
        // Changing screen resets the focus; otherwise keep what the UI set.
        if self.screen == before && self.rebinding.is_none() {
            // A selection belongs to one box: moving to another drops it.
            self.text_selected = selected && focus == self.focus;
            self.focus = focus;
        }
        if clicked {
            self.audio.play(Sound::Click, 1.0);
        }
        r.end_2d();
    }
}
