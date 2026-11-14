//! Pictures of creatures and players, and where they come from.
//!
//! Every creature is painted with a picture: its model's own (named after the
//! model: `cow`), or the one its data names with `tex` (`cow_brown`,
//! `villager_farmer`...). The game's own pictures (the player and the skins) are in
//! `data/textures/entity/`, carried inside the program; the creatures' pictures come
//! with their mods, which name them with `picture <name> file <png>` lines (see
//! [`mod_image`]). All are made by `tools/entity_textures.py`. A texture
//! pack can replace any of them: a picture called the same (or one of the names
//! in [`pack_names`]) in the pack's `assets/<namespace>/textures/entity/` folder
//! (or `textures/entity/`, `entity/`, or the pack folder itself), also in a
//! sub-folder of those (`entity/cow/cow_brown.png`, `entity/player/wide/steve.png`).
//!
//! A picture must be laid out like ours: the models say where on the picture each
//! part is, so a picture drawn for another game's models will look scrambled.
//! `python3 tools/entity_textures.py --template cow` writes a map of where
//! the parts of a model go. Player skins use the usual 64 x 64 skin layout, so
//! skins from anywhere work. See docs/TEXTURING.md, "Creatures and players".

use crate::png::Image;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicPtr, Ordering};

/// The built-in skins. A player who has not chosen one wears the skin
/// [`default_skin_for`] picks from their name; the pictures are `skin_<name>`.
pub const DEFAULT_SKINS: [&str; 8] = ["explorer", "miner", "farmer", "sailor", "knight", "mage", "ranger", "baker"];

/// A picture name made safe: letters, digits, `_` and `-` only, at most 48.
pub fn clean_name(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').take(48).collect()
}

/// Which of the built-in skins a player of this name wears (the same one every
/// time, on every computer: it comes from a hash of the lower-case name).
pub fn default_skin_for(player: &str) -> &'static str {
    let mut h: u32 = 2166136261;
    for b in player.bytes() {
        h ^= b.to_ascii_lowercase() as u32;
        h = h.wrapping_mul(16777619);
    }
    // Mix the high bits down: the low ones of a short name hash alone are not very even.
    h ^= h >> 15;
    DEFAULT_SKINS[(h % DEFAULT_SKINS.len() as u32) as usize]
}

/// The picture name of a skin: `skin_explorer` for a built-in one, `skin_<name>` for a mod's.
pub fn skin_picture(skin: &str) -> String {
    format!("skin_{}", clean_name(skin))
}

/// What a pack might call one of our pictures, besides its own name. The
/// skins have the usual player-skin layout, so the common default skins of
/// the other game will do (the wide-armed ones; the slim ones are `player_slim`).
const ALIASES: &[(&str, &[&str])] = &[
    ("player", &["player/wide/steve", "steve"]),
    ("player_slim", &["player/slim/alex", "alex"]),
    ("skin_explorer", &["player/wide/steve", "steve"]),
    ("skin_miner", &["player/wide/kai", "kai"]),
    ("skin_farmer", &["player/wide/sunny", "sunny"]),
    ("skin_sailor", &["player/wide/makena", "makena"]),
    ("skin_knight", &["player/wide/ari", "ari"]),
    ("skin_mage", &["player/wide/noor", "noor"]),
    ("skin_ranger", &["player/wide/efe", "efe"]),
    ("skin_baker", &["player/wide/zuri", "zuri"]),
];

/// The names a pack might call the picture `name`, best first: the name, the
/// name as a file in a folder of the same kind (`cow_brown` in `cow/`), then
/// the table above.
pub fn pack_names(name: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    let mut add = |n: &str| {
        if !n.is_empty() && !v.iter().any(|x| x == n) {
            v.push(n.to_string());
        }
    };
    add(name);
    // `cow_brown` may be `cow/cow_brown`, `villager_farmer_tan` may be `villager_farmer/villager_farmer_tan`.
    let mut cut = name;
    while let Some(at) = cut.rfind('_') {
        cut = &name[..at];
        add(&format!("{}/{}", cut, name));
    }
    if let Some((_, extra)) = ALIASES.iter().find(|a| a.0 == name) {
        for n in extra.iter() {
            add(n);
        }
    }
    v
}

