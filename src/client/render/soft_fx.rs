//! Drawing the soft particles of mods (`game/fx/soft.rs`): round, blurred
//! discs facing the camera, each with its own colour, opacity and size. The
//! picture is one small white disc whose opacity falls off smoothly from the
//! middle (made here, nothing to load); the vertex colour tints it.
//!
//! They are drawn like the other effects, after the world and the water: tested
//! against the depth buffer, writing none, no fog. The ones laid over the view
//! go first, farthest first so that they stack right, then the glowing ones,
//! which can be in any order.

use super::*;
use crate::game::fx::soft;
use crate::session::Session;
use core::sync::atomic::{AtomicU32, Ordering};

/// The GL texture of the soft disc (0: not made yet).
static DISC_TEX: AtomicU32 = AtomicU32::new(0);

/// Side of the disc's picture, pixels.
const DISC: usize = 32;
/// Most drawn in a frame.
const MAX_DRAWN: usize = soft::MAX_LIVE;

/// The pixels of the disc: white, opaque in the middle and fading smoothly to
/// nothing at the edge (a smoothstep of the distance from the middle).
pub fn disc_pixels() -> Vec<u8> {
    let mut px = alloc::vec![255u8; DISC * DISC * 4];
    let c = (DISC as f32 - 1.0) * 0.5;
    for y in 0..DISC {
        for x in 0..DISC {
            let (dx, dy) = ((x as f32 - c) / (c + 0.5), (y as f32 - c) / (c + 0.5));
            let d = crate::math::sqrtf(dx * dx + dy * dy);
            let t = (1.0 - d).clamp(0.0, 1.0);
            let a = t * t * (3.0 - 2.0 * t);
            px[(y * DISC + x) * 4 + 3] = (a * 255.0) as u8;
        }
    }
    px
}

/// A speck to draw: relative to the camera, half its width, colour with opacity, added or not.
#[derive(Clone, Copy, Debug)]
pub struct SoftDraw {
    pub at: [f32; 3],
    pub half: f32,
    pub col: [u8; 4],
    pub additive: bool,
}

impl Renderer {
    fn disc_texture(&mut self) -> u32 {
        let t = DISC_TEX.load(Ordering::Relaxed);
        if t != 0 {
            return t;
        }
        let t = make_texture(DISC, DISC, &disc_pixels(), gl::LINEAR, gl::CLAMP_TO_EDGE);
        DISC_TEX.store(t, Ordering::Relaxed);
        t
    }

    /// Draw the soft particles the session holds, over the finished world.
    pub fn draw_soft_particles(&mut self, g: &Session, cam: &Camera, env: &Env) {
        let Some(st) = crate::game::fx::state_ref(g) else { return };
        if st.soft.list.is_empty() {
            return;
        }
        let ahead = math::forward(cam.yaw, cam.pitch);
        let mut list: Vec<(f32, SoftDraw)> = Vec::new();
        for p in st.soft.list.iter() {
            if list.len() >= MAX_DRAWN {
                break;
            }
            let (half, alpha) = p.look();
            if alpha == 0 || half <= 0.0 {
                continue;
            }
            let at = [(p.pos[0] - cam.pos[0]) as f32, (p.pos[1] - cam.pos[1]) as f32, (p.pos[2] - cam.pos[2]) as f32];
            let d2 = at[0] * at[0] + at[1] * at[1] + at[2] * at[2];
            if d2 > soft::DRAW_RANGE * soft::DRAW_RANGE || at[0] * ahead.x + at[1] * ahead.y + at[2] * ahead.z < -half {
                continue;
            }
            list.push((d2, SoftDraw { at, half, col: [p.rgb[0], p.rgb[1], p.rgb[2], alpha], additive: p.additive }));
        }
        if list.is_empty() {
            return;
        }
        // Farthest first.
        list.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(core::cmp::Ordering::Equal));
        let view = math::view_rotation(cam.yaw, cam.pitch);
        let right = [view[0], view[4], view[8]];
        let up = [view[1], view[5], view[9]];
        self.flush(gl::QUADS);
        let aspect = self.w as f32 / self.h.max(1) as f32;
        let far = env.distance as f32 * CW as f32 * 1.5 + 96.0;
        Self::load_matrices(&math::perspective(cam.fov, aspect, 0.08, far), &view);
        Self::atlas_matrix(false);
        let tex = self.disc_texture();
        let gl_ = gl();
        unsafe {
            (gl_.Enable)(gl::BLEND);
            (gl_.Enable)(gl::DEPTH_TEST);
            (gl_.DepthMask)(0);
            (gl_.Disable)(gl::ALPHA_TEST);
            (gl_.Disable)(gl::CULL_FACE);
            (gl_.Disable)(gl::FOG);
            (gl_.BindBuffer)(gl::ARRAY_BUFFER, 0);
        }
        for additive in [false, true] {
            unsafe { (gl_.BlendFunc)(gl::SRC_ALPHA, if additive { gl::ONE } else { gl::ONE_MINUS_SRC_ALPHA }) };
            self.batch_tex = tex;
            for p in list.iter().map(|l| &l.1).filter(|p| p.additive == additive) {
                let (r, u) = ([right[0] * p.half, right[1] * p.half, right[2] * p.half], [up[0] * p.half, up[1] * p.half, up[2] * p.half]);
                let c = |sx: f32, sy: f32| [p.at[0] + r[0] * sx + u[0] * sy, p.at[1] + r[1] * sx + u[1] * sy, p.at[2] + r[2] * sx + u[2] * sy];
                self.quad([c(-1.0, -1.0), c(1.0, -1.0), c(1.0, 1.0), c(-1.0, 1.0)], [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]], p.col);
            }
            self.flush(gl::QUADS);
        }
        unsafe {
            (gl_.BlendFunc)(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            (gl_.DepthMask)(1);
        }
        self.batch_tex = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_disc_is_solid_in_the_middle_and_clear_at_the_edge_and_corners() {
        let px = disc_pixels();
        assert_eq!(px.len(), DISC * DISC * 4);
        let a = |x: usize, y: usize| px[(y * DISC + x) * 4 + 3];
        assert!(a(DISC / 2, DISC / 2) > 240);
        assert_eq!(a(0, 0), 0);
        assert_eq!(a(DISC - 1, DISC - 1), 0);
        assert_eq!(a(0, DISC / 2), 0);
        // It falls off steadily along a row, and is the same on both sides.
        for x in DISC / 2..DISC - 1 {
            assert!(a(x, DISC / 2) >= a(x + 1, DISC / 2));
        }
        assert_eq!(a(11, 10), a(20, 10));
        // White all through: the vertex colour does the tinting.
        assert!(px.chunks(4).all(|p| p[0] == 255 && p[1] == 255 && p[2] == 255));
    }
}
