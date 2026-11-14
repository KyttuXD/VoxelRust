//! Which names and paths a server's mod download may use. Everything a server
//! sends is checked here before any of it is written to disk: a hostile
//! server must not be able to write outside `<data>/tmp/mods/`, fill the disk
//! or drop a program on the machine.

use alloc::string::String;
use alloc::vec::Vec;

/// The file types a downloaded mod may hold (the extension, lower case).
/// No program, shell script or library is among them.
pub const ALLOWED_EXT: &[&str] = &["txt", "json", "bbmodel", "png", "jpg", "gif", "bmp", "tga", "wav", "ogg", "mp3", "flac", "rs", "md", "glsl"];

/// Most mods taken from one server.
pub const MAX_MODS: usize = 64;
/// Most files in one mod.
pub const MAX_FILES: usize = 2000;
/// Largest single file, in bytes. (A picture of an HD tile is held to 2 MiB besides, see
/// `content/hd/size.rs`: the receiver looks at its declared size before it decodes it.)
pub const MAX_FILE_BYTES: usize = 4 << 20;
/// Most bytes in one mod (the sum of its files): room for a mod with HD pictures. Older
/// clients keep their lower limit and leave such a mod out.
pub const MAX_MOD_BYTES: usize = 64 << 20;
/// Most bytes in all the mods of one server.
pub const MAX_TOTAL_BYTES: usize = 96 << 20;
/// Longest relative path, and the deepest nesting of folders.
pub const MAX_PATH_LEN: usize = 160;
pub const MAX_DEPTH: usize = 8;

/// Whether `name` ends in one of the [`ALLOWED_EXT`] types.
pub fn allowed_ext(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => {
            let ext = ext.to_ascii_lowercase();
            ALLOWED_EXT.contains(&ext.as_str())
        }
        _ => false,
    }
}

/// Whether `name` is one Windows reserves for devices (CON, NUL, COM1...),
/// with or without an extension and in any case: opening such a file there
/// opens the device instead, so it is never made on any system.
pub fn is_reserved_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end_matches(' ').to_ascii_uppercase();
    match stem.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$" | "CLOCK$" => true,
        s => s.len() == 4 && (s.starts_with("COM") || s.starts_with("LPT")) && s.as_bytes()[3].is_ascii_digit(),
    }
}

/// Folders of the data folder's tmp folder that belong to the game itself:
/// a downloaded mod of that name would share them (a script's tmp_write goes
/// to tmp/<mod>/, and tmp/mods and tmp/update hold the downloads and the
/// updater's files).
fn is_shared_folder(name: &str) -> bool {
    ["mods", "update"].iter().any(|n| name.eq_ignore_ascii_case(n))
}

/// A mod's name as a folder name: letters, digits, `_` and `-`, 1 to 32 long,
/// and not a name that is special on some file systems.
pub fn clean_mod_name(name: &str) -> Option<String> {
    let ok = !name.is_empty() && name.len() <= 32 && !name.starts_with('-') && !is_reserved_name(name) && !is_shared_folder(name) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Some(String::from(name))
    } else {
        None
    }
}

/// A path inside a mod's folder, as the server names it: forward slashes,
/// relative, no `.` or `..` parts, no drive letters, no hidden names, only an
/// allowed file type. `None` if anything is wrong with it.
pub fn clean_rel_path(path: &str) -> Option<String> {
    if path.is_empty() || path.len() > MAX_PATH_LEN || path.starts_with('/') || path.ends_with('/') {
        return None;
    }
    // Backslashes, drive letters, control characters and anything a shell or
    // a file system treats specially are refused outright.
    if path.chars().any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | ' ' | '+' | '(' | ')' | ','))) {
        return None;
    }
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() > MAX_DEPTH {
        return None;
    }
    for p in &parts {
        if p.is_empty() || *p == "." || *p == ".." || p.starts_with('.') || p.len() > 64 || p.ends_with(' ') || p.ends_with('.') || is_reserved_name(p) {
            return None;
        }
    }
    if !allowed_ext(parts[parts.len() - 1]) {
        return None;
    }
    Some(String::from(path))
}

