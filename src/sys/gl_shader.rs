//! The few OpenGL 2.0 calls that post-processing shaders need (see
//! docs/SHADERS.md).
//!
//! The rest of the game is fixed function (`gl.rs`), and a graphics card
//! from before 2004 has none of these: so they are looked up apart, and a
//! card without them only means "no shaders", never that the game cannot
//! start. In the browser the same names are carried out by `web/voxelrust.js`.

use super::sdl;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_void};
use core::sync::atomic::{AtomicPtr, Ordering};

pub const FRAGMENT_SHADER: u32 = 0x8B30;
pub const VERTEX_SHADER: u32 = 0x8B31;
pub const COMPILE_STATUS: u32 = 0x8B81;
pub const LINK_STATUS: u32 = 0x8B82;
pub const INFO_LOG_LENGTH: u32 = 0x8B84;
pub const TRIANGLE_STRIP: u32 = 5;

macro_rules! shader_api {
    ($(fn $name:ident($($t:ty),*) $(-> $r:ty)?;)*) => {
        #[allow(non_snake_case)]
        pub struct ShaderApi { $(pub $name: unsafe extern "system" fn($($t),*) $(-> $r)?,)* }
        impl ShaderApi {
            /// Every function, or the name of the first one the driver lacks.
            unsafe fn load() -> Result<ShaderApi, &'static str> {
                Ok(ShaderApi {
                    $($name: {
                        let get = sdl::sdl().SDL_GL_GetProcAddress;
                        let p = get(concat!("gl", stringify!($name), "\0").as_ptr() as *const c_char);
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

shader_api! {
    fn CreateShader(u32) -> u32;
    fn ShaderSource(u32, i32, *const *const c_char, *const i32);
    fn CompileShader(u32);
    fn GetShaderiv(u32, u32, *mut i32);
    fn GetShaderInfoLog(u32, i32, *mut i32, *mut u8);
    fn DeleteShader(u32);
    fn CreateProgram() -> u32;
    fn AttachShader(u32, u32);
    fn BindAttribLocation(u32, u32, *const c_char);
    fn LinkProgram(u32);
    fn GetProgramiv(u32, u32, *mut i32);
    fn GetProgramInfoLog(u32, i32, *mut i32, *mut u8);
    fn UseProgram(u32);
    fn DeleteProgram(u32);
    fn GetUniformLocation(u32, *const c_char) -> i32;
    fn Uniform1i(i32, i32);
    fn Uniform1f(i32, f32);
    fn Uniform2f(i32, f32, f32);
    fn Uniform3f(i32, f32, f32, f32);
    fn Uniform4f(i32, f32, f32, f32, f32);
    fn EnableVertexAttribArray(u32);
    fn DisableVertexAttribArray(u32);
    fn VertexAttribPointer(u32, i32, u32, u8, i32, *const c_void);
}

static API: AtomicPtr<ShaderApi> = AtomicPtr::new(core::ptr::null_mut());

/// The shader functions, if this driver has them.
pub fn shader_api() -> Option<&'static ShaderApi> {
    let p = API.load(Ordering::Acquire);
    if p.is_null() {
        None
    } else {
        Some(unsafe { &*p })
    }
}

/// Look the functions up (with a current OpenGL context). Not having them is
/// not an error: the game logs it and goes without shaders.
pub fn load() {
    match unsafe { ShaderApi::load() } {
        Ok(api) => API.store(Box::into_raw(Box::new(api)), Ordering::Release),
        Err(missing) => super::log(&alloc::format!("voxelrust: no shaders on this graphics driver (it has no {})", missing)),
    }
}

fn c_string(text: &str) -> Vec<u8> {
    let mut v: Vec<u8> = text.bytes().filter(|b| *b != 0).collect();
    v.push(0);
    v
}

fn log_text(buf: &[u8], len: i32) -> String {
    let n = (len.max(0) as usize).min(buf.len());
    String::from_utf8_lossy(&buf[..n]).trim().chars().take(400).collect()
}

/// Compile one shader; the driver's complaint if it does not.
pub fn compile(api: &ShaderApi, kind: u32, source: &str) -> Result<u32, String> {
    unsafe {
        let sh = (api.CreateShader)(kind);
        if sh == 0 {
            return Err(String::from("the driver would not make a shader"));
        }
        let ptr = source.as_ptr() as *const c_char;
        let len = source.len() as i32;
        (api.ShaderSource)(sh, 1, &ptr, &len);
        (api.CompileShader)(sh);
        let mut ok = 0i32;
        (api.GetShaderiv)(sh, COMPILE_STATUS, &mut ok);
        if ok != 0 {
            return Ok(sh);
        }
        let mut buf = [0u8; 512];
        let mut n = 0i32;
        (api.GetShaderInfoLog)(sh, buf.len() as i32, &mut n, buf.as_mut_ptr());
        (api.DeleteShader)(sh);
        Err(log_text(&buf, n))
    }
}

/// Compile and link a vertex and a fragment shader into a program whose
/// attribute `a_pos` is number 0.
pub fn program(api: &ShaderApi, vertex: &str, fragment: &str) -> Result<u32, String> {
    let vs = compile(api, VERTEX_SHADER, vertex)?;
    let fs = match compile(api, FRAGMENT_SHADER, fragment) {
        Ok(f) => f,
        Err(e) => {
            unsafe { (api.DeleteShader)(vs) };
            return Err(e);
        }
    };
    unsafe {
        let p = (api.CreateProgram)();
        (api.AttachShader)(p, vs);
        (api.AttachShader)(p, fs);
        (api.BindAttribLocation)(p, 0, b"a_pos\0".as_ptr() as *const c_char);
        (api.LinkProgram)(p);
        // Flagged for deletion with the program.
        (api.DeleteShader)(vs);
        (api.DeleteShader)(fs);
        let mut ok = 0i32;
        (api.GetProgramiv)(p, LINK_STATUS, &mut ok);
        if ok != 0 {
            return Ok(p);
        }
        let mut buf = [0u8; 512];
        let mut n = 0i32;
        (api.GetProgramInfoLog)(p, buf.len() as i32, &mut n, buf.as_mut_ptr());
        (api.DeleteProgram)(p);
        Err(log_text(&buf, n))
    }
}

/// Where uniform `name` is in `program` (-1: not there, or not used).
pub fn uniform(api: &ShaderApi, program: u32, name: &str) -> i32 {
    let c = c_string(name);
    unsafe { (api.GetUniformLocation)(program, c.as_ptr() as *const c_char) }
}
