//! HMAC-SHA256 (RFC 2104) and a comparison that takes the same time whatever the bytes are,
//! for the account logins (see `net/accounts`).

use crate::util::hash::sha256::digest;
use alloc::vec::Vec;

/// HMAC-SHA256 of `msg` under `key`.
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner: Vec<u8> = Vec::with_capacity(64 + msg.len());
    inner.extend(k.iter().map(|b| b ^ 0x36));
    inner.extend_from_slice(msg);
    let inner_digest = digest(&inner);
    let mut outer: Vec<u8> = Vec::with_capacity(96);
    outer.extend(k.iter().map(|b| b ^ 0x5c));
    outer.extend_from_slice(&inner_digest);
    digest(&outer)
}

/// Whether two byte strings are equal, looking at every byte whatever the
/// first difference is (so the time taken says nothing about how much matched).
/// Strings of different lengths are never equal (the length is not a secret here).
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    // Keep the optimiser from turning the loop into an early exit.
    core::hint::black_box(diff) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(d: &[u8]) -> alloc::string::String {
        let mut s = alloc::string::String::new();
        for b in d {
            s.push(char::from_digit((*b >> 4) as u32, 16).unwrap());
            s.push(char::from_digit((*b & 15) as u32, 16).unwrap());
        }
        s
    }

    #[test]
    fn rfc_4231_known_answers() {
        assert_eq!(hex(&hmac_sha256(&[0x0b; 20], b"Hi There")), "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7");
        assert_eq!(hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
        // A key longer than a block is digested first.
        assert_eq!(
            hex(&hmac_sha256(&[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First")),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn comparison_finds_every_difference() {
        assert!(ct_eq(b"", b""));
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"xbc"));
        assert!(!ct_eq(b"abc", b"ab"));
        let a = [7u8; 32];
        for i in 0..32 {
            let mut b = a;
            b[i] ^= 0x80;
            assert!(!ct_eq(&a, &b));
        }
    }
}
