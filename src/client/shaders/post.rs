//! Compiling the chosen shader and drawing its passes (see docs/SHADERS.md).
//!
//! Everything here is guarded: a card without shader support, a shader that
//! does not compile or link, an id nobody has loaded: each is logged and
//! leaves [`Post::passes`] at 0, so the renderer takes its normal path and
//! the player never sees a black screen because of a shader.

use crate::client::shaders::names::{self, UserUniforms};
use crate::client::shaders::{plan, source};
use crate::sys::gl::{self, gl};
use crate::sys::gl_shader::{self, shader_api, ShaderApi};
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::c_void;

/// What the renderer knows about this frame, for the uniforms.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame {
    /// Seconds, wrapped (see `plan::wrap_time`).
    pub time: f32,
    /// Towards the sun, in the view's axes.
    pub sun: [f32; 3],
    pub daylight: f32,
    pub underwater: bool,
    pub rain: f32,
    /// Vertical field of view in degrees.
    pub fov: f32,
}

/// Where the game's uniforms are in one program (-1: the shader does not use it).
struct Locs {
    scene: i32,
    map: i32,
    time: i32,
    resolution: i32,
    sun: i32,
    daylight: i32,
    underwater: i32,
    rain: i32,
    fov: i32,
}

struct Prog {
    id: u32,
    loc: Locs,
    /// Locations of the uniforms scripts set, found the first time each is needed.
    user: Vec<(String, i32)>,
}

pub struct Post {
    /// The shader the player chose (its id; empty = none).
    pub wanted: String,
    pub frame: Frame,
    /// What scripts have set (`set_shader_uniform`).
    pub user: UserUniforms,
    /// Why the chosen shader is not running, if it is not (empty: nothing wrong).
    pub error: String,
    programs: Vec<Prog>,
    /// The text the programs were compiled from.
    compiled: Vec<String>,
    /// The registry and the id the last attempt was made for.
    attempted: Option<(usize, String)>,
    quad: u32,
}

impl Post {
    pub fn new() -> Post {
        Post { wanted: String::new(), frame: Frame::default(), user: UserUniforms::default(), error: String::new(), programs: Vec::new(), compiled: Vec::new(), attempted: None, quad: 0 }
    }

    /// Whether this graphics driver can run shaders at all.
    pub fn supported(&self) -> bool {
        shader_api().is_some()
    }

    /// How many passes the chosen shader runs this frame: 0 means "draw the
    /// normal way". Compiles the shader the first time (and again if the
    /// pack changed or another was chosen).
    pub fn passes(&mut self) -> usize {
        self.sync();
        self.programs.len()
    }

    fn release(&mut self) {
        if let Some(api) = shader_api() {
            for p in self.programs.drain(..) {
                unsafe { (api.DeleteProgram)(p.id) };
            }
        }
        self.programs.clear();
        self.compiled.clear();
    }

    fn sync(&mut self) {
        let Some(api) = shader_api() else {
            self.programs.clear();
            return;
        };
        if self.wanted.is_empty() {
            if self.attempted.is_some() || !self.programs.is_empty() {
                self.release();
                self.attempted = None;
                self.error.clear();
            }
            return;
        }
        let reg = crate::block::reg();
        let key = reg as *const crate::block::Registry as usize;
        if matches!(&self.attempted, Some((k, id)) if *k == key && *id == self.wanted) {
            return;
        }
        self.attempted = Some((key, self.wanted.clone()));
        let Some(def) = reg.shaders.find(&self.wanted) else {
            self.release();
            self.error = String::from("NOT LOADED");
            crate::sys::log(&alloc::format!("voxelrust: shader {} is not loaded (is its mod switched on?)", self.wanted));
            return;
        };
        // The same text as before (the registry was only reloaded): keep what is compiled.
        if !self.programs.is_empty() && self.compiled == def.passes {
            return;
        }
        self.release();
        self.error.clear();
        for (n, text) in def.passes.iter().enumerate() {
            match gl_shader::program(api, source::VERTEX, &source::fragment(text)) {
                Ok(id) => self.programs.push(Prog { id, loc: Self::locate(api, id), user: Vec::new() }),
                Err(e) => {
                    crate::sys::log(&alloc::format!("voxelrust: shader {} (pass {}) does not compile, so it is off: {}", def.id, n + 1, e));
                    self.release();
                    self.error = String::from("FAILED");
                    return;
                }
            }
        }
        self.compiled = def.passes.clone();
        crate::sys::log(&alloc::format!("voxelrust: shader {} is on ({} pass{})", def.id, self.programs.len(), if self.programs.len() == 1 { "" } else { "es" }));
    }

