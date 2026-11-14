//! The polling thread of a dedicated server.
//!
//! Every `interval` seconds (plus a little jitter) it reads the signed
//! release list from the mirror and the release, as the game does: two tiny
//! files, checked against the project's key before a single number in them is
//! believed. Only when the release number is newer than this program's (and
//! than any release that had to be rolled back) does it run the full path in
//! `client::update`: download the program, check its SHA-256, size and
//! `--stamp`, and put it in place. After repeated failures it waits longer
//! each time (see `schedule::backoff_seconds`), so a broken release or a
//! dead network is never hammered.

use super::schedule;
use crate::sys;
use crate::update;
use alloc::string::String;

/// The release number recorded by `scripts/server-supervisor.sh` for a
/// release that crashed on start and was rolled back (None: none).
fn rejected_seq(state_dir: &str) -> Option<u32> {
    if state_dir.is_empty() {
        return None;
    }
    let bytes = sys::read_file(&alloc::format!("{}rejected-seq", state_dir))?;
    String::from_utf8_lossy(&bytes).trim().parse::<u32>().ok()
}

/// The release number of the first source whose signed list can be read.
fn listed_release(build: &str, scratch: &str, mirror: &str) -> Option<u32> {
    sys::make_dirs(scratch);
    for base in update::sources(build, mirror) {
        if let Ok(list) = update::fetch_signed_list(&base, scratch) {
            if let Some(n) = update::listed_seq(&list) {
                return Some(n);
            }
        }
    }
    None
}

/// Look for a new release until one is installed (then it returns and the
/// server loop restarts). `folder` is the data folder; `state_dir` is the
/// supervisor's folder (with a trailing slash), or empty without one.
/// Call it from a thread of its own.
pub fn watch(build: &str, folder: &str, mirror: &str, interval: u32, state_dir: &str) {
    if update::sources(build, mirror).is_empty() {
        return;
    }
    sys::sleep_real_ms(4000);
    let scratch = alloc::format!("{}tmp/update/", folder);
    let mut failures = 0u32;
    loop {
        match listed_release(build, &scratch, mirror) {
            None => failures += 1,
            Some(listed) if schedule::worth_attempting(update::SEQ, listed, rejected_seq(state_dir)) => {
                logf!("voxelrust: update: release {} is out; fetching it", listed);
                update::check_and_install(build, folder, true, mirror);
                match update::state() {
                    update::UPDATED => return,
                    update::FAILED | update::REFUSED => failures += 1,
                    _ => failures = 0,
                }
            }
            Some(_) => failures = 0,
        }
        let salt = (sys::now().to_bits() >> 12) as u32;
        let wait = schedule::with_jitter(schedule::backoff_seconds(interval, failures), salt);
        for _ in 0..wait {
            sys::sleep_real_ms(1000);
        }
    }
}
