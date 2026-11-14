//! Fetching a skin picture from a web address (SETTINGS > SKIN > IMPORT FROM
//! WEB ADDRESS).
//!
//! The player pastes a direct link to a picture; `curl` (through
//! `sys::run_command`, as the updater does) fetches it on a worker thread
//! into `tmp/skins/` in the data folder, and the thread then judges the file
//! (a picture the game can read, 64 x 64 or 64 x 32). The menu reads the
//! state every frame, so it never freezes. When the state says [`OK`] the
//! menu takes the file through the usual skin import (`app::import_skin`).
//!
//! Everything that decides something is a pure function (`clean_url`,
//! `valid_url`, `begin`, `judge`, `message`) so that it can be tested.

use crate::sys;
use alloc::string::String;
use core::sync::atomic::{AtomicU8, Ordering};

/// Nothing going on.
pub const IDLE: u8 = 0;
pub const DOWNLOADING: u8 = 1;
/// A usable skin picture is waiting in the download file.
pub const OK: u8 = 2;
pub const FAILED: u8 = 3;
pub const NOT_SKIN: u8 = 4;
pub const TOO_BIG: u8 = 5;
/// The old Mac build's `curl` has no secure connections.
pub const NO_HTTPS: u8 = 6;
pub const BAD_URL: u8 = 7;

/// The most that is downloaded: a skin picture is a few kilobytes.
pub const MAX_BYTES: usize = 2 << 20;
/// Longest address accepted.
pub const MAX_URL: usize = 300;

static STATE: AtomicU8 = AtomicU8::new(IDLE);

pub fn state() -> u8 {
    STATE.load(Ordering::Acquire)
}

fn set_state(s: u8) {
    STATE.store(s, Ordering::Release);
}

/// What to show the player for a state.
pub fn message(state: u8) -> &'static str {
    match state {
        DOWNLOADING => "DOWNLOADING...",
        OK => "DOWNLOADED - IMPORTING...",
        FAILED => "COULD NOT DOWNLOAD",
        NOT_SKIN => "NOT A SKIN PICTURE",
        TOO_BIG => "TOO BIG",
        NO_HTTPS => "THIS MAC CAN ONLY OPEN HTTP ADDRESSES (NO HTTPS)",
        BAD_URL => "NOT A WEB ADDRESS (HTTP OR HTTPS, NO SPACES)",
        _ => "",
    }
}

/// Whether the state is one that ends a try and wants to be shown.
pub fn is_failure(state: u8) -> bool {
    matches!(state, FAILED | NOT_SKIN | TOO_BIG | NO_HTTPS | BAD_URL)
}

/// Forget the last try unless a download is still running.
pub fn reset() {
    let _ = STATE.compare_exchange(OK, IDLE, Ordering::AcqRel, Ordering::Acquire);
    for s in [FAILED, NOT_SKIN, TOO_BIG, NO_HTTPS, BAD_URL] {
        let _ = STATE.compare_exchange(s, IDLE, Ordering::AcqRel, Ordering::Acquire);
    }
}

/// Take the finished download: true once when the state is [`OK`], which
/// then goes back to idle.
pub fn take_ok() -> bool {
    STATE.compare_exchange(OK, IDLE, Ordering::AcqRel, Ordering::Acquire).is_ok()
}

/// Where the download is kept: `tmp/skins/` in the data folder.
pub fn folder(base: &str) -> String {
    alloc::format!("{}tmp/skins/", base)
}

/// The downloaded file.
pub fn file(base: &str) -> String {
    alloc::format!("{}download.img", folder(base))
}

/// Characters an address may have. No quotes, spaces, `$`, backticks,
/// backslashes, `;`, `|`, `<`, `>`, brackets or anything else a shell or
/// `cmd` could read as more than text. (Windows additionally refuses `%`.)
fn url_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~' | ':' | '/' | '?' | '=' | '&' | '%' | '+' | ',' | '#')
}

