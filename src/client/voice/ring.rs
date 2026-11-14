//! A lock-free ring of samples between exactly two threads: the audio
//! thread's capture callback writes, the game thread reads. Neither ever
//! waits. If the reader falls behind, the newest samples are dropped (the
//! ring is only a second-ish of slack).

use alloc::vec::Vec;
use core::sync::atomic::{AtomicI16, AtomicUsize, Ordering};

pub struct Ring {
    buf: Vec<AtomicI16>,
    mask: usize,
    /// Total samples ever written / read (they wrap around `usize`; only
    /// the difference matters).
    head: AtomicUsize,
    tail: AtomicUsize,
}

impl Ring {
    /// A ring that holds at least `capacity` samples (rounded up to a power of two).
    pub fn new(capacity: usize) -> Ring {
        let cap = capacity.max(2).next_power_of_two();
        let mut buf = Vec::with_capacity(cap);
        for _ in 0..cap {
            buf.push(AtomicI16::new(0));
        }
        Ring { buf, mask: cap - 1, head: AtomicUsize::new(0), tail: AtomicUsize::new(0) }
    }

    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Samples waiting to be read.
    pub fn len(&self) -> usize {
        self.head.load(Ordering::Acquire).wrapping_sub(self.tail.load(Ordering::Acquire))
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Writer side only: add what fits. Returns how many were taken.
    pub fn push(&self, data: &[i16]) -> usize {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        let free = self.buf.len() - head.wrapping_sub(tail);
        let n = data.len().min(free);
        for (i, &s) in data[..n].iter().enumerate() {
            self.buf[head.wrapping_add(i) & self.mask].store(s, Ordering::Relaxed);
        }
        self.head.store(head.wrapping_add(n), Ordering::Release);
        n
    }

    /// Reader side only: take up to `out.len()` samples, oldest first.
    /// Returns how many were read.
    pub fn pop(&self, out: &mut [i16]) -> usize {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        let n = out.len().min(head.wrapping_sub(tail));
        for (i, o) in out[..n].iter_mut().enumerate() {
            *o = self.buf[tail.wrapping_add(i) & self.mask].load(Ordering::Relaxed);
        }
        self.tail.store(tail.wrapping_add(n), Ordering::Release);
        n
    }

    /// Reader side only: throw everything away.
    pub fn clear(&self) {
        let head = self.head.load(Ordering::Acquire);
        self.tail.store(head, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_come_out_in_order_and_wrap() {
        let r = Ring::new(8);
        assert_eq!(r.capacity(), 8);
        assert!(r.is_empty());
        let mut next = 0i16;
        let mut expect = 0i16;
        let mut out = [0i16; 5];
        for _ in 0..50 {
            let data: [i16; 3] = [next, next + 1, next + 2];
            assert_eq!(r.push(&data), 3);
            next += 3;
            let n = r.pop(&mut out[..3]);
            assert_eq!(n, 3);
            for &s in &out[..3] {
                assert_eq!(s, expect);
                expect += 1;
            }
        }
        assert!(r.is_empty());
    }

    #[test]
    fn a_full_ring_drops_the_newest_and_never_overruns() {
        let r = Ring::new(4);
        assert_eq!(r.push(&[1, 2, 3, 4, 5, 6]), 4);
        assert_eq!(r.len(), 4);
        assert_eq!(r.push(&[9]), 0);
        let mut out = [0i16; 10];
        assert_eq!(r.pop(&mut out), 4);
        assert_eq!(&out[..4], &[1, 2, 3, 4]);
        assert_eq!(r.pop(&mut out), 0);
        // Room again.
        assert_eq!(r.push(&[7, 8]), 2);
        r.clear();
        assert!(r.is_empty());
        assert_eq!(r.pop(&mut out), 0);
    }

    #[test]
    fn it_works_between_two_threads() {
        extern crate std;
        use std::sync::Arc;
        let r = Arc::new(Ring::new(256));
        let w = r.clone();
        let writer = std::thread::spawn(move || {
            let mut i = 0u32;
            while i < 20_000 {
                let chunk: [i16; 7] = core::array::from_fn(|k| ((i + k as u32) % 30_000) as i16);
                let n = w.push(&chunk);
                // Only the samples that were accepted count.
                i += n as u32;
                if n == 0 {
                    std::thread::yield_now();
                }
            }
        });
        let mut got = 0u32;
        let mut buf = [0i16; 64];
        while got < 20_000 {
            let n = r.pop(&mut buf);
            for &s in &buf[..n] {
                assert_eq!(s as u32, got % 30_000, "out of order at {got}");
                got += 1;
            }
            if n == 0 {
                std::thread::yield_now();
            }
        }
        writer.join().unwrap();
    }
}
