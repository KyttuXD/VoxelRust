//! Painting what mods put on the HUD: the frame's drawing commands, the
//! lines in the slots, toasts and the action bar. The placing, clipping and
//! layout are in `hudcmd.rs` and `hud.rs`; this only hands the pieces to the
//! renderer.

use crate::block::{self, reg, Kind};
use crate::client::modui::hud::{self, Geometry};
use crate::client::modui::hudcmd::{self, Frame, Prim};
use crate::client::modui::with_ui;
use crate::render::{Renderer, WHITE};
use alloc::string::String;
use alloc::vec::Vec;

/// Most different item or block ids looked up in one frame.
const MAX_LOOKUPS: usize = 48;

/// Draw the mods' part of the HUD for this frame. `w`, `h` are the window,
/// `s` the interface scale, `geo` where the game's own hotbar is.
pub fn draw(r: &mut Renderer, w: f32, h: f32, s: f32, geo: &Geometry) {
    sync_pictures(r);
    if with_ui(|u| u.hud.is_empty()).unwrap_or(true) {
        return;
    }
    let prims: Vec<Prim> = with_ui(|u| {
        let measure = |t: &str, sc: f32| Renderer::text_width(t, sc);
        let f = Frame { w, h, s, measure: &measure };
        let mut p = hudcmd::place_all(&u.hud.buf.cmds, &f);
        p.extend(hud::layout(&u.hud, geo, &f));
        p
    })
    .unwrap_or_default();
    let mut items: Vec<(String, Option<usize>)> = Vec::new();
    for p in prims {
        match p {
            Prim::Rect { x, y, w, h, col } => r.rect(x, y, w, h, col),
            Prim::Text { x, y, scale, text, col } => r.label(x, y, scale, &text, col),
            Prim::Item { x, y, size, id } => {
                if let Some(i) = item_index(&mut items, &id) {
                    crate::client::app::draw_item(r, x, y, size, i, WHITE);
                }
            }
            Prim::Block { x, y, size, id } => draw_block(r, x, y, size, &id, &mut items),
            Prim::Image { x, y, w, h, uv, col, img } => {
                if let Some(tex) = with_ui(|u| u.images.tex_of(img)).flatten() {
                    r.image_tinted(tex, x, y, w, h, uv, col);
                }
            }
        }
    }
}

/// Send the pictures mods asked for to the card (once each), and give back
/// those of mods that were switched off.
fn sync_pictures(r: &mut Renderer) {
    let dead = with_ui(|u| {
        u.images.upload(&mut |w, h, px| r.add_picture(w, h, px));
        u.images.take_dead()
    })
    .unwrap_or_default();
    if !dead.is_empty() {
        r.delete_textures(&dead);
    }
}

/// The index of an item id, remembered for the rest of the frame.
fn item_index(cache: &mut Vec<(String, Option<usize>)>, id: &str) -> Option<usize> {
    if let Some(c) = cache.iter().find(|c| c.0 == id) {
        return c.1;
    }
    let found = reg().items.iter().position(|it| it.id == id);
    if cache.len() < MAX_LOOKUPS {
        cache.push((String::from(id), found));
    }
    found
}

fn draw_block(r: &mut Renderer, x: f32, y: f32, size: f32, id: &str, cache: &mut Vec<(String, Option<usize>)>) {
    let Some(b) = reg().block(id) else { return };
    let d = block::def(b);
    match d.kind {
        Kind::Cube | Kind::Cutout | Kind::Translucent if d.first != block::AIR => r.icon_block(x, y, size, d.tex, &[], WHITE),
        Kind::Boxes => {
            let mut shape = Vec::new();
            block::shape(d.first, 0, &mut shape);
            r.icon_block(x, y, size, d.tex, &shape, WHITE);
        }
        // Anything else is shown by the item that places it, if there is one.
        _ => {
            let key = alloc::format!("#{}", id);
            let found = match cache.iter().find(|c| c.0 == key) {
                Some(c) => c.1,
                None => {
                    let f = reg().items.iter().position(|it| it.place == Some(b));
                    if cache.len() < MAX_LOOKUPS {
                        cache.push((key, f));
                    }
                    f
                }
            };
            if let Some(i) = found {
                crate::client::app::draw_item(r, x, y, size, i, WHITE);
            }
        }
    }
}
