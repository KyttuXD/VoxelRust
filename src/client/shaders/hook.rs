//! What the app tells the renderer about shaders once a frame.

use crate::client::render::Renderer;
use crate::client::shaders::names::UserUniforms;

/// Hand the renderer the shader the settings name and the numbers scripts
/// have set (none outside a world). Changes only when something differs.
pub fn sync(r: &mut Renderer, wanted: &str, user: Option<&UserUniforms>) {
    if r.post.wanted != wanted {
        r.post.wanted = alloc::string::String::from(wanted);
    }
    match user {
        Some(u) => {
            if r.post.user != *u {
                r.post.user = u.clone();
            }
        }
        None => {
            if !r.post.user.list.is_empty() {
                r.post.user.list.clear();
            }
        }
    }
}
