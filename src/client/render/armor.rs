//! Drawing worn armour over a player model: for each worn slot, a layer of
//! slightly bigger copies of the cubes that slot covers (`ModelDef::armor_quads`),
//! in the armour's own picture (`armor_tex`, laid out like a skin) or in a
//! plain colour (`armor_tint`). Bounded: at most as many quads as the body.

use super::{Renderer, V2};
use crate::sys::gl::{self, gl};
use crate::game::gear::armor;
use crate::inventory::Slot;
use alloc::string::String;

/// How one worn piece looks.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    /// A picture by name (empty: a plain colour).
    pub tex: String,
    /// Multiplies the picture (or is the colour).
    pub tint: [u8; 3],
}

/// The colour of a plain layer for an item with no `armor_tint`: by what its
/// id says it is made of, else a mid grey.
pub fn default_tint(id: &str) -> [u8; 3] {
    const MATERIALS: [(&str, [u8; 3]); 9] = [
        ("leather", [150, 98, 62]),
        ("iron", [200, 205, 215]),
        ("gold", [236, 196, 64]),
        ("copper", [214, 120, 74]),
        ("crystal", [124, 224, 246]),
        ("ruby", [226, 54, 104]),
        ("chain", [150, 154, 164]),
        ("diamond", [96, 226, 232]),
        ("wood", [166, 118, 70]),
    ];
    MATERIALS.iter().find(|m| id.contains(m.0)).map_or([150, 154, 164], |m| m.1)
}

/// The looks of what a player wears, slot by slot.
pub fn looks(worn: &[Slot; armor::SLOTS]) -> [Option<Look>; armor::SLOTS] {
    let items = &crate::block::reg().items;
    let mut out: [Option<Look>; armor::SLOTS] = [None, None, None, None];
    for (i, w) in worn.iter().enumerate() {
        if let Some(it) = w.and_then(|s| items.get(s.item as usize)) {
            out[i] = Some(Look { tex: it.x.armor_tex.clone(), tint: it.x.armor_tint.unwrap_or_else(|| default_tint(&it.id)) });
        }
    }
    out
}

impl Renderer {
    /// Draw the armour over a model just drawn with `draw_model` at the same place and pose.
    pub fn draw_model_armor(&mut self, model: usize, pose: &crate::model::Pose, at: [f32; 3], yaw: f32, height: f32, tint: [u8; 3], worn: &[Option<Look>; armor::SLOTS]) {
        for (slot, w) in worn.iter().enumerate() {
            let Some(w) = w else { continue };
            let tex = if w.tex.is_empty() { 0 } else { self.entity_texture(&w.tex) };
            self.ensure_model(model);
            let Some(def) = self.models.get(model).and_then(|m| m.as_ref()) else { return };
            self.model_verts.clear();
            def.armor_quads(slot, pose, at, yaw, height, false, &mut self.model_verts);
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
            let c = [(tint[0] as u32 * w.tint[0] as u32 / 255) as u8, (tint[1] as u32 * w.tint[1] as u32 / 255) as u8, (tint[2] as u32 * w.tint[2] as u32 / 255) as u8];
            for i in 0..self.model_verts.len() {
                let v = self.model_verts[i];
                let k = v.shade as u32;
                self.batch.push(V2 { pos: v.pos, uv: v.uv, col: [(c[0] as u32 * k / 255) as u8, (c[1] as u32 * k / 255) as u8, (c[2] as u32 * k / 255) as u8, 255] });
            }
            self.flush(gl::QUADS);
        }
    }

    /// As [`Renderer::draw_model_armor`] for a model drawn flat on the screen
    /// (the inventory portrait). Only the faces turned toward the viewer are
    /// drawn, so the back of a layer does not cover the body.
    pub fn draw_model_2d_armor(&mut self, model: usize, pose: &crate::model::Pose, x: f32, y: f32, yaw: f32, height: f32, worn: &[Option<Look>; armor::SLOTS]) {
        for (slot, w) in worn.iter().enumerate() {
            let Some(w) = w else { continue };
            let tex = if w.tex.is_empty() { 0 } else { self.entity_texture(&w.tex) };
            self.ensure_model(model);
            let Some(def) = self.models.get(model).and_then(|m| m.as_ref()) else { return };
            self.model_verts.clear();
            def.armor_quads(slot, pose, [x, y, 0.0], yaw, height, true, &mut self.model_verts);
            let mut quads: alloc::vec::Vec<usize> = (0..self.model_verts.len() / 4).filter(|&q| (0..4).map(|k| self.model_verts[q * 4 + k].pos[2]).sum::<f32>() >= 0.0).collect();
            let depth = |q: usize| (0..4).map(|k| self.model_verts[q * 4 + k].pos[2]).sum::<f32>();
            quads.sort_by(|&a, &b| depth(a).partial_cmp(&depth(b)).unwrap_or(core::cmp::Ordering::Equal));
            self.flush(gl::QUADS);
            self.batch_tex = tex;
            unsafe { (gl().Enable)(gl::ALPHA_TEST) };
            for q in quads {
                for k in 0..4 {
                    let v = self.model_verts[q * 4 + k];
                    let s = v.shade as u16;
                    self.batch.push(V2 { pos: [v.pos[0], v.pos[1], 0.0], uv: v.uv, col: [(s * w.tint[0] as u16 / 255) as u8, (s * w.tint[1] as u16 / 255) as u8, (s * w.tint[2] as u16 / 255) as u8, 255] });
                }
            }
            self.flush(gl::QUADS);
            unsafe { (gl().Disable)(gl::ALPHA_TEST) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plain_colour_follows_the_material_in_the_name() {
        assert_eq!(default_tint("leather_helmet"), [150, 98, 62]);
        assert_eq!(default_tint("iron_boots"), [200, 205, 215]);
        assert_eq!(default_tint("mystery_hat"), [150, 154, 164]);
    }
}
