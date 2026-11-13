//! Vorbis codebooks: a Huffman code for entry numbers, and optionally a table
//! that turns an entry into a vector of numbers (lookup types 1 and 2).

use super::bits::{ilog, Bits};
use alloc::vec;
use alloc::vec::Vec;

/// In the decoding tree, a child with this bit set is a leaf holding an entry number.
const LEAF: u32 = 0x8000_0000;

struct Lookup {
    /// 1: entries are indexes into a grid of values; 2: each entry has its own values.
    kind: u8,
    vals: Vec<f32>,
    /// How many values a grid axis has (type 1).
    values: usize,
    sequence: bool,
}

pub(super) struct Codebook {
    pub(super) dims: usize,
    pub(super) entries: usize,
    /// Binary decoding tree: node 0 is the root; a child of 0 is absent.
    tree: Vec<[u32; 2]>,
    lookup: Option<Lookup>,
}

/// Vorbis's own 32-bit floating point format.
fn float32_unpack(x: u32) -> f32 {
    let mantissa = (x & 0x1F_FFFF) as f64;
    let exponent = ((x & 0x7FE0_0000) >> 21) as i32 - 788;
    let mut v = mantissa;
    let mut e = exponent;
    while e > 0 {
        v *= 2.0;
        e -= 1;
    }
    while e < 0 {
        v *= 0.5;
        e += 1;
    }
    if x & 0x8000_0000 != 0 {
        v = -v;
    }
    let f = v as f32;
    if f.is_finite() {
        f
    } else {
        0.0
    }
}

/// The largest r with r to the power `dims` at most `entries`.
fn lookup1_values(entries: usize, dims: usize) -> usize {
    let mut r = 0usize;
    loop {
        let n = r + 1;
        let mut p = 1u64;
        for _ in 0..dims {
            p *= n as u64;
            if p > entries as u64 {
                return r;
            }
        }
        r = n;
    }
}

impl Codebook {
    pub(super) fn parse(b: &mut Bits) -> Option<Codebook> {
        if b.read(24) != 0x56_4342 {
            return None;
        }
        let dims = b.read(16) as usize;
        let entries = b.read(24) as usize;
        if dims == 0 || entries > 1 << 18 || b.eop {
            return None;
        }
        let mut lengths = vec![0u8; entries];
        if b.bit() == 0 {
            let sparse = b.bit() == 1;
            if entries > b.remaining_bits() {
                return None;
            }
            for l in lengths.iter_mut() {
                if sparse && b.bit() == 0 {
                    continue;
                }
                *l = b.read(5) as u8 + 1;
            }
        } else {
            let mut cur = b.read(5) + 1;
            let mut e = 0usize;
            while e < entries {
                if cur > 32 || b.eop {
                    return None;
                }
                let num = b.read(ilog((entries - e) as u32)) as usize;
                if e + num > entries {
                    return None;
                }
                for l in &mut lengths[e..e + num] {
                    *l = cur as u8;
                }
                e += num;
                cur += 1;
            }
        }
        if b.eop {
            return None;
        }
        let tree = build_tree(&lengths)?;
        let lookup = match b.read(4) {
            0 => None,
            kind @ (1 | 2) => {
                let min = float32_unpack(b.read(32));
                let delta = float32_unpack(b.read(32));
                let value_bits = b.read(4) + 1;
                let sequence = b.bit() == 1;
                let values = if kind == 1 { lookup1_values(entries, dims) } else { entries.checked_mul(dims)? };
                if values.checked_mul(value_bits as usize)? > b.remaining_bits() {
                    return None;
                }
                let mut vals = Vec::with_capacity(values);
                for _ in 0..values {
                    vals.push(b.read(value_bits) as f32 * delta + min);
                }
                Some(Lookup { kind: kind as u8, vals, values, sequence })
            }
            _ => return None,
        };
        if b.eop {
            return None;
        }
        Some(Codebook { dims, entries, tree, lookup })
    }

    /// Read one entry number. `None` when the bits run out or lead nowhere.
    pub(super) fn scalar(&self, b: &mut Bits) -> Option<usize> {
        let mut node = 0usize;
        for _ in 0..33 {
            let bit = b.bit() as usize;
            let next = self.tree.get(node)?[bit];
            if next == 0 || b.eop {
                return None;
            }
            if next & LEAF != 0 {
                return Some((next & !LEAF) as usize);
            }
            node = next as usize;
        }
        None
    }

