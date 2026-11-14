//! The session's side of land from the server (a child module of
//! `session.rs`): the messages about land, the requests a world sends, what to
//! tell the player, and where to stand when the first chunk comes.
//! See `net/chunks` and docs/MULTIPLAYER.md.

use super::*;

impl Session {
    /// The server said whether it sends land. If it does not, the world makes
    /// its own (ordinary terrain) and the player is told what they lack.
    pub(super) fn land_caps(&mut self, flags: u8, limit: u32) {
        self.world.stream_caps(flags, limit);
        self.land_check_failed();
    }

    /// A piece of a chunk of the server's land arrived.
    pub(super) fn land_part(&mut self, cx: i32, cz: i32, part: u8, parts: u8, data: Vec<u8>) {
        self.world.stream_part(cx, cz, part, parts, data);
        self.land_place_player(cx, cz);
    }

    /// Once a frame: ask the server for the chunks the world is missing, nearest first.
    pub(super) fn land_pump(&mut self, dt: f32) {
        // (Nothing is asked before the server's content is in use: its chunks are in its block numbers.)
        if self.world.remote.is_none() || self.server_pack.is_some() {
            return;
        }
        let wanted = self.world.stream_requests(dt);
        if let Some(n) = self.net.as_mut() {
            for (cx, cz) in wanted {
                n.send_chunk_request(cx, cz);
            }
        }
        self.land_check_failed();
    }

    /// The server turned out not to send land (or never said): ordinary terrain, and a notice.
    fn land_check_failed(&mut self) {
        if self.world.stream_failed() {
            let name = self.world.stop_streaming();
            self.push_chat(alloc::format!("THIS WORLD NEEDS THE {} MOD (THE SERVER CANNOT SEND ITS LAND)", name.to_ascii_uppercase()));
        }
    }

    /// A new player has no place of their own to come back to, so they start where this game's
    /// ordinary terrain would put them: when the chunk under them arrives they are set on
    /// the ground of the real land (not in it, not high above it).
    fn land_place_player(&mut self, cx: i32, cz: i32) {
        if self.spot_applied || self.dead {
            return;
        }
        let (x, z) = (floord(self.player.pos[0]), floord(self.player.pos[2]));
        if crate::chunk::chunk_of(x) != cx || crate::chunk::chunk_of(z) != cz {
            return;
        }
        if let Some(h) = self.world.standing_height(x, z) {
            if let Some(y) = placed_y(self.player.pos[1], h) {
                self.player.pos[1] = y;
                self.player.vel = [0.0; 3];
            }
        }
    }
}

/// Where a player at height `y` should stand on ground whose standing height is `h`:
/// `None` if they are on it (or just above it) already.
pub(super) fn placed_y(y: f64, h: i32) -> Option<f64> {
    if y < f64::from(h) - 0.5 || y > f64::from(h) + 3.0 {
        Some(f64::from(h) + 0.01)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_inside_the_ground_or_high_above_it_is_set_on_it() {
        assert_eq!(placed_y(90.0, 20), Some(20.01));
        assert_eq!(placed_y(5.0, 20), Some(20.01));
        assert_eq!(placed_y(20.0, 20), None);
        assert_eq!(placed_y(21.5, 20), None);
        assert_eq!(placed_y(19.6, 20), None);
        assert_eq!(placed_y(23.5, 20), Some(20.01));
    }
}

