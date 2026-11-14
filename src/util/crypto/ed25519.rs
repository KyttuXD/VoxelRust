//! Ed25519 signature checking (RFC 8032, section 5.1.7), the plain variant
//! with no context and no pre-hashing. Only verification: signing is done by
//! `openssl` in the release scripts.

use super::point::Curve;
use super::sha512;
use alloc::vec::Vec;

/// The order of the base point, L = 2^252 + 27742317777372353535851937790883648493,
/// little-endian. A signature's S must be below it.
const L: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
];

/// Whether little-endian `s` is below L.
fn below_l(s: &[u8]) -> bool {
    for i in (0..32).rev() {
        if s[i] != L[i] {
            return s[i] < L[i];
        }
    }
    false
}

/// Whether `signature` is a valid signature of `message` by the owner of
/// `public_key`. Anything malformed (a key or point that is not on the
/// curve, an S that is not reduced) is simply false.
pub fn verify(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
    let curve = Curve::new();
    let mut r_bytes = [0u8; 32];
    r_bytes.copy_from_slice(&signature[..32]);
    let s = &signature[32..];
    if !below_l(s) {
        return false;
    }
    let (Some(a), Some(r)) = (curve.decompress(public_key), curve.decompress(&r_bytes)) else {
        return false;
    };
    // k = SHA-512(R || A || M), taken whole as a 512-bit number.
    let mut input: Vec<u8> = Vec::with_capacity(64 + message.len());
    input.extend_from_slice(&r_bytes);
    input.extend_from_slice(public_key);
    input.extend_from_slice(message);
    let k = sha512::digest(&input);
    // [S]B == R + [k]A
    let left = curve.scalar_mul(&curve.base(), s);
    let right = curve.add(&r, &curve.scalar_mul(&a, &k));
    left.same(&right)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn unhex(s: &str) -> Vec<u8> {
        let b = s.as_bytes();
        let digit = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
        (0..b.len() / 2).map(|i| digit(b[i * 2]) * 16 + digit(b[i * 2 + 1])).collect()
    }

    fn check(pk: &str, msg: &str, sig: &str) -> bool {
        let (mut p, mut s) = ([0u8; 32], [0u8; 64]);
        p.copy_from_slice(&unhex(pk));
        s.copy_from_slice(&unhex(sig));
        verify(&p, &unhex(msg), &s)
    }

    // RFC 8032, section 7.1.
    const PK1: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    const SIG1: &str = "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b";
    const PK2: &str = "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c";
    const SIG2: &str = "92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00";
    const PK3: &str = "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025";
    const SIG3: &str = "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a";

    #[test]
    fn rfc_8032_vectors_verify() {
        assert!(check(PK1, "", SIG1), "test 1 (empty message)");
        assert!(check(PK2, "72", SIG2), "test 2 (one byte)");
        assert!(check(PK3, "af82", SIG3), "test 3 (two bytes)");
    }

    #[test]
    fn a_changed_message_key_or_signature_is_refused() {
        assert!(!check(PK1, "00", SIG1), "other message");
        assert!(!check(PK2, "73", SIG2), "message with a bit changed");
        assert!(!check(PK2, "72", SIG3), "another message's signature");
        assert!(!check(PK3, "af82", SIG2));
        assert!(!check(PK1, "", SIG2), "another key");
        // Every single bit of the signature matters (spot checks).
        for byte in [0usize, 1, 31, 32, 33, 62] {
            let mut sig = unhex(SIG2);
            sig[byte] ^= 1;
            let mut s = [0u8; 64];
            s.copy_from_slice(&sig);
            let mut p = [0u8; 32];
            p.copy_from_slice(&unhex(PK2));
            assert!(!verify(&p, &[0x72], &s), "byte {byte}");
        }
        // An S that is not reduced (S + L) is refused even though it is
        // the same number modulo L.
        let sig = unhex(SIG2);
        let mut s = [0u8; 64];
        s.copy_from_slice(&sig);
        let mut carry = 0u16;
        for i in 0..32 {
            let sum = s[32 + i] as u16 + L[i] as u16 + carry;
            s[32 + i] = sum as u8;
            carry = sum >> 8;
        }
        let mut p = [0u8; 32];
        p.copy_from_slice(&unhex(PK2));
        assert!(!verify(&p, &[0x72], &s), "unreduced S");
        // Garbage that is not a point at all.
        assert!(!verify(&[2u8; 32], b"x", &[0u8; 64]));
        assert!(!verify(&[0xffu8; 32], b"x", &[0xffu8; 64]));
    }
}