    /// The vector of `dims` numbers an entry stands for, in `out`.
    pub(super) fn vector(&self, entry: usize, out: &mut Vec<f32>) -> Option<()> {
        let lk = self.lookup.as_ref()?;
        if entry >= self.entries {
            return None;
        }
        out.clear();
        let mut last = 0f32;
        if lk.kind == 1 {
            if lk.values == 0 {
                return None;
            }
            let mut div = 1usize;
            for _ in 0..self.dims {
                let v = *lk.vals.get((entry / div) % lk.values)? + last;
                if lk.sequence {
                    last = v;
                }
                out.push(v);
                div = div.saturating_mul(lk.values);
            }
        } else {
            let base = entry.checked_mul(self.dims)?;
            for j in 0..self.dims {
                let v = *lk.vals.get(base + j)? + last;
                if lk.sequence {
                    last = v;
                }
                out.push(v);
            }
        }
        Some(())
    }
}

/// Assign the codewords as Vorbis does (each entry takes the first free
/// codeword of its length) and put them in a tree.
fn build_tree(lengths: &[u8]) -> Option<Vec<[u32; 2]>> {
    let mut tree: Vec<[u32; 2]> = vec![[0, 0]];
    let mut avail = [0u32; 33];
    let mut first = true;
    for (entry, &len) in lengths.iter().enumerate() {
        if len == 0 {
            continue;
        }
        let len = len as usize;
        let code;
        if first {
            first = false;
            code = 0u32;
            for i in 1..=len {
                avail[i] = 1u32 << (32 - i);
            }
        } else {
            let mut z = len;
            while z > 0 && avail[z] == 0 {
                z -= 1;
            }
            if z == 0 {
                return None; // more codewords than the lengths allow
            }
            code = avail[z];
            avail[z] = 0;
            for y in (z + 1..=len).rev() {
                avail[y] = code.wrapping_add(1u32 << (32 - y));
            }
        }
        let mut node = 0usize;
        for i in 0..len {
            let bit = ((code >> (31 - i)) & 1) as usize;
            if i == len - 1 {
                if tree[node][bit] != 0 {
                    return None;
                }
                tree[node][bit] = LEAF | entry as u32;
            } else {
                let next = tree[node][bit];
                if next == 0 {
                    if tree.len() > 1 << 22 {
                        return None;
                    }
                    tree.push([0, 0]);
                    let id = tree.len() - 1;
                    tree[node][bit] = id as u32;
                    node = id;
                } else if next & LEAF != 0 {
                    return None;
                } else {
                    node = next as usize;
                }
            }
        }
    }
    Some(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_handed_out_in_order() {
        let tree = build_tree(&[1, 2, 3, 3]).unwrap();
        let book = Codebook { dims: 1, entries: 4, tree, lookup: None };
        // The codes 0, 10, 110, 111, one after the other, least significant bit first.
        let mut b = Bits::new(&[218, 1]);
        assert_eq!([book.scalar(&mut b), book.scalar(&mut b), book.scalar(&mut b), book.scalar(&mut b)], [Some(0), Some(1), Some(2), Some(3)]);
        // Three one-bit codes do not fit.
        assert!(build_tree(&[1, 1, 1]).is_none());
        // Unused entries (length 0) are skipped.
        let tree = build_tree(&[0, 1, 0, 1]).unwrap();
        let book = Codebook { dims: 1, entries: 4, tree, lookup: None };
        let mut b = Bits::new(&[0b10]);
        assert_eq!([book.scalar(&mut b), book.scalar(&mut b)], [Some(1), Some(3)]);
    }

    #[test]
    fn grid_size_and_floats() {
        assert_eq!(lookup1_values(81, 4), 3);
        assert_eq!(lookup1_values(80, 4), 2);
        assert_eq!(lookup1_values(1, 1), 1);
        assert_eq!(lookup1_values(0, 3), 0);
        // 1.0 is mantissa 2^20 at exponent 788 - 20 = 768.
        assert_eq!(float32_unpack((768 << 21) | (1 << 20)), 1.0);
        assert_eq!(float32_unpack(0x8000_0000 | (769 << 21) | (1 << 20)), -2.0);
    }
}
