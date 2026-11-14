//! The client's side of land from the server, as a state machine with no
//! sockets and no clock of its own (the tests drive it).
//!
//! The world says which chunks it is missing ([`Streamer::want`]); every frame
//! [`Streamer::plan`] says which of them to ask for now: nearest to the
//! player first, at most [`MAX_IN_FLIGHT`] unanswered, at most
//! [`PER_SECOND`] a second, and any that got no answer in
//! [`RETRY_SECONDS`] again. Pieces that arrive are put together
//! ([`Streamer::on_part`]) and the last [`KEEP_CHUNKS`] chunks are kept as
//! compressed bytes, so walking back over ground costs no network.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::vec::Vec;

/// Requests that may wait for an answer at once (the server's own limit).
pub const MAX_IN_FLIGHT: usize = 16;
/// A request with no answer by then is made again.
pub const RETRY_SECONDS: f64 = 3.0;
/// Chunks asked for in a second, at most (the server answers 200).
pub const PER_SECOND: f32 = 120.0;
/// How long to wait for the server to say whether it sends land.
pub const CAPS_WAIT: f64 = 20.0;
/// Chunks kept after they have been shown, and their bytes.
pub const KEEP_CHUNKS: usize = 512;
pub const KEEP_BYTES: usize = 8 << 20;
/// Chunks whose pieces may be arriving at once.
const MAX_ASSEMBLING: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// The server has not said yet whether it sends land: nothing is asked for.
    Waiting,
    /// Land comes from the server.
    Streaming,
    /// The server cannot send land (or never said): the world makes its own.
    Fallback,
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    /// When the request was last sent (None: not yet).
    sent_at: Option<f64>,
}

struct Assembly {
    parts: Vec<Option<Vec<u8>>>,
}

/// What happened, for the log and the tests.
#[derive(Clone, Copy, Default, Debug)]
pub struct Stats {
    pub requested: u32,
    pub retried: u32,
    pub received: u32,
    pub damaged: u32,
}

pub struct Streamer {
    /// The first word of the world's kind (what the player's game lacks), for messages.
    pub name: String,
    pub mode: Mode,
    /// How far from the origin the server answers, in blocks.
    pub limit: u32,
    pub stats: Stats,
    now: f64,
    tokens: f32,
    pending: BTreeMap<(i32, i32), Entry>,
    assembling: BTreeMap<(i32, i32), Assembly>,
    kept: VecDeque<((i32, i32), Vec<u8>)>,
    kept_bytes: usize,
}

fn dist2(a: (i32, i32), b: (i32, i32)) -> i64 {
    let (dx, dz) = (i64::from(a.0) - i64::from(b.0), i64::from(a.1) - i64::from(b.1));
    dx.saturating_mul(dx).saturating_add(dz.saturating_mul(dz))
}

impl Streamer {
    /// For a world of the kind `kind` (its text; the first word is the name shown).
    pub fn new(kind: &str) -> Streamer {
        Streamer {
            name: String::from(kind.split(' ').next().unwrap_or("")),
            mode: Mode::Waiting,
            limit: 0,
            stats: Stats::default(),
            now: 0.0,
            tokens: PER_SECOND,
            pending: BTreeMap::new(),
            assembling: BTreeMap::new(),
            kept: VecDeque::new(),
            kept_bytes: 0,
        }
    }

    /// Whether the world should leave chunks to this (and not generate them itself).
    pub fn takes_chunks(&self) -> bool {
        self.mode != Mode::Fallback
    }

    /// The server said what it does (its [`crate::proto::Msg::ChunkCaps`]).
    pub fn on_caps(&mut self, flags: u8, limit: u32) {
        if self.mode != Mode::Waiting {
            return;
        }
        self.limit = limit;
        self.mode = if flags & crate::proto::CHUNKS_SERVED != 0 { Mode::Streaming } else { Mode::Fallback };
    }

    /// The world is missing chunk `key`. If it was shown lately its compressed
    /// bytes are returned at once; otherwise it is put on the list to ask for.
    pub fn want(&mut self, key: (i32, i32)) -> Option<Vec<u8>> {
        if let Some(at) = self.kept.iter().position(|e| e.0 == key) {
            if let Some(e) = self.kept.remove(at) {
                let bytes = e.1.clone();
                self.kept.push_back(e);
                return Some(bytes);
            }
        }
        self.pending.entry(key).or_insert(Entry { sent_at: None });
        None
    }

