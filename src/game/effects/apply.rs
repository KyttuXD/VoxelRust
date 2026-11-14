//! Where effects meet the game: what each one does to speed, jumping, the
//! damage dealt, hearts and hunger (pure functions of the list and the
//! definitions, tested here), and the glue that gives them to the player
//! and to creatures, ticks them and answers `/effect`.
//!
//! The effect list itself and its timing are in `mod.rs`; what an effect
//! is, in `data.rs`.

use super::{clock, roman, EffectDef, Effects, Given, HeartStyle, Nature, SourceKind, MAX_LEVEL, MAX_SECS};
use crate::audio::Audio;
use crate::block::{self, reg, AIR};
use crate::math::{floord, forward};
use crate::session::{GameMode, Session, MAX_HEALTH, MAX_HUNGER};
use alloc::string::String;
use alloc::vec::Vec;

/// Other names that are typed for effects (`/effect give freezing`): the
/// name and the id of the effect it means. An effect that a pack defines
/// under the name itself wins.
const ALIASES: &[(&str, &str)] = &[
    ("freezing", "frozen"),
    ("fatigue", "mining_fatigue"),
    ("blind", "blindness"),
    ("levitate", "levitation"),
    ("breathing", "water_breathing"),
    ("waterbreathing", "water_breathing"),
    ("regen", "regeneration"),
    ("fire", "burning"),
    ("nightvision", "night_vision"),
    ("jump", "jump_boost"),
];

/// The effect with this id (or another name for it) in the content.
pub fn lookup(id: &str) -> Option<&'static EffectDef> {
    let t = &reg().fx;
    t.get(id).or_else(|| ALIASES.iter().find(|a| a.0 == id).and_then(|a| t.get(a.1)))
}

type Lookup<'a, 'b> = &'b dyn Fn(&str) -> Option<&'a EffectDef>;

/// Sum over the active effects of `pick(def) * level`.
fn sum<'a>(fx: &Effects, look: Lookup<'a, '_>, pick: fn(&EffectDef) -> i32) -> i32 {
    fx.list.iter().fold(0i32, |s, a| s.saturating_add(look(&a.id).map_or(0, |d| pick(d).saturating_mul(a.level))))
}

/// What walking speed is multiplied by (1 with no effects).
pub fn speed_scale<'a>(fx: &Effects, look: Lookup<'a, '_>) -> f32 {
    (1.0 + sum(fx, look, |d| d.speed) as f32 / 100.0).clamp(0.05, 4.0)
}

/// What jumping speed is multiplied by.
pub fn jump_scale<'a>(fx: &Effects, look: Lookup<'a, '_>) -> f32 {
    (1.0 + sum(fx, look, |d| d.jump) as f32 / 100.0).clamp(0.0, 4.0)
}

/// What the damage dealt in melee is multiplied by.
pub fn melee_scale<'a>(fx: &Effects, look: Lookup<'a, '_>) -> f32 {
    (1.0 + sum(fx, look, |d| d.melee) as f32 / 100.0).clamp(0.0, 10.0)
}

/// A blow of `base` points as the effects change it: weakness and strength
/// scale it, and a blow is never made nothing.
pub fn melee_damage<'a>(fx: &Effects, look: Lookup<'a, '_>, base: i32) -> i32 {
    if base <= 0 || fx.list.is_empty() {
        return base;
    }
    let v = base as f32 * melee_scale(fx, look) + 0.5;
    (v as i32).max(1)
}

/// How the hearts are drawn: the strongest style of the active effects.
pub fn heart_style<'a>(fx: &Effects, look: Lookup<'a, '_>) -> HeartStyle {
    fx.list.iter().filter_map(|a| look(&a.id)).map(|d| d.heart).fold(HeartStyle::Normal, |best, h| if h.priority() > best.priority() { h } else { best })
}

/// Whether a flame is drawn beside the hearts.
pub fn on_fire<'a>(fx: &Effects, look: Lookup<'a, '_>) -> bool {
    fx.list.iter().any(|a| look(&a.id).map_or(false, |d| d.flame))
}

/// How much brighter the night looks: 0 (not at all) to 1.
pub fn night_vision<'a>(fx: &Effects, look: Lookup<'a, '_>) -> f32 {
    if fx.list.iter().any(|a| look(&a.id).map_or(false, |d| d.vision)) {
        1.0
    } else {
        0.0
    }
}

