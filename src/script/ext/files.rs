//! Small files a mod keeps for what can be fetched or made again: caches,
//! downloads. They live in `<data folder>/tmp/<mod>/` (see
//! `native::mod_tmp_dir`), and a script can reach nothing else on the disk.
//! The names are plain (letters, digits, `.`, `_`, `-`), the sizes bounded.

use super::args::{text, want};
use super::Ctx;
use crate::script::value::Value;
use crate::session::Session;
use alloc::format;
use alloc::string::String;

pub const FUNCTIONS: &[(&str, &str)] = &[
    ("tmp_read(name: &str) -> String", "the text of a file in this mod's `tmp` folder (empty if there is none, or it is not text)"),
    ("world_file_read(name: &str) -> String", "the text of a file the world's save keeps for a mod's old compiled code (`<world>.<name>`, read only, and only `vehicles.txt` and `manfromthefog.txt`), empty if there is none or the game is shared: for a script that takes over from the code and reads what the code saved once"),
    ("tmp_write(name: &str, text: &str) -> bool", "write a file in this mod's `tmp` folder; false if the name is not plain (letters, digits, `.`, `_`, `-`, up to 64 bytes), the text is over 256 KiB, or the mod already has 64 files"),
];

/// Longest file, and most files for a mod.
const MAX_BYTES: usize = 256 * 1024;
const MAX_FILES: usize = 64;
/// The files the compiled code of the mods that now run as scripts kept beside a world,
/// the only ones `world_file_read` gives (a script must not read the rest of a save).
const LEGACY: &[&str] = &["vehicles.txt", "manfromthefog.txt"];

fn plain(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

/// A mod's name as a folder name.
fn folder(mod_name: &str) -> String {
    mod_name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

pub fn call(g: &mut Session, ctx: &Ctx, name: &str, args: &[Value]) -> Result<Value, String> {
    if name == "world_file_read" {
        want(name, args, 1)?;
        let file = text(name, args, 0)?;
        if !LEGACY.contains(&file) || !g.net.is_none() {
            return Ok(Value::str(""));
        }
        return Ok(match crate::sys::read_file(&format!("{}{}", g.prefix, file)) {
            Some(b) if b.len() <= MAX_BYTES => Value::string(String::from_utf8(b).unwrap_or_default()),
            _ => Value::str(""),
        });
    }
    if ctx.data_dir.is_empty() {
        return Err(format!("`{}`: this game has no data folder to keep files in", name));
    }
    match name {
        "tmp_read" => {
            want(name, args, 1)?;
            let file = text(name, args, 0)?;
            if !plain(file) {
                return Ok(Value::str(""));
            }
            let path = format!("{}tmp/{}/{}", ctx.data_dir, folder(&ctx.mod_name), file);
            Ok(match crate::sys::read_file(&path) {
                Some(b) if b.len() <= MAX_BYTES => Value::string(String::from_utf8(b).unwrap_or_default()),
                _ => Value::str(""),
            })
        }
        "tmp_write" => {
            want(name, args, 2)?;
            let (file, body) = (text(name, args, 0)?, text(name, args, 1)?);
            if !plain(file) || body.len() > MAX_BYTES {
                return Ok(Value::Bool(false));
            }
            let dir = crate::native::mod_tmp_dir(&ctx.data_dir, &folder(&ctx.mod_name));
            let have = crate::sys::list_dir(&dir);
            if have.len() >= MAX_FILES && !have.iter().any(|f| f == file) {
                return Ok(Value::Bool(false));
            }
            Ok(Value::Bool(crate::sys::write_file(&format!("{}{}", dir, file), body.as_bytes())))
        }
        _ => Err(format!("no host function `{}` here", name)),
    }
}
