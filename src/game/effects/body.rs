//! What effects do to the body beyond speed and damage: how fast blocks
//! break (haste, mining fatigue), floating upwards (levitation), how far
//! the view reaches (blindness), breathing under water, and the colour the
//! screen takes. Every function is a pure function of the active effects
//! and their definitions, so it is tested without a world.

use super::{EffectDef, Effects};

type Lookup<'a, 'b> = &'b dyn Fn(&str) -> Option<&'a EffectDef>;

/// Sum over the active effects of `pick(def) * level`.
fn sum<'a>(fx: &Effects, look: Lookup<'a, '_>, pick: fn(&EffectDef) -> i32) -> i32 {
    fx.list.iter().fold(0i32, |s, a| s.saturating_add(look(&a.id).map_or(0, |d| pick(d).saturating_mul(a.level))))
}

/// What the speed of breaking blocks is multiplied by (1 with no effects).
/// However tired, digging is not stopped.
pub fn mine_scale<'a>(fx: &Effects, look: Lookup<'a, '_>) -> f32 {
    if fx.list.is_empty() {
        return 1.0;
    }
    (1.0 + sum(fx, look, |d| d.mine) as f32 / 100.0).clamp(0.05, 6.0)
}

/// The upward speed (blocks a second) the effects lift the body at; 0 for none.
pub fn lift_speed<'a>(fx: &Effects, look: Lookup<'a, '_>) -> f32 {
    let mut up = 0.0f32;
    for a in &fx.list {
        if let Some(d) = look(&a.id) {
            up += d.lift * a.level as f32;
        }
    }
    up.clamp(0.0, 12.0)
}

/// Vertical speed after `dt` seconds of being lifted at `lift`: it eases
/// towards the lift speed (and never slows a body that already rises
/// faster). With no lift, the speed is left as it is.
pub fn levitate(vel_y: f32, lift: f32, dt: f32) -> f32 {
    if !(lift > 0.0) || !(dt > 0.0) {
        return vel_y;
    }
    let k = (6.0 * dt).min(1.0);
    if vel_y >= lift {
        vel_y
    } else {
        vel_y + (lift - vel_y) * k
    }
}

/// How much of the view is lost: 0 (all of it is seen) to 1 (nothing).
pub fn blindness<'a>(fx: &Effects, look: Lookup<'a, '_>) -> f32 {
    if fx.list.is_empty() {
        return 0.0;
    }
    (sum(fx, look, |d| d.blind) as f32 / 100.0).clamp(0.0, 1.0)
}

/// The fog's start and end (blocks) with `blind` of the view lost: the end
/// comes in to a few blocks and the start to nothing.
pub fn fog_range(start: f32, end: f32, blind: f32) -> (f32, f32) {
    let b = if blind > 1.0 { 1.0 } else { blind };
    if !(b > 0.0) {
        return (start, end);
    }
    const NEAR: f32 = 3.5;
    let end2 = if end > NEAR { end + (NEAR - end) * b } else { end };
    (start * (1.0 - b), end2)
}

/// Whether an effect lets the body breathe under water.
pub fn can_breathe<'a>(fx: &Effects, look: Lookup<'a, '_>) -> bool {
    fx.list.iter().any(|a| look(&a.id).map_or(false, |d| d.breath))
}

