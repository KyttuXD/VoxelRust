//! Start-up timing: one log line for each phase before the first frame, so a
//! slow start can be pinned on a step (`--log FILE`, lines starting
//! `voxelrust: startup`).

use crate::sys;

/// The time since the start of the program, and since the last mark.
pub struct Phases {
    began: f64,
    last: f64,
}

impl Phases {
    pub fn new() -> Phases {
        let now = sys::now();
        Phases { began: now, last: now }
    }

    /// Log how long the phase that just ended took.
    pub fn mark(&mut self, what: &str) {
        let now = sys::now();
        crate::logf!("voxelrust: startup {:>6.0} ms  {} (+{:.0} ms)", (now - self.began) * 1000.0, what, (now - self.last) * 1000.0);
        self.last = now;
    }
}
