//! What other players' health looks like in a shared game: small hearts under
//! their name tag (within [`HEARTS_RANGE`] blocks), the list of players with
//! health and ping (held with Tab), and the armour they wear on their models.
//! The numbers come from `net/sync`.

use crate::block::reg;
use crate::game::gear::hearts;
use crate::inventory::{Slot, Stack};
use crate::net::sync::vitals::{Vitals, SLOTS};
use crate::render::{Color, Renderer};
use crate::session::Session;
use alloc::string::String;
use alloc::vec::Vec;

/// Hearts are shown under a name tag of a player this near (blocks).
pub const HEARTS_RANGE: f32 = 24.0;
/// Most rows the list shows.
const MAX_ROWS: usize = 24;

const WHITE: Color = [255, 255, 255, 255];

/// Small hearts for `hp` out of `max` (half hearts), centred on `cx` with the top at `y`:
/// ten to a row, more rows below for more health. `k` is the size of a heart picture's pixel.
/// Drawn from the same pictures as the player's own hearts, or flat red pips if the content has none.
pub fn small_hearts(r: &mut Renderer, cx: f32, y: f32, hp: i32, max: i32, k: f32) {
    let t = &reg().fx.hud;
    let pictures = match (t.empty, t.full[0], t.half[0]) {
        (Some(e), Some(f), Some(h)) => Some((e, f, h)),
        _ => None,
    };
    let step = 8.0 * k;
    let left = cx - hearts::PER_ROW as f32 * step * 0.5;
    for c in hearts::layout(hp, max) {
        let (x, yy) = (left + c.col as f32 * step, y + c.row as f32 * 9.0 * k);
        match pictures {
            // The heart is 9 x 8 inside a 16 x 16 tile, from (3, 4).
            Some((e, f, h)) => r.icon(x - 3.0 * k, yy - 4.0 * k, 16.0 * k, [e, h, f][c.fill.min(2) as usize], WHITE),
            None => {
                let col: Color = if c.fill == 2 { [226, 44, 52, 255] } else if c.fill == 1 { [150, 40, 46, 255] } else { [40, 16, 18, 210] };
                r.rect(x, yy, 7.0 * k, 6.0 * k, col);
            }
        }
    }
}

/// The armour a player wears, as the slots `render::armor::looks` takes: the item numbers
/// (plus one) from their health message; one the content does not have is not worn.
pub fn worn_slots(ids: &[u16; SLOTS]) -> [Slot; SLOTS] {
    let items = reg().items.len();
    let mut out: [Slot; SLOTS] = [None; SLOTS];
    for (o, &id) in out.iter_mut().zip(ids.iter()) {
        if id > 0 && (id as usize) <= items {
            *o = Some(Stack::new(id - 1, 1));
        }
    }
    out
}

/// One line of the player list.
struct Row {
    name: String,
    vitals: Option<Vitals>,
    ping: u16,
}

/// The list of players (name, health bar, health, ping), centred near the top. `own` is
/// the player's own name.
pub fn tab_list(r: &mut Renderer, g: &Session, own: &str, w: f32, s: f32) {
    let Some(net) = g.net.as_ref() else { return };
    let mine = g.vitals_now();
    let mut rows: Vec<Row> = alloc::vec![Row { name: String::from(own), vitals: Some(mine), ping: net.own_ping }];
    let mut others: Vec<Row> = net.players.values().map(|p| Row { name: p.name.clone(), vitals: p.vitals, ping: p.vitals.map_or(0, |v| v.ping) }).collect();
    others.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
    rows.extend(others);
    let more = rows.len().saturating_sub(MAX_ROWS);
    rows.truncate(MAX_ROWS);
    let (row_h, panel_w) = (11.0 * s, 190.0 * s);
    let x0 = (w - panel_w) * 0.5;
    let y0 = 14.0 * s;
    let height = (rows.len() as f32 + 1.0) * row_h + 6.0 * s + if more > 0 { row_h } else { 0.0 };
    r.rect(x0, y0, panel_w, height, [8, 10, 16, 190]);
    r.label(x0 + 4.0 * s, y0 + 3.0 * s, s, &alloc::format!("PLAYERS ({})", rows.len() + more), [250, 240, 170, 255]);
    for (i, row) in rows.iter().enumerate() {
        let y = y0 + 3.0 * s + (i as f32 + 1.0) * row_h;
        r.label(x0 + 4.0 * s, y, s, &row.name, WHITE);
        if let Some(v) = row.vitals {
            // A bar of how much health of the most, and the numbers.
            let bar = 40.0 * s;
            let bx = x0 + 96.0 * s;
            r.rect(bx, y + s, bar, 6.0 * s, [40, 16, 18, 230]);
            let frac = if v.max == 0 { 0.0 } else { (v.hp as f32 / v.max as f32).clamp(0.0, 1.0) };
            r.rect(bx, y + s, bar * frac, 6.0 * s, if v.hp == 0 { [90, 90, 90, 255] } else { [226, 44, 52, 255] });
            let text = if v.hp == 0 { String::from("DEAD") } else { alloc::format!("{}/{}", v.hp, v.max) };
            r.label(bx + bar + 3.0 * s, y, s, &text, WHITE);
        }
        if row.ping > 0 {
            let col: Color = if row.ping < 100 { [120, 230, 120, 255] } else if row.ping < 250 { [240, 220, 90, 255] } else { [240, 100, 90, 255] };
            let text = alloc::format!("{}MS", row.ping);
            r.label(x0 + panel_w - Renderer::text_width(&text, s) - 4.0 * s, y, s, &text, col);
        }
    }
    if more > 0 {
        let y = y0 + 3.0 * s + (rows.len() as f32 + 1.0) * row_h;
        r.label(x0 + 4.0 * s, y, s, &alloc::format!("... AND {} MORE", more), WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worn_ids_that_the_content_does_not_have_are_not_worn() {
        let n = reg().items.len() as u16;
        let slots = worn_slots(&[0, 1, n, n + 1]);
        assert!(slots[0].is_none());
        assert_eq!(slots[1].map(|s| s.item), Some(0));
        assert_eq!(slots[2].map(|s| s.item), Some(n - 1));
        assert!(slots[3].is_none());
    }
}
