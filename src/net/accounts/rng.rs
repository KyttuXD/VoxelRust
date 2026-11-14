//! Random bytes for salts and challenges, on the server: the system's random source
//! (`/dev/urandom`, `RtlGenRandom`) stretched with SHA-256, and stirred again from the
//! system now and then. Where a machine has no such source (the browser, which never
//! hosts) the bytes are only as good as the clock, which is said in docs/ACCOUNTS.md.

use crate::util::hash::sha256::digest;
use alloc::vec::Vec;

/// How many blocks are made before the system is asked again.
const RESEED_AFTER: u32 = 4096;

pub struct Rng {
    state: [u8; 32],
    counter: u64,
    made: u32,
    seeded: bool,
}

impl Default for Rng {
    /// Not seeded yet: that happens on first use (so a server that never needs a random
    /// number never touches the system's source).
    fn default() -> Rng {
        Rng { state: [0; 32], counter: 0, made: 0, seeded: false }
    }
}

impl Rng {
    /// A generator that makes the same bytes every time from the same seed (for tests).
    pub fn from_seed(seed: &[u8]) -> Rng {
        Rng { state: digest(seed), counter: 0, made: 0, seeded: true }
    }

    /// Mix fresh bytes from the system (and, as a poor second source, the clock) into the state.
    fn stir(&mut self) {
        let mut fresh = [0u8; 32];
        let from_system = crate::sys::random::fill(&mut fresh);
        let mut input: Vec<u8> = Vec::with_capacity(32 + 32 + 24);
        input.extend_from_slice(&self.state);
        input.extend_from_slice(&fresh);
        input.extend_from_slice(&crate::sys::clock::unix_seconds().to_le_bytes());
        // Where the stack happens to be: different from run to run on systems that randomise it.
        let marker = 0u8;
        input.extend_from_slice(&(&marker as *const u8 as usize as u64).to_le_bytes());
        input.extend_from_slice(&self.counter.to_le_bytes());
        if !from_system {
            // No better source: at least never repeat what an earlier call gave.
            input.extend_from_slice(b"no system source");
        }
        self.state = digest(&input);
        self.seeded = true;
        self.made = 0;
    }

    /// Fill `out` with random bytes.
    pub fn fill(&mut self, out: &mut [u8]) {
        if !self.seeded || self.made >= RESEED_AFTER {
            self.stir();
        }
        for chunk in out.chunks_mut(32) {
            self.counter = self.counter.wrapping_add(1);
            let mut block: Vec<u8> = Vec::with_capacity(32 + 8 + 3);
            block.extend_from_slice(b"out");
            block.extend_from_slice(&self.state);
            block.extend_from_slice(&self.counter.to_le_bytes());
            let o = digest(&block);
            chunk.copy_from_slice(&o[..chunk.len()]);
            // The state moves on, so a block seen now says nothing about the next.
            block[..3].copy_from_slice(b"nxt");
            self.state = digest(&block);
            self.made += 1;
        }
    }

    pub fn bytes16(&mut self) -> [u8; 16] {
        let mut b = [0u8; 16];
        self.fill(&mut b);
        b
    }

    /// A number from 0 to `n - 1` (`n` above 0; the tiny bias of the remainder does not matter here).
    pub fn below(&mut self, n: u32) -> u32 {
        let mut b = [0u8; 4];
        self.fill(&mut b);
        u32::from_le_bytes(b) % n.max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_bytes_and_every_block_is_new() {
        let (mut a, mut b) = (Rng::from_seed(b"x"), Rng::from_seed(b"x"));
        assert_eq!(a.bytes16(), b.bytes16());
        let first = a.bytes16();
        let second = a.bytes16();
        assert_ne!(first, second);
        assert_ne!(Rng::from_seed(b"y").bytes16(), Rng::from_seed(b"x").bytes16());
        let mut long = [0u8; 100];
        a.fill(&mut long);
        assert!(long[..32] != long[32..64] && long[32..64] != long[64..96]);
    }

    #[test]
    fn an_unseeded_generator_seeds_itself_from_the_system() {
        let (mut a, mut b) = (Rng::default(), Rng::default());
        assert_ne!(a.bytes16(), b.bytes16());
        assert!(a.below(10) < 10);
    }
}
