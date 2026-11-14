//! What mods may say about an explosion: the hooks before and after it, the
//! blocks it may not destroy, and the limits on all of it.
//!
//! Every explosion of the game (TNT and lit blocks, a creature that blows
//! up, `/blast`, a script's `explode`, another mod's compiled code) goes
//! through `explosion::explode_with`, and that calls into this folder, so
//! each path is covered by the same rules:
//!
//! 1. `before` asks the native mods' `before_explode(g, x, y, z, radius,
//!    cause) -> Option<f32>` (none cancels, a number is the new radius, 0 to
//!    16), then the scripts' older `on_explode` (power), then the scripts'
//!    `before_explode(x, y, z, radius, cause) -> bool` (which may call
//!    `set_explosion_radius`).
//! 2. For each block the blast would destroy a [`Gate`] decides: blocks on a
//!    mod's immune list stay at once (a bit test); for the rest the mods'
//!    `explode_block(x, y, z, block) -> bool` is asked, for the first
//!    [`MAX_BLOCK_CALLS`] blocks of the blast (past that the immune list
//!    alone decides).
//! 3. `after` tells `after_explode(x, y, z, radius, destroyed, cause)`.
//!
//! Explosions that happen inside a hook (a mod calling `explode` from its
//! `before_explode`) nest: from the third level on ([`MAX_HOOK_DEPTH`]) no
//! hook is called any more, only the immune lists apply, so a mod cannot
//! make the game recurse. A chain of TNT is not a nesting: each lit block
//! explodes in its own frame and asks the hooks on its own.
//!
//! | file | subject |
//! | --- | --- |
//! | `immune.rs` | the per-mod lists of immune blocks, as one bit set |
//! | `mod.rs` | the state, the radius rules, the gate, the calls to the mods |

pub mod immune;

use crate::block::{reg, Block};
use crate::explosion::{self, ExplosionInfo, RADIUS_PER_POWER};
use crate::session::Session;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// The key in `Session::ext`.
pub const KEY: &str = "explosion-hooks";
/// The most blocks of one blast `explode_block` is asked about.
pub const MAX_BLOCK_CALLS: usize = 20_000;
/// Explosions nested this deep (inside hooks of explosions) still call hooks.
pub const MAX_HOOK_DEPTH: u32 = 2;
/// The largest radius a hook may set (a ball 16 blocks in radius).
pub const MAX_RADIUS: f32 = 16.0;

/// What the session keeps for the explosion hooks.
#[derive(Default)]
pub struct HookState {
    /// How many explosions are being carried out inside one another.
    pub depth: u32,
    /// The blocks mods keep immune.
    pub immune: immune::Registry,
    /// The radius of the blast whose `before_explode` is running (a script
    /// may change it with `set_explosion_radius`); `None` outside the hook.
    pub pending: Option<f32>,
}

impl HookState {
    /// `set_explosion_radius(r)`: accepted only inside the hook, held to
    /// 0..=[`MAX_RADIUS`] (0 cancels the blast). Whether it was taken.
    pub fn set_radius(&mut self, r: f64) -> bool {
        if self.pending.is_none() || r.is_nan() {
            return false;
        }
        self.pending = Some(clamp_radius(r as f32));
        true
    }
}

/// A radius a hook asked for, held to 0..=[`MAX_RADIUS`] (not a number is 0).
pub fn clamp_radius(r: f32) -> f32 {
    if !(r > 0.0) {
        0.0
    } else if r > MAX_RADIUS {
        MAX_RADIUS
    } else {
        r
    }
}

/// The session's hook state, made if there is none yet.
pub fn state(g: &mut Session) -> &mut HookState {
    let at = match g.ext.iter().position(|e| e.0 == KEY) {
        Some(i) => i,
        None => {
            g.ext.push((KEY, Box::new(HookState::default())));
            g.ext.len() - 1
        }
    };
    if g.ext[at].1.downcast_ref::<HookState>().is_none() {
        g.ext[at].1 = Box::new(HookState::default());
    }
    match g.ext[at].1.downcast_mut::<HookState>() {
        Some(s) => s,
        None => unreachable!("the entry was just made a HookState"),
    }
}

/// What a list entry means in the game's blocks.
pub fn resolve(s: &immune::Spec) -> Option<Block> {
    let r = reg();
    match s {
        immune::Spec::Name(n) => r.block(n),
        immune::Spec::Number(n) if *n >= 0 && (*n as usize) < r.blocks.len() => Some(*n as Block),
        _ => None,
    }
}

/// Whether blasts leave `b` alone because some mod said so (a script's list
/// or a native mod's `immune_blocks`). Not the game's own `resists`.
pub fn is_immune(g: &mut Session, b: Block) -> bool {
    if state(g).immune.contains(b) {
        return true;
    }
    crate::native::active().any(|(_, n)| n.immune_blocks.iter().any(|name| reg().block(name) == Some(b)))
}

