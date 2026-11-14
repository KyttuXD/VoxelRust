//! Helpers for scripted pictures: the creature line-up and the block showcase (methods of `App`, moved out of app.rs).

#[allow(unused_imports)]
use super::*;

impl App {
    /// Stand one of every creature in rows in front of the
    /// player (for screenshots, and for checking a pack's models).
    pub(super) fn line_up_creatures(&mut self) {
        let g = match self.session.as_mut() {
            Some(g) => g,
            None => return,
        };
        let p = g.player.pos;
        let fwd = crate::math::forward(g.player.yaw, 0.0);
        let right = [-fwd.z, fwd.x];
        // One of each model is enough: skip colour variants. With "looks:NAME", every look of
        // the creatures with NAME in their id, the plain one first.
        let mut seen = Vec::new();
        let mut kinds: Vec<(usize, u8)> = Vec::new();
        for i in 0..reg().entities.len() {
            if let Some(name) = &self.show_looks {
                if reg().entities[i].id.contains(name.as_str()) {
                    for v in 0..=reg().entities[i].x.variants.len() {
                        kinds.push((i, v as u8));
                    }
                }
                continue;
            }
            let m = reg().entities[i].model;
            if !seen.contains(&m) {
                seen.push(m);
                kinds.push((i, 0));
            }
        }
        logf!("voxelrust: line-up of {} creatures", kinds.len());
        let per_row = 8;
        let total = kinds.len();
        for n in 0..total {
            let (row, col) = (n / per_row, n % per_row);
            let side = (col as f32 - (per_row as f32 - 1.0) * 0.5) * 2.6;
            let ahead = 6.0 + row as f32 * 3.2;
            let (x, z) = (p[0] as f32 + fwd.x * ahead + right[0] * side, p[2] as f32 + fwd.z * ahead + right[1] * side);
            let world = &g.world;
            let solid = |x: i32, y: i32, z: i32| block::is_solid(world.get(x, y, z));
            let c = match crate::path::ground_near(&solid, floord(x as f64), floord(p[1]), floord(z as f64), 12) {
                Some(c) => c,
                None => continue,
            };
            let at = [c[0] as f64 + 0.5, c[1] as f64 + 0.01, c[2] as f64 + 0.5];
            if n < kinds.len() {
                let i = g.mobs.spawn(kinds[n].0, at);
                g.mobs.list[i].variant = kinds[n].1;
                // "walking" shows them mid-stride, each at its own point.
                if self.show_walking {
                    g.mobs.list[i].stride = 1.0;
                    g.mobs.list[i].phase = 0.9 + n as f32 * 0.7;
                    g.mobs.list[i].clock = n as f32 * 0.37;
                }
                // Turned a little, so shape and face both show.
                g.mobs.list[i].body.yaw = g.player.yaw + PI - 0.6;
                g.mobs.list[i].phase = 1.2;
                g.mobs.list[i].stride = 0.6;
            }
        }
    }

    /// Place one of every block on a grid in front of the player.
    pub(super) fn build_showcase(&mut self) {
        let r = reg();
        let g = match self.session.as_mut() {
            Some(g) => g,
            None => return,
        };
        let p = g.player.pos;
        let (x0, y0, z0) = (floord(p[0]) - 19, floord(p[1]) + 1, floord(p[2]) - 12);
        let mut n = 0;
        for it in &r.items {
            if let (Some(first), 0) = (it.place, it.brush) {
                let (col, row) = (n % 20, n / 20);
                g.world.set_block(x0 + col * 2, (y0 + row * 2).min(CH - 1), z0, r.placed_state(first, 0, false));
                n += 1;
            }
        }
    }
}
