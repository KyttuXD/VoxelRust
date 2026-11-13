//! Residues: the fine spectral detail, coded as vectors from codebooks in
//! up to eight passes. Types 0, 1 and 2 differ in how vectors are laid out.

use super::bits::Bits;
use super::codebook::Codebook;
use alloc::vec;
use alloc::vec::Vec;

pub(super) struct Residue {
    kind: u32,
    begin: usize,
    end: usize,
    psize: usize,
    nclass: usize,
    classbook: usize,
    /// Per class, the book of each of the eight passes (-1: none).
    books: Vec<[i32; 8]>,
}

impl Residue {
    pub(super) fn parse(b: &mut Bits, nbooks: usize) -> Option<Residue> {
        let kind = b.read(16);
        if kind > 2 {
            return None;
        }
        let begin = b.read(24) as usize;
        let end = b.read(24) as usize;
        let psize = b.read(24) as usize + 1;
        let nclass = b.read(6) as usize + 1;
        let classbook = b.read(8) as usize;
        if classbook >= nbooks {
            return None;
        }
        let mut cascade = Vec::new();
        for _ in 0..nclass {
            let low = b.read(3);
            let high = if b.bit() == 1 { b.read(5) } else { 0 };
            cascade.push(high * 8 + low);
        }
        let mut books = Vec::new();
        for c in cascade {
            let mut row = [-1i32; 8];
            for (j, slot) in row.iter_mut().enumerate() {
                if c & (1 << j) != 0 {
                    let v = b.read(8) as usize;
                    if v >= nbooks {
                        return None;
                    }
                    *slot = v as i32;
                }
            }
            books.push(row);
        }
        if b.eop {
            return None;
        }
        Some(Residue { kind, begin, end, psize, nclass, classbook, books })
    }

    /// Decode the residue of the channels in `out` (each `half` long, zeroed;
    /// `skip[i]` channels carry nothing) and add it into them.
    /// `None` for a damaged stream; the end of the packet just stops early.
    pub(super) fn decode(&self, b: &mut Bits, books: &[Codebook], half: usize, skip: &[bool], out: &mut [Vec<f32>]) -> Option<()> {
        let nch = out.len();
        if nch == 0 || skip.len() != nch {
            return Some(());
        }
        if self.kind == 2 {
            if skip.iter().all(|&s| s) {
                return Some(());
            }
            let mut one = vec![vec![0f32; half * nch]];
            self.run(b, books, &[false], &mut one)?;
            for (c, v) in out.iter_mut().enumerate() {
                for i in 0..half {
                    v[i] += one[0][i * nch + c];
                }
            }
            Some(())
        } else {
            self.run(b, books, skip, out)
        }
    }

    fn run(&self, b: &mut Bits, books: &[Codebook], skip: &[bool], vecs: &mut [Vec<f32>]) -> Option<()> {
        let size = vecs[0].len();
        let lb = self.begin.min(size);
        let le = self.end.min(size);
        if le <= lb {
            return Some(());
        }
        let nparts = (le - lb) / self.psize;
        if nparts == 0 {
            return Some(());
        }
        let cbook = books.get(self.classbook)?;
        let cw = cbook.dims;
        let nch = vecs.len();
        let mut classes = vec![vec![0u8; nparts + cw]; nch];
        let mut tmp: Vec<f32> = Vec::new();
        for pass in 0..8 {
            let mut pc = 0;
            while pc < nparts {
                if pass == 0 {
                    for j in 0..nch {
                        if skip[j] {
                            continue;
                        }
                        let Some(mut t) = cbook.scalar(b) else {
                            return if b.eop { Some(()) } else { None };
                        };
                        for i in (0..cw).rev() {
                            classes[j][pc + i] = (t % self.nclass) as u8;
                            t /= self.nclass;
                        }
                    }
                }
                let mut i = 0;
                while i < cw && pc < nparts {
                    for j in 0..nch {
                        if skip[j] {
                            continue;
                        }
                        let book = self.books[classes[j][pc] as usize][pass];
                        if book < 0 {
                            continue;
                        }
                        let bk = books.get(book as usize)?;
                        let dims = bk.dims;
                        let offset = lb + pc * self.psize;
                        let v = &mut vecs[j];
                        if self.kind == 0 {
                            // Interleaved: the k-th value of each vector goes `step` apart.
                            let step = self.psize / dims;
                            for s in 0..step {
                                let Some(e) = bk.scalar(b) else {
                                    return if b.eop { Some(()) } else { None };
                                };
                                bk.vector(e, &mut tmp)?;
                                for k in 0..dims {
                                    if let Some(o) = v.get_mut(offset + s + k * step) {
                                        *o += tmp[k];
                                    }
                                }
                            }
                        } else {
                            // One vector after another.
                            for s in 0..self.psize / dims {
                                let Some(e) = bk.scalar(b) else {
                                    return if b.eop { Some(()) } else { None };
                                };
                                bk.vector(e, &mut tmp)?;
                                for k in 0..dims {
                                    if let Some(o) = v.get_mut(offset + s * dims + k) {
                                        *o += tmp[k];
                                    }
                                }
                            }
                        }
                    }
                    pc += 1;
                    i += 1;
                }
            }
        }
        Some(())
    }
}
