//! How lit the things drawn in the world are (methods of `App`): the sky's
//! share, and the share of the hand. The block light of lava and lamps is in
//! `client/block_light.rs`.

#[allow(unused_imports)]
use super::*;

impl App {
    /// Daylight now, 0 to 1 (the dimension's sky counted).
    pub(super) fn day_light(g: &Session) -> f32 {
        let look = crate::game::fx::sky::current(g);
        let (_, _, day) = crate::render::sky_colors(look.time_of(g.time));
        look.light(day)
    }

    /// The sky's light at a spot, 0 to 1: full daylight under open sky, dimmer
    /// under cover. Never so dark that a creature cannot be seen against a
    /// night sky (zombies and the like come out at night): about two fifths
    /// of daylight at the least.
    pub(super) fn sky_light_at(g: &Session, day: f32, pos: [f64; 3]) -> f32 {
        let reg = reg();
        let (x, z) = (floord(pos[0]), floord(pos[2]));
        let covered = (floord(pos[1]) + 2..CH).any(|y| reg.opaque[g.world.get(x, y, z) as usize] != 0);
        (0.40 + 0.60 * day) * if covered { 0.75 } else { 1.0 }
    }

    /// How bright the first-person hand and what it holds are: lit like a
    /// creature standing at the eye (the sky, or lava and lamps close by),
    /// but never darker than about half (a hand in the dark is still seen).
    pub(super) fn hand_light(g: &Session, eye: [f64; 3]) -> f32 {
        let sky = Self::sky_light_at(g, Self::day_light(g), eye);
        let level = crate::client::block_light::level_in_world(&g.world, [eye[0], eye[1] - 0.5, eye[2]]);
        0.55 + 0.45 * crate::client::block_light::lit(sky, level)
    }
}
