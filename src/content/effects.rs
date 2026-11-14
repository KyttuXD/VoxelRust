//! The effects of the content packs: `effect <id>` blocks, kept in the
//! registry as [`EffectTable`] (see docs/DATA.md, "Effects"). The parser in
//! `block.rs` only collects the lines of each block; they are read here,
//! with `game::effects::data`, once the textures are known.

use crate::block::{Section, TileDef};
use crate::game::effects::data::{parse_def, EffectDef, Source, SourceKind};
use alloc::string::String;
use alloc::vec::Vec;

/// Tiles of the HUD's hearts and flame, found by name when the registry is
/// built. A style is Normal, Poison, Wither, Frozen (in that order).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HudTiles {
    pub empty: Option<u16>,
    pub full: [Option<u16>; 4],
    pub half: [Option<u16>; 4],
    pub absorb_full: Option<u16>,
    pub absorb_half: Option<u16>,
    pub flame: Option<u16>,
}

#[derive(Clone, Debug, Default)]
pub struct EffectTable {
    pub defs: Vec<EffectDef>,
    pub hud: HudTiles,
    /// Block sources, flattened: the block's id, the effect's place in
    /// `defs` and the source's place in its list.
    pub block_sources: Vec<(String, usize, usize)>,
}

impl EffectTable {
    pub fn get(&self, id: &str) -> Option<&EffectDef> {
        self.defs.iter().find(|d| d.id == id)
    }

    /// Every source of one kind, with the effect it belongs to.
    pub fn sources(&self, kind: SourceKind) -> impl Iterator<Item = (&EffectDef, &Source)> {
        self.defs.iter().flat_map(|d| d.sources.iter().map(move |s| (d, s))).filter(move |(_, s)| s.kind == kind)
    }
}

/// Start (or restart, redefining the same id) the block `effect <id>` in the
/// parser's list, returning its place there.
pub fn begin(list: &mut Vec<Section>, id: &str, at: String) -> usize {
    let fresh = Section { kind: String::from("effect"), id: String::from(id), at, props: Vec::new() };
    match list.iter().position(|s| s.id == id) {
        Some(i) => {
            list[i] = fresh;
            i
        }
        None => {
            list.push(fresh);
            list.len() - 1
        }
    }
}

/// Fewest characters and most that an effect id may have: letters, digits
/// and `_`, so that it can be typed and saved.
fn good_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Make the table from the blocks the parser collected.
pub fn build(sections: &[Section], tiles: &[TileDef], warnings: &mut Vec<String>) -> EffectTable {
    let tile = |name: &str| tiles.iter().position(|t| t.id == name).map(|i| i as u16);
    let mut defs: Vec<EffectDef> = Vec::new();
    for sec in sections {
        let id = sec.id.to_ascii_lowercase();
        if !good_id(&id) {
            warnings.push(alloc::format!("{}: effect ids are letters, digits and _ (32 at most), not `{}`", sec.at, sec.id));
            continue;
        }
        let mut def = parse_def(&id, &sec.at, &sec.props, warnings);
        let icon = if def.icon_name.is_empty() { alloc::format!("effect_{}", id) } else { def.icon_name.clone() };
        def.icon = tile(&icon);
        if !def.icon_name.is_empty() && def.icon.is_none() {
            warnings.push(alloc::format!("{}: effect {}: no texture called `{}`", sec.at, id, icon));
        }
        match defs.iter().position(|d| d.id == id) {
            Some(i) => defs[i] = def,
            None => defs.push(def),
        }
    }
    let mut block_sources = Vec::new();
    for (di, d) in defs.iter().enumerate() {
        for (si, s) in d.sources.iter().enumerate() {
            if s.kind == SourceKind::Block {
                block_sources.push((s.id.clone(), di, si));
            }
        }
    }
    let style = ["", "poison_", "wither_", "frozen_"];
    let mut hud = HudTiles { empty: tile("heart_empty"), absorb_full: tile("heart_absorb_full"), absorb_half: tile("heart_absorb_half"), flame: tile("effect_flame"), ..HudTiles::default() };
    for (k, s) in style.iter().enumerate() {
        hud.full[k] = tile(&alloc::format!("heart_{}full", s));
        hud.half[k] = tile(&alloc::format!("heart_{}half", s));
    }
    EffectTable { defs, hud, block_sources }
}

#[cfg(test)]
mod tests {
    use crate::block::{reg, Registry};

    #[test]
    fn the_game_ships_its_effects_and_their_pictures() {
        let r = reg();
        for id in ["poison", "burning", "regeneration", "slowness", "speed", "hunger", "weakness", "strength", "wither", "absorption", "frozen", "saturation", "night_vision", "jump_boost"] {
            let d = r.fx.get(id).unwrap_or_else(|| panic!("no effect {id}"));
            assert!(d.icon.is_some(), "{id} has no icon tile");
        }
        let h = &r.fx.hud;
        assert!(h.empty.is_some() && h.absorb_full.is_some() && h.absorb_half.is_some() && h.flame.is_some());
        assert!(h.full.iter().all(|t| t.is_some()) && h.half.iter().all(|t| t.is_some()));
        // Poison does not kill; burning does and is put out by water; wither is dark.
        assert!(!r.fx.get("poison").unwrap().lethal);
        assert!(r.fx.get("burning").unwrap().extinguish);
        assert_eq!(r.fx.get("wither").unwrap().heart, crate::game::effects::HeartStyle::Wither);
        // Lava burns.
        assert!(r.fx.block_sources.iter().any(|b| b.0 == "lava"));
    }

    #[test]
    fn a_pack_may_define_and_redefine_effects() {
        let pack = "effect shiver\n  name Shiver\n  speed -10\n  colour 1 2 3\neffect shiver\n  name Cold Shiver\n  speed -20\neffect Bad-Id\n  speed 5\neffect odd\n  fizz 3\n";
        let r = Registry::from_sources(&[("base.txt", crate::block::BASE_DATA), ("x/pack.txt", pack)], &mut |_| None);
        let d = r.fx.get("shiver").expect("shiver");
        assert_eq!((d.name.as_str(), d.speed), ("Cold Shiver", -20));
        assert_eq!(d.colour, [200, 200, 200], "a redefinition starts afresh");
        assert!(r.fx.get("bad-id").is_none());
        assert!(r.warnings.iter().any(|w| w.contains("Bad-Id")), "{:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("fizz")), "{:?}", r.warnings);
        // Redefining a built-in keeps one of it.
        let r = Registry::from_sources(&[("base.txt", crate::block::BASE_DATA), ("x/pack.txt", "effect poison\n  damage 9\n")], &mut |_| None);
        assert_eq!(r.fx.defs.iter().filter(|d| d.id == "poison").count(), 1);
        assert_eq!(r.fx.get("poison").unwrap().damage, 9.0);
    }

    #[test]
    fn effect_lines_do_not_disturb_the_blocks_and_items_around_them() {
        let pack = "block pebble\n  name Pebble\neffect shiver\n  name Shiver\nitem trinket\n  name Trinket\n";
        let r = Registry::from_sources(&[("base.txt", crate::block::BASE_DATA), ("x/pack.txt", pack)], &mut |_| None);
        assert!(r.block("pebble").is_some());
        assert!(r.items.iter().any(|i| i.id == "trinket" && i.name == "Trinket"));
        assert!(r.fx.get("shiver").is_some());
        // An effect is not an item.
        assert!(!r.items.iter().any(|i| i.id == "shiver"));
    }
}
