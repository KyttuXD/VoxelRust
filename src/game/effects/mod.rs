//! Status effects: poison, burning, regeneration, slowness and the rest.
//!
//! What an effect is comes from the data (`effect <id>` in a pack, read in
//! `data.rs` and kept in the registry); this file is the list of the ones
//! that are active on the player (or a creature) and how time works on them.
//! Everything here is a pure function of the list, the time passed and the
//! effect definitions (handed in as a lookup), so it is tested without a
//! world. `apply.rs` is where the effects meet the game: what they do to
//! speed, damage, hunger and the session, and the `/effect` command.
//!
//! Rules:
//! - Giving an effect that is already active keeps the stronger one: a
//!   higher level replaces it (and its time); the same level takes the
//!   longer time; a lower level changes nothing.
//! - Damage and healing are dealt in whole points: a rate of 0.5 a second
//!   takes a point every two seconds.
//! - Absorption is padding that takes damage before health does; it goes
//!   when its effect does.
//!
//! Other files here: `air.rs` (drowning), `body.rs` (mining speed,
//! levitation, blindness, breathing, screen tint), `script_api.rs` (what
//! scripts may do with effects) and `sync.rs` (telling other players).

pub mod air;
pub mod apply;
pub mod body;
pub mod data;
pub mod script_api;
pub mod sync;

pub use data::{EffectDef, HeartStyle, Nature, Source, SourceKind, MAX_LEVEL, MAX_SECS};

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Most effects active at once on one body.
pub const MAX_ACTIVE: usize = 32;

/// One effect in force.
#[derive(Clone, PartialEq, Debug)]
pub struct Active {
    pub id: String,
    /// Seconds left.
    pub secs: f32,
    /// 1 is the base strength.
    pub level: i32,
    /// Absorption this one gives (points), as of when it was given.
    pub absorb: i32,
    timer: f32,
    acc_hurt: f32,
    acc_heal: f32,
    acc_hunger: f32,
}

/// What giving an effect did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Given {
    /// It was not active before.
    Started,
    /// It was, and now is stronger.
    Raised,
    /// Same strength, now for longer.
    Extended,
    /// The one already there is as good or better.
    Unchanged,
}

/// What a stretch of time did to the effects, for the game to carry out.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Tick {
    /// Damage that may kill.
    pub hard: i32,
    /// Damage that may not take the last health point.
    pub soft: i32,
    pub heal: i32,
    /// Hunger points lost (negative: gained).
    pub hunger: i32,
    /// Effects that ran out in that time.
    pub ended: Vec<String>,
}

/// The effects active on one body.
#[derive(Clone, Debug, Default)]
pub struct Effects {
    pub list: Vec<Active>,
    /// Absorption left, in health points.
    pub absorb: i32,
    /// Set whenever the list changes (so a creature's speed is worked out
    /// again); the user of it clears it.
    pub changed: bool,
    /// What was last told to the other players (see `sync.rs`).
    pub sent: Vec<(String, u8)>,
    /// Seconds since the scripts' `on_effect_tick` was last called.
    pub beat: f32,
    rng: u32,
}

/// Whole points in an accumulator, taken out of it.
fn take_whole(acc: &mut f32) -> i32 {
    let n = *acc as i32;
    *acc -= n as f32;
    n
}

/// A level as the HUD shows it: roman up to ten, a number after that.
pub fn roman(level: i32) -> String {
    const R: [&str; 10] = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
    if (1..=10).contains(&level) {
        R[level as usize - 1].to_string()
    } else {
        alloc::format!("{}", level)
    }
}

/// A time in seconds as `m:ss`, rounded up, for the HUD.
pub fn clock(secs: f32) -> String {
    let mut s = secs.max(0.0) as i64;
    if secs > s as f32 {
        s += 1;
    }
    alloc::format!("{}:{:02}", s / 60, s % 60)
}

impl Effects {
    pub fn is_empty(&self) -> bool {
        self.list.is_empty() && self.absorb == 0
    }

    pub fn get(&self, id: &str) -> Option<&Active> {
        self.list.iter().find(|a| a.id == id)
    }

    /// The level of an effect, 0 if it is not active.
    pub fn level(&self, id: &str) -> i32 {
        self.get(id).map_or(0, |a| a.level)
    }

