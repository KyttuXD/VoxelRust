//! Example of native event hooks. Copy `examples/hooks` to `mods/hooksdemo`
//! to build it into the game (see pack.txt next to it).
//!
//! It uses three hooks:
//! - `on_block_break`: Vault Blocks cannot be broken in survival.
//! - `on_player_hurt`: falls hurt half as much.
//! - `on_draw_hud`: shows the player's health as a number.
//!
//! Hooks are plain functions the game calls at fixed points (the table is
//! `native::Native`). Every one is optional; fill in what you need and
//! leave the rest to `..Native::NONE`.

use crate::block::{self, Block};
use crate::game::hooks::HudSink;
use crate::native::{Native, API_VERSION};
use crate::session::{GameMode, Session};
use alloc::format;

/// Must match the `feature` line in pack.txt.
const NAME: &str = "hooksdemo";

/// The player is about to break `b` at `cell`. Returning false keeps it.
fn on_block_break(g: &mut Session, _cell: [i32; 3], b: Block) -> bool {
    let vault = block::def(b).id == "vault_block";
    if vault && g.mode == GameMode::Survival {
        g.say("THE VAULT BLOCK WILL NOT BREAK");
        return false;
    }
    true
}

/// Damage is coming. `amount` may be changed; false (or 0) cancels it.
fn on_player_hurt(_g: &mut Session, amount: &mut i32, cause: &str) -> bool {
    if cause == "fall" {
        *amount = (*amount + 1) / 2;
    }
    true
}

/// Draw on the heads-up display. `hud` takes pixel positions; multiply
/// sizes by `hud.scale()` so they follow the window size.
fn on_draw_hud(g: &Session, hud: &mut dyn HudSink) {
    let s = hud.scale();
    let text = format!("HP {}", g.health);
    let w = hud.text_width(&text, 1.0);
    hud.rect(4.0 * s, 4.0 * s, w + 6.0 * s, 10.0 * s, [0, 0, 0, 120]);
    hud.text(7.0 * s, 6.0 * s, 1.0, &text, [255, 220, 220, 255]);
}

pub static NATIVE: Native = Native {
    name: NAME,
    api: API_VERSION,
    on_block_break: Some(on_block_break),
    on_player_hurt: Some(on_player_hurt),
    on_draw_hud: Some(on_draw_hud),
    ..Native::NONE
};
