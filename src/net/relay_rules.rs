//! Which state channels a player may send for the server to pass on. Some channels carry
//! the server's own word (where a player was, how they were, who was hit): if a player could
//! send those, the server would pass them on with that player's number and (before the
//! receiver has heard the server's own message) another game could be moved or hurt by it.

use crate::proto::channel as ch;

/// Whether a `State` message on `channel` from a player is relayed at all.
pub fn player_may_send(channel: u8) -> bool {
    !matches!(channel, ch::PLAYER | ch::PVP_HURT | ch::PVP_SEEN | ch::VITALS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_servers_own_channels_are_not_relayed_from_players() {
        assert!(!player_may_send(ch::PLAYER));
        assert!(!player_may_send(ch::PVP_HURT));
        assert!(!player_may_send(ch::PVP_SEEN));
        assert!(!player_may_send(ch::VITALS));
        for c in [ch::MOBS, ch::HIT, ch::CHEST, ch::SCRIPT, ch::MODMSG, ch::NATIVE, 200] {
            assert!(player_may_send(c), "{c}");
        }
    }
}
