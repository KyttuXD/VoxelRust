//! The server's last answers: chunks as compressed pieces, least recently
//! used out first. At most [`MAX_ENTRIES`] chunks and [`MAX_BYTES`] bytes, so
//! the server's memory for land is bounded however many players walk about.

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// A chunk's compressed pieces (shared by everyone waiting for it).
pub type Pieces = Arc<Vec<Vec<u8>>>;

/// Most chunks kept.
pub const MAX_ENTRIES: usize = 256;
/// Most bytes kept in all (a chunk of land is a few kilobytes; noise is the worst case).
pub const MAX_BYTES: usize = 24 << 20;

#[derive(Default)]
pub struct Lru {
    /// Oldest use first.
    items: VecDeque<((i32, i32), Pieces)>,
    bytes: usize,
}

fn size(p: &Pieces) -> usize {
    p.iter().map(|x| x.len()).sum()
}

impl Lru {
    /// The chunk, if it is here; it counts as used now.
    pub fn get(&mut self, key: (i32, i32)) -> Option<Pieces> {
        let at = self.items.iter().position(|e| e.0 == key)?;
        let e = self.items.remove(at)?;
        let p = e.1.clone();
        self.items.push_back(e);
        Some(p)
    }

    /// Keep a chunk (a chunk bigger than the whole budget is not kept).
    pub fn put(&mut self, key: (i32, i32), pieces: Pieces) {
        self.remove(key);
        let n = size(&pieces);
        if n > MAX_BYTES {
            return;
        }
        self.bytes += n;
        self.items.push_back((key, pieces));
        while self.items.len() > MAX_ENTRIES || self.bytes > MAX_BYTES {
            match self.items.pop_front() {
                Some((_, old)) => self.bytes = self.bytes.saturating_sub(size(&old)),
                None => break,
            }
        }
    }

    /// Forget a chunk (its land changed).
    pub fn remove(&mut self, key: (i32, i32)) {
        if let Some(at) = self.items.iter().position(|e| e.0 == key) {
            if let Some((_, old)) = self.items.remove(at) {
                self.bytes = self.bytes.saturating_sub(size(&old));
            }
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(n: usize) -> Pieces {
        Arc::new(alloc::vec![alloc::vec![7u8; n]])
    }

    #[test]
    fn the_least_recently_used_chunk_goes_first_and_there_are_at_most_256() {
        let mut c = Lru::default();
        for i in 0..MAX_ENTRIES as i32 {
            c.put((i, 0), p(10));
        }
        assert_eq!(c.len(), 256);
        // Using (0, 0) saves it; (1, 0) is now the oldest.
        assert!(c.get((0, 0)).is_some());
        c.put((999, 0), p(10));
        assert_eq!(c.len(), 256);
        assert!(c.get((1, 0)).is_none());
        assert!(c.get((0, 0)).is_some() && c.get((999, 0)).is_some());
        assert_eq!(c.bytes(), 256 * 10);
    }

    #[test]
    fn bytes_are_counted_replaced_removed_and_bounded() {
        let mut c = Lru::default();
        c.put((0, 0), p(100));
        c.put((0, 0), p(40));
        assert_eq!((c.len(), c.bytes()), (1, 40));
        c.remove((0, 0));
        assert_eq!((c.len(), c.bytes()), (0, 0));
        c.remove((5, 5));
        // Big ones push the old out; one over the whole budget is not kept.
        c.put((1, 0), p(MAX_BYTES / 2 + 1));
        c.put((2, 0), p(MAX_BYTES / 2 + 1));
        assert!(c.get((1, 0)).is_none() && c.get((2, 0)).is_some());
        c.put((3, 0), p(MAX_BYTES + 1));
        assert!(c.get((3, 0)).is_none());
        assert!(c.bytes() <= MAX_BYTES);
    }
}
