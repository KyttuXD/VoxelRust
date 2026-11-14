//! How much of the graphics card other players' skins may use, and at what
//! size each is kept (see docs/PLAYING.md, "Skins").
//!
//! A skin is kept at its full size while its player is within [`NEAR`]
//! blocks, at 256 x 256 (or its own size if that is smaller) out to [`FAR`],
//! and at 64 x 64 beyond. All together the other players' skins may use
//! [`GPU_BUDGET`] bytes of texture memory; when they would use more, the
//! skins whose players were near least recently are made smaller first (a
//! least-recently-near order), and they grow again when their player comes
//! back. Plain functions, tested without a graphics card.

use alloc::vec::Vec;

/// Texture memory for the skins of other players, in bytes.
pub const GPU_BUDGET: usize = 64 << 20;
/// Distances (blocks) out to which a skin is kept at full size, and at 256.
pub const NEAR: f32 = 48.0;
pub const FAR: f32 = 96.0;
/// The sizes of the middle and far tiers.
pub const MID_SIDE: usize = 256;
pub const LOW_SIDE: usize = 64;
/// A player this far inside or outside a border has crossed it (so that
/// standing on the border does not make the skin flicker between sizes).
const MARGIN: f32 = 6.0;

/// What a skin of `side` takes on the card: the pixels and, for a smooth
/// (mipmapped) one, a third more.
pub fn gpu_bytes(side: usize) -> usize {
    side * side * 4 * 4 / 3
}

/// The side of a skin that is `full` wide in a tier: 0 full size, 1 middle, 2 far.
pub fn tier_side(full: usize, tier: u8) -> usize {
    match tier {
        0 => full,
        1 => full.min(MID_SIDE),
        _ => full.min(LOW_SIDE),
    }
}

/// The tier a skin of `full` that is held at `cur` pixels is in (None for
/// one not on the card yet).
fn tier_of(full: usize, cur: usize) -> Option<u8> {
    if cur == 0 {
        None
    } else if cur >= full {
        Some(0)
    } else if cur > LOW_SIDE {
        Some(1)
    } else {
        Some(2)
    }
}

/// The tier a player at `dist` blocks wants; `now` is the tier they have, so
/// that a border is crossed only by a margin.
pub fn wanted_tier(now: Option<u8>, dist: f32) -> u8 {
    // (A distance that is not a number counts as far away.)
    let dist = if dist.is_finite() { dist } else { 1.0e9 };
    let near = match now {
        Some(0) => NEAR + MARGIN,
        Some(_) => NEAR - MARGIN,
        None => NEAR,
    };
    if dist <= near {
        return 0;
    }
    let mid = match now {
        Some(0) | Some(1) => FAR + MARGIN,
        Some(_) => FAR - MARGIN,
        None => FAR,
    };
    if dist <= mid {
        1
    } else {
        2
    }
}

/// One player's skin as far as the plan is concerned.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub id: u16,
    /// Side of the skin as the player made it.
    pub full: usize,
    /// Side on the card now (0 = not yet).
    pub cur: usize,
    pub dist: f32,
    /// When the player was last near (a counter that only goes up).
    pub last_near: u64,
    /// Cannot be made bigger or smaller (the picture is no longer kept).
    pub fixed: bool,
}

