//! Allowances: memory the player chose to spend on pictures (HD textures,
//! HD skins) is not the game's to give back.
//!
//! The soft limit (300 MB, 700 MB with mods) is for the world: when the game
//! is over it, the ladder shortens the view distance and simplifies far
//! terrain. A pack of 512 x 512 textures would use that up on its own and the
//! world would shrink to nothing for something the player asked for. So what
//! the HD pictures cost (graphics card and main memory) is added to the limit
//! while they are in use, up to a hard ceiling for the whole game.
//!
//! Nothing changes with no HD content loaded: the allowance is 0 and the
//! limit is the soft limit.
//!
//! Whoever loads HD pictures reports their cost:
//! * [`set_allowance`] says what a kind costs now (replacing what it said before);
//! * [`add_allowance`] / [`remove_allowance`] when pictures come and go one by one.

use core::sync::atomic::{AtomicUsize, Ordering};

const MB: usize = 1 << 20;

/// The most the whole game may be allowed to hold, whatever the allowances say.
pub const HARD_CEILING: usize = 1536 * MB;

/// What the allowed memory is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Allow {
    /// The HD atlas of block and item textures, and the pictures kept to build it again.
    Textures,
    /// Pictures of creatures and models bigger than the usual 64 x 64.
    Models,
    /// HD player skins.
    Skins,
}

const KINDS: usize = 3;

static ALLOWED: [AtomicUsize; KINDS] = [AtomicUsize::new(0), AtomicUsize::new(0), AtomicUsize::new(0)];

fn slot(kind: Allow) -> &'static AtomicUsize {
    &ALLOWED[kind as usize]
}

/// `kind` costs `bytes` now (replacing what it was).
pub fn set_allowance(kind: Allow, bytes: usize) {
    slot(kind).store(bytes, Ordering::Relaxed);
}

/// `bytes` more are spent on `kind` (pictures loaded).
pub fn add_allowance(kind: Allow, bytes: usize) {
    let a = slot(kind);
    let mut cur = a.load(Ordering::Relaxed);
    loop {
        match a.compare_exchange_weak(cur, cur.saturating_add(bytes), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(now) => cur = now,
        }
    }
}

/// `bytes` less are spent on `kind` (pictures let go). Never below 0.
pub fn remove_allowance(kind: Allow, bytes: usize) {
    let a = slot(kind);
    let mut cur = a.load(Ordering::Relaxed);
    loop {
        match a.compare_exchange_weak(cur, cur.saturating_sub(bytes), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(now) => cur = now,
        }
    }
}

/// What one kind is allowed now.
pub fn allowance_of(kind: Allow) -> usize {
    slot(kind).load(Ordering::Relaxed)
}

/// What all kinds are allowed together (before the ceiling).
pub fn allowance() -> usize {
    ALLOWED.iter().fold(0usize, |a, k| a.saturating_add(k.load(Ordering::Relaxed)))
}

/// The limit with an allowance added: never over [`HARD_CEILING`], and never
/// under the soft limit itself (a base over the ceiling is left as it is).
pub fn raised_limit(base: usize, allowed: usize) -> usize {
    base.saturating_add(allowed).min(HARD_CEILING.max(base))
}

/// The limit now: the soft limit (see `soft_limit`) plus what is allowed.
pub fn current_limit(with_mods: bool) -> usize {
    raised_limit(super::soft_limit(with_mods), allowance())
}

/// The most that can be allowed on top of the soft limit: what the HD
/// atlas may cost in all (it is planned to fit this).
pub fn room(with_mods: bool) -> usize {
    HARD_CEILING.saturating_sub(super::soft_limit(with_mods))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_nothing_allowed_the_limits_are_the_old_ones() {
        assert_eq!(raised_limit(300 * MB, 0), 300 * MB);
        assert_eq!(raised_limit(700 * MB, 0), 700 * MB);
    }

    #[test]
    fn the_limit_rises_by_the_allowance() {
        assert_eq!(raised_limit(300 * MB, 85 * MB), 385 * MB);
        assert_eq!(raised_limit(700 * MB, 200 * MB), 900 * MB);
    }

    #[test]
    fn the_limit_stops_at_the_hard_ceiling() {
        assert_eq!(raised_limit(300 * MB, 5000 * MB), HARD_CEILING);
        assert_eq!(raised_limit(700 * MB, usize::MAX), HARD_CEILING);
        // A base over the ceiling (never so today) is not lowered by it.
        assert_eq!(raised_limit(2000 * MB, 10 * MB), 2000 * MB);
    }

    #[test]
    fn what_can_be_allowed_is_what_is_left_under_the_ceiling() {
        assert_eq!(room(false), HARD_CEILING - 300 * MB);
        assert_eq!(room(true), HARD_CEILING - 700 * MB);
    }

    #[test]
    fn allowances_add_remove_and_never_go_below_zero() {
        // (The statics are shared with other tests: only the Skins slot is touched here.)
        set_allowance(Allow::Skins, 0);
        add_allowance(Allow::Skins, 10 * MB);
        add_allowance(Allow::Skins, 5 * MB);
        assert_eq!(allowance_of(Allow::Skins), 15 * MB);
        remove_allowance(Allow::Skins, 4 * MB);
        assert_eq!(allowance_of(Allow::Skins), 11 * MB);
        remove_allowance(Allow::Skins, 100 * MB);
        assert_eq!(allowance_of(Allow::Skins), 0);
        add_allowance(Allow::Skins, usize::MAX);
        add_allowance(Allow::Skins, 7);
        assert_eq!(allowance_of(Allow::Skins), usize::MAX);
        set_allowance(Allow::Skins, 0);
        assert_eq!(allowance_of(Allow::Skins), 0);
    }
}
