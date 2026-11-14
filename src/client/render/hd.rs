//! The HD atlas on the graphics card (the pages of `content/hd`), and drawing
//! with it.
//!
//! Each page is a texture of its own with mip levels made on the CPU (a box
//! filter, `hd::scale::half_wh`) so that no tile's colours mix into its
//! neighbour's. Magnified it is nearest (the pixels stay square); minified it
//! is linear within the nearest level, which keeps far walls from shimmering
//! and is no more than a plain texture to draw.
//!
//! A chunk's HD faces are one vertex buffer with a run of vertices for each
//! page (`GpuMesh::hd_runs`): the world is drawn once with the normal atlas,
//! then once per page in use. The draw calls grow by the number of pages.

use super::*;
use crate::hd::atlas::{Built, PageImage};

/// `LINEAR_MIPMAP_NEAREST`: linear in a level, the nearest level.
const MIN_FILTER_MIPS: i32 = 0x2701;

impl Renderer {
    /// Ask the card how big a texture may be and tell the HD planner (call before the atlas is built).
    pub(super) fn note_max_texture() {
        let g = gl();
        let mut max = 0i32;
        unsafe { (g.GetIntegerv)(gl::MAX_TEXTURE_SIZE, &mut max) };
        if max > 0 {
            crate::hd::set_max_texture(max as usize);
        }
    }

    /// Upload the pages of a built HD atlas, in place of the ones before (which are given back).
    pub(super) fn upload_hd(&mut self, built: Built) {
        let g = gl();
        self.flush(gl::QUADS);
        if !self.hd_tex.is_empty() {
            unsafe { (g.DeleteTextures)(self.hd_tex.len() as i32, self.hd_tex.as_ptr()) };
        }
        self.hd_tex.clear();
        self.hd_dims.clear();
        self.hd_bytes = built.bytes;
        self.hd_tiles = built.tiles;
        self.hd_cap = built.cap;
        for page in built.pages {
            let PageImage { w, h, levels, rgba } = page;
            let id = make_texture(w, h, &rgba, gl::NEAREST, gl::CLAMP_TO_EDGE);
            if levels > 0 {
                let (mut lw, mut lh) = (w, h);
                let mut level = rgba;
                for i in 1..=levels {
                    level = crate::hd::scale::half_wh(&level, lw, lh);
                    lw = (lw / 2).max(1);
                    lh = (lh / 2).max(1);
                    unsafe {
                        (g.TexImage2D)(gl::TEXTURE_2D, i as i32, gl::RGBA as i32, lw as i32, lh as i32, 0, gl::RGBA, gl::UNSIGNED_BYTE, level.as_ptr() as *const c_void);
                    }
                }
                unsafe {
                    (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_MAX_LEVEL, levels as i32);
                    (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, MIN_FILTER_MIPS);
                }
            }
            self.hd_tex.push(id);
            self.hd_dims.push((w, h));
        }
        self.batch_tex = 0;
    }

    /// The texture coordinates of a texture's picture of the HD atlas for drawing in 2D
    /// (item icons): the page's texture and (u0, v0, u1, v1). `None` for a normal tile.
    pub(super) fn hd_icon(&self, tile: u16) -> Option<(u32, [f32; 4])> {
        if self.hd_tex.is_empty() {
            return None;
        }
        let (page, uv) = crate::hd::table::current()?.uv(tile as usize)?;
        Some((*self.hd_tex.get(page)?, uv))
    }

    /// The texture matrix for an HD page: chunk vertices' coordinates are quarter pixels.
    fn hd_matrix(w: usize, h: usize) {
        let g = gl();
        let mut m = math::IDENTITY;
        m[0] = 1.0 / (w as f32 * crate::hd_split::UNITS as f32);
        m[5] = 1.0 / (h as f32 * crate::hd_split::UNITS as f32);
        unsafe {
            (g.MatrixMode)(gl::TEXTURE);
            (g.LoadMatrixf)(m.as_ptr());
            (g.MatrixMode)(gl::MODELVIEW);
        }
    }

    /// Draw the HD faces of the visible chunks, a page at a time, then leave the normal atlas bound.
    pub(super) fn draw_hd_chunks(&mut self, view: &Mat4, visible: &[(i32, Vec3, GpuMesh)]) {
        if self.hd_tex.is_empty() || !visible.iter().any(|v| v.2.hd != 0 && v.2.hd_runs.iter().any(|n| *n > 0)) {
            return;
        }
        let g = gl();
        for page in 0..self.hd_tex.len().min(4) {
            let (w, h) = self.hd_dims[page];
            unsafe { (g.BindTexture)(gl::TEXTURE_2D, self.hd_tex[page]) };
            Self::hd_matrix(w, h);
            for &(_, origin, m) in visible {
                let n = m.hd_runs[page];
                if n <= 0 || m.hd == 0 {
                    continue;
                }
                let first: i32 = m.hd_runs[..page].iter().sum();
                Self::draw_chunk_run(view, origin, m.hd, first, n);
                self.stats.quads += n as u32 / 4;
            }
        }
        unsafe { (g.BindTexture)(gl::TEXTURE_2D, self.atlas) };
        Self::atlas_matrix(true);
    }

    /// [`Renderer::draw_chunk`] for `count` vertices from `first`.
    fn draw_chunk_run(view: &Mat4, origin: Vec3, vbo: u32, first: i32, count: i32) {
        let g = gl();
        unsafe {
            let m = math::translate_scale(view, origin, 1.0 / 16.0);
            (g.LoadMatrixf)(m.as_ptr());
            (g.BindBuffer)(gl::ARRAY_BUFFER, vbo);
            (g.VertexPointer)(3, gl::SHORT, 20, core::ptr::null());
            (g.ColorPointer)(4, gl::UNSIGNED_BYTE, 20, 8 as *const c_void);
            (g.TexCoordPointer)(2, gl::SHORT, 20, 12 as *const c_void);
            (g.ClientActiveTexture)(gl::TEXTURE1);
            (g.TexCoordPointer)(2, gl::SHORT, 20, 16 as *const c_void);
            (g.ClientActiveTexture)(gl::TEXTURE0);
            (g.DrawArrays)(gl::QUADS, first, count);
        }
    }

    /// A picture of a creature or a model went to the card. What a picture costs beyond 128 x 128
    /// (about what the game's own models use) is memory the player chose, and the budget's limit
    /// rises by it: with none that big, nothing changes.
    pub(super) fn note_model_picture(&mut self, w: usize, h: usize) {
        let extra = (w * h * 4 * 4 / 3).saturating_sub(128 * 128 * 4 * 4 / 3);
        if extra > 0 {
            self.hd_model_bytes = self.hd_model_bytes.saturating_add(extra);
            crate::budget::set_allowance(crate::budget::Allow::Models, self.hd_model_bytes);
        }
    }

    /// Four numbers for the F3 screen: pages, tiles and bytes of the HD atlas, and the size its pictures ended up at.
    pub fn hd_summary(&self) -> (usize, usize, usize, usize) {
        (self.hd_tex.len(), self.hd_tiles, self.hd_bytes, self.hd_cap)
    }
}
