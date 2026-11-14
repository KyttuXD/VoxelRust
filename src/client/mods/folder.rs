//! The folder the player's mods go in, and the note that says how.

use crate::sys;
use alloc::string::String;

/// What `README.txt` in the mods folder says.
pub const README: &str = r#"VoxelRust mods
==============

Put each mod you want in this folder as a folder of its own:

    mods/
      mymod/
        pack.txt        the mod's description (blocks, items, recipes...)
        textures/       its pictures; also models/, sounds/ ...

A mod is a folder that holds a pack.txt (a lone NAME.txt pack file works
too). The game finds it when it starts, or when you press RELOAD in
Settings > MODS (or type /reload in a world of your own). Switch a mod on or
off in Settings > MODS; delete its folder to remove it.

No mods come with the game. Get them from their authors, or join a server:
a server sends the mods it uses when you join, keeps them in the data
folder's tmp/mods for that visit only and deletes them when you leave.
Mods that need compiled code (not just a pack.txt and scripts) are not sent;
they have to be built into the game (cargo feature `native-mods`).

Only install mods you trust: a mod can change how the game plays.

Another folder can be used with the option  --mods-dir FOLDER.
See docs/MODDING.md for how to make a mod.
"#;

/// Make the mods folder (and its README.txt, if there is none) so the
/// player has somewhere to put mods. Does nothing in a browser.
pub fn ensure(dir: &str) {
    let dir: String = if dir.ends_with('/') || dir.ends_with('\\') { String::from(dir) } else { alloc::format!("{}/", dir) };
    if dir == "/" {
        return;
    }
    sys::make_dirs(&dir);
    let readme = alloc::format!("{}README.txt", dir);
    if sys::read_file(&readme).is_none() {
        sys::write_file(&readme, README.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_readme_explains_the_folder_layout() {
        assert!(super::README.contains("pack.txt") && super::README.contains("--mods-dir") && super::README.contains("native-mods"));
        assert!(super::README.is_ascii());
    }
}
