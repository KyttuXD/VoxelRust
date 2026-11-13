//! Keeping the program up to date from the project's nightly release.
//!
//! The release carries, beside the archives people download by hand, each
//! build's bare program and a list, `nightly.txt`:
//!
//! ```text
//! commit 1a2b3c4d5e6f
//! linux-x86_64 voxelrust-linux-x86_64.bin <sha256> <bytes>
//! ```
//!
//! A program only ever takes the file listed for its own build name (see
//! `app::BUILD`), so the build for old Macs is never replaced by the one
//! for new Macs. The file is checked against its SHA-256 before it is put
//! in the running program's place; the change takes effect at the next
//! start. Downloading is done by the system's `curl`, as for map tiles.

use crate::sys;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, Ordering};

/// Where the nightly release's files are.
pub const RELEASE: &str = "https://github.com/KyttuXD/VoxelRust/releases/download/nightly/";

/// The commit this program was built from ("dev" when not built from git).
pub const COMMIT: &str = match option_env!("VOXELRUST_COMMIT") {
    Some(c) => c,
    None => "dev",
};

/// How far the check has got, for the title screen to show.
pub const IDLE: u8 = 0;
pub const CHECKING: u8 = 1;
pub const CURRENT: u8 = 2;
pub const UPDATED: u8 = 3;
pub const AVAILABLE: u8 = 4;
pub const FAILED: u8 = 5;
static STATE: AtomicU8 = AtomicU8::new(IDLE);

pub fn state() -> u8 {
    STATE.load(Ordering::Acquire)
}

/// What to tell the player, if anything.
pub fn message() -> &'static str {
    match state() {
        CHECKING => "CHECKING FOR A NEWER VERSION...",
        CURRENT => "THIS IS THE NEWEST VERSION",
        UPDATED => "A NEWER VERSION IS INSTALLED - START THE GAME AGAIN TO USE IT",
        AVAILABLE => "A NEWER VERSION IS OUT (AUTOMATIC UPDATES ARE OFF)",
        // Not being able to look (no network, or the release is not open to
        // the public) is nothing the player need be told on every start.
        FAILED => "",
        _ => "",
    }
}

/// One line of `nightly.txt` for a build: file name, SHA-256, size.
pub fn entry<'a>(list: &'a str, build: &str) -> Option<(&'a str, &'a str, usize)> {
    list.lines().find_map(|l| {
        let mut w = l.split_whitespace();
        if w.next()? != build {
            return None;
        }
        let (file, sum, size) = (w.next()?, w.next()?, w.next()?.parse::<usize>().ok()?);
        // A plain file name and a real checksum, or nothing.
        let plain = !file.is_empty() && file.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
        if plain && sum.len() == 64 && sum.chars().all(|c| c.is_ascii_hexdigit()) && size > 10_000 && size < 200_000_000 {
            Some((file, sum, size))
        } else {
            None
        }
    })
}

/// The commit the list says the release was built from.
pub fn listed_commit(list: &str) -> Option<&str> {
    list.lines().find_map(|l| l.strip_prefix("commit ")).map(|c| c.trim()).filter(|c| !c.is_empty() && c.chars().all(|c| c.is_ascii_hexdigit()))
}

fn fetch(url: &str, to: &str, seconds: u32) -> Option<Vec<u8>> {
    let cmd = alloc::format!("curl -fsSL --connect-timeout 8 --max-time {} -A \"VoxelRust/{}\" -o \"{}\" \"{}\"", seconds, env!("CARGO_PKG_VERSION"), to, url);
    if sys::run_command(&cmd) != 0 {
        return None;
    }
    sys::read_file(to)
}

/// Look at the release and, if `install`, put a newer program in place of
/// this one. `scratch` is a folder to download into (the data folder).
/// Blocks while it downloads: call it from a thread of its own.
pub fn run(build: &str, scratch: &str, install: bool) {
    // The old-Mac build cannot do this: the `curl` of Mac OS X 10.6 has no
    // way to make the secure connection the release needs. It is updated
    // by hand (the release page says which file).
    if build.starts_with("macos-legacy") {
        return;
    }
    // Let the game get its window up first.
    sys::sleep_ms(4000);
    STATE.store(CHECKING, Ordering::Release);
    let done = |s: u8| STATE.store(s, Ordering::Release);
    let exe = sys::exe_path();
    let list_path = alloc::format!("{}nightly.txt", scratch);
    let list = match fetch(&alloc::format!("{}nightly.txt", RELEASE), &list_path, 15) {
        Some(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        None => return done(FAILED),
    };
    let newest = match listed_commit(&list) {
        Some(c) => c,
        None => return done(FAILED),
    };
    if COMMIT != "dev" && (newest.starts_with(COMMIT) || COMMIT.starts_with(newest)) {
        return done(CURRENT);
    }
    let (file, sum, size) = match entry(&list, build) {
        Some(e) => e,
        // Nothing for this kind of machine: nothing to do.
        None => return done(CURRENT),
    };
    if !install || exe.is_empty() || COMMIT == "dev" {
        // (A program built by hand from the source is left alone.)
        return done(AVAILABLE);
    }
    let fresh = alloc::format!("{}.new", exe);
    let ok = match fetch(&alloc::format!("{}{}", RELEASE, file), &fresh, 180) {
        Some(bytes) => bytes.len() == size && crate::sha256::hex(&bytes) == sum.to_ascii_lowercase(),
        None => false,
    };
    if ok && sys::replace_file(&fresh, &exe) {
        logf!("voxelrust: updated to {} (in use from the next start)", newest);
        done(UPDATED)
    } else {
        done(FAILED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_read_strictly() {
        let sum = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let list = alloc::format!("commit 1a2b3c4d5e6f\nlinux-x86_64 voxelrust-linux-x86_64.bin {} 1500000\nmacos-legacy-x86_64 ../../evil {} 1500000\nwindows-x86_64 v.exe nothex 1500000\n", sum, sum);
        assert_eq!(listed_commit(&list), Some("1a2b3c4d5e6f"));
        assert_eq!(entry(&list, "linux-x86_64"), Some(("voxelrust-linux-x86_64.bin", sum, 1_500_000)));
        // A path, a bad checksum, or a build that is not listed: nothing.
        assert_eq!(entry(&list, "macos-legacy-x86_64"), None);
        assert_eq!(entry(&list, "windows-x86_64"), None);
        assert_eq!(entry(&list, "macos-x86_64"), None);
        assert_eq!(listed_commit("commit not-a-hash\n"), None);
    }
}
