//! Feeding the world's lists to the mod hooks, a little each frame: chunks
//! that were just generated (`on_chunk_generated`), blocks that were just
//! placed (`on_block_placed`) and night creatures waiting for a verdict on
//! burning in daylight (`on_creature_sunburn`).
//!
//! `events::poll` calls [`pump`] once a frame. Generation, edits and the
//! creature step only write bounded lists; the work of the hooks is spread
//! here so a burst (a whole view of new chunks, a flood of water) never
//! stalls a frame:
//!
//! - at most [`CHUNKS_PER_FRAME`] chunks a frame get their hook, oldest first,
//!   and each call may write at most `structures::MAX_WRITES` blocks and
//!   spend one script call's step allowance;
//! - at most [`PLACED_PER_FRAME`] placements a frame, and a placement made by
//!   a hook is passed on only [`MAX_PLACE_DEPTH`](crate::terrain::world_log::MAX_PLACE_DEPTH)
//!   hooks deep, so a script that places a block in answer to a block
//!   being placed cannot go on for ever;
//! - nothing is kept for a hook nobody listens to.

use crate::game::{hooks, mob_hooks};
use crate::game::session::Session;
use crate::terrain::world_log::may_dispatch;

/// Most chunks that get `on_chunk_generated` in one frame.
pub const CHUNKS_PER_FRAME: usize = 2;
/// Most placements that get `on_block_placed` in one frame.
pub const PLACED_PER_FRAME: usize = 128;

/// Whether anything (a mod's code or a script) listens for the hook `name`;
/// `native` says whether a switched-on mod's code has it.
fn listened(native: impl Fn(&crate::native::Native) -> bool, name: &str) -> bool {
    crate::native::active().any(|(_, n)| native(n)) || crate::script::bridge::wants(name)
}

/// Once a frame.
pub fn pump(g: &mut Session) {
    let live = g.ev.hooks_live;
    let want_chunks = live && listened(|n| n.chunk_generated.is_some(), "on_chunk_generated");
    let want_placed = live && listened(|n| n.on_block_placed.is_some(), "on_block_placed");
    let want_sun = live && listened(|n| n.on_creature_sunburn.is_some(), "on_creature_sunburn");
    g.world.log.track_placed = want_placed;
    g.mobs.ask_sun = want_sun;

    // Night creatures in daylight: the scripts say whether they burn.
    if want_sun {
        for (id, kind) in core::mem::take(&mut g.mobs.sun_asks) {
            let burns = hooks::creature_sunburn(g, id, kind);
            if let Some(m) = g.mobs.list.iter_mut().find(|m| m.id == id) {
                m.sun_veto = mob_hooks::sun_answer(burns);
            }
        }
    } else {
        g.mobs.sun_asks.clear();
    }

    // New chunks: only whoever runs the world places structures (others get the blocks sent).
    if want_chunks && g.runs_the_world() {
        for (cx, cz) in g.world.log.take_generated(CHUNKS_PER_FRAME) {
            // It may have gone again since; the neighbours may or may not be there, the hook cannot tell.
            if g.world.is_loaded(cx.wrapping_mul(16), cz.wrapping_mul(16)) {
                hooks::chunk_generated(g, cx, cz);
            }
        }
    } else {
        g.world.log.generated.clear();
    }

    // Placed blocks.
    if want_placed {
        for p in g.world.log.take_placed(PLACED_PER_FRAME) {
            if !may_dispatch(p.depth) {
                continue;
            }
            // Whatever the hook places is one hook deeper.
            let before = core::mem::replace(&mut g.world.log.depth, p.depth + 1);
            hooks::block_placed(g, p.cell, p.block, p.cause);
            g.world.log.depth = before;
        }
    } else {
        g.world.log.placed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_budgets_are_small_but_not_zero() {
        assert!((1..=8).contains(&CHUNKS_PER_FRAME));
        assert!((1..=512).contains(&PLACED_PER_FRAME));
    }

    #[test]
    fn a_chain_of_hooks_ends_after_two() {
        // Depth 0: a player's block. Its hook places one (depth 1), whose hook
        // places one (depth 2): that one is not passed on.
        let mut depth = 0u8;
        let mut calls = 0;
        while may_dispatch(depth) {
            calls += 1;
            depth += 1;
        }
        assert_eq!(calls, 2);
    }
}
