//! Deflate compression (RFC 1951) in a zlib wrapper (RFC 1950), for the PNG
//! encoder (`png_encode.rs`).
//!
//! LZ77 with hash chains and lazy matching finds the repeats; every block is
//! then written with its own dynamic Huffman codes, or stored as it is if
//! that comes out smaller (noise). The result is what any inflater reads
//! (the decoder in `png.rs` included); see docs/IMAGES.md.
//!
//! How hard it looks for repeats depends on the size of the input, so that
//! a skin of 1024 x 1024 takes a fraction of a second and one of 4096 x 4096
//! a few seconds at most.

use super::png::adler32;
use alloc::collections::BinaryHeap;
use alloc::vec;
use alloc::vec::Vec;
use core::cmp::Reverse;

const WSIZE: usize = 32768;
const WMASK: usize = WSIZE - 1;
const HASH_BITS: u32 = 15;
const MIN_MATCH: usize = 3;
const MAX_MATCH: usize = 258;
/// Symbols (literals and matches) in one block, at most.
const BLOCK_SYMBOLS: usize = 1 << 16;
/// A token that is a match: `MATCH | (length - 3) << 16 | (distance - 1)`.
const MATCH: u32 = 0x8000_0000;

const LEN_BASE: [u16; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEN_EXTRA: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
const DIST_EXTRA: [u8; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
/// The order the code-length code lengths are stored in.
const CLEN_ORDER: [u8; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// The length symbol (0..29, that is 257 + this) of every match length.
const fn len_table() -> [u8; 259] {
    let mut t = [0u8; 259];
    let mut len = 3;
    while len <= 258 {
        let mut s = 28;
        while LEN_BASE[s] as usize > len {
            s -= 1;
        }
        t[len] = s as u8;
        len += 1;
    }
    t
}
static LEN_SYM: [u8; 259] = len_table();

/// The distance symbol (0..30) of a distance of 1..=32768.
fn dist_sym(dist: usize) -> usize {
    let d = dist - 1;
    if d < 4 {
        d
    } else {
        let n = (usize::BITS - 1 - d.leading_zeros()) as usize;
        2 * n + ((d >> (n - 1)) & 1)
    }
}

/// How much effort to put into finding matches.
#[derive(Clone, Copy)]
struct Effort {
    /// Candidates looked at per position.
    chain: usize,
    /// A match this long is good enough.
    nice: usize,
    /// A match shorter than this is only taken if the next position has no
    /// longer one (0 = always take the first).
    lazy: usize,
    /// Matches longer than this do not put every position they cover in the table.
    skip: usize,
}

impl Effort {
    fn for_len(n: usize) -> Effort {
        if n <= 8 << 20 {
            Effort { chain: 48, nice: 128, lazy: 24, skip: 48 }
        } else if n <= 24 << 20 {
            Effort { chain: 16, nice: 64, lazy: 12, skip: 32 }
        } else {
            Effort { chain: 8, nice: 48, lazy: 0, skip: 24 }
        }
    }
}

struct BitWriter {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitWriter {
    /// Write the low `n` (at most 16) bits of `v`, least significant first.
    fn bits(&mut self, v: u32, n: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    fn align(&mut self) {
        if self.n > 0 {
            self.out.push(self.acc as u8);
            self.acc = 0;
            self.n = 0;
        }
    }
}

/// Finds repeats: a table of where each three-byte hash was last seen, and
/// for every position the one before it with the same hash.
struct Matcher<'a> {
    d: &'a [u8],
    head: Vec<u32>,
    prev: Vec<u32>,
    /// Positions before this are in the table.
    ins: usize,
    effort: Effort,
}

impl<'a> Matcher<'a> {
    fn new(d: &'a [u8], effort: Effort) -> Matcher<'a> {
        Matcher { d, head: vec![0; 1 << HASH_BITS], prev: vec![0; WSIZE], ins: 0, effort }
    }

    fn hash(&self, p: usize) -> usize {
        let v = self.d[p] as u32 | (self.d[p + 1] as u32) << 8 | (self.d[p + 2] as u32) << 16;
        (v.wrapping_mul(0x9E37_79B1) >> (32 - HASH_BITS)) as usize
    }

    /// Put every position before `q` in the table.
    fn upto(&mut self, q: usize) {
        while self.ins < q {
            let p = self.ins;
            if p + 2 < self.d.len() {
                let h = self.hash(p);
                self.prev[p & WMASK] = self.head[h];
                self.head[h] = (p + 1) as u32;
            }
            self.ins += 1;
        }
    }

    /// The longest match at `p` that is longer than `above`: (length,
    /// distance), or (0, 0). Needs every position before `p` in the table.
    fn find(&self, p: usize, above: usize) -> (usize, usize) {
        let d = self.d;
        let n = d.len();
        if p + MIN_MATCH > n {
            return (0, 0);
        }
        let max = MAX_MATCH.min(n - p);
        let mut best = above.max(MIN_MATCH - 1);
        if best >= max {
            return (0, 0);
        }
        let mut best_dist = 0;
        let mut cand = self.head[self.hash(p)] as usize;
        let mut chain = self.effort.chain;
        while cand != 0 && chain > 0 {
            let c = cand - 1;
            if c >= p || p - c > WSIZE {
                break;
            }
            if d[c + best] == d[p + best] && d[c] == d[p] {
                let mut l = 0;
                while l < max && d[c + l] == d[p + l] {
                    l += 1;
                }
                if l > best {
                    best = l;
                    best_dist = p - c;
                    if l >= self.effort.nice || l >= max {
                        break;
                    }
                }
            }
            // The chain only ever goes back in time (a slot of the ring that
            // was written over by a newer position would not).
            let next = self.prev[c & WMASK] as usize;
            if next == 0 || next - 1 >= c {
                break;
            }
            cand = next;
            chain -= 1;
        }
        // A match of three bytes far away costs more than the bytes.
        if best_dist == 0 || (best == MIN_MATCH && best_dist > 4096) {
            return (0, 0);
        }
        (best, best_dist)
    }
}

/// Canonical Huffman code lengths for these frequencies, none longer than
/// `limit`. At least two symbols always get a code (so that the code is
/// complete, which every decoder accepts).
fn build_lengths(freq: &[u32], limit: u8) -> Vec<u8> {
    let mut f: Vec<u32> = freq.to_vec();
    let used = f.iter().filter(|&&x| x > 0).count();
    if used < 2 {
        let mut need = 2 - used;
        for x in f.iter_mut() {
            if *x == 0 && need > 0 {
                *x = 1;
                need -= 1;
            }
        }
    }
    loop {
        let lens = huffman_depths(&f);
        if lens.iter().all(|&l| l <= limit) {
            return lens;
        }
        // Too deep: flatten the frequencies and try again (all ones is a balanced tree).
        for x in f.iter_mut() {
            if *x > 0 {
                *x = (*x + 1) / 2;
            }
        }
    }
}

/// The depth of every symbol in the Huffman tree of `f` (0 for unused symbols).
fn huffman_depths(f: &[u32]) -> Vec<u8> {
    let n = f.len();
    let mut parent = vec![usize::MAX; 2 * n];
    let mut heap: BinaryHeap<Reverse<(u64, usize)>> = BinaryHeap::new();
    for (i, &x) in f.iter().enumerate() {
        if x > 0 {
            heap.push(Reverse((x as u64, i)));
        }
    }
    let mut next = n;
    while heap.len() > 1 {
        let (Some(Reverse(a)), Some(Reverse(b))) = (heap.pop(), heap.pop()) else { break };
        parent[a.1] = next;
        parent[b.1] = next;
        heap.push(Reverse((a.0 + b.0, next)));
        next += 1;
    }
    let mut lens = vec![0u8; n];
    for i in 0..n {
        if f[i] > 0 {
            let (mut depth, mut at) = (0u32, i);
            while parent[at] != usize::MAX {
                at = parent[at];
                depth += 1;
            }
            lens[i] = depth.min(255) as u8;
        }
    }
    lens
}

/// The codes of canonical Huffman lengths, with the bits turned around so
/// that they can be written least significant bit first.
fn codes_of(lengths: &[u8]) -> Vec<u16> {
    let mut count = [0u32; 17];
    for &l in lengths {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut next = [0u32; 17];
    let mut code = 0u32;
    for bits in 1..16 {
        code = (code + count[bits - 1]) << 1;
        next[bits] = code;
    }
    let mut out = vec![0u16; lengths.len()];
    for (sym, &l) in lengths.iter().enumerate() {
        if l == 0 {
            continue;
        }
        let c = next[l as usize];
        next[l as usize] += 1;
        let mut rev = 0u32;
        for k in 0..l as u32 {
            rev |= ((c >> k) & 1) << (l as u32 - 1 - k);
        }
        out[sym] = rev as u16;
    }
    out
}

/// The code lengths run-length coded: (code-length symbol, extra bits, their value).
fn rle_lengths(seq: &[u8]) -> Vec<(u8, u8, u8)> {
    let mut items = Vec::new();
    let mut i = 0;
    while i < seq.len() {
        let l = seq[i];
        let mut run = 1;
        while i + run < seq.len() && seq[i + run] == l {
            run += 1;
        }
        let mut r = run;
        if l == 0 {
            while r >= 11 {
                let k = r.min(138);
                items.push((18, 7, (k - 11) as u8));
                r -= k;
            }
            if r >= 3 {
                items.push((17, 3, (r - 3) as u8));
                r = 0;
            }
        } else {
            items.push((l, 0, 0));
            r -= 1;
            while r >= 3 {
                let k = r.min(6);
                items.push((16, 2, (k - 3) as u8));
                r -= k;
            }
        }
        for _ in 0..r {
            items.push((l, 0, 0));
        }
        i += run;
    }
    items
}

/// Write one block: the symbols in `toks`, which stand for `raw` (the bytes
/// they were made from, for storing them instead if that is shorter).
fn write_block(bw: &mut BitWriter, toks: &[u32], raw: &[u8], last: bool) {
    let mut fl = [0u32; 286];
    let mut fd = [0u32; 30];
    let mut extra_bits = 0u64;
    for &t in toks {
        if t & MATCH == 0 {
            fl[t as usize] += 1;
        } else {
            let len = ((t >> 16) & 0x1FF) as usize + 3;
            let dist = (t & 0xFFFF) as usize + 1;
            let ls = LEN_SYM[len] as usize;
            let ds = dist_sym(dist);
            fl[257 + ls] += 1;
            fd[ds] += 1;
            extra_bits += LEN_EXTRA[ls] as u64 + DIST_EXTRA[ds] as u64;
        }
    }
    fl[256] = 1;
    let ll = build_lengths(&fl, 15);
    let dl = build_lengths(&fd, 15);
    let hlit = (ll.iter().rposition(|&l| l > 0).map_or(257, |i| i + 1)).max(257);
    let hdist = (dl.iter().rposition(|&l| l > 0).map_or(1, |i| i + 1)).max(1);
    let mut seq: Vec<u8> = Vec::with_capacity(hlit + hdist);
    seq.extend_from_slice(&ll[..hlit]);
    seq.extend_from_slice(&dl[..hdist]);
    let items = rle_lengths(&seq);
    let mut fc = [0u32; 19];
    for it in &items {
        fc[it.0 as usize] += 1;
    }
    let cl = build_lengths(&fc, 7);
    let hclen = (4..=19).rev().find(|&k| cl[CLEN_ORDER[k - 1] as usize] > 0).unwrap_or(4).max(4);

    // What each way costs, in bits.
    let mut dyn_bits: u64 = 3 + 14 + 3 * hclen as u64 + extra_bits;
    for it in &items {
        dyn_bits += cl[it.0 as usize] as u64 + it.1 as u64;
    }
    for i in 0..286 {
        dyn_bits += fl[i] as u64 * ll[i] as u64;
    }
    for i in 0..30 {
        dyn_bits += fd[i] as u64 * dl[i] as u64;
    }
    let pieces = if raw.is_empty() { 1 } else { (raw.len() + 65534) / 65535 };
    let stored_bits = (raw.len() + pieces * 5) as u64 * 8 + 7;

    if stored_bits <= dyn_bits {
        let mut chunks: Vec<&[u8]> = raw.chunks(65535).collect();
        if chunks.is_empty() {
            chunks.push(&[]);
        }
        let n = chunks.len();
        for (i, c) in chunks.into_iter().enumerate() {
            bw.bits((last && i + 1 == n) as u32, 1);
            bw.bits(0, 2);
            bw.align();
            let len = c.len() as u16;
            bw.out.extend_from_slice(&len.to_le_bytes());
            bw.out.extend_from_slice(&(!len).to_le_bytes());
            bw.out.extend_from_slice(c);
        }
        return;
    }

    bw.bits(last as u32, 1);
    bw.bits(2, 2);
    bw.bits((hlit - 257) as u32, 5);
    bw.bits((hdist - 1) as u32, 5);
    bw.bits((hclen - 4) as u32, 4);
    for k in 0..hclen {
        bw.bits(cl[CLEN_ORDER[k] as usize] as u32, 3);
    }
    let cc = codes_of(&cl);
    for it in &items {
        bw.bits(cc[it.0 as usize] as u32, cl[it.0 as usize] as u32);
        if it.1 > 0 {
            bw.bits(it.2 as u32, it.1 as u32);
        }
    }
    let lc = codes_of(&ll);
    let dc = codes_of(&dl);
    for &t in toks {
        if t & MATCH == 0 {
            bw.bits(lc[t as usize] as u32, ll[t as usize] as u32);
        } else {
            let len = ((t >> 16) & 0x1FF) as usize + 3;
            let dist = (t & 0xFFFF) as usize + 1;
            let ls = LEN_SYM[len] as usize;
            let ds = dist_sym(dist);
            bw.bits(lc[257 + ls] as u32, ll[257 + ls] as u32);
            if LEN_EXTRA[ls] > 0 {
                bw.bits((len - LEN_BASE[ls] as usize) as u32, LEN_EXTRA[ls] as u32);
            }
            bw.bits(dc[ds] as u32, dl[ds] as u32);
            if DIST_EXTRA[ds] > 0 {
                bw.bits((dist - DIST_BASE[ds] as usize) as u32, DIST_EXTRA[ds] as u32);
            }
        }
    }
    bw.bits(lc[256] as u32, ll[256] as u32);
}

fn deflate(d: &[u8], bw: &mut BitWriter) {
    let n = d.len();
    if n == 0 {
        // One empty block with the fixed codes: just the end-of-block code.
        bw.bits(1, 1);
        bw.bits(1, 2);
        bw.bits(0, 7);
        return;
    }
    let effort = Effort::for_len(n);
    let mut m = Matcher::new(d, effort);
    let mut toks: Vec<u32> = Vec::with_capacity(BLOCK_SYMBOLS);
    let (mut start, mut p) = (0usize, 0usize);
    let mut cur = m.find(0, MIN_MATCH - 1);
    while p < n {
        let (l, dist) = cur;
        if l >= MIN_MATCH {
            let mut take = true;
            let mut next = (0, 0);
            if l < effort.lazy && p + 1 < n {
                m.upto(p + 1);
                next = m.find(p + 1, l);
                if next.0 > l {
                    take = false;
                }
            }
            if take {
                toks.push(MATCH | ((l - MIN_MATCH) as u32) << 16 | (dist - 1) as u32);
                let end = p + l;
                if l > effort.skip {
                    // A long repeat: the first few positions are enough to find it again.
                    m.upto(p + 8);
                    m.ins = m.ins.max(end);
                } else {
                    m.upto(end);
                }
                p = end;
                cur = if p < n { m.find(p, MIN_MATCH - 1) } else { (0, 0) };
            } else {
                toks.push(d[p] as u32);
                p += 1;
                cur = next;
            }
        } else {
            toks.push(d[p] as u32);
            p += 1;
            m.upto(p);
            cur = if p < n { m.find(p, MIN_MATCH - 1) } else { (0, 0) };
        }
        if toks.len() >= BLOCK_SYMBOLS && p < n {
            write_block(bw, &toks, &d[start..p], false);
            toks.clear();
            start = p;
        }
    }
    write_block(bw, &toks, &d[start..n], true);
}

/// `data` as a zlib stream: header, deflate blocks, Adler-32.
pub fn zlib(data: &[u8]) -> Vec<u8> {
    let mut bw = BitWriter { out: Vec::with_capacity(data.len() / 3 + 64), acc: 0, n: 0 };
    // Deflate, 32 KiB window, no dictionary; the check bits make the pair a multiple of 31.
    bw.out.extend_from_slice(&[0x78, 0x9C]);
    deflate(data, &mut bw);
    bw.align();
    bw.out.extend_from_slice(&adler32(data).to_be_bytes());
    bw.out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::png::inflate;

    fn round(data: &[u8]) -> usize {
        let z = zlib(data);
        assert_eq!(inflate(&z).as_deref(), Some(data), "round trip of {} bytes", data.len());
        z.len()
    }

    /// Deterministic noise.
    fn noise(n: usize, seed: u32) -> Vec<u8> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                (s >> 24) as u8
            })
            .collect()
    }

    #[test]
    fn small_and_odd_inputs_round_trip() {
        round(&[]);
        round(&[7]);
        round(&[1, 2]);
        round(b"abcabcabcabcabcabc");
        round(&[0; 1000]);
        round(&[0; 300_000]);
        let mut mixed = Vec::new();
        for i in 0..20_000u32 {
            mixed.extend_from_slice(&i.to_le_bytes()[..2]);
            mixed.extend_from_slice(b"the same words again and again ");
        }
        let n = round(&mixed);
        assert!(n < mixed.len() / 4, "repeats are found: {} of {}", n, mixed.len());
    }

    #[test]
    fn noise_is_stored_and_costs_almost_nothing_extra() {
        let d = noise(200_000, 7);
        let n = round(&d);
        assert!(n < d.len() + 100, "{} for {}", n, d.len());
    }

    #[test]
    fn runs_of_two_symbols_and_one_symbol_use_complete_codes() {
        // Only one literal and no matches at all would need a code of one symbol.
        round(&[5, 9]);
        let two: Vec<u8> = (0..5000).map(|i| if i % 7 < 3 { 1 } else { 2 }).collect();
        round(&two);
        // Distances that need every distance symbol.
        let mut far = noise(40_000, 3);
        let copy = far[..300].to_vec();
        far.extend_from_slice(&copy);
        round(&far);
    }

    #[test]
    fn many_blocks_and_the_window_edge() {
        // More symbols than one block holds, with repeats at the largest distance.
        let base = noise(32_768, 11);
        let mut d = base.clone();
        d.extend_from_slice(&base);
        d.extend_from_slice(&noise(100_000, 12));
        d.extend_from_slice(&base);
        round(&d);
    }

    #[test]
    fn lengths_and_distances_map_to_their_symbols() {
        for len in 3..=258usize {
            let s = LEN_SYM[len] as usize;
            let top = LEN_BASE[s] as usize + (1usize << LEN_EXTRA[s]) - 1;
            assert!(LEN_BASE[s] as usize <= len && (len <= top || s == 28), "len {}", len);
        }
        for dist in 1..=32768usize {
            let s = dist_sym(dist);
            assert!(DIST_BASE[s] as usize <= dist && dist - (DIST_BASE[s] as usize) < (1usize << DIST_EXTRA[s]), "dist {}", dist);
        }
    }

    #[test]
    fn code_lengths_are_limited_and_complete() {
        // A very skewed set of frequencies would give a tree deeper than 15.
        let f: Vec<u32> = (0..40).map(|i| 1u32 << (i.min(30))).collect();
        let l = build_lengths(&f, 15);
        assert!(l.iter().all(|&x| x >= 1 && x <= 15));
        let kraft: f64 = l.iter().map(|&x| 1.0 / (1u64 << x) as f64).sum();
        assert!(kraft <= 1.0 + 1e-12, "{kraft}");
        // One used symbol still gets a partner.
        let l = build_lengths(&[0, 0, 9, 0], 15);
        assert_eq!(l.iter().filter(|&&x| x > 0).count(), 2);
    }
}
