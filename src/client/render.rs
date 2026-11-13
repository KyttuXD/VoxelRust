//! The renderer: fixed-function OpenGL 1.5.
//!
//! World geometry is drawn from vertex buffers with two texture units: the
//! block atlas, and a 16 x 16 light map indexed by (block light, sky light).
//! Changing the time of day only rewrites those 256 texels; no mesh is
//! rebuilt. Everything else (sky, clouds, HUD) is batched client-side quads.

use crate::block::{reg, Bx};
use crate::chunk::{CH, CW};
use crate::font;
use crate::math::{self, clampf, cosf, lerp, sinf, smoothstep, Frustum, Mat4, Vec3, TAU};
use crate::mesh::{MeshData, Vertex};
use crate::noise::{hash2, noise2};
use crate::sys::gl::{self, gl};
use crate::textures;
use crate::world::{GpuMesh, MeshSink, World};
use alloc::vec::Vec;
use core::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct V2 {
    pos: [f32; 3],
    uv: [f32; 2],
    col: [u8; 4],
}

pub type Color = [u8; 4];
pub const WHITE: Color = [255, 255, 255, 255];

pub struct Camera {
    pub pos: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub fov: f32,
}

pub struct Env {
    /// 0..1; 0 is sunrise, 0.25 noon, 0.5 sunset.
    pub time: f32,
    /// View distance in chunks.
    pub distance: i32,
    pub clouds: bool,
    pub underwater: bool,
    /// Seconds since start, for cloud drift.
    pub clock: f32,
    /// How far distant terrain is drawn, in chunks (0 = off).
    pub far_distance: i32,
    /// 0 (clear) ..= 1 (pouring): greys and darkens the sky.
    pub rain: f32,
    /// Lightning: brightens everything for an instant.
    pub flash: f32,
}

#[derive(Default, Clone, Copy)]
pub struct FrameStats {
    pub chunks_drawn: u32,
    pub quads: u32,
}

pub struct Renderer {
    atlas: u32,
    light: u32,
    /// Five stages of cracks side by side, for blocks being dug.
    cracks: u32,
    /// Colour the next 2D model is multiplied by (then white again).
    pub tint_2d: [u8; 3],
    /// Which step of the water animation is in the atlas now.
    water_phase: i32,
    font: u32,
    clouds: u32,
    emblem: u32,
    pub w: i32,
    pub h: i32,
    batch: Vec<V2>,
    batch_tex: u32,
    light_key: i32,
    stars: Vec<V2>,
    pub stats: FrameStats,
    /// Bytes of vertex data currently on the graphics card.
    pub vbo_bytes: usize,
    /// Percentage of the window's resolution the 3D view is drawn at. Below
    /// 100 the view is drawn small and stretched, which old graphics cards
    /// handle far faster; the HUD stays sharp.
    pub render_scale: i32,
    scene_tex: u32,
    scene_tex_size: i32,
    /// Size the 3D view was drawn at this frame, if it still needs stretching.
    scaled: Option<(i32, i32)>,
    pub models: Vec<crate::model::ModelDef>,
    /// When not 0, the next model is drawn with this texture instead of
    /// its own (player skins). Reset after each model.
    pub tex_override: u32,
    model_verts: Vec<crate::model::ModelVert>,
    pass: Option<Pass>,
}

/// What `draw_world` hands on to `finish_world`.
struct Pass {
    view: Mat4,
    visible: Vec<(i32, Vec3, GpuMesh)>,
    far: f32,
    day: f32,
    fog: (f32, f32),
}

/// Switches for timing parts of the drawing on a particular machine
/// (`--exp N`): 1 no alpha test, 2 no fog, 4 no light texture.
pub static EXPERIMENT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Five 16 x 16 pictures of cracks, each with more than the last: lines
/// wandering out from the middle, dark and partly see-through.
fn crack_pixels() -> Vec<u8> {
    let mut px = alloc::vec![0u8; 80 * 16 * 4];
    for stage in 0..5usize {
        let mut seed = 0x9E37_79B9u32;
        let mut next = move || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 16) as i32
        };
        // The same walkers every stage, each going further: cracks grow.
        for _ in 0..(3 + stage * 2) {
            let (mut x, mut y) = (7 + next() % 3, 7 + next() % 3);
            let (dx, dy) = (next() % 3 - 1, next() % 3 - 1);
            for _ in 0..(4 + stage * 3) {
                if (0..16).contains(&x) && (0..16).contains(&y) {
                    let o = (y as usize * 80 + stage * 16 + x as usize) * 4;
                    px[o..o + 4].copy_from_slice(&[12, 12, 14, 190]);
                }
                // Mostly straight on, sometimes a jog to the side.
                let turn = next() % 4;
                x += if turn == 0 { next() % 3 - 1 } else { dx };
                y += if turn == 1 { next() % 3 - 1 } else { dy };
            }
        }
    }
    px
}

fn make_texture(w: usize, h: usize, rgba: &[u8], filter: i32, wrap: i32) -> u32 {
    let g = gl();
    let mut id = 0u32;
    unsafe {
        (g.GenTextures)(1, &mut id);
        (g.BindTexture)(gl::TEXTURE_2D, id);
        (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, filter);
        (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, filter);
        (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, wrap);
        (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, wrap);
        (g.TexImage2D)(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as i32,
            w as i32,
            h as i32,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            rgba.as_ptr() as *const c_void,
        );
    }
    id
}

/// Sky colours for a time of day: (zenith, horizon/fog, daylight 0..1).
pub fn sky_colors(time: f32) -> ([f32; 3], [f32; 3], f32) {
    let s = sinf(time * TAU);
    let day = smoothstep(-0.12, 0.22, s);
    let dusk = (1.0 - clampf(math::absf(s) * 3.5, 0.0, 1.0)) * smoothstep(-0.3, 0.0, s);
    let mix = |n: [f32; 3], d: [f32; 3]| [lerp(n[0], d[0], day), lerp(n[1], d[1], day), lerp(n[2], d[2], day)];
    let zenith = mix([0.01, 0.015, 0.05], [0.30, 0.55, 0.94]);
    let mut horizon = mix([0.03, 0.04, 0.09], [0.70, 0.83, 0.97]);
    let glow = [0.96, 0.52, 0.28];
    for i in 0..3 {
        horizon[i] = lerp(horizon[i], glow[i], dusk * 0.7);
    }
    (zenith, horizon, day)
}

