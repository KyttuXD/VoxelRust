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
//! A program only ever takes the file listed for its own build name (see
//! `app::BUILD`), checked against its SHA-256 and size, and then run with
//! `--stamp` to make sure it is the build the list describes. Only then does
//! it take the running program's place. The new version starts when the game
//! is next quit: the update thread never restarts a game in play, and the
//! restarted copy never restarts again.

use crate::sys;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

/// Where the nightly release's files are.
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

fn log(what: &str) {
    logf!("voxelrust: update: {}", what);
}

fn fetch(url: &str, to: &str, seconds: u32) -> Option<Vec<u8>> {
    let (u, t) = (sys::shell_arg(url)?, sys::shell_arg(to)?);
    let cmd = alloc::format!(
        "curl -fsSL --connect-timeout 8 --max-time {} -A \"VoxelRust/{}\" -o {} {}",
        seconds,
        env!("CARGO_PKG_VERSION"),
        t,
        u
    );
    if sys::run_command(&cmd) != 0 {
        return None;
    }
    sys::read_file(to)
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
pub fn run(build: &str, folder: &str, install: bool) {
    // The old-Mac build cannot do this: the `curl` of Mac OS X 10.6 has no
    // way to make the secure connection the release needs. It is updated
    // by hand (the release page says which file).
    if build.starts_with("macos-legacy") {
        return;
    }
    // Let the game get its window up first.
    sys::sleep_ms(4000);
    run_inner(build, folder, install);
}

/// For a program that stays up for days (a server): look again every ten
/// minutes until a newer version is installed. Call it from a thread of its own.
pub fn watch(build: &str, folder: &str, install: bool) {
    if build.starts_with("macos-legacy") {
        return;
    }
    sys::sleep_ms(4000);
    loop {
        run_inner(build, folder, install);
        if STATE.load(Ordering::Acquire) == UPDATED {
            return;
        }
        for _ in 0..600 {
            sys::sleep_real_ms(1000);
        }
    }
}

/// Where the updater keeps what it fetches and notes: `tmp/update/` in the
/// data folder, so the data folder itself is not cluttered.
fn tmp_folder(folder: &str) -> String {
    alloc::format!("{}tmp/update/", folder)
}

fn run_inner(build: &str, folder: &str, install: bool) {
    set_state(CHECKING);
    let own = tmp_folder(folder);
    sys::make_dirs(&own);
    // Earlier versions kept the list loose in the data folder.
    sys::remove_file(&alloc::format!("{}nightly.txt", folder));
    let folder = own.as_str();
    let list = match fetch(&alloc::format!("{}nightly.txt", RELEASE), &alloc::format!("{}nightly.txt", folder), 15) {
        Some(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        None => {
            log("could not read the release list");
            return set_state(FAILED);
        }
    };
    let (Some(commit), Some(listed)) = (listed_commit(&list), listed_seq(&list)) else {
        log("the release list has no commit or sequence number");
        return set_state(FAILED);
    };
    // A program built by hand, without a release number, is left alone.
    let Some(own) = SEQ else {
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

    let ok = match fetch(&alloc::format!("{}{}", RELEASE, file), &fresh, 300) {
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
pub fn run_now(build: &str, folder: &str) {
    run_inner(build, folder, true);
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