/// Whether `url` is an http or https address of a plain, safe shape.
pub fn valid_url(url: &str) -> bool {
    if url.len() < 10 || url.len() > MAX_URL || !url.chars().all(url_char) {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &url[8..]
    } else if lower.starts_with("http://") {
        &url[7..]
    } else {
        return false;
    };
    let authority = rest.split(|c| c == '/' || c == '?' || c == '#').next().unwrap_or("");
    let (host, port) = match authority.split_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (authority, None),
    };
    if let Some(p) = port {
        if p.is_empty() || p.len() > 5 || !p.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    // A host name or an address: letters, digits, dots and hyphens, no empty
    // or hyphen-edged labels.
    if host.is_empty() || host.len() > 253 || host.contains("..") {
        return false;
    }
    host.split('.').all(|label| !label.is_empty() && !label.starts_with('-') && !label.ends_with('-') && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
}

/// Whether the address is a secure one.
pub fn is_https(url: &str) -> bool {
    url.len() >= 8 && url[..8].eq_ignore_ascii_case("https://")
}

/// What the player typed, made into an address: spaces at the ends dropped,
/// `https://` added to a bare `host/path`. None if it is not a usable address.
pub fn clean_url(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    let full = if t.contains("://") { String::from(t) } else { alloc::format!("https://{}", t) };
    if valid_url(&full) {
        Some(full)
    } else {
        None
    }
}

/// The first step of a try, as a pure decision: the address to fetch, or
/// the state that ends the try at once. `legacy` is the old Mac build.
pub fn begin(text: &str, legacy: bool, busy: bool) -> Result<String, u8> {
    if busy {
        return Err(DOWNLOADING);
    }
    let Some(url) = clean_url(text) else { return Err(BAD_URL) };
    if legacy && is_https(&url) {
        return Err(NO_HTTPS);
    }
    Ok(url)
}

/// What a finished download comes to: curl's exit status and the file it
/// left (if any).
pub fn judge(curl_status: i32, bytes: Option<&[u8]>) -> u8 {
    // 63 is curl's "maximum file size exceeded".
    if curl_status == 63 {
        return TOO_BIG;
    }
    let Some(b) = bytes else { return FAILED };
    if curl_status != 0 || b.is_empty() {
        return FAILED;
    }
    if b.len() > MAX_BYTES {
        return TOO_BIG;
    }
    let Some(img) = crate::image::decode(b) else { return NOT_SKIN };
    if img.w > 64 || img.h > 64 {
        return TOO_BIG;
    }
    if crate::client::skinfile::size_ok(img.w, img.h) {
        OK
    } else {
        NOT_SKIN
    }
}

/// Start fetching `text` for the skin. Returns the state right after: a
/// state that ends the try ([`BAD_URL`], [`NO_HTTPS`]), or [`DOWNLOADING`]
/// with a worker thread running. `base` is the data folder.
pub fn start(text: &str, base: &str, build: &str) -> u8 {
    let busy = state() == DOWNLOADING;
    let url = match begin(text, build.starts_with("macos-legacy"), busy) {
        Ok(u) => u,
        Err(s) => {
            if s != DOWNLOADING {
                set_state(s);
            }
            return s;
        }
    };
    let legacy = build.starts_with("macos-legacy");
    set_state(DOWNLOADING);
    let base = String::from(base);
    let task = alloc::boxed::Box::new(move || {
        let s = download(&url, &base, legacy);
        set_state(s);
    });
    if sys::thread::spawn(task).is_none() {
        set_state(FAILED);
    }
    state()
}

/// The worker: fetch, judge, and leave the file only if it is a skin.
fn download(url: &str, base: &str, legacy: bool) -> u8 {
    let dir = folder(base);
    sys::make_dirs(&dir);
    let to = file(base);
    sys::remove_file(&to);
    let (Some(u), Some(t)) = (sys::shell_arg(url), sys::shell_arg(&to)) else { return BAD_URL };
    // Only http and https, also after a redirect (the old curl has no such
    // switch, but it does not follow a redirect to a file either).
    let protocols = if legacy { "" } else { "--proto =http,https --proto-redir =http,https " };
    let cmd = alloc::format!(
        "curl -fsSL --connect-timeout 8 --max-time 20 --max-redirs 5 --max-filesize {} {}-A \"VoxelRust/{}\" -o {} {}",
        MAX_BYTES,
        protocols,
        env!("CARGO_PKG_VERSION"),
        t,
        u
    );
    let status = sys::run_command(&cmd);
    let bytes = sys::read_file(&to);
    let verdict = judge(status, bytes.as_deref());
    if verdict != OK {
        sys::remove_file(&to);
        logf!("voxelrust: skin download failed (curl status {}, state {})", status, verdict);
    }
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn png(w: usize, h: usize) -> Vec<u8> {
        crate::png::encode(w, h, &alloc::vec![200u8; w * h * 4])
    }

    #[test]
    fn plain_web_addresses_are_accepted() {
        for u in [
            "https://textures.minecraft.net/texture/0123abcd",
            "http://example.com/skin.png",
            "HTTPS://Example.com/a/b-c_d.png?x=1&y=2",
            "https://s.namemc.com/i/0123456789abcdef.png",
            "http://127.0.0.1:8080/skin.png",
        ] {
            assert!(valid_url(u), "{}", u);
        }
    }

    #[test]
    fn dangerous_or_odd_addresses_are_refused() {
        for u in [
            "ftp://example.com/skin.png",
            "file:///etc/passwd",
            "https://",
            "https://example.com/a b.png",
            "https://example.com/a';rm -rf ~;'.png",
            "https://example.com/`id`.png",
            "https://example.com/$(id).png",
            "https://example.com/a\"b.png",
            "https://example.com/a|b",
            "https://example.com/a\\b",
            "https://user@example.com/a.png",
            "https://example.com:port/a.png",
            "https://example.com:123456/a.png",
            "https://-bad.example.com/a.png",
            "https://exa..mple.com/a.png",
            "https://example.com/\n",
            "javascript:alert(1)",
        ] {
            assert!(!valid_url(u), "{}", u);
        }
        let long = alloc::format!("https://example.com/{}", "a".repeat(400));
        assert!(!valid_url(&long));
    }

    #[test]
    fn typed_text_is_tidied() {
        assert_eq!(clean_url("  https://example.com/a.png  ").as_deref(), Some("https://example.com/a.png"));
        assert_eq!(clean_url("example.com/a.png").as_deref(), Some("https://example.com/a.png"));
        assert_eq!(clean_url("   "), None);
        assert_eq!(clean_url("ftp://example.com/a.png"), None);
        assert_eq!(clean_url("not an address"), None);
    }

    #[test]
    fn the_old_mac_says_so_for_secure_addresses() {
        assert_eq!(begin("https://example.com/a.png", true, false), Err(NO_HTTPS));
        assert_eq!(begin("http://example.com/a.png", true, false), Ok(String::from("http://example.com/a.png")));
        assert_eq!(begin("https://example.com/a.png", false, false), Ok(String::from("https://example.com/a.png")));
        assert_eq!(begin("nonsense words", false, false), Err(BAD_URL));
        assert_eq!(begin("https://example.com/a.png", false, true), Err(DOWNLOADING));
    }

    #[test]
    fn a_download_is_judged_by_status_size_and_picture() {
        assert_eq!(judge(0, Some(&png(64, 64))), OK);
        assert_eq!(judge(0, Some(&png(64, 32))), OK);
        assert_eq!(judge(0, Some(&png(32, 32))), NOT_SKIN);
        assert_eq!(judge(0, Some(&png(64, 48))), NOT_SKIN);
        assert_eq!(judge(0, Some(&png(128, 128))), TOO_BIG);
        assert_eq!(judge(0, Some(b"<html>not a picture at all, just a page</html>")), NOT_SKIN);
        assert_eq!(judge(22, None), FAILED);
        assert_eq!(judge(6, None), FAILED);
        assert_eq!(judge(22, Some(&png(64, 64))), FAILED);
        assert_eq!(judge(0, Some(&[])), FAILED);
        assert_eq!(judge(63, None), TOO_BIG);
        assert_eq!(judge(0, Some(&alloc::vec![0u8; MAX_BYTES + 1])), TOO_BIG);
    }

    #[test]
    fn an_old_layout_download_becomes_a_square_skin() {
        let img = crate::image::decode(&png(64, 32)).unwrap();
        let px = crate::client::skinfile::from_image(&img).unwrap();
        assert_eq!(px.len(), 64 * 64 * 4);
    }

    #[test]
    fn every_ending_state_has_words() {
        for s in [DOWNLOADING, OK, FAILED, NOT_SKIN, TOO_BIG, NO_HTTPS, BAD_URL] {
            assert!(!message(s).is_empty());
        }
        assert!(message(IDLE).is_empty());
        assert!(is_failure(FAILED) && !is_failure(OK) && !is_failure(DOWNLOADING) && !is_failure(IDLE));
    }

    #[test]
    fn the_state_goes_idle_when_taken_or_reset() {
        // One test owns the shared state: no other test touches it.
        set_state(DOWNLOADING);
        reset();
        assert_eq!(state(), DOWNLOADING);
        assert!(!take_ok());
        set_state(OK);
        assert!(take_ok());
        assert_eq!(state(), IDLE);
        assert!(!take_ok());
        set_state(NOT_SKIN);
        reset();
        assert_eq!(state(), IDLE);
    }
}
