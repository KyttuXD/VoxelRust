//! Reading bits the way Vorbis packs them: least significant bit first.

pub(super) struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    /// Set once a read went past the end of the packet (the bits read were zeros).
    pub(super) eop: bool,
}

impl<'a> Bits<'a> {
    pub(super) fn new(d: &'a [u8]) -> Bits<'a> {
        Bits { d, pos: 0, eop: false }
    }

    pub(super) fn bit(&mut self) -> u32 {
        let at = self.pos;
        self.pos = self.pos.saturating_add(1);
        match self.d.get(at >> 3) {
            Some(&b) => ((b >> (at & 7)) & 1) as u32,
            None => {
                self.eop = true;
                0
            }
        }
    }

    /// `n` bits (at most 32) as a number.
    pub(super) fn read(&mut self, n: u32) -> u32 {
        let mut v = 0u32;
        for i in 0..n.min(32) {
            v |= self.bit() << i;
        }
        v
    }

    pub(super) fn remaining_bits(&self) -> usize {
        (self.d.len() * 8).saturating_sub(self.pos)
    }
}

/// The number of bits it takes to write `x` (0 for 0).
pub(super) fn ilog(x: u32) -> u32 {
    32 - x.leading_zeros()
}