/// The colour of an icon's frame for an effect of this nature.
pub fn frame_colour(n: Nature) -> [u8; 3] {
    match n {
        Nature::Good => [90, 170, 90],
        Nature::Bad => [190, 70, 70],
        Nature::Neutral => [140, 140, 150],
    }
}

// ---------------------------------------------------------------- the player

/// Give the player an effect: `secs` seconds (0 for the effect's own time)
/// at `level`. Mods' code may change the time and level or refuse
/// (`Native::effect_apply`). Raises the `effectstart` event when it starts.
pub fn give(g: &mut Session, id: &str, secs: f32, level: i32) -> Result<Given, String> {
    let id = id.to_ascii_lowercase();
    let Some(d) = lookup(&id) else { return Err(alloc::format!("NO SUCH EFFECT: {}", id.to_ascii_uppercase())) };
    let id = d.id.clone();
    let mut secs = if secs > 0.0 { secs } else { d.secs };
    let mut level = level;
    secs = secs.clamp(0.0, MAX_SECS);
    level = level.clamp(1, MAX_LEVEL);
    // Not asking the mods about what would change nothing.
    if g.fx.judge(&id, secs, level) == Given::Unchanged {
        return Ok(Given::Unchanged);
    }
    for (_, n) in crate::native::active() {
        if let Some(hook) = n.effect_apply {
            if !hook(g, &id, &mut secs, &mut level) {
                return Err(String::from("REFUSED"));
            }
        }
    }
    secs = secs.clamp(0.0, MAX_SECS);
    level = level.clamp(1, MAX_LEVEL);
    let result = g.fx.give(d, secs, level);
    if result == Given::Started {
        started(g, d, level);
    }
    Ok(result)
}

/// An effect has just started on the player: the event, its sound, and the
/// mods' `effect_start` (native) and `on_effect_start` (script) hooks.
fn started(g: &mut Session, d: &EffectDef, level: i32) {
    g.fire("effectstart", &d.id);
    if !d.sound.is_empty() && g.sound_queue.len() < 32 {
        g.sound_queue.push(d.sound.clone());
    }
    for (_, n) in crate::native::active() {
        if let Some(hook) = n.effect_start {
            hook(g, &d.id, level);
        }
    }
    crate::script::effects_bridge::start(g, &d.id, level);
}

/// An effect of the player's has ended (ran out, put out, cleared): the
/// event and the mods' `effect_end` / `on_effect_end` hooks.
fn ended(g: &mut Session, id: &str) {
    g.fire("effectend", id);
    for (_, n) in crate::native::active() {
        if let Some(hook) = n.effect_end {
            hook(g, id);
        }
    }
    crate::script::effects_bridge::end(g, id);
}

/// Take one effect away, raising `effectend`. Returns whether it was there.
pub fn clear(g: &mut Session, id: &str) -> bool {
    let gone = g.fx.clear(id);
    if gone {
        ended(g, id);
    }
    gone
}

/// Take every effect away.
pub fn clear_all(g: &mut Session) {
    for id in g.fx.clear_all() {
        ended(g, &id);
    }
}

/// Whether the player is in water that would put a fire out.
fn wet(g: &Session) -> bool {
    let p = g.player.pos;
    [0.4, 1.0].iter().any(|dy| block::is_water(g.world.get(floord(p[0]), floord(p[1] + dy), floord(p[2]))))
}

/// Damage from an effect: it may kill, or may stop at half a heart.
fn hurt(g: &mut Session, amount: i32, lethal: bool, audio: &mut Audio) {
    let amount = if lethal { amount } else { amount.min(g.health - 1) };
    if amount > 0 && !g.dead {
        // Not a bite: whoever bit last is not to blame.
        g.mobs.last_biter = None;
        g.damage(amount, audio);
    }
}

/// Blocks the player is in, on or against that give effects.
fn touch(g: &mut Session) {
    let table = &reg().fx;
    if table.block_sources.is_empty() {
        return;
    }
    let (mn, mx) = g.player.aabb();
    let grow = 0.15;
    let mut found: Vec<(usize, usize)> = Vec::new();
    for x in floord(mn[0] - grow)..=floord(mx[0] + grow) {
        for y in floord(mn[1] - grow)..=floord(mx[1] + grow) {
            for z in floord(mn[2] - grow)..=floord(mx[2] + grow) {
                let b = g.world.get(x, y, z);
                if b == AIR {
                    continue;
                }
                let id = &block::def(b).id;
                for (bid, di, si) in &table.block_sources {
                    if bid == id && !found.contains(&(*di, *si)) {
                        found.push((*di, *si));
                    }
                }
            }
        }
    }
    for (di, si) in found {
        let d = &table.defs[di];
        let s = &d.sources[si];
        let secs = if s.secs > 0.0 { s.secs } else { d.secs };
        // Still touching it: keep it going without starting it again.
        if !g.fx.refresh(&d.id, secs, s.level) {
            let _ = give(g, &d.id, secs, s.level);
        }
    }
}