// ---- Nesting.

/// An explosion starts: how deep it is (1 for one that is not inside another).
pub fn enter(g: &mut Session) -> u32 {
    let s = state(g);
    s.depth = s.depth.saturating_add(1);
    s.depth
}

/// The explosion that [`enter`]ed is over.
pub fn leave(g: &mut Session) {
    let s = state(g);
    s.depth = s.depth.saturating_sub(1);
}

/// Whether an explosion at this depth calls the hooks.
pub fn hooks_allowed(depth: u32) -> bool {
    depth <= MAX_HOOK_DEPTH
}

// ---- Before and after.

/// Ask the mods about an explosion about to happen. They may cancel it
/// (false) or change `info.power`; what they leave is held to the game's
/// limits by the caller.
pub fn before(g: &mut Session, info: &mut ExplosionInfo) -> bool {
    let cause = info.source.cause();
    let mut radius = clamp_radius(explosion::radius(info.power));
    for (_, n) in crate::native::active() {
        if let Some(hook) = n.before_explode {
            match hook(g, info.at[0], info.at[1], info.at[2], radius, cause) {
                Some(r) => {
                    radius = clamp_radius(r);
                    if radius <= 0.0 {
                        return false;
                    }
                }
                None => return false,
            }
        }
    }
    info.power = radius / RADIUS_PER_POWER;
    // The scripts' `on_explode(x, y, z, power, source)`, from before the radius hooks.
    if !crate::script::bridge::explode(g, info) {
        return false;
    }
    let radius = clamp_radius(explosion::radius(info.power));
    match crate::script::bridge::before_explode(g, info.at, radius, cause) {
        Some(r) => {
            info.power = r / RADIUS_PER_POWER;
            true
        }
        None => false,
    }
}

/// Tell the mods an explosion is over: `destroyed` blocks went.
pub fn after(g: &mut Session, info: &ExplosionInfo, destroyed: usize) {
    for (_, n) in crate::native::active() {
        if let Some(hook) = n.after_explode {
            hook(g, info, destroyed);
        }
    }
    crate::script::bridge::after_explode(g, info.at, explosion::radius(info.power), destroyed, info.source.cause());
}

// ---- The gate.

/// Decides, for one blast, which blocks it may destroy.
pub struct Gate {
    immune: Vec<u64>,
    /// How many `explode_block` questions were asked.
    pub calls: usize,
    /// The most questions to ask.
    pub cap: usize,
    /// Some native mod has an `explode_block`.
    pub native_veto: bool,
    /// Some script has an `explode_block`.
    pub script_veto: bool,
}

impl Gate {
    /// A gate with these immune blocks (a bit set) and no one to ask.
    pub fn new(immune: Vec<u64>, cap: usize) -> Gate {
        Gate { immune, calls: 0, cap, native_veto: false, script_veto: false }
    }

    /// Whether a mod's list protects `b`.
    pub fn is_immune(&self, b: Block) -> bool {
        immune::bit_set(&self.immune, b)
    }

    /// Whether the blast may destroy `b`: not if it is immune; else, while
    /// the budget of questions lasts, as `ask` answers.
    pub fn allows(&mut self, b: Block, ask: &mut dyn FnMut() -> bool) -> bool {
        if self.is_immune(b) {
            return false;
        }
        if !(self.native_veto || self.script_veto) || self.calls >= self.cap {
            return true;
        }
        self.calls += 1;
        ask()
    }
}

/// The gate for a blast at this depth: the immune lists of the scripts and
/// the native mods, and (when hooks are allowed) who wants to be asked.
pub fn gate(g: &mut Session, depth: u32) -> Gate {
    let r = reg();
    let st = state(g);
    st.immune.refresh(r.blocks.len(), &resolve);
    let mut bits = st.immune.bits();
    let hooks = hooks_allowed(depth);
    let mut native_veto = false;
    for (_, n) in crate::native::active() {
        for name in n.immune_blocks {
            if let Some(b) = r.block(name) {
                immune::add_bit(&mut bits, b);
            }
        }
        if hooks && n.explode_block.is_some() {
            native_veto = true;
        }
    }
    let mut gate = Gate::new(bits, MAX_BLOCK_CALLS);
    gate.native_veto = native_veto;
    gate.script_veto = hooks && crate::script::bridge::wants_block_calls();
    gate
}

/// Ask the mods whether the blast may destroy the block `b` at `cell`.
pub fn ask_block(g: &mut Session, native: bool, script: bool, cell: [i32; 3], b: Block) -> bool {
    if native {
        for (_, n) in crate::native::active() {
            if let Some(hook) = n.explode_block {
                if !hook(g, cell, b) {
                    return false;
                }
            }
        }
    }
    if script {
        return crate::script::bridge::explode_block(g, cell, b);
    }
    true
}

