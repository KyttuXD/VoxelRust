//! What a mod may read about the player and the world for its HUD: a
//! read-only snapshot. Scripts get it through the host functions
//! (`player_health`, `hunger`, `air`, `player_pos`, `held_item`, `time`,
//! `biome`, `fps`, `heading`, `selected_slot`); native code through
//! [`read`]. Nothing here changes the game.

use crate::session::Session;
use alloc::format;
use alloc::string::String;

/// The player's state, for a HUD.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerInfo {
    /// 0 to 20.
    pub health: i32,
    /// 0 to 20 (survival).
    pub hunger: i32,
    /// Breath left, 0 to 10 (10 is full).
    pub air: f32,
    pub pos: [f64; 3],
    /// Compass heading in degrees, 0 to 360: 0 is north (toward -z), 90 east (+x).
    pub heading: f64,
    /// The id of the item in hand (empty if none).
    pub held: String,
    /// The hotbar slot chosen, 0 to 8.
    pub selected: usize,
    /// Time of day, 0 to 1 (0 is sunrise, 0.25 noon).
    pub time: f64,
    /// The biome under the player, lower case (`plains`, `forest`...).
    pub biome: String,
    /// Frames a second, as last counted.
    pub fps: f32,
}

/// The compass heading, in degrees from 0 up to (not including) 360, of a
/// player facing `yaw` (the game's yaw turns left as it grows, and 0 faces -z).
pub fn heading_degrees(yaw: f32) -> f64 {
    if !yaw.is_finite() {
        return 0.0;
    }
    let mut d = -(yaw as f64) * 180.0 / core::f64::consts::PI;
    // Yaw is kept within a turn; two loops at most, but never forever.
    let mut guard = 0;
    while d < 0.0 && guard < 100 {
        d += 360.0;
        guard += 1;
    }
    while d >= 360.0 && guard < 200 {
        d -= 360.0;
        guard += 1;
    }
    if d >= 360.0 || d < 0.0 {
        0.0
    } else {
        d
    }
}

/// The biome under the player, as a lower case word.
pub fn biome_name(g: &Session) -> String {
    let (x, z) = (crate::math::floord(g.player.pos[0]), crate::math::floord(g.player.pos[2]));
    format!("{:?}", crate::worldgen::column(g.world.seed, x, z).biome).to_ascii_lowercase()
}

/// Read the player's state; `fps` is what the window loop counted.
pub fn read(g: &Session, fps: f32) -> PlayerInfo {
    PlayerInfo {
        health: g.health,
        hunger: g.hunger,
        air: g.air,
        pos: g.player.pos,
        heading: heading_degrees(g.player.yaw),
        held: g.held().map(|h| h.1.id.clone()).unwrap_or_default(),
        selected: g.selected,
        time: crate::script::value::clean_f32(g.time),
        biome: biome_name(g),
        fps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_compass_runs_clockwise_from_north() {
        let pi = core::f32::consts::PI;
        assert_eq!(heading_degrees(0.0), 0.0);
        // Turning left (yaw up) from north goes to west (270).
        assert!((heading_degrees(pi * 0.5) - 270.0).abs() < 0.001);
        assert!((heading_degrees(-pi * 0.5) - 90.0).abs() < 0.001);
        assert!((heading_degrees(pi) - 180.0).abs() < 0.001);
        assert!((heading_degrees(-pi) - 180.0).abs() < 0.001);
    }

    #[test]
    fn the_compass_survives_bad_angles() {
        for y in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0e9, -1.0e9, 100.0, -100.0] {
            let d = heading_degrees(y);
            assert!((0.0..360.0).contains(&d), "{} -> {}", y, d);
        }
    }
}
