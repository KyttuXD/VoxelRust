//! The minimap and the full map screens (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    /// Paint a little more of the minimap (see client::fullmap).
    pub(super) fn paint_minimap(&mut self, r: &mut Renderer) {
        let g = match self.session.as_ref() {
            Some(g) if self.settings.minimap => g,
            _ => return,
        };
        let others: Vec<(String, [f32; 2])> = g.net.as_ref().map(|n| n.players.values().filter(|p| p.seen).map(|p| (p.name.clone(), [p.pos[0] as f32, p.pos[2] as f32])).collect()).unwrap_or_default();
        let frame = crate::client::fullmap::Frame {
            others: &others,
            seed: g.world.seed,
            world: crate::client::fullmap::world_key(g.world.seed, &g.name),
            explored: &g.explored,
            charted: crate::worldgen::source().map_or(false, |s| s.charted) && crate::worldgen::shape() == 0,
            player: [g.player.pos[0] as f32, g.player.pos[2] as f32],
            now: sys::now(),
        };
        if crate::render::EXPERIMENT.load(core::sync::atomic::Ordering::Relaxed) & 16 != 0 {
            return;
        }
        // A little each frame keeps this off the frame time.
        crate::client::fullmap::minimap_work(&mut self.minimap, r, &frame, 40);
    }

    /// Paint the full map while it is open.
    pub(super) fn work_full_map(&mut self, r: &mut Renderer) {
        let (Some(g), Some(full)) = (self.session.as_ref(), self.full.as_mut()) else { return };
        let others: Vec<(String, [f32; 2])> = g.net.as_ref().map(|n| n.players.values().filter(|p| p.seen).map(|p| (p.name.clone(), [p.pos[0] as f32, p.pos[2] as f32])).collect()).unwrap_or_default();
        let frame = crate::client::fullmap::Frame {
            others: &others,
            seed: g.world.seed,
            world: crate::client::fullmap::world_key(g.world.seed, &g.name),
            explored: &g.explored,
            charted: crate::worldgen::source().map_or(false, |s| s.charted) && crate::worldgen::shape() == 0,
            player: [g.player.pos[0] as f32, g.player.pos[2] as f32],
            now: sys::now(),
        };
        full.work(r, &frame);
    }

    /// The map key: a mod's own map for this world if it has one, else
    /// the minimap grows.
    pub(super) fn open_map(&mut self) {
        self.map_native = None;
        if let Some(g) = self.session.as_mut() {
            for (k, n) in crate::native::active() {
                if n.map_open.map_or(false, |open| open(g)) {
                    self.map_native = Some(k);
                    break;
                }
            }
        }
        if self.map_native.is_some() {
            self.set_screen(Screen::Map);
        } else {
            self.open_full_map();
        }
    }

    /// The block map: the world's terrain at any zoom, with waypoints.
    pub(super) fn open_full_map(&mut self) {
        if self.session.is_some() {
            self.set_screen(Screen::FullMap);
        }
    }

    /// Leave the block map, keeping its zoom and toggles for next time.
    pub(super) fn close_full_map(&mut self) {
        if let Some(full) = self.full.as_ref() {
            self.settings.map_zoom = crate::client::fullmap::scale_index(full.view.step) as i32;
            self.settings.map_follow = full.follow;
            self.settings.map_grid = full.grid;
            self.settings.map_waypoints = full.show_waypoints;
            if !self.scripted {
                sys::write_file_atomic(&self.settings_path, self.settings.to_text().as_bytes());
            }
        }
        self.set_screen(Screen::Game);
    }

    /// The block map's screen: its picture, panel and waypoints.
    pub(super) fn screen_full_map(&mut self, ui: &mut Ui) {
        if self.session.is_none() {
            self.set_screen(Screen::Game);
            return;
        }
        if self.full.is_none() {
            let st = &self.settings;
            self.full = Some(crate::client::fullmap::FullMap::new(st.map_zoom.max(0) as usize, st.map_follow, st.map_grid, st.map_waypoints));
        }
        let (Some(g), Some(full)) = (self.session.as_mut(), self.full.as_mut()) else { return };
        let others: Vec<(String, [f32; 2])> = g.net.as_ref().map(|n| n.players.values().filter(|p| p.seen).map(|p| (p.name.clone(), [p.pos[0] as f32, p.pos[2] as f32])).collect()).unwrap_or_default();
        let frame = crate::client::fullmap::Frame {
            others: &others,
            seed: g.world.seed,
            world: crate::client::fullmap::world_key(g.world.seed, &g.name),
            explored: &g.explored,
            charted: crate::worldgen::source().map_or(false, |s| s.charted) && crate::worldgen::shape() == 0,
            player: [g.player.pos[0] as f32, g.player.pos[2] as f32],
            now: sys::now(),
        };
        let asked = crate::client::fullmap::screen(full, ui, &frame, &mut g.waypoints);
        if let Some(i) = asked.preset {
            let step = crate::client::fullmap::MAP_SCALES[(self.settings.map_preset[i].clamp(0, 6)) as usize];
            full.view.step = step;
            full.follow = true;
        }
    }

    /// The screen a mod's code put up for the map key.
    pub(super) fn screen_map(&mut self, ui: &mut Ui) {
        let screen = self.map_native.and_then(|k| crate::native::all()[k].map_screen);
        let stay = match (screen, self.session.as_mut()) {
            (Some(screen), Some(g)) => screen(ui, g),
            _ => false,
        };
        if !stay {
            self.set_screen(Screen::Game);
        }
    }
}