    /// The world dropped the chunks farther than `keep2` (squared chunks) from `center`:
    /// they are not asked for any more.
    pub fn forget_far(&mut self, center: (i32, i32), keep2: i64) {
        self.pending.retain(|k, _| dist2(*k, center) <= keep2);
        self.assembling.retain(|k, _| dist2(*k, center) <= keep2);
    }

    /// Requests waiting for an answer, and chunks not yet asked for.
    pub fn open(&self) -> usize {
        self.pending.len()
    }

    /// Let `dt` seconds pass and say which chunks to ask for now, nearest `center` first.
    pub fn plan(&mut self, dt: f32, center: (i32, i32)) -> Vec<(i32, i32)> {
        let dt = if dt > 0.0 && dt <= 1.0 { dt } else { 0.0 };
        self.now += f64::from(dt);
        self.tokens = (self.tokens + dt * PER_SECOND).min(PER_SECOND);
        if self.mode == Mode::Waiting && self.now > CAPS_WAIT {
            self.mode = Mode::Fallback;
        }
        if self.mode != Mode::Streaming {
            return Vec::new();
        }
        let now = self.now;
        let live = self.pending.values().filter(|e| e.sent_at.map_or(false, |t| now - t < RETRY_SECONDS)).count();
        let mut room = MAX_IN_FLIGHT.saturating_sub(live);
        if room == 0 {
            return Vec::new();
        }
        let mut due: Vec<(i64, (i32, i32))> = self.pending.iter().filter(|(_, e)| e.sent_at.map_or(true, |t| now - t >= RETRY_SECONDS)).map(|(k, _)| (dist2(*k, center), *k)).collect();
        due.sort();
        let mut out = Vec::new();
        for (_, key) in due {
            if room == 0 || self.tokens < 1.0 {
                break;
            }
            if let Some(e) = self.pending.get_mut(&key) {
                if e.sent_at.is_some() {
                    self.stats.retried = self.stats.retried.saturating_add(1);
                }
                e.sent_at = Some(now);
            }
            self.tokens -= 1.0;
            room -= 1;
            self.stats.requested = self.stats.requested.saturating_add(1);
            out.push(key);
        }
        out
    }

    /// A piece of chunk (cx, cz) arrived. When it is the last one missing, the whole
    /// compressed chunk is returned (and the chunk is no longer waited for: say how it
    /// went with [`Streamer::remember`] or [`Streamer::damaged`]). Pieces of chunks that
    /// were not asked for are ignored.
    pub fn on_part(&mut self, cx: i32, cz: i32, part: u8, parts: u8, data: Vec<u8>) -> Option<Vec<u8>> {
        let key = (cx, cz);
        if self.mode != Mode::Streaming || !self.pending.get(&key).map_or(false, |e| e.sent_at.is_some()) {
            return None;
        }
        let (part, parts) = (part as usize, parts as usize);
        if parts == 0 || parts > crate::proto::MAX_CHUNK_PARTS || part >= parts || data.len() > crate::proto::MAX_CHUNK_PART {
            return None;
        }
        if !self.assembling.contains_key(&key) && self.assembling.len() >= MAX_ASSEMBLING {
            return None;
        }
        let a = self.assembling.entry(key).or_insert_with(|| Assembly { parts: (0..parts).map(|_| None).collect() });
        if a.parts.len() != parts {
            // The server changed its mind about how many pieces: start again.
            a.parts = (0..parts).map(|_| None).collect();
        }
        a.parts[part] = Some(data);
        if a.parts.iter().any(|p| p.is_none()) {
            return None;
        }
        let whole: Vec<u8> = a.parts.iter().flatten().flat_map(|p| p.iter().copied()).collect();
        self.assembling.remove(&key);
        self.pending.remove(&key);
        self.stats.received = self.stats.received.saturating_add(1);
        Some(whole)
    }

    /// The chunk that [`Streamer::on_part`] completed was good and is shown: keep its bytes.
    pub fn remember(&mut self, key: (i32, i32), whole: Vec<u8>) {
        if let Some(at) = self.kept.iter().position(|e| e.0 == key) {
            if let Some(old) = self.kept.remove(at) {
                self.kept_bytes = self.kept_bytes.saturating_sub(old.1.len());
            }
        }
        if whole.len() > KEEP_BYTES {
            return;
        }
        self.kept_bytes += whole.len();
        self.kept.push_back((key, whole));
        while self.kept.len() > KEEP_CHUNKS || self.kept_bytes > KEEP_BYTES {
            match self.kept.pop_front() {
                Some((_, old)) => self.kept_bytes = self.kept_bytes.saturating_sub(old.len()),
                None => break,
            }
        }
    }

