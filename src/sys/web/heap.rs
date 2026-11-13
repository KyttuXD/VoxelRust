//! The browser build's memory allocator.
//!
//! A browser has no C library to take `malloc` from, so this is one: a
//! boundary-tag allocator over one growing stretch of memory. Free blocks
//! are kept in lists by size, neighbours that are both free are joined at
//! once, and a block is split when it is larger than what was asked for.
//! It is not thread-safe, and need not be: the browser build has one thread.
//!
//! Everything is an offset from the start of the heap, held in 32 bits, so
//! the same code runs (and is tested) on a 64-bit machine.
//!
//! A block is an 8-byte header followed by the caller's bytes:
//!
//! ```text
//!   +0  size of this block, header included (a multiple of 8); bit 0 = in use
//!   +4  size of the block before it
//!   +8  the caller's bytes; in a free block, the next and previous free
//!       blocks of its list
//! ```

const HEADER: u32 = 8;
/// Smallest block: a header and room for the two list links.
const MIN_BLOCK: u32 = 16;
const NIL: u32 = u32::MAX;
const BINS: usize = 128;
/// One WebAssembly page; memory is asked for in multiples of this.
const PAGE: u32 = 65536;
/// Ask for at least this much at a time.
const GROW: u32 = 16 * PAGE;
/// Requests at or above this are refused.
const TOO_BIG: usize = 0x7FF0_0000;

pub struct Heap {
    base: *mut u8,
    /// Bytes the heap covers so far, from `base`.
    end: u32,
    /// Size of the last block (what a block added at `end` has before it).
    end_prev: u32,
    /// Head of each list of free blocks.
    bins: [u32; BINS],
    /// One bit per list: set while the list is not empty.
    map: [u32; BINS / 32],
    /// Get `n` more bytes (a multiple of `PAGE`) directly after everything
    /// handed out before; null when there is no more.
    more: unsafe fn(usize) -> *mut u8,
}

/// Which list a free block of `size` bytes belongs in: one list per size up
/// to 256 bytes, then four lists per power of two.
fn bin_of(size: u32) -> usize {
    if size < 256 {
        (size >> 3) as usize
    } else {
        let l = 31 - size.leading_zeros();
        (32 + (l - 8) * 4 + ((size >> (l - 2)) & 3)) as usize
    }
}

/// The block size needed to hold `n` bytes.
fn block_for(n: usize) -> u32 {
    (((n as u32) + HEADER + 7) & !7).max(MIN_BLOCK)
}

impl Heap {
    pub const fn new(more: unsafe fn(usize) -> *mut u8) -> Heap {
        Heap { base: core::ptr::null_mut(), end: 0, end_prev: 0, bins: [NIL; BINS], map: [0; BINS / 32], more }
    }

    unsafe fn rd(&self, o: u32) -> u32 {
        (self.base.add(o as usize) as *const u32).read()
    }

    unsafe fn wr(&mut self, o: u32, v: u32) {
        (self.base.add(o as usize) as *mut u32).write(v)
    }

    unsafe fn size(&self, o: u32) -> u32 {
        self.rd(o) & !7
    }

    unsafe fn used(&self, o: u32) -> bool {
        self.rd(o) & 1 != 0
    }

    /// Write a block's header, and tell the block after it how big this is.
    unsafe fn set(&mut self, o: u32, size: u32, used: bool) {
        self.wr(o, size | used as u32);
        if o + size < self.end {
            self.wr(o + size + 4, size);
        } else {
            self.end_prev = size;
        }
    }

    unsafe fn insert(&mut self, o: u32) {
        let b = bin_of(self.size(o));
        let head = self.bins[b];
        self.wr(o + 8, head);
        self.wr(o + 12, NIL);
        if head != NIL {
            self.wr(head + 12, o);
        }
        self.bins[b] = o;
        self.map[b / 32] |= 1 << (b % 32);
    }

    unsafe fn unlink(&mut self, o: u32) {
        let b = bin_of(self.size(o));
        let (next, prev) = (self.rd(o + 8), self.rd(o + 12));
        if prev == NIL {
            self.bins[b] = next;
        } else {
            self.wr(prev + 8, next);
        }
        if next != NIL {
            self.wr(next + 12, prev);
        }
        if self.bins[b] == NIL {
            self.map[b / 32] &= !(1 << (b % 32));
        }
    }

    /// A free block of at least `need` bytes, if there is one.
    unsafe fn find(&self, need: u32) -> Option<u32> {
        let b = bin_of(need);
        // Its own list: blocks there may be a little smaller than `need`.
        // Look at a few and move on rather than walk a long list.
        let mut o = self.bins[b];
        let mut tries = 0;
        while o != NIL && tries < 32 {
            if self.size(o) >= need {
                return Some(o);
            }
            o = self.rd(o + 8);
            tries += 1;
        }
        // Any block in a list of larger sizes is big enough.
        let mut i = b + 1;
        while i < BINS {
            let word = self.map[i / 32] & (u32::MAX << (i % 32));
            if word != 0 {
                return Some(self.bins[(i / 32) * 32 + word.trailing_zeros() as usize]);
            }
            i = (i / 32 + 1) * 32;
        }
        None
    }

