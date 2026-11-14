//! Pictures of mods on the HUD (see `client/modui/hud_image.rs`): uploading
//! one, and drawing a part of it with a tint and opacity.

use super::*;

impl Renderer {
    /// Upload a mod's picture. Small ones (pixel art, strips of counters) keep
    /// hard pixels; big ones are smoothed when they are drawn at another size.
    pub fn add_picture(&mut self, w: usize, h: usize, rgba: &[u8]) -> u32 {
        self.flush(gl::QUADS);
        let filter = if w.max(h) <= 256 { gl::NEAREST } else { gl::LINEAR };
        let id = make_texture(w, h, rgba, filter, gl::CLAMP_TO_EDGE);
        self.batch_tex = 0;
        id
    }

    /// The part `uv` (u0, v0, u1, v1) of a texture, in the rectangle at (x, y) of size (w, h),
    /// multiplied by `col` (its opacity too).
    pub fn image_tinted(&mut self, tex: u32, x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], col: Color) {
        self.set_tex(tex, gl::QUADS);
        self.quad([[x, y, 0.0], [x + w, y, 0.0], [x + w, y + h, 0.0], [x, y + h, 0.0]], [[uv[0], uv[1]], [uv[2], uv[1]], [uv[2], uv[3]], [uv[0], uv[3]]], col);
    }
}
