//! Limits on how much finished geometry is handed to the graphics card in
//! one frame, and in what order.
//!
//! Sending a mesh to the card (`glBufferData`) is done on the main thread.
//! When a whole ring of chunks finishes at once (on arriving in a world, or
//! after the light changes), sending them all in one frame makes the frame
//! stall. Instead a few milliseconds a frame are spent on it, nearest chunks
//! first; the rest wait for the next frame.

/// Seconds a frame may spend uploading (after the first two meshes, which
/// always go, so loading never stalls).
pub const LIMIT: f64 = 0.006;
/// Meshes that are always uploaded in a frame.
pub const MINIMUM: u32 = 2;

/// The time spent uploading in one frame.
pub struct Budget {
    start: Option<f64>,
    used: u32,
}

impl Budget {
    /// Start counting. Without a window (tests, a server) there is no clock
    /// and no limit.
    pub fn start() -> Budget {
        let start = if crate::sys::sdl::loaded() { Some(crate::sys::now()) } else { None };
        Budget { start, used: 0 }
    }

    /// Whether one more upload may be made now.
    pub fn allow(&mut self) -> bool {
        let ok = match self.start {
            None => true,
            Some(_) if self.used < MINIMUM => true,
            Some(t0) => crate::sys::now() - t0 < LIMIT,
        };
        if ok {
            self.used += 1;
        }
        ok
    }
}

/// Squared distance in chunks, to sort by.
pub fn distance2(dx: i32, dz: i32) -> i32 {
    dx * dx + dz * dz
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_a_clock_everything_goes() {
        let mut b = Budget { start: None, used: 0 };
        assert!((0..1000).all(|_| b.allow()));
    }

    #[test]
    fn the_first_meshes_always_go() {
        // A start far in the past: the time is long used up, but two still go
        // (and no clock is asked for before that).
        let mut b = Budget { start: Some(-1.0e9), used: 0 };
        assert!(b.allow());
        assert!(b.allow());
    }

    #[test]
    fn nearer_chunks_sort_first() {
        assert!(distance2(1, 1) < distance2(0, 3));
        assert_eq!(distance2(-2, 0), distance2(0, 2));
    }
}
