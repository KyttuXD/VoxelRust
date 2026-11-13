//! What a status effect is: the data an `effect <id>` block in a pack gives
//! (see docs/DATA.md, "Effects"). Reading the block is a pure function of its
//! lines, so it is tested without a registry.
//!
//! ```text
//! effect poison
//!   name Poison
//!   kind bad
//!   damage 1          health points a second at level 1 (level 2 = double)
//!   lethal no         poison never takes the last heart
//!   colour 120 200 60
//!   heart poison
//!   secs 12           how long when a command or source does not say
//!   from entity spider 8 1      a spider's bite
//!   from eat raw_fish 6 1 20    eating it: 20 in 100 times
//! ```

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Whether an effect is welcome. Only the frame of its HUD icon differs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nature {
    Good,
    Bad,
    Neutral,
}

/// How the hearts are drawn while the effect lasts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HeartStyle {
    Normal,
    Poison,
    Wither,
    Frozen,
}

impl HeartStyle {
    /// Which of the styles wins when several effects want one.
    pub fn priority(self) -> u8 {
        match self {
            HeartStyle::Normal => 0,
            HeartStyle::Frozen => 1,
            HeartStyle::Poison => 2,
            HeartStyle::Wither => 3,
        }
    }

    /// Position in tables of the four styles.
    pub fn index(self) -> usize {
        match self {
            HeartStyle::Normal => 0,
            HeartStyle::Poison => 1,
            HeartStyle::Wither => 2,
            HeartStyle::Frozen => 3,
        }
    }

    pub fn parse(word: &str) -> Option<HeartStyle> {
        Some(match word {
            "normal" | "red" | "none" => HeartStyle::Normal,
            "poison" | "green" => HeartStyle::Poison,
            "wither" | "black" | "dark" => HeartStyle::Wither,
            "frozen" | "frost" | "cold" | "blue" => HeartStyle::Frozen,
            _ => return None,
        })
    }
}

