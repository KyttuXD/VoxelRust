//! Names: shader ids, and the uniforms a script may set (`set_shader_uniform`).

use alloc::string::String;
use alloc::vec::Vec;

/// Longest shader id.
pub const MAX_ID: usize = 32;
/// Longest name of a uniform a script sets (without the `u_`).
pub const MAX_NAME: usize = 24;
/// Most different uniforms scripts may set at once.
pub const MAX_USER: usize = 16;
/// Values scripts set are kept between minus and plus this.
pub const LIMIT: f32 = 1000.0;

/// What the game itself provides (as `u_<name>` in the shader, apart from
/// `scene()`): scripts cannot set these.
pub const BUILTIN: &[&str] = &["time", "resolution", "sun", "daylight", "underwater", "rain", "fov", "scene", "map"];

/// A shader's id as a pack writes it: letters, digits, `_` and `-`, 1 to 32 long.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A uniform name a script may set: a lower-case letter, then lower-case
/// letters, digits and `_`, up to 24 long, and not one the game provides.
pub fn valid_user_name(name: &str) -> bool {
    let mut chars = name.chars();
    let first_ok = chars.next().map_or(false, |c| c.is_ascii_lowercase());
    first_ok && name.len() <= MAX_NAME && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') && !BUILTIN.contains(&name)
}

/// The uniform's name in the shader.
pub fn glsl_name(name: &str) -> String {
    alloc::format!("u_{}", name)
}

/// The values scripts have set: `(name, value)`, at most [`MAX_USER`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UserUniforms {
    pub list: Vec<(String, f32)>,
}

impl UserUniforms {
    /// Set `name` to `value` (kept within +-[`LIMIT`]). `false`, and nothing
    /// changes, for a bad name, a value that is not a finite number, or too many names.
    pub fn set(&mut self, name: &str, value: f64) -> bool {
        if !valid_user_name(name) || !value.is_finite() {
            return false;
        }
        let v = if value > LIMIT as f64 {
            LIMIT
        } else if value < -(LIMIT as f64) {
            -LIMIT
        } else {
            value as f32
        };
        match self.list.iter_mut().find(|e| e.0 == name) {
            Some(e) => e.1 = v,
            None => {
                if self.list.len() >= MAX_USER {
                    return false;
                }
                self.list.push((String::from(name), v));
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_plain_words() {
        assert!(valid_id("soft_bloom") && valid_id("a-b_9") && valid_id("X"));
        assert!(!valid_id("") && !valid_id("has space") && !valid_id("a/b") && !valid_id("dot.dot"));
        assert!(!valid_id(&"a".repeat(33)) && valid_id(&"a".repeat(32)));
    }

    #[test]
    fn uniform_names_are_checked() {
        assert!(valid_user_name("glow") && valid_user_name("a1_b2") && valid_user_name(&"a".repeat(24)));
        assert!(!valid_user_name("") && !valid_user_name("Glow") && !valid_user_name("1glow") && !valid_user_name("_glow"));
        assert!(!valid_user_name("g low") && !valid_user_name("glow;") && !valid_user_name("u_glow ") && !valid_user_name(&"a".repeat(25)));
        for b in BUILTIN {
            assert!(!valid_user_name(b), "{} is the game's", b);
        }
        assert_eq!(glsl_name("glow"), "u_glow");
    }

    #[test]
    fn values_are_bounded_and_the_list_is_short() {
        let mut u = UserUniforms::default();
        assert!(u.set("glow", 0.5));
        assert!(u.set("glow", 2.0));
        assert_eq!(u.list, [(String::from("glow"), 2.0)]);
        assert!(u.set("big", 1e30) && u.set("small", -1e30));
        assert_eq!((u.list[1].1, u.list[2].1), (LIMIT, -LIMIT));
        assert!(!u.set("nan", f64::NAN) && !u.set("inf", f64::INFINITY) && !u.set("Bad", 1.0) && !u.set("time", 1.0));
        for i in 0..40 {
            u.set(&alloc::format!("n{}", i), 1.0);
        }
        assert_eq!(u.list.len(), MAX_USER);
        assert!(!u.set("one_more", 1.0));
        assert!(u.set("glow", 3.0), "a name already there can still change");
    }
}
