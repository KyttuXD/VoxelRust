//! The restart of the public server into an installed update: tell the
//! players, give them 30 seconds, save, go.

use core::sync::atomic::{AtomicI32, Ordering};

/// How long players are given between the first warning and the restart.
pub const WARNING_SECONDS: f64 = 30.0;

/// What the server loop should do now.
#[derive(Debug, PartialEq)]
pub enum Step {
    Wait,
    /// Say this in chat (and in the log).
    Say(&'static str),
    /// Save and restart now.
    Go,
}

/// The countdown. Feed it the server's state every loop; it never un-starts.
#[derive(Default)]
pub struct RestartPlan {
    started: Option<f64>,
    told_ten: bool,
}

impl RestartPlan {
    /// `ready`: an update is installed. `players`: how many are connected.
    /// `now`: seconds on any steady clock.
    pub fn step(&mut self, ready: bool, players: usize, now: f64) -> Step {
        if !ready {
            return Step::Wait;
        }
        // Nobody to warn: go at once.
        if players == 0 {
            return Step::Go;
        }
        let Some(t0) = self.started else {
            self.started = Some(now);
            return Step::Say("RESTARTING FOR AN UPDATE IN 30 SECONDS");
        };
        let waited = now - t0;
        if waited >= WARNING_SECONDS {
            Step::Go
        } else if waited >= WARNING_SECONDS - 10.0 && !self.told_ten {
            self.told_ten = true;
            Step::Say("RESTARTING FOR AN UPDATE IN 10 SECONDS")
        } else {
            Step::Wait
        }
    }
}

/// The program's exit code: 0, or 75 when `scripts/server-supervisor.sh`
/// is to start it again.
static EXIT_CODE: AtomicI32 = AtomicI32::new(0);

pub fn set_exit_code(code: i32) {
    EXIT_CODE.store(code, Ordering::Release);
}

pub fn exit_code() -> i32 {
    EXIT_CODE.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_happens_until_an_update_is_installed() {
        let mut p = RestartPlan::default();
        assert_eq!(p.step(false, 3, 0.0), Step::Wait);
        assert_eq!(p.step(false, 0, 100.0), Step::Wait);
    }

    #[test]
    fn an_empty_server_restarts_at_once() {
        let mut p = RestartPlan::default();
        assert_eq!(p.step(true, 0, 5.0), Step::Go);
    }

    #[test]
    fn players_are_warned_then_given_thirty_seconds() {
        let mut p = RestartPlan::default();
        assert_eq!(p.step(true, 2, 100.0), Step::Say("RESTARTING FOR AN UPDATE IN 30 SECONDS"));
        assert_eq!(p.step(true, 2, 110.0), Step::Wait);
        assert_eq!(p.step(true, 2, 120.0), Step::Say("RESTARTING FOR AN UPDATE IN 10 SECONDS"));
        assert_eq!(p.step(true, 2, 125.0), Step::Wait, "the 10 second warning comes once");
        assert_eq!(p.step(true, 2, 129.9), Step::Wait);
        assert_eq!(p.step(true, 2, 130.0), Step::Go);
    }

    #[test]
    fn the_countdown_ends_early_when_everyone_leaves() {
        let mut p = RestartPlan::default();
        assert_eq!(p.step(true, 1, 0.0), Step::Say("RESTARTING FOR AN UPDATE IN 30 SECONDS"));
        assert_eq!(p.step(true, 0, 4.0), Step::Go);
    }
}