/// The name of a block as hooks see it.
pub fn block_id(b: Block) -> String {
    crate::block::def(b).id.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::explosion::ExplosionSource;

    fn gate_with(immune_blocks: &[Block], cap: usize, veto: bool) -> Gate {
        let mut bits = Vec::new();
        for &b in immune_blocks {
            immune::add_bit(&mut bits, b);
        }
        let mut g = Gate::new(bits, cap);
        g.script_veto = veto;
        g
    }

    #[test]
    fn the_radius_is_held_between_nothing_and_sixteen() {
        assert_eq!(clamp_radius(5.0), 5.0);
        assert_eq!(clamp_radius(16.0), 16.0);
        assert_eq!(clamp_radius(100.0), 16.0);
        assert_eq!(clamp_radius(f32::INFINITY), 16.0);
        assert_eq!(clamp_radius(0.0), 0.0);
        assert_eq!(clamp_radius(-3.0), 0.0);
        assert_eq!(clamp_radius(f32::NAN), 0.0);
    }

    #[test]
    fn set_explosion_radius_works_only_inside_the_hook() {
        let mut s = HookState::default();
        assert!(!s.set_radius(4.0), "taken outside a hook");
        assert_eq!(s.pending, None);
        s.pending = Some(8.0);
        assert!(s.set_radius(4.0));
        assert_eq!(s.pending, Some(4.0));
        assert!(s.set_radius(99.0));
        assert_eq!(s.pending, Some(16.0));
        // 0 (and anything below) cancels: it is kept as 0.
        assert!(s.set_radius(-1.0));
        assert_eq!(s.pending, Some(0.0));
        // Not a number is refused and changes nothing.
        s.pending = Some(3.0);
        assert!(!s.set_radius(f64::NAN));
        assert_eq!(s.pending, Some(3.0));
    }

    #[test]
    fn immune_blocks_stay_without_anyone_being_asked() {
        let mut gate = gate_with(&[7], 10, true);
        let mut asked = 0;
        assert!(!gate.allows(7, &mut || {
            asked += 1;
            true
        }));
        assert_eq!(asked, 0);
        assert_eq!(gate.calls, 0);
        assert!(gate.allows(3, &mut || {
            asked += 1;
            true
        }));
        assert_eq!((asked, gate.calls), (1, 1));
    }

    #[test]
    fn a_veto_spares_the_block() {
        let mut gate = gate_with(&[], 10, true);
        assert!(!gate.allows(3, &mut || false));
        assert!(gate.allows(3, &mut || true));
    }

    #[test]
    fn nobody_is_asked_when_no_mod_has_a_veto() {
        let mut gate = gate_with(&[], 10, false);
        assert!(gate.allows(3, &mut || panic!("asked for nothing")));
        assert_eq!(gate.calls, 0);
    }

    #[test]
    fn past_the_cap_only_the_immune_list_decides() {
        let mut gate = gate_with(&[9], 3, true);
        let mut asked = 0;
        let mut spared = 0;
        for _ in 0..10 {
            if !gate.allows(1, &mut || {
                asked += 1;
                false
            }) {
                spared += 1;
            }
        }
        // The first three were asked (and refused); the other seven went ahead unasked.
        assert_eq!((asked, spared, gate.calls), (3, 3, 3));
        // Immune blocks are still spared past the cap.
        assert!(!gate.allows(9, &mut || true));
    }

    #[test]
    fn the_cap_is_twenty_thousand() {
        assert_eq!(MAX_BLOCK_CALLS, 20_000);
        let mut gate = gate_with(&[], MAX_BLOCK_CALLS, true);
        let mut asked = 0usize;
        for _ in 0..MAX_BLOCK_CALLS + 500 {
            gate.allows(1, &mut || {
                asked += 1;
                true
            });
        }
        assert_eq!(asked, MAX_BLOCK_CALLS);
    }

    #[test]
    fn hooks_stop_below_the_second_level() {
        assert!(hooks_allowed(1));
        assert!(hooks_allowed(2));
        assert!(!hooks_allowed(3));
        assert!(!hooks_allowed(u32::MAX));
    }

    #[test]
    fn every_way_to_blow_up_has_a_cause_name() {
        assert_eq!(ExplosionSource::Tnt.cause(), "tnt");
        assert_eq!(ExplosionSource::Creature(0).cause(), "creature");
        assert_eq!(ExplosionSource::Command.cause(), "player");
        assert_eq!(ExplosionSource::Mod(1).cause(), "script");
        for s in [ExplosionSource::Tnt, ExplosionSource::Creature(3), ExplosionSource::Command, ExplosionSource::Mod(9)] {
            assert!(["tnt", "creature", "script", "player", "fire", "other"].contains(&s.cause()));
        }
    }
}
