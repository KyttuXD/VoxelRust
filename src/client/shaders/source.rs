//! The shader text the game adds around a mod's fragment shader.
//!
//! Mods write only the body of a GLSL ES 1.00 fragment shader (the dialect
//! WebGL 1 speaks, which desktop OpenGL 2.1 and the legacy Mac understand as
//! well): no `#version`, no `precision` line, no declaration of the uniforms
//! below. The game puts this header in front, so the same text compiles
//! everywhere, and line numbers in an error are the mod file's own.

use alloc::string::String;

/// The vertex shader: one full-screen strip. `v_uv` is 0..1 from the
/// bottom left of the picture.
pub const VERTEX: &str = "attribute vec2 a_pos;\nvarying vec2 v_uv;\nvoid main() {\n  v_uv = a_pos * 0.5 + 0.5;\n  gl_Position = vec4(a_pos, 0.0, 1.0);\n}\n";

/// What comes before a mod's fragment shader.
pub const HEADER: &str = "#ifdef GL_ES\n#ifdef GL_FRAGMENT_PRECISION_HIGH\nprecision highp float;\n#else\nprecision mediump float;\n#endif\n#endif\n\
varying vec2 v_uv;\n\
uniform sampler2D u_scene;\n\
uniform vec4 u_map;\n\
uniform float u_time;\n\
uniform vec2 u_resolution;\n\
uniform vec3 u_sun;\n\
uniform float u_daylight;\n\
uniform float u_underwater;\n\
uniform float u_rain;\n\
uniform float u_fov;\n\
vec4 scene(vec2 uv) {\n  return texture2D(u_scene, clamp(uv, 0.0, 1.0) * u_map.xy + u_map.zw);\n}\n\
#line 1\n";

/// Largest fragment shader file, in bytes.
pub const MAX_SOURCE: usize = 16 * 1024;

/// Whether `text` can be a fragment shader body; the reason if not.
pub fn check(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err(String::from("the shader file is empty"));
    }
    if text.len() > MAX_SOURCE {
        return Err(alloc::format!("the shader file is bigger than {} KB", MAX_SOURCE / 1024));
    }
    if text.contains('\0') {
        return Err(String::from("the shader file is not text"));
    }
    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with("#version") {
            return Err(String::from("leave out `#version`: the game adds the header (docs/SHADERS.md)"));
        }
        if t.starts_with("#extension") {
            return Err(String::from("`#extension` is not allowed: use only plain GLSL ES 1.00"));
        }
    }
    Ok(())
}

/// The whole fragment shader for a mod's text.
pub fn fragment(body: &str) -> String {
    let mut s = String::with_capacity(HEADER.len() + body.len() + 1);
    s.push_str(HEADER);
    s.push_str(body);
    if !body.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_body_is_accepted_and_wrapped() {
        let body = "void main() { gl_FragColor = scene(v_uv); }";
        assert!(check(body).is_ok());
        let f = fragment(body);
        assert!(f.starts_with("#ifdef GL_ES"));
        assert!(f.ends_with("gl_FragColor = scene(v_uv); }\n"));
        assert!(f.contains("#line 1\nvoid main()"), "the body starts at line 1");
    }

    #[test]
    fn what_the_header_owns_is_refused_in_the_body() {
        assert!(check("#version 100\nvoid main(){}").unwrap_err().contains("#version"));
        assert!(check("  #extension GL_OES_standard_derivatives : enable\nvoid main(){}").is_err());
        assert!(check("   \n").is_err());
        assert!(check("void main(){}\0").is_err());
        assert!(check(&"x".repeat(MAX_SOURCE + 1)).is_err());
    }

    #[test]
    fn the_header_declares_every_documented_uniform() {
        for n in ["u_time", "u_resolution", "u_sun", "u_daylight", "u_underwater", "u_rain", "u_fov", "u_scene"] {
            assert!(HEADER.contains(n), "{}", n);
        }
        for b in crate::client::shaders::names::BUILTIN {
            assert!(HEADER.contains(&alloc::format!("u_{}", b)) || *b == "scene", "{}", b);
        }
    }
}
