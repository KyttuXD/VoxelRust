//! A server offering its mods for download: which folders to look in.

use crate::net::modsync::serve::{collect, DiskFs, ServedMod};
use alloc::string::String;
use alloc::vec::Vec;

/// The mods folders in `pack_paths` (the entries ending in `/*`), each
/// ending in `/`.
pub fn mod_folders(pack_paths: &[String]) -> Vec<String> {
    pack_paths.iter().filter_map(|p| p.strip_suffix("/*")).map(|f| alloc::format!("{}/", f)).collect()
}

/// The mods this server sends to players. `only` is the `--serve-mods` list
/// (`None`: every mod that does not say `client no`); mods switched off in
/// `off` are never sent. What there is to say is logged here.
pub fn served(pack_paths: &[String], off: &str, only: Option<&[String]>) -> Vec<ServedMod> {
    let mut log = Vec::new();
    let mut mods = collect(&DiskFs, &mod_folders(pack_paths), only, &mut log);
    mods.retain(|m| !off.split(',').any(|o| o == m.name));
    for line in log {
        logf!("voxelrust: {}", line);
    }
    mods
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_mods_folders_are_searched() {
        let paths = alloc::vec![String::from("data/pack.txt"), String::from("data/mods/*"), String::from("mods/*"), String::from("extra.txt")];
        assert_eq!(mod_folders(&paths), ["data/mods/", "mods/"]);
    }
}
