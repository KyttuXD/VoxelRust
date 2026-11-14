//! Where a server's mods are kept on this machine: `<data>/tmp/mods/<host_port>/<mod>/...`,
//! for the one session only. The folder is cleared when the player leaves
//! the server, when the game quits and, for anything left behind by a
//! crash, when the game starts.
//!
//! The decisions (what is stale, what the manifest says) are pure functions
//! over names; the functions at the bottom do the file work.

use super::announce::Cached;
use super::path::*;
use crate::sys;
use alloc::string::String;
use alloc::vec::Vec;

/// The file in a server's folder that says which mods in it are complete.
pub const MANIFEST: &str = "manifest.txt";

/// `<data>/tmp/mods/`.
pub fn root(base: &str) -> String {
    alloc::format!("{}tmp/mods/", base)
}

/// `<data>/tmp/mods/<key>/` for the key from [`server_key`].
pub fn server_dir(base: &str, key: &str) -> String {
    alloc::format!("{}{}/", root(base), key)
}

/// The manifest as text: one `name version checksum` line per complete mod
/// (`-` for no version).
pub fn manifest_text(mods: &[Cached]) -> String {
    let mut s = String::new();
    for m in mods {
        let v = if m.version.is_empty() { "-" } else { m.version.as_str() };
        s.push_str(&alloc::format!("{} {} {:016x}\n", m.name, v, m.sum));
    }
    s
}

/// The inverse of [`manifest_text`]; lines that do not read are left out.
pub fn parse_manifest(text: &str) -> Vec<Cached> {
    let mut out = Vec::new();
    for line in text.lines() {
        let w: Vec<&str> = line.split(' ').collect();
        if let [name, version, sum] = w[..] {
            if let (Some(name), Ok(sum)) = (clean_mod_name(name), u64::from_str_radix(sum, 16)) {
                let version = if version == "-" { String::new() } else { String::from(version) };
                out.push(Cached { name, version, sum });
            }
        }
    }
    out
}

/// Of the names found in `tmp/mods/`, the folders to delete: every one this
/// code could have made, except the folder of `keep` (the server being
/// joined). Anything else the player may have put there is left alone.
pub fn stale_folders(listing: &[String], keep: Option<&str>) -> Vec<String> {
    listing.iter().filter(|n| is_server_key(n) && Some(n.as_str()) != keep).cloned().collect()
}

/// Of the names found in one server's folder, those to delete: everything
/// except the manifest and the mods in `keep`.
pub fn stale_mod_dirs(listing: &[String], keep: &[String]) -> Vec<String> {
    listing.iter().filter(|n| n.as_str() != MANIFEST && !keep.contains(n)).cloned().collect()
}

// ------------------------------------------------------------ file work

/// Delete a folder and everything in it, but only a folder under a
/// `tmp/mods` path (a bug elsewhere must not be able to delete anything else).
pub fn remove_tree(path: &str) -> bool {
    let p = path.trim_end_matches(|c| c == '/' || c == '\\');
    let under = p.contains("/tmp/mods/") || p.contains("\\tmp\\mods\\") || p.starts_with("tmp/mods/") || p.ends_with("/tmp/mods") || p == "tmp/mods";
    if p.is_empty() || !under || p.split(|c| c == '/' || c == '\\').any(|part| part == "..") {
        return false;
    }
    remove_files(&alloc::format!("{}/", p), 0);
    #[cfg(not(target_arch = "wasm32"))]
    {
        // The folders are left: take them away (the path is ours, but a
        // data folder with a quote in its name is not worth the risk).
        if !p.chars().any(|c| matches!(c, '"' | '`' | '$' | '\n' | '\r' | '%' | '!')) {
            sys::run_command(&if cfg!(windows) { alloc::format!("rmdir /s /q \"{}\" 2>nul", p.replace('/', "\\")) } else { alloc::format!("rm -rf \"{}\"", p) });
        }
    }
    sys::list_dir(p).is_empty()
}

fn remove_files(dir: &str, depth: usize) {
    if depth > MAX_DEPTH + 3 {
        return;
    }
    for name in sys::list_dir(dir) {
        let path = alloc::format!("{}{}", dir, name);
        if sys::is_dir(&path) {
            remove_files(&alloc::format!("{}/", path), depth + 1);
        } else {
            sys::remove_file(&path);
        }
    }
}

/// Delete every server folder in `<data>/tmp/mods/` but `keep`'s.
pub fn clean_stale(base: &str, keep: Option<&str>) {
    let root = root(base);
    for name in stale_folders(&sys::list_dir(&root), keep) {
        remove_tree(&alloc::format!("{}{}", root, name));
    }
}

