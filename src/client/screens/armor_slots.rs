//! The four armour slots beside the player's picture in the inventory
//! screen (head, chest, legs, feet), and what a click on one does:
//!
//! - an empty hand on a worn piece takes it off, into the pack (it is set
//!   down at the feet if the pack is full);
//! - an armour piece in the hand, for that slot, is put on, and what was
//!   worn goes to the hand (or to the pack when the hand held several).
//!
//! Shift-click or right click on armour in the pack puts it on (see
//! `screens/inventory.rs`). The rules are in `game/gear/armor.rs`.

use crate::block::reg;
use crate::client::app::draw_slot;
use crate::client::ui::{self, Ui};
use crate::game::gear::armor;
use crate::session::Session;
use alloc::string::String;

/// The letters shown in an empty slot.
const LETTERS: [&str; armor::SLOTS] = ["H", "C", "L", "F"];

/// Draw the four slots down from (`x`, `y`), `cell` pixels each, and act on
/// a left click (`click_l`) or right click. Returns a line for the tooltip.
pub fn draw(ui: &mut Ui, g: &mut Session, x: f32, y: f32, cell: f32, click_l: bool, click_r: bool) -> Option<String> {
    let s = ui.s;
    let mut tip = None;
    for slot in 0..armor::SLOTS {
        let sy = y + slot as f32 * cell;
        let hot = ui.hot(x, sy, cell, cell);
        draw_slot(ui.r, x, sy, cell, s, g.inv.worn[slot], hot);
        if g.inv.worn[slot].is_none() {
            ui.text(x + cell * 0.5 - 2.0 * s, sy + cell * 0.5 - 3.0 * s, LETTERS[slot], ui::DIM);
        }
        if !hot {
            continue;
        }
        tip = Some(match g.inv.worn[slot].and_then(|w| reg().items.get(w.item as usize)) {
            Some(it) => alloc::format!("{} ({})", it.name, armor::NAMES[slot]),
            None => String::from(armor::NAMES[slot]),
        });
        if click_l || click_r {
            click(g, slot);
            ui.clicked_any = true;
        }
    }
    // What all of it adds up to.
    let (defense, _) = g.armor_points();
    let line = alloc::format!("ARMOUR {}", defense as i32);
    ui.text(x, y + armor::SLOTS as f32 * cell + 3.0 * s, &line, ui::DIM);
    tip
}

/// A click on worn slot `slot` with the mouse's hand (`g.cursor`).
pub fn click(g: &mut Session, slot: usize) {
    match g.cursor {
        None => {
            g.take_off_armor(slot);
        }
        Some(c) => {
            // Only armour made for this slot goes in it.
            if armor::piece(c.item).map_or(true, |p| p.slot as usize != slot) {
                return;
            }
            let old = g.inv.worn[slot].take();
            g.inv.worn[slot] = Some(c.with_count(1));
            match (c.count, old) {
                (1, old) => g.cursor = old,
                (n, old) => {
                    g.cursor = Some(c.with_count(n - 1));
                    if let Some(o) = old {
                        g.give_stack_or_drop(o);
                    }
                }
            }
            crate::script::gear_bridge::armor_changed(g, slot);
        }
    }
}