impl Renderer {
    /// Create textures and set the state that never changes. Needs a current
    /// OpenGL context with [`gl::load`] done.
    pub fn new() -> Renderer {
        let g = gl();
        let r = reg();
        let atlas_px = textures::build_atlas(r);
        let atlas = make_texture(textures::ATLAS, textures::ATLAS, &atlas_px, gl::NEAREST, gl::CLAMP_TO_EDGE);
        unsafe {
            // Mip levels down to one pixel per tile, built by hand so that
            // no colour crosses from one tile into the next.
            let mut level = atlas_px;
            let mut size = textures::ATLAS;
            for i in 1..=4 {
                level = textures::half_size(&level, size);
                size /= 2;
                (g.TexImage2D)(
                    gl::TEXTURE_2D,
                    i,
                    gl::RGBA as i32,
                    size as i32,
                    size as i32,
                    0,
                    gl::RGBA,
                    gl::UNSIGNED_BYTE,
                    level.as_ptr() as *const c_void,
                );
            }
            (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_MAX_LEVEL, 4);
            (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST_MIPMAP_LINEAR);
        }
        let font = make_texture(font::TEX_W, font::TEX_H, &font::build_texture(), gl::NEAREST, gl::CLAMP_TO_EDGE);
        let light = make_texture(16, 16, &[255u8; 16 * 16 * 4], gl::LINEAR, gl::CLAMP_TO_EDGE);

        let mut cloud_px = alloc::vec![255u8; 64 * 64 * 4];
        for y in 0..64 {
            for x in 0..64 {
                // Wrap the noise so the texture tiles.
                let n = |ox: i32, oy: i32| noise2(77, x + ox, y + oy, 8);
                let (fx, fy) = (x as f32 / 64.0, y as f32 / 64.0);
                let v = lerp(lerp(n(0, 0), n(-64, 0), fx), lerp(n(0, -64), n(-64, -64), fx), fy);
                cloud_px[((y * 64 + x) * 4 + 3) as usize] = if v > 0.56 { 215 } else { 0 };
            }
        }
        let clouds = make_texture(64, 64, &cloud_px, gl::NEAREST, gl::REPEAT);
        let emblem = make_texture(128, 128, &crate::icon::rgba(128), gl::LINEAR, gl::CLAMP_TO_EDGE);

        let mut stars = Vec::new();
        for i in 0..420 {
            let u = (hash2(1, i, 0) % 10_000) as f32 / 10_000.0;
            let v = (hash2(2, i, 0) % 10_000) as f32 / 10_000.0;
            let (a, y) = (u * TAU, v * 2.0 - 1.0);
            let rad = math::sqrtf(1.0 - y * y);
            let b = 150 + (hash2(3, i, 0) % 105) as u8;
            stars.push(V2 { pos: [cosf(a) * rad * 90.0, y * 90.0, sinf(a) * rad * 90.0], uv: [0.0, 0.0], col: [b, b, 255, 255] });
        }

        unsafe {
            (g.PixelStorei)(gl::UNPACK_ALIGNMENT, 1);
            (g.PixelStorei)(gl::PACK_ALIGNMENT, 1);
            (g.DepthFunc)(gl::LEQUAL);
            (g.CullFace)(gl::BACK);
            (g.AlphaFunc)(gl::GREATER, 0.5);
            (g.BlendFunc)(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);
            (g.Fogi)(gl::FOG_MODE, gl::LINEAR);
            (g.Hint)(gl::FOG_HINT, gl::FASTEST);
            (g.Hint)(gl::PERSPECTIVE_CORRECTION_HINT, gl::FASTEST);
            (g.EnableClientState)(gl::VERTEX_ARRAY);
            (g.EnableClientState)(gl::COLOR_ARRAY);
            (g.EnableClientState)(gl::TEXTURE_COORD_ARRAY);
            // The light map lives on texture unit 1 for the whole run.
            (g.ActiveTexture)(gl::TEXTURE1);
            (g.BindTexture)(gl::TEXTURE_2D, light);
            (g.MatrixMode)(gl::TEXTURE);
            let mut m = math::IDENTITY;
            m[0] = 1.0 / 256.0;
            m[5] = 1.0 / 256.0;
            (g.LoadMatrixf)(m.as_ptr());
            (g.ActiveTexture)(gl::TEXTURE0);
        }
        Renderer {
            atlas,
            light,
            water_phase: 0,
            tint_2d: [255; 3],
            cracks: make_texture(80, 16, &crack_pixels(), gl::NEAREST, gl::CLAMP_TO_EDGE),
            font,
            clouds,
            emblem,
            w: 1,
            h: 1,
            batch: Vec::new(),
            batch_tex: 0,
            light_key: -1,
            stars,
            stats: FrameStats::default(),
            vbo_bytes: 0,
            render_scale: 100,
            scene_tex: 0,
            scene_tex_size: 0,
            scaled: None,
            models: Vec::new(),
            tex_override: 0,
            model_verts: Vec::new(),
            pass: None,
        }
    }

    fn update_light_map(&mut self, day: f32) {
        let key = (day * 96.0) as i32;
        if key == self.light_key {
            return;
        }
        self.light_key = key;
        let mut px = [0u8; 16 * 16 * 4];
        // Each light level is 82% as bright as the one above it.
        let mut curve = [0f32; 16];
        let mut v = 1.0;
        for l in (0..16).rev() {
            curve[l] = v;
            v *= 0.82;
        }
        let sky_scale = 0.13 + 0.87 * day;
        let sky_tint = [lerp(0.55, 1.0, day), lerp(0.65, 1.0, day), 1.0];
        for sky in 0..16 {
            for blk in 0..16 {
                let o = (sky * 16 + blk) * 4;
                let s = curve[sky] * sky_scale;
                let b = curve[blk];
                let warm = [1.0, 0.86, 0.66];
                for c in 0..3 {
                    let val = (s * sky_tint[c]).max(b * warm[c]) * 0.965 + 0.035;
                    px[o + c] = (clampf(val, 0.0, 1.0) * 255.0) as u8;
                }
                px[o + 3] = 255;
            }
        }
        let g = gl();
        unsafe {
            (g.ActiveTexture)(gl::TEXTURE1);
            (g.TexImage2D)(gl::TEXTURE_2D, 0, gl::RGBA as i32, 16, 16, 0, gl::RGBA, gl::UNSIGNED_BYTE, px.as_ptr() as *const c_void);
            (g.ActiveTexture)(gl::TEXTURE0);
        }
        let _ = self.light;
    }

    // ------------------------------------------------------------- batches

    fn set_tex(&mut self, tex: u32, mode: u32) {
        if tex != self.batch_tex {
            self.flush(mode);
            self.batch_tex = tex;
        }
    }

    fn quad(&mut self, p: [[f32; 3]; 4], uv: [[f32; 2]; 4], col: Color) {
        for i in 0..4 {
            self.batch.push(V2 { pos: p[i], uv: uv[i], col });
        }
    }

    fn draw_v2(verts: &[V2], mode: u32, tex: u32) {
        if verts.is_empty() {
            return;
        }
        let g = gl();
        unsafe {
            (g.BindBuffer)(gl::ARRAY_BUFFER, 0);
            if tex != 0 {
                (g.Enable)(gl::TEXTURE_2D);
                (g.BindTexture)(gl::TEXTURE_2D, tex);
            } else {
                (g.Disable)(gl::TEXTURE_2D);
            }
            let base = verts.as_ptr() as *const u8;
            (g.VertexPointer)(3, gl::FLOAT, 24, base as *const c_void);
            (g.TexCoordPointer)(2, gl::FLOAT, 24, base.add(12) as *const c_void);
            (g.ColorPointer)(4, gl::UNSIGNED_BYTE, 24, base.add(20) as *const c_void);
            (g.DrawArrays)(mode, 0, verts.len() as i32);
        }
    }

    fn flush(&mut self, mode: u32) {
        Self::draw_v2(&self.batch, mode, self.batch_tex);
        self.batch.clear();
    }

    fn load_matrices(proj: &Mat4, view: &Mat4) {
        let g = gl();
        unsafe {
            (g.MatrixMode)(gl::PROJECTION);
            (g.LoadMatrixf)(proj.as_ptr());
            (g.MatrixMode)(gl::MODELVIEW);
            (g.LoadMatrixf)(view.as_ptr());
        }
    }

    /// Unit 0 texture matrix: identity for 0..1 coordinates, or the block
    /// atlas scale for mesh vertices (1/16ths of a pixel).
    fn atlas_matrix(mesh_units: bool) {
        let g = gl();
        let mut m = math::IDENTITY;
        if mesh_units {
            let units = (textures::PER_ROW * 256) as f32;
            m[0] = 1.0 / units;
            m[5] = 1.0 / units;
        }
        unsafe {
            (g.MatrixMode)(gl::TEXTURE);
            (g.LoadMatrixf)(m.as_ptr());
            (g.MatrixMode)(gl::MODELVIEW);
        }
    }

    // ---------------------------------------------------------------- world

