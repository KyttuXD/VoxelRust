//! Weather and (optional) natural disasters.
//!
//! Weather is regional: it is a function of where you are and of the
//! world's clock, so walking far enough takes you out of a storm, and
//! everyone on a server sees the same sky. Cold places get snow instead of
//! rain. Disasters are off unless the player turns them on, and each needs
//! its own conditions: tornadoes come only out of storms over open country,
//! tsunamis only follow an earthquake near the sea.

use crate::noise::{hash2, noise2};
use crate::worldgen::{self, Biome, SEA};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Sky {
    /// 0 (dry) ..= 1 (pouring).
    pub rain: f32,
    /// The precipitation falls as snow.
    pub snow: bool,
    /// Thunder and lightning.
    pub storm: bool,
}

pub const CLEAR: Sky = Sky { rain: 0.0, snow: false, storm: false };

/// The weather at block (x, z) when the world's clock reads `seconds`.
pub fn sky_at(seed: u32, x: i32, z: i32, seconds: f32) -> Sky {
    // Weather systems are a few hundred blocks across and drift east, about
    // a block a second, changing shape slowly.
    let drift = (seconds * 1.2) as i32;
    let phase = (seconds / 240.0) as i32;
    let blend = seconds / 240.0 - phase as f32;
    let field = |p: i32| noise2(seed ^ 0xEA7 ^ (p as u32).wrapping_mul(0x9E37), x - drift, z, 420);
    let v = field(phase) + (field(phase + 1) - field(phase)) * blend;
    // About a third of the map is under cloud at any moment.
    let rain = ((v - 0.56) * 6.0).clamp(0.0, 1.0);
    if rain <= 0.0 {
        return CLEAR;
    }
    let col = worldgen::column(seed, x, z);
    let snow = matches!(col.biome, Biome::Tundra | Biome::Taiga) || (col.biome == Biome::Mountain && col.height > SEA + 40);
    // Deserts stay dry however dark the sky gets.
    if col.biome == Biome::Desert || col.biome == Biome::Badlands {
        return Sky { rain: rain * 0.15, snow: false, storm: false };
    }
    Sky { rain, snow, storm: rain > 0.75 && !snow }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Disaster {
    /// The ground shakes; `left` seconds to go.
    Earthquake { left: f32 },
    /// A funnel at (x, z) travelling along (dx, dz).
    Tornado { x: f32, z: f32, dx: f32, dz: f32, left: f32 },
    /// The sea comes up `rise` blocks above its level, then drains.
    Tsunami { left: f32, rise: f32 },
}

/// Whether a tornado could form here: a storm overhead, over open, low country.
pub fn tornado_possible(seed: u32, x: i32, z: i32, sky: Sky) -> bool {
    let col = worldgen::column(seed, x, z);
    sky.storm && matches!(col.biome, Biome::Plains | Biome::Savanna) && col.height < SEA + 24
}

/// Whether the sea is close enough for a tsunami to reach (x, z).
pub fn near_sea(seed: u32, x: i32, z: i32) -> bool {
    let here = worldgen::column(seed, x, z);
    if here.height > SEA + 6 || here.height < SEA {
        return false;
    }
    [(48, 0), (-48, 0), (0, 48), (0, -48), (34, 34), (-34, 34), (34, -34), (-34, -34)]
        .iter()
        .any(|&(dx, dz)| worldgen::column(seed, x + dx, z + dz).height < SEA - 2)
}

/// Roll for a new disaster. Called once a second while disasters are
/// enabled and none is running; `roll` is a random number.
pub fn maybe_start(seed: u32, x: i32, z: i32, sky: Sky, roll: u32) -> Option<Disaster> {
    // On average a quake every twenty minutes or so; a tornado within a
    // couple of minutes of suitable weather.
    if roll % 1200 == 0 {
        return Some(Disaster::Earthquake { left: 7.0 });
    }
    if roll % 120 == 1 && tornado_possible(seed, x, z, sky) {
        let a = (hash2(roll, x, z) % 628) as f32 / 100.0;
        let (dx, dz) = (crate::math::sinf(a), crate::math::cosf(a));
        // It starts thirty blocks off and passes close by.
        return Some(Disaster::Tornado { x: x as f32 - dx * 30.0, z: z as f32 - dz * 30.0, dx: dx * 3.5, dz: dz * 3.5, left: 26.0 });
    }
    None
}

/// What follows when a disaster ends: an earthquake by the sea brings a wave.
pub fn aftermath(seed: u32, x: i32, z: i32, ended: Disaster) -> Option<Disaster> {
    match ended {
        Disaster::Earthquake { .. } if near_sea(seed, x, z) => Some(Disaster::Tsunami { left: 24.0, rise: 0.0 }),
        _ => None,
    }
}

/// How high a tsunami stands `left` seconds before its end: it climbs for
/// six seconds, holds, then drains over the last eight.
pub fn tsunami_height(left: f32) -> f32 {
    let t = 24.0 - left;
    if t < 6.0 {
        t / 6.0 * 3.0
    } else if left > 8.0 {
        3.0
    } else {
        (left / 8.0 * 3.0).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_is_regional_changes_over_time_and_is_repeatable() {
        let mut wet = 0;
        let mut total = 0;
        for i in 0..40 {
            for j in 0..40 {
                let s = sky_at(7, i * 97, j * 89, 1000.0);
                assert!((0.0..=1.0).contains(&s.rain));
                assert!(!(s.storm && s.snow));
                total += 1;
                wet += (s.rain > 0.0) as i32;
            }
        }
        // Somewhere it rains and somewhere it does not.
        assert!(wet > total / 20 && wet < total * 3 / 4, "{wet} of {total} places are wet");
        assert_eq!(sky_at(7, 500, 500, 1000.0), sky_at(7, 500, 500, 1000.0));
        // Neighbouring blocks share their weather; it is a smooth field.
        let (a, b) = (sky_at(7, 3000, 3000, 50.0).rain, sky_at(7, 3001, 3000, 50.0).rain);
        assert!((a - b).abs() < 0.05);
        // Over an hour, the weather at one spot changes.
        let mut seen = [false, false];
        for t in 0..60 {
            seen[(sky_at(7, 640, 640, t as f32 * 60.0).rain > 0.0) as usize] = true;
        }
        assert!(seen[0] && seen[1], "the weather never changed at one spot in an hour");
    }

    #[test]
    fn disasters_need_their_conditions() {
        // A tornado is never offered under a clear sky.
        for roll in 0..5000u32 {
            if let Some(Disaster::Tornado { .. }) = maybe_start(7, 100, 100, CLEAR, roll) {
                panic!("a tornado out of a clear sky");
            }
        }
        // Earthquakes are rare but do happen.
        let quakes = (0..12_000u32).filter(|&r| matches!(maybe_start(7, 0, 0, CLEAR, r), Some(Disaster::Earthquake { .. }))).count();
        assert_eq!(quakes, 10);
        // A wave only follows a quake, and only by the sea.
        let quake = Disaster::Earthquake { left: 0.0 };
        let mut coast = None;
        let mut inland = None;
        for i in 0..4000 {
            let (x, z) = (i * 13, i * 7);
            if near_sea(7, x, z) && coast.is_none() {
                coast = Some((x, z));
            }
            if worldgen::column(7, x, z).height > SEA + 20 && inland.is_none() {
                inland = Some((x, z));
            }
        }
        let (c, l) = (coast.expect("no coast found"), inland.expect("no high ground found"));
        assert!(matches!(aftermath(7, c.0, c.1, quake), Some(Disaster::Tsunami { .. })));
        assert_eq!(aftermath(7, l.0, l.1, quake), None);
        assert_eq!(aftermath(7, c.0, c.1, Disaster::Tsunami { left: 0.0, rise: 0.0 }), None);
        // The wave rises, holds and drains.
        assert_eq!(tsunami_height(24.0), 0.0);
        assert_eq!(tsunami_height(15.0), 3.0);
        assert!(tsunami_height(4.0) < 2.0 && tsunami_height(0.0) == 0.0);
    }
}
