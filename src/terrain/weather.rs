//! Weather and (optional) natural disasters.
//!
//! Weather is regional: it is a function of where you are and of the
//! world's clock, so walking far enough takes you out of a storm, and
//! everyone on a server sees the same sky. Cold places get snow instead of
//! rain. Disasters are off unless the player turns them on, and each needs
//! its own conditions: tornadoes come only out of storms over open country,
//! tsunamis only follow an earthquake near the sea.

use crate::noise::noise2;
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

/// A disaster in progress. What kinds there are, when they come and what
/// they do is all in the packs (`disaster` lines; see docs/MODDING.md):
/// this is only the one running now.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Running {
    /// Which of the registry's disasters.
    pub kind: usize,
    /// Seconds to go.
    pub left: f32,
    /// Where it is (one that does not move stays on the player's start).
    pub x: f32,
    pub z: f32,
    pub dx: f32,
    pub dz: f32,
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

/// How high a wave of `peak` blocks stands when the disaster that raises
/// it is `progress` (0..1) of the way through: it climbs for the first
/// quarter, holds, then drains over the last third.
pub fn wave_height(progress: f32, peak: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    if p < 0.25 {
        p / 0.25 * peak
    } else if p < 2.0 / 3.0 {
        peak
    } else {
        (1.0 - p) * 3.0 * peak
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
    fn the_sea_is_near_coasts_only_and_waves_rise_hold_and_drain() {
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
        let l = inland.expect("no high ground found");
        assert!(coast.is_some(), "no coast found");
        assert!(!near_sea(7, l.0, l.1));
        assert_eq!(wave_height(0.0, 3.0), 0.0);
        assert_eq!(wave_height(0.4, 3.0), 3.0);
        assert!(wave_height(0.85, 3.0) < 2.0 && wave_height(1.0, 3.0) == 0.0);
    }
}