    fn locate(api: &ShaderApi, id: u32) -> Locs {
        let u = |name: &str| gl_shader::uniform(api, id, name);
        Locs {
            scene: u("u_scene"),
            map: u("u_map"),
            time: u("u_time"),
            resolution: u("u_resolution"),
            sun: u("u_sun"),
            daylight: u("u_daylight"),
            underwater: u("u_underwater"),
            rain: u("u_rain"),
            fov: u("u_fov"),
        }
    }

    /// Draw the chain. The 3D view (`scene` pixels, in the window's bottom
    /// left corner) has been copied into texture `tex`, a square of `size`
    /// pixels; the result goes over the whole `window`.
    pub fn run(&mut self, tex: u32, size: i32, scene: (i32, i32), window: (i32, i32)) {
        let Some(api) = shader_api() else { return };
        let steps = plan::plan(self.programs.len(), scene, window);
        if steps.is_empty() {
            return;
        }
        let g = gl();
        if self.quad == 0 {
            let strip: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0];
            unsafe {
                (g.GenBuffers)(1, &mut self.quad);
                (g.BindBuffer)(gl::ARRAY_BUFFER, self.quad);
                (g.BufferData)(gl::ARRAY_BUFFER, 32, strip.as_ptr() as *const c_void, gl::STATIC_DRAW);
            }
        }
        let map = plan::uv_map(scene.0, scene.1, size);
        let f = self.frame;
        unsafe {
            (g.ActiveTexture)(gl::TEXTURE0);
            (g.BindTexture)(gl::TEXTURE_2D, tex);
            (g.Disable)(gl::BLEND);
            // The fixed-function arrays must not compete for attribute 0.
            (g.DisableClientState)(gl::VERTEX_ARRAY);
            (g.DisableClientState)(gl::COLOR_ARRAY);
            (g.DisableClientState)(gl::TEXTURE_COORD_ARRAY);
            (g.BindBuffer)(gl::ARRAY_BUFFER, self.quad);
            (api.EnableVertexAttribArray)(0);
            (api.VertexAttribPointer)(0, 2, gl::FLOAT, 0, 0, core::ptr::null());
        }
        for step in &steps {
            let prog = &mut self.programs[step.index];
            unsafe {
                (api.UseProgram)(prog.id);
                let l = &prog.loc;
                if l.scene >= 0 {
                    (api.Uniform1i)(l.scene, 0);
                }
                if l.map >= 0 {
                    (api.Uniform4f)(l.map, map[0], map[1], map[2], map[3]);
                }
                if l.time >= 0 {
                    (api.Uniform1f)(l.time, f.time);
                }
                if l.resolution >= 0 {
                    (api.Uniform2f)(l.resolution, step.resolution.0, step.resolution.1);
                }
                if l.sun >= 0 {
                    (api.Uniform3f)(l.sun, f.sun[0], f.sun[1], f.sun[2]);
                }
                if l.daylight >= 0 {
                    (api.Uniform1f)(l.daylight, f.daylight);
                }
                if l.underwater >= 0 {
                    (api.Uniform1f)(l.underwater, if f.underwater { 1.0 } else { 0.0 });
                }
                if l.rain >= 0 {
                    (api.Uniform1f)(l.rain, f.rain);
                }
                if l.fov >= 0 {
                    (api.Uniform1f)(l.fov, f.fov);
                }
            }
            for (name, value) in &self.user.list {
                let at = match prog.user.iter().find(|e| e.0 == *name) {
                    Some(e) => e.1,
                    None => {
                        let loc = gl_shader::uniform(api, prog.id, &names::glsl_name(name));
                        prog.user.push((name.clone(), loc));
                        loc
                    }
                };
                if at >= 0 {
                    unsafe { (api.Uniform1f)(at, *value) };
                }
            }
            unsafe {
                (g.Viewport)(0, 0, step.viewport.0, step.viewport.1);
                (g.DrawArrays)(gl_shader::TRIANGLE_STRIP, 0, 4);
                if step.copy_back {
                    (g.BindTexture)(gl::TEXTURE_2D, tex);
                    (g.CopyTexSubImage2D)(gl::TEXTURE_2D, 0, 0, 0, 0, 0, scene.0, scene.1);
                }
            }
        }
        unsafe {
            (api.UseProgram)(0);
            (api.DisableVertexAttribArray)(0);
            (g.BindBuffer)(gl::ARRAY_BUFFER, 0);
            (g.EnableClientState)(gl::VERTEX_ARRAY);
            (g.EnableClientState)(gl::COLOR_ARRAY);
            (g.EnableClientState)(gl::TEXTURE_COORD_ARRAY);
            (g.Viewport)(0, 0, window.0, window.1);
        }
    }
}
