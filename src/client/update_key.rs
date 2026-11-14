//! The public key the updater checks `nightly.txt.sig` against.
//!
//! Whoever holds the matching private key (`~/.voxelrust/update-signing-key.pem`
//! on the release machine, never in the repository) decides what the game
//! installs. To make a new pair, run `scripts/make-signing-key.sh`: it prints
//! the 32 bytes as a Rust array to paste below. Programs built with the old
//! key only accept lists signed with the old key, so changing it means every
//! player installs the new program by hand once.

/// The key in the program, 32 bytes (Ed25519).
const EMBEDDED: [u8; 32] = [
    0xda, 0x5d, 0x1b, 0x09, 0x31, 0x04, 0x81, 0xd5, 0xf1, 0x14, 0xe7, 0x9d, 0x34, 0x90, 0xa2, 0xe1,
    0x75, 0x26, 0x44, 0x9d, 0x1c, 0x74, 0xf4, 0xd7, 0x6e, 0x02, 0x28, 0x22, 0xf6, 0x9c, 0xd0, 0x6f,
];

/// A key given in the environment while building (64 hex digits). Only the
/// `update-test` feature reads it, for `scripts/test-update.sh` (which signs
/// with a throwaway key); a normal build always has the embedded key.
const TEST_KEY: Option<&str> = if cfg!(feature = "update-test") { option_env!("VOXELRUST_UPDATE_KEY") } else { None };

const fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

const fn parse_key(s: &str) -> Option<[u8; 32]> {
    let b = s.as_bytes();
    if b.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        let (Some(hi), Some(lo)) = (hex_digit(b[i * 2]), hex_digit(b[i * 2 + 1])) else {
            return None;
        };
        out[i] = hi * 16 + lo;
        i += 1;
    }
    Some(out)
}

const fn choose() -> [u8; 32] {
    if let Some(text) = TEST_KEY {
        if let Some(key) = parse_key(text) {
            return key;
        }
    }
    EMBEDDED
}

/// The key the program trusts.
pub const UPDATE_KEY: [u8; 32] = choose();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_read_from_hex() {
        let k = parse_key("00ff10Ab000000000000000000000000000000000000000000000000000000cd");
        assert_eq!(k.map(|k| (k[0], k[1], k[2], k[3], k[31])), Some((0, 255, 16, 0xab, 0xcd)));
        assert_eq!(parse_key("00"), None);
        assert_eq!(parse_key("zz00000000000000000000000000000000000000000000000000000000000000"), None);
    }

    #[test]
    fn the_embedded_key_is_a_real_point() {
        let curve = crate::util::crypto::point::Curve::new();
        assert!(curve.decompress(&EMBEDDED).is_some());
    }
}
