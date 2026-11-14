//! What one connection may ask the server for: land only within
//! [`LIMIT_BLOCKS`] of the origin, at most [`MAX_IN_FLIGHT`] chunks that have
//! not been answered yet, and [`PER_SECOND`] chunks a second. A plain struct,
//! tested without sockets.

/// Farthest from the origin (in blocks, on each axis) that land is sent.
pub const LIMIT_BLOCKS: i64 = 30_000;
/// Requests of one connection that may wait for their answer at once.
pub const MAX_IN_FLIGHT: usize = 16;
/// Chunks a second one connection may ask for (a burst of as many is allowed).
pub const PER_SECOND: f32 = 200.0;
/// Refused requests in a row (less what the time since has forgiven) after
/// which the connection is cut instead of merely ignored.
const ABUSE_LIMIT: f32 = 3000.0;

/// Whether chunk (cx, cz) lies within [`LIMIT_BLOCKS`] of the origin.
pub fn in_range(cx: i32, cz: i32) -> bool {
    let ok = |c: i32| {
        let lo = i64::from(c) * 16;
        lo >= -LIMIT_BLOCKS && lo + 15 <= LIMIT_BLOCKS
    };
    ok(cx) && ok(cz)
}

/// What became of a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Take it: make or fetch the chunk.
    Accept,
    /// The same chunk is already being answered: nothing to do.
    Duplicate,
    /// Too many at once or too fast: ignored (the client asks again later).
    Refuse,
    /// Not a place the server serves: ignored.
    Invalid,
}

/// One connection's allowance.
#[derive(Clone, Debug)]
pub struct ConnChunks {
    pending: alloc::vec::Vec<(i32, i32)>,
    tokens: f32,
    abuse: f32,
}

impl Default for ConnChunks {
    fn default() -> ConnChunks {
        ConnChunks { pending: alloc::vec::Vec::new(), tokens: PER_SECOND, abuse: 0.0 }
    }
}

impl ConnChunks {
    /// Let `dt` seconds pass.
    pub fn refill(&mut self, dt: f32) {
        // (NaN or a negative step must not poison the allowance.)
        let dt = if dt > 0.0 && dt < 3600.0 { dt } else { 0.0 };
        self.tokens = (self.tokens + dt * PER_SECOND).min(PER_SECOND);
        self.abuse = (self.abuse - dt * 100.0).max(0.0);
    }

    /// A request for chunk (cx, cz).
    pub fn ask(&mut self, cx: i32, cz: i32) -> Verdict {
        if !in_range(cx, cz) {
            self.abuse += 1.0;
            return Verdict::Invalid;
        }
        if self.pending.contains(&(cx, cz)) {
            // (An honest client asks again after three seconds, not a thousand times a second.)
            self.abuse += 1.0;
            return Verdict::Duplicate;
        }
        if self.pending.len() >= MAX_IN_FLIGHT || self.tokens < 1.0 {
            self.abuse += 1.0;
            return Verdict::Refuse;
        }
        self.tokens -= 1.0;
        self.pending.push((cx, cz));
        Verdict::Accept
    }

    /// The answer to chunk (cx, cz) has been sent (or given up): its place is free.
    pub fn done(&mut self, cx: i32, cz: i32) {
        self.pending.retain(|&k| k != (cx, cz));
    }

    /// Requests waiting for an answer.
    pub fn in_flight(&self) -> usize {
        self.pending.len()
    }

    /// Whether this connection keeps asking for far more than it may.
    pub fn abusive(&self) -> bool {
        self.abuse > ABUSE_LIMIT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_land_within_thirty_thousand_blocks_is_served() {
        assert!(in_range(0, 0));
        assert!(in_range(1874, -1875));
        assert!(!in_range(1875, 0), "its far edge is 30015");
        assert!(!in_range(0, -1876));
        assert!(!in_range(i32::MAX, 0) && !in_range(0, i32::MIN));
        assert!(!in_range(i32::MIN, i32::MIN));
    }

    #[test]
    fn sixteen_at_once_and_no_more_until_one_is_answered() {
        let mut c = ConnChunks::default();
        for i in 0..MAX_IN_FLIGHT as i32 {
            assert_eq!(c.ask(i, 0), Verdict::Accept);
        }
        assert_eq!(c.ask(99, 0), Verdict::Refuse);
        // Asking again for one that is on its way is no new request.
        assert_eq!(c.ask(3, 0), Verdict::Duplicate);
        assert_eq!(c.in_flight(), 16);
        c.done(3, 0);
        assert_eq!(c.ask(99, 0), Verdict::Accept);
        assert_eq!(c.ask(3, 0), Verdict::Refuse);
    }

    #[test]
    fn two_hundred_a_second_and_time_gives_more() {
        let mut c = ConnChunks::default();
        let mut taken = 0;
        for i in 0..1000 {
            let key = (i % 100, i / 100);
            if c.ask(key.0, key.1) == Verdict::Accept {
                taken += 1;
            }
            c.done(key.0, key.1);
        }
        assert_eq!(taken, 200);
        c.refill(0.25);
        let again = (0..1000).filter(|&i| c.ask(i, 5) == Verdict::Accept && {
            c.done(i, 5);
            true
        }).count();
        assert_eq!(again, 50);
        // Nonsense time steps change nothing.
        let mut d = ConnChunks::default();
        d.refill(f32::NAN);
        d.refill(-3.0);
        assert_eq!(d.ask(0, 0), Verdict::Accept);
    }

    #[test]
    fn a_flood_of_bad_requests_is_abusive_and_is_forgiven_with_time() {
        let mut c = ConnChunks::default();
        for _ in 0..1000 {
            assert_eq!(c.ask(100_000, 0), Verdict::Invalid);
        }
        assert!(!c.abusive());
        for _ in 0..3000 {
            c.ask(100_000, 0);
        }
        assert!(c.abusive());
        c.refill(60.0);
        assert!(!c.abusive());
    }
}
