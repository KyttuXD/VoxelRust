//! OpenGL 1.5 for a browser: every call the game makes is imported from
//! JavaScript, where `web/voxelrust.js` carries it out with WebGL (a shader
//! standing in for the fixed-function pipeline).
//!
//! The list below must match `gl_api!` in `sys/gl.rs` name for name and
//! argument for argument: the game finds these through
//! `SDL_GL_GetProcAddress`, like any driver's, and calls them through
//! pointers of the types declared there.

use core::ffi::c_void;

macro_rules! imports {
    ($(fn $name:ident($($a:ident: $t:ty),*) $(-> $r:ty)?;)*) => {
        #[link(wasm_import_module = "env")]
        extern "C" {
            $(fn $name($($a: $t),*) $(-> $r)?;)*
            /// Copy the text `glGetString(what)` would return to `dst`;
            /// returns its length.
            fn js_gl_string(what: u32, dst: *mut u8, cap: usize) -> usize;
        }

        /// The function called `name` (no closing NUL), or null.
        pub fn sym(name: &[u8]) -> *mut c_void {
            $(if name == stringify!($name).as_bytes() {
                return $name as *const () as *mut c_void;
            })*
            if name == b"glGetString" {
                return glGetString as *const () as *mut c_void;
            }
            core::ptr::null_mut()
        }
    };
}

imports! {
    fn glClear(mask: u32);
    fn glClearColor(r: f32, g: f32, b: f32, a: f32);
    fn glEnable(cap: u32);
    fn glDisable(cap: u32);
    fn glViewport(x: i32, y: i32, w: i32, h: i32);
    fn glMatrixMode(mode: u32);
    fn glLoadMatrixf(m: *const f32);
    fn glLoadIdentity();
    fn glGenTextures(n: i32, ids: *mut u32);
    fn glDeleteTextures(n: i32, ids: *const u32);
    fn glBindTexture(target: u32, id: u32);
    fn glTexImage2D(target: u32, level: i32, internal: i32, w: i32, h: i32, border: i32, format: u32, kind: u32, px: *const c_void);
    fn glTexParameteri(target: u32, name: u32, value: i32);
    fn glTexSubImage2D(target: u32, level: i32, x: i32, y: i32, w: i32, h: i32, format: u32, kind: u32, px: *const c_void);
    fn glCopyTexSubImage2D(target: u32, level: i32, x: i32, y: i32, sx: i32, sy: i32, w: i32, h: i32);
    fn glEnableClientState(array: u32);
    fn glDisableClientState(array: u32);
    fn glVertexPointer(size: i32, kind: u32, stride: i32, p: *const c_void);
    fn glColorPointer(size: i32, kind: u32, stride: i32, p: *const c_void);
    fn glTexCoordPointer(size: i32, kind: u32, stride: i32, p: *const c_void);
    fn glDrawArrays(mode: u32, first: i32, count: i32);
    fn glFogf(name: u32, value: f32);
    fn glFogfv(name: u32, values: *const f32);
    fn glFogi(name: u32, value: i32);
    fn glBlendFunc(src: u32, dst: u32);
    fn glDepthFunc(func: u32);
    fn glDepthMask(on: u8);
    fn glDepthRange(near: f64, far: f64);
    fn glAlphaFunc(func: u32, reference: f32);
    fn glCullFace(face: u32);
    fn glReadPixels(x: i32, y: i32, w: i32, h: i32, format: u32, kind: u32, px: *mut c_void);
    fn glPixelStorei(name: u32, value: i32);
    fn glLineWidth(width: f32);
    fn glPointSize(size: f32);
    fn glHint(target: u32, mode: u32);
    fn glFinish();
    fn glGetIntegerv(name: u32, out: *mut i32);
    fn glGenBuffers(n: i32, ids: *mut u32);
    fn glBindBuffer(target: u32, id: u32);
    fn glBufferData(target: u32, size: isize, data: *const c_void, usage: u32);
    fn glDeleteBuffers(n: i32, ids: *const u32);
    fn glActiveTexture(unit: u32);
    fn glClientActiveTexture(unit: u32);
    // The shader calls of sys/gl_shader.rs (post-processing only).
    fn glCreateShader(kind: u32) -> u32;
    fn glShaderSource(shader: u32, count: i32, strings: *const *const u8, lengths: *const i32);
    fn glCompileShader(shader: u32);
    fn glGetShaderiv(shader: u32, name: u32, out: *mut i32);
    fn glGetShaderInfoLog(shader: u32, cap: i32, len: *mut i32, dst: *mut u8);
    fn glDeleteShader(shader: u32);
    fn glCreateProgram() -> u32;
    fn glAttachShader(program: u32, shader: u32);
    fn glBindAttribLocation(program: u32, index: u32, name: *const u8);
    fn glLinkProgram(program: u32);
    fn glGetProgramiv(program: u32, name: u32, out: *mut i32);
    fn glGetProgramInfoLog(program: u32, cap: i32, len: *mut i32, dst: *mut u8);
    fn glUseProgram(program: u32);
    fn glDeleteProgram(program: u32);
    fn glGetUniformLocation(program: u32, name: *const u8) -> i32;
    fn glUniform1i(location: i32, v: i32);
    fn glUniform1f(location: i32, v: f32);
    fn glUniform2f(location: i32, a: f32, b: f32);
    fn glUniform3f(location: i32, a: f32, b: f32, c: f32);
    fn glUniform4f(location: i32, a: f32, b: f32, c: f32, d: f32);
    fn glEnableVertexAttribArray(index: u32);
    fn glDisableVertexAttribArray(index: u32);
    fn glVertexAttribPointer(index: u32, size: i32, kind: u32, normalized: u8, stride: i32, p: *const c_void);
}

/// The one call that returns memory: the text is fetched into a buffer
/// here, which stays valid until the next call (the game copies it at once).
#[allow(non_snake_case)]
unsafe extern "C" fn glGetString(what: u32) -> *const u8 {
    static mut TEXT: [u8; 256] = [0; 256];
    let text = core::ptr::addr_of_mut!(TEXT) as *mut u8;
    let n = js_gl_string(what, text, 255).min(255);
    *text.add(n) = 0;
    text
}