    /// Draw the sky and the solid world. Models and the block outline are
    /// drawn next, then [`Renderer::finish_world`] adds water and clouds.
    pub fn draw_world(&mut self, world: &World, cam: &Camera, env: &Env) {
        let g = gl();
        self.stats = FrameStats::default();
        let (mut zenith, mut fog, mut day) = sky_colors(env.time);
        if env.rain > 0.0 || env.flash > 0.0 {
            // Overcast: colours drain toward grey and the light drops.
            let k = env.rain.clamp(0.0, 1.0) * 0.7;
            let grey = (0.30 + 0.25 * day) * (1.0 - 0.3 * env.rain);
            for c in 0..3 {
                zenith[c] = lerp(zenith[c], grey * 0.85, k);
                fog[c] = lerp(fog[c], grey, k);
            }
            day = (day * (1.0 - 0.35 * env.rain) + env.flash * 3.0).clamp(0.0, 1.0);
            if env.flash > 0.0 {
                for c in 0..3 {
                    zenith[c] = lerp(zenith[c], 0.95, env.flash * 3.0);
                    fog[c] = lerp(fog[c], 0.95, env.flash * 3.0);
                }
            }
        }
        if env.underwater {
            fog = [0.04 + 0.06 * day, 0.16 + 0.14 * day, 0.30 + 0.25 * day];
        }
        self.update_light_map(day);
        let aspect = self.w as f32 / self.h.max(1) as f32;
        let view = math::view_rotation(cam.yaw, cam.pitch);
        let fogc = [fog[0], fog[1], fog[2], 1.0];
        let scale = self.render_scale.clamp(25, 100);
        let (vw, vh) = ((self.w * scale / 100).max(16), (self.h * scale / 100).max(16));
        self.scaled = if scale < 100 { Some((vw, vh)) } else { None };
        unsafe {
            (g.Viewport)(0, 0, vw, vh);
            (g.ClearColor)(fog[0], fog[1], fog[2], 1.0);
            (g.DepthMask)(1);
            (g.Clear)(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
            (g.Fogfv)(gl::FOG_COLOR, fogc.as_ptr());
            (g.Disable)(gl::CULL_FACE);
            (g.Disable)(gl::ALPHA_TEST);
            (g.Disable)(gl::DEPTH_TEST);
            (g.DepthMask)(0);
        }
        Self::atlas_matrix(false);

        // --- Sky: a coloured ceiling that fog fades into the horizon.
        if !env.underwater {
            Self::load_matrices(&math::perspective(cam.fov, aspect, 1.0, 2000.0), &view);
            unsafe {
                (g.Enable)(gl::FOG);
                (g.Fogf)(gl::FOG_START, 40.0);
                (g.Fogf)(gl::FOG_END, 420.0);
                (g.Disable)(gl::BLEND);
            }
            let zc = [(zenith[0] * 255.0) as u8, (zenith[1] * 255.0) as u8, (zenith[2] * 255.0) as u8, 255];
            self.set_tex(0, gl::QUADS);
            let n = 10;
            let step = 120.0;
            for i in -n..n {
                for j in -n..n {
                    let (x0, z0) = (i as f32 * step, j as f32 * step);
                    let (x1, z1) = (x0 + step, z0 + step);
                    self.quad([[x0, 34.0, z0], [x1, 34.0, z0], [x1, 34.0, z1], [x0, 34.0, z1]], [[0.0; 2]; 4], zc);
                }
            }
            self.flush(gl::QUADS);
            unsafe {
                (g.Disable)(gl::FOG);
                (g.Enable)(gl::BLEND);
            }
            // Stars fade in at night.
            let night = 1.0 - day;
            if night > 0.05 {
                let a = (night * 255.0) as u8;
                let turn = env.time * TAU;
                let (s, c) = (sinf(turn), cosf(turn));
                let stars: Vec<V2> = self
                    .stars
                    .iter()
                    .map(|v| V2 { pos: [v.pos[0] * c - v.pos[1] * s, v.pos[0] * s + v.pos[1] * c, v.pos[2]], uv: v.uv, col: [v.col[0], v.col[1], v.col[2], a] })
                    .collect();
                unsafe { (g.PointSize)(if self.h > 900 { 2.0 } else { 1.5 }) };
                Self::draw_v2(&stars, gl::POINTS, 0);
            }
            // Sun and moon.
            let a = env.time * TAU;
            let sun = Vec3::new(cosf(a), sinf(a), 0.28).norm();
            let r = reg();
            for (dir, tile, size) in [(sun, r.sun_tile, 16.0f32), (sun * -1.0, r.moon_tile, 11.0)] {
                if dir.y < -0.25 {
                    continue;
                }
                let right = dir.cross(Vec3::new(0.0, 0.0, 1.0)).norm() * size;
                let up = right.cross(dir).norm() * size;
                let c = dir * 100.0;
                let p = |sx: f32, sy: f32| {
                    let v = c + right * sx + up * sy;
                    [v.x, v.y, v.z]
                };
                let n = textures::PER_ROW as u16;
                let e = 1.0 / n as f32;
                let (u0, v0) = ((tile % n) as f32 * e, (tile / n) as f32 * e);
                self.set_tex(self.atlas, gl::QUADS);
                self.quad(
                    [p(-1.0, -1.0), p(1.0, -1.0), p(1.0, 1.0), p(-1.0, 1.0)],
                    [[u0, v0 + e], [u0 + e, v0 + e], [u0 + e, v0], [u0, v0]],
                    WHITE,
                );
            }
            self.flush(gl::QUADS);
        }

        // --- Terrain.
        let far = env.distance as f32 * CW as f32;
        let proj = math::perspective(cam.fov, aspect, 0.08, far * 1.5 + 96.0);
        Self::load_matrices(&proj, &view);
        let frustum = Frustum::new(&math::mat_mul(&proj, &view));
        let (ccx, ccz) = crate::world::chunk_at(cam.pos[0], cam.pos[2]);
        let reach = env.distance * env.distance + env.distance;
        let mut visible: Vec<(i32, Vec3, GpuMesh)> = Vec::new();
        for (&(cx, cz), chunk) in world.chunks.iter() {
            let m = match chunk.mesh {
                Some(m) if m.opaque_count > 0 || m.water_count > 0 => m,
                _ => continue,
            };
            let (dx, dz) = (cx - ccx, cz - ccz);
            let d2 = dx * dx + dz * dz;
            if d2 > reach {
                continue;
            }
            let origin = Vec3::new(
                ((cx * CW) as f64 - cam.pos[0]) as f32,
                (0.0 - cam.pos[1]) as f32,
                ((cz * CW) as f64 - cam.pos[2]) as f32,
            );
            let min = Vec3::new(origin.x, origin.y + m.ymin as f32, origin.z);
            let max = Vec3::new(origin.x + CW as f32, origin.y + m.ymax as f32, origin.z + CW as f32);
            if frustum.box_visible(min, max) {
                visible.push((d2, origin, m));
            }
        }
        visible.sort_unstable_by_key(|v| v.0);

        // With distant terrain behind them, the detailed chunks must not
        // fade out at their own edge: fog is set by the far distance instead.
        let lod_far = env.far_distance as f32 * CW as f32;
        let lod = lod_far > far && !env.underwater && !world.far.is_empty();
        let (fog_start, fog_end) = if env.underwater {
            (0.0, 26.0)
        } else if lod {
            (lod_far * 0.45, lod_far * 0.98)
        } else {
            (far * 0.55, far * 0.96)
        };
        unsafe {
            (g.Enable)(gl::FOG);
            (g.Fogf)(gl::FOG_START, fog_start);
            (g.Fogf)(gl::FOG_END, fog_end);
            (g.Enable)(gl::DEPTH_TEST);
            (g.DepthMask)(1);
            (g.Enable)(gl::CULL_FACE);
            (g.Enable)(gl::ALPHA_TEST);
            (g.Disable)(gl::BLEND);
            (g.Enable)(gl::TEXTURE_2D);
            (g.BindTexture)(gl::TEXTURE_2D, self.atlas);
            (g.ActiveTexture)(gl::TEXTURE1);
            (g.Enable)(gl::TEXTURE_2D);
            (g.ActiveTexture)(gl::TEXTURE0);
            (g.ClientActiveTexture)(gl::TEXTURE1);
            (g.EnableClientState)(gl::TEXTURE_COORD_ARRAY);
            (g.ClientActiveTexture)(gl::TEXTURE0);
        }
        Self::atlas_matrix(true);
        let exp = EXPERIMENT.load(core::sync::atomic::Ordering::Relaxed);
        unsafe {
            if exp & 1 != 0 {
                (g.Disable)(gl::ALPHA_TEST);
            }
            if exp & 2 != 0 {
                (g.Disable)(gl::FOG);
            }
            if exp & 4 != 0 {
                (g.ActiveTexture)(gl::TEXTURE1);
                (g.Disable)(gl::TEXTURE_2D);
                (g.ActiveTexture)(gl::TEXTURE0);
            }
        }
        // Detailed chunks first, in the front nine tenths of the depth range.
        // Distant terrain then takes the last tenth, so it can never cover
        // them, and the graphics card skips every distant pixel that a
        // nearby block already hides (most of the screen, usually).
        unsafe { (g.DepthRange)(0.0, if lod { 0.9 } else { 1.0 }) };
        for &(_, origin, m) in &visible {
            if m.opaque_count > 0 {
                Self::draw_chunk(&view, origin, m.opaque, m.opaque_count);
                self.stats.chunks_drawn += 1;
                self.stats.quads += m.opaque_count as u32 / 4;
            }
        }
        if lod {
            let far_proj = math::perspective(cam.fov, aspect, 2.0, lod_far * 1.3 + 256.0);
            let far_frustum = Frustum::new(&math::mat_mul(&far_proj, &view));
            unsafe {
                (g.MatrixMode)(gl::PROJECTION);
                (g.LoadMatrixf)(far_proj.as_ptr());
                (g.MatrixMode)(gl::MODELVIEW);
                (g.DepthRange)(0.9, 1.0);
                (g.Disable)(gl::ALPHA_TEST);
                // Distant terrain is coloured by its vertices alone.
                (g.Disable)(gl::TEXTURE_2D);
            }
            let tile = crate::lod::TILE as f32;
            // Tiles wholly inside the detailed area (less a margin for
            // chunks still loading at its edge) are hidden anyway.
            let inner = (far - 2.0 * CW as f32).max(0.0);
            // Nearest first: each tile then hides the pixels of those behind
            // it before they are shaded.
            let mut tiles: Vec<(i64, Vec3, GpuMesh)> = Vec::new();
            for (&(tx, tz), t) in world.far.iter() {
                let m = match t.mesh {
                    Some(m) if m.opaque_count > 0 => m,
                    _ => continue,
                };
                let origin = Vec3::new(
                    ((tx * crate::lod::TILE) as f64 - cam.pos[0]) as f32,
                    (0.0 - cam.pos[1]) as f32,
                    ((tz * crate::lod::TILE) as f64 - cam.pos[2]) as f32,
                );
                let reach_x = math::absf(origin.x).max(math::absf(origin.x + tile));
                let reach_z = math::absf(origin.z).max(math::absf(origin.z + tile));
                if world.settled && reach_x * reach_x + reach_z * reach_z < inner * inner {
                    continue;
                }
                let min = Vec3::new(origin.x, origin.y + m.ymin as f32, origin.z);
                let max = Vec3::new(origin.x + tile, origin.y + m.ymax as f32, origin.z + tile);
                if far_frustum.box_visible(min, max) {
                    let (cx, cz) = (origin.x + tile * 0.5, origin.z + tile * 0.5);
                    tiles.push(((cx * cx + cz * cz) as i64, origin, m));
                }
            }
            tiles.sort_unstable_by_key(|t| t.0);
            for &(_, origin, m) in &tiles {
                Self::draw_chunk(&view, origin, m.opaque, m.opaque_count);
                self.stats.quads += m.opaque_count as u32 / 4;
            }
            unsafe {
                (g.MatrixMode)(gl::PROJECTION);
                (g.LoadMatrixf)(proj.as_ptr());
                (g.MatrixMode)(gl::MODELVIEW);
                (g.DepthRange)(0.0, 0.9);
                (g.Enable)(gl::TEXTURE_2D);
                (g.Enable)(gl::ALPHA_TEST);
            }
        }
        // Leave the state ready for models: plain batches on unit 0 only.
        unsafe {
            (g.BindBuffer)(gl::ARRAY_BUFFER, 0);
            (g.ActiveTexture)(gl::TEXTURE1);
            (g.Disable)(gl::TEXTURE_2D);
            (g.ActiveTexture)(gl::TEXTURE0);
            (g.ClientActiveTexture)(gl::TEXTURE1);
            (g.DisableClientState)(gl::TEXTURE_COORD_ARRAY);
            (g.ClientActiveTexture)(gl::TEXTURE0);
            (g.LoadMatrixf)(view.as_ptr());
            (g.Disable)(gl::CULL_FACE);
        }
        Self::atlas_matrix(false);
        self.pass = Some(Pass { view, visible, far, day, fog: (fog_start, fog_end) });
    }

    fn draw_chunk(view: &Mat4, origin: Vec3, vbo: u32, count: i32) {
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
            (g.DrawArrays)(gl::QUADS, 0, count);
        }
    }

