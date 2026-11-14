//! The keys of a PIN login. Nothing here is secret code: it is the recipe both sides follow.
//!
//! ```text
//! k(0)   = SHA-256("voxelrust-pin-v1" || salt || pin)
//! k(i)   = SHA-256(k(i-1) || salt || pin)            i = 1 .. rounds-1
//! key    = k(rounds-1)                               (what the server keeps: "the verifier")
//! proof  = HMAC-SHA-256(key, nonce || lower-case name)
//! ```
//!
//! The salt is 16 random bytes the server chose when the account was made; the
//! nonce is 16 fresh random bytes the server sends for every login. The PIN
//! never leaves the player's machine; the server stores the key, never the PIN.

use crate::util::crypto::hmac::{ct_eq, hmac_sha256};
use crate::util::hash::sha256::digest;
use alloc::string::String;
use alloc::vec::Vec;

pub type Salt = [u8; 16];
pub type Nonce = [u8; 16];
pub type Key = [u8; 32];
pub type Proof = [u8; 32];

/// How many times the PIN is hashed (a server may choose another number, within [`rounds_ok`]).
pub const ROUNDS: u32 = 20_000;

/// Whether a number of rounds a server asks for is sane (a hostile server must not be able to
/// make a client spin for minutes, nor ask for one round).
pub fn rounds_ok(rounds: u32) -> bool {
    (1_000..=200_000).contains(&rounds)
}

/// The key made from a PIN and a salt.
pub fn derive_key(pin: &str, salt: &Salt, rounds: u32) -> Key {
    let mut first: Vec<u8> = Vec::with_capacity(16 + 16 + pin.len());
    first.extend_from_slice(b"voxelrust-pin-v1");
    first.extend_from_slice(salt);
    first.extend_from_slice(pin.as_bytes());
    let mut k = digest(&first);
    let mut buf: Vec<u8> = Vec::with_capacity(32 + 16 + pin.len());
    for _ in 1..rounds.max(1) {
        buf.clear();
        buf.extend_from_slice(&k);
        buf.extend_from_slice(salt);
        buf.extend_from_slice(pin.as_bytes());
        k = digest(&buf);
    }
    k
}

/// The answer to a login challenge: the key's HMAC of the nonce and the (lower-case) name.
pub fn login_proof(key: &Key, nonce: &Nonce, name: &str) -> Proof {
    let mut msg: Vec<u8> = Vec::with_capacity(16 + name.len());
    msg.extend_from_slice(nonce);
    msg.extend_from_slice(name.to_ascii_lowercase().as_bytes());
    hmac_sha256(key, &msg)
}

/// Whether `proof` is what the stored `key` makes of this challenge (compared in constant time).
pub fn proof_matches(key: &Key, nonce: &Nonce, name: &str, proof: &Proof) -> bool {
    ct_eq(&login_proof(key, nonce, name), proof)
}

/// Bytes as lower-case hex.
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((*b >> 4) as u32, 16).unwrap_or('0'));
        s.push(char::from_digit((*b & 15) as u32, 16).unwrap_or('0'));
    }
    s
}

/// Hex back into `N` bytes (None if it is not exactly that many hex digits).
pub fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    let b = text.as_bytes();
    if b.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for i in 0..N {
        let hi = (b[i * 2] as char).to_digit(16)?;
        let lo = (b[i * 2 + 1] as char).to_digit(16)?;
        out[i] = (hi * 16 + lo) as u8;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn salt() -> Salt {
        let mut s = [0u8; 16];
        for (i, b) in s.iter_mut().enumerate() {
            *b = i as u8;
        }
        s
    }

    #[test]
    fn the_key_of_a_pin_is_what_the_recipe_says() {
        // Worked out with another program's SHA-256 (python hashlib), following the recipe above.
        assert_eq!(hex(&derive_key("4821", &salt(), 1)), "ccf306038e7bfc63331fa70601f13730b3cacfb725bc6223ebd34bd8afb9b80f");
        assert_eq!(hex(&derive_key("4821", &salt(), 3)), "00f72300b1ba51543496438ed5aefe50b2050159cb8c724df3133cc2d9dde53f");
        assert_eq!(hex(&derive_key("4821", &salt(), ROUNDS)), "a6e8de3c8cab007f1fc0d7499d1aac4fd0419b2ce08707088b18558a635038b0");
        // Zero rounds is one.
        assert_eq!(derive_key("4821", &salt(), 0), derive_key("4821", &salt(), 1));
    }

    #[test]
    fn the_proof_binds_the_pin_the_nonce_and_the_name() {
        let key = derive_key("4821", &salt(), ROUNDS);
        let mut nonce = [0u8; 16];
        for (i, b) in nonce.iter_mut().enumerate() {
            *b = 100 + i as u8;
        }
        let proof = login_proof(&key, &nonce, "kyttu");
        assert_eq!(hex(&proof), "60b2bca431e3e2ab30c48ea082f2cee29a20b862df40e48e46fcd93a2c8715af");
        // Capital letters in the name do not matter, the rest does.
        assert_eq!(login_proof(&key, &nonce, "KyTTu"), proof);
        assert!(proof_matches(&key, &nonce, "Kyttu", &proof));
        assert!(!proof_matches(&key, &nonce, "kyttv", &proof));
        let mut other_nonce = nonce;
        other_nonce[0] ^= 1;
        assert!(!proof_matches(&key, &other_nonce, "kyttu", &proof));
        let wrong = derive_key("4822", &salt(), ROUNDS);
        assert!(!proof_matches(&wrong, &nonce, "kyttu", &proof));
        let mut other_salt = salt();
        other_salt[15] ^= 1;
        assert_ne!(derive_key("4821", &other_salt, 1000), derive_key("4821", &salt(), 1000));
    }

    #[test]
    fn hex_round_trips_and_rejects_junk() {
        let s = salt();
        assert_eq!(unhex::<16>(&hex(&s)), Some(s));
        assert_eq!(unhex::<16>("00"), None);
        assert_eq!(unhex::<2>("zz00"), None);
        assert_eq!(unhex::<2>("ABcd"), Some([0xab, 0xcd]));
    }

    #[test]
    fn a_server_cannot_ask_for_absurd_work() {
        assert!(rounds_ok(ROUNDS) && rounds_ok(1_000) && rounds_ok(200_000));
        assert!(!rounds_ok(1) && !rounds_ok(0) && !rounds_ok(u32::MAX) && !rounds_ok(999));
    }
}
