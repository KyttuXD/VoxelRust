//! OpenGL 1.5, fixed function, looked up through SDL.
//!
//! No shaders and no framebuffer objects: this is the feature set of a
//! 2003-era graphics card, which every OpenGL driver since still offers
//! (on macOS through the legacy 2.1 context, elsewhere through the
//! compatibility profile). Buffer objects and multitexture fall back to
//! their `ARB` names for drivers that predate 1.5.

use super::sdl;
use alloc::boxed::Box;
use alloc::string::String;
use core::ffi::{c_char, c_void};
use core::sync::atomic::{AtomicPtr, Ordering};

pub const COLOR_BUFFER_BIT: u32 = 0x4000;
pub const DEPTH_BUFFER_BIT: u32 = 0x100;
pub const POINTS: u32 = 0;
pub const LINES: u32 = 1;
pub const QUADS: u32 = 7;
pub const DEPTH_TEST: u32 = 0x0B71;
pub const CULL_FACE: u32 = 0x0B44;
pub const BLEND: u32 = 0x0BE2;
pub const ALPHA_TEST: u32 = 0x0BC0;
pub const FOG: u32 = 0x0B60;
pub const TEXTURE_2D: u32 = 0x0DE1;
pub const MODELVIEW: u32 = 0x1700;
pub const PROJECTION: u32 = 0x1701;
pub const TEXTURE: u32 = 0x1702;
pub const RGB: u32 = 0x1907;
pub const RGBA: u32 = 0x1908;
pub const UNSIGNED_BYTE: u32 = 0x1401;
pub const SHORT: u32 = 0x1402;
pub const FLOAT: u32 = 0x1406;
pub const NEAREST: i32 = 0x2600;
pub const LINEAR: i32 = 0x2601;
pub const NEAREST_MIPMAP_LINEAR: i32 = 0x2702;
pub const MAX_TEXTURE_SIZE: u32 = 0x0D33;
pub const TEXTURE_MAG_FILTER: u32 = 0x2800;
pub const TEXTURE_MIN_FILTER: u32 = 0x2801;
pub const TEXTURE_WRAP_S: u32 = 0x2802;
pub const TEXTURE_WRAP_T: u32 = 0x2803;
pub const REPEAT: i32 = 0x2901;
pub const CLAMP_TO_EDGE: i32 = 0x812F;
pub const TEXTURE_MAX_LEVEL: u32 = 0x813D;
pub const VERTEX_ARRAY: u32 = 0x8074;
pub const COLOR_ARRAY: u32 = 0x8076;
pub const TEXTURE_COORD_ARRAY: u32 = 0x8078;
pub const ARRAY_BUFFER: u32 = 0x8892;
pub const STATIC_DRAW: u32 = 0x88E4;
pub const TEXTURE0: u32 = 0x84C0;
pub const TEXTURE1: u32 = 0x84C1;
pub const FOG_MODE: u32 = 0x0B65;
pub const FOG_START: u32 = 0x0B63;
pub const FOG_END: u32 = 0x0B64;
pub const FOG_COLOR: u32 = 0x0B66;
pub const SRC_ALPHA: u32 = 0x0302;
pub const ONE_MINUS_SRC_ALPHA: u32 = 0x0303;
pub const ONE: u32 = 1;
pub const LEQUAL: u32 = 0x0203;
pub const GREATER: u32 = 0x0204;
pub const BACK: u32 = 0x0405;
pub const VENDOR: u32 = 0x1F00;
pub const RENDERER: u32 = 0x1F01;
pub const VERSION: u32 = 0x1F02;
pub const PACK_ALIGNMENT: u32 = 0x0D05;
pub const UNPACK_ALIGNMENT: u32 = 0x0CF5;
pub const PERSPECTIVE_CORRECTION_HINT: u32 = 0x0C50;
pub const FOG_HINT: u32 = 0x0C54;
pub const FASTEST: u32 = 0x1101;