/// The colour the screen takes (red, green, blue, opacity), from the effect
/// that tints it most; `None` when no effect does.
pub fn tint<'a>(fx: &Effects, look: Lookup<'a, '_>) -> Option<[u8; 4]> {
    let mut best: Option<&EffectDef> = None;
    for a in &fx.list {
        if let Some(d) = look(&a.id) {
            if d.tint > 0 && best.map_or(true, |b| d.tint > b.tint) {
                best = Some(d);
            }
        }
    }
    best.map(|d| {
        // Never quite opaque: a tint is seen through.
        let alpha = (d.tint as u32 * 255 / 100).min(215) as u8;
        [d.colour[0], d.colour[1], d.colour[2], alpha]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::effects::data::parse_text;
    use alloc::vec::Vec;

    fn defs() -> Vec<EffectDef> {
        parse_text(
            "effect haste\n  mine 20\n\
             effect mining_fatigue\n  mine -30\n\
             effect levitation\n  lift 1.5\n\
             effect blindness\n  blind 60\n  tint 80\n  colour 1 2 3\n\
             effect water_breathing\n  breath\n\
             effect glow\n  tint 20\n  colour 9 8 7\n",
        )
        .0
    }

    fn with(defs: &[EffectDef], list: &[(&str, i32)]) -> Effects {
        let mut fx = Effects::default();
        for (id, level) in list {
            fx.give(defs.iter().find(|d| d.id == *id).unwrap(), 10.0, *level);
        }
        fx
    }

    #[test]
    fn haste_and_fatigue_change_the_digging_speed_but_never_stop_it() {
        let defs = defs();
        let look = |id: &str| defs.iter().find(|d| d.id == id);
        assert_eq!(mine_scale(&Effects::default(), &look), 1.0);
        assert!((mine_scale(&with(&defs, &[("haste", 2)]), &look) - 1.4).abs() < 1e-5);
        assert!((mine_scale(&with(&defs, &[("mining_fatigue", 1)]), &look) - 0.7).abs() < 1e-5);
        assert!((mine_scale(&with(&defs, &[("haste", 1), ("mining_fatigue", 1)]), &look) - 0.9).abs() < 1e-5);
        assert!((mine_scale(&with(&defs, &[("mining_fatigue", 9)]), &look) - 0.05).abs() < 1e-5);
        assert_eq!(mine_scale(&with(&defs, &[("haste", 200)]), &look), 6.0);
    }

    #[test]
    fn levitation_lifts_by_level_and_eases_the_speed() {
        let defs = defs();
        let look = |id: &str| defs.iter().find(|d| d.id == id);
        assert_eq!(lift_speed(&Effects::default(), &look), 0.0);
        assert_eq!(lift_speed(&with(&defs, &[("levitation", 2)]), &look), 3.0);
        assert_eq!(lift_speed(&with(&defs, &[("levitation", 200)]), &look), 12.0);
        // No lift leaves the speed alone, even a fall.
        assert_eq!(levitate(-9.0, 0.0, 0.1), -9.0);
        // A fall turns into a rise within a second and never overshoots.
        let mut v = -20.0;
        for _ in 0..60 {
            let n = levitate(v, 3.0, 1.0 / 60.0);
            assert!(n >= v && n <= 3.0);
            v = n;
        }
        assert!((v - 3.0).abs() < 0.01, "{v}");
        // Already rising faster: not slowed.
        assert_eq!(levitate(8.0, 3.0, 0.1), 8.0);
        // A long step lands on the lift speed.
        assert_eq!(levitate(-5.0, 3.0, 10.0), 3.0);
    }

    #[test]
    fn blindness_closes_the_fog_in() {
        let defs = defs();
        let look = |id: &str| defs.iter().find(|d| d.id == id);
        assert_eq!(blindness(&Effects::default(), &look), 0.0);
        assert!((blindness(&with(&defs, &[("blindness", 1)]), &look) - 0.6).abs() < 1e-5);
        assert_eq!(blindness(&with(&defs, &[("blindness", 2)]), &look), 1.0);
        assert_eq!(fog_range(40.0, 100.0, 0.0), (40.0, 100.0));
        assert_eq!(fog_range(40.0, 100.0, 1.0), (0.0, 3.5));
        assert_eq!(fog_range(40.0, 100.0, 0.5), (20.0, 51.75));
        // A fog that is closer already is not pushed out.
        assert_eq!(fog_range(0.0, 2.0, 1.0), (0.0, 2.0));
        assert_eq!(fog_range(10.0, 20.0, -3.0), (10.0, 20.0));
    }

    #[test]
    fn only_some_effects_let_the_body_breathe() {
        let defs = defs();
        let look = |id: &str| defs.iter().find(|d| d.id == id);
        assert!(!can_breathe(&Effects::default(), &look));
        assert!(!can_breathe(&with(&defs, &[("haste", 1)]), &look));
        assert!(can_breathe(&with(&defs, &[("haste", 1), ("water_breathing", 1)]), &look));
    }

    #[test]
    fn the_strongest_tint_wins_and_stays_see_through() {
        let defs = defs();
        let look = |id: &str| defs.iter().find(|d| d.id == id);
        assert_eq!(tint(&Effects::default(), &look), None);
        assert_eq!(tint(&with(&defs, &[("haste", 1)]), &look), None);
        assert_eq!(tint(&with(&defs, &[("glow", 1)]), &look), Some([9, 8, 7, 51]));
        assert_eq!(tint(&with(&defs, &[("glow", 1), ("blindness", 1)]), &look), Some([1, 2, 3, 204]));
    }
}
