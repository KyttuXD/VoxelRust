//! `--step`: how much game time each frame of a scripted run (`--screenshot`,
//! `--bench`, with or without `--run`) lasts.
//!
//! Scripted runs freeze time so that pictures are repeatable: without `--step`
//! every frame passes 0 seconds, and nothing that runs on a clock moves (mod
//! timers, creatures, the day). With `--step SECONDS` each frame is that long
//! for everything at once: the clock, the scripts' `on_tick` and `on_update`
//! `dt` and the creatures. `--step` with no number after it means 1/60 of a
//! second, a frame at 60 frames a second.
//!
//! The default (no `--step`) is deliberately unchanged, so the scripted
//! pictures made before this option was understood come out the same.

use alloc::format;
use alloc::string::String;

/// What `--step` alone means: one frame at 60 frames a second.
pub const DEFAULT_STEP: f32 = 1.0 / 60.0;
/// The longest step (a frame of the real game is clamped to the same).
pub const MAX_STEP: f32 = 0.1;

/// Read what follows `--step`. `next` is the next word of the command line,
/// if any. Returns the step and whether that word was used up (it was a
/// number); with no word, or one that is another option, the step is
/// [`DEFAULT_STEP`] and nothing is used up.
pub fn parse(next: Option<&str>) -> Result<(f32, bool), String> {
    let word = match next {
        Some(w) if !w.starts_with("--") => w,
        _ => return Ok((DEFAULT_STEP, false)),
    };
    match word.parse::<f32>() {
        Ok(v) if v.is_finite() => Ok((v.clamp(0.0, MAX_STEP), true)),
        _ => Err(format!("--step: `{}` is not a valid number", word)),
    }
}

/// The time one frame passes: in a scripted run (and not in a shared game,
/// which needs a real clock) the step, 0 when none was asked for; otherwise
/// the real time of the frame.
pub fn frame_dt(scripted_offline: bool, step: Option<f32>, real: f32) -> f32 {
    if scripted_offline {
        step.unwrap_or(0.0)
    } else {
        real
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_taken_and_used_up() {
        assert_eq!(parse(Some("0.05")), Ok((0.05, true)));
        assert_eq!(parse(Some("0")), Ok((0.0, true)));
    }

    #[test]
    fn no_number_means_a_sixtieth_of_a_second() {
        assert_eq!(parse(None), Ok((DEFAULT_STEP, false)));
        assert_eq!(parse(Some("--run")), Ok((DEFAULT_STEP, false)));
        assert!((DEFAULT_STEP - 0.016_666_668).abs() < 1e-6);
    }

    #[test]
    fn too_long_a_step_is_cut_and_nonsense_refused() {
        assert_eq!(parse(Some("5")), Ok((MAX_STEP, true)));
        assert_eq!(parse(Some("-1")), Ok((0.0, true)));
        assert!(parse(Some("nan")).is_err());
        assert!(parse(Some("inf")).is_err());
        assert!(parse(Some("fast")).is_err());
    }

    #[test]
    fn scripted_runs_without_a_step_stay_frozen_as_they_were() {
        assert_eq!(frame_dt(true, None, 0.016), 0.0);
        assert_eq!(frame_dt(true, Some(0.02), 0.5), 0.02);
        assert_eq!(frame_dt(false, Some(0.02), 0.016), 0.016);
        assert_eq!(frame_dt(false, None, 0.03), 0.03);
    }

    #[test]
    fn the_step_is_the_same_for_every_frame() {
        // Whatever the real time of a frame, the game moves by the step: the
        // clock, `on_tick` and the creatures all get this one number.
        for real in [0.001, 0.016, 0.1] {
            assert_eq!(frame_dt(true, Some(DEFAULT_STEP), real), DEFAULT_STEP);
        }
    }
}
