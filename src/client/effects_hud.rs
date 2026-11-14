//! The HUD of status effects: the hearts (green when poisoned, dark when
//! withered, blue when frozen, with a row of yellow ones above for
//! absorption and a flame beside them when on fire) and the list of active
//! effects, with their icons and the time left, at the top right.
//!
//! The pictures are the tiles `heart_*` and `effect_*` (painted by
//! content/art/hud.rs, or the texture pack's own).

use crate::block::reg;
use crate::game::effects::apply;
use crate::game::effects::{clock, roman, Nature};
use crate::render::{Color, Renderer};
use crate::session::{Session, MAX_HEALTH};
use alloc::string::String;

const WHITE: Color = [255, 255, 255, 255];

/// How far apart hearts are, in pixels of the picture (they are 9 wide, so
/// their outlines share a column).
const STEP: f32 = 8.0;

/// Draw the hearts with their left edge at `x0` and their top at `y`.
/// Returns false when the content has no heart pictures (the caller then
/// draws its own).
pub fn hearts(r: &mut Renderer, g: &Session, x0: f32, y: f32, s: f32) -> bool {
    let t = &reg().fx.hud;
    let style = apply::heart_style(&g.fx, &apply::lookup).index();
    let (Some(empty), Some(full), Some(half)) = (t.empty, t.full[style], t.half[style]) else { return false };
    // The heart is 9 x 8 inside a 16 x 16 tile, from (3, 4).
    let (tile, ox, oy) = (16.0 * s, 3.0 * s, 4.0 * s);
    for i in 0..MAX_HEALTH / 2 {
        let picture = match (g.health - i * 2).clamp(0, 2) {
            2 => full,
            1 => half,
            _ => empty,
        };
        r.icon(x0 + i as f32 * STEP * s - ox, y - oy, tile, picture, WHITE);
    }
    // Absorption: yellow hearts in a row above, a point to a half.
    if let (true, Some(af), Some(ah)) = (g.fx.absorb > 0, t.absorb_full, t.absorb_half) {
        let n = ((g.fx.absorb + 1) / 2).min(MAX_HEALTH / 2);
        for i in 0..n {
            let picture = if g.fx.absorb - i * 2 >= 2 { af } else { ah };
            r.icon(x0 + i as f32 * STEP * s - ox, y - oy - 9.0 * s, tile, picture, WHITE);
        }
    }
    // On fire: a flame after the hearts.
    if let (true, Some(flame)) = (apply::on_fire(&g.fx, &apply::lookup), t.flame) {
        let wob = if (g.clock * 6.0) as i32 % 2 == 0 { 0.0 } else { s };
        r.icon(x0 + (MAX_HEALTH / 2) as f32 * STEP * s + 2.0 * s - 3.0 * s, y - oy - wob, tile, flame, WHITE);
    }
    true
}

/// The list of active effects, one row each, right-aligned with its right
/// edge `4` pixels in from the screen's, starting `top` pixels down.
pub fn list(r: &mut Renderer, g: &Session, w: f32, top: f32, s: f32) {
    let table = &reg().fx;
    let row = 18.0 * s;
    for (k, a) in g.fx.list.iter().take(8).enumerate() {
        let Some(d) = table.get(&a.id) else { continue };
        let y = top + k as f32 * row;
        let icon_x = w - 4.0 * s - 16.0 * s;
        // About to run out: it blinks.
        let dim = a.secs < 10.0 && (g.clock * 4.0) as i32 % 2 == 0;
        let alpha = if dim { 120 } else { 255 };
        match d.icon {
            Some(tile) => r.icon(icon_x, y, 16.0 * s, tile, [255, 255, 255, alpha]),
            None => r.rect(icon_x + 2.0 * s, y + 2.0 * s, 12.0 * s, 12.0 * s, [d.colour[0], d.colour[1], d.colour[2], alpha]),
        }
        let name: String = if a.level > 1 { alloc::format!("{} {}", d.name, roman(a.level)) } else { d.name.clone() };
        let text = alloc::format!("{}  {}", name.to_ascii_uppercase(), clock(a.secs));
        let tw = Renderer::text_width(&text, s);
        let (tx, ty) = (icon_x - 3.0 * s - tw, y + 4.0 * s);
        r.rect(tx - 2.0 * s, ty - s, tw + 4.0 * s, 10.0 * s, [0, 0, 0, 110]);
        let tone = match d.nature {
            Nature::Good => [200, 255, 200],
            Nature::Bad => [255, 200, 190],
            Nature::Neutral => [235, 235, 240],
        };
        r.label(tx, ty, s, &text, [tone[0], tone[1], tone[2], alpha]);
    }
}
