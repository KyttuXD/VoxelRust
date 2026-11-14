//! The server's side: find the mods in the server's mods folders and turn
//! each into a bundle clients can download.
//!
//! A mod is offered unless it declares `client no` in its pack.txt, has
//! native (compiled) code (`code/mod.rs`: that cannot be sent), or breaks the
//! limits. `--serve-mods a,b,c` offers exactly the named mods instead.

use super::bundle;
use super::path::*;
use alloc::string::String;
use alloc::vec::Vec;

/// A mod ready to be sent, as the server holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct ServedMod {
    pub name: String,
    pub version: String,
    pub sum: u64,
    pub files: u16,
    /// The bundle (see [`bundle::encode`]).
    pub data: Vec<u8>,
}

impl ServedMod {
    /// What the announcement says about it.
    pub fn info(&self) -> crate::proto::ModInfo {
        crate::proto::ModInfo { name: self.name.clone(), version: self.version.clone(), sum: self.sum, size: self.data.len() as u32, files: self.files }
    }
}

/// The few things read from the disk, so that tests can use a made-up one.
pub trait Fs {
    /// Names in a folder (empty if there is none).
    fn list(&self, dir: &str) -> Vec<String>;
    fn is_dir(&self, path: &str) -> bool;
    fn read(&self, path: &str) -> Option<Vec<u8>>;
}

/// The real disk.
pub struct DiskFs;

impl Fs for DiskFs {
    fn list(&self, dir: &str) -> Vec<String> {
        crate::sys::list_dir(dir)
    }
    fn is_dir(&self, path: &str) -> bool {
        crate::sys::is_dir(path)
    }
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        crate::sys::read_file(path)
    }
}

/// Read a folder's files (paths relative to it). Files of a type that may not
/// be sent, and files over [`MAX_FILE_BYTES`] (or that cannot be read), are
/// left out and their paths are added to `left_out` so that the log can name
/// them. Hidden ones are skipped silently. `false` if there are too many files.
fn read_folder(fs: &dyn Fs, dir: &str, rel: &str, depth: usize, out: &mut Vec<(String, Vec<u8>)>, left_out: &mut Vec<String>) -> bool {
    if depth > MAX_DEPTH {
        return true;
    }
    for name in fs.list(dir) {
        if name.starts_with('.') {
            continue;
        }
        let (path, rel_path) = (alloc::format!("{}{}", dir, name), alloc::format!("{}{}", rel, name));
        if fs.is_dir(&path) {
            if !read_folder(fs, &alloc::format!("{}/", path), &alloc::format!("{}/", rel_path), depth + 1, out, left_out) {
                return false;
            }
        } else if clean_rel_path(&rel_path).is_none() {
            left_out.push(rel_path);
        } else {
            match fs.read(&path) {
                Some(d) if d.len() <= MAX_FILE_BYTES => {
                    out.push((rel_path, d));
                    if out.len() > MAX_FILES {
                        return false;
                    }
                }
                _ => left_out.push(rel_path),
            }
        }
    }
    true
}

/// Bundle the mod folder `<folder>/<name>/` (or the lone pack file
/// `<folder>/<name>.txt`). `Err` carries the reason for the log.
pub fn bundle_mod(fs: &dyn Fs, folder: &str, name: &str) -> Result<ServedMod, String> {
    let dir = alloc::format!("{}{}/", folder, name);
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut left_out: Vec<String> = Vec::new();
    if fs.is_dir(&dir) {
        if fs.is_dir(&alloc::format!("{}code", dir)) && fs.read(&alloc::format!("{}code/mod.rs", dir)).is_some() {
            return Err(String::from("it has native code, which cannot be downloaded"));
        }
        if !read_folder(fs, &dir, "", 0, &mut files, &mut left_out) {
            return Err(String::from("it has too many files"));
        }
    } else if let Some(text) = fs.read(&alloc::format!("{}{}.txt", folder, name)) {
        files.push((String::from("pack.txt"), text));
    } else {
        return Err(String::from("not found"));
    }
    let pack = files.iter().find(|f| f.0 == "pack.txt").map(|f| String::from_utf8_lossy(&f.1).into_owned());
    let pack = pack.ok_or_else(|| String::from("it has no pack.txt"))?;
    let meta = bundle::read_meta(&pack);
    let data = bundle::encode(&files).map_err(|r| String::from(r.text()))?;
    if !left_out.is_empty() {
        logf!(
            "mod {}: {} file{} not sent to clients (each must be at most {} MiB, and of a type a mod may send): {}",
            name,
            left_out.len(),
            if left_out.len() == 1 { "" } else { "s" },
            MAX_FILE_BYTES >> 20,
            left_out.join(", ")
        );
    }
    Ok(ServedMod { name: String::from(name), version: meta.version, sum: bundle::sum_of(&data), files: files.len() as u16, data })
}

