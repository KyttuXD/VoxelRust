//! When the occlusion walk is worth its cost (see `cull.rs`).
//!
//! On flat country the walk finds nothing to hide and only costs time (about
//! half a millisecond at view distance 32). [`Adaptive`] watches how many of
//! the sections with geometry the walk hid over the last 60 frames it ran;
//! below 2 percent it is switched off, and every 30 frames it runs once more
//! to see whether that is still right (one good frame turns it back on).
//! Skipping only means that sections are judged by the view frustum alone, as
//! with `--show nocull`: it can show more, never less.

/// Frames the walk runs before its benefit is judged.
pub const WINDOW: u32 = 60;
/// While it is off, it runs again every this many frames.
pub const RETEST: u32 = 30;

#[derive(Clone, Copy, Debug, Default)]
pub struct Adaptive {
    skipping: bool,
    /// Frames since the last test while skipping.
    idle: u32,
    frames: u32,
    total: u64,
    hidden: u64,
}

impl Adaptive {
    pub fn new() -> Adaptive {
        Adaptive::default()
    }

    /// Whether the walk is off at the moment (for the F3 screen).
    pub fn skipping(&self) -> bool {
        self.skipping
    }

    /// Call once per frame: should the walk run this frame?
    pub fn should_run(&mut self) -> bool {
        if !self.skipping {
            return true;
        }
        self.idle += 1;
        if self.idle >= RETEST {
            self.idle = 0;
            return true;
        }
        false
    }

    /// Report a frame in which the walk ran: `total` sections with geometry,
    /// `hidden` of them (in view, but behind rock) not drawn because of it.
    pub fn record(&mut self, total: u32, hidden: u32) {
        if total == 0 {
            return;
        }
        if self.skipping {
            // A test frame: one frame that hides 2 percent or more is enough.
            if (hidden as u64) * 50 >= total as u64 {
                self.skipping = false;
                self.reset();
            }
            return;
        }
        self.frames += 1;
        self.total += total as u64;
        self.hidden += hidden as u64;
        if self.frames >= WINDOW {
            if self.hidden * 50 < self.total {
                self.skipping = true;
                self.idle = 0;
            }
            self.reset();
        }
    }

    fn reset(&mut self) {
        self.frames = 0;
        self.total = 0;
        self.hidden = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_walk_that_hides_nothing_is_switched_off_after_a_window() {
        let mut a = Adaptive::new();
        for _ in 0..WINDOW - 1 {
            assert!(a.should_run());
            a.record(1000, 0);
            assert!(!a.skipping());
        }
        assert!(a.should_run());
        a.record(1000, 0);
        assert!(a.skipping());
    }

    #[test]
    fn a_useful_walk_stays_on() {
        let mut a = Adaptive::new();
        for _ in 0..WINDOW * 3 {
            assert!(a.should_run());
            a.record(1000, 100);
        }
        assert!(!a.skipping());
    }

    #[test]
    fn it_is_retested_every_thirty_frames_and_comes_back_on_a_good_frame() {
        let mut a = Adaptive::new();
        for _ in 0..WINDOW {
            a.should_run();
            a.record(500, 1);
        }
        assert!(a.skipping());
        let mut ran = 0;
        for _ in 0..RETEST - 1 {
            if a.should_run() {
                ran += 1;
            }
        }
        assert_eq!(ran, 0);
        assert!(a.should_run());
        // Still nothing hidden: stays off.
        a.record(500, 2);
        assert!(a.skipping());
        for _ in 0..RETEST - 1 {
            assert!(!a.should_run());
        }
        assert!(a.should_run());
        // Now a cave: 5 percent hidden.
        a.record(500, 25);
        assert!(!a.skipping());
        assert!(a.should_run());
    }

    #[test]
    fn empty_frames_do_not_count() {
        let mut a = Adaptive::new();
        for _ in 0..WINDOW * 2 {
            a.record(0, 0);
        }
        assert!(!a.skipping());
    }
}
