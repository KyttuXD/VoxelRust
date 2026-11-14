//! What should be the same for every player of a shared game, beyond what the
//! older messages carry: other players' health, hunger, worn armour and
//! ping; the death notice; the order players came in (who runs the
//! creatures); the last lines of chat for newcomers; players who come back
//! to a server and where their health was.
//!
//! It all rides on one state channel ([`crate::proto::channel::VITALS`]), so
//! the protocol version did not change: a game that does not know the
//! channel ignores it.
//!
//! | file | subject |
//! | --- | --- |
//! | `vitals.rs` | the bytes of the health message, its checks, the death text |
//! | `history.rs` | the last chat lines, told to newcomers |
//! | `state.rs` | what the server keeps per connection and as a whole for this |
//! | `server_side.rs` | the server's half (a child module of `net/server.rs`) |
//! | `client_side.rs` | the client's half: sending, and keeping what others say |
//! | `tests.rs` | the whole thing over sockets on this machine |

pub mod client_side;
pub mod history;
pub mod state;
pub mod vitals;

#[cfg(test)]
mod tests;
