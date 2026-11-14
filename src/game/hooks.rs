//! Where the game calls the event hooks of a mod's native code
//! (the `on_...` fields of `native::Native`).
//!
//! Every dispatcher here is small: it walks the mods whose code is switched
//! on and calls the hook if the mod gave one. A hook that can cancel
//! something returns `false` for "do not"; the mods are asked in order and
//! the first `false` ends the asking (see [`allow`]). Once a frame
//! `events::poll` looks at whether any mod has such a hook at all
//! (`Session::ev.hooks_live`), and when none has, a dispatcher costs one
//! test of that flag.
//!
//! To add a hook: add the field to `Native` (and to `Native::NONE` and
//! [`has_any`]), add a dispatcher here that tests `hooks_live`, call it from
//! the right place, and add a row to the table in docs/MODDING.md.

use crate::block::Block;
use crate::native::Native;
use crate::render::{Color, Renderer};
use crate::game::session::Session;

/// Ask each hook in turn; the first to say `false` cancels and the rest are
/// not asked. `true` when none objected (or there are no hooks).
pub fn allow<T: Copy>(hooks: impl Iterator<Item = T>, mut ask: impl FnMut(T) -> bool) -> bool {
    for h in hooks {
        if !ask(h) {
            return false;
        }
    }
    true
}

/// Whether a mod has any of the event hooks (so a frame may need them).
pub fn has_any(n: &Native) -> bool {
    n.on_block_break.is_some()
        || n.on_block_place.is_some()
        || n.on_entity_spawn.is_some()
        || n.on_entity_death.is_some()
        || n.chunk_generated.is_some()
        || n.on_block_placed.is_some()
        || n.on_creature_death.is_some()
        || n.on_creature_removed.is_some()
        || n.on_creature_interact.is_some()
        || n.on_creature_sunburn.is_some()
        || n.on_player_hurt.is_some()
        || n.on_chat.is_some()
        || n.on_item_use.is_some()
        || n.on_craft.is_some()
        || n.on_tick.is_some()
        || n.on_event.is_some()
}

/// Whether any switched-on mod has event hooks.
pub fn any_active() -> bool {
    crate::native::active().any(|(_, n)| has_any(n))
}

/// A block is about to be broken at `cell`; `false` keeps it.
pub fn block_break(g: &mut Session, cell: [i32; 3], b: Block) -> bool {
    if !g.ev.hooks_live {
        return true;
    }
    allow(crate::native::active().filter_map(|(_, n)| n.on_block_break), |f| f(g, cell, b)) && crate::script::bridge::block_break(g, cell, b)
}

/// Block `b` is about to be placed at `cell`; `false` stops it.
pub fn block_place(g: &mut Session, cell: [i32; 3], b: Block) -> bool {
    if !g.ev.hooks_live {
        return true;
    }
    allow(crate::native::active().filter_map(|(_, n)| n.on_block_place), |f| f(g, cell, b)) && crate::script::bridge::block_place(g, cell, b)
}

/// A creature of `kind` has appeared; `false` takes it away again.
pub fn entity_spawn(g: &mut Session, kind: usize) -> bool {
    if !g.ev.hooks_live {
        return true;
    }
    allow(crate::native::active().filter_map(|(_, n)| n.on_entity_spawn), |f| f(g, kind)) && crate::script::bridge::entity_spawn(g, kind)
}

/// A creature of `kind` died at `at`.
pub fn entity_death(g: &mut Session, kind: usize, at: [f64; 3]) {
    if !g.ev.hooks_live {
        return;
    }
    for f in crate::native::active().filter_map(|(_, n)| n.on_entity_death) {
        f(g, kind, at);
    }
    crate::script::bridge::entity_death(g, kind, at);
}

/// A chunk was just generated: the mods may place structures in it. At most
/// `structures::MAX_WRITES` block writes are kept per call.
pub fn chunk_generated(g: &mut Session, cx: i32, cz: i32) {
    if !g.ev.hooks_live {
        return;
    }
    let seed = g.world.seed;
    g.ev.block_budget = Some(crate::script::ext::structures::MAX_WRITES);
    // Blocks of world generation are not "placed" for `on_block_placed`.
    let tracking = core::mem::replace(&mut g.world.log.track_placed, false);
    for f in crate::native::active().filter_map(|(_, n)| n.chunk_generated) {
        f(g, cx, cz, seed);
        // Each mod's code has the whole allowance to itself.
        g.ev.block_budget = Some(crate::script::ext::structures::MAX_WRITES);
    }
    crate::script::bridge::chunk_generated(g, cx, cz, seed);
    g.world.log.track_placed = tracking;
    g.ev.block_budget = None;
}

