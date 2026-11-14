//! The events packs can answer with `when <event> [what] do ...`, and the
//! small helpers the game calls to raise the newer ones, so that a call
//! site in the session stays a one-liner.
//!
//! [`REGISTRY`] is the one list of event names: the docs table in
//! docs/MODDING.md is written from it, and the tests keep it honest. To add
//! an event, add a row here, raise it with `Session::fire` (or a helper
//! below), and add the row to the docs table. A mod's native code can watch
//! every event through `Native::on_event` (see `game/hooks.rs`) without
//! needing a hook of its own.

use crate::block::reg;
use crate::game::{hooks, session::{Controls, Session}};
use alloc::string::String;

// ---- names (the ones the session raises through these helpers)
pub const ENTITY_SPAWN: &str = "entityspawn";
pub const ENTITY_DEATH: &str = "entitydeath";
pub const ENTITY_HURT: &str = "entityhurt";
pub const PLAYER_HURT: &str = "playerhurt";
pub const RESPAWN: &str = "respawn";
pub const CHAT: &str = "chat";
pub const COMMAND: &str = "command";
pub const ITEM_PICKUP: &str = "itempickup";
pub const ITEM_DROP: &str = "itemdrop";
pub const CRAFT: &str = "craft";
pub const SLEEP: &str = "sleep";
pub const WAKE: &str = "wake";
pub const WEATHER: &str = "weather";
pub const JOIN: &str = "join";
pub const LEAVE: &str = "leave";

/// One event, as the docs describe it.
pub struct EventDoc {
    pub name: &'static str,
    /// When it happens.
    pub when: &'static str,
    /// What the `what` word is (`none` when there is none).
    pub what: &'static str,
    /// The native hook that can cancel it before it happens ("" if none).
    pub cancel: &'static str,
}

const fn ev(name: &'static str, when: &'static str, what: &'static str, cancel: &'static str) -> EventDoc {
    EventDoc { name, when, what, cancel }
}

/// Every event the game raises. Another feature that raises events of its
/// own (explosions, status effects) adds its rows here.
pub const REGISTRY: &[EventDoc] = &[
    ev("start", "a world is opened", "none", ""),
    ev("every", "that many seconds have passed, over and over", "the seconds", ""),
    ev("break", "a block is broken by the player", "block id", "on_block_break"),
    ev("place", "a block is placed by the player", "block id", "on_block_place"),
    ev("use", "a block with a `use` script is right clicked", "block id", ""),
    ev("useitem", "an item with a script is right clicked", "item id", "on_item_use"),
    ev("eat", "food is eaten from the hand", "item id", ""),
    ev("hit", "the player hits a creature", "entity id", ""),
    ev("kill", "the player kills a creature", "entity id", ""),
    ev("tame", "a creature is tamed", "entity id", ""),
    ev("ride", "the player gets on a creature", "entity id", ""),
    ev("dimension", "the player arrives in a dimension", "dimension name", ""),
    ev("night", "night falls", "none", ""),
    ev("day", "day breaks", "none", ""),
    ev("death", "the player dies", "none", ""),
    ev("episode", "an episode begins", "episode id", ""),
    ev("episode_end", "an episode ends", "episode id", ""),
    ev(ENTITY_SPAWN, "a creature appears, by itself, from an egg or from breeding", "entity id", "on_entity_spawn"),
    ev(ENTITY_DEATH, "a creature dies, whatever the cause", "entity id", ""),
    ev(ENTITY_HURT, "the player strikes a creature (it may die of it)", "entity id", ""),
    ev(PLAYER_HURT, "the player takes damage in survival", "the cause", "on_player_hurt"),
    ev(RESPAWN, "the player comes back after dying", "none", ""),
    ev(CHAT, "the player says something (a line not starting with /)", "its first word", "on_chat"),
    ev(COMMAND, "the player types a /command", "the command's name, without the /", "on_chat"),
    ev(ITEM_PICKUP, "the player picks an item up off the ground", "item id", ""),
    ev(ITEM_DROP, "the player drops an item", "item id", ""),
    ev(CRAFT, "the player crafts something at a table or oven", "the output item id", ""),
    ev(SLEEP, "the player lies down to sleep", "none", ""),
    ev(WAKE, "the player wakes", "none", ""),
    ev(WEATHER, "the weather where the player is changes", "clear, rain, snow or storm", ""),
    ev(JOIN, "another player joins a shared game", "their name", ""),
    ev(LEAVE, "another player leaves a shared game", "their name", ""),
    // explosions: see TNT (added by the explosion feature).
    // effects: see status effects (added by that feature).
];

/// Other names for the same event, as (alias, canonical name). A pack may
/// write either; the game keeps the canonical one.
pub const ALIASES: &[(&str, &str)] = &[
    ("blockbreak", "break"),
    ("blockplace", "place"),
    ("playerdeath", "death"),
    ("nightfall", "night"),
    ("daybreak", "day"),
    ("itemuse", "useitem"),
];

