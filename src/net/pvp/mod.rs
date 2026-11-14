//! Players against players (see docs/MULTIPLAYER.md, "Player versus player").
//!
//! Nothing here changes the protocol: the messages ride [`crate::proto::Msg::State`]
//! on channels `PVP_HIT`, `PVP_HURT`, `PVP_SEEN` and `PVP_GEAR`, which games from
//! before this existed simply ignore. The server decides everything (`check`),
//! the one who is hit applies the damage to their own health (they own it).
//!
//! * `rules`: the server's switches (`pvp`, `spawn_protection_seconds`).
//! * `wire`: the four small payloads.
//! * `check`: whether a hit counts, and what it does (range, cooldown, protection,
//!   permissions, armour, knockback); plain functions, tested without sockets.
//! * `aim`: the attacker's ray test against the other players' boxes.
//! * `state`: what the server keeps per player and for all.
//! * `server_side.rs` (a child module of `server`): the server's handling.

pub mod aim;
pub mod check;
pub mod rules;
pub mod state;
pub mod wire;

#[cfg(test)]
mod tests;
