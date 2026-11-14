//! `--show bends:NAME[@PHASE]`: a still of one posture and gait of the player's
//! own body (use it with `third` or `front`). The names: `idle`, `walk`,
//! `sprint`, `crouchwalk`, `swim`, `climb`, `sit`, `sleep`, `fall`, `ride`.
//! PHASE is the point of the stride in radians (default 1.1, a leg well forward).

use crate::content::pose::Kind;

/// What a still shows: the posture, and the gait as (phase, stride, run).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shown {
    pub kind: Kind,
    pub gait: (f32, f32, f32),
}

/// The gait of a name, or `None` for a name that is not one.
pub fn parse(spec: &str) -> Option<Shown> {
    let (name, at) = match spec.split_once('@') {
        Some((n, p)) => (n, p.parse::<f32>().ok().filter(|p| p.is_finite())),
        None => (spec, None),
    };
    let phase = at.unwrap_or(1.1).clamp(-1000.0, 1000.0);
    let (kind, stride, run) = match name.to_ascii_lowercase().as_str() {
        "idle" | "stand" => (Kind::Stand, 0.0, 0.0),
        "walk" => (Kind::Stand, 1.0, 0.0),
        "sprint" | "run" => (Kind::Stand, 1.0, 1.0),
        "crouchwalk" | "crouch" => (Kind::Crouch, 1.0, 0.0),
        "swim" => (Kind::Swim, 1.0, 0.0),
        "climb" => (Kind::Climb, 1.0, 0.0),
        "sit" => (Kind::Sit, 0.0, 0.0),
        "sleep" => (Kind::Sleep, 0.0, 0.0),
        "fall" => (Kind::Fall, 0.0, 0.0),
        "ride" => (Kind::Ride, 0.0, 0.0),
        _ => return None,
    };
    Some(Shown { kind, gait: (phase, stride, run) })
}

/// Hold the player's body in the gait named by `--show` (the whole option string), if it has `bends:`.
pub fn apply(show: &str, posing: &mut crate::game::posing::Posing) {
    if let Some(spec) = show.split(',').find_map(|w| w.strip_prefix("bends:")) {
        match parse(spec) {
            Some(s) => {
                posing.forced = Some(s.kind);
                posing.shown_gait = Some(s.gait);
            }
            None => crate::logf!("voxelrust: --show bends:{}: no such pose (idle walk sprint crouchwalk swim climb sit sleep fall ride)", spec),
        }
    }
    // "swing:F": the arm F (0 to 1) of the way through a punch, as another player's is seen.
    if let Some(f) = show.split(',').find_map(|w| w.strip_prefix("swing:")).and_then(|f| f.parse::<f32>().ok()) {
        posing.shown_swing = Some(if f.is_finite() { f.clamp(0.001, 1.0) } else { 0.5 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_swing_for_a_picture_is_kept_inside_its_range() {
        let mut p = crate::game::posing::Posing::default();
        apply("third,swing:0.4", &mut p);
        assert_eq!(p.shown_swing, Some(0.4));
        apply("swing:7", &mut p);
        assert_eq!(p.shown_swing, Some(1.0));
        let mut q = crate::game::posing::Posing::default();
        apply("third", &mut q);
        assert_eq!(q.shown_swing, None);
    }

    #[test]
    fn names_give_postures_and_gaits() {
        assert_eq!(parse("walk").unwrap().kind, Kind::Stand);
        assert_eq!(parse("sprint").unwrap().gait.2, 1.0);
        assert_eq!(parse("Walk").unwrap().gait.2, 0.0);
        assert_eq!(parse("crouchwalk").unwrap().kind, Kind::Crouch);
        assert_eq!(parse("idle").unwrap().gait.1, 0.0);
        assert_eq!(parse("swim@2.5").unwrap().gait.0, 2.5);
        assert_eq!(parse("walk@oops").unwrap().gait.0, 1.1);
        assert_eq!(parse("walk@NaN").unwrap().gait.0, 1.1);
        assert!(parse("moonwalk").is_none());
    }
}
