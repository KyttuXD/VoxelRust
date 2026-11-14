//! Drawing the effects mods ask for (game/fx): particles as little flat
//! pictures facing the camera, and translucent quads kept in the world.
//!
//! It runs after the water and the clouds ([`Renderer::draw_effects`]), when
//! everything solid is on screen: quads and particles are tested against the
//! depth buffer but write none, so they blend over what is behind them and
//! never hide each other's depth. Quads are drawn farthest first. Particles
//! come from one small picture, eight 8 x 8 flat shapes in white that the
//! vertex colour tints (the OCD style: flat fills, no soft edges).

use super::*;
use crate::game::fx::{particles, quads::{Blend, DRAW_RANGE}};
use crate::session::Session;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};

/// The GL texture of the particle shapes (0: not made yet).
static PARTICLE_TEX: AtomicU32 = AtomicU32::new(0);

/// Farthest a particle is drawn, blocks.
const PARTICLE_RANGE: f32 = 64.0;
/// Most quads and particles drawn in a frame.
const MAX_QUADS_DRAWN: usize = 1024;
const MAX_PARTICLES_DRAWN: usize = 2000;

/// The eight shapes, in the order of `particles::Kind`: spark, smoke, flame,
/// bubble, dust, magic, heart, cloud. `#` is solid.
const SHAPES: [[&str; 8]; 8] = [
    ["...##...", "...##...", "...##...", "########", "########", "...##...", "...##...", "...##..."],
    ["..####..", ".######.", "########", "########", "########", "########", ".######.", "..####.."],
    ["...##...", "...##...", "..####..", "..####..", ".######.", ".######.", ".######.", "..####.."],
    ["..####..", ".#....#.", "#......#", "#......#", "#......#", "#......#", ".#....#.", "..####.."],
    ["........", "........", "..####..", "..####..", "..####..", "..####..", "........", "........"],
    ["...##...", "..####..", ".######.", "########", "########", ".######.", "..####..", "...##..."],
    [".##..##.", "########", "########", "########", ".######.", "..####..", "...##...", "........"],
    ["........", "...###..", ".######.", "########", "########", ".######.", "........", "........"],
];

