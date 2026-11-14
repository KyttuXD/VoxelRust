//! Drawing the HUD (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    pub(super) fn draw_hud(&mut self, r: &mut Renderer, w: f32, h: f32, s: f32) {
        crate::client::modui::dispatch::set_fps(self.fps);
        let g = match &self.session {
            Some(g) => g,
            None => return,
        };
        if g.hurt_flash > 0.0 {
            r.rect(0.0, 0.0, w, h, [200, 20, 20, (g.hurt_flash * 300.0).min(110.0) as u8]);
        }
        // An effect that colours the screen (blindness, mods' own).
        if let Some(t) = crate::game::effects::body::tint(&g.fx, &crate::game::effects::apply::lookup) {
            r.rect(0.0, 0.0, w, h, t);
        }
        // A colour a mod's script laid over the screen (`hud_tint`).
        crate::script::ext::atmosphere::draw_tint(g, r, w, h);
        if self.screen == Screen::Game {
            let c: Color = [255, 255, 255, 200];
            r.rect(w * 0.5 - 5.0 * s, h * 0.5 - 0.5 * s, 10.0 * s, s, c);
            r.rect(w * 0.5 - 0.5 * s, h * 0.5 - 5.0 * s, s, 10.0 * s, c);
            if let Some((_, p)) = g.mining {
                let bw = 40.0 * s;
                r.rect(w * 0.5 - bw * 0.5 - s, h * 0.5 + 11.0 * s, bw + 2.0 * s, 5.0 * s, [0, 0, 0, 150]);
                r.rect(w * 0.5 - bw * 0.5, h * 0.5 + 12.0 * s, bw * p.clamp(0.0, 1.0), 3.0 * s, [250, 250, 250, 230]);
            }
            crate::game::hooks::draw_hud(g, r, w, h, s);
        }
        // A bow being drawn shows its drawn picture in the hotbar and the inventory too.
        {
            let held = g.held().map(|h| h.0);
            let draw = held.map_or(0.0, |i| g.bow.shown(i as u16));
            crate::game::bow::show(crate::game::bow::pull_frame(draw).and_then(|f| held.map(|i| (i, f))));
        }
        // Hotbar. Empty slots stay empty: an empty hand is a valid choice.
        let slot = 22.0 * s;
        let x0 = (w - HOTBAR as f32 * slot) * 0.5;
        let y0 = h - slot - 4.0 * s;
        r.rect(x0 - 2.0 * s, y0 - 2.0 * s, HOTBAR as f32 * slot + 4.0 * s, slot + 4.0 * s, [16, 18, 24, 170]);
        for i in 0..HOTBAR {
            let x = x0 + i as f32 * slot;
            draw_slot(r, x, y0, slot, s, g.inv.slots[i], false);
            if i == g.selected {
                r.frame(x - s, y0 - s, slot + 2.0 * s, slot + 2.0 * s, 2.0 * s, [250, 250, 250, 255]);
            }
        }
        let survival = g.mode == GameMode::Survival;
        if let Some((_, item)) = g.held() {
            let tw = Renderer::text_width(&item.name, s);
            r.label((w - tw) * 0.5, y0 - (if survival { 24.0 } else { 13.0 }) * s, s, &item.name, WHITE);
        }
        if survival {
            let hy = y0 - 12.0 * s;
            // Hearts, drawn from the heart pictures (green, dark, blue by effect).
            if !crate::client::effects_hud::hearts(r, g, x0, hy, s) {
                for c in crate::game::gear::hearts::layout(g.health, g.max_health()) {
                    heart(r, x0 + c.col as f32 * 9.0 * s, hy - c.row as f32 * 8.0 * s, s, c.fill as i32);
                }
            }
            // Hunger, as ten drumstick-coloured pips on the right.
            for i in 0..crate::session::MAX_HUNGER / 2 {
                let x = x0 + HOTBAR as f32 * slot - (i + 1) as f32 * 9.0 * s;
                let fill = (g.hunger - i * 2).clamp(0, 2);
                r.rect(x + s, hy + s, 6.0 * s, 5.0 * s, [40, 26, 14, 210]);
                if fill > 0 {
                    r.rect(x + s, hy + s, if fill == 2 { 6.0 } else { 3.0 } * s, 5.0 * s, [214, 142, 62, 255]);
                }
            }
            // (More than ten hearts make more rows; the bubbles go above the top one.)
            let extra_rows = crate::game::gear::hearts::rows(g.max_health()).saturating_sub(1) as f32;
            crate::client::effects_hud::bubbles(r, g, x0 + HOTBAR as f32 * slot, hy - 9.0 * s - extra_rows * 9.0 * s, s);
        }
        // What mods put on the HUD (their drawing commands, lines in the slots, toasts).
        crate::client::modui::hud_draw::draw(r, w, h, s, &crate::client::modui::hud::Geometry { hotbar_x: x0, hotbar_y: y0, survival });
        // Active effects, with their icons and the time left, top right (under the minimap).
        if matches!(self.screen, Screen::Game | Screen::Chat) && !g.fx.list.is_empty() {
            let under_map = if self.settings.minimap && self.minimap.tex != 0 { 70.0 * s } else { 0.0 };
            crate::client::effects_hud::list(r, g, w, 4.0 * s + under_map, s);
        }
        for (x, y, name, shrink) in &self.tags {
            let ts = s * shrink;
            let tw = Renderer::text_width(name, ts);
            r.rect(x - tw * 0.5 - 2.0 * ts, y - 2.0 * ts, tw + 4.0 * ts, 11.0 * ts, [0, 0, 0, 110]);
            r.label(x - tw * 0.5, *y, ts, name, WHITE);
        }
        // Small hearts under the names of players near, and the list of players while Tab is held.
        for (x, y, hp, max, shrink) in self.tag_hearts.borrow().iter() {
            crate::client::playerlist::small_hearts(r, *x, *y + 12.0 * s * shrink, *hp, *max, s * shrink * 0.5);
        }
        if self.screen == Screen::Game && self.keys[43] && g.net.is_some() {
            crate::client::playerlist::tab_list(r, g, &self.settings.player_name, w, s);
        }
        // Voice chat: your microphone, and who is talking.
        {
            let names: Vec<String> = match g.net.as_ref() {
                Some(n) => self.voice.talking().filter_map(|id| n.players.get(&id)).map(|p| p.name.to_ascii_uppercase()).collect(),
                None => Vec::new(),
            };
            let shown: Vec<&str> = names.iter().map(|n| n.as_str()).collect();
            crate::client::voice::hud::draw(r, s, h, self.voice.mic(), &shown, &self.tags);
        }
        // Chat and notices, newest at the bottom, fading out.
        let lines: Vec<&(String, f32)> = g.chat.iter().filter(|c| c.1 > 0.0 || self.screen == Screen::Chat).collect();
        for (i, (line, left)) in lines.iter().rev().enumerate() {
            let a = if self.screen == Screen::Chat { 255 } else { (left * 255.0).clamp(0.0, 255.0) as u8 };
            let y = h - 52.0 * s - i as f32 * 10.0 * s;
            // "&c" and the like change colour and are not shown themselves.
            let plain: String = strip_colours(line);
            r.rect(2.0 * s, y - s, Renderer::text_width(&plain, s) + 4.0 * s, 10.0 * s, [0, 0, 0, a / 3]);
            label_coloured(r, 4.0 * s, y, s, line, a);
        }
        // Lines scripts ask for (host call `hud`), under the frame rate.
        if !self.show_debug {
            for (i, line) in g.script_hud.texts().enumerate() {
                let y = 14.0 * s + i as f32 * 10.0 * s;
                r.rect(2.0 * s, y - s, Renderer::text_width(line, s) + 4.0 * s, 10.0 * s, [0, 0, 0, 90]);
                r.label(4.0 * s, y, s, line, WHITE);
            }
        }
        if g.note.1 > 0.0 {
            let tw = Renderer::text_width(&g.note.0, s * 2.0);
            r.label((w - tw) * 0.5, h * 0.3, s * 2.0, &g.note.0, [255, 240, 170, 255]);
        }
        // Subtitles: what was just heard, newest lowest, fading out.
        if self.settings.subtitles {
            for (i, (text, left)) in self.audio.captions.iter().rev().enumerate() {
                let t = text.to_ascii_uppercase();
                let tw = Renderer::text_width(&t, s);
                let a = (left * 200.0).clamp(0.0, 255.0) as u8;
                let y = h - 60.0 * s - i as f32 * 11.0 * s;
                r.rect(w - tw - 10.0 * s, y - 2.0 * s, tw + 6.0 * s, 10.0 * s, [0, 0, 0, a / 2]);
                r.label(w - tw - 7.0 * s, y, s, &t, [255, 255, 255, a]);
            }
        }
        // The minimap: north up, the player a dot with a nose.
        if self.settings.minimap && self.minimap.tex != 0 && matches!(self.screen, Screen::Game | Screen::Chat) {
            let size = 64.0 * s;
            let (mx, my) = (w - size - 4.0 * s, 4.0 * s);
            r.rect(mx - s, my - s, size + 2.0 * s, size + 2.0 * s, [16, 18, 24, 200]);
            r.image(self.minimap.tex, mx, my, size, size);
            // The minimap shows 2 blocks per texel, centred where its picture began.
            let per = size / (crate::client::fullmap::MINIMAP_TEXELS as f32 * self.minimap.shown_step as f32);
            let to_map = |bx: f32, bz: f32| (mx + size * 0.5 + (bx - self.minimap.shown_origin.0 as f32) * per, my + size * 0.5 + (bz - self.minimap.shown_origin.1 as f32) * per);
            if self.settings.map_waypoints {
                for wp in &g.waypoints {
                    let (wx, wz) = to_map(wp.x as f32 + 0.5, wp.z as f32 + 0.5);
                    if wx > mx && wx < mx + size && wz > my && wz < my + size {
                        let c = crate::game::waypoints::COLOURS[wp.colour.min(7) as usize];
                        r.rect(wx - 1.5 * s, wz - 1.5 * s, 3.0 * s, 3.0 * s, [c[0], c[1], c[2], 255]);
                    }
                }
            }
            // Other players first, so the player's own arrow is on top.
            if let Some(net) = g.net.as_ref() {
                for p in net.players.values().filter(|p| p.seen) {
                    let (ox, oz) = to_map(p.pos[0] as f32, p.pos[2] as f32);
                    if ox > mx && ox < mx + size && oz > my && oz < my + size {
                        minimap_arrow(r, ox, oz, p.yaw, s, [110, 220, 255, 255]);
                    }
                }
            }
            let (cx, cy) = to_map(g.player.pos[0] as f32, g.player.pos[2] as f32);
            if cx > mx && cx < mx + size && cy > my && cy < my + size {
                minimap_arrow(r, cx, cy, g.player.yaw, s, [255, 255, 255, 255]);
            }
            let place = alloc::format!("{} {} {}", floord(g.player.pos[0]), floord(g.player.pos[1]), floord(g.player.pos[2]));
            let tw = Renderer::text_width(&place, s);
            r.label(mx + size - tw, my + size + 2.0 * s, s, &place, [230, 232, 240, 220]);
        }
        if self.settings.show_fps && !self.show_debug {
            r.label(4.0 * s, 4.0 * s, s, &alloc::format!("{:.0} FPS", self.fps), WHITE);
        }
        if self.show_debug {
            let p = g.player.pos;
            let st = r.stats;
            let lines = [
                alloc::format!("VOXELRUST {} ({})   {:.0} FPS", env!("CARGO_PKG_VERSION"), BUILD.to_ascii_uppercase(), self.fps),
                alloc::format!("POS {:.1} {:.1} {:.1}   CHUNK {} {}   FACING {}", p[0], p[1], p[2], floord(p[0]) >> 4, floord(p[2]) >> 4, ["NORTH (-Z)", "WEST (-X)", "SOUTH (+Z)", "EAST (+X)"][crate::interact::facing(g.player.yaw) as usize]),
                alloc::format!(
                    "BIOME {:?}   SEED {}   {}   {} CREATURES",
                    crate::worldgen::column(g.world.seed, floord(p[0]), floord(p[2])).biome,
                    g.world.seed,
                    if g.mode == GameMode::Survival { "SURVIVAL" } else { "CREATIVE" },
                    g.mobs.list.len()
                )
                .to_ascii_uppercase(),
                match &g.target {
                    Some(hit) => alloc::format!("LOOKING AT {} ({} {} {})", block::def(g.world.get(hit.cell[0], hit.cell[1], hit.cell[2])).name, hit.cell[0], hit.cell[1], hit.cell[2]).to_ascii_uppercase(),
                    None => String::from("LOOKING AT NOTHING"),
                },
                alloc::format!(
                    "CHUNKS {} LOADED, {} DRAWN, {} QUADS, {} KB   VIEW {}",
                    g.world.chunks.len(),
                    st.chunks_drawn,
                    st.quads,
                    r.vbo_bytes / 1024,
                    self.distance
                ),
                {
                    let c = r.culler.stats;
                    alloc::format!(
                        "SECTIONS {} DRAWN, {} FRUSTUM CULLED, {} OCCLUDED OF {}   OCCLUSION {} (F3+O)",
                        c.drawn,
                        c.frustum,
                        c.occluded,
                        c.total,
                        if r.culler.occlusion { "ON" } else { "OFF" }
                    )
                },
                alloc::format!("DRAW CALLS {}   CULLING {:.2} MS", st.draw_calls, r.culler.stats.micros as f32 / 1000.0),
                alloc::format!("JOBS {} ON {} THREADS   TIME {:.2}", g.pool.in_flight, g.pool.thread_count(), g.time),
                gl::get_string(gl::RENDERER),
            ];
            let mem = self.mem.lines(r);
            let lines: Vec<String> = lines.into_iter().chain(mem).collect();
            for (i, l) in lines.iter().enumerate() {
                r.label(4.0 * s, 4.0 * s + i as f32 * 10.0 * s, s, l, WHITE);
            }
        }
    }
}