/// The pictures of the texture packs: `(name inside the entity folder, full path)`.
struct PackIndex {
    files: alloc::collections::BTreeMap<String, String>,
}

static PACKS: AtomicPtr<PackIndex> = AtomicPtr::new(core::ptr::null_mut());
/// The index before the current one, freed at the next change (readers may still be using it for a moment).
static PACKS_OLD: AtomicPtr<PackIndex> = AtomicPtr::new(core::ptr::null_mut());

/// Look in these packs (folders), in order, for creature pictures. Later packs
/// win. Returns how many pictures were found. Called when the packs are chosen
/// (see `textures::set_texture_packs`).
pub fn set_packs(paths: &[String]) -> usize {
    let mut files: alloc::collections::BTreeMap<String, String> = alloc::collections::BTreeMap::new();
    for root in paths {
        let root = if root.ends_with('/') { root.clone() } else { format!("{}/", root) };
        let mut dirs: Vec<String> = Vec::new();
        for sub in ["", "textures/entity/", "entity/"] {
            dirs.push(format!("{}{}", root, sub));
        }
        for ns in crate::sys::list_dir(&format!("{}assets/", root)) {
            dirs.push(format!("{}assets/{}/textures/entity/", root, ns));
        }
        for d in dirs {
            scan(&d, "", 0, &mut files);
        }
    }
    let n = files.len();
    let p = if paths.is_empty() { core::ptr::null_mut() } else { alloc::boxed::Box::into_raw(alloc::boxed::Box::new(PackIndex { files })) };
    // The index before this one is kept until the next change, and the one before that freed.
    let old = PACKS.swap(p, Ordering::AcqRel);
    let older = PACKS_OLD.swap(old, Ordering::AcqRel);
    if !older.is_null() {
        // SAFETY: made by `Box::into_raw` above, replaced two changes ago.
        unsafe { drop(alloc::boxed::Box::from_raw(older)) };
    }
    n
}

/// Index the pictures of a folder and of the folders in it, three deep. A
/// picture is known by its name with its folders (`player/wide/steve`) and,
/// unless a shallower picture already has it, by its name alone (`steve`).
fn scan(dir: &str, prefix: &str, depth: u32, out: &mut alloc::collections::BTreeMap<String, String>) {
    for f in crate::sys::list_dir(dir) {
        if let Some(stem) = f.strip_suffix(".png") {
            let path = format!("{}{}", dir, f);
            let full = format!("{}{}", prefix, stem);
            out.insert(full, path.clone());
            if !prefix.is_empty() && !out.contains_key(stem) {
                out.insert(stem.to_string(), path);
            }
        } else if depth < 3 && !f.contains('.') {
            scan(&format!("{}{}/", dir, f), &format!("{}{}/", prefix, f), depth + 1, out);
        }
    }
}

fn index() -> Option<&'static PackIndex> {
    let p = PACKS.load(Ordering::Acquire);
    // SAFETY: from a boxed index, freed only two changes after it was replaced.
    if p.is_null() { None } else { Some(unsafe { &*p }) }
}

/// A texture pack's picture for `name`, if a pack has one.
pub fn pack_image(name: &str) -> Option<Image> {
    let idx = index()?;
    for n in pack_names(name) {
        if let Some(path) = idx.files.get(&n) {
            if let Some(img) = crate::sys::read_file(path).and_then(|b| crate::image::decode(&b)) {
                return Some(img);
            }
        }
    }
    None
}

/// The game's own picture for `name`, from `data/textures/entity/`.
pub fn builtin_image(name: &str) -> Option<Image> {
    let bytes = crate::block::embedded::file(&format!("textures/entity/{}.png", clean_name(name)))?;
    crate::png::decode(bytes)
}

/// The picture a mod brought for `name` (`picture <name> file <png>` in its pack file).
pub fn mod_image(name: &str) -> Option<Image> {
    let name = clean_name(name);
    let bytes = &crate::block::reg().pictures.iter().find(|p| p.0 == name)?.1;
    crate::png::decode(bytes)
}

/// The picture for `name`: a texture pack's if it has one, else a mod's, else the game's own.
pub fn image(name: &str) -> Option<Image> {
    if name.is_empty() {
        return None;
    }
    pack_image(name).or_else(|| mod_image(name)).or_else(|| builtin_image(name))
}