    /// Rain or snow around the camera. Call between [`Renderer::draw_world`]
    /// and [`Renderer::finish_world`].
    pub fn draw_precipitation(&mut self, cam: &Camera, rain: f32, snow: bool, clock: f32) {
        let count = (rain.clamp(0.0, 1.0) * 700.0) as i32;
        if count == 0 {
            return;
        }
        let g = gl();
        unsafe {
            (g.Enable)(gl::BLEND);
            (g.Enable)(gl::DEPTH_TEST);
            (g.Disable)(gl::ALPHA_TEST);
            (g.LineWidth)(if snow { 2.0 } else { 1.0 });
        }
        self.flush(gl::QUADS);
        self.batch_tex = 0;
        // Drops live on a fixed grid in the world, so they do not slide
        // about when the camera moves; only the part near the camera is drawn.
        let frac = |v: f64| (v - math::floord(v) as f64) as f32;
        let (ox, oy, oz) = (frac(cam.pos[0] / 28.0) * 28.0, frac(cam.pos[1] / 24.0) * 24.0, frac(cam.pos[2] / 28.0) * 28.0);
        let (speed, len, col): (f32, f32, Color) = if snow { (2.2, 0.07, [255, 255, 255, 230]) } else { (19.0, 0.9, [170, 190, 230, 130]) };
        for i in 0..count {
            let h = hash2(11, i, 7);
            let wrap = |v: f32, m: f32| {
                let r = v - math::floorf(v / m) * m;
                r - m * 0.5
            };
            let sway = if snow { sinf(clock * 1.3 + i as f32) * 0.4 } else { 0.0 };
            let x = wrap((h & 1023) as f32 / 1023.0 * 28.0 - ox + sway, 28.0);
            let z = wrap(((h >> 10) & 1023) as f32 / 1023.0 * 28.0 - oz, 28.0);
            let y = wrap(((h >> 20) & 1023) as f32 / 1023.0 * 24.0 - clock * speed - oy, 24.0);
            self.batch.push(V2 { pos: [x, y, z], uv: [0.0; 2], col });
            self.batch.push(V2 { pos: [x, y + len, z], uv: [0.0; 2], col });
        }
        self.flush(gl::LINES);
    }

    /// A whirling funnel, `at` relative to the camera (its foot).
    /// Small bright things in flight (arrows, spells): each a little
    /// three-way cross, so it shows from any side. Positions are relative
    /// to the camera.
    pub fn draw_sparks(&mut self, sparks: &[([f32; 3], [u8; 3])]) {
        if sparks.is_empty() {
            return;
        }
        let g = gl();
        unsafe {
            (g.Disable)(gl::BLEND);
            (g.Enable)(gl::DEPTH_TEST);
            (g.Disable)(gl::ALPHA_TEST);
            (g.Disable)(gl::CULL_FACE);
        }
        self.flush(gl::QUADS);
        self.batch_tex = 0;
        let k = 0.09;
        for &(p, c) in sparks {
            let col = [c[0], c[1], c[2], 255];
            let v = |dx: f32, dy: f32, dz: f32| [p[0] + dx, p[1] + dy, p[2] + dz];
            self.quad([v(-k, -k, 0.0), v(k, -k, 0.0), v(k, k, 0.0), v(-k, k, 0.0)], [[0.0; 2]; 4], col);
            self.quad([v(0.0, -k, -k), v(0.0, -k, k), v(0.0, k, k), v(0.0, k, -k)], [[0.0; 2]; 4], col);
            self.quad([v(-k, 0.0, -k), v(k, 0.0, -k), v(k, 0.0, k), v(-k, 0.0, k)], [[0.0; 2]; 4], col);
        }
        self.flush(gl::QUADS);
    }

    pub fn draw_funnel(&mut self, at: [f32; 3], clock: f32) {
        let g = gl();
        unsafe {
            (g.Enable)(gl::BLEND);
            (g.Enable)(gl::DEPTH_TEST);
            (g.Disable)(gl::ALPHA_TEST);
            (g.Disable)(gl::CULL_FACE);
            (g.DepthMask)(0);
        }
        self.flush(gl::QUADS);
        self.batch_tex = 0;
        let rings = 14;
        let sides = 10;
        for k in 0..rings {
            // Narrow at the ground, wide at the cloud, and swaying.
            let (y0, y1) = (k as f32 * 4.0, (k + 1) as f32 * 4.0);
            let radius = |y: f32| 1.2 + y * 0.16;
            let sway = |y: f32| (sinf(clock * 1.7 + y * 0.12) * (0.4 + y * 0.03), cosf(clock * 1.3 + y * 0.1) * (0.4 + y * 0.03));
            let shade = 96 + ((k * 37) % 40) as u8;
            let col = [shade, shade, shade + 6, 150];
            for s in 0..sides {
                let a0 = clock * 4.0 + s as f32 / sides as f32 * math::TAU + k as f32 * 0.5;
                let a1 = a0 + math::TAU / sides as f32;
                let p = |a: f32, y: f32| {
                    let (sx, sz) = sway(y);
                    [at[0] + sx + cosf(a) * radius(y), at[1] + y, at[2] + sz + sinf(a) * radius(y)]
                };
                self.quad([p(a0, y0), p(a1, y0), p(a1, y1), p(a0, y1)], [[0.0; 2]; 4], col);
            }
        }
        self.flush(gl::QUADS);
        unsafe { (g.DepthMask)(1) };
    }

