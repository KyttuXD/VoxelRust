//! Skins on their way out of the server, a few pieces to a tick.
//!
//! A big skin is up to 1 MiB, 27 messages. Sent in one go to every other
//! player it would hold up the server's tick (and so everyone's game) for as
//! long as the slowest connection takes to accept it. Each connection has a
//! [`SkinQueue`] instead: the relayed skins wait in it, shared between the
//! connections that get the same bytes, and the tick sends [`PER_TICK`] bytes
//! of them at most (whole messages only, so other messages never land in the
//! middle of one).

use alloc::collections::VecDeque;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// Bytes of skin sent to one connection in one tick (at least one message).
pub const PER_TICK: usize = 100_000;
/// Bytes that may wait for one connection; a slower one is let go.
pub const MAX_QUEUED: usize = 8 << 20;

/// The encoded messages of one skin, shared by everyone who is sent it.
pub type Batch = Arc<Vec<Vec<u8>>>;

#[derive(Default)]
pub struct SkinQueue {
    /// Batches waiting, with the number of messages of each already sent.
    items: VecDeque<(Batch, usize)>,
    /// Bytes not yet sent.
    bytes: usize,
}

fn size_of(batch: &Batch) -> usize {
    batch.iter().map(|m| m.len()).sum()
}

impl SkinQueue {
    /// Put a skin in the queue. False if this connection already has too
    /// much waiting (the skin is not queued).
    pub fn push(&mut self, batch: Batch) -> bool {
        let n = size_of(&batch);
        if self.bytes.saturating_add(n) > MAX_QUEUED {
            return false;
        }
        if n > 0 {
            self.bytes += n;
            self.items.push_back((batch, 0));
        }
        true
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Bytes still waiting.
    pub fn waiting(&self) -> usize {
        self.bytes
    }

    /// Take the messages to send this tick: whole messages, `budget` bytes
    /// of them at most, but always one if any waits.
    pub fn take(&mut self, budget: usize, out: &mut Vec<Vec<u8>>) {
        let mut used = 0usize;
        while let Some((batch, at)) = self.items.front_mut() {
            let Some(m) = batch.get(*at) else {
                self.items.pop_front();
                continue;
            };
            if used > 0 && used + m.len() > budget {
                return;
            }
            used += m.len();
            self.bytes = self.bytes.saturating_sub(m.len());
            out.push(m.clone());
            *at += 1;
            if *at >= batch.len() {
                self.items.pop_front();
            }
        }
    }

    /// Forget everything (the connection is going).
    pub fn clear(&mut self) {
        self.items.clear();
        self.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batch(sizes: &[usize]) -> Batch {
        Arc::new(sizes.iter().enumerate().map(|(i, &n)| alloc::vec![i as u8; n]).collect())
    }

    #[test]
    fn a_big_skin_leaves_a_few_pieces_to_a_tick_in_order() {
        let mut q = SkinQueue::default();
        assert!(q.push(batch(&[40_000; 10])));
        assert_eq!(q.waiting(), 400_000);
        let mut ticks = 0;
        let mut got = Vec::new();
        while !q.is_empty() {
            let mut out = Vec::new();
            q.take(PER_TICK, &mut out);
            assert!(!out.is_empty() && out.len() <= 2, "{} pieces", out.len());
            got.extend(out);
            ticks += 1;
        }
        assert_eq!(ticks, 5);
        assert_eq!(got.len(), 10);
        assert!(got.iter().all(|m| m.len() == 40_000));
        assert_eq!(q.waiting(), 0);
    }

    #[test]
    fn a_message_bigger_than_the_budget_still_goes() {
        let mut q = SkinQueue::default();
        q.push(batch(&[500]));
        let mut out = Vec::new();
        q.take(10, &mut out);
        assert_eq!(out.len(), 1);
        assert!(q.is_empty());
    }

    #[test]
    fn batches_keep_their_order_and_a_slow_connection_is_refused_in_the_end() {
        let mut q = SkinQueue::default();
        assert!(q.push(batch(&[30_000, 30_000])) && q.push(batch(&[10])));
        let mut out = Vec::new();
        q.take(PER_TICK, &mut out);
        assert_eq!(out.iter().map(|m| m.len()).collect::<Vec<_>>(), [30_000, 30_000, 10]);
        let mut q = SkinQueue::default();
        let mut pushed = 0;
        while q.push(batch(&[1 << 20])) {
            pushed += 1;
            assert!(pushed < 100);
        }
        assert_eq!(pushed, 8);
        q.clear();
        assert!(q.is_empty() && q.waiting() == 0);
    }
}
