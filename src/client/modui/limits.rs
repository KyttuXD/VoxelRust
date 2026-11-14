//! Every bound on what a mod may ask of the interface, in one place.
//! Scripts and native code go through the same checks, so a mod that
//! asks for too much is cut off at these numbers and never blocks a frame.

use alloc::string::String;

/// Drawing commands kept for one frame, all mods together.
pub const MAX_HUD_CMDS: usize = 512;
/// ...and from any one mod (so one mod cannot use up the others' share).
pub const MAX_HUD_CMDS_PER_MOD: usize = 256;
/// Longest text on the HUD, in a toast, or on a screen.
pub const MAX_TEXT: usize = 120;
/// Longest id (a key binding, a HUD element, a widget).
pub const MAX_ID: usize = 32;
/// Longest label of a key binding, a switch or a widget.
pub const MAX_LABEL: usize = 40;
/// Longest id of an item or block named in a drawing command.
pub const MAX_ITEM_ID: usize = 48;

/// Key bindings: per mod, and all together.
pub const MAX_BINDS_PER_MOD: usize = 64;
pub const MAX_BINDS: usize = 256;
/// Rows a mod may add to the mod-switches page: per mod, and all together.
pub const MAX_SWITCHES_PER_MOD: usize = 16;
pub const MAX_SWITCHES: usize = 64;

/// Retained HUD elements (the named slots): per mod, and all together.
pub const MAX_ELEMENTS_PER_MOD: usize = 16;
pub const MAX_ELEMENTS: usize = 64;
/// Toasts waiting or showing.
pub const MAX_TOASTS: usize = 8;
/// How long a toast or the action bar may stay, in seconds.
pub const MAX_SECS: f32 = 30.0;

/// Widgets on a mod's screen, and entries of one list.
pub const MAX_WIDGETS: usize = 48;
pub const MAX_LIST_ITEMS: usize = 128;
/// Events from a mod's screen waiting to be handed to the mods.
pub const MAX_UI_EVENTS: usize = 64;

/// Text made safe to show: printable ASCII only (the font has no more),
/// upper case, at most `max` characters.
pub fn clean_text(s: &str, max: usize) -> String {
    s.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()).map(|c| c.to_ascii_uppercase()).take(max).collect()
}

/// An id: letters, digits, `_` and `-`, at most [`MAX_ID`] long, not empty.
pub fn clean_id(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || t.len() > MAX_ID || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return None;
    }
    Some(String::from(t))
}

/// A mod's name made safe to put in a settings line (no `=`, spaces or dots).
pub fn clean_mod(s: &str) -> String {
    let t: String = s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).take(40).collect();
    if t.is_empty() {
        String::from("_")
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_cut_upper_cased_and_stripped() {
        assert_eq!(clean_text("héllo\u{7} wörld", 100), "HLLO WRLD");
        assert_eq!(clean_text("abcdef", 3), "ABC");
    }

    #[test]
    fn ids_are_plain_and_short() {
        assert_eq!(clean_id(" a-b_1 ").as_deref(), Some("a-b_1"));
        assert_eq!(clean_id(""), None);
        assert_eq!(clean_id("a b"), None);
        assert_eq!(clean_id("a.b"), None);
        assert_eq!(clean_id(&"x".repeat(MAX_ID + 1)), None);
        assert_eq!(clean_mod("my mod.v2=x"), "my_mod_v2_x");
    }
}
