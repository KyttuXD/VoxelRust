//! Signatures, for checking that an update really comes from the project.
//!
//! - `ed25519`: `verify(public_key, message, signature)` (RFC 8032)
//! - `sha512`: the hash Ed25519 uses
//! - `hmac`: HMAC-SHA256 and a constant-time comparison, for the account logins
//! - `field`, `point`: the arithmetic under it (mod 2^255 - 19, and the curve)

pub mod ed25519;
pub mod field;
pub mod hmac;
pub mod point;
pub mod sha512;
