//! A bit reader, most significant bit first. Every read says `None` at the end of the data.

pub(super) struct BitReader<'a> {
    d: &'a [u8],
    pos: usize, // in bits
}

impl<'a> BitReader<'a> {
    pub fn new(d: &'a [u8]) -> BitReader<'a> {
        BitReader { d, pos: 0 }
    }

    /// Where the next bit is, counted in bits from the start.
    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn set_pos(&mut self, pos: usize) {
        self.pos = pos;
    }

    pub fn bit(&mut self) -> Option<u32> {
        let byte = *self.d.get(self.pos >> 3)?;
        let v = (byte >> (7 - (self.pos & 7))) & 1;
        self.pos += 1;
        Some(v as u32)
    }

    /// `n` bits (at most 32) as a number.
    pub fn bits(&mut self, n: u32) -> Option<u32> {
        if n > 32 {
            return None;
        }
        let mut v = 0u64;
        for _ in 0..n {
            v = (v << 1) | self.bit()? as u64;
        }
        Some(v as u32)
    }
}