    pub fn has(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    /// What giving this would do, without doing it.
    pub fn judge(&self, id: &str, secs: f32, level: i32) -> Given {
        let level = level.clamp(1, MAX_LEVEL);
        if !(secs > 0.0) {
            return Given::Unchanged;
        }
        match self.get(id) {
            None => {
                if self.list.len() < MAX_ACTIVE {
                    Given::Started
                } else {
                    Given::Unchanged
                }
            }
            Some(a) if level > a.level => Given::Raised,
            Some(a) if level == a.level && secs > a.secs => Given::Extended,
            Some(_) => Given::Unchanged,
        }
    }

    /// Give an effect for `secs` seconds at `level` (1 or more).
    pub fn give(&mut self, def: &EffectDef, secs: f32, level: i32) -> Given {
        let level = level.clamp(1, MAX_LEVEL);
        let secs = if secs > MAX_SECS { MAX_SECS } else { secs };
        let result = self.judge(&def.id, secs, level);
        let absorb = def.absorb.saturating_mul(level);
        match result {
            Given::Started => self.list.push(Active { id: def.id.clone(), secs, level, absorb, timer: 0.0, acc_hurt: 0.0, acc_heal: 0.0, acc_hunger: 0.0 }),
            Given::Raised => {
                if let Some(a) = self.list.iter_mut().find(|a| a.id == def.id) {
                    a.secs = secs;
                    a.level = level;
                    a.absorb = absorb;
                }
            }
            Given::Extended => {
                if let Some(a) = self.list.iter_mut().find(|a| a.id == def.id) {
                    a.secs = secs;
                }
            }
            Given::Unchanged => return result,
        }
        self.changed = true;
        // Fresh padding fills up.
        if def.absorb > 0 && result != Given::Extended {
            self.absorb = self.absorb_cap();
        }
        result
    }

    /// Keep an effect going without starting it again: when it is active at
    /// `level` or better, make sure it has at least `secs` left. Returns
    /// whether it was (the caller gives it properly if not).
    pub fn refresh(&mut self, id: &str, secs: f32, level: i32) -> bool {
        let secs = if secs > MAX_SECS { MAX_SECS } else { secs };
        match self.list.iter_mut().find(|a| a.id == id) {
            Some(a) if a.level >= level => {
                if secs > a.secs {
                    a.secs = secs;
                }
                true
            }
            _ => false,
        }
    }

    /// Take an effect away. Returns whether it was there.
    pub fn clear(&mut self, id: &str) -> bool {
        let before = self.list.len();
        self.list.retain(|a| a.id != id);
        let gone = self.list.len() != before;
        if gone {
            self.changed = true;
            self.trim_absorb();
        }
        gone
    }

    /// Take them all away: the ids that were active.
    pub fn clear_all(&mut self) -> Vec<String> {
        let ids: Vec<String> = self.list.iter().map(|a| a.id.clone()).collect();
        if !ids.is_empty() || self.absorb != 0 {
            self.changed = true;
        }
        self.list.clear();
        self.absorb = 0;
        ids
    }

    /// The most absorption the active effects allow.
    pub fn absorb_cap(&self) -> i32 {
        self.list.iter().fold(0i32, |s, a| s.saturating_add(a.absorb))
    }

    fn trim_absorb(&mut self) {
        let cap = self.absorb_cap();
        if self.absorb > cap {
            self.absorb = cap;
        }
    }

    /// Let absorption take what it can of a blow of `amount`; returns what
    /// is left for health to take.
    pub fn absorb_hit(&mut self, amount: i32) -> i32 {
        if amount <= 0 || self.absorb <= 0 {
            return amount;
        }
        let taken = amount.min(self.absorb);
        self.absorb -= taken;
        amount - taken
    }

    /// Water puts out the effects that say so. Returns their ids.
    pub fn extinguish<'a>(&mut self, lookup: &dyn Fn(&str) -> Option<&'a EffectDef>) -> Vec<String> {
        let put_out: Vec<String> = self.list.iter().filter(|a| lookup(&a.id).map_or(false, |d| d.extinguish)).map(|a| a.id.clone()).collect();
        for id in &put_out {
            self.clear(id);
        }
        put_out
    }