/// A block was placed at `cell` by `cause`.
pub fn block_placed(g: &mut Session, cell: [i32; 3], b: Block, cause: &str) {
    if !g.ev.hooks_live {
        return;
    }
    for f in crate::native::active().filter_map(|(_, n)| n.on_block_placed) {
        f(g, cell, b, cause);
    }
    crate::script::bridge::block_placed(g, cell, b, cause);
}

/// A creature died; `killer` is a player's name, a creature's id or a cause.
pub fn creature_death(g: &mut Session, id: u32, kind: usize, at: [f64; 3], killer: &str) {
    if !g.ev.hooks_live {
        return;
    }
    for f in crate::native::active().filter_map(|(_, n)| n.on_creature_death) {
        f(g, id, kind, at, killer);
    }
    crate::script::bridge::creature_death(g, id, kind, at, killer);
}

/// A creature left the world (`mob_hooks::reason`).
pub fn creature_removed(g: &mut Session, id: u32, kind: usize, reason: &str) {
    if !g.ev.hooks_live {
        return;
    }
    for f in crate::native::active().filter_map(|(_, n)| n.on_creature_removed) {
        f(g, id, kind, reason);
    }
    crate::script::bridge::creature_removed(g, id, kind, reason);
}

/// The player right-clicked a creature; `false` cancels the game's own handling.
pub fn creature_interact(g: &mut Session, id: u32, kind: usize, item: Option<usize>) -> bool {
    if !g.ev.hooks_live {
        return true;
    }
    allow(crate::native::active().filter_map(|(_, n)| n.on_creature_interact), |f| f(g, id, kind, item)) && crate::script::bridge::creature_interact(g, id, kind, item)
}

/// A night creature would burn in daylight; `false` spares it.
pub fn creature_sunburn(g: &mut Session, id: u32, kind: usize) -> bool {
    if !g.ev.hooks_live {
        return true;
    }
    allow(crate::native::active().filter_map(|(_, n)| n.on_creature_sunburn), |f| f(g, id, kind)) && crate::script::bridge::creature_sunburn(g, id, kind)
}

/// The player is about to take `amount` damage from `cause`. The mods may
/// change the amount; returns whether any is left to deal.
pub fn player_hurt(g: &mut Session, amount: &mut i32, cause: &str) -> bool {
    if !g.ev.hooks_live {
        return *amount > 0;
    }
    let ok = allow(crate::native::active().filter_map(|(_, n)| n.on_player_hurt), |f| f(g, amount, cause));
    if ok {
        crate::script::bridge::player_hurt(g, amount, cause);
    }
    ok && *amount > 0
}

/// The player typed `line`; `false` swallows it.
pub fn chat(g: &mut Session, line: &str) -> bool {
    if !g.ev.hooks_live {
        return true;
    }
    allow(crate::native::active().filter_map(|(_, n)| n.on_chat), |f| f(g, line)) && crate::script::bridge::chat(g, line)
}

/// The player pressed the use button holding item `item`; `false` stops the use.
pub fn item_use(g: &mut Session, item: usize) -> bool {
    if !g.ev.hooks_live {
        return true;
    }
    allow(crate::native::active().filter_map(|(_, n)| n.on_item_use), |f| f(g, item)) && crate::script::bridge::item_use(g, item)
}

/// The player crafted item `out`.
pub fn craft(g: &mut Session, out: usize) {
    if !g.ev.hooks_live {
        return;
    }
    for f in crate::native::active().filter_map(|(_, n)| n.on_craft) {
        f(g, out);
    }
    crate::script::bridge::craft(g, out);
}

/// Once a frame.
pub fn tick(g: &mut Session, dt: f32) {
    if !g.ev.hooks_live {
        return;
    }
    for f in crate::native::active().filter_map(|(_, n)| n.on_tick) {
        f(g, dt);
    }
    crate::script::bridge::tick(g, dt);
}

/// A script event was raised (see `Session::fire`): every mod's `on_event`
/// hears it. Events raised by the hook itself are not passed back to it.
pub fn event(g: &mut Session, name: &str, what: &str) {
    if !g.ev.hooks_live || g.ev.in_event {
        return;
    }
    g.ev.in_event = true;
    for f in crate::native::active().filter_map(|(_, n)| n.on_event) {
        f(g, name, what);
    }
    crate::script::bridge::event(g, name, what);
    g.ev.in_event = false;
}