    /// The chunk that [`Streamer::on_part`] completed could not be read: it is asked
    /// for again after [`RETRY_SECONDS`].
    pub fn damaged(&mut self, key: (i32, i32)) {
        self.stats.damaged = self.stats.damaged.saturating_add(1);
        self.pending.insert(key, Entry { sent_at: Some(self.now) });
    }

    /// Bytes of chunks kept.
    pub fn kept_bytes(&self) -> usize {
        self.kept_bytes
    }

    pub fn kept_chunks(&self) -> usize {
        self.kept.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn streaming() -> Streamer {
        let mut s = Streamer::new("somewhere 46.8 -121.7 30 75");
        s.on_caps(crate::proto::CHUNKS_SERVED, 30_000);
        s
    }

    #[test]
    fn nothing_is_asked_until_the_server_says_it_sends_land() {
        let mut s = Streamer::new("somewhere 46.8 -121.7 30 75");
        assert_eq!(s.name, "somewhere");
        s.want((0, 0));
        assert!(s.plan(0.1, (0, 0)).is_empty());
        assert_eq!(s.mode, Mode::Waiting);
        s.on_caps(crate::proto::CHUNKS_SERVED, 30_000);
        assert_eq!(s.mode, Mode::Streaming);
        assert_eq!(s.plan(0.1, (0, 0)), alloc::vec![(0, 0)]);
        // What it said first stands.
        s.on_caps(0, 0);
        assert_eq!(s.mode, Mode::Streaming);
    }

    #[test]
    fn a_server_that_sends_no_land_or_says_nothing_makes_the_world_use_its_own() {
        let mut s = Streamer::new("x");
        s.on_caps(0, 0);
        assert_eq!(s.mode, Mode::Fallback);
        assert!(!s.takes_chunks());
        let mut s = Streamer::new("x");
        s.want((1, 1));
        for _ in 0..50 {
            assert!(s.plan(0.5, (0, 0)).is_empty());
        }
        assert_eq!(s.mode, Mode::Fallback, "no answer in {} seconds", CAPS_WAIT);
    }

    #[test]
    fn the_nearest_chunks_are_asked_for_first_sixteen_at_a_time() {
        let mut s = streaming();
        for x in -10..=10 {
            for z in -10..=10 {
                s.want((x, z));
            }
        }
        let first = s.plan(0.016, (0, 0));
        assert_eq!(first.len(), MAX_IN_FLIGHT);
        assert_eq!(first[0], (0, 0));
        let far = first.iter().map(|&k| dist2(k, (0, 0))).max().unwrap();
        assert!(far <= 5, "the sixteen nearest, not {far}");
        // Nothing more until some are answered.
        assert!(s.plan(0.016, (0, 0)).is_empty());
        let whole = s.on_part(0, 0, 0, 1, alloc::vec![1, 2, 3]).unwrap();
        assert_eq!(whole, alloc::vec![1, 2, 3]);
        let next = s.plan(0.016, (0, 0));
        assert_eq!(next.len(), 1);
        assert!(!first.contains(&next[0]));
        // The player walked: the nearest to them now come first.
        let mut t = streaming();
        for x in 0..30 {
            t.want((x, 0));
        }
        assert_eq!(t.plan(0.0, (29, 0))[0], (29, 0));
    }

    #[test]
    fn unanswered_requests_are_made_again_after_three_seconds_and_not_before() {
        let mut s = streaming();
        s.want((4, 4));
        assert_eq!(s.plan(0.1, (0, 0)).len(), 1);
        for _ in 0..25 {
            assert!(s.plan(0.1, (0, 0)).is_empty(), "still waiting");
        }
        let mut again = Vec::new();
        for _ in 0..10 {
            again.extend(s.plan(0.1, (0, 0)));
        }
        assert_eq!(again, alloc::vec![(4, 4)]);
        assert_eq!(s.stats.retried, 1);
        assert_eq!(s.stats.requested, 2);
    }

    #[test]
    fn at_most_a_hundred_and_twenty_a_second() {
        let mut s = streaming();
        let mut asked = 0;
        // Answer every request at once, so only the rate limits (not the sixteen).
        for i in 0..2000 {
            s.want((i % 100, i / 100));
        }
        for _ in 0..100 {
            for k in s.plan(0.01, (0, 0)) {
                asked += 1;
                s.on_part(k.0, k.1, 0, 1, alloc::vec![9]);
            }
        }
        // A second passed: the burst held at the start plus a second's worth.
        assert!((200..=245).contains(&asked), "{asked}");
        assert!(s.open() > 0);
    }

    #[test]
    fn pieces_are_put_together_in_any_order_and_strangers_are_ignored() {
        let mut s = streaming();
        s.want((2, 3));
        // Not asked for yet: ignored.
        assert!(s.on_part(2, 3, 0, 1, alloc::vec![1]).is_none());
        s.plan(0.1, (0, 0));
        // Not wanted at all.
        assert!(s.on_part(9, 9, 0, 1, alloc::vec![1]).is_none());
        // Nonsense counts.
        assert!(s.on_part(2, 3, 0, 0, alloc::vec![1]).is_none());
        assert!(s.on_part(2, 3, 3, 3, alloc::vec![1]).is_none());
        assert!(s.on_part(2, 3, 0, 99, alloc::vec![1]).is_none());
        assert!(s.on_part(2, 3, 2, 3, alloc::vec![3, 3]).is_none());
        assert!(s.on_part(2, 3, 0, 3, alloc::vec![1]).is_none());
        assert!(s.on_part(2, 3, 0, 3, alloc::vec![1]).is_none(), "the same piece twice");
        assert_eq!(s.on_part(2, 3, 1, 3, alloc::vec![2, 2]).unwrap(), alloc::vec![1, 2, 2, 3, 3]);
        assert_eq!(s.open(), 0);
        // After it is complete, more pieces of it are strangers.
        assert!(s.on_part(2, 3, 0, 1, alloc::vec![1]).is_none());
        assert_eq!(s.stats.received, 1);
    }

    #[test]
    fn a_damaged_chunk_is_asked_for_again_and_a_good_one_is_kept_for_later() {
        let mut s = streaming();
        s.want((1, 0));
        s.plan(0.1, (0, 0));
        let whole = s.on_part(1, 0, 0, 1, alloc::vec![5; 10]).unwrap();
        s.damaged((1, 0));
        assert_eq!(s.open(), 1);
        for _ in 0..3 {
            assert!(s.plan(0.9, (0, 0)).is_empty());
        }
        assert_eq!(s.plan(0.5, (0, 0)), alloc::vec![(1, 0)]);
        // A good one: kept, and handed back when the world wants it again.
        s.remember((1, 0), whole.clone());
        assert_eq!(s.kept_chunks(), 1);
        assert_eq!(s.want((1, 0)), Some(whole));
        assert_eq!(s.want((7, 7)), None);
    }

    #[test]
    fn what_is_kept_is_bounded() {
        let mut s = streaming();
        for i in 0..(KEEP_CHUNKS as i32 + 50) {
            s.remember((i, 0), alloc::vec![0; 100]);
        }
        assert_eq!(s.kept_chunks(), KEEP_CHUNKS);
        assert_eq!(s.kept_bytes(), KEEP_CHUNKS * 100);
        assert!(s.want((0, 0)).is_none(), "the oldest went");
        let mut t = streaming();
        t.remember((0, 0), alloc::vec![0; KEEP_BYTES / 2 + 1]);
        t.remember((1, 0), alloc::vec![0; KEEP_BYTES / 2 + 1]);
        assert!(t.kept_bytes() <= KEEP_BYTES);
        t.remember((2, 0), alloc::vec![0; KEEP_BYTES + 1]);
        assert!(t.want((2, 0)).is_none());
    }

    #[test]
    fn chunks_the_world_dropped_are_not_asked_for() {
        let mut s = streaming();
        s.want((0, 0));
        s.want((50, 0));
        s.forget_far((0, 0), 20 * 20);
        assert_eq!(s.open(), 1);
        assert_eq!(s.plan(0.1, (0, 0)), alloc::vec![(0, 0)]);
    }

    #[test]
    fn odd_time_steps_do_not_upset_it() {
        let mut s = streaming();
        s.want((0, 0));
        assert_eq!(s.plan(f32::NAN, (0, 0)).len(), 1);
        s.plan(-4.0, (0, 0));
        s.plan(1.0e9, (0, 0));
        s.plan(f32::INFINITY, (i32::MIN, i32::MAX));
        assert_eq!(s.mode, Mode::Streaming);
    }
}