    /// A pseudo-random number 0..100, for sources that happen by chance.
    pub fn roll(&mut self) -> u32 {
        if self.rng == 0 {
            self.rng = 0x9E37_79B9;
        }
        // xorshift32
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) % 100
    }

    /// Let `dt` seconds pass. Effects that run out are removed and named in
    /// the result, together with the damage, healing and hunger due. Time
    /// that an effect had not got left to run is not counted for it.
    pub fn tick<'a>(&mut self, dt: f32, lookup: &dyn Fn(&str) -> Option<&'a EffectDef>) -> Tick {
        let mut t = Tick::default();
        if !(dt > 0.0) {
            return t;
        }
        for a in self.list.iter_mut() {
            let used = if dt < a.secs { dt } else { a.secs.max(0.0) };
            a.secs -= dt;
            let Some(d) = lookup(&a.id) else {
                // Not defined any more (the content changed): drop it.
                a.secs = 0.0;
                continue;
            };
            let level = a.level as f32;
            // Damage and healing come in lumps, `ticks` of them a second.
            a.timer += used;
            let every = 1.0 / d.ticks.max(0.5);
            let mut n = (a.timer / every) as i32;
            if n < 0 {
                n = 0;
            }
            if n > 0 {
                a.timer -= n as f32 * every;
                a.acc_hurt += d.damage * level * every * n as f32;
                a.acc_heal += d.heal * level * every * n as f32;
            }
            a.acc_hunger += d.hunger * level * used;
            let hurt = take_whole(&mut a.acc_hurt);
            if d.lethal {
                t.hard += hurt;
            } else {
                t.soft += hurt;
            }
            t.heal += take_whole(&mut a.acc_heal);
            t.hunger += take_whole(&mut a.acc_hunger);
        }
        let mut ended = Vec::new();
        self.list.retain(|a| {
            if a.secs <= 0.0 {
                ended.push(a.id.clone());
                false
            } else {
                true
            }
        });
        if !ended.is_empty() {
            self.changed = true;
            self.trim_absorb();
        }
        t.ended = ended;
        t
    }

    /// The list as text for a save file: `id:seconds:level,...`.
    pub fn to_text(&self) -> String {
        let parts: Vec<String> = self.list.iter().map(|a| alloc::format!("{}:{}:{}", a.id, a.secs, a.level)).collect();
        parts.join(",")
    }

    /// Read what [`Effects::to_text`] wrote, with the absorption that was
    /// left. Effects that are not defined any more are dropped.
    pub fn from_text<'a>(text: &str, absorb: i32, lookup: &dyn Fn(&str) -> Option<&'a EffectDef>) -> Effects {
        let mut fx = Effects::default();
        for part in text.split(',') {
            let mut f = part.split(':');
            let (Some(id), Some(secs), Some(level)) = (f.next(), f.next(), f.next()) else { continue };
            let (Ok(secs), Ok(level)) = (secs.parse::<f32>(), level.parse::<i32>()) else { continue };
            if let Some(d) = lookup(id) {
                fx.give(d, secs, level);
            }
        }
        fx.absorb = absorb.clamp(0, fx.absorb_cap());
        fx.changed = false;
        fx
    }

    /// Ids of the active effects.
    pub fn ids(&self) -> Vec<String> {
        self.list.iter().map(|a| a.id.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use data::parse_text;

    fn defs() -> Vec<EffectDef> {
        parse_text(
            "effect burning\n  kind bad\n  damage 2\n  flame\n  extinguish\n  secs 5\n\
             effect poison\n  kind bad\n  damage 1\n  lethal no\n  heart poison\n\
             effect regeneration\n  kind good\n  heal 0.5\n\
             effect hunger\n  hunger 0.5\n\
             effect saturation\n  hunger -1\n\
             effect slowness\n  speed -15\n\
             effect absorption\n  absorb 4\n\
             effect ticker\n  damage 2\n  ticks 4\n",
        )
        .0
    }

    fn get<'a>(defs: &'a [EffectDef], id: &str) -> Option<&'a EffectDef> {
        defs.iter().find(|d| d.id == id)
    }

    #[test]
    fn a_new_effect_starts_and_the_stronger_or_longer_one_wins() {
        let defs = defs();
        let mut fx = Effects::default();
        let poison = get(&defs, "poison").unwrap();
        assert_eq!(fx.give(poison, 10.0, 1), Given::Started);
        // Same level, shorter: nothing changes.
        assert_eq!(fx.give(poison, 4.0, 1), Given::Unchanged);
        assert_eq!(fx.get("poison").unwrap().secs, 10.0);
        // Same level, longer: the time grows.
        assert_eq!(fx.give(poison, 20.0, 1), Given::Extended);
        assert_eq!(fx.get("poison").unwrap().secs, 20.0);
        // Higher level, even for less time: it replaces.
        assert_eq!(fx.give(poison, 5.0, 2), Given::Raised);
        assert_eq!((fx.level("poison"), fx.get("poison").unwrap().secs), (2, 5.0));
        // Lower level, even for longer: nothing.
        assert_eq!(fx.give(poison, 99.0, 1), Given::Unchanged);
        assert_eq!(fx.level("poison"), 2);
        assert_eq!(fx.list.len(), 1);
        // Nothing for no time.
        assert_eq!(fx.judge("burning", 0.0, 1), Given::Unchanged);
    }

    #[test]
    fn judging_changes_nothing() {
        let defs = defs();
        let mut fx = Effects::default();
        assert_eq!(fx.judge("poison", 5.0, 1), Given::Started);
        assert!(fx.list.is_empty());
        fx.give(get(&defs, "poison").unwrap(), 5.0, 1);
        assert_eq!(fx.judge("poison", 9.0, 1), Given::Extended);
        assert_eq!(fx.get("poison").unwrap().secs, 5.0);
    }

    #[test]
    fn damage_per_second_is_dealt_in_whole_points() {
        let defs = defs();
        let look = |id: &str| get(&defs, id);
        // Burning, 2 a second, for five seconds, in quarter seconds.
        let mut fx = Effects::default();
        fx.give(get(&defs, "burning").unwrap(), 5.0, 1);
        let (mut hard, mut ended) = (0, Vec::new());
        for _ in 0..40 {
            let t = fx.tick(0.25, &look);
            hard += t.hard;
            assert_eq!(t.soft, 0);
            ended.extend(t.ended);
        }
        assert_eq!(hard, 10, "five seconds at two points a second");
        assert_eq!(ended, ["burning"]);
        assert!(fx.list.is_empty());

        // Level doubles it: poison (1 a second, soft) at level 3 for 4 seconds is 12.
        fx.give(get(&defs, "poison").unwrap(), 4.0, 3);
        let mut soft = 0;
        for _ in 0..16 {
            soft += fx.tick(0.25, &look).soft;
        }
        assert_eq!(soft, 12);

        // Half a point a second takes a point every two seconds.
        fx.give(get(&defs, "regeneration").unwrap(), 10.0, 1);
        let heal: i32 = (0..40).map(|_| fx.tick(0.25, &look).heal).sum();
        assert_eq!(heal, 5);
    }

    #[test]
    fn ticks_split_a_second_into_smaller_lumps() {
        let defs = defs();
        let look = |id: &str| get(&defs, id);
        let mut fx = Effects::default();
        fx.give(get(&defs, "ticker").unwrap(), 10.0, 1);
        // 2 a second in 4 lumps a second: half a point each, so a point every other lump.
        let mut seen = Vec::new();
        for _ in 0..8 {
            seen.push(fx.tick(0.25, &look).hard);
        }
        assert_eq!(seen, [0, 1, 0, 1, 0, 1, 0, 1]);
    }

    #[test]
    fn an_effect_is_only_paid_for_the_time_it_had_left() {
        let defs = defs();
        let look = |id: &str| get(&defs, id);
        let mut fx = Effects::default();
        fx.give(get(&defs, "burning").unwrap(), 1.0, 1);
        // One big step of ten seconds: only the one second left counts.
        let t = fx.tick(10.0, &look);
        assert_eq!(t.hard, 2);
        assert_eq!(t.ended, ["burning"]);
        // A zero or negative step does nothing.
        fx.give(get(&defs, "burning").unwrap(), 1.0, 1);
        assert_eq!(fx.tick(0.0, &look), Tick::default());
        assert_eq!(fx.tick(-1.0, &look), Tick::default());
        assert!(fx.has("burning"));
    }

    #[test]
    fn hunger_drains_and_saturation_feeds() {
        let defs = defs();
        let look = |id: &str| get(&defs, id);
        let mut fx = Effects::default();
        fx.give(get(&defs, "hunger").unwrap(), 10.0, 2);
        let lost: i32 = (0..40).map(|_| fx.tick(0.25, &look).hunger).sum();
        assert_eq!(lost, 10, "half a point a second, twice, for ten seconds");
        fx.give(get(&defs, "saturation").unwrap(), 4.0, 1);
        let gained: i32 = (0..16).map(|_| fx.tick(0.25, &look).hunger).sum();
        assert_eq!(gained, -4);
    }

    #[test]
    fn water_puts_out_burning_and_only_burning() {
        let defs = defs();
        let look = |id: &str| get(&defs, id);
        let mut fx = Effects::default();
        fx.give(get(&defs, "burning").unwrap(), 5.0, 1);
        fx.give(get(&defs, "poison").unwrap(), 5.0, 1);
        assert_eq!(fx.extinguish(&look), ["burning"]);
        assert!(!fx.has("burning") && fx.has("poison"));
        assert!(fx.extinguish(&look).is_empty());
    }

    #[test]
    fn absorption_takes_blows_first_and_goes_with_its_effect() {
        let defs = defs();
        let look = |id: &str| get(&defs, id);
        let mut fx = Effects::default();
        fx.give(get(&defs, "absorption").unwrap(), 10.0, 2);
        assert_eq!((fx.absorb, fx.absorb_cap()), (8, 8));
        assert_eq!(fx.absorb_hit(3), 0);
        assert_eq!(fx.absorb, 5);
        assert_eq!(fx.absorb_hit(9), 4, "what the padding cannot take goes through");
        assert_eq!(fx.absorb, 0);
        assert_eq!(fx.absorb_hit(0), 0);
        // Padding left over is lost when the time is up.
        fx.give(get(&defs, "absorption").unwrap(), 10.0, 3);
        assert_eq!(fx.absorb, 12);
        fx.absorb_hit(2);
        let t = fx.tick(11.0, &look);
        assert_eq!(t.ended, ["absorption"]);
        assert_eq!(fx.absorb, 0);
        // Clearing does the same.
        fx.give(get(&defs, "absorption").unwrap(), 10.0, 1);
        assert!(fx.clear("absorption"));
        assert_eq!(fx.absorb, 0);
        assert!(!fx.clear("absorption"));
    }

    #[test]
    fn an_effect_that_is_not_defined_any_more_is_dropped() {
        let defs = defs();
        let mut fx = Effects::default();
        fx.give(get(&defs, "poison").unwrap(), 10.0, 1);
        let nothing = |_: &str| -> Option<&EffectDef> { None };
        let t = fx.tick(0.1, &nothing);
        assert_eq!(t.ended, ["poison"]);
        assert!(fx.list.is_empty());
    }

    #[test]
    fn effects_are_saved_and_read_back() {
        let defs = defs();
        let look = |id: &str| get(&defs, id);
        let mut fx = Effects::default();
        fx.give(get(&defs, "poison").unwrap(), 12.5, 2);
        fx.give(get(&defs, "absorption").unwrap(), 60.0, 1);
        fx.absorb_hit(1);
        let text = fx.to_text();
        assert_eq!(text, "poison:12.5:2,absorption:60:1");
        let back = Effects::from_text(&text, fx.absorb, &look);
        assert_eq!(back.level("poison"), 2);
        assert_eq!(back.get("poison").unwrap().secs, 12.5);
        assert_eq!(back.absorb, 3);
        assert!(!back.changed);
        // Rubbish and unknown effects are skipped.
        let back = Effects::from_text("poison:x:1,nothing:5:1,:,burning:3:1,slowness", 99, &look);
        assert_eq!(back.ids(), ["burning"]);
        assert_eq!(back.absorb, 0);
        assert!(Effects::from_text("", 0, &look).is_empty());
    }

    #[test]
    fn refreshing_keeps_an_effect_going_without_restarting_it() {
        let defs = defs();
        let mut fx = Effects::default();
        assert!(!fx.refresh("burning", 5.0, 1), "not active: the caller must give it");
        fx.give(get(&defs, "burning").unwrap(), 2.0, 2);
        assert!(fx.refresh("burning", 6.0, 1));
        assert_eq!((fx.get("burning").unwrap().secs, fx.level("burning")), (6.0, 2));
        assert!(fx.refresh("burning", 1.0, 2), "never shortens");
        assert_eq!(fx.get("burning").unwrap().secs, 6.0);
        assert!(!fx.refresh("burning", 9.0, 3), "a stronger one is a new give");
    }

    #[test]
    fn levels_read_as_roman_numerals() {
        assert_eq!((roman(1).as_str(), roman(4).as_str(), roman(10).as_str(), roman(11).as_str()), ("I", "IV", "X", "11"));
    }

    #[test]
    fn the_hud_clock_rounds_up() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(0.2), "0:01");
        assert_eq!(clock(59.0), "0:59");
        assert_eq!(clock(61.5), "1:02");
        assert_eq!(clock(-3.0), "0:00");
    }

    #[test]
    fn no_more_than_the_limit_are_kept() {
        let mut fx = Effects::default();
        for i in 0..MAX_ACTIVE + 5 {
            let d = EffectDef::plain(&alloc::format!("e{i}"));
            fx.give(&d, 5.0, 1);
        }
        assert_eq!(fx.list.len(), MAX_ACTIVE);
    }

    #[test]
    fn chances_stay_in_range() {
        let mut fx = Effects::default();
        let rolls: Vec<u32> = (0..200).map(|_| fx.roll()).collect();
        assert!(rolls.iter().all(|r| *r < 100));
        assert!(rolls.iter().any(|r| *r < 50) && rolls.iter().any(|r| *r >= 50));
    }
}
