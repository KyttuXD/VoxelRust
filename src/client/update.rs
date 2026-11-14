//! Keeping the program up to date from the project's nightly release.
//!
//! The release carries, beside the archives people download by hand, each
//! build's bare program and a list, `nightly.txt`:
//!
//! ```text
//! commit 1a2b3c4d5e6f
//! seq 42
//! linux-x86_64 voxelrust-linux-x86_64.bin <sha256> <bytes>
//! ```
//!
//! The `seq` is the release's number in order (the count of commits on the
//! release branch, set by the release script). A program is only replaced by
//! a release with a larger number, so an old list can never move a program
//! back; a program built without a number (a developer's build) is never
//! replaced at all.
//!
//! The list is signed: beside it is `nightly.txt.sig`, an Ed25519 signature
//! over the exact bytes of `nightly.txt`, made with a private key that only
//! the release machine has. The matching public key is built into the program
//! (`update_key.rs`). A list whose signature does not check is thrown away
//! unread, wherever it came from, so the files can be served by anyone, even
//! over plain HTTP: a mirror (the `update_mirror` setting, or `--update-mirror`)
//! is how a program that cannot make HTTPS connections (the build for Mac OS X
//! 10.6) is updated. Whoever serves the files can withhold an update but cannot
//! make a program install anything the key's owner did not publish.
//!
//! A program only ever takes the file listed for its own build name (see
//! `app::BUILD`), checked against its SHA-256 and size, and then run with
//! `--stamp` to make sure it is the build the list describes. Only then does
//! it take the running program's place. The new version starts when the game
//! is next quit: the update thread never restarts a game in play, and the
//! restarted copy never restarts again.

use super::update_key::UPDATE_KEY;
use crate::sys;
use crate::util::crypto::ed25519;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

/// Where the nightly release's files are.
/// The project's update mirror (the GitHub repository is private).
pub const MIRROR: &str = "https://voxelrust.games.kyttuslab.org/updates/";
pub const MIRROR_PLAIN: &str = "http://voxelrust.games.kyttuslab.org/updates/";

pub const RELEASE: &str = match option_env!("VOXELRUST_RELEASE") {
    Some(url) => url,
    None => "https://github.com/KyttuXD/VoxelRust/releases/download/nightly/",
};

/// The commit this program was built from ("dev" when not built from git).
pub const COMMIT: &str = match option_env!("VOXELRUST_COMMIT") {
    Some(c) => c,
    None => "dev",
};

/// The release's number, when the release script set one; None otherwise.
pub const SEQ: Option<u32> = match option_env!("VOXELRUST_SEQ") {
    Some(s) => parse_seq(s),
    None => None,
};

/// A release number: digits only, nine at most (the count of commits).
const fn parse_seq(s: &str) -> Option<u32> {
    let b = s.as_bytes();
    if b.is_empty() || b.len() > 9 {
        return None;
    }
    let mut value: u32 = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] < b'0' || b[i] > b'9' {
            return None;
        }
        value = value * 10 + (b[i] - b'0') as u32;
        i += 1;
    }
    Some(value)
}

/// How far the check has got, for the title screen to show.
pub const IDLE: u8 = 0;
pub const CHECKING: u8 = 1;
pub const CURRENT: u8 = 2;
pub const UPDATED: u8 = 3;
pub const AVAILABLE: u8 = 4;
pub const FAILED: u8 = 5;
/// Putting the new program in place. Quitting waits for it to finish.
pub const SWAPPING: u8 = 6;
/// A release list was found whose signature is wrong: nothing was used.
pub const REFUSED: u8 = 7;
static STATE: AtomicU8 = AtomicU8::new(IDLE);
/// The release number of the program installed by this run.
static INSTALLED_SEQ: AtomicU32 = AtomicU32::new(0);
/// The command line the program was started with (see [`remember_args`]).
static RESTART_ARGS: core::sync::atomic::AtomicPtr<Vec<String>> = core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// Keep the command line, so that a restart after an update starts the
/// same program the same way (a server stays a server, on its port).
/// `--update-now` is the test switch: never repeated.
pub fn remember_args(args: &[String]) {
    let kept: Vec<String> = args.iter().skip(1).filter(|a| a.as_str() != "--update-now").cloned().collect();
    RESTART_ARGS.store(alloc::boxed::Box::into_raw(alloc::boxed::Box::new(kept)), Ordering::Release);
}

