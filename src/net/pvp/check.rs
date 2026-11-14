//! Whether a hit on a player counts, and what it does: the server's rules as
//! plain functions of numbers (tested without sockets).

use super::wire::Gear;
use crate::math::sqrtf;

/// Farthest a hit may be from the attacker's eyes to the other's body: the
/// reach of a player (4.5) with some room for the time a message takes.
pub const REACH: f64 = 5.2;
/// Seconds between two hits of one attacker (the blow cooldown of the game).
pub const MIN_GAP: f64 = 0.35;
/// A little less is let through, for messages that arrive in a bunch.
pub const GAP_SLACK: f64 = 0.04;
/// The most one hit can do before armour, whatever a game claims.
pub const MAX_BLOW: i32 = 40;
/// The most defense and toughness the server believes (the vanilla totals).
pub const MAX_DEFENSE: u8 = 20;
pub const MAX_TOUGHNESS: u8 = 8;
/// Half the width and the height of a player's body, and the height of the eyes.
const HALF: f64 = 0.3;
const HEIGHT: f64 = 1.8;
const EYE: f64 = 1.62;

/// Everything the server looks at before it lets a hit count.
#[derive(Clone, Copy, Debug)]
pub struct Check {
    /// The server's switch (`pvp`).
    pub pvp_on: bool,
    /// Both players' `PERM_PVP`.
    pub attacker_may: bool,
    pub target_may: bool,
    pub attacker_creative: bool,
    pub target_creative: bool,
    pub attacker_alive: bool,
    pub target_alive: bool,
    /// The target's game sent its gear message (an older game is not attackable).
    pub target_capable: bool,
    /// Last known feet positions (None until a position arrived).
    pub attacker_pos: Option<[f64; 3]>,
    pub target_pos: Option<[f64; 3]>,
    /// Seconds since this attacker's last hit that counted (large when none).
    pub since_last: f64,
    /// Seconds of spawn protection the target has left (0 or less: none).
    pub protected_left: f64,
}

/// Why a hit does not count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    Off,
    NotAllowed,
    Peaceful,
    Creative,
    Dead,
    OldGame,
    NoPosition,
    Protected,
    Range,
    Cooldown,
}

/// Distance from `eye` to the nearest point of the body standing at `feet`.
pub fn distance_to_body(eye: [f64; 3], feet: [f64; 3]) -> f64 {
    let lo = [feet[0] - HALF, feet[1], feet[2] - HALF];
    let hi = [feet[0] + HALF, feet[1] + HEIGHT, feet[2] + HALF];
    let mut sum = 0.0f64;
    for a in 0..3 {
        let out = if eye[a] < lo[a] {
            lo[a] - eye[a]
        } else if eye[a] > hi[a] {
            eye[a] - hi[a]
        } else {
            0.0
        };
        sum += out * out;
    }
    sqrtf(sum as f32) as f64
}

/// Decide a hit: Ok, or the first reason it does not count. The order is
/// cheap and plain reasons first; the cooldown last, so a rejected hit never
/// uses it up.
pub fn judge(c: &Check) -> Result<(), Why> {
    if !c.pvp_on {
        return Err(Why::Off);
    }
    if !c.attacker_may {
        return Err(Why::NotAllowed);
    }
    if !c.target_may {
        return Err(Why::Peaceful);
    }
    if c.attacker_creative || c.target_creative {
        return Err(Why::Creative);
    }
    if !c.attacker_alive || !c.target_alive {
        return Err(Why::Dead);
    }
    if !c.target_capable {
        return Err(Why::OldGame);
    }
    let (Some(a), Some(t)) = (c.attacker_pos, c.target_pos) else { return Err(Why::NoPosition) };
    if c.protected_left > 0.0 {
        return Err(Why::Protected);
    }
    if !(distance_to_body([a[0], a[1] + EYE, a[2]], t) <= REACH) {
        return Err(Why::Range);
    }
    if !(c.since_last >= MIN_GAP - GAP_SLACK) {
        return Err(Why::Cooldown);
    }
    Ok(())
}