/// Delete everything this feature ever put in `<data>/tmp/mods/`.
pub fn clean_all(base: &str) {
    clean_stale(base, None);
}

/// The mods the manifest in a server's folder lists as complete and still on disk.
pub fn read_manifest(dir: &str) -> Vec<Cached> {
    let text = sys::read_file(&alloc::format!("{}{}", dir, MANIFEST)).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let present = sys::list_dir(dir);
    parse_manifest(&text).into_iter().filter(|c| present.contains(&c.name)).collect()
}

pub fn write_manifest(dir: &str, mods: &[Cached]) -> bool {
    sys::make_dirs(dir);
    sys::write_file(&alloc::format!("{}{}", dir, MANIFEST), manifest_text(mods).as_bytes())
}

/// Write a mod's files (already checked by `bundle::check`) to
/// `<dir><name>/...`, after clearing any older copy. False if anything
/// could not be written.
pub fn write_mod(dir: &str, name: &str, files: &[(String, Vec<u8>)]) -> bool {
    if clean_mod_name(name).is_none() {
        return false;
    }
    let root = alloc::format!("{}{}/", dir, name);
    remove_tree(&root);
    sys::make_dirs(&root);
    for (rel, data) in files {
        // Checked again: this is the last gate before the disk.
        if clean_rel_path(rel).is_none() {
            return false;
        }
        let path = alloc::format!("{}{}", root, rel);
        if let Some(i) = path.rfind('/') {
            sys::make_dirs(&path[..i + 1]);
        }
        if !sys::write_file(&path, data) {
            return false;
        }
    }
    true
}

/// Every file of a mod folder, as (path inside it, bytes): what a server
/// would send, or what a script host is given. Files that are not of an
/// allowed type are left out.
pub fn read_tree(dir: &str) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    walk(dir, "", 0, &mut out);
    out
}

fn walk(dir: &str, rel: &str, depth: usize, out: &mut Vec<(String, Vec<u8>)>) {
    if depth > MAX_DEPTH || out.len() > MAX_FILES {
        return;
    }
    for name in sys::list_dir(dir) {
        if name.starts_with('.') {
            continue;
        }
        let (path, rel_path) = (alloc::format!("{}{}", dir, name), alloc::format!("{}{}", rel, name));
        if sys::is_dir(&path) {
            walk(&alloc::format!("{}/", path), &alloc::format!("{}/", rel_path), depth + 1, out);
        } else if clean_rel_path(&rel_path).is_some() {
            if let Some(data) = sys::read_file(&path) {
                out.push((rel_path, data));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| String::from(*s)).collect()
    }

    #[test]
    fn the_manifest_round_trips_and_ignores_junk() {
        let m = alloc::vec![
            Cached { name: String::from("earth"), version: String::from("1.2"), sum: 0xABCD_0000_1234_5678 },
            Cached { name: String::from("nover"), version: String::new(), sum: 5 },
        ];
        let text = manifest_text(&m);
        assert_eq!(parse_manifest(&text), m);
        let junk = alloc::format!("{}../x 1 00\nonly two\nbad name 1 zz\nok - nothex\n\n", text);
        assert_eq!(parse_manifest(&junk), m);
    }

    #[test]
    fn only_folders_this_code_made_are_stale() {
        let listing = names(&["play_example_com_25590", "other_host_25591", "my.notes", ".hidden", "README.txt", "..", "a b"]);
        assert_eq!(stale_folders(&listing, None), names(&["play_example_com_25590", "other_host_25591"]));
        assert_eq!(stale_folders(&listing, Some("other_host_25591")), names(&["play_example_com_25590"]));
        assert!(stale_folders(&[], None).is_empty());
        // At startup nothing is kept; the server being joined keeps its own.
        assert!(stale_folders(&names(&["x_1"]), Some("x_1")).is_empty());
    }

    #[test]
    fn a_servers_folder_keeps_the_manifest_and_the_kept_mods() {
        let listing = names(&["manifest.txt", "a", "b", "half_written", "c"]);
        assert_eq!(stale_mod_dirs(&listing, &names(&["a", "c"])), names(&["b", "half_written"]));
        assert!(stale_mod_dirs(&names(&["manifest.txt"]), &[]).is_empty());
    }

    #[test]
    fn remove_tree_refuses_anything_outside_tmp_mods() {
        for bad in ["", "/", "/home/me", "/home/me/worlds", "tmp", "/data/tmp/mods/../worlds", "/data/tmp/other/x", "C:\\data\\worlds"] {
            assert!(!remove_tree(bad), "{bad}");
        }
    }
}
