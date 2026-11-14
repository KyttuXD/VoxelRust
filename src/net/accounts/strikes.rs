//! Counting wrong PINs, and the lockouts that follow: 5 wrong tries lock for a minute,
//! each further lockout doubles it (1, 2, 4, 8, 16, 32 minutes, then an hour at most).
//! The same record is kept for each account and for each address the tries come from.

/// Wrong tries before a lockout.
pub const MAX_TRIES: u32 = 5;
/// The first lockout, in seconds.
pub const FIRST_LOCK: u64 = 60;
/// No lockout is longer than this.
pub const MAX_LOCK: u64 = 3600;
/// Wrong tries are forgotten after this long without a new one; the doubling after a week.
const FORGET_TRIES_AFTER: u64 = 86_400;
const FORGET_LEVEL_AFTER: u64 = 7 * 86_400;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Strikes {
    /// Wrong tries since the last lockout.
    pub fails: u32,
    /// How many lockouts there have been (the next one lasts `FIRST_LOCK << level`).
    pub level: u32,
    /// Locked until this time (seconds since 1970); 0 or the past: not locked.
    pub locked_until: u64,
    /// When the last wrong try was.
    pub last: u64,
}

/// What a wrong try led to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fail {
    /// Not locked yet: this many tries are left.
    Left(u32),
    /// Locked until this time.
    Locked(u64),
}

impl Strikes {
    /// Locked at `now`? Then until when.
    pub fn locked(&self, now: u64) -> Option<u64> {
        if now < self.locked_until {
            Some(self.locked_until)
        } else {
            None
        }
    }

    pub fn tries_left(&self) -> u32 {
        MAX_TRIES.saturating_sub(self.fails).max(1)
    }

    /// Forget what is old.
    fn decay(&mut self, now: u64) {
        if now >= self.locked_until && now.saturating_sub(self.last) > FORGET_TRIES_AFTER {
            self.fails = 0;
        }
        if now >= self.locked_until && now.saturating_sub(self.last) > FORGET_LEVEL_AFTER {
            self.level = 0;
        }
    }

    /// A wrong PIN at `now`.
    pub fn fail(&mut self, now: u64) -> Fail {
        self.decay(now);
        self.fails += 1;
        self.last = now;
        if self.fails >= MAX_TRIES {
            let lock = (FIRST_LOCK << self.level.min(10)).min(MAX_LOCK);
            self.level = self.level.saturating_add(1);
            self.fails = 0;
            self.locked_until = now + lock;
            Fail::Locked(self.locked_until)
        } else {
            Fail::Left(MAX_TRIES - self.fails)
        }
    }

    /// The right PIN: the account is forgiven (not so an address, see `AddrRecord`).
    pub fn forgive(&mut self) {
        *self = Strikes::default();
    }

    /// Nothing worth remembering.
    pub fn is_clear(&self, now: u64) -> bool {
        self.fails == 0 && self.level == 0 && now >= self.locked_until
    }
}

/// "locked until 14:05 UTC (3 minutes)" from the time of day and what is left.
pub fn lock_text(until: u64, now: u64) -> alloc::string::String {
    let left = until.saturating_sub(now);
    let how_long = if left >= 90 { alloc::format!("{} minutes", (left + 30) / 60) } else { alloc::format!("{} seconds", left.max(1)) };
    alloc::format!("too many wrong PINs: locked until {} UTC ({} from now)", crate::sys::clock::clock_text(until), how_long)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_wrong_tries_lock_and_the_lockout_doubles() {
        let mut s = Strikes::default();
        let mut now = 1_000_000u64;
        let mut locks = alloc::vec::Vec::new();
        for round in 0..9 {
            for k in 1..MAX_TRIES {
                assert_eq!(s.fail(now), Fail::Left(MAX_TRIES - k), "round {round} try {k}");
            }
            match s.fail(now) {
                Fail::Locked(until) => locks.push(until - now),
                other => panic!("{other:?}"),
            }
            assert!(s.locked(now).is_some());
            assert!(s.locked(now + locks[round] - 1).is_some());
            assert!(s.locked(now + locks[round]).is_none());
            // The next try comes the moment the lock ends.
            now += locks[round];
        }
        assert_eq!(locks, [60, 120, 240, 480, 960, 1920, 3600, 3600, 3600]);
    }

    #[test]
    fn old_wrong_tries_and_lockouts_are_forgotten_and_a_right_pin_forgives() {
        let mut s = Strikes::default();
        for _ in 0..4 {
            s.fail(1000);
        }
        assert_eq!(s.tries_left(), 1);
        // A day later the four are gone.
        assert_eq!(s.fail(1000 + 86_401), Fail::Left(4));
        let mut s = Strikes::default();
        for _ in 0..5 {
            s.fail(1000);
        }
        assert_eq!(s.level, 1);
        // A week and more later the doubling starts over.
        for _ in 0..4 {
            s.fail(1000 + 8 * 86_400);
        }
        assert_eq!(s.level, 0);
        assert!(matches!(s.fail(1000 + 8 * 86_400), Fail::Locked(u) if u == 1000 + 8 * 86_400 + 60));
        s.forgive();
        assert!(s.is_clear(0));
    }

    #[test]
    fn the_text_says_when_and_for_how_long() {
        let t = lock_text(14 * 3600 + 5 * 60, 14 * 3600 + 2 * 60);
        assert!(t.contains("14:05 UTC") && t.contains("3 minutes"), "{t}");
        assert!(lock_text(100, 70).contains("30 seconds"));
    }
}
