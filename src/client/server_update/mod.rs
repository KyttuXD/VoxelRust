//! How the public (dedicated) server keeps itself up to date, quickly and
//! without ever looping: see `docs/DEPLOY.md`.
//!
//! - `schedule`: the pure rules (the check interval, jitter, back-off, which
//!   release is worth fetching, when a start counts as a failed update).
//! - `watch`: the polling thread. Every `update_check_seconds` it reads the
//!   signed release list (a few hundred bytes) and only when the release
//!   number is newer does the whole download-verify-install path run
//!   (`client::update`).
//! - `restart`: the warning and the countdown before the server restarts
//!   into the new program, and how it leaves (self-restart, or an exit code
//!   for `scripts/server-supervisor.sh`).

pub mod restart;
pub mod schedule;
pub mod watch;