/// The name an event goes by inside the game.
pub fn canonical(name: &str) -> &str {
    ALIASES.iter().find(|a| a.0 == name).map_or(name, |a| a.1)
}

/// Whether the game itself raises an event of this name (packs may raise
/// others with `/fire`).
pub fn is_known(name: &str) -> bool {
    let name = canonical(name);
    REGISTRY.iter().any(|e| e.name == name)
}

// ---- weather

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Wet {
    #[default]
    Clear,
    Rain,
    Snow,
    Storm,
}

impl Wet {
    pub fn name(self) -> &'static str {
        match self {
            Wet::Clear => "clear",
            Wet::Rain => "rain",
            Wet::Snow => "snow",
            Wet::Storm => "storm",
        }
    }
}

/// The weather after one more look at the sky. Rain starts above 0.25 and
/// stops below 0.15, so a sky hovering at the edge does not flicker.
pub fn next_wet(prev: Wet, rain: f32, snow: bool, storm: bool) -> Wet {
    let wet = if prev == Wet::Clear { rain > 0.25 } else { rain > 0.15 };
    if !wet {
        Wet::Clear
    } else if storm {
        Wet::Storm
    } else if snow {
        Wet::Snow
    } else {
        Wet::Rain
    }
}

// ---- the use button

/// Remembers, while the use button is held, whether a mod cancelled the
/// press, so the hook is asked once per press and not once per frame.
#[derive(Clone, Copy, Default, Debug)]
pub struct UseLatch {
    down: bool,
    blocked: bool,
}

impl UseLatch {
    /// Whether this frame is a new press (the hook should be asked).
    pub fn is_press(&self, pressed: bool) -> bool {
        pressed && !self.down
    }

    /// Take this frame's state; `verdict` is the hook's answer on a press
    /// (`None` when it was not asked). Returns whether the use goes ahead.
    pub fn settle(&mut self, pressed: bool, verdict: Option<bool>) -> bool {
        if !pressed {
            self.down = false;
            self.blocked = false;
            return true;
        }
        if !self.down {
            self.down = true;
            self.blocked = verdict == Some(false);
        }
        !self.blocked
    }
}

// ---- what the session keeps for all this

/// Bookkeeping for the events above; one field of `Session`.
#[derive(Default)]
pub struct Tracker {
    pub use_latch: UseLatch,
    /// Whether any switched-on mod has a hook of the newer kinds (looked at
    /// once a frame, so that the common case costs one `bool` test).
    pub hooks_live: bool,
    /// A hook is running: the events it raises are not passed back to it.
    pub in_event: bool,
    /// Why the next `Session::damage` happens (see `Session::damage_from`).
    pub cause: String,
    /// The cause for damage held over in `pending_hurt`.
    pub pending_cause: String,
    was_sleeping: bool,
    wet: Wet,
}

// ---- helpers called from the session and the client

/// Once a frame, from `Session::update`: creatures that appeared or died,
/// sleeping, the weather, and the mods' per-frame hook.
pub fn poll(g: &mut Session, dt: f32) {
    g.ev.hooks_live = hooks::any_active();
    let (born, died) = (core::mem::take(&mut g.mobs.born), core::mem::take(&mut g.mobs.died));
    // Only whoever runs the world knows when creatures come and go.
    if g.runs_the_world() {
        let kinds = reg().entities.len();
        for (id, kind, _) in born.into_iter().filter(|b| b.1 < kinds) {
            if !hooks::entity_spawn(g, kind) {
                g.mobs.list.retain(|m| m.id != id);
                continue;
            }
            let name = reg().entities[kind].id.clone();
            g.fire(ENTITY_SPAWN, &name);
        }
        for (_, kind, at) in died.into_iter().filter(|d| d.1 < kinds) {
            hooks::entity_death(g, kind, at);
            let name = reg().entities[kind].id.clone();
            g.fire(ENTITY_DEATH, &name);
        }
    }
    if g.sleeping != g.ev.was_sleeping {
        g.ev.was_sleeping = g.sleeping;
        g.fire(if g.sleeping { SLEEP } else { WAKE }, "");
    }
    let wet = next_wet(g.ev.wet, g.sky.rain, g.sky.snow, g.sky.storm);
    if wet != g.ev.wet {
        g.ev.wet = wet;
        g.fire(WEATHER, wet.name());
    }
    hooks::tick(g, dt);
}

/// The player hit a creature of this kind.
pub fn entity_hurt(g: &mut Session, kind: usize) {
    if let Some(d) = reg().entities.get(kind) {
        g.fire(ENTITY_HURT, &d.id);
    }
}

/// The player typed this line. Returns whether to go on with it (a mod's
/// `on_chat` may swallow it). Scripts hear `chat` or `command` first.
pub fn chat_line(g: &mut Session, text: &str) -> bool {
    let line = text.trim();
    let first = line.split_whitespace().next().unwrap_or("");
    if let Some(name) = first.strip_prefix('/') {
        g.fire(COMMAND, &name.to_ascii_lowercase());
    } else if !first.is_empty() {
        g.fire(CHAT, &first.to_ascii_lowercase());
    }
    hooks::chat(g, line)
}