/// One frame of effects on the player: sources from the blocks around,
/// water, mods' ticks, then time, damage, healing and hunger.
pub fn step(g: &mut Session, dt: f32, audio: &mut Audio) {
    super::sync::send_if_changed(g);
    if g.dead || !(dt > 0.0) {
        return;
    }
    let survival = g.mode == GameMode::Survival;
    if survival {
        touch(g);
    }
    if g.fx.list.is_empty() {
        g.fx.changed = false;
        g.fx.beat = 0.0;
        return;
    }
    if wet(g) {
        for id in g.fx.extinguish(&lookup) {
            ended(g, &id);
        }
    }
    // The mods' code is told of each effect that is running: native code
    // every frame, scripts about once a second.
    g.fx.beat += dt;
    let script_beat = g.fx.beat >= 1.0 && crate::script::bridge::live();
    if g.fx.beat >= 1.0 {
        g.fx.beat = 0.0;
    }
    if script_beat || crate::native::active().any(|(_, n)| n.effect_tick.is_some()) {
        let running: Vec<(String, f32, i32)> = g.fx.list.iter().map(|a| (a.id.clone(), a.secs, a.level)).collect();
        for (id, secs, level) in running {
            for (_, n) in crate::native::active() {
                if let Some(hook) = n.effect_tick {
                    hook(g, &id, secs, level, dt);
                }
            }
            if script_beat {
                crate::script::effects_bridge::tick(g, &id, secs, level);
            }
        }
    }
    let t = g.fx.tick(dt, &lookup);
    if survival {
        if t.hard > 0 {
            hurt(g, t.hard, true, audio);
        }
        if t.soft > 0 {
            hurt(g, t.soft, false, audio);
        }
        if t.heal > 0 && !g.dead {
            g.health = (g.health + t.heal).min(MAX_HEALTH);
        }
        if t.hunger != 0 {
            g.hunger = (g.hunger - t.hunger).clamp(0, MAX_HUNGER);
        }
    }
    for id in t.ended {
        ended(g, &id);
    }
    g.fx.changed = false;
}

/// Something was raised that effects may start from (`from eat ...`,
/// `from event ...`). Called for every script event.
pub fn on_event(g: &mut Session, event: &str, what: &str) {
    let table = &reg().fx;
    for (d, s) in table.sources(SourceKind::Event) {
        if s.id != event || !(s.what.is_empty() || s.what == what) {
            continue;
        }
        if s.chance < 100 && g.fx.roll() >= s.chance {
            continue;
        }
        let _ = give(g, &d.id, s.secs, s.level);
    }
}

/// A creature of kind `kind` (an index into the entities) bit or shot the
/// player: effects that start from it.
pub fn on_bitten(g: &mut Session, kind: usize) {
    let Some(e) = reg().entities.get(kind) else { return };
    let table = &reg().fx;
    for (d, s) in table.sources(SourceKind::Entity) {
        if s.id != e.id {
            continue;
        }
        if s.chance < 100 && g.fx.roll() >= s.chance {
            continue;
        }
        let _ = give(g, &d.id, s.secs, s.level);
    }
}

/// The player struck creature number `i` with the item `item_id` in hand:
/// the effects that blow gives it.
pub fn on_strike(g: &mut Session, i: usize, item_id: &str) {
    let table = &reg().fx;
    for (d, s) in table.sources(SourceKind::Hit) {
        if s.id != item_id {
            continue;
        }
        if s.chance < 100 && g.fx.roll() >= s.chance {
            continue;
        }
        let secs = if s.secs > 0.0 { s.secs } else { d.secs };
        if let Some(m) = g.mobs.list.get_mut(i) {
            m.fx.give(d, secs, s.level);
        }
    }
}

// ----------------------------------------------------------------- creatures

/// One step of effects on a creature: damage, healing, putting out fires,
/// and its walking speed. `base_speed` is its speed scale with no effects.
/// Returns the health to add (negative: to take).
pub fn tick_mob(fx: &mut Effects, dt: f32, in_water: bool, base_speed: f32, speed_scale_out: &mut f32) -> i32 {
    if in_water {
        fx.extinguish(&lookup);
    }
    let t = fx.tick(dt, &lookup);
    if fx.changed {
        *speed_scale_out = base_speed * speed_scale(fx, &lookup);
        fx.changed = false;
    }
    t.heal - t.hard - t.soft
}

