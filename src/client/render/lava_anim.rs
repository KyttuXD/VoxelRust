//! The lava picture moving: its tile in the atlas is repainted when the slow
//! scroll or the brightness pulse reaches a new step (terrain/lava_look.rs
//! has the numbers). Like the water's, and only for the built-in picture.

use super::*;

impl Renderer {
    /// Step the lava texture (a few repaints a second at most).
    pub(super) fn animate_lava(&mut self, clock: f32) {
        let key = crate::lava_look::scroll_rows(clock) * 64 + crate::lava_look::pulse_step(clock);
        if key == self.lava_phase {
            return;
        }
        let r = reg();
        let tile = crate::block::def(r.wk.lava).tex[0] as usize;
        if !matches!(r.tiles.get(tile).map(|t| &t.src), Some(crate::block::TileSrc::Builtin(n)) if n == "lava") {
            self.lava_phase = key;
            return;
        }
        self.lava_phase = key;
        let g = gl();
        let (tx, ty) = ((tile % textures::PER_ROW) as i32, (tile / textures::PER_ROW) as i32);
        let mut level = crate::lava_look::frame(crate::lava_look::scroll_rows(clock), crate::lava_look::pulse_step(clock));
        let mut size = textures::TILE;
        self.flush(gl::QUADS);
        unsafe {
            (g.BindTexture)(gl::TEXTURE_2D, self.atlas);
            for i in 0..=4 {
                let s = size as i32;
                (g.TexSubImage2D)(gl::TEXTURE_2D, i, tx * s, ty * s, s, s, gl::RGBA, gl::UNSIGNED_BYTE, level.as_ptr() as *const c_void);
                if size > 1 {
                    level = textures::half_size(&level, size);
                    size /= 2;
                }
            }
        }
        self.batch_tex = 0;
    }
}
