//! The HUD of status effects: the hearts (green when poisoned, dark when
//! withered, blue when frozen, with a row of yellow ones above for
//! absorption and a flame beside them when on fire) and the list of active
//! effects, with their icons and the time left, at the top right.
//!
//! The air meter of someone under water is here too: a row of bubbles
//! above the hearts, which pop one a second (see game/effects/air.rs).
//!
//! The pictures are the tiles `heart_*`, `effect_*` and `hud_bubble*`
//! (painted by content/art/hud.rs and content/art/status.rs, or the texture
//! pack's own).

use crate::block::reg;
use crate::game::effects::apply;
use crate::game::effects::{clock, roman, Nature};
use crate::render::{Color, Renderer};
use crate::game::gear::hearts;
use crate::session::Session;
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
    // Ten hearts to a row, more health in rows above (game/gear/hearts.rs).
    let rows = hearts::rows(g.max_health());
    for c in hearts::layout(g.health, g.max_health()) {
        let picture = match c.fill {
            2 => full,
            1 => half,
            _ => empty,
        };
        r.icon(x0 + c.col as f32 * STEP * s - ox, y - oy - c.row as f32 * 9.0 * s, tile, picture, WHITE);
    }
    // Absorption: yellow hearts in a row above all of them, a point to a half.
    if let (true, Some(af), Some(ah)) = (g.fx.absorb > 0, t.absorb_full, t.absorb_half) {
        for c in hearts::absorption(g.fx.absorb, rows) {
            let picture = if c.fill == 2 { af } else { ah };
            r.icon(x0 + c.col as f32 * STEP * s - ox, y - oy - c.row as f32 * 9.0 * s, tile, picture, WHITE);
        }
    }
    // On fire: a flame after the hearts.
    if let (true, Some(flame)) = (apply::on_fire(&g.fx, &apply::lookup), t.flame) {
        let wob = if (g.clock * 6.0) as i32 % 2 == 0 { 0.0 } else { s };
        r.icon(x0 + hearts::PER_ROW as f32 * STEP * s + 2.0 * s - 3.0 * s, y - oy - wob, tile, flame, WHITE);
    }
    true
}

/// The air meter: a bubble for each unit of air left, from the right edge
/// `right` going left, with the top at `y`. Shown only when some air is
/// missing; with none left, the last place blinks as a popped bubble.
pub fn bubbles(r: &mut Renderer, g: &Session, right: f32, y: f32, s: f32) {
    use crate::game::effects::air;
    if !air::shown(g.air) {
        return;
    }
    let n = air::bubbles(g.air);
    let t = &reg().fx.hud;
    match (t.bubble, t.bubble_pop) {
        (Some(full), Some(pop)) => {
            // The picture is 9 x 9 from (3, 3) of its tile.
            let (tile, off) = (16.0 * s, 3.0 * s);
            for i in 0..air::MAX_AIR as i32 {
                let x = right - 9.0 * s - i as f32 * STEP * s;
                if i < n {
                    r.icon(x - off, y - off, tile, full, WHITE);
                } else if i == n && n < 10 && g.air <= 0.0 && (g.clock * 4.0) as i32 % 2 == 0 {
                    r.icon(x - off, y - off, tile, pop, WHITE);
                }
            }
        }
        _ => {
            for i in 0..n.clamp(0, 10) {
                let x = right - (i + 1) as f32 * 9.0 * s;
                r.rect(x + s, y + s, 6.0 * s, 6.0 * s, [120, 190, 255, 230]);
                r.rect(x + 2.0 * s, y + 2.0 * s, 2.0 * s, 2.0 * s, [230, 245, 255, 255]);
            }
        }
    }
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