/// What the item in hand (index plus one, 0 for the bare hand) does as a blow
/// before anything is added: 1 for fists.
pub fn item_base(item: u16) -> i32 {
    if item == 0 {
        return 1;
    }
    crate::block::reg().items.get(item as usize - 1).map_or(1, |i| i.x.damage.max(1))
}

/// The most a game may claim for a weapon whose blow is `base`: effects and
/// the damage attribute may multiply it, within reason.
pub fn claim_cap(base: i32) -> i32 {
    base.max(1).saturating_mul(4).saturating_add(4).min(MAX_BLOW)
}

/// The blow that counts: what the attacker's game says (at least 1), but no more than the weapon allows.
pub fn blow(base: i32, claimed: u8) -> i32 {
    (claimed as i32).clamp(1, claim_cap(base))
}

/// The gear as the server believes it.
pub fn clamp_gear(g: Gear) -> Gear {
    Gear { defense: g.defense.min(MAX_DEFENSE), toughness: g.toughness.min(MAX_TOUGHNESS), alive: g.alive }
}

/// A blow after the target's armour (the game's own maths, see `gear::armor`),
/// never below 1.
pub fn after_armour(damage: i32, g: Gear) -> i32 {
    let g = clamp_gear(g);
    crate::game::gear::armor::reduced(damage.max(1), g.defense as f32, g.toughness as f32)
}

/// The way the target is thrown: horizontally away from the attacker, as a
/// unit vector (x, z). Standing on the same spot, along the way the attacker
/// faces (`yaw`).
pub fn knock_dir(attacker: [f64; 3], target: [f64; 3], yaw: f32) -> [f32; 2] {
    let (dx, dz) = ((target[0] - attacker[0]) as f32, (target[2] - attacker[2]) as f32);
    let len = sqrtf(dx * dx + dz * dz);
    if len > 0.05 {
        [dx / len, dz / len]
    } else {
        [-crate::math::sinf(yaw), -crate::math::cosf(yaw)]
    }
}