/// Whether the mod in `<folder>/<name>/` asks not to be sent (`client no`).
fn declines(fs: &dyn Fs, folder: &str, name: &str) -> bool {
    let text = fs.read(&alloc::format!("{}{}/pack.txt", folder, name)).or_else(|| fs.read(&alloc::format!("{}{}.txt", folder, name)));
    text.map_or(false, |t| !bundle::read_meta(&String::from_utf8_lossy(&t)).client)
}

/// The names of the mods in a folder (folders holding a pack.txt, and lone
/// `.txt` packs), in name order. Zip files are not offered.
fn names_in(fs: &dyn Fs, folder: &str) -> Vec<String> {
    let mut out = Vec::new();
    for entry in fs.list(folder) {
        if entry.starts_with('.') {
            continue;
        }
        let path = alloc::format!("{}{}", folder, entry);
        if fs.is_dir(&path) {
            if fs.read(&alloc::format!("{}/pack.txt", path)).is_some() {
                out.push(entry);
            }
        } else if let Some(stem) = entry.strip_suffix(".txt") {
            if stem != "mods" && stem != "pack" && !stem.eq_ignore_ascii_case("README") {
                out.push(String::from(stem));
            }
        }
    }
    out
}

/// Collect the mods a server offers. `folders` are the mods folders in the
/// order the game reads them (each ending in `/`; the first copy of a name
/// wins). With `only` set, exactly those mods are offered; otherwise every
/// one that does not say `client no`. `log` gets a line for each mod left out
/// and for the total.
pub fn collect(fs: &dyn Fs, folders: &[String], only: Option<&[String]>, log: &mut Vec<String>) -> Vec<ServedMod> {
    let mut out: Vec<ServedMod> = Vec::new();
    let mut total = 0usize;
    let mut seen: Vec<String> = Vec::new();
    let mut found: Vec<(String, String)> = Vec::new();
    for f in folders {
        for n in names_in(fs, f) {
            if !seen.contains(&n) {
                seen.push(n.clone());
                found.push((f.clone(), n));
            }
        }
    }
    if let Some(only) = only {
        for want in only {
            if !seen.contains(want) {
                log.push(alloc::format!("--serve-mods: there is no mod called {}", want));
            }
        }
    }
    for (folder, name) in found {
        if clean_mod_name(&name).is_none() {
            log.push(alloc::format!("mod {} is not offered for download: its name is not a plain folder name", name));
            continue;
        }
        let wanted = match only {
            Some(list) => list.contains(&name),
            None => !declines(fs, &folder, &name),
        };
        if !wanted {
            continue;
        }
        if out.len() >= MAX_MODS {
            log.push(alloc::format!("mod {} is not offered for download: too many mods", name));
            continue;
        }
        match bundle_mod(fs, &folder, &name) {
            Ok(m) => {
                if total + m.data.len() > MAX_TOTAL_BYTES {
                    log.push(alloc::format!("mod {} is not offered for download: the mods add up to too much", name));
                    continue;
                }
                total += m.data.len();
                out.push(m);
            }
            Err(why) => log.push(alloc::format!("mod {} is not offered for download: {}", name, why)),
        }
    }
    if !out.is_empty() {
        log.push(alloc::format!("offering {} mod{} for download ({} bytes): {}", out.len(), if out.len() == 1 { "" } else { "s" }, total, out.iter().map(|m| m.name.as_str()).collect::<Vec<_>>().join(", ")));
    }
    out
}

