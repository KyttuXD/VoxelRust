//! Huffman decoding of the 576 quantised values of a granule.

use super::bits::BitReader;
use super::header::Granule;
use super::tables::{BAD_TABLE, HUFF, LINBITS, QUAD_A_CODES, QUAD_A_LENS, TABLE_OF_SELECT, ZERO_TABLE};
use alloc::vec;
use alloc::vec::Vec;

/// A binary tree for each table, built once: node `i` has `[child for 0, child for 1]`; a
/// positive entry is the index of a node, a negative one is a leaf holding `-(symbol) - 1`,
/// and 0 is a code that does not exist.
pub(super) struct Huffman {
    pub trees: Vec<Vec<[i16; 2]>>,
    pub quad_a: Vec<[i16; 2]>,
}

fn build(syms: impl Iterator<Item = (u8, u32)>) -> Vec<[i16; 2]> {
    let mut nodes = vec![[0i16; 2]];
    for (v, (len, code)) in syms.enumerate() {
        let mut cur = 0usize;
        for i in (0..len as u32).rev() {
            let b = ((code >> i) & 1) as usize;
            if i == 0 {
                nodes[cur][b] = -(v as i16) - 1;
            } else {
                let mut next = nodes[cur][b];
                if next <= 0 {
                    nodes.push([0, 0]);
                    next = (nodes.len() - 1) as i16;
                    nodes[cur][b] = next;
                }
                cur = next as usize;
            }
        }
    }
    nodes
}

/// Follow bits down a tree to a symbol.
pub(super) fn walk(tree: &[[i16; 2]], br: &mut BitReader) -> Option<usize> {
    let mut cur = 0usize;
    for _ in 0..32 {
        let next = tree.get(cur)?[br.bit()? as usize];
        if next < 0 {
            return Some((-(next as i32) - 1) as usize);
        }
        if next == 0 {
            return None;
        }
        cur = next as usize;
    }
    None
}

impl Huffman {
    pub fn new() -> Huffman {
        let trees = HUFF.iter().map(|t| build(t.lens.iter().zip(t.codes.iter()).map(|(&l, &c)| (l, c as u32)))).collect();
        let quad_a = build(QUAD_A_LENS.iter().zip(QUAD_A_CODES.iter()).map(|(&l, &c)| (l, c as u32)));
        Huffman { trees, quad_a }
    }

    /// Decode the values of one granule from `br`, which is at the first Huffman bit; the data
    /// ends at bit `end_bit`. `sfl` are the long scale factor band edges. `is` is zeroed first.
    pub fn decode(&self, br: &mut BitReader, g: &Granule, sfl: &[u16; 23], end_bit: usize, is: &mut [i32; 576]) -> Option<()> {
        *is = [0; 576];
        let big = (g.big_values * 2).min(576);
        // Where the second and third regions start (in lines).
        let (b1, b2) = if g.window_switching {
            (if g.block_type == 2 { 36 } else { sfl[8] as usize }, 576)
        } else {
            (sfl[(g.region0_count + 1).min(22)] as usize, sfl[(g.region0_count + g.region1_count + 2).min(22)] as usize)
        };
        let bounds = [b1.min(big), b2.min(big), big];
        let mut pos = 0usize;
        for region in 0..3 {
            let end = bounds[region];
            let sel = (g.table_select[region] & 31) as usize;
            let which = TABLE_OF_SELECT[sel];
            if pos >= end {
                continue;
            }
            if which == ZERO_TABLE {
                pos = end;
                continue;
            }
            if which == BAD_TABLE {
                return None;
            }
            let n = HUFF[which as usize].n;
            let tree = &self.trees[which as usize];
            let lin = LINBITS[sel] as u32;
            while pos < end && pos + 1 < 576 {
                let v = walk(tree, br)?;
                let (mut x, mut y) = ((v / n) as i32, (v % n) as i32);
                if x == 15 && lin > 0 {
                    x += br.bits(lin)? as i32;
                }
                if x != 0 && br.bit()? == 1 {
                    x = -x;
                }
                if y == 15 && lin > 0 {
                    y += br.bits(lin)? as i32;
                }
                if y != 0 && br.bit()? == 1 {
                    y = -y;
                }
                is[pos] = x;
                is[pos + 1] = y;
                pos += 2;
            }
        }
        // The rest is groups of four values of magnitude 0 or 1.
        while pos <= 572 && br.pos() < end_bit {
            let v = if g.count1_table_b { 15 - br.bits(4)? as usize } else { walk(&self.quad_a, br)? };
            let mut q = [((v >> 3) & 1) as i32, ((v >> 2) & 1) as i32, ((v >> 1) & 1) as i32, (v & 1) as i32];
            for k in 0..4 {
                if q[k] != 0 && br.bit()? == 1 {
                    q[k] = -1;
                }
            }
            if br.pos() > end_bit {
                break; // that group ran past the data: it is not one
            }
            is[pos..pos + 4].copy_from_slice(&q);
            pos += 4;
        }
        Some(())
    }
}
