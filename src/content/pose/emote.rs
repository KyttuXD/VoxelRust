//! Playing an emote on a body: which one is playing, how far along, and the
//! rotation it gives each bone (see `content::emotes` for the data).

use super::Body;
use crate::content::emotes::Emote;

/// What `Body::emote` holds for "no emote" and for stopping one.
pub const NONE: u16 = 0;

/// The code of the emote at place `index` of the registry.
pub fn code(index: usize) -> u16 {
    (index + 1).min(u16::MAX as usize) as u16
}

/// The emote a code stands for, if the registry has it.
pub fn of(code: u16) -> Option<&'static Emote> {
    (code as usize).checked_sub(1).and_then(|i| crate::block::reg().emotes.get(i))
}

/// The emote a body is playing, and the time into it, while it lasts.
pub fn playing(b: &Body) -> Option<(&'static Emote, f32)> {
    let e = of(b.emote)?;
    e.at(b.emote_t).map(|t| (e, t))
}

/// Add the rotation the playing emote gives the bone called `bone` (lower case).
pub fn add(bone: &str, b: &Body, rot: &mut [f32; 3]) {
    if b.emote == NONE {
        return;
    }
    if let Some((e, t)) = playing(b) {
        if let Some(r) = e.rotation(bone, t) {
            for i in 0..3 {
                rot[i] += r[i];
            }
        }
    }
}

/// Whether the emote being played moves an arm (so the first-person view shows it).
pub fn shows_arm(b: &Body) -> bool {
    playing(b).map_or(false, |(e, _)| e.uses_arm())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_count_from_one_and_unknown_ones_do_nothing() {
        assert_eq!(code(0), 1);
        assert_eq!(code(9), 10);
        assert!(of(NONE).is_none());
        assert!(of(60_000).is_none());
        let mut rot = [1.0, 2.0, 3.0];
        add("r-arm", &Body { emote: 60_000, ..Body::default() }, &mut rot);
        assert_eq!(rot, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn the_wave_that_comes_with_the_game_raises_the_right_arm() {
        let reg = crate::block::reg();
        let i = reg.emotes.find("wave").expect("the base data has a wave");
        let b = Body { emote: code(i), emote_t: 0.9, ..Body::default() };
        let mut rot = [0.0; 3];
        add("r-arm", &b, &mut rot);
        // Out to the side and up: the right arm turns about z, positive.
        assert!(rot[2] > 90.0, "{rot:?}");
        assert!(shows_arm(&b));
        // Over by the end of it.
        let done = Body { emote_t: 99.0, ..b };
        assert!(playing(&done).is_none());
    }
}