/// Which mods clients are sent when the server starts, from the
/// `--serve-mods` list: `None` means "every mod that does not say `client no`".
pub fn parse_serve_list(arg: &str) -> Option<Vec<String>> {
    let list: Vec<String> = arg.split(',').map(|s| String::from(s.trim())).filter(|s| !s.is_empty()).collect();
    if list.is_empty() || arg.trim().eq_ignore_ascii_case("all") {
        None
    } else {
        Some(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;

    /// A disk made of a map of path -> bytes (folders are implied).
    struct Fake(BTreeMap<String, Vec<u8>>);

    impl Fake {
        fn new(files: &[(&str, &str)]) -> Fake {
            Fake(files.iter().map(|(p, d)| (String::from(*p), d.as_bytes().to_vec())).collect())
        }
    }

    impl Fs for Fake {
        fn list(&self, dir: &str) -> Vec<String> {
            let mut out: Vec<String> = Vec::new();
            for k in self.0.keys() {
                if let Some(rest) = k.strip_prefix(dir) {
                    let first = rest.split('/').next().unwrap_or("");
                    if !first.is_empty() && !out.iter().any(|o| o == first) {
                        out.push(String::from(first));
                    }
                }
            }
            out
        }
        fn is_dir(&self, path: &str) -> bool {
            let p = alloc::format!("{}/", path.trim_end_matches('/'));
            self.0.keys().any(|k| k.starts_with(&p))
        }
        fn read(&self, path: &str) -> Option<Vec<u8>> {
            self.0.get(path).cloned()
        }
    }

    fn disk() -> Fake {
        Fake::new(&[
            ("m/earth/pack.txt", "version 2.0\nfeature earth\n"),
            ("m/earth/code/mod.rs", "// native"),
            ("m/plain/pack.txt", "version 1.1\nblock x\n"),
            ("m/plain/textures/x.png", "PNG"),
            ("m/plain/scripts/go.rs", "fn go() {}"),
            ("m/plain/bin/run.exe", "MZ"),
            ("m/plain/.hidden.txt", "no"),
            ("m/serveronly/pack.txt", "client no\nblock s\n"),
            ("m/lone.txt", "block lone\n"),
            ("m/README.txt", "VoxelRust mods: the game writes this file itself, it is no mod"),
            ("m/nopack/readme.txt", "hello"),
            ("m/bad name/pack.txt", "x"),
            ("m/zipped.zip", "PK"),
            ("other/plain/pack.txt", "block other\n"),
            ("other/second/pack.txt", "version 3\n"),
        ])
    }

    #[test]
    fn the_default_offers_what_is_not_native_or_server_only() {
        let fs = disk();
        let mut log = Vec::new();
        let folders = alloc::vec![String::from("m/"), String::from("other/")];
        let mods = collect(&fs, &folders, None, &mut log);
        let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["lone", "plain", "second"]);
        let plain = mods.iter().find(|m| m.name == "plain").unwrap();
        assert_eq!(plain.version, "1.1");
        // The program and the hidden file stay behind; the first folder wins.
        let files = bundle::decode(&plain.data).unwrap();
        let mut paths: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
        paths.sort();
        assert_eq!(paths, ["pack.txt", "scripts/go.rs", "textures/x.png"]);
        assert_eq!(files.iter().find(|f| f.0 == "pack.txt").unwrap().1, b"version 1.1\nblock x\n");
        assert_eq!(plain.sum, bundle::sum_of(&plain.data));
        assert_eq!(plain.files, 3);
        assert_eq!(plain.info().size as usize, plain.data.len());
        assert!(log.iter().any(|l| l.contains("earth") && l.contains("native code")), "{log:?}");
        assert!(log.iter().any(|l| l.contains("bad name")), "{log:?}");
        assert!(!log.iter().any(|l| l.contains("serveronly")));
    }

    #[test]
    fn serve_mods_names_exactly_what_is_offered() {
        let fs = disk();
        let mut log = Vec::new();
        let folders = alloc::vec![String::from("m/")];
        let only = parse_serve_list("serveronly, plain ,ghost");
        assert_eq!(only.as_deref(), Some(&[String::from("serveronly"), String::from("plain"), String::from("ghost")][..]));
        let mods = collect(&fs, &folders, only.as_deref(), &mut log);
        let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["plain", "serveronly"]);
        assert!(log.iter().any(|l| l.contains("ghost")));
        assert_eq!(parse_serve_list(""), None);
        assert_eq!(parse_serve_list("ALL"), None);
    }

    #[test]
    fn a_lone_pack_file_is_offered_as_its_pack_txt() {
        let fs = disk();
        let m = bundle_mod(&fs, "m/", "lone").unwrap();
        assert_eq!(bundle::decode(&m.data).unwrap(), alloc::vec![(String::from("pack.txt"), b"block lone\n".to_vec())]);
        assert!(bundle_mod(&fs, "m/", "nothing").is_err());
    }

    #[test]
    fn a_file_that_is_left_out_is_named() {
        let fs = disk();
        let (mut out, mut left) = (Vec::new(), Vec::new());
        assert!(read_folder(&fs, "m/plain/", "", 0, &mut out, &mut left));
        // A program is not a type a mod may send: it is named, not sent.
        assert!(left.iter().any(|p| p == "bin/run.exe"), "{:?}", left);
        assert!(!out.iter().any(|(p, _)| p == "bin/run.exe"));
        // A hidden file is skipped without being named.
        assert!(!left.iter().any(|p| p.contains("hidden")));
    }
}