/// The kept command line, quoted for the shell (None if an argument cannot be quoted).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // the web build never restarts
fn restart_args() -> Option<String> {
    let p = RESTART_ARGS.load(Ordering::Acquire);
    if p.is_null() {
        return Some(String::new());
    }
    // SAFETY: stored once at start-up and never freed.
    let args = unsafe { &*p };
    let mut out = String::new();
    for a in args {
        out.push(' ');
        out.push_str(&sys::shell_arg(a)?);
    }
    Some(out)
}

/// Set when this program is the copy the updater just started: it never
/// restarts again, and says so on the title screen.
static RESTART_CHILD: AtomicBool = AtomicBool::new(false);

pub fn state() -> u8 {
    STATE.load(Ordering::Acquire)
}

fn set_state(s: u8) {
    STATE.store(s, Ordering::Release);
}

/// What to tell the player, if anything.
pub fn message() -> &'static str {
    if RESTART_CHILD.load(Ordering::Acquire) {
        return "UPDATED - NOW RUNNING THE NEWEST VERSION";
    }
    match state() {
        CHECKING => "CHECKING FOR A NEWER VERSION...",
        CURRENT => "THIS IS THE NEWEST VERSION",
        UPDATED => "A NEWER VERSION IS INSTALLED - THE GAME RESTARTS WHEN YOU QUIT",
        SWAPPING => "INSTALLING A NEWER VERSION...",
        AVAILABLE => "A NEWER VERSION IS OUT (AUTOMATIC UPDATES ARE OFF)",
        REFUSED => "AN UPDATE WAS REFUSED: ITS SIGNATURE IS NOT VALID",
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
        // (Not a name that starts with a dot: `..` is a folder, not a file.)
        let plain = !file.is_empty() && !file.starts_with('.') && file.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
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

/// The release number the list gives: one `seq N` line, digits only. A list
/// with no such line, or with more than one, gives nothing.
pub fn listed_seq(list: &str) -> Option<u32> {
    let mut found = None;
    for line in list.lines() {
        if let Some(rest) = line.strip_prefix("seq ") {
            if found.is_some() {
                return None;
            }
            found = parse_seq(rest.trim());
            found?;
        }
    }
    found
}

/// Whether a release with number `listed` is newer than this program. A
/// program with no number is never replaced.
pub fn is_newer(own: Option<u32>, listed: u32) -> bool {
    matches!(own, Some(o) if listed > o)
}

/// What `--stamp` writes: which build this program is.
pub fn stamp_text(build: &str) -> String {
    let seq = match SEQ {
        Some(n) => alloc::format!("{}", n),
        None => String::from("none"),
    };
    alloc::format!("voxelrust-stamp {} {} {}", build, COMMIT, seq)
}

/// The most a list or its signature may weigh: nothing real is near this.
const LIST_LIMIT: usize = 64 * 1024;

/// A signature file: the 64 bytes as they are (what `openssl` writes), or as
/// 128 hexadecimal digits, with spaces or a line end around them.
pub fn parse_signature(bytes: &[u8]) -> Option<[u8; 64]> {
    let mut out = [0u8; 64];
    if bytes.len() == 64 {
        out.copy_from_slice(bytes);
        return Some(out);
    }
    let text = core::str::from_utf8(bytes).ok()?.trim();
    if text.len() != 128 || !text.is_ascii() {
        return None;
    }
    let digits = text.as_bytes();
    for i in 0..64 {
        let hi = (digits[i * 2] as char).to_digit(16)?;
        let lo = (digits[i * 2 + 1] as char).to_digit(16)?;
        out[i] = (hi * 16 + lo) as u8;
    }
    Some(out)
}

/// Whether `signature` (a signature file) is a valid signature of the
/// exact bytes of `list` by the owner of `key`.
pub fn list_is_signed_by(key: &[u8; 32], list: &[u8], signature: &[u8]) -> bool {
    match parse_signature(signature) {
        Some(sig) => ed25519::verify(key, list, &sig),
        None => false,
    }
}

/// Whether the list is signed by the project's key (the one in the program).
pub fn list_is_signed(list: &[u8], signature: &[u8]) -> bool {
    list_is_signed_by(&UPDATE_KEY, list, signature)
}

/// A mirror's address as the updater uses it: `http://` or `https://`, a host
/// and a folder made of plain characters, and a `/` at the end. Anything
/// else (a space, a quote, another kind of address) is no mirror.
pub fn mirror_url(text: &str) -> Option<String> {
    let t = text.trim();
    let rest = t.strip_prefix("http://").or_else(|| t.strip_prefix("https://"))?;
    let plain = rest.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '~' | '/' | ':' | '[' | ']' | '%' | '@' | '+'));
    if rest.is_empty() || !plain || rest.starts_with('/') || t.len() > 300 {
        return None;
    }
    let mut url = String::from(t);
    if !url.ends_with('/') {
        url.push('/');
    }
    Some(url)
}