/// Whether two positions are within `range` blocks of each other on the ground plane and up.
pub fn near(a: [f64; 3], b: [f64; 3], range: f64) -> bool {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    dx * dx + dy * dy + dz * dz <= range * range
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fine() -> Check {
        Check {
            pvp_on: true,
            attacker_may: true,
            target_may: true,
            attacker_creative: false,
            target_creative: false,
            attacker_alive: true,
            target_alive: true,
            target_capable: true,
            attacker_pos: Some([0.0, 64.0, 0.0]),
            target_pos: Some([2.0, 64.0, 0.0]),
            since_last: 10.0,
            protected_left: 0.0,
        }
    }

    #[test]
    fn a_plain_hit_in_range_counts() {
        assert_eq!(judge(&fine()), Ok(()));
    }

    #[test]
    fn range_is_measured_from_the_eyes_to_the_body() {
        let mut c = fine();
        c.target_pos = Some([5.0, 64.0, 0.0]); // the near side of the body is 4.7 away
        assert_eq!(judge(&c), Ok(()));
        c.target_pos = Some([5.6, 64.0, 0.0]); // 5.3 away
        assert_eq!(judge(&c), Err(Why::Range));
        // Far above, or below: the body's box is what counts.
        c.target_pos = Some([0.0, 64.0 + 1.62 + 6.0, 0.0]);
        assert_eq!(judge(&c), Err(Why::Range));
        c.target_pos = Some([0.0, 64.0 + 1.62 - 0.5, 0.0]);
        assert_eq!(judge(&c), Ok(()));
        // Inside each other is no distance at all.
        assert_eq!(distance_to_body([0.0, 1.0, 0.0], [0.0, 0.0, 0.0]), 0.0);
        assert!((distance_to_body([3.3, 1.0, 0.0], [0.0, 0.0, 0.0]) - 3.0).abs() < 1e-5);
    }

    #[test]
    fn a_second_hit_too_soon_is_refused_and_one_after_the_cooldown_counts() {
        let mut c = fine();
        c.since_last = 0.1;
        assert_eq!(judge(&c), Err(Why::Cooldown));
        c.since_last = 0.2;
        assert_eq!(judge(&c), Err(Why::Cooldown));
        c.since_last = 0.35;
        assert_eq!(judge(&c), Ok(()));
        c.since_last = f64::NAN;
        assert_eq!(judge(&c), Err(Why::Cooldown));
    }

    #[test]
    fn spawn_protection_the_switch_and_the_permissions_stop_hits() {
        let mut c = fine();
        c.protected_left = 2.0;
        assert_eq!(judge(&c), Err(Why::Protected));
        c.protected_left = 0.0;
        c.pvp_on = false;
        assert_eq!(judge(&c), Err(Why::Off));
        c.pvp_on = true;
        c.attacker_may = false;
        assert_eq!(judge(&c), Err(Why::NotAllowed));
        c.attacker_may = true;
        c.target_may = false;
        assert_eq!(judge(&c), Err(Why::Peaceful));
    }

    #[test]
    fn creative_players_neither_hit_nor_are_hit_and_the_dead_are_left_alone() {
        let mut c = fine();
        c.attacker_creative = true;
        assert_eq!(judge(&c), Err(Why::Creative));
        c.attacker_creative = false;
        c.target_creative = true;
        assert_eq!(judge(&c), Err(Why::Creative));
        c.target_creative = false;
        c.target_alive = false;
        assert_eq!(judge(&c), Err(Why::Dead));
        c.target_alive = true;
        c.attacker_alive = false;
        assert_eq!(judge(&c), Err(Why::Dead));
        c.attacker_alive = true;
        c.target_capable = false;
        assert_eq!(judge(&c), Err(Why::OldGame));
        c.target_capable = true;
        c.target_pos = None;
        assert_eq!(judge(&c), Err(Why::NoPosition));
    }

    #[test]
    fn the_blow_is_what_the_game_says_within_what_the_weapon_allows() {
        assert_eq!(item_base(0), 1);
        assert_eq!(blow(1, 0), 1);
        assert_eq!(blow(1, 3), 3);
        assert_eq!(blow(1, 200), claim_cap(1));
        assert_eq!(claim_cap(1), 8);
        assert_eq!(claim_cap(7), 32);
        assert_eq!(claim_cap(1000), MAX_BLOW);
        assert_eq!(claim_cap(-5), 8);
        // An item number that is not one is a fist.
        assert_eq!(item_base(u16::MAX), 1);
    }

    #[test]
    fn armour_takes_its_share_and_what_a_game_reports_is_clamped() {
        let none = Gear::default();
        assert_eq!(after_armour(10, none), 10);
        // 10 points of defense: 40 percent off.
        assert_eq!(after_armour(10, Gear { defense: 10, toughness: 0, alive: true }), 6);
        // A game that reports 255 and 255 is believed only up to 20 and 8: the same as 20 and 8.
        let lie = Gear { defense: 255, toughness: 255, alive: true };
        let top = Gear { defense: 20, toughness: 8, alive: true };
        assert_eq!(after_armour(10, lie), after_armour(10, top));
        assert_eq!(clamp_gear(lie), top);
        // Never less than 1, and never more than the armour rule's cap (80 percent).
        assert_eq!(after_armour(1, top), 1);
        assert!(after_armour(40, top) >= 8);
    }

    #[test]
    fn the_knockback_points_away_from_the_attacker() {
        let [x, z] = knock_dir([0.0, 64.0, 0.0], [3.0, 64.0, 0.0], 0.0);
        assert!((x - 1.0).abs() < 1e-5 && z.abs() < 1e-5);
        let [x, z] = knock_dir([5.0, 64.0, 5.0], [5.0, 64.0, 1.0], 0.0);
        assert!(x.abs() < 1e-5 && (z + 1.0).abs() < 1e-5);
        // On the same spot: along the way the attacker looks (yaw 0 looks toward -z).
        let [x, z] = knock_dir([1.0, 64.0, 1.0], [1.0, 64.0, 1.0], 0.0);
        assert!(x.abs() < 1e-5 && (z + 1.0).abs() < 1e-5);
    }

    #[test]
    fn near_is_a_ball() {
        assert!(near([0.0; 3], [3.0, 4.0, 0.0], 5.0));
        assert!(!near([0.0; 3], [3.0, 4.0, 0.1], 5.0));
    }
}