    /// Draw one posed model. Call between [`Renderer::draw_world`] and
    /// [`Renderer::finish_world`]. `at` is relative to the camera, `tint`
    /// multiplies the texture (light level, team colour, hurt flash).
    pub fn draw_model(&mut self, model: usize, pose: &crate::model::Pose, at: [f32; 3], yaw: f32, height: f32, tint: [u8; 3]) {
        let def = match self.models.get(model) {
            Some(d) => d,
            None => return,
        };
        self.model_verts.clear();
        def.quads(pose, at, yaw, height, false, &mut self.model_verts);
        let tex = if self.tex_override != 0 { self.tex_override } else { def.tex };
        self.tex_override = 0;
        let g = gl();
        unsafe {
            (g.Enable)(gl::FOG);
            (g.Enable)(gl::DEPTH_TEST);
            (g.Enable)(gl::ALPHA_TEST);
            (g.Disable)(gl::BLEND);
            (g.Disable)(gl::CULL_FACE);
        }
        self.flush(gl::QUADS);
        self.batch_tex = tex;
        for i in 0..self.model_verts.len() {
            let v = self.model_verts[i];
            let k = v.shade as u32;
            let col = [(tint[0] as u32 * k / 255) as u8, (tint[1] as u32 * k / 255) as u8, (tint[2] as u32 * k / 255) as u8, 255];
            self.batch.push(V2 { pos: v.pos, uv: v.uv, col });
        }
        self.flush(gl::QUADS);
    }

    /// Draw a model flat on the screen (inventory portrait). `x`, `y` is
    /// where its feet go, in pixels; `height` its height in pixels.
    /// How tall a model is and how far it spreads sideways, in its own units.
    pub fn model_extent(&self, model: usize) -> (f32, f32) {
        match self.models.get(model) {
            Some(d) => {
                let m = &d.model;
                (d.height(), (m.max[0] - m.min[0]).max(m.max[2] - m.min[2]).max(0.001))
            }
            None => (1.0, 1.0),
        }
    }

    /// The first-person hand: the arm model drawn with the player's own skin
    /// (`tex`), then the held item from the block atlas. Both are in the
    /// hand's space: the depth buffer is cleared and a close-up projection
    /// is used. `to_view` moves each model point into view space.
    pub fn draw_first_person(&mut self, model: usize, tex: u32, fov: f32, height: f32, to_view: &dyn Fn([f32; 3]) -> [f32; 3], items: &[crate::client::hand::Quad]) {
        let g = gl();
        self.flush(gl::QUADS);
        self.model_verts.clear();
        if let Some(def) = self.models.get(model) {
            def.quads(&crate::model::Pose::default(), [0.0, 0.0, 0.0], 0.0, height, false, &mut self.model_verts);
        }
        let aspect = self.w as f32 / self.h.max(1) as f32;
        Self::load_matrices(&math::perspective(fov, aspect, 0.03, 8.0), &math::IDENTITY);
        unsafe {
            (g.Clear)(gl::DEPTH_BUFFER_BIT);
            (g.Enable)(gl::DEPTH_TEST);
            (g.Disable)(gl::CULL_FACE);
            (g.Disable)(gl::FOG);
            (g.Disable)(gl::BLEND);
            (g.Enable)(gl::ALPHA_TEST);
        }
        Self::atlas_matrix(false);
        let arm: Vec<V2> = self.model_verts.iter().map(|v| V2 { pos: to_view(v.pos), uv: v.uv, col: [v.shade, v.shade, v.shade, 255] }).collect();
        if !arm.is_empty() && tex != 0 {
            self.batch_tex = tex;
            self.batch.extend(arm);
            self.flush(gl::QUADS);
        }
        self.batch_tex = self.atlas;
        for q in items {
            for k in 0..4 {
                self.batch.push(V2 { pos: q.p[k], uv: q.uv[k], col: q.col });
            }
        }
        self.flush(gl::QUADS);
        unsafe {
            (g.Enable)(gl::FOG);
        }
    }

    /// Draw flat-shaded quads from the block atlas (the hand and what it
    /// holds, see `client::hand`). With `view_fov`, they are in the hand's
    /// own space: the depth buffer is cleared and a close-up projection is
    /// used, so the hand is never hidden by, or clipped into, the world.
    /// Without it they are in the world's camera-relative space.
    pub fn draw_quads(&mut self, quads: &[crate::client::hand::Quad], view_fov: Option<f32>) {
        if quads.is_empty() {
            return;
        }
        let g = gl();
        self.flush(gl::QUADS);
        match view_fov {
            Some(fov) => {
                let aspect = self.w as f32 / self.h.max(1) as f32;
                Self::load_matrices(&math::perspective(fov, aspect, 0.03, 8.0), &math::IDENTITY);
                unsafe { (g.Clear)(gl::DEPTH_BUFFER_BIT) };
            }
            None => {}
        }
        unsafe {
            (g.Enable)(gl::DEPTH_TEST);
            (g.Disable)(gl::CULL_FACE);
            (g.Disable)(gl::FOG);
            (g.Disable)(gl::BLEND);
            (g.Enable)(gl::ALPHA_TEST);
        }
        Self::atlas_matrix(false);
        self.batch_tex = self.atlas;
        for q in quads {
            for k in 0..4 {
                self.batch.push(V2 { pos: q.p[k], uv: q.uv[k], col: q.col });
            }
        }
        self.flush(gl::QUADS);
        unsafe {
            (g.Enable)(gl::FOG);
        }
    }

    pub fn draw_model_2d(&mut self, model: usize, pose: &crate::model::Pose, x: f32, y: f32, yaw: f32, height: f32) {
        let def = match self.models.get(model) {
            Some(d) => d,
            None => return,
        };
        self.model_verts.clear();
        def.quads(pose, [x, y, 0.0], yaw, height, true, &mut self.model_verts);
        // Painter's order by depth, since the 2D pass has no depth test:
        // sort whole quads back to front (larger z is nearer here).
        let mut quads: Vec<usize> = (0..self.model_verts.len() / 4).collect();
        let depth = |q: usize| (0..4).map(|k| self.model_verts[q * 4 + k].pos[2]).sum::<f32>();
        quads.sort_by(|&a, &b| depth(a).partial_cmp(&depth(b)).unwrap_or(core::cmp::Ordering::Equal));
        let tex = if self.tex_override != 0 { self.tex_override } else { def.tex };
        self.tex_override = 0;
        self.flush(gl::QUADS);
        self.batch_tex = tex;
        unsafe { (gl().Enable)(gl::ALPHA_TEST) };
        for q in quads {
            for k in 0..4 {
                let v = self.model_verts[q * 4 + k];
                let c = v.shade as u16;
                let t = self.tint_2d;
                self.batch.push(V2 { pos: [v.pos[0], v.pos[1], 0.0], uv: v.uv, col: [(c * t[0] as u16 / 255) as u8, (c * t[1] as u16 / 255) as u8, (c * t[2] as u16 / 255) as u8, 255] });
            }
        }
        self.flush(gl::QUADS);
        self.tint_2d = [255; 3];
        unsafe { (gl().Disable)(gl::ALPHA_TEST) };
    }

    /// Where a point (relative to the camera) lands on screen, in pixels,
    /// if it is in front of the camera. For name tags.
    pub fn project(&self, cam: &Camera, rel: [f32; 3]) -> Option<(f32, f32)> {
        let aspect = self.w as f32 / self.h.max(1) as f32;
        let m = math::mat_mul(&math::perspective(cam.fov, aspect, 0.08, 1000.0), &math::view_rotation(cam.yaw, cam.pitch));
        let row = |i: usize| m[i] * rel[0] + m[4 + i] * rel[1] + m[8 + i] * rel[2] + m[12 + i];
        let w = row(3);
        if w <= 0.05 {
            return None;
        }
        Some(((row(0) / w * 0.5 + 0.5) * self.w as f32, (0.5 - row(1) / w * 0.5) * self.h as f32))
    }

