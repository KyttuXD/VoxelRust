//! The game's own restart into an installed update. From a menu: a short 3 second countdown
//! (so the player sees it coming). From a world: the 30 second warning of `restart`. Never
//! while the player types into a text field or while a server's mods or world are arriving:
//! the countdown starts again once that is over.

use alloc::string::String;

/// Seconds of warning in a menu.
pub const MENU_SECONDS: f64 = 3.0;

/// What the game is doing, as far as a restart is concerned.
#[derive(Clone, Copy, Debug, Default)]
pub struct Busy {
    /// A world is open (a game screen, or a menu over one).
    pub in_world: bool,
    /// A text field has the keyboard (a name, an address, chat, a PIN...).
    pub typing: bool,
    /// A server's mods or world are being received.
    pub downloading: bool,
}

#[derive(Debug, PartialEq)]
pub enum Step {
    Wait,
    /// Show this to the player.
    Say(String),
    /// A countdown that was running is off (the player began to type, or a download began).
    Cancel,
    /// Save and restart now.
    Go,
}

#[derive(Default)]
pub struct GameRestart {
    started: Option<(f64, bool)>,
    told_ten: bool,
}

impl GameRestart {
    pub fn step(&mut self, ready: bool, busy: Busy, now: f64) -> Step {
        if !ready || busy.typing || busy.downloading || !now.is_finite() {
            // Not now: the countdown, if there was one, starts over later.
            self.told_ten = false;
            return if self.started.take().is_some() { Step::Cancel } else { Step::Wait };
        }
        let (t0, was_world) = match self.started {
            Some(s) => s,
            None => {
                self.started = Some((now, busy.in_world));
                self.told_ten = false;
                let text = if busy.in_world { "RESTARTING FOR AN UPDATE IN 30 SECONDS" } else { "RESTARTING FOR AN UPDATE IN 3 SECONDS" };
                return Step::Say(String::from(text));
            }
        };
        // Went into or out of a world during the countdown: start it again for the new place.
        if was_world != busy.in_world {
            self.started = None;
            return Step::Wait;
        }
        let total = if busy.in_world { super::restart::WARNING_SECONDS } else { MENU_SECONDS };
        let waited = now - t0;
        if waited < 0.0 {
            // The clock went back: count again.
            self.started = Some((now, busy.in_world));
            Step::Wait
        } else if waited >= total {
            Step::Go
        } else if busy.in_world && waited >= total - 10.0 && !self.told_ten {
            self.told_ten = true;
            Step::Say(String::from("RESTARTING FOR AN UPDATE IN 10 SECONDS"))
        } else {
            Step::Wait
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MENU: Busy = Busy { in_world: false, typing: false, downloading: false };
    const WORLD: Busy = Busy { in_world: true, typing: false, downloading: false };

    #[test]
    fn a_menu_counts_three_seconds_first() {
        let mut p = GameRestart::default();
        assert!(matches!(p.step(true, MENU, 10.0), Step::Say(_)));
        assert_eq!(p.step(true, MENU, 11.0), Step::Wait);
        assert_eq!(p.step(true, MENU, 13.0), Step::Go);
    }

    #[test]
    fn typing_and_downloads_hold_it_back_and_the_count_starts_over() {
        let mut p = GameRestart::default();
        assert!(matches!(p.step(true, MENU, 0.0), Step::Say(_)));
        let typing = Busy { typing: true, ..MENU };
        assert_eq!(p.step(true, typing, 2.0), Step::Cancel);
        assert_eq!(p.step(true, typing, 500.0), Step::Wait);
        assert!(matches!(p.step(true, MENU, 501.0), Step::Say(_)), "a fresh countdown");
        assert_eq!(p.step(true, MENU, 502.0), Step::Wait);
        let dl = Busy { downloading: true, ..WORLD };
        assert_eq!(p.step(true, dl, 600.0), Step::Wait);
    }

    #[test]
    fn a_world_gets_thirty_seconds_and_one_ten_second_warning() {
        let mut p = GameRestart::default();
        assert!(matches!(p.step(true, WORLD, 0.0), Step::Say(_)));
        assert!(matches!(p.step(true, WORLD, 20.0), Step::Say(_)));
        assert_eq!(p.step(true, WORLD, 25.0), Step::Wait);
        assert_eq!(p.step(true, WORLD, 30.0), Step::Go);
    }

    #[test]
    fn changing_between_menu_and_world_starts_over_and_odd_clocks_are_safe() {
        let mut p = GameRestart::default();
        assert!(matches!(p.step(true, MENU, 0.0), Step::Say(_)));
        assert_eq!(p.step(true, WORLD, 1.0), Step::Wait);
        assert!(matches!(p.step(true, WORLD, 2.0), Step::Say(_)));
        assert_eq!(p.step(true, WORLD, f64::NAN), Step::Cancel);
        assert_eq!(p.step(false, MENU, 3.0), Step::Wait);
        let mut q = GameRestart::default();
        assert!(matches!(q.step(true, MENU, 100.0), Step::Say(_)));
        assert_eq!(q.step(true, MENU, 50.0), Step::Wait, "the clock went back");
        assert_eq!(q.step(true, MENU, 52.9), Step::Wait);
        assert_eq!(q.step(true, MENU, 53.0), Step::Go);
    }
}