/// The world was opened (once per session; `Native::open` is also called
/// for each dimension).
pub fn world_loaded(g: &mut Session) {
    for f in crate::native::active().filter_map(|(_, n)| n.on_world_load) {
        f(g);
    }
    crate::script::bridge::world_loaded(g);
}

/// The world was just saved.
pub fn world_saved(g: &mut Session) {
    for f in crate::native::active().filter_map(|(_, n)| n.on_world_save) {
        f(g);
    }
    crate::script::bridge::world_saved(g);
}

/// What a mod draws the heads-up display with. Positions are in screen
/// pixels from the top left; `scale()` is the size of one "pixel" of the
/// game's own interface at this window size (multiply sizes by it).
pub trait HudSink {
    fn size(&self) -> (f32, f32);
    fn scale(&self) -> f32;
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, col: Color);
    /// Text with a shadow, `scale` times the interface scale.
    fn text(&mut self, x: f32, y: f32, scale: f32, s: &str, col: Color);
    /// How wide `s` would be at `scale` times the interface scale.
    fn text_width(&self, s: &str, scale: f32) -> f32;
}

/// The renderer, as a [`HudSink`].
pub struct RenderHud<'a> {
    pub r: &'a mut Renderer,
    pub w: f32,
    pub h: f32,
    pub s: f32,
}

impl HudSink for RenderHud<'_> {
    fn size(&self) -> (f32, f32) {
        (self.w, self.h)
    }
    fn scale(&self) -> f32 {
        self.s
    }
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, col: Color) {
        self.r.rect(x, y, w, h, col);
    }
    fn text(&mut self, x: f32, y: f32, scale: f32, s: &str, col: Color) {
        self.r.label(x, y, scale * self.s, s, col);
    }
    fn text_width(&self, s: &str, scale: f32) -> f32 {
        Renderer::text_width(s, scale * self.s)
    }
}

/// Let the mods draw on the heads-up display (play only, after the
/// crosshair and before the hotbar).
pub fn draw_hud(g: &Session, r: &mut Renderer, w: f32, h: f32, s: f32) {
    if !g.ev.hooks_live {
        return;
    }
    let mut hud = RenderHud { r, w, h, s };
    for f in crate::native::active().filter_map(|(_, n)| n.on_draw_hud) {
        f(g, &mut hud);
    }
    crate::script::ext::draw::draw_hud(g, &mut hud);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nobody_listening_means_go_ahead() {
        assert!(allow(core::iter::empty::<u32>(), |_| false));
    }

    #[test]
    fn the_first_no_cancels_and_ends_the_asking() {
        let mut asked = alloc::vec::Vec::new();
        let ok = allow([1, 2, 3, 4].into_iter(), |k| {
            asked.push(k);
            k != 2
        });
        assert!(!ok);
        assert_eq!(asked, [1, 2]);
    }

    #[test]
    fn all_saying_yes_lets_it_through() {
        let mut n = 0;
        assert!(allow([5, 6, 7].into_iter(), |_| {
            n += 1;
            true
        }));
        assert_eq!(n, 3);
    }

    #[test]
    fn hooks_can_be_function_pointers() {
        fn no(_: i32) -> bool {
            false
        }
        fn yes(_: i32) -> bool {
            true
        }
        let list: [Option<fn(i32) -> bool>; 3] = [Some(yes), None, Some(no)];
        assert!(!allow(list.iter().copied().flatten(), |f| f(0)));
        assert!(allow(list[..2].iter().copied().flatten(), |f| f(0)));
    }

    #[test]
    fn an_empty_table_has_no_hooks() {
        assert!(!has_any(&Native::NONE));
        let mut n = Native::NONE;
        fn placed(_: &mut Session, _: [i32; 3], _: Block, _: &str) {}
        n.on_block_placed = Some(placed);
        assert!(has_any(&n));
        let mut n = Native::NONE;
        fn chunk(_: &mut Session, _: i32, _: i32, _: u32) {}
        n.chunk_generated = Some(chunk);
        assert!(has_any(&n));
        let mut n = Native::NONE;
        fn sun(_: &mut Session, _: u32, _: usize) -> bool {
            true
        }
        n.on_creature_sunburn = Some(sun);
        assert!(has_any(&n));
        let mut n = Native::NONE;
        fn tick(_: &mut Session, _: f32) {}
        n.on_tick = Some(tick);
        assert!(has_any(&n));
    }
}