/// The pixels of the picture of particle shapes: 8 shapes side by side, 64 x 8.
pub fn particle_pixels() -> Vec<u8> {
    let mut px = alloc::vec![0u8; 64 * 8 * 4];
    for (k, shape) in SHAPES.iter().enumerate() {
        for (y, row) in shape.iter().enumerate() {
            for (x, c) in row.bytes().enumerate() {
                if c == b'#' {
                    let o = (y * 64 + k * 8 + x) * 4;
                    px[o..o + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
    }
    px
}

/// A particle to draw: where (relative to the camera), half its width, its
/// colour with opacity, whether it is added, and which shape.
#[derive(Clone, Copy, Debug)]
pub struct PartDraw {
    pub at: [f32; 3],
    pub half: f32,
    pub col: [u8; 4],
    pub additive: bool,
    pub tile: u8,
}

/// What a quad is drawn with.
#[derive(Clone, Debug, PartialEq)]
pub enum QuadTex {
    /// A plain colour.
    Plain,
    /// A tile of the block atlas.
    Tile(u16),
    /// A picture a mod brought, by name.
    Picture(String),
}

/// A quad to draw: its corners relative to the camera.
#[derive(Clone, Debug)]
pub struct QuadDraw {
    pub p: [[f32; 3]; 4],
    pub tex: QuadTex,
    pub col: [u8; 4],
    pub additive: bool,
}

/// What a quad's texture text means: `block:<id>` the top picture of that
/// block, a name a mod's picture, empty a plain colour.
pub fn tex_of(text: &str) -> QuadTex {
    if text.is_empty() {
        return QuadTex::Plain;
    }
    match text.strip_prefix("block:") {
        Some(id) => match reg().block(id) {
            Some(b) => QuadTex::Tile(crate::block::def(b).tex[0]),
            None => QuadTex::Plain,
        },
        None => QuadTex::Picture(String::from(text)),
    }
}

/// The squared distance of a quad's middle from the camera (to sort by).
fn middle_dist2(p: &[[f32; 3]; 4]) -> f32 {
    let mut m = [0.0f32; 3];
    for c in p {
        for a in 0..3 {
            m[a] += c[a] * 0.25;
        }
    }
    m[0] * m[0] + m[1] * m[1] + m[2] * m[2]
}

impl Renderer {
    /// The particle shapes' texture, made the first time it is needed.
    fn particle_texture(&mut self) -> u32 {
        let t = PARTICLE_TEX.load(Ordering::Relaxed);
        if t != 0 {
            return t;
        }
        let t = make_texture(64, 8, &particle_pixels(), gl::NEAREST, gl::CLAMP_TO_EDGE);
        PARTICLE_TEX.store(t, Ordering::Relaxed);
        t
    }

    /// Draw the particles and quads the session holds, over the finished world.
    pub fn draw_effects(&mut self, g: &Session, cam: &Camera, env: &Env) {
        let rel = |p: [f64; 3]| [(p[0] - cam.pos[0]) as f32, (p[1] - cam.pos[1]) as f32, (p[2] - cam.pos[2]) as f32];
        let ahead = math::forward(cam.yaw, cam.pitch);
        let in_front = |p: [f32; 3], size: f32| p[0] * ahead.x + p[1] * ahead.y + p[2] * ahead.z > -size;
        // Particles of the kinds (kind 0, the plain specks, are drawn with the creatures).
        let mut parts: Vec<PartDraw> = Vec::new();
        for s in g.primed.sparks.iter().filter(|s| s.kind != 0) {
            if parts.len() >= MAX_PARTICLES_DRAWN {
                break;
            }
            let Some((half, alpha, additive, tile)) = particles::appearance(s) else { continue };
            if alpha == 0 {
                continue;
            }
            let at = rel(s.pos);
            if at[0] * at[0] + at[1] * at[1] + at[2] * at[2] > PARTICLE_RANGE * PARTICLE_RANGE || !in_front(at, half) {
                continue;
            }
            parts.push(PartDraw { at, half, col: [s.colour[0], s.colour[1], s.colour[2], alpha], additive, tile });
        }
        // Quads, those within range and not wholly behind.
        let mut quads: Vec<(f32, QuadDraw)> = Vec::new();
        if let Some(st) = crate::game::fx::state_ref(g) {
            for q in st.quads.list.iter() {
                if quads.len() >= MAX_QUADS_DRAWN || q.rgba[3] == 0 {
                    continue;
                }
                let p = [rel(q.corners[0]), rel(q.corners[1]), rel(q.corners[2]), rel(q.corners[3])];
                let d2 = middle_dist2(&p);
                if d2 > (DRAW_RANGE * DRAW_RANGE) as f32 || p.iter().all(|c| !in_front(*c, 1.0)) {
                    continue;
                }
                quads.push((d2, QuadDraw { p, tex: tex_of(&q.tex), col: q.rgba, additive: q.blend == Blend::Additive }));
            }
        }
        if parts.is_empty() && quads.is_empty() {
            return;
        }
        // Farthest first.
        quads.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(core::cmp::Ordering::Equal));
        let view = math::view_rotation(cam.yaw, cam.pitch);
        let right = [view[0], view[4], view[8]];
        let up = [view[1], view[5], view[9]];
        self.flush(gl::QUADS);
        let aspect = self.w as f32 / self.h.max(1) as f32;
        let far = env.distance as f32 * CW as f32 * 1.5 + 96.0;
        Self::load_matrices(&math::perspective(cam.fov, aspect, 0.08, far), &view);
        Self::atlas_matrix(false);
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
        // The quads, each with the picture it names.
        let mut blend_add = false;
        for (_, q) in &quads {
            if q.additive != blend_add {
                self.flush(gl::QUADS);
                blend_add = q.additive;
                unsafe { (gl_.BlendFunc)(gl::SRC_ALPHA, if blend_add { gl::ONE } else { gl::ONE_MINUS_SRC_ALPHA }) };
            }
            let (tex, uv) = match &q.tex {
                QuadTex::Plain => (0, [[0.0; 2]; 4]),
                QuadTex::Tile(t) => {
                    let n = textures::PER_ROW as u16;
                    let e = 1.0 / n as f32;
                    let (u0, v0) = ((*t % n) as f32 * e, (*t / n) as f32 * e);
                    (self.atlas, [[u0, v0 + e], [u0 + e, v0 + e], [u0 + e, v0], [u0, v0]])
                }
                QuadTex::Picture(name) => {
                    let t = self.entity_texture(name);
                    (t, [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]])
                }
            };
            // A picture that is not there draws as a plain colour.
            let uv = if tex == 0 { [[0.0; 2]; 4] } else { uv };
            self.set_tex(tex, gl::QUADS);
            self.quad(q.p, uv, q.col);
        }
        self.flush(gl::QUADS);
        // The particles: laid over first, then the glowing ones added.
        if !parts.is_empty() {
            let tex = self.particle_texture();
            for additive in [false, true] {
                unsafe { (gl_.BlendFunc)(gl::SRC_ALPHA, if additive { gl::ONE } else { gl::ONE_MINUS_SRC_ALPHA }) };
                self.batch_tex = tex;
                for p in parts.iter().filter(|p| p.additive == additive) {
                    let (r, u) = ([right[0] * p.half, right[1] * p.half, right[2] * p.half], [up[0] * p.half, up[1] * p.half, up[2] * p.half]);
                    let c = |sx: f32, sy: f32| [p.at[0] + r[0] * sx + u[0] * sy, p.at[1] + r[1] * sx + u[1] * sy, p.at[2] + r[2] * sx + u[2] * sy];
                    let (u0, u1) = (p.tile as f32 / 8.0, (p.tile as f32 + 1.0) / 8.0);
                    self.quad([c(-1.0, -1.0), c(1.0, -1.0), c(1.0, 1.0), c(-1.0, 1.0)], [[u0, 1.0], [u1, 1.0], [u1, 0.0], [u0, 0.0]], p.col);
                }
                self.flush(gl::QUADS);
            }
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
    fn the_shapes_are_eight_by_eight_and_not_empty() {
        for (k, s) in SHAPES.iter().enumerate() {
            assert_eq!(s.len(), 8, "shape {}", k);
            for row in s {
                assert_eq!(row.len(), 8, "shape {}", k);
                assert!(row.bytes().all(|c| c == b'#' || c == b'.'));
            }
            assert!(s.iter().any(|r| r.contains('#')), "shape {} is blank", k);
        }
        // One shape for each kind of particle.
        assert_eq!(SHAPES.len(), particles::Kind::ALL.len());
    }

    #[test]
    fn the_picture_has_one_tile_for_each_shape() {
        let px = particle_pixels();
        assert_eq!(px.len(), 64 * 8 * 4);
        for k in 0..8 {
            let mut solid = 0;
            for y in 0..8 {
                for x in 0..8 {
                    if px[(y * 64 + k * 8 + x) * 4 + 3] == 255 {
                        solid += 1;
                    }
                }
            }
            let expect: usize = SHAPES[k].iter().map(|r| r.bytes().filter(|c| *c == b'#').count()).sum();
            assert_eq!(solid, expect, "tile {}", k);
            assert!(solid > 4 && solid < 64);
        }
    }

    #[test]
    fn a_texture_text_says_what_to_draw() {
        assert_eq!(tex_of(""), QuadTex::Plain);
        assert_eq!(tex_of("storm"), QuadTex::Picture(String::from("storm")));
        assert_eq!(tex_of("block:no_such_block_anywhere"), QuadTex::Plain);
    }

    #[test]
    fn the_nearest_quad_sorts_last() {
        let near = [[0.0, 0.0, 1.0]; 4];
        let far = [[0.0, 0.0, 9.0]; 4];
        assert!(middle_dist2(&far) > middle_dist2(&near));
        assert_eq!(middle_dist2(&near), 1.0);
    }
}