macro_rules! gl_api {
    ($(fn $name:ident($($t:ty),*) $(-> $r:ty)?;)*) => {
        #[allow(non_snake_case)]
        pub struct Api { $(pub $name: unsafe extern "system" fn($($t),*) $(-> $r)?,)* }
        impl Api {
            unsafe fn load() -> Result<Api, &'static str> {
                Ok(Api {
                    $($name: {
                        let get = sdl::sdl().SDL_GL_GetProcAddress;
                        let mut p = get(concat!("gl", stringify!($name), "\0").as_ptr() as *const c_char);
                        if p.is_null() {
                            p = get(concat!("gl", stringify!($name), "ARB\0").as_ptr() as *const c_char);
                        }
                        if p.is_null() {
                            return Err(concat!("gl", stringify!($name)));
                        }
                        core::mem::transmute::<*mut c_void, unsafe extern "system" fn($($t),*) $(-> $r)?>(p)
                    },)*
                })
            }
        }
    };
}

gl_api! {
    fn Clear(u32);
    fn ClearColor(f32, f32, f32, f32);
    fn Enable(u32);
    fn Disable(u32);
    fn Viewport(i32, i32, i32, i32);
    fn MatrixMode(u32);
    fn LoadMatrixf(*const f32);
    fn LoadIdentity();
    fn GenTextures(i32, *mut u32);
    fn DeleteTextures(i32, *const u32);
    fn BindTexture(u32, u32);
    fn TexImage2D(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
    fn TexParameteri(u32, u32, i32);
    fn TexSubImage2D(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void);
    fn CopyTexSubImage2D(u32, i32, i32, i32, i32, i32, i32, i32);
    fn EnableClientState(u32);
    fn DisableClientState(u32);
    fn VertexPointer(i32, u32, i32, *const c_void);
    fn ColorPointer(i32, u32, i32, *const c_void);
    fn TexCoordPointer(i32, u32, i32, *const c_void);
    fn DrawArrays(u32, i32, i32);
    fn Fogf(u32, f32);
    fn Fogfv(u32, *const f32);
    fn Fogi(u32, i32);
    fn BlendFunc(u32, u32);
    fn DepthFunc(u32);
    fn DepthMask(u8);
    fn DepthRange(f64, f64);
    fn AlphaFunc(u32, f32);
    fn CullFace(u32);
    fn ReadPixels(i32, i32, i32, i32, u32, u32, *mut c_void);
    fn GetString(u32) -> *const u8;
    fn PixelStorei(u32, i32);
    fn LineWidth(f32);
    fn PointSize(f32);
    fn Hint(u32, u32);
    fn Finish();
    fn GetIntegerv(u32, *mut i32);
    fn GenBuffers(i32, *mut u32);
    fn BindBuffer(u32, u32);
    fn BufferData(u32, isize, *const c_void, u32);
    fn DeleteBuffers(i32, *const u32);
    fn ActiveTexture(u32);
    fn ClientActiveTexture(u32);
}

static API: AtomicPtr<Api> = AtomicPtr::new(core::ptr::null_mut());

/// The loaded OpenGL functions. Panics before [`load`] has succeeded.
pub fn gl() -> &'static Api {
    let p = API.load(Ordering::Acquire);
    assert!(!p.is_null(), "OpenGL is not loaded");
    unsafe { &*p }
}

/// Look up every function. Call with a current OpenGL context.
pub fn load() -> Result<(), String> {
    let api = unsafe { Api::load() }.map_err(|missing| {
        alloc::format!(
            "this graphics driver has no `{}`. VoxelRust needs OpenGL 1.5 \
             (or 1.3 with the ARB vertex buffer extension).",
            missing
        )
    })?;
    API.store(Box::into_raw(Box::new(api)), Ordering::Release);
    // Shaders are optional (gl_shader.rs): a card without them still plays.
    super::gl_shader::load();
    Ok(())
}

pub fn get_string(what: u32) -> String {
    unsafe { super::from_cstr((gl().GetString)(what) as *const c_char) }
}
