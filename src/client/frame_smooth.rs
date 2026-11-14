//! Smoothing of the frame time the world moves by (see the main loop in `app.rs`).

/// The longest step the world takes in one frame.
pub const MAX_STEP: f32 = 0.1;

/// The next smoothed frame time from the previous one (`smooth`) and the real one (`real`).
/// A running average evens out the wobble; a hitch (a window drag, a breakpoint, waking
/// from sleep) or a sudden speed-up is taken as it is, but never kept above `MAX_STEP`, so
/// that the frames after a pause do not run the world too fast while the average decays.
pub fn next(smooth: f32, real: f32) -> f32 {
    if !real.is_finite() || real <= 0.0 {
        return if smooth.is_finite() && smooth > 0.0 { smooth.min(MAX_STEP) } else { 0.0 };
    }
    let real = real.min(1.0);
    let smooth = if smooth.is_finite() { smooth } else { 0.0 };
    if smooth <= 0.0 || real > smooth * 2.5 + 0.004 || real * 2.5 + 0.004 < smooth {
        real.min(MAX_STEP)
    } else {
        (smooth + (real - smooth) * 0.2).min(MAX_STEP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pause_does_not_make_the_next_frames_run_fast() {
        let mut s = next(0.0, 0.016);
        for _ in 0..10 {
            s = next(s, 0.016);
        }
        // Five seconds of nothing (the window was dragged).
        s = next(s, 5.0);
        assert!(s <= MAX_STEP);
        // Back to normal frames: the world time per frame is the real one at once.
        s = next(s, 0.016);
        assert!((s - 0.016).abs() < 1e-6, "{s}");
    }

    #[test]
    fn wobble_is_evened_out_and_the_total_stays_the_real_time() {
        let reals = [0.015f32, 0.018, 0.014, 0.019, 0.016, 0.015, 0.017, 0.016];
        let mut s = 0.016;
        let (mut sum_s, mut sum_r) = (0.0f32, 0.0f32);
        for r in reals.iter().cycle().take(800) {
            s = next(s, *r);
            sum_s += s;
            sum_r += *r;
        }
        assert!((sum_s - sum_r).abs() < 0.05 * sum_r, "{sum_s} {sum_r}");
    }

    #[test]
    fn nonsense_is_harmless() {
        assert_eq!(next(0.0, f32::NAN), 0.0);
        assert_eq!(next(0.02, f32::NAN), 0.02);
        assert_eq!(next(f32::NAN, 0.016), 0.016);
        assert_eq!(next(0.0, f32::INFINITY), 0.0);
        assert_eq!(next(0.5, -1.0), MAX_STEP);
        assert!(next(0.016, 1.0e30) <= MAX_STEP);
    }
}