/// What starts an effect by itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceKind {
    /// Standing in, on or against a block (`id` is the block's id).
    Block,
    /// Being bitten or shot by a creature (`id` is the creature's id).
    Entity,
    /// Striking a creature with an item in hand (`id` is the item's id): it
    /// is the creature that gets the effect.
    Hit,
    /// A script event was raised (`id` is the event, `what` what it was
    /// about; empty `what` matches any): `eat`, `wake`, `kill`...
    Event,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Source {
    pub kind: SourceKind,
    pub id: String,
    pub what: String,
    /// Seconds; 0 means the effect's own default.
    pub secs: f32,
    pub level: i32,
    /// Out of 100, for the sources that happen once (not blocks).
    pub chance: u32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct EffectDef {
    pub id: String,
    pub name: String,
    pub nature: Nature,
    /// Health points a second at level 1, times the level.
    pub damage: f32,
    /// Whether that damage can kill (otherwise it stops at half a heart).
    pub lethal: bool,
    /// Health points a second given back at level 1, times the level.
    pub heal: f32,
    /// Percent change of walking speed per level (negative slows).
    pub speed: i32,
    /// Percent change of jump height per level.
    pub jump: i32,
    /// Percent change of the damage dealt in melee per level.
    pub melee: i32,
    /// Hunger points a second lost at level 1 (negative: gained).
    pub hunger: f32,
    /// Absorption (health points of padding) per level.
    pub absorb: i32,
    /// Night vision.
    pub vision: bool,
    /// Water puts it out.
    pub extinguish: bool,
    pub colour: [u8; 3],
    pub heart: HeartStyle,
    /// A flame is shown beside the hearts.
    pub flame: bool,
    /// Default length in seconds.
    pub secs: f32,
    /// Times a second damage and healing are dealt in lumps.
    pub ticks: f32,
    /// Name of the icon picture (a texture id); empty for `effect_<id>`.
    pub icon_name: String,
    /// The icon's tile, found when the registry is built.
    pub icon: Option<u16>,
    pub sources: Vec<Source>,
}

impl EffectDef {
    /// An effect that does nothing by itself.
    pub fn plain(id: &str) -> EffectDef {
        EffectDef {
            id: id.to_string(),
            name: id.to_string(),
            nature: Nature::Neutral,
            damage: 0.0,
            lethal: true,
            heal: 0.0,
            speed: 0,
            jump: 0,
            melee: 0,
            hunger: 0.0,
            absorb: 0,
            vision: false,
            extinguish: false,
            colour: [200, 200, 200],
            heart: HeartStyle::Normal,
            flame: false,
            secs: 30.0,
            ticks: 1.0,
            icon_name: String::new(),
            icon: None,
            sources: Vec::new(),
        }
    }
}

/// Longest an effect may be given for: a day.
pub const MAX_SECS: f32 = 86_400.0;
/// Strongest level.
pub const MAX_LEVEL: i32 = 255;

fn number(words: &[String], k: usize) -> Option<f32> {
    words.get(k).and_then(|w| w.parse::<f32>().ok()).filter(|v| *v == *v)
}

fn whole(words: &[String], k: usize) -> Option<i32> {
    words.get(k).and_then(|w| w.parse::<i32>().ok())
}

fn yes(words: &[String]) -> bool {
    !matches!(words.first().map(|w| w.as_str()), Some("no" | "false" | "0" | "off"))
}

/// Read one `from <what> <id> [secs] [level] [chance]` line.
fn parse_source(words: &[String]) -> Option<Source> {
    let what = words.first()?.as_str();
    let id = words.get(1)?.to_ascii_lowercase();
    let (kind, id, event_what) = match what {
        "block" => (SourceKind::Block, id, String::new()),
        "entity" | "bite" | "creature" => (SourceKind::Entity, id, String::new()),
        "hit" | "strike" | "weapon" => (SourceKind::Hit, id, String::new()),
        // Eating (or drinking, or using up) an item is the `eat` event on it.
        "eat" | "item" | "drink" => (SourceKind::Event, String::from("eat"), id),
        // `event name` or `event name:what`.
        "event" => match id.split_once(':') {
            Some((e, w)) => (SourceKind::Event, e.to_string(), w.to_string()),
            None => (SourceKind::Event, id, String::new()),
        },
        _ => return None,
    };
    Some(Source {
        kind,
        id,
        what: event_what,
        secs: number(words, 2).unwrap_or(0.0).clamp(0.0, MAX_SECS),
        level: whole(words, 3).unwrap_or(1).clamp(1, MAX_LEVEL),
        chance: whole(words, 4).unwrap_or(100).clamp(1, 100) as u32,
    })
}

/// Make an effect from the lines indented under `effect <id>`. What is not
/// understood is reported in `warnings`, with `at` (where the block began).
pub fn parse_def(id: &str, at: &str, props: &[(String, Vec<String>)], warnings: &mut Vec<String>) -> EffectDef {
    let mut d = EffectDef::plain(id);
    for (key, w) in props {
        let ok = match key.as_str() {
            "name" => {
                d.name = w.join(" ");
                !d.name.is_empty()
            }
            "kind" | "nature" => match w.first().map(|x| x.as_str()) {
                Some("good" | "buff" | "positive") => {
                    d.nature = Nature::Good;
                    true
                }
                Some("bad" | "debuff" | "harm" | "negative") => {
                    d.nature = Nature::Bad;
                    true
                }
                Some("neutral" | "plain") => {
                    d.nature = Nature::Neutral;
                    true
                }
                _ => false,
            },
            "damage" => number(w, 0).map(|v| d.damage = v.clamp(0.0, 1000.0)).is_some(),
            "lethal" => {
                d.lethal = yes(w);
                true
            }
            "heal" | "regen" => number(w, 0).map(|v| d.heal = v.clamp(0.0, 1000.0)).is_some(),
            "speed" => whole(w, 0).map(|v| d.speed = v.clamp(-100, 400)).is_some(),
            "jump" => whole(w, 0).map(|v| d.jump = v.clamp(-100, 400)).is_some(),
            "melee" | "strength" => whole(w, 0).map(|v| d.melee = v.clamp(-100, 1000)).is_some(),
            "hunger" => number(w, 0).map(|v| d.hunger = v.clamp(-100.0, 100.0)).is_some(),
            "absorb" | "absorption" => whole(w, 0).map(|v| d.absorb = v.clamp(0, 200)).is_some(),
            "vision" | "nightvision" => {
                d.vision = yes(w);
                true
            }
            "extinguish" | "water" => {
                // `water clears` and `extinguish` both mean it.
                d.extinguish = !matches!(w.first().map(|x| x.as_str()), Some("no" | "false" | "0" | "off" | "keeps"));
                true
            }
            "colour" | "color" => match (whole(w, 0), whole(w, 1), whole(w, 2)) {
                (Some(r), Some(g), Some(b)) => {
                    d.colour = [r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8];
                    true
                }
                _ => false,
            },
            "heart" | "hearts" => match w.first().and_then(|x| HeartStyle::parse(x)) {
                Some(s) => {
                    d.heart = s;
                    true
                }
                None => false,
            },
            "flame" => {
                d.flame = yes(w);
                true
            }
            "secs" | "seconds" | "duration" => number(w, 0).map(|v| d.secs = v.clamp(1.0, MAX_SECS)).is_some(),
            "ticks" => number(w, 0).map(|v| d.ticks = v.clamp(0.5, 20.0)).is_some(),
            "icon" => {
                d.icon_name = w.first().cloned().unwrap_or_default();
                !d.icon_name.is_empty()
            }
            "from" => match parse_source(w) {
                Some(s) => {
                    d.sources.push(s);
                    true
                }
                None => false,
            },
            _ => {
                warnings.push(alloc::format!("{}: effect {}: `{}` is not an effect line", at, id, key));
                continue;
            }
        };
        if !ok {
            warnings.push(alloc::format!("{}: effect {}: `{} {}` is not understood", at, id, key, w.join(" ")));
        }
    }
    d
}

/// Read blocks of text written like a pack's (`effect <id>`, then indented
/// lines). For tests and tools; the game reads them through the registry.
pub fn parse_text(text: &str) -> (Vec<EffectDef>, Vec<String>) {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    let mut cur: Option<(String, Vec<(String, Vec<String>)>)> = None;
    fn finish(cur: &mut Option<(String, Vec<(String, Vec<String>)>)>, out: &mut Vec<EffectDef>, warnings: &mut Vec<String>) {
        if let Some((id, props)) = cur.take() {
            out.push(parse_def(&id, "text", &props, warnings));
        }
    }
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("");
        let mut words = line.split_whitespace();
        let Some(key) = words.next() else { continue };
        let rest: Vec<String> = words.map(|w| w.to_string()).collect();
        if key == "effect" && raw.starts_with(|c: char| !c.is_whitespace()) {
            finish(&mut cur, &mut out, &mut warnings);
            cur = Some((rest.first().cloned().unwrap_or_default(), Vec::new()));
        } else if let Some((_, props)) = cur.as_mut() {
            props.push((key.to_string(), rest));
        }
    }
    finish(&mut cur, &mut out, &mut warnings);
    (out, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_of_lines_makes_an_effect() {
        let (defs, warnings) = parse_text(
            "effect poison\n  name Deadly Poison\n  kind bad\n  damage 1.5\n  lethal no\n  heart poison\n  colour 120 200 60\n  secs 12\n  ticks 2\n  from entity Spider 8 2\n  from eat raw_fish 6 1 20\n  from block cobweb 3\n",
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        let d = &defs[0];
        assert_eq!((d.id.as_str(), d.name.as_str()), ("poison", "Deadly Poison"));
        assert_eq!(d.nature, Nature::Bad);
        assert_eq!(d.damage, 1.5);
        assert!(!d.lethal);
        assert_eq!(d.heart, HeartStyle::Poison);
        assert_eq!(d.colour, [120, 200, 60]);
        assert_eq!((d.secs, d.ticks), (12.0, 2.0));
        assert_eq!(d.sources.len(), 3);
        assert_eq!(d.sources[0], Source { kind: SourceKind::Entity, id: "spider".into(), what: String::new(), secs: 8.0, level: 2, chance: 100 });
        // Eating is the `eat` event about that item.
        assert_eq!(d.sources[1], Source { kind: SourceKind::Event, id: "eat".into(), what: "raw_fish".into(), secs: 6.0, level: 1, chance: 20 });
        assert_eq!(d.sources[2].kind, SourceKind::Block);
        assert_eq!(d.sources[2].chance, 100);
    }

    #[test]
    fn defaults_are_those_of_a_plain_harmless_effect() {
        let (defs, warnings) = parse_text("effect nothing\n");
        assert!(warnings.is_empty());
        assert_eq!(defs[0], EffectDef::plain("nothing"));
        assert!(defs[0].lethal && defs[0].secs == 30.0 && defs[0].ticks == 1.0);
    }

    #[test]
    fn nonsense_is_reported_and_values_are_kept_in_range() {
        let (defs, warnings) = parse_text("effect odd\n  damage lots\n  speed 9999\n  secs 0\n  wobble 3\n  heart purple\n  from sky 3\n  colour 300 -5 7\n");
        assert_eq!(warnings.len(), 4, "{warnings:?}");
        let d = &defs[0];
        assert_eq!(d.speed, 400);
        assert_eq!(d.secs, 1.0);
        assert_eq!(d.damage, 0.0);
        assert_eq!(d.colour, [255, 0, 7]);
    }

    #[test]
    fn events_may_name_what_they_are_about() {
        let (defs, _) = parse_text("effect a\n  from event kill:spider 5\n  from event wake 10 2\n");
        assert_eq!((defs[0].sources[0].id.as_str(), defs[0].sources[0].what.as_str()), ("kill", "spider"));
        assert_eq!((defs[0].sources[1].id.as_str(), defs[0].sources[1].what.as_str(), defs[0].sources[1].level), ("wake", "", 2));
    }
}