/// Where to look, in order: the mirror, if there is one, then the project's
/// release (not for a build that cannot make secure connections).
pub(crate) fn sources(build: &str, mirror: &str) -> Vec<String> {
    let mut list = Vec::new();
    if let Some(m) = mirror_url(mirror) {
        list.push(m);
    }
    // The project's own mirror is always tried (the release list is signed, so
    // the network and the host need no trust): plain HTTP for the old Mac,
    // whose curl cannot make secure connections.
    let own = if build.starts_with("macos-legacy") { MIRROR_PLAIN } else { MIRROR };
    if !list.iter().any(|m| m == own) {
        list.push(String::from(own));
    }
    if !build.starts_with("macos-legacy") {
        list.push(String::from(RELEASE));
    }
    list
}

/// Fetch `nightly.txt` and its signature from `base`, and give the list only
/// if the signature is right.
/// Err(true) when a list was there but its signature is wrong, Err(false)
/// when the files could not be read at all.
pub(crate) fn fetch_signed_list(base: &str, folder: &str) -> Result<String, bool> {
    let list_path = alloc::format!("{}nightly.txt", folder);
    let sig_path = alloc::format!("{}nightly.txt.sig", folder);
    sys::remove_file(&sig_path);
    let Some(bytes) = fetch(&alloc::format!("{}nightly.txt", base), &list_path, 15, LIST_LIMIT) else {
        log("could not read the release list");
        return Err(false);
    };
    let sig = fetch(&alloc::format!("{}nightly.txt.sig", base), &sig_path, 15, LIST_LIMIT);
    sys::remove_file(&sig_path);
    let Some(sig) = sig else {
        log("could not read the release list's signature; the list is not used");
        return Err(false);
    };
    if bytes.len() > LIST_LIMIT || sig.len() > LIST_LIMIT || !list_is_signed(&bytes, &sig) {
        log("the release list's signature is not valid; the list is refused");
        return Err(true);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn log(what: &str) {
    logf!("voxelrust: update: {}", what);
}

/// `max` is the most bytes taken: a server that sends more (or for ever) is cut off
/// by curl, and a file that still came out bigger is refused.
fn fetch(url: &str, to: &str, seconds: u32, max: usize) -> Option<Vec<u8>> {
    let (u, t) = (sys::shell_arg(url)?, sys::shell_arg(to)?);
    let cmd = alloc::format!(
        "curl -fsSL --connect-timeout 8 --max-time {} --max-filesize {} -A \"VoxelRust/{}\" -o {} {}",
        seconds,
        max,
        env!("CARGO_PKG_VERSION"),
        t,
        u
    );
    if sys::run_command(&cmd) != 0 {
        // What was written before it stopped is not a file.
        sys::remove_file(to);
        return None;
    }
    sys::read_file_limit(to, max)
}

/// Whether the program at `program` runs and says it is the build the list
/// describes. A program that does not run here, or that is another build,
/// is refused before it can replace a working one.
fn stamp_matches(program: &str, build: &str, commit: &str, seq: u32, folder: &str) -> bool {
    let out = alloc::format!("{}update-stamp.txt", folder);
    sys::remove_file(&out);
    let (Some(p), Some(o)) = (sys::shell_arg(program), sys::shell_arg(&out)) else { return false };
    #[cfg(not(windows))]
    let cmd = alloc::format!("chmod 755 {p} && {p} --stamp {o} </dev/null >/dev/null 2>&1");
    #[cfg(windows)]
    let cmd = alloc::format!("{p} --stamp {o} >NUL 2>&1");
    if sys::run_command(&cmd) != 0 {
        sys::remove_file(&out);
        return false;
    }
    let want = alloc::format!("voxelrust-stamp {} {} {}", build, commit, seq);
    let got = sys::read_file(&out).map(|b| String::from(String::from_utf8_lossy(&b).trim()));
    sys::remove_file(&out);
    got.as_deref() == Some(want.as_str())
}

/// Look at the release and, if `install`, put a newer program in place of
/// this one. `folder` is the data folder: downloads and the stamp check go
/// there. Blocks while it downloads: call it from a thread of its own.
pub fn run(build: &str, folder: &str, install: bool, mirror: &str) {
    // The old-Mac build cannot reach the release: the `curl` of Mac OS X 10.6
    // has no way to make the secure connection it needs. It updates only from
    // a mirror over plain HTTP (see the module notes); without one it is
    // updated by hand (the release page says which file).
    if sources(build, mirror).is_empty() {
        return;
    }
    // Let the game get its window up first.
    sys::sleep_ms(4000);
    run_inner(build, folder, install, mirror);
}

/// For a program that stays up for days: look again every ten minutes until a
/// newer version is installed. (The dedicated server uses
/// `server_update::watch` instead: faster, with back-off.) Call it from a
/// thread of its own.
pub fn watch(build: &str, folder: &str, install: bool, mirror: &str) {
    if sources(build, mirror).is_empty() {
        return;
    }
    sys::sleep_ms(4000);
    loop {
        run_inner(build, folder, install, mirror);
        if STATE.load(Ordering::Acquire) == UPDATED {
            return;
        }
        for _ in 0..600 {
            sys::sleep_real_ms(1000);
        }
    }
}

/// One look at the release and, if `install` and it is newer, the install
/// (the same as one round of [`watch`]): for the server's own polling thread
/// (`server_update::watch`). Blocks while it downloads.
pub fn check_and_install(build: &str, folder: &str, install: bool, mirror: &str) {
    run_inner(build, folder, install, mirror);
}

/// The release number of the program this run installed (0 if none).
pub fn installed_seq() -> u32 {
    INSTALLED_SEQ.load(Ordering::Acquire)
}

/// A server started by an update must be able to restart for the next one
/// (the game must not, so that a bad update cannot loop; a server's updates
/// only ever go to a larger release number, which cannot loop either).
pub fn allow_restart_again() {
    RESTART_CHILD.store(false, Ordering::Release);
}

/// Where the updater keeps what it fetches and notes: `tmp/update/` in the
/// data folder, so the data folder itself is not cluttered.
fn tmp_folder(folder: &str) -> String {
    alloc::format!("{}tmp/update/", folder)
}

fn run_inner(build: &str, folder: &str, install: bool, mirror: &str) {
    set_state(CHECKING);
    let own = tmp_folder(folder);
    sys::make_dirs(&own);
    // Earlier versions kept the list loose in the data folder.
    sys::remove_file(&alloc::format!("{}nightly.txt", folder));
    let folder = own.as_str();
    // The first source with a list that is properly signed. Nothing in a
    // list is believed before that: not the numbers, not the checksums.
    let mut found = None;
    let mut refused = false;
    for base in sources(build, mirror) {
        match fetch_signed_list(&base, folder) {
            Ok(list) => {
                found = Some((base, list));
                break;
            }
            Err(bad_signature) => refused |= bad_signature,
        }
    }
    let Some((base, list)) = found else {
        return set_state(if refused { REFUSED } else { FAILED });
    };
    let (Some(commit), Some(listed)) = (listed_commit(&list), listed_seq(&list)) else {
        log("the release list has no commit or sequence number");
        return set_state(FAILED);
    };
    // A program built by hand, without a release number, is left alone.
    let Some(own) = SEQ else {
        log("this build has no release number, so it is left alone");
        return set_state(AVAILABLE);
    };
    if !is_newer(Some(own), listed) {
        return set_state(CURRENT);
    }
    let Some((file, sum, size)) = entry(&list, build) else {
        log("the release has no program for this kind of computer");
        return set_state(CURRENT);
    };
    if !install {
        return set_state(AVAILABLE);
    }

    let exe = sys::exe_path();
    if exe.is_empty() || sys::shell_arg(&exe).is_none() {
        log("cannot tell where the program is, or its path cannot be used safely");
        return set_state(FAILED);
    }
    let fresh = alloc::format!("{}.new", exe);
    let old = alloc::format!("{}.old", exe);
    sys::remove_file(&fresh);
    // Left by an earlier update on Windows; it may still be locked, then it stays.
    sys::remove_file(&old);

    let ok = match fetch(&alloc::format!("{}{}", base, file), &fresh, 300, size) {
        Some(bytes) => bytes.len() == size && crate::sha256::hex(&bytes) == sum.to_ascii_lowercase(),
        None => false,
    };
    if !ok {
        sys::remove_file(&fresh);
        log("the download failed or does not match the release list");
        return set_state(FAILED);
    }
    if !stamp_matches(&fresh, build, commit, listed, folder) {
        sys::remove_file(&fresh);
        log("the new program does not run here or is not the build the list describes");
        return set_state(FAILED);
    }

    set_state(SWAPPING);
    let mut placed = false;
    for _ in 0..5 {
        if sys::replace_file(&fresh, &exe) {
            placed = true;
            break;
        }
        sys::sleep_real_ms(200);
    }
    sys::remove_file(&fresh);
    if !placed {
        log("could not put the new program in place (the old one is still there)");
        return set_state(FAILED);
    }
    INSTALLED_SEQ.store(listed, Ordering::Release);
    logf!("voxelrust: updated to release {} (commit {}); the new version starts when you quit", listed, commit);
    set_state(UPDATED);
}

/// The same check and install as [`run`], without the wait, for the
/// updater's end-to-end test (only in builds with the `update-test` feature).
#[cfg(feature = "update-test")]
pub fn run_now(build: &str, folder: &str, mirror: &str) {
    run_inner(build, folder, true, mirror);
}

/// Called when the game is quit, after everything is saved and the window
/// and sound are closed: starts the installed version, if there is one.
/// Does nothing if nothing was installed, if this copy was itself started
/// by an update, or if the program's path is not known.
#[cfg(not(target_arch = "wasm32"))]
pub fn restart(exe: &str, folder: &str) {
    if RESTART_CHILD.load(Ordering::Acquire) || exe.is_empty() {
        return;
    }
    // An install that is still being put in place must finish first.
    let mut waited = 0;
    while state() == SWAPPING && waited < 200 {
        sys::sleep_real_ms(50);
        waited += 1;
    }
    if state() != UPDATED {
        return;
    }
    // Written before the new copy starts, so the copy cannot miss it.
    sys::make_dirs(&tmp_folder(folder));
    let marker = alloc::format!("{}update.txt", tmp_folder(folder));
    let seq = INSTALLED_SEQ.load(Ordering::Acquire);
    if !sys::write_file(&marker, alloc::format!("relaunch {}\n", seq).as_bytes()) {
        log("could not note the restart; the new version starts at the next start");
        return;
    }
    // The same command line (and so the same data folder, which the copy that
    // starts needs in order to find the marker).
    let args = match restart_args() {
        Some(a) => a,
        None => String::new(),
    };
    if sys::start_program(exe, &args) {
        logf!("voxelrust: starting the new version");
    } else {
        sys::remove_file(&marker);
        log("could not start the new version; it starts at the next start");
    }
}

/// At start-up: if the program was started by an update, note that, so it
/// does not restart in turn. Returns whether it was.
#[cfg(not(target_arch = "wasm32"))]
pub fn consume_restart_marker(folder: &str) -> bool {
    let marker = alloc::format!("{}update.txt", tmp_folder(folder));
    if sys::read_file(&marker).is_none() {
        return false;
    }
    sys::remove_file(&marker);
    RESTART_CHILD.store(true, Ordering::Release);
    true
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
        // A name that is only dots is a folder, not a program.
        let dots = alloc::format!("linux-x86_64 .. {} 1500000\nwindows-x86_64 ..hidden {} 1500000\n", sum, sum);
        assert_eq!(entry(&dots, "linux-x86_64"), None);
        assert_eq!(entry(&dots, "windows-x86_64"), None);
    }

    // A list signed with a throwaway key (made with `openssl pkeyutl -sign -rawin`).
    const TEST_KEY: &str = "e42289f222a499a8d102f7877ec0417e6feb82a73306cf3acfb8618470ecbf02";
    const TEST_LIST: &str = "commit 1a2b3c4d5e6f\nseq 42\nlinux-x86_64 voxelrust-linux-x86_64.bin ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad 1500000\n";
    const TEST_SIG: &str = "3a3340520238a222ea0bdfbb7475bcd3003c5e8b7aab4581b5bcac23527a46544010039ac4577cdf2cfcaaa90f9b3e03e4082c4152ccce266bf7a489a74d030a";

    fn unhex(s: &str) -> Vec<u8> {
        let digit = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
        s.as_bytes().chunks(2).map(|p| digit(p[0]) * 16 + digit(p[1])).collect()
    }

    fn test_key() -> [u8; 32] {
        let mut k = [0u8; 32];
        k.copy_from_slice(&unhex(TEST_KEY));
        k
    }

    #[test]
    fn a_signed_list_is_accepted_and_read() {
        let key = test_key();
        let raw = unhex(TEST_SIG);
        assert!(list_is_signed_by(&key, TEST_LIST.as_bytes(), &raw), "the signature file as raw bytes");
        let hex_file = alloc::format!("{}\n", TEST_SIG);
        assert!(list_is_signed_by(&key, TEST_LIST.as_bytes(), hex_file.as_bytes()), "as hexadecimal text");
        // Once signed, it parses as always.
        assert_eq!(listed_commit(TEST_LIST), Some("1a2b3c4d5e6f"));
        assert_eq!(listed_seq(TEST_LIST), Some(42));
        assert!(entry(TEST_LIST, "linux-x86_64").is_some());
    }

    #[test]
    fn a_changed_list_or_signature_is_refused() {
        let key = test_key();
        let raw = unhex(TEST_SIG);
        // A newer number, another checksum, one added byte, a lost newline.
        let changed = [
            TEST_LIST.replace("seq 42", "seq 43"),
            TEST_LIST.replace("ba7816bf", "ba7816be"),
            alloc::format!("{}\n", TEST_LIST),
            String::from(TEST_LIST.trim_end()),
            String::new(),
        ];
        for list in &changed {
            assert!(!list_is_signed_by(&key, list.as_bytes(), &raw), "{list:?}");
        }
        // A signature that is damaged, short, long, or missing.
        let mut bad = raw.clone();
        bad[10] ^= 0x40;
        assert!(!list_is_signed_by(&key, TEST_LIST.as_bytes(), &bad));
        assert!(!list_is_signed_by(&key, TEST_LIST.as_bytes(), &raw[..63]));
        assert!(!list_is_signed_by(&key, TEST_LIST.as_bytes(), b""));
        assert!(!list_is_signed_by(&key, TEST_LIST.as_bytes(), b"not a signature"));
        let mut long = raw.clone();
        long.push(b'\n');
        assert!(!list_is_signed_by(&key, TEST_LIST.as_bytes(), &long));
        // Another key's name does not do.
        let mut other = key;
        other[0] ^= 1;
        assert!(!list_is_signed_by(&other, TEST_LIST.as_bytes(), &raw));
        // And the project's own key does not accept the throwaway key's list.
        assert!(!list_is_signed(TEST_LIST.as_bytes(), &raw));
    }

    #[test]
    fn mirror_addresses_are_checked() {
        assert_eq!(mirror_url("http://example.org/vox").as_deref(), Some("http://example.org/vox/"));
        assert_eq!(mirror_url(" http://192.168.1.5:8000/ ").as_deref(), Some("http://192.168.1.5:8000/"));
        assert_eq!(mirror_url("https://example.org/a/b/").as_deref(), Some("https://example.org/a/b/"));
        assert_eq!(mirror_url(""), None);
        assert_eq!(mirror_url("ftp://example.org/"), None);
        assert_eq!(mirror_url("http://"), None);
        assert_eq!(mirror_url("http://exa mple.org/"), None);
        assert_eq!(mirror_url("http://example.org/';rm -rf ~;'"), None);
        assert_eq!(mirror_url("http://example.org/$(x)"), None);
    }

    #[test]
    fn the_old_mac_build_looks_only_in_a_plain_http_mirror() {
        assert_eq!(sources("macos-legacy-x86_64", ""), alloc::vec![String::from(MIRROR_PLAIN)]);
        assert_eq!(sources("macos-legacy-x86_64", "not a url"), alloc::vec![String::from(MIRROR_PLAIN)]);
        assert_eq!(sources("macos-legacy-x86_64", "http://m.example/v"), alloc::vec![String::from("http://m.example/v/"), String::from(MIRROR_PLAIN)]);
        assert!(MIRROR_PLAIN.starts_with("http://"));
        assert_eq!(sources("linux-x86_64", ""), alloc::vec![String::from(MIRROR), String::from(RELEASE)]);
        let both = sources("linux-x86_64", "http://m.example/v/");
        assert_eq!(both.len(), 3);
        assert_eq!(both[0], "http://m.example/v/", "your own mirror first");
    }

    #[test]
    fn the_sequence_number_is_read_strictly() {
        assert_eq!(listed_seq("commit abc\nseq 42\nlinux-x86_64 x 1 1\n"), Some(42));
        assert_eq!(listed_seq("commit abc\n"), None, "no seq line");
        assert_eq!(listed_seq("seq 42x\n"), None, "not digits");
        assert_eq!(listed_seq("seq -3\n"), None, "a sign is not allowed");
        assert_eq!(listed_seq("seq \n"), None, "empty");
        assert_eq!(listed_seq("seq 1234567890\n"), None, "too many digits");
        assert_eq!(listed_seq("seq 4\nseq 5\n"), None, "two lines disagree");
    }

    #[test]
    fn a_build_number_is_compiled_in_only_when_it_is_plain_digits() {
        assert_eq!(parse_seq("12"), Some(12));
        assert_eq!(parse_seq("0"), Some(0));
        assert_eq!(parse_seq(""), None);
        assert_eq!(parse_seq("1a"), None);
        assert_eq!(parse_seq("123456789"), Some(123_456_789));
        assert_eq!(parse_seq("1234567890"), None);
    }

    #[test]
    fn only_a_higher_release_replaces_a_program() {
        assert!(is_newer(Some(4), 5));
        assert!(!is_newer(Some(5), 5), "the same release is current");
        assert!(!is_newer(Some(6), 5), "an older list never moves a program back");
        assert!(!is_newer(None, 5), "a program with no number is never replaced");
    }

    #[test]
    fn a_program_path_is_quoted_so_the_shell_reads_it_as_one_word() {
        #[cfg(not(windows))]
        {
            assert_eq!(sys::shell_arg("/opt/a b/vox").as_deref(), Some("'/opt/a b/vox'"));
            assert_eq!(sys::shell_arg("it's").as_deref(), Some("'it'\\''s'"));
            assert_eq!(sys::shell_arg("$(rm -rf ~)").as_deref(), Some("'$(rm -rf ~)'"));
        }
        #[cfg(windows)]
        {
            assert!(sys::shell_arg("C:\\games\\vox.exe").is_some());
            assert!(sys::shell_arg("C:\\50%\\vox.exe").is_none());
            assert!(sys::shell_arg("C:\\\"quoted\\\"\\vox.exe").is_none());
        }
    }

    #[test]
    fn the_stamp_names_the_build_and_the_release() {
        assert_eq!(parse_seq("7"), Some(7));
        let text = stamp_text("linux-x86_64");
        assert!(text.starts_with("voxelrust-stamp linux-x86_64 "), "{text}");
    }
}
