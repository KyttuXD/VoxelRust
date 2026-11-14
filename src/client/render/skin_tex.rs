//! The graphics card side of skins (see `client/skins`): a 64 x 64 skin is
//! kept pixel-sharp (nearest filtering), a bigger one is smoothed: linear
//! filtering with mipmaps, which the card makes itself (`GENERATE_MIPMAP`,
//! OpenGL 1.4) every time the texture is written, so that a far-away HD skin
//! does not shimmer.

use super::Renderer;
use crate::sys::gl::{self, gl};
use core::ffi::c_void;

impl Renderer {
    /// Upload a `side` x `side` skin and return its handle.
    pub fn add_skin_texture(&mut self, side: usize, rgba: &[u8]) -> u32 {
        if side <= 64 {
            return self.add_texture(side, side, rgba);
        }
        if rgba.len() < side * side * 4 {
            return 0;
        }
        self.flush(gl::QUADS);
        let g = gl();
        let mut id = 0u32;
        unsafe {
            (g.GenTextures)(1, &mut id);
            (g.BindTexture)(gl::TEXTURE_2D, id);
            (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR_MIPMAP_LINEAR);
            (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR);
            (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
            (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
            (g.TexParameteri)(gl::TEXTURE_2D, gl::GENERATE_MIPMAP, 1);
            (g.TexImage2D)(gl::TEXTURE_2D, 0, gl::RGBA as i32, side as i32, side as i32, 0, gl::RGBA, gl::UNSIGNED_BYTE, rgba.as_ptr() as *const c_void);
        }
        self.batch_tex = 0;
        id
    }

    /// Write new pixels over a skin texture of the same side made by [`Renderer::add_skin_texture`].
    pub fn update_skin_texture(&mut self, tex: u32, side: usize, rgba: &[u8]) {
        if rgba.len() >= side * side * 4 {
            self.update_texture(tex, side, side, rgba);
        }
    }
}