    /// Make the heap at least `bytes` longer; the new space becomes a free
    /// block (joined to the last block, if that was free).
    unsafe fn grow(&mut self, bytes: u32) -> bool {
        let round = |n: u32| n.checked_add(PAGE - 1).map(|v| v & !(PAGE - 1));
        let (want, least) = match (round(bytes.max(GROW)), round(bytes)) {
            (Some(w), Some(l)) => (w, l),
            _ => return false,
        };
        let mut got = want;
        let mut p = (self.more)(want as usize);
        if p.is_null() && least < want {
            got = least;
            p = (self.more)(least as usize);
        }
        if p.is_null() {
            return false;
        }
        if self.base.is_null() {
            self.base = p;
        } else if p != self.base.add(self.end as usize) || self.end.checked_add(got).is_none() {
            // Not where the heap ends: unusable (and cannot happen while
            // this is the only code that grows the memory).
            return false;
        }
        let o = self.end;
        let before = self.end_prev;
        self.end += got;
        self.wr(o, got | 1);
        self.wr(o + 4, before);
        self.end_prev = got;
        self.release(o);
        true
    }

    /// Mark a block free, join it to free neighbours and list it.
    unsafe fn release(&mut self, mut o: u32) {
        let mut size = self.size(o);
        let next = o + size;
        if next < self.end && !self.used(next) {
            self.unlink(next);
            size += self.size(next);
        }
        if o > 0 {
            let before = self.rd(o + 4);
            let prev = o - before;
            if !self.used(prev) {
                self.unlink(prev);
                o = prev;
                size += before;
            }
        }
        self.set(o, size, false);
        self.insert(o);
    }

    /// Cut an in-use block down to `need` bytes; the rest goes back.
    unsafe fn trim(&mut self, o: u32, need: u32) {
        let size = self.size(o);
        if size - need >= MIN_BLOCK {
            self.set(o, need, true);
            self.set(o + need, size - need, true);
            self.release(o + need);
        }
    }

    /// `n` bytes aligned to 8, or null. The bytes are not cleared.
    ///
    /// # Safety
    /// Not thread-safe: one caller at a time.
    pub unsafe fn alloc(&mut self, n: usize) -> *mut u8 {
        if n >= TOO_BIG {
            return core::ptr::null_mut();
        }
        let need = block_for(n);
        let o = match self.find(need) {
            Some(o) => o,
            None => {
                if !self.grow(need) {
                    return core::ptr::null_mut();
                }
                match self.find(need) {
                    Some(o) => o,
                    None => return core::ptr::null_mut(),
                }
            }
        };
        self.unlink(o);
        let size = self.size(o);
        self.set(o, size, true);
        self.trim(o, need);
        self.base.add((o + HEADER) as usize)
    }

    /// # Safety
    /// `p` must be null or come from this heap and not be freed already.
    pub unsafe fn free(&mut self, p: *mut u8) {
        if !p.is_null() {
            self.release((p as usize - self.base as usize) as u32 - HEADER);
        }
    }