    /// The second half of drawing the world: water, then clouds.
    pub fn finish_world(&mut self, cam: &Camera, env: &Env) {
        let g = gl();
        let Pass { view, visible, far, day, fog } = match self.pass.take() {
            Some(p) => p,
            None => return,
        };
        self.flush(gl::QUADS);
        self.animate_water(env.clock);
        unsafe {
            (g.Enable)(gl::FOG);
            (g.Fogf)(gl::FOG_START, fog.0);
            (g.Fogf)(gl::FOG_END, fog.1);
            (g.Enable)(gl::DEPTH_TEST);
            (g.DepthMask)(1);
            (g.Enable)(gl::CULL_FACE);
            (g.Enable)(gl::TEXTURE_2D);
            (g.BindTexture)(gl::TEXTURE_2D, self.atlas);
            (g.ActiveTexture)(gl::TEXTURE1);
            (g.Enable)(gl::TEXTURE_2D);
            (g.ActiveTexture)(gl::TEXTURE0);
            (g.ClientActiveTexture)(gl::TEXTURE1);
            (g.EnableClientState)(gl::TEXTURE_COORD_ARRAY);
            (g.ClientActiveTexture)(gl::TEXTURE0);
        }
        Self::atlas_matrix(true);
        self.batch_tex = self.atlas;
        // Water last, far to near, so what is behind it shows through.
        unsafe {
            (g.Enable)(gl::BLEND);
            (g.Disable)(gl::ALPHA_TEST);
        }
        // A slow swell: the whole surface rises and falls a little.
        let swell = math::sinf(env.clock * 1.1) * 0.03 - 0.035;
        for &(_, origin, m) in visible.iter().rev() {
            if m.water_count > 0 {
                Self::draw_chunk(&view, Vec3::new(origin.x, origin.y + swell, origin.z), m.water, m.water_count);
                self.stats.quads += m.water_count as u32 / 4;
            }
        }
        unsafe {
            (g.BindBuffer)(gl::ARRAY_BUFFER, 0);
            (g.ActiveTexture)(gl::TEXTURE1);
            (g.Disable)(gl::TEXTURE_2D);
            (g.ActiveTexture)(gl::TEXTURE0);
            (g.ClientActiveTexture)(gl::TEXTURE1);
            (g.DisableClientState)(gl::TEXTURE_COORD_ARRAY);
            (g.ClientActiveTexture)(gl::TEXTURE0);
            (g.LoadMatrixf)(view.as_ptr());
            (g.Disable)(gl::CULL_FACE);
        }
        Self::atlas_matrix(false);

        // --- Clouds: one drifting, tiling layer above the world.
        if env.clouds && !env.underwater {
            let y = (CH as f64 + 6.0 - cam.pos[1]) as f32;
            let tile = 64.0 * 8.0; // one texture repeat, in blocks
            let frac = |v: f64| (v / tile as f64 - math::floord(v / tile as f64) as f64) as f32;
            let (u0, v0) = (frac(cam.pos[0] + env.clock as f64 * 1.5), frac(cam.pos[2]));
            let half = (far * 1.3).max(160.0);
            let shade = (150.0 + 105.0 * day) as u8;
            let col = [shade, shade, shade, 230];
            unsafe {
                (g.DepthMask)(0);
                (g.Fogf)(gl::FOG_START, half * 0.5);
                (g.Fogf)(gl::FOG_END, half * 1.0);
            }
            self.set_tex(self.clouds, gl::QUADS);
            let n = 8;
            let step = half * 2.0 / n as f32;
            for i in 0..n {
                for j in 0..n {
                    let (x0, z0) = (-half + i as f32 * step, -half + j as f32 * step);
                    let (x1, z1) = (x0 + step, z0 + step);
                    let uv = |x: f32, z: f32| [u0 + x / tile, v0 + z / tile];
                    self.quad([[x0, y, z0], [x1, y, z0], [x1, y, z1], [x0, y, z1]], [uv(x0, z0), uv(x1, z0), uv(x1, z1), uv(x0, z1)], col);
                }
            }
            self.flush(gl::QUADS);
            unsafe { (g.DepthMask)(1) };
        }
        unsafe { (g.Disable)(gl::FOG) };
    }

    /// Draw a wire frame around boxes inside `cell` (the targeted block).
    pub fn outline(&mut self, cam: &Camera, cell: [i32; 3], boxes: &[Bx]) {
        let g = gl();
        let o = [
            (cell[0] as f64 - cam.pos[0]) as f32,
            (cell[1] as f64 - cam.pos[1]) as f32,
            (cell[2] as f64 - cam.pos[2]) as f32,
        ];
        self.set_tex(0, gl::LINES);
        let col = [0, 0, 0, 170];
        for b in boxes {
            let e = 0.004;
            let lo = [0, 1, 2].map(|i| o[i] + b.min[i] as f32 / 16.0 - e);
            let hi = [0, 1, 2].map(|i| o[i] + b.max[i] as f32 / 16.0 + e);
            let c = |i: usize| [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
            for (a, bb) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                self.batch.push(V2 { pos: c(a), uv: [0.0; 2], col });
                self.batch.push(V2 { pos: c(bb), uv: [0.0; 2], col });
            }
        }
        unsafe {
            (g.Enable)(gl::BLEND);
            (g.Enable)(gl::DEPTH_TEST);
            (g.LineWidth)(if self.h > 900 { 2.0 } else { 1.0 });
        }
        self.flush(gl::LINES);
    }

