//! Loading content: the mods folder, packs, importing a mod, exporting textures (moved out of app.rs).

#[allow(unused_imports)]
use super::*;

/// Read the pack files and build the content from them (after the built-in
/// data). Also returns the packs' text joined together, which is what a
/// server sends to its players.
/// Every mod in a mods folder, in name order: `<folder>/<mod>/pack.txt`
/// for a mod with its own folder of textures and models, or a lone
/// `<folder>/<mod>.txt`.
pub(super) fn mods_in(folder: &str) -> Vec<String> {
    let mut found = Vec::new();
    for name in sys::list_dir(folder) {
        let path = alloc::format!("{}{}", folder, name);
        if name.to_ascii_lowercase().ends_with(".zip") {
            // A mod in a zip: each pack inside it, by a path that reads through the archive.
            if let Some(entries) = sys::read_file(&path).and_then(|z| crate::util::zip::entries(&z)) {
                for e in entries {
                    if e.name == "pack.txt" || e.name.ends_with("/pack.txt") {
                        found.push(alloc::format!("{}!/{}", path, e.name));
                    }
                }
            }
        } else if name.ends_with(".txt") && name != "mods.txt" && !name.eq_ignore_ascii_case("README.txt") {
            found.push(path);
        } else {
            let pack = alloc::format!("{}/pack.txt", path);
            if sys::read_file(&pack).is_some() {
                found.push(pack);
            } else {
                // A mod unpacked from an archive often sits one folder down.
                for inner in sys::list_dir(&path) {
                    let pack = alloc::format!("{}/{}/pack.txt", path, inner);
                    if sys::read_file(&pack).is_some() {
                        found.push(pack);
                        break;
                    }
                }
            }
        }
    }
    found
}

/// The name a mod goes by: its folder, or its file without `.txt`, or the
/// name of its zip.
pub(super) fn mod_name(path: &str) -> String {
    let p = path.strip_suffix("/pack.txt").unwrap_or(path);
    // Inside a zip (`mods/name.zip!/folder`): the zip's name.
    let p = match p.find(".zip!") {
        Some(i) => &p[..i + 4],
        None => p,
    };
    let p = p.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(p);
    let p = p.strip_suffix(".zip").unwrap_or(p);
    String::from(p.strip_suffix(".txt").unwrap_or(p))
}

/// A file a mod names: from disk, or from inside a zip (`mods/name.zip!/inner/file`).
pub(super) fn read_mod_file(path: &str) -> Option<Vec<u8>> {
    match path.find(".zip!/") {
        Some(i) => crate::util::zip::read(&sys::read_file(&path[..i + 4])?, &path[i + 6..]),
        None => sys::read_file(path),
    }
}

/// Bring a mod in from somewhere else on the machine: a folder, a .zip,
/// or a lone pack file. It is copied into the data folder's `mods`, where
/// the game then finds it. Returns what to tell the player.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn import_mod(mods: &str, from: &str) -> String {
    let from = from.trim().trim_end_matches(|c| c == '/' || c == '\\');
    // The path goes on a command line: nothing that could end the quotes.
    if from.is_empty() || from.chars().any(|c| matches!(c, '"' | '`' | '$' | '\n' | '\r' | '%' | '!')) {
        return String::from("TYPE WHERE THE MOD IS: A FOLDER, A .ZIP OR A PACK FILE");
    }
    let name: String = mod_name(from).trim_end_matches(".zip").chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')).take(32).collect();
    if name.is_empty() {
        return String::from("THAT NAME WILL NOT DO FOR A MOD");
    }
    let dest = alloc::format!("{}/{}", mods, name);
    let win = cfg!(windows);
    sys::run_command(&if win { alloc::format!("mkdir \"{}\" 2>nul", mods.replace('/', "\\")) } else { alloc::format!("mkdir -p \"{}\"", mods) });
    let lower = from.to_ascii_lowercase();
    let ok = if lower.ends_with(".txt") {
        match sys::read_file(from) {
            Some(bytes) => sys::write_file(&alloc::format!("{}.txt", dest), &bytes),
            None => false,
        }
    } else if lower.ends_with(".zip") {
        // Windows 10 and later, macOS and most Linux systems can unpack a zip.
        let cmd = if win {
            alloc::format!("mkdir \"{0}\" 2>nul & tar -xf \"{1}\" -C \"{0}\"", dest.replace('/', "\\"), from)
        } else {
            alloc::format!("mkdir -p \"{0}\" && (unzip -o -q \"{1}\" -d \"{0}\" || tar -xf \"{1}\" -C \"{0}\")", dest, from)
        };
        sys::run_command(&cmd) == 0
    } else {
        let cmd = if win { alloc::format!("xcopy /E /I /Y /Q \"{}\" \"{}\" >nul", from, dest.replace('/', "\\")) } else { alloc::format!("rm -rf \"{1}\" && cp -R \"{0}\" \"{1}\"", from, dest) };
        sys::run_command(&cmd) == 0
    };
    if !ok {
        return String::from("COULD NOT COPY THAT. IS THE PATH RIGHT?");
    }
    let found = mods_in(&alloc::format!("{}/", mods)).iter().any(|m| mod_name(m) == name || m.contains(&alloc::format!("/{}/", name)));
    if found {
        alloc::format!("ADDED THE MOD {}", name.to_ascii_uppercase())
    } else {
        String::from("COPIED, BUT THERE IS NO PACK.TXT IN IT")
    }
}