    /// Resize a block, in place when the space after it allows.
    ///
    /// # Safety
    /// As for [`Heap::free`]. On failure (null) `p` is left as it was.
    pub unsafe fn realloc(&mut self, p: *mut u8, n: usize) -> *mut u8 {
        if p.is_null() {
            return self.alloc(n);
        }
        if n >= TOO_BIG {
            return core::ptr::null_mut();
        }
        let need = block_for(n);
        let o = (p as usize - self.base as usize) as u32 - HEADER;
        loop {
            let size = self.size(o);
            if size >= need {
                self.trim(o, need);
                return p;
            }
            let next = o + size;
            if next == self.end {
                // The last block: lengthen the heap, then take the new space.
                if !self.grow(need - size) {
                    break;
                }
            } else if !self.used(next) {
                let extra = self.size(next);
                if size + extra >= need {
                    self.unlink(next);
                    self.set(o, size + extra, true);
                } else if !(next + extra == self.end && self.grow(need - size - extra)) {
                    break;
                }
            } else {
                break;
            }
        }
        let q = self.alloc(n);
        if !q.is_null() {
            let old = (self.size(o) - HEADER) as usize;
            core::ptr::copy_nonoverlapping(p, q, old.min(n));
            self.free(p);
        }
        q
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Each test has its own arena, handed out front to back like
    // WebAssembly's memory.grow.
    const ARENA: usize = 96 << 20;

    macro_rules! arena {
        () => {{
            static START: AtomicUsize = AtomicUsize::new(0);
            static USED: AtomicUsize = AtomicUsize::new(0);
            unsafe fn more(n: usize) -> *mut u8 {
                if START.load(Ordering::Relaxed) == 0 {
                    let v = std::vec![0u64; ARENA / 8].leak();
                    START.store(v.as_mut_ptr() as usize, Ordering::Relaxed);
                }
                let used = USED.load(Ordering::Relaxed);
                if used + n > ARENA {
                    return core::ptr::null_mut();
                }
                USED.store(used + n, Ordering::Relaxed);
                (START.load(Ordering::Relaxed) + used) as *mut u8
            }
            Heap::new(more)
        }};
    }

    impl Heap {
        /// Walk every block and every list; panic on anything inconsistent.
        /// Returns (blocks in use, free blocks).
        unsafe fn check(&self) -> (usize, usize) {
            let (mut o, mut before, mut used, mut free, mut last_free) = (0u32, 0u32, 0, 0, false);
            while o < self.end {
                let size = self.size(o);
                assert!(size >= MIN_BLOCK && size % 8 == 0 && o + size <= self.end, "bad size {} at {}", size, o);
                if o > 0 {
                    assert_eq!(self.rd(o + 4), before, "wrong previous size at {}", o);
                }
                if self.used(o) {
                    used += 1;
                    last_free = false;
                } else {
                    assert!(!last_free, "two free blocks side by side at {}", o);
                    free += 1;
                    last_free = true;
                }
                before = size;
                o += size;
            }
            assert_eq!(o, self.end);
            assert_eq!(self.end_prev, before);
            let mut listed = 0;
            for b in 0..BINS {
                let (mut o, mut prev) = (self.bins[b], NIL);
                assert_eq!(self.map[b / 32] >> (b % 32) & 1 == 1, o != NIL, "map bit of list {}", b);
                while o != NIL {
                    assert!(!self.used(o) && bin_of(self.size(o)) == b && self.rd(o + 12) == prev);
                    listed += 1;
                    prev = o;
                    o = self.rd(o + 8);
                }
            }
            assert_eq!(listed, free);
            (used, free)
        }
    }

    #[test]
    fn web_heap_survives_random_use() {
        let mut h = arena!();
        let mut rng = 0x1234_5678u32;
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            rng
        };
        // (pointer, length, fill byte)
        let mut live: Vec<(*mut u8, usize, u8)> = Vec::new();
        let filled = |p: *mut u8, n: usize, v: u8| unsafe { core::slice::from_raw_parts(p, n).iter().all(|&b| b == v) };
        unsafe {
            for step in 0..60_000u32 {
                let r = next();
                let size = match r >> 28 {
                    0 => (next() % 300_000) as usize,
                    1..=3 => (next() % 5000) as usize,
                    _ => (next() % 200) as usize,
                };
                match r % 8 {
                    0..=2 => {
                        let p = h.alloc(size);
                        assert!(!p.is_null() && p as usize % 8 == 0);
                        let v = step as u8;
                        core::ptr::write_bytes(p, v, size);
                        live.push((p, size, v));
                    }
                    3..=5 if !live.is_empty() => {
                        let (p, n, v) = live.swap_remove(next() as usize % live.len());
                        assert!(filled(p, n, v), "block damaged before free");
                        h.free(p);
                    }
                    6 | 7 if !live.is_empty() => {
                        let i = next() as usize % live.len();
                        let (p, n, v) = live[i];
                        let q = h.realloc(p, size);
                        assert!(!q.is_null() && q as usize % 8 == 0);
                        assert!(filled(q, n.min(size), v), "realloc lost data");
                        core::ptr::write_bytes(q, v, size);
                        live[i] = (q, size, v);
                    }
                    _ => {}
                }
                if step % 2048 == 0 {
                    assert_eq!(h.check().0, live.len());
                }
            }
            for (p, n, v) in live.drain(..) {
                assert!(filled(p, n, v));
                h.free(p);
            }
            // Everything given back joins up into one free block.
            assert_eq!(h.check(), (0, 1));
        }
    }

    #[test]
    fn web_heap_grows_in_place_and_reuses() {
        let mut h = arena!();
        unsafe {
            // A lone block at the end of the heap grows where it is, the
            // way a `Vec` being pushed to does.
            let mut p = h.alloc(100);
            let first = p;
            for i in 1..200usize {
                p = h.realloc(p, i * 40_000);
                assert_eq!(p, first);
            }
            // Shrinking gives the tail back, and it is used again.
            p = h.realloc(p, 64);
            assert_eq!(p, first);
            let q = h.alloc(1_000_000);
            assert_eq!(q as usize, first as usize + 72);
            h.free(p);
            h.free(q);
            assert_eq!(h.check(), (0, 1));
            // Nothing absurd is accepted.
            assert!(h.alloc(usize::MAX - 64).is_null());
            assert!(h.alloc(TOO_BIG).is_null());
            assert!(h.alloc(0) != h.alloc(0));
        }
    }
}