    /// Step the water texture: repaint its tile in the atlas a few times a
    /// second (only when the content uses the built-in water picture).
    fn animate_water(&mut self, clock: f32) {
        let phase = (clock * 5.0) as i32 & 7;
        if phase == self.water_phase {
            return;
        }
        self.water_phase = phase;
        let r = reg();
        let tile = crate::block::def(r.wk.water).tex[0] as usize;
        if !matches!(r.tiles.get(tile).map(|t| &t.src), Some(crate::block::TileSrc::Builtin(n)) if n == "water") {
            return;
        }
        let g = gl();
        let (tx, ty) = ((tile % textures::PER_ROW) as i32, (tile / textures::PER_ROW) as i32);
        let mut level = textures::water_frame(phase);
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

    /// Cracks spreading over a block as it is dug: `progress` 0..1.
    pub fn crack(&mut self, cam: &Camera, cell: [i32; 3], boxes: &[Bx], progress: f32) {
        let g = gl();
        let o = [
            (cell[0] as f64 - cam.pos[0]) as f32,
            (cell[1] as f64 - cam.pos[1]) as f32,
            (cell[2] as f64 - cam.pos[2]) as f32,
        ];
        let stage = ((progress * 5.0) as i32).clamp(0, 4) as f32;
        let (u0, u1) = (stage / 5.0, (stage + 1.0) / 5.0);
        self.set_tex(self.cracks, gl::QUADS);
        let col = [255, 255, 255, 255];
        for b in boxes {
            let e = 0.003;
            let lo = [0, 1, 2].map(|i| o[i] + b.min[i] as f32 / 16.0 - e);
            let hi = [0, 1, 2].map(|i| o[i] + b.max[i] as f32 / 16.0 + e);
            // Each face: the fixed axis, its value, and the two it spans.
            for (axis, at) in [(0, lo[0]), (0, hi[0]), (1, lo[1]), (1, hi[1]), (2, lo[2]), (2, hi[2])] {
                let (a, c) = ((axis + 1) % 3, (axis + 2) % 3);
                let corner = |s: f32, t: f32| {
                    let mut p = [0.0f32; 3];
                    p[axis] = at;
                    p[a] = lo[a] + (hi[a] - lo[a]) * s;
                    p[c] = lo[c] + (hi[c] - lo[c]) * t;
                    p
                };
                self.quad([corner(0.0, 0.0), corner(1.0, 0.0), corner(1.0, 1.0), corner(0.0, 1.0)], [[u0, 0.0], [u1, 0.0], [u1, 1.0], [u0, 1.0]], col);
            }
        }
        unsafe {
            (g.Enable)(gl::BLEND);
            (g.Enable)(gl::DEPTH_TEST);
            (g.Disable)(gl::CULL_FACE);
            (g.DepthMask)(0);
        }
        self.flush(gl::QUADS);
        unsafe { (g.DepthMask)(1) };
    }

    // ------------------------------------------------------------------ 2D

    /// Repaint the block atlas from the current registry (content reloaded).
    pub fn rebuild_atlas(&mut self) {
        let g = gl();
        let mut level = textures::build_atlas(reg());
        let mut size = textures::ATLAS;
        self.flush(gl::QUADS);
        unsafe {
            (g.BindTexture)(gl::TEXTURE_2D, self.atlas);
            for i in 0..=4 {
                (g.TexImage2D)(gl::TEXTURE_2D, i, gl::RGBA as i32, size as i32, size as i32, 0, gl::RGBA, gl::UNSIGNED_BYTE, level.as_ptr() as *const c_void);
                level = textures::half_size(&level, size);
                size /= 2;
            }
        }
        self.batch_tex = 0;
    }

    /// Parse every model the data files declare and upload its texture.
    /// Indices match the registry's. A model that fails to load is logged
    /// and replaced by the first good one, so creatures never vanish.
    pub fn load_models(&mut self) {
        let r = reg();
        let mut loaded: Vec<Option<crate::model::ModelDef>> = Vec::new();
        for spec in &r.models {
            match crate::model::ModelDef::from_bbmodel(&spec.id, &spec.text) {
                Ok(mut def) => {
                    def.tex = match &def.model.texture {
                        Some(img) => make_texture(img.w, img.h, &img.rgba, gl::NEAREST, gl::CLAMP_TO_EDGE),
                        None => make_texture(1, 1, &[255, 255, 255, 255], gl::NEAREST, gl::CLAMP_TO_EDGE),
                    };
                    loaded.push(Some(def));
                }
                Err(e) => {
                    logf!("voxelrust: data: model {}: {}", spec.id, e);
                    loaded.push(None);
                }
            }
        }
        let fallback = loaded.iter().position(|m| m.is_some());
        self.models.clear();
        for (i, m) in loaded.iter().enumerate() {
            let src = if m.is_some() { Some(i) } else { fallback };
            if let Some(Some(def)) = src.map(|j| &loaded[j]) {
                let mut copy = crate::model::ModelDef::from_bbmodel(&def.id, &r.models[src.unwrap()].text).unwrap();
                copy.tex = def.tex;
                self.models.push(copy);
            }
        }
    }

    /// Clear the screen to one colour (menus without a world behind them).
    pub fn clear(&mut self, rgb: [f32; 3]) {
        let g = gl();
        unsafe {
            (g.Viewport)(0, 0, self.w, self.h);
            (g.ClearColor)(rgb[0], rgb[1], rgb[2], 1.0);
            (g.DepthMask)(1);
            (g.Clear)(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }
    }

    /// Upload a picture as a texture and return its handle.
    pub fn add_texture(&mut self, w: usize, h: usize, rgba: &[u8]) -> u32 {
        self.flush(gl::QUADS);
        let id = make_texture(w, h, rgba, gl::NEAREST, gl::CLAMP_TO_EDGE);
        self.batch_tex = 0;
        id
    }

    /// Replace the pixels of a texture made with [`Renderer::add_texture`].
    pub fn update_texture(&mut self, tex: u32, w: usize, h: usize, rgba: &[u8]) {
        self.flush(gl::QUADS);
        let g = gl();
        unsafe {
            (g.BindTexture)(gl::TEXTURE_2D, tex);
            (g.TexSubImage2D)(gl::TEXTURE_2D, 0, 0, 0, w as i32, h as i32, gl::RGBA, gl::UNSIGNED_BYTE, rgba.as_ptr() as *const c_void);
        }
        self.batch_tex = 0;
    }

    /// Draw a whole texture into a rectangle.
    pub fn image(&mut self, tex: u32, x: f32, y: f32, w: f32, h: f32) {
        self.set_tex(tex, gl::QUADS);
        self.quad(
            [[x, y, 0.0], [x + w, y, 0.0], [x + w, y + h, 0.0], [x, y + h, 0.0]],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            WHITE,
        );
    }

    /// A block as a little 3D picture: seen from above and one corner,
    /// top lit, sides shaded. `boxes` empty means a whole cube.
    pub fn icon_block(&mut self, x: f32, y: f32, size: f32, tiles: [u16; 3], boxes: &[Bx], col: Color) {
        self.set_tex(self.atlas, gl::QUADS);
        let n = textures::PER_ROW as u16;
        let e = 1.0 / n as f32;
        let inset = e / 64.0;
        // Block space (0..1 each way, y up) to the icon's square.
        // A true isometric view: a cube comes out as tall as it is wide.
        let at = |p: [f32; 3]| [x + (0.5 + (p[0] - p[2]) * 0.433) * size, y + (0.5 + (p[0] + p[2]) * 0.25 - p[1] * 0.5) * size, 0.0];
        let uv_of = |tile: u16, u: f32, v: f32| {
            let (tu, tv) = ((tile % n) as f32 * e, (tile / n) as f32 * e);
            [tu + inset + u * (e - 2.0 * inset), tv + inset + v * (e - 2.0 * inset)]
        };
        let shade = |k: u16| [0, 1, 2, 3].map(|i| if i < 3 { (col[i] as u16 * k / 255) as u8 } else { col[3] });
        let full = [Bx::FULL];
        let mut list: Vec<&Bx> = if boxes.is_empty() { full.iter().collect() } else { boxes.iter().collect() };
        // Back to front: what is nearer the viewer is drawn over the rest.
        list.sort_by_key(|b| b.max[0] as i32 + b.max[2] as i32 + b.max[1] as i32);
        for b in list {
            let lo = [0, 1, 2].map(|i| b.min[i] as f32 / 16.0);
            let hi = [0, 1, 2].map(|i| b.max[i] as f32 / 16.0);
            // Top.
            self.quad(
                [at([lo[0], hi[1], lo[2]]), at([hi[0], hi[1], lo[2]]), at([hi[0], hi[1], hi[2]]), at([lo[0], hi[1], hi[2]])],
                [uv_of(tiles[0], lo[0], lo[2]), uv_of(tiles[0], hi[0], lo[2]), uv_of(tiles[0], hi[0], hi[2]), uv_of(tiles[0], lo[0], hi[2])],
                shade(255),
            );
            // The side facing left (south).
            self.quad(
                [at([lo[0], hi[1], hi[2]]), at([hi[0], hi[1], hi[2]]), at([hi[0], lo[1], hi[2]]), at([lo[0], lo[1], hi[2]])],
                [uv_of(tiles[1], lo[0], 1.0 - hi[1]), uv_of(tiles[1], hi[0], 1.0 - hi[1]), uv_of(tiles[1], hi[0], 1.0 - lo[1]), uv_of(tiles[1], lo[0], 1.0 - lo[1])],
                shade(196),
            );
            // The side facing right (east).
            self.quad(
                [at([hi[0], hi[1], hi[2]]), at([hi[0], hi[1], lo[2]]), at([hi[0], lo[1], lo[2]]), at([hi[0], lo[1], hi[2]])],
                [uv_of(tiles[1], 1.0 - hi[2], 1.0 - hi[1]), uv_of(tiles[1], 1.0 - lo[2], 1.0 - hi[1]), uv_of(tiles[1], 1.0 - lo[2], 1.0 - lo[1]), uv_of(tiles[1], 1.0 - hi[2], 1.0 - lo[1])],
                shade(142),
            );
        }
    }

    /// The game's emblem.
    pub fn emblem(&mut self, x: f32, y: f32, size: f32) {
        self.set_tex(self.emblem, gl::QUADS);
        self.quad(
            [[x, y, 0.0], [x + size, y, 0.0], [x + size, y + size, 0.0], [x, y + size, 0.0]],
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            WHITE,
        );
    }

    /// Switch to pixel coordinates with the origin at the top left.
    pub fn begin_2d(&mut self) {
        let g = gl();
        self.flush(gl::QUADS);
        Self::load_matrices(&math::ortho(self.w as f32, self.h as f32), &math::IDENTITY);
        unsafe {
            (g.Viewport)(0, 0, self.w, self.h);
            (g.Disable)(gl::DEPTH_TEST);
            (g.Disable)(gl::CULL_FACE);
            (g.Disable)(gl::ALPHA_TEST);
            (g.Disable)(gl::FOG);
            (g.Enable)(gl::BLEND);
        }
        if let Some((vw, vh)) = self.scaled.take() {
            self.stretch_scene(vw, vh);
        }
    }

    /// The 3D view was drawn into the bottom-left `vw` x `vh` pixels: copy
    /// it to a texture and draw that over the whole window. (Plain OpenGL
    /// 1.1; no framebuffer objects needed.)
    fn stretch_scene(&mut self, vw: i32, vh: i32) {
        let g = gl();
        // A power-of-two texture big enough, for cards that need one.
        let mut need = 256;
        while need < vw.max(vh) {
            need *= 2;
        }
        unsafe {
            if self.scene_tex == 0 || self.scene_tex_size != need {
                if self.scene_tex == 0 {
                    (g.GenTextures)(1, &mut self.scene_tex);
                }
                (g.BindTexture)(gl::TEXTURE_2D, self.scene_tex);
                (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR);
                (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR);
                (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
                (g.TexParameteri)(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
                (g.TexImage2D)(gl::TEXTURE_2D, 0, gl::RGB as i32, need, need, 0, gl::RGB, gl::UNSIGNED_BYTE, core::ptr::null());
                self.scene_tex_size = need;
            }
            (g.BindTexture)(gl::TEXTURE_2D, self.scene_tex);
            (g.CopyTexSubImage2D)(gl::TEXTURE_2D, 0, 0, 0, 0, 0, vw, vh);
            (g.Disable)(gl::BLEND);
        }
        // Half a texel in from the edge, so nothing outside the view bleeds in.
        let (u, v) = ((vw as f32 - 0.5) / need as f32, (vh as f32 - 0.5) / need as f32);
        let (w, h) = (self.w as f32, self.h as f32);
        self.batch_tex = self.scene_tex;
        // The texture's first row is the bottom of the picture.
        self.quad([[0.0, 0.0, 0.0], [w, 0.0, 0.0], [w, h, 0.0], [0.0, h, 0.0]], [[0.0, v], [u, v], [u, 0.0], [0.0, 0.0]], WHITE);
        self.flush(gl::QUADS);
        unsafe { (g.Enable)(gl::BLEND) };
    }

    pub fn end_2d(&mut self) {
        self.flush(gl::QUADS);
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, col: Color) {
        self.set_tex(0, gl::QUADS);
        self.quad([[x, y, 0.0], [x + w, y, 0.0], [x + w, y + h, 0.0], [x, y + h, 0.0]], [[0.0; 2]; 4], col);
    }

    /// A hollow rectangle `t` pixels thick.
    pub fn frame(&mut self, x: f32, y: f32, w: f32, h: f32, t: f32, col: Color) {
        self.rect(x, y, w, t, col);
        self.rect(x, y + h - t, w, t, col);
        self.rect(x, y + t, t, h - 2.0 * t, col);
        self.rect(x + w - t, y + t, t, h - 2.0 * t, col);
    }

    /// One atlas tile, as an item icon.
    pub fn icon(&mut self, x: f32, y: f32, size: f32, tile: u16, col: Color) {
        self.set_tex(self.atlas, gl::QUADS);
        let n = textures::PER_ROW as u16;
        let e = 1.0 / n as f32;
        let (u, v) = ((tile % n) as f32 * e, (tile / n) as f32 * e);
        let i = e / 64.0;
        self.quad(
            [[x, y, 0.0], [x + size, y, 0.0], [x + size, y + size, 0.0], [x, y + size, 0.0]],
            [[u + i, v + i], [u + e - i, v + i], [u + e - i, v + e - i], [u + i, v + e - i]],
            col,
        );
    }

    /// An icon for a block made of boxes: the tile cut to the block's side
    /// profile, so stairs and slabs are told apart from full blocks.
    pub fn icon_boxes(&mut self, x: f32, y: f32, size: f32, tile: u16, boxes: &[Bx], col: Color) {
        self.set_tex(self.atlas, gl::QUADS);
        let n = textures::PER_ROW as u16;
        let e = 1.0 / n as f32;
        let (u, v) = ((tile % n) as f32 * e, (tile / n) as f32 * e);
        for b in boxes {
            // Seen from the side: depth runs left to right, height upward.
            let (l, r) = (b.min[2] as f32 / 16.0, b.max[2] as f32 / 16.0);
            let (t, bt) = (1.0 - b.max[1] as f32 / 16.0, 1.0 - b.min[1] as f32 / 16.0);
            self.quad(
                [[x + l * size, y + t * size, 0.0], [x + r * size, y + t * size, 0.0], [x + r * size, y + bt * size, 0.0], [x + l * size, y + bt * size, 0.0]],
                [[u + l * e, v + t * e], [u + r * e, v + t * e], [u + r * e, v + bt * e], [u + l * e, v + bt * e]],
                col,
            );
        }
    }

    pub fn text_width(s: &str, scale: f32) -> f32 {
        s.chars().count() as f32 * font::ADVANCE * scale
    }

    pub fn text(&mut self, x: f32, y: f32, scale: f32, s: &str, col: Color) {
        self.set_tex(self.font, gl::QUADS);
        let (cw, ch) = (font::CELL as f32 / font::TEX_W as f32, font::CELL as f32 / font::TEX_H as f32);
        let size = font::CELL as f32 * scale;
        let mut cx = x;
        for c in s.chars() {
            let gi = font::glyph_index(c);
            let (u, v) = ((gi % 16) as f32 * cw, (gi / 16) as f32 * ch);
            self.quad(
                [[cx, y, 0.0], [cx + size, y, 0.0], [cx + size, y + size, 0.0], [cx, y + size, 0.0]],
                [[u, v], [u + cw, v], [u + cw, v + ch], [u, v + ch]],
                col,
            );
            cx += font::ADVANCE * scale;
        }
    }

    /// Text with a dark drop shadow, readable over any background.
    pub fn label(&mut self, x: f32, y: f32, scale: f32, s: &str, col: Color) {
        self.text(x + scale, y + scale, scale, s, [0, 0, 0, 170]);
        self.text(x, y, scale, s, col);
    }

    /// The current frame as a 24-bit BMP file.
    pub fn screenshot(&self) -> Vec<u8> {
        let (w, h) = (self.w as usize, self.h as usize);
        let mut px = alloc::vec![0u8; w * h * 3];
        unsafe {
            (gl().Finish)();
            (gl().ReadPixels)(0, 0, self.w, self.h, gl::RGB, gl::UNSIGNED_BYTE, px.as_mut_ptr() as *mut c_void);
        }
        let row = (w * 3 + 3) & !3;
        let size = 54 + row * h;
        let mut out = Vec::with_capacity(size);
        out.extend_from_slice(b"BM");
        out.extend_from_slice(&(size as u32).to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&54u32.to_le_bytes());
        out.extend_from_slice(&40u32.to_le_bytes());
        out.extend_from_slice(&(w as i32).to_le_bytes());
        out.extend_from_slice(&(h as i32).to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&24u16.to_le_bytes());
        out.extend_from_slice(&[0; 24]);
        // OpenGL rows already run bottom-up, as BMP wants; swap to BGR.
        for y in 0..h {
            for x in 0..w {
                let o = (y * w + x) * 3;
                out.extend_from_slice(&[px[o + 2], px[o + 1], px[o]]);
            }
            out.resize(out.len() + row - w * 3, 0);
        }
        out
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Renderer::new()
    }
}

impl MeshSink for Renderer {
    fn upload(&mut self, m: &MeshData) -> GpuMesh {
        let g = gl();
        let mut make = |v: &[Vertex]| -> u32 {
            if v.is_empty() {
                return 0;
            }
            let mut id = 0u32;
            let bytes = core::mem::size_of_val(v);
            unsafe {
                (g.GenBuffers)(1, &mut id);
                (g.BindBuffer)(gl::ARRAY_BUFFER, id);
                (g.BufferData)(gl::ARRAY_BUFFER, bytes as isize, v.as_ptr() as *const c_void, gl::STATIC_DRAW);
            }
            self.vbo_bytes += bytes;
            id
        };
        let mesh = GpuMesh {
            opaque: make(&m.opaque),
            opaque_count: m.opaque.len() as i32,
            water: make(&m.water),
            water_count: m.water.len() as i32,
            ymin: m.ymin,
            ymax: m.ymax,
        };
        unsafe { (g.BindBuffer)(gl::ARRAY_BUFFER, 0) };
        mesh
    }

    fn free(&mut self, m: GpuMesh) {
        let ids = [m.opaque, m.water];
        self.vbo_bytes = self.vbo_bytes.saturating_sub((m.opaque_count + m.water_count) as usize * core::mem::size_of::<Vertex>());
        unsafe { (gl().DeleteBuffers)(2, ids.as_ptr()) };
    }
}