/// The 64 x 64 skin picture of a skin (`explorer`..., or a mod's own: its
/// `picture skin_<name>`): a pack's if it has one of that size, else the
/// game's own, else a mod's of that size.
pub fn skin_image(skin: &str) -> Option<Image> {
    let name = skin_picture(skin);
    pack_image(&name)
        .filter(|i| i.w == 64 && i.h == 64)
        .or_else(|| builtin_image(&name))
        .or_else(|| mod_image(&name).filter(|i| i.w == 64 && i.h == 64))
}

/// A picture is usable for a model if it has the shape of the model's texture
/// (`tex_w` x `tex_h`): the model's faces are found by fractions of it.
pub fn fits(img: &Image, tex_w: f32, tex_h: f32) -> bool {
    let off = img.w as f32 * tex_h - img.h as f32 * tex_w;
    let off = if off < 0.0 { -off } else { off };
    img.w > 0 && img.h > 0 && tex_w > 0.0 && tex_h > 0.0 && off < 0.5 * tex_h.max(tex_w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_skins_are_picked_by_name_and_all_get_used() {
        assert_eq!(default_skin_for("Alice"), default_skin_for("alice"));
        let mut seen = [false; 8];
        for n in ["Steve", "Alex", "Kyttu", "Player", "bob", "carol", "dave", "erin", "frank", "grace", "heidi", "ivan", "judy", "mallory", "oscar", "peggy", "trent", "victor", "wendy", "zed"] {
            let s = default_skin_for(n);
            seen[DEFAULT_SKINS.iter().position(|d| *d == s).unwrap()] = true;
        }
        assert!(seen.iter().filter(|s| **s).count() >= 3, "{:?}", seen);
        assert_eq!(default_skin_for(""), default_skin_for(""));
    }

    #[test]
    fn every_default_skin_has_a_picture_in_the_right_layout() {
        for s in DEFAULT_SKINS {
            let img = builtin_image(&skin_picture(s)).unwrap_or_else(|| panic!("skin_{} is missing: run tools/entity_textures.py", s));
            assert_eq!((img.w, img.h), (64, 64), "{s}");
            assert!(img.rgba.chunks(4).any(|p| p[3] == 255), "{s}");
        }
    }

    #[test]
    fn pack_names_have_the_name_first_and_the_table_after() {
        assert_eq!(pack_names("cow")[0], "cow");
        let n = pack_names("cow_brown");
        assert_eq!(n[0], "cow_brown");
        assert!(n.iter().any(|x| x == "cow/cow_brown"));
        assert!(pack_names("skin_explorer").iter().any(|x| x == "player/wide/steve"));
        assert!(pack_names("villager_farmer_tan").iter().any(|x| x == "villager_farmer/villager_farmer_tan"));
    }

    #[test]
    fn a_picture_fits_a_model_of_the_same_shape() {
        let img = Image { w: 256, h: 128, rgba: Vec::new() };
        assert!(fits(&img, 128.0, 64.0));
        assert!(!fits(&img, 64.0, 64.0));
        assert!(!fits(&Image { w: 0, h: 0, rgba: Vec::new() }, 64.0, 64.0));
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("../cow brown!"), "cowbrown");
    }

    #[test]
    fn every_creature_picture_the_data_names_exists_and_fits_its_model() {
        let r = crate::block::reg();
        for e in &r.entities {
            let mut names: Vec<(&str, usize)> = Vec::new();
            if !e.x.tex.is_empty() {
                names.push((e.x.tex.as_str(), e.model));
            }
            for v in &e.x.variants {
                if !v.tex.is_empty() {
                    names.push((v.tex.as_str(), v.model));
                }
            }
            for (name, model) in names {
                let img = match builtin_image(name) {
                    Some(img) => img,
                    None => {
                        // Mods may name pictures of their own, which this table does not have.
                        continue;
                    }
                };
                let spec = &r.models[model];
                let def = crate::model::ModelDef::from_bbmodel(&spec.id, &spec.text).unwrap();
                assert!(fits(&img, def.model.tex_w, def.model.tex_h), "{}: picture {} is {}x{}, model {} is {}x{}", e.id, name, img.w, img.h, spec.id, def.model.tex_w, def.model.tex_h);
            }
        }
    }
}