/// One step of a creature's air: whether its head is under water and what
/// its effects say about breathing. Returns the health points that drowning
/// takes.
pub fn tick_mob_air(fx: &Effects, air: &mut super::air::Air, dt: f32, under: bool) -> i32 {
    let (a, hurt) = super::air::step(*air, dt, under, super::body::can_breathe(fx, &lookup));
    *air = a;
    hurt
}

// ------------------------------------------------------------------ /effect

fn say_given(g: &Session, id: &str) -> String {
    let (Some(d), Some(a)) = (lookup(id), g.fx.get(id)) else { return String::new() };
    alloc::format!("{} {} {}", d.name, roman(a.level), clock(a.secs)).to_ascii_uppercase()
}

fn known(prefix: &str) -> String {
    let ids: Vec<&str> = reg().fx.defs.iter().map(|d| d.id.as_str()).collect();
    alloc::format!("{}{}", prefix, ids.join(" ")).to_ascii_uppercase()
}

/// `/effect give ID [SECONDS] [LEVEL]`, `/effect clear [ID]`,
/// `/effect list`, `/effect mob ID [SECONDS] [LEVEL]`.
pub fn command(g: &mut Session, words: &[&str]) -> String {
    let arg = |k: usize| words.get(k).copied().unwrap_or("");
    match arg(1) {
        "give" | "add" => {
            if arg(2).is_empty() {
                return known("/EFFECT GIVE ID [SECONDS] [LEVEL]: ");
            }
            let secs = if arg(3).is_empty() { 0.0 } else { arg(3).parse::<f32>().unwrap_or(-1.0) };
            let level = if arg(4).is_empty() { 1 } else { arg(4).parse::<i32>().unwrap_or(0) };
            if secs < 0.0 || level < 1 {
                return String::from("/EFFECT GIVE ID [SECONDS] [LEVEL]  (LEVEL 1 OR MORE)");
            }
            let Some(def) = lookup(&arg(2).to_ascii_lowercase()) else { return known("NO SUCH EFFECT. KNOWN: ") };
            let id = def.id.clone();
            match give(g, &id, secs, level) {
                Ok(Given::Unchanged) => alloc::format!("{} IS ALREADY STRONGER", id.to_ascii_uppercase()),
                Ok(_) => say_given(g, &id),
                Err(e) => e,
            }
        }
        "clear" | "remove" => {
            if arg(2).is_empty() {
                let n = g.fx.list.len();
                clear_all(g);
                alloc::format!("CLEARED {}", n)
            } else if lookup(&arg(2).to_ascii_lowercase()).map_or(false, |d| clear(g, &d.id)) {
                String::from("CLEARED")
            } else {
                String::from("NOT ACTIVE")
            }
        }
        "list" | "" => {
            if g.fx.list.is_empty() {
                return String::from("NO EFFECTS");
            }
            let parts: Vec<String> = g.fx.list.iter().filter_map(|a| lookup(&a.id).map(|d| alloc::format!("{} {} {}", d.name, roman(a.level), clock(a.secs)))).collect();
            parts.join(", ").to_ascii_uppercase()
        }
        // The creature in front of you.
        "mob" => {
            let id = arg(2).to_ascii_lowercase();
            let Some(d) = lookup(&id) else { return known("NO SUCH EFFECT. KNOWN: ") };
            let secs = (if arg(3).is_empty() { d.secs } else { arg(3).parse::<f32>().unwrap_or(d.secs) }).clamp(1.0, MAX_SECS);
            let level = (if arg(4).is_empty() { 1 } else { arg(4).parse::<i32>().unwrap_or(1) }).clamp(1, MAX_LEVEL);
            let dir = forward(g.player.yaw, g.player.pitch);
            match g.mobs.hit_test(g.player.eye(), dir, 8.0) {
                Some((i, _)) => {
                    if let Some(m) = g.mobs.list.get_mut(i) {
                        m.fx.give(d, secs, level);
                    }
                    String::from("DONE")
                }
                None => String::from("NO CREATURE IN SIGHT"),
            }
        }
        _ => String::from("/EFFECT GIVE ID [SECONDS] [LEVEL]   /EFFECT CLEAR [ID]   /EFFECT LIST   /EFFECT MOB ID"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::effects::data::parse_text;

    fn defs() -> Vec<EffectDef> {
        parse_text(
            "effect slowness\n  speed -15\n\
             effect speed\n  speed 20\n  jump 10\n\
             effect weakness\n  melee -25\n\
             effect strength\n  melee 30\n\
             effect poison\n  heart poison\n\
             effect wither\n  heart wither\n\
             effect frozen\n  heart frozen\n\
             effect burning\n  flame\n\
             effect night_vision\n  vision\n",
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
    fn speed_is_the_sum_of_the_effects_times_their_levels() {
        let defs = defs();
        let look = |id: &str| defs.iter().find(|d| d.id == id);
        assert_eq!(speed_scale(&Effects::default(), &look), 1.0);
        assert!((speed_scale(&with(&defs, &[("slowness", 1)]), &look) - 0.85).abs() < 1e-5);
        assert!((speed_scale(&with(&defs, &[("slowness", 2)]), &look) - 0.70).abs() < 1e-5);
        assert!((speed_scale(&with(&defs, &[("slowness", 1), ("speed", 1)]), &look) - 1.05).abs() < 1e-5);
        // However slow, it is not stopped, and however fast, it is held.
        assert!((speed_scale(&with(&defs, &[("slowness", 20)]), &look) - 0.05).abs() < 1e-5);
        assert_eq!(speed_scale(&with(&defs, &[("speed", 100)]), &look), 4.0);
        assert!((jump_scale(&with(&defs, &[("speed", 2)]), &look) - 1.2).abs() < 1e-5);
    }

    #[test]
    fn weakness_and_strength_change_the_blow_but_never_to_nothing() {
        let defs = defs();
        let look = |id: &str| defs.iter().find(|d| d.id == id);
        assert_eq!(melee_damage(&Effects::default(), &look, 6), 6);
        assert_eq!(melee_damage(&with(&defs, &[("strength", 1)]), &look, 10), 13);
        assert_eq!(melee_damage(&with(&defs, &[("weakness", 1)]), &look, 8), 6);
        assert_eq!(melee_damage(&with(&defs, &[("weakness", 4)]), &look, 8), 1, "-100% still leaves a scratch");
        assert_eq!(melee_damage(&with(&defs, &[("weakness", 1)]), &look, 0), 0);
        assert_eq!(melee_damage(&with(&defs, &[("weakness", 1), ("strength", 1)]), &look, 20), 21);
    }

    #[test]
    fn the_hearts_take_the_darkest_style_among_the_effects() {
        let defs = defs();
        let look = |id: &str| defs.iter().find(|d| d.id == id);
        assert_eq!(heart_style(&Effects::default(), &look), HeartStyle::Normal);
        assert_eq!(heart_style(&with(&defs, &[("frozen", 1)]), &look), HeartStyle::Frozen);
        assert_eq!(heart_style(&with(&defs, &[("frozen", 1), ("poison", 1)]), &look), HeartStyle::Poison);
        assert_eq!(heart_style(&with(&defs, &[("poison", 1), ("wither", 1), ("frozen", 1)]), &look), HeartStyle::Wither);
        assert!(!on_fire(&with(&defs, &[("poison", 1)]), &look));
        assert!(on_fire(&with(&defs, &[("poison", 1), ("burning", 1)]), &look));
        assert_eq!(night_vision(&with(&defs, &[("night_vision", 1)]), &look), 1.0);
        assert_eq!(night_vision(&Effects::default(), &look), 0.0);
    }

    #[test]
    fn a_creature_on_fire_loses_health_until_it_swims() {
        let defs = parse_text("effect burning\n  damage 2\n  extinguish\n  flame\neffect slowness\n  speed -50\n").0;
        let look = |id: &str| defs.iter().find(|d| d.id == id);
        let mut fx = Effects::default();
        fx.give(&defs[0], 10.0, 1);
        fx.give(&defs[1], 10.0, 1);
        // Same working as tick_mob, with the test's own definitions.
        let t = fx.tick(1.0, &look);
        assert_eq!(t.hard, 2);
        assert_eq!(fx.extinguish(&look), ["burning"]);
        assert!(fx.has("slowness"));
        assert!((speed_scale(&fx, &look) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn the_shipped_effects_behave_as_the_docs_say() {
        let r = reg();
        let look = |id: &str| r.fx.get(id);
        // Burning: about a heart (2 points) a second.
        let mut fx = Effects::default();
        fx.give(r.fx.get("burning").unwrap(), 4.0, 1);
        let mut hard = 0;
        for _ in 0..16 {
            hard += fx.tick(0.25, &look).hard;
        }
        assert_eq!(hard, 8);
        // Poison cannot kill.
        let mut fx = Effects::default();
        fx.give(r.fx.get("poison").unwrap(), 5.0, 1);
        let t = fx.tick(1.0, &look);
        assert!(t.soft > 0 && t.hard == 0);
        // Slowness slows, speed quickens.
        let mut fx = Effects::default();
        fx.give(r.fx.get("slowness").unwrap(), 5.0, 1);
        assert!(speed_scale(&fx, &look) < 1.0);
        let mut fx = Effects::default();
        fx.give(r.fx.get("speed").unwrap(), 5.0, 1);
        assert!(speed_scale(&fx, &look) > 1.0);
        // Absorption pads.
        let mut fx = Effects::default();
        fx.give(r.fx.get("absorption").unwrap(), 5.0, 1);
        assert!(fx.absorb > 0);
    }

    #[test]
    fn the_newer_effects_are_shipped_and_do_what_the_docs_say() {
        use crate::game::effects::{air, body};
        let r = reg();
        let look = |id: &str| r.fx.get(id);
        let with = |id: &str, level: i32| {
            let mut fx = Effects::default();
            fx.give(r.fx.get(id).unwrap_or_else(|| panic!("no effect {id}")), 30.0, level);
            fx
        };
        assert!(body::mine_scale(&with("haste", 1), &look) > 1.0);
        assert!(body::mine_scale(&with("mining_fatigue", 1), &look) < 1.0);
        assert!(body::lift_speed(&with("levitation", 1), &look) > 0.0);
        let blind = with("blindness", 1);
        assert!(body::blindness(&blind, &look) > 0.5 && body::tint(&blind, &look).is_some());
        assert!(body::tint(&with("frozen", 1), &look).is_some(), "freezing tints the screen blue");
        assert!(!body::can_breathe(&Effects::default(), &look));
        let breathing = with("water_breathing", 1);
        assert!(body::can_breathe(&breathing, &look));
        // Fourteen seconds under water: it takes damage, unless it breathes.
        let (_, hurt) = air::step(air::Air::FULL, 14.0, true, false);
        assert!(hurt > 0);
        let (_, hurt) = air::step(air::Air::FULL, 14.0, true, body::can_breathe(&breathing, &look));
        assert_eq!(hurt, 0);
        // Decay is wither-like: dark hearts, half a point a second, and it can kill.
        assert_eq!(heart_style(&with("decay", 1), &look), HeartStyle::Wither);
        let mut fx = with("decay", 1);
        let hard: i32 = (0..16).map(|_| fx.tick(0.25, &look).hard).sum();
        assert_eq!(hard, 2);
        // Freezing is the frozen effect: slow, blue hearts.
        assert_eq!(heart_style(&with("frozen", 1), &look), HeartStyle::Frozen);
        assert!(speed_scale(&with("frozen", 1), &look) < 1.0);
    }

    #[test]
    fn other_names_find_the_effects() {
        assert_eq!(lookup("freezing").map(|d| d.id.as_str()), Some("frozen"));
        assert_eq!(lookup("fatigue").map(|d| d.id.as_str()), Some("mining_fatigue"));
        assert_eq!(lookup("blind").map(|d| d.id.as_str()), Some("blindness"));
        assert_eq!(lookup("poison").map(|d| d.id.as_str()), Some("poison"));
        assert!(lookup("nonsense").is_none());
        // Every alias leads somewhere.
        for (alias, id) in ALIASES {
            assert!(reg().fx.get(id).is_some(), "{alias} leads to {id}, which is not an effect");
        }
    }

    #[test]
    fn the_new_effects_are_saved_with_the_player_and_read_back() {
        let r = reg();
        let look = |id: &str| r.fx.get(id);
        let mut fx = Effects::default();
        for (id, secs, level) in [("levitation", 7.5, 2), ("water_breathing", 120.0, 1), ("blindness", 4.0, 1), ("haste", 60.0, 3)] {
            fx.give(r.fx.get(id).unwrap(), secs, level);
        }
        let back = Effects::from_text(&fx.to_text(), fx.absorb, &look);
        assert_eq!(back.ids(), ["levitation", "water_breathing", "blindness", "haste"]);
        assert_eq!((back.level("haste"), back.get("levitation").unwrap().secs), (3, 7.5));
    }
}