#[cfg(target_arch = "wasm32")]
pub(super) fn import_mod(_: &str, _: &str) -> String {
    String::from("NOT POSSIBLE IN A BROWSER")
}

/// Every mod there is to switch on or off, as (name, version): those in
/// the mods folders. (No mod is built into the program.)
pub(super) fn all_mods(pack_paths: &[String]) -> Vec<(String, String)> {
    let mut mods: Vec<(String, String)> = Vec::new();
    for p in pack_paths {
        if let Some(folder) = p.strip_suffix("/*") {
            for m in mods_in(&alloc::format!("{}/", folder)) {
                let n = mod_name(&m);
                if !mods.iter().any(|x| x.0 == n) {
                    let version = read_mod_file(&m).map(|b| crate::net::modsync::bundle::read_meta(&String::from_utf8_lossy(&b)).version).unwrap_or_default();
                    mods.push((n, version));
                }
            }
        }
    }
    mods
}

pub(super) fn load_content(pack_paths: &[String], off: &str) -> (Registry, crate::client::pack_text::PackText) {
    let mut sources: Vec<(String, String)> = alloc::vec![(String::from("base.txt"), String::from(block::BASE_DATA))];
    let is_off = |name: &str| off.split(',').any(|o| o == name);
    // Mods are looked for afresh each time, so one dropped into the folder
    // while the game runs is there after a reload.
    let mut all: Vec<String> = Vec::new();
    for p in pack_paths {
        match p.strip_suffix("/*") {
            Some(folder) => all.extend(mods_in(&alloc::format!("{}/", folder)).into_iter().filter(|m| !is_off(&mod_name(m)))),
            None => all.push(p.clone()),
        }
    }
    // The same mod found in two places (beside the program and in the
    // folder the game was started from, say) is loaded once.
    let mut named: Vec<String> = Vec::new();
    all.retain(|p| {
        let n = mod_name(p);
        let fresh = !crate::native::in_mods_folder(p) || !named.contains(&n);
        named.push(n);
        fresh
    });
    for p in &all {
        if let Some(bytes) = read_mod_file(p) {
            logf!("voxelrust: content from {}", p);
            sources.push((p.clone(), String::from_utf8_lossy(&bytes).into_owned()));
        }
    }
    let refs: Vec<(&str, &str)> = sources.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    // Remember which files the packs pull in (textures, models, includes), to send to
    // players who join: only their names, the bundle reads them if it is ever made.
    let mut used: Vec<String> = Vec::new();
    let mut seen: alloc::collections::BTreeSet<String> = alloc::collections::BTreeSet::new();
    let registry = Registry::from_sources(&refs, &mut |p| {
        let data = read_mod_file(p);
        if data.is_some() && seen.insert(String::from(p)) {
            used.push(String::from(p));
        }
        data
    });
    drop(refs);
    // The scripts of the mods in use (`scripts/*.rs` in a mod's folder; not
    // read from inside a zip).
    let mut scripted: Vec<(String, Vec<(String, Vec<u8>)>)> = Vec::new();
    for p in &all {
        let Some(folder) = p.strip_suffix("pack.txt") else { continue };
        if p.contains(".zip!") {
            continue;
        }
        let dir = alloc::format!("{}scripts/", folder);
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for f in sys::list_dir(&dir) {
            if f.ends_with(".rs") {
                if let Some(b) = sys::read_file(&alloc::format!("{}{}", dir, f)) {
                    files.push((alloc::format!("scripts/{}", f), b));
                }
            }
        }
        if !files.is_empty() {
            scripted.push((mod_name(p), files));
        }
    }
    crate::script::ScriptHost::load_local(&scripted);
    let packs: Vec<(String, String)> = sources.into_iter().skip(1).collect();
    (registry, crate::client::pack_text::PackText::lazy(packs, used))
}