/// The side every skin should be at: by distance first, then, if that is more
/// than `budget` bytes in all, the skins whose players were near longest ago
/// are made smaller (to the middle tier, then to the far one) until it fits.
/// The answer is in the order of `entries`.
pub fn plan(entries: &[Entry], budget: usize) -> Vec<(u16, usize)> {
    let mut tier: Vec<u8> = entries.iter().map(|e| if e.fixed { tier_of(e.full, e.cur).unwrap_or(0) } else { wanted_tier(tier_of(e.full, e.cur), e.dist) }).collect();
    let side = |e: &Entry, t: u8| if e.fixed { e.cur } else { tier_side(e.full, t) };
    let mut total: usize = entries.iter().zip(&tier).map(|(e, &t)| gpu_bytes(side(e, t))).sum();
    while total > budget {
        // The skin to shrink: not fixed, still bigger than the smallest, longest since its player was near.
        let pick = entries
            .iter()
            .enumerate()
            .filter(|(i, e)| !e.fixed && tier[*i] < 2 && side(e, tier[*i]) > LOW_SIDE)
            .min_by(|(_, a), (_, b)| a.last_near.cmp(&b.last_near).then(b.dist.partial_cmp(&a.dist).unwrap_or(core::cmp::Ordering::Equal)).then(a.id.cmp(&b.id)))
            .map(|(i, _)| i);
        let Some(i) = pick else { break };
        let e = &entries[i];
        let before = side(e, tier[i]);
        // The next tier that is really smaller.
        while tier[i] < 2 && side(e, tier[i]) >= before {
            tier[i] += 1;
        }
        total = total - gpu_bytes(before) + gpu_bytes(side(e, tier[i]));
    }
    entries.iter().zip(&tier).map(|(e, &t)| (e.id, side(e, t))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u16, full: usize, cur: usize, dist: f32, last_near: u64) -> Entry {
        Entry { id, full, cur, dist, last_near, fixed: false }
    }

    #[test]
    fn distance_picks_the_size() {
        let p = plan(&[entry(1, 1024, 0, 10.0, 0), entry(2, 1024, 0, 70.0, 0), entry(3, 1024, 0, 200.0, 0), entry(4, 128, 0, 70.0, 0), entry(5, 64, 0, 10.0, 0)], GPU_BUDGET);
        assert_eq!(p, alloc::vec![(1, 1024), (2, 256), (3, 64), (4, 128), (5, 64)]);
    }

    #[test]
    fn a_border_is_crossed_only_by_a_margin() {
        // Held at full size, 50 blocks away: still full (the border is at 48 + 6).
        assert_eq!(plan(&[entry(1, 1024, 1024, 50.0, 0)], GPU_BUDGET)[0].1, 1024);
        assert_eq!(plan(&[entry(1, 1024, 1024, 55.0, 0)], GPU_BUDGET)[0].1, 256);
        // Held at 256, 46 blocks away: not yet full again; at 40 it is.
        assert_eq!(plan(&[entry(1, 1024, 256, 46.0, 0)], GPU_BUDGET)[0].1, 256);
        assert_eq!(plan(&[entry(1, 1024, 256, 40.0, 0)], GPU_BUDGET)[0].1, 1024);
        // Not on the card yet: the plain borders.
        assert_eq!(wanted_tier(None, 47.9), 0);
        assert_eq!(wanted_tier(None, 48.1), 1);
        assert_eq!(wanted_tier(None, 96.1), 2);
        assert_eq!(wanted_tier(Some(0), f32::NAN), 2);
    }

    #[test]
    fn what_does_not_fit_is_made_smaller_longest_unseen_first() {
        // Four 2048 skins (22 MB each), all near: 89 MB is over 64. The two whose
        // players were last near longest ago shrink, the other two stay.
        let es = [entry(1, 2048, 0, 10.0, 50), entry(2, 2048, 0, 10.0, 10), entry(3, 2048, 0, 10.0, 40), entry(4, 2048, 0, 10.0, 30)];
        let p = plan(&es, GPU_BUDGET);
        assert_eq!(p, alloc::vec![(1, 2048), (2, 256), (3, 2048), (4, 256)]);
        let total: usize = p.iter().map(|&(_, s)| gpu_bytes(s)).sum();
        assert!(total <= GPU_BUDGET, "{total}");
    }

    #[test]
    fn a_skin_that_alone_is_over_the_budget_is_shrunk_too() {
        let p = plan(&[entry(1, 4096, 0, 10.0, 0)], GPU_BUDGET);
        assert_eq!(p, alloc::vec![(1, 256)]);
    }

    #[test]
    fn a_budget_that_nothing_can_meet_still_ends() {
        let es: Vec<Entry> = (0..300).map(|i| entry(i, 4096, 0, 10.0, i as u64)).collect();
        let p = plan(&es, 1000);
        assert!(p.iter().all(|&(_, s)| s == 64), "everything goes to the smallest");
    }

    #[test]
    fn small_skins_and_fixed_ones_are_left_alone() {
        let mut fixed = entry(1, 2048, 2048, 10.0, 0);
        fixed.fixed = true;
        let es = [fixed, entry(2, 128, 0, 10.0, 0), entry(3, 128, 0, 10.0, 0)];
        // A budget too small for the fixed one: it stays, the others go to 64.
        let p = plan(&es, 1 << 20);
        assert_eq!(p[0].1, 2048);
        assert_eq!(p[1].1, 64);
        assert_eq!(p[2].1, 64);
        // Plenty of room: nothing changes.
        assert_eq!(plan(&es, GPU_BUDGET), alloc::vec![(1, 2048), (2, 128), (3, 128)]);
    }

    #[test]
    fn the_budget_holds_a_few_big_skins_and_many_small_ones() {
        assert!(10 * gpu_bytes(1024) < GPU_BUDGET);
        assert!(gpu_bytes(4096) > GPU_BUDGET);
        assert!(256 * gpu_bytes(64) < 8 << 20);
    }
}
