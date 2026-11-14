//! The rules of the server's update polling, with no side effects (so they
//! can be tested). `scripts/server-supervisor.sh` applies the same rollback
//! rule in shell (`scripts/test-supervisor.sh` checks it).

/// How often the server looks for a new release, unless told otherwise.
pub const DEFAULT_SECONDS: u32 = 90;
pub const MIN_SECONDS: u32 = 30;
pub const MAX_SECONDS: u32 = 600;
/// The longest the server waits between looks after failures.
pub const BACKOFF_CAP_SECONDS: u32 = 1800;
/// A program that stops this soon after an update started it is a failed update.
pub const ROLLBACK_WINDOW_SECONDS: u32 = 20;
/// The exit code that asks `scripts/server-supervisor.sh` to start the program again.
pub const RESTART_EXIT_CODE: i32 = 75;

/// The `update_check_seconds` setting as text: a whole number of seconds,
/// kept within 30..600. Empty or unreadable text gives the default (90).
pub fn clamp_interval(text: &str) -> u32 {
    match text.trim().parse::<u32>() {
        Ok(n) => n.clamp(MIN_SECONDS, MAX_SECONDS),
        Err(_) => DEFAULT_SECONDS,
    }
}

/// `interval` plus a little (up to a sixth of it, picked by `salt`), so that
/// many servers do not all ask in the same second.
pub fn with_jitter(interval: u32, salt: u32) -> u32 {
    interval + salt % (interval / 6 + 1)
}

/// How long to wait after `failures` failed looks or installs in a row: the
/// interval doubled each time, up to half an hour. No failures: the interval.
pub fn backoff_seconds(interval: u32, failures: u32) -> u32 {
    let doubled = (interval as u64) << failures.min(12);
    doubled.min(BACKOFF_CAP_SECONDS as u64).max(interval as u64) as u32
}

/// Whether a listed release should be downloaded: it is newer than this
/// program (a program with no number is never replaced) and newer than any
/// release that was tried and had to be rolled back.
pub fn worth_attempting(own: Option<u32>, listed: u32, rejected: Option<u32>) -> bool {
    match own {
        Some(o) => listed > o && rejected.map_or(true, |r| listed > r),
        None => false,
    }
}

/// Whether the start that follows an update has to be undone: the program
/// stopped with an error (not 0, and not a request to restart) within 20
/// seconds of being started by an update.
pub fn rollback_needed(exit_code: i32, lived_seconds: u32, started_by_update: bool) -> bool {
    started_by_update && exit_code != 0 && exit_code != RESTART_EXIT_CODE && lived_seconds < ROLLBACK_WINDOW_SECONDS
}

/// The wait before the supervisor starts a program that crashed (not after
/// an update): 5 seconds, doubling, at most a minute.
pub fn crash_delay_seconds(crashes_in_a_row: u32) -> u32 {
    (5u32 << crashes_in_a_row.min(4)).min(60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interval_is_clamped_and_defaults() {
        assert_eq!(clamp_interval(""), 90);
        assert_eq!(clamp_interval("abc"), 90);
        assert_eq!(clamp_interval("-5"), 90);
        assert_eq!(clamp_interval("1"), 30);
        assert_eq!(clamp_interval("30"), 30);
        assert_eq!(clamp_interval(" 120 "), 120);
        assert_eq!(clamp_interval("600"), 600);
        assert_eq!(clamp_interval("601"), 600);
        assert_eq!(clamp_interval("99999999999"), 90, "too big for a number: the default");
    }

    #[test]
    fn jitter_stays_within_a_sixth() {
        for salt in [0u32, 1, 7, 14, 15, 16, 1000, u32::MAX] {
            let w = with_jitter(90, salt);
            assert!((90..=105).contains(&w), "{w}");
        }
        assert_eq!(with_jitter(90, 0), 90);
        assert!((30..=35).contains(&with_jitter(30, 12345)));
    }

    #[test]
    fn backoff_doubles_to_a_cap() {
        assert_eq!(backoff_seconds(90, 0), 90);
        assert_eq!(backoff_seconds(90, 1), 180);
        assert_eq!(backoff_seconds(90, 2), 360);
        assert_eq!(backoff_seconds(90, 3), 720);
        assert_eq!(backoff_seconds(90, 4), 1440);
        assert_eq!(backoff_seconds(90, 5), 1800);
        assert_eq!(backoff_seconds(90, 1000), 1800);
        // An interval above the cap is never shortened.
        assert_eq!(backoff_seconds(600, 0), 600);
        assert_eq!(backoff_seconds(600, 3), 1800);
    }

    #[test]
    fn only_a_newer_unrejected_release_is_fetched() {
        assert!(worth_attempting(Some(10), 11, None));
        assert!(!worth_attempting(Some(10), 10, None), "the same release");
        assert!(!worth_attempting(Some(10), 9, None), "an older one");
        assert!(!worth_attempting(None, 11, None), "a hand-built program is never replaced");
        // Release 12 was installed, crashed and was rolled back: not again,
        // but 13 is tried.
        assert!(!worth_attempting(Some(10), 12, Some(12)));
        assert!(!worth_attempting(Some(10), 11, Some(12)));
        assert!(worth_attempting(Some(10), 13, Some(12)));
    }

    #[test]
    fn a_quick_failure_after_an_update_rolls_back() {
        assert!(rollback_needed(1, 3, true));
        assert!(rollback_needed(134, 19, true), "aborted");
        assert!(rollback_needed(139, 0, true), "crashed at once");
        assert!(!rollback_needed(1, 20, true), "it ran 20 seconds: it works");
        assert!(!rollback_needed(1, 300, true));
        assert!(!rollback_needed(0, 3, true), "stopped on purpose");
        assert!(!rollback_needed(RESTART_EXIT_CODE, 3, true), "asked for another update");
        assert!(!rollback_needed(1, 3, false), "not started by an update: no rollback, only a delay");
    }

    #[test]
    fn crashes_are_delayed_more_each_time() {
        assert_eq!(crash_delay_seconds(0), 5);
        assert_eq!(crash_delay_seconds(1), 10);
        assert_eq!(crash_delay_seconds(2), 20);
        assert_eq!(crash_delay_seconds(3), 40);
        assert_eq!(crash_delay_seconds(4), 60);
        assert_eq!(crash_delay_seconds(50), 60);
    }
}