/// The folder name for a server's address: `host_port`, with every character
/// that is not a letter, digit or `-` made `_`. Never empty, never `..`.
pub fn server_key(host: &str, port: u16) -> String {
    let mut s: String = host.chars().take(60).map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    if s.is_empty() {
        s.push('_');
    }
    s.push('_');
    s.push_str(&alloc::format!("{}", port));
    s
}

/// A folder name found in `tmp/mods/` that this code could have made (the
/// same alphabet as [`server_key`]); only such folders are ever deleted.
pub fn is_server_key(name: &str) -> bool {
    !name.is_empty() && name.len() <= 80 && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_paths_pass_and_bad_ones_do_not() {
        for ok in ["pack.txt", "textures/stone.png", "scripts/a/b.rs", "models/Car.BBMODEL", "sounds/ding.ogg", "README.md"] {
            assert_eq!(clean_rel_path(ok).as_deref(), Some(ok), "{ok}");
        }
        for bad in [
            "", "/etc/passwd", "../x.txt", "a/../b.txt", "a/./b.txt", "a//b.txt", "a/", "C:/x.txt", "C:\\x.txt", "a\\b.txt", ".hidden.txt", "a/.git/x.txt", "run.exe", "lib.so", "noext", "x.sh", "x.txt.", "a b.txt ", "a/b.txt\0", "a;rm.txt", "a$b.txt", "~/x.txt", "mod.dll", "x.png/../../y.png",
        ] {
            assert_eq!(clean_rel_path(bad), None, "{bad:?}");
        }
        assert_eq!(clean_rel_path(&alloc::format!("{}b.txt", "a/".repeat(8))), None, "too deep");
        assert!(clean_rel_path(&alloc::format!("{}b.txt", "a/".repeat(7))).is_some());
        assert_eq!(clean_rel_path(&alloc::format!("{}.txt", "x".repeat(200))), None, "too long");
        // Names Windows treats as devices, folders too, whatever the case or extension.
        for bad in ["con.txt", "CON.png", "textures/nul.png", "Aux.txt", "com1.txt", "LPT9.rs", "sounds/prn.ogg", "con/x.png", "nul .txt"] {
            assert_eq!(clean_rel_path(bad), None, "{bad:?}");
        }
        assert!(clean_rel_path("console.txt").is_some() && clean_rel_path("com10.txt").is_some() && clean_rel_path("lpt.txt").is_some());
    }

    #[test]
    fn mod_names_and_server_keys_are_plain() {
        assert_eq!(clean_mod_name("earth").as_deref(), Some("earth"));
        assert_eq!(clean_mod_name("my-mod_2").as_deref(), Some("my-mod_2"));
        for bad in ["", "..", "a/b", "a b", ".x", "-x", &"x".repeat(33), "con", "NUL", "com3", "mods", "Update"] {
            assert_eq!(clean_mod_name(bad), None, "{bad:?}");
        }
        assert_eq!(server_key("play.example.com", 25590), "play_example_com_25590");
        assert_eq!(server_key("..", 1), "___1");
        assert_eq!(server_key("", 7), "__7");
        assert!(is_server_key(&server_key("a/b\\c:d", 9)));
        assert!(!is_server_key("..") && !is_server_key("") && !is_server_key("a/b") && !is_server_key(".hid"));
    }

    #[test]
    fn extensions_are_checked_case_blind() {
        assert!(allowed_ext("A.PNG") && allowed_ext("x.flac") && allowed_ext("a.b.rs"));
        assert!(!allowed_ext(".png") && !allowed_ext("png") && !allowed_ext("a.exe") && !allowed_ext("a.rs.exe"));
    }
}
