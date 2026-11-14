//! Opening the game's own screens from a script: the crafting grid and the
//! inventory, for a mod that wants them on a key or an item without placing
//! a bench. The call only asks; the client opens the screen on the next frame,
//! and only while the player is in the game (not in a menu, the chat or
//! another screen), so a script cannot pile screens up or open one over a
//! menu.

use super::args::{int, want};
use super::data::data;
use super::Ctx;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("open_crafting(size: i64)", "open the crafting screen with a grid of 3, 5 or 7 squares a side (the table, the bench, the workshop) as if a bench had been used; any other size is an error. The screen opens next frame, only if the player is in the game and no other screen is open"),
    ("open_inventory()", "open the player's pack screen (the creative item list in creative mode), under the same conditions"),
];

/// A screen a script asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenRequest {
    /// The crafting grid with this side (3, 5 or 7).
    Craft(usize),
    Inventory,
}

/// Whether a grid side is one the game has.
pub fn valid_side(size: i64) -> bool {
    matches!(size, 3 | 5 | 7)
}

/// What the client should open now, if a script asked (and forget it).
pub fn take_request(g: &mut Session) -> Option<ScreenRequest> {
    // Nothing is made just to find out there is no request.
    if g.ext.iter().all(|e| e.0 != super::KEY) {
        return None;
    }
    data(g).screen.take()
}

pub fn call(g: &mut Session, _ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    match name {
        "open_crafting" => {
            want(name, args, 1)?;
            let size = int(name, args, 0)?;
            if !valid_side(size) {
                return Err(format!("`open_crafting`: the size must be 3, 5 or 7, not {}", size));
            }
            data(g).screen = Some(ScreenRequest::Craft(size as usize));
            Ok(Value::Unit)
        }
        "open_inventory" => {
            want(name, args, 0)?;
            data(g).screen = Some(ScreenRequest::Inventory);
            Ok(Value::Unit)
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_three_grids_of_the_game_are_valid() {
        for ok in [3, 5, 7] {
            assert!(valid_side(ok));
        }
        for bad in [-3, 0, 1, 2, 4, 6, 8, 9, 100, i64::MAX] {
            assert!(!valid_side(bad), "{}", bad);
        }
    }

    #[test]
    fn the_sides_match_what_a_block_opens() {
        use crate::block::Usable;
        assert_eq!(Usable::Craft.craft_side(), Some(3));
        assert_eq!(Usable::Craft5.craft_side(), Some(5));
        assert_eq!(Usable::Craft7.craft_side(), Some(7));
    }
}