/// The content a server dictates: the built-in data and what it sent.
pub(super) fn server_content(pack: &[u8]) -> Registry {
    let files = if pack.is_empty() {
        Vec::new()
    } else {
        crate::save::unbundle_labels(pack).unwrap_or_else(|| {
            logf!("voxelrust: the server's content could not be read; using the built-in content");
            Vec::new()
        })
    };
    let mut sources: Vec<(&str, String)> = alloc::vec![("base.txt", String::from(block::BASE_DATA))];
    for (name, data) in &files {
        if let Some(path) = name.strip_prefix("pack:") {
            sources.push((path, String::from_utf8_lossy(data).into_owned()));
        }
    }
    let refs: Vec<(&str, &str)> = sources.iter().map(|(p, t)| (*p, t.as_str())).collect();
    // (Their files live only in memory, so their sounds cannot be decoded again later.)
    crate::util::data::reread::keep_all(true);
    let mut registry = Registry::from_sources(&refs, &mut |p| files.iter().find(|f| f.0.strip_prefix("file:") == Some(p)).map(|f| f.1.clone()));
    crate::util::data::reread::keep_all(false);
    // What a server sends may define things, but the lines kept for the
    // mods' own code (addresses to fetch from, for one) stay our own.
    registry.lines = block::reg().lines.clone();
    registry
}

/// Write every texture as `<id>.png` (16 x 16, RGBA) in `dir`, with the
/// whole atlas as `atlas.png`, a list of the ids, and a note on editing them.
pub(super) fn export_textures(r: &block::Registry, dir: &str) -> Result<usize, String> {
    let dir = if dir.ends_with('/') || dir.ends_with('\\') { String::from(dir) } else { alloc::format!("{}/", dir) };
    sys::run_command(&if cfg!(windows) { alloc::format!("mkdir \"{}\" 2>nul", dir.replace('/', "\\")) } else { alloc::format!("mkdir -p \"{}\"", dir) });
    let mut list = String::new();
    for (i, t) in r.tiles.iter().enumerate() {
        // The picture the data describes, not one already in textures/.
        let px = crate::textures::described_pixels(i, t);
        if !sys::write_file(&alloc::format!("{}{}.png", dir, t.id), &crate::png::encode(16, 16, &px)) {
            return Err(alloc::format!("cannot write {}{}.png", dir, t.id));
        }
        list.push_str(&alloc::format!("{} {}\n", i, t.id));
    }
    sys::write_file(&alloc::format!("{}atlas.png", dir), &crate::png::encode(crate::textures::ATLAS, crate::textures::ATLAS, &crate::textures::build_atlas_described(r)));
    sys::write_file(&alloc::format!("{}textures.txt", dir), list.as_bytes());
    sys::write_file(&alloc::format!("{}README.txt", dir), b"One PNG per texture, 16 x 16 pixels, RGBA (transparent pixels are holes).\nEdit any of them with an image editor and copy the ones you changed into the\n`textures` folder of the game's data folder (it is made when the game starts):\nthey replace the game's own the next time the game starts, or when packs are\nreloaded. PNG, JPEG, GIF, BMP and TGA files of any size are read (a picture of\nanother size is fitted to 16 x 16). atlas.png is every texture in one\npicture, for looking at; textures.txt lists the number and id of each one.\n");
    Ok(r.tiles.len())
}
