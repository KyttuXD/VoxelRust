//! The swing of another player's arm, from the swing flag of their pose byte.
//!
//! The pose byte only says that the arm is swinging (`pose::SWING`), a few
//! times a second at most. The one who watches starts the swing when the flag
//! comes up and plays it through in [`SECONDS`]: the right arm goes back, then
//! forward and up, and the forearm follows (`activity::strike`). If the flag
//! stays up, because blows follow one another, the swing goes round again.

use crate::math::floorf;

/// How long one swing takes.
pub const SECONDS: f32 = 0.3;
/// "Long ago": what the time since a swing started is when none has.
pub const REST: f32 = 1000.0;
/// A flag that is still up this long after it came up means blows are following
/// one another: the swing goes round again (a single blow's flag is gone by then).
const AGAIN_AFTER: f32 = 0.4;

/// Whether the flag came up between two pose bytes.
pub fn starts(before: u8, after: u8) -> bool {
    let up = |p: u8| super::unpack(p).1 & super::SWING != 0;
    up(after) && !up(before)
}

/// How far through its swing the arm is, 0..1 (never exactly 0, which means
/// "not swinging"): `since` seconds after the flag came up, `flag` whether it is
/// up now. None when the arm is at rest.
pub fn phase(since: f32, flag: bool) -> Option<f32> {
    if !(since >= 0.0) {
        return None;
    }
    if since < SECONDS {
        return Some((since / SECONDS).max(0.001));
    }
    if flag && since >= AGAIN_AFTER {
        let x = since / SECONDS;
        return Some((x - floorf(x)).max(0.001));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::pose::{pack, Kind, BOW, SWING};

    #[test]
    fn a_swing_starts_when_the_flag_comes_up_and_not_while_it_stays_up() {
        let rest = pack(Kind::Stand, 0);
        let up = pack(Kind::Stand, SWING);
        assert!(starts(rest, up));
        assert!(!starts(up, up));
        assert!(!starts(up, rest));
        // Another flag, or another posture, is no swing.
        assert!(!starts(rest, pack(Kind::Stand, BOW)));
        assert!(starts(pack(Kind::Crouch, BOW), pack(Kind::Crouch, BOW | SWING)));
    }

    #[test]
    fn the_swing_runs_through_in_three_tenths_of_a_second_and_stops() {
        assert_eq!(phase(REST, false), None);
        assert_eq!(phase(-1.0, true), None);
        assert_eq!(phase(f32::NAN, true), None);
        let first = phase(0.0, true).unwrap();
        assert!(first > 0.0 && first < 0.01);
        assert!((phase(0.15, true).unwrap() - 0.5).abs() < 1e-5);
        assert!(phase(0.29, false).unwrap() > 0.95);
        // The flag is gone (a single blow): at rest once the swing is done.
        assert_eq!(phase(0.3, false), None);
        assert_eq!(phase(0.35, true), None);
    }

    #[test]
    fn a_flag_that_stays_up_makes_the_swing_go_round_again() {
        let p = phase(0.45, true).unwrap();
        assert!((p - 0.5).abs() < 1e-4);
        let q = phase(0.6, true).unwrap();
        assert!(q > 0.0 && q <= 1.0);
        assert_eq!(phase(0.45, false), None);
        // Always a phase the arm maths takes.
        for k in 0..400 {
            if let Some(p) = phase(k as f32 * 0.01, true) {
                assert!(p > 0.0 && p <= 1.0);
            }
        }
    }
}