/// Item `item` (an index into the registry's items) left the player's hand.
pub fn item_dropped(g: &mut Session, item: usize) {
    if let Some(d) = reg().items.get(item) {
        g.fire(ITEM_DROP, &d.id);
    }
}

/// The player picked item `item` up.
pub fn item_picked(g: &mut Session, item: usize) {
    if let Some(d) = reg().items.get(item) {
        g.fire(ITEM_PICKUP, &d.id);
    }
}

/// The player crafted item `out`.
pub fn crafted(g: &mut Session, out: usize) {
    hooks::craft(g, out);
    if let Some(d) = reg().items.get(out) {
        g.fire(CRAFT, &d.id);
    }
}

/// The use button, for `Session::use_items`: asks a mod's `on_item_use`
/// when a press begins. Returns the controls to go on with: `None` to use
/// them as they are, or a copy without the use button when a mod said no.
pub fn gate_use(g: &mut Session, c: &Controls, held: Option<usize>) -> Option<Controls> {
    let verdict = match held {
        Some(item) if g.ev.use_latch.is_press(c.place_held) => Some(hooks::item_use(g, item)),
        _ => None,
    };
    if g.ev.use_latch.settle(c.place_held, verdict) {
        None
    } else {
        Some(Controls { place_held: false, ..*c })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_names_are_unique_lowercase_words() {
        for (i, e) in REGISTRY.iter().enumerate() {
            assert!(!e.name.is_empty() && e.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "bad event name {}", e.name);
            assert!(REGISTRY[i + 1..].iter().all(|o| o.name != e.name), "{} is listed twice", e.name);
            assert!(!e.when.is_empty() && !e.what.is_empty(), "{} lacks its description", e.name);
        }
    }

    #[test]
    fn aliases_point_at_registered_events_and_clash_with_none() {
        for (alias, name) in ALIASES {
            assert!(REGISTRY.iter().any(|e| e.name == *name), "{} points at unknown {}", alias, name);
            assert!(REGISTRY.iter().all(|e| e.name != *alias), "{} is both an alias and an event", alias);
        }
        assert_eq!(canonical("blockbreak"), "break");
        assert_eq!(canonical("nightfall"), "night");
        assert_eq!(canonical("break"), "break");
        assert_eq!(canonical("my_own_event"), "my_own_event");
        assert!(is_known("playerdeath") && is_known("chat") && !is_known("my_own_event"));
    }

    #[test]
    fn the_names_the_helpers_raise_are_registered() {
        for n in [ENTITY_SPAWN, ENTITY_DEATH, ENTITY_HURT, PLAYER_HURT, RESPAWN, CHAT, COMMAND, ITEM_PICKUP, ITEM_DROP, CRAFT, SLEEP, WAKE, WEATHER, JOIN, LEAVE] {
            assert!(REGISTRY.iter().any(|e| e.name == n), "{} is not in the registry", n);
        }
    }

    #[test]
    fn events_with_a_cancelling_hook_name_a_real_one() {
        for e in REGISTRY.iter().filter(|e| !e.cancel.is_empty()) {
            assert!(e.cancel.starts_with("on_"), "{}", e.cancel);
        }
    }

    #[test]
    fn weather_does_not_flicker_at_the_edge() {
        assert_eq!(next_wet(Wet::Clear, 0.2, false, false), Wet::Clear);
        assert_eq!(next_wet(Wet::Clear, 0.3, false, false), Wet::Rain);
        // Once raining, a dip to 0.2 is still rain; 0.1 is over.
        assert_eq!(next_wet(Wet::Rain, 0.2, false, false), Wet::Rain);
        assert_eq!(next_wet(Wet::Rain, 0.1, false, false), Wet::Clear);
        assert_eq!(next_wet(Wet::Rain, 0.9, true, false), Wet::Snow);
        assert_eq!(next_wet(Wet::Rain, 0.95, false, true), Wet::Storm);
    }

    #[test]
    fn a_cancelled_press_stays_cancelled_until_the_button_is_let_go() {
        let mut l = UseLatch::default();
        assert!(l.is_press(true));
        assert!(!l.settle(true, Some(false)));
        // Held on: not a press again, and still refused.
        assert!(!l.is_press(true));
        assert!(!l.settle(true, None));
        // Let go, and the next press is asked afresh.
        assert!(l.settle(false, None));
        assert!(l.is_press(true));
        assert!(l.settle(true, Some(true)));
        assert!(l.settle(true, None));
    }

    #[test]
    fn a_press_with_nothing_held_is_never_refused() {
        let mut l = UseLatch::default();
        assert!(l.settle(true, None));
        assert!(l.settle(true, None));
    }
}
