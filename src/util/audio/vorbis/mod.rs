//! A Vorbis I decoder: the three headers, then audio packets turned into
//! samples (floor times residue per channel, channel decoupling, inverse MDCT,
//! window, overlap-add). It is given the packets an Ogg file holds (see
//! `ogg.rs`) and gives back interleaved samples.
//!
//! Everything here is bounded by the data it is given: damaged input ends in
//! `None`, never in a panic or a loop that does not end.

mod bits;
mod codebook;
mod floor;
mod mapping;
mod mathx;
mod mdct;
mod residue;
pub(crate) mod stream;

use super::{Pcm, MAX_SECONDS};
use alloc::vec;
use alloc::vec::Vec;
use bits::{ilog, Bits};
use codebook::Codebook;
use floor::Floor;
use mapping::{Mapping, Mode};
use mdct::Mdct;
use residue::Residue;

struct Setup {
    books: Vec<Codebook>,
    floors: Vec<Floor>,
    residues: Vec<Residue>,
    mappings: Vec<Mapping>,
    modes: Vec<Mode>,
}

struct Decoder {
    channels: usize,
    bs0: usize,
    bs1: usize,
    setup: Setup,
    mdct0: Mdct,
    mdct1: Mdct,
    /// The rising half of the window, for slopes of bs0/2 and of bs1/2 samples.
    slope0: Vec<f32>,
    slope1: Vec<f32>,
    db: Vec<f32>,
    /// The second half of the last block, per channel, still to be overlapped.
    pending: Vec<Vec<f32>>,
    /// The size of the last block (0: none yet).
    prev_n: usize,
}

fn slope(len: usize) -> Vec<f32> {
    let pi = core::f64::consts::PI;
    (0..len)
        .map(|i| {
            let s = mathx::sin(pi * (i as f64 + 0.5) / (2 * len) as f64);
            mathx::sin(pi * 0.5 * s * s) as f32
        })
        .collect()
}

fn header_start(p: &[u8], kind: u8) -> bool {
    p.len() >= 7 && p[0] == kind && &p[1..7] == b"vorbis"
}

/// Decode a Vorbis stream from its packets (the first three are the headers).
/// `total` is the number of samples per channel the container says there are,
/// if it says so.
pub(super) fn decode(packets: &[Vec<u8>], total: Option<u64>) -> Option<Pcm> {
    if packets.len() < 4 {
        return None;
    }
    let (channels, rate, bs0, bs1) = identification(&packets[0])?;
    if !header_start(&packets[1], 3) {
        return None;
    }
    let setup = parse_setup(&packets[2], channels)?;
    let mut d = Decoder {
        channels,
        bs0,
        bs1,
        setup,
        mdct0: Mdct::new(bs0),
        mdct1: Mdct::new(bs1),
        slope0: slope(bs0 / 2),
        slope1: slope(bs1 / 2),
        db: floor::inverse_db_table(),
        pending: vec![Vec::new(); channels],
        prev_n: 0,
    };
    let limit = (rate as u64 * MAX_SECONDS as u64 + 65_536) as usize * channels;
    let mut data: Vec<f32> = Vec::new();
    for p in &packets[3..] {
        d.block(p, &mut data)?;
        if data.len() > limit {
            return None;
        }
    }
    if let Some(t) = total {
        let want = (t as usize).saturating_mul(channels);
        if want < data.len() {
            data.truncate(want);
        }
    }
    if data.is_empty() {
        return None;
    }
    Some(Pcm { rate, channels, data })
}

/// The identification header: channels, rate and the two block sizes.
fn identification(p: &[u8]) -> Option<(usize, u32, usize, usize)> {
    if !header_start(p, 1) || p.len() < 30 {
        return None;
    }
    let u32at = |i: usize| u32::from_le_bytes([p[i], p[i + 1], p[i + 2], p[i + 3]]);
    if u32at(7) != 0 {
        return None;
    }
    let channels = p[11] as usize;
    let rate = u32at(12);
    let (e0, e1) = ((p[28] & 15) as u32, (p[28] >> 4) as u32);
    if channels == 0 || channels > 8 || rate == 0 || e0 < 6 || e0 > 13 || e1 < e0 || e1 > 13 || p[29] & 1 == 0 {
        return None;
    }
    Some((channels, rate, 1usize << e0, 1usize << e1))
}

fn parse_setup(p: &[u8], channels: usize) -> Option<Setup> {
    if !header_start(p, 5) {
        return None;
    }
    let mut b = Bits::new(&p[7..]);
    let mut books = Vec::new();
    for _ in 0..b.read(8) + 1 {
        books.push(Codebook::parse(&mut b)?);
    }
    // Time domain transforms: a count of placeholders, all zero.
    for _ in 0..b.read(6) + 1 {
        if b.read(16) != 0 {
            return None;
        }
    }
    let mut floors = Vec::new();
    for _ in 0..b.read(6) + 1 {
        floors.push(Floor::parse(&mut b, books.len())?);
    }
    let mut residues = Vec::new();
    for _ in 0..b.read(6) + 1 {
        residues.push(Residue::parse(&mut b, books.len())?);
    }
    let mut mappings = Vec::new();
    for _ in 0..b.read(6) + 1 {
        mappings.push(Mapping::parse(&mut b, channels, floors.len(), residues.len())?);
    }
    let mut modes = Vec::new();
    for _ in 0..b.read(6) + 1 {
        modes.push(Mode::parse(&mut b, mappings.len())?);
    }
    if b.bit() != 1 || b.eop {
        return None;
    }
    Some(Setup { books, floors, residues, mappings, modes })
}

impl Decoder {
    /// Decode one audio packet and append the samples it completes to `out`.
    fn block(&mut self, pkt: &[u8], out: &mut Vec<f32>) -> Option<()> {
        let mut b = Bits::new(pkt);
        if b.bit() != 0 || b.eop {
            return Some(()); // not an audio packet
        }
        let nmodes = self.setup.modes.len();
        let mode = self.setup.modes.get(b.read(ilog(nmodes as u32 - 1)) as usize)?;
        let long = mode.long;
        let (prev_long, next_long) = if long { (b.bit() == 1, b.bit() == 1) } else { (false, false) };
        let n = if long { self.bs1 } else { self.bs0 };
        let half = n / 2;
        let ch = self.channels;
        let map = &self.setup.mappings[mode.mapping];

        // Each channel's floor.
        let mut curves: Vec<Vec<f32>> = vec![Vec::new(); ch];
        let mut floor_ok = vec![false; ch];
        for c in 0..ch {
            let sm = &map.submaps[map.mux[c]];
            floor_ok[c] = self.setup.floors[sm.floor].decode(&mut b, &self.setup.books, &self.db, half, &mut curves[c])?;
        }

        // A coupled channel needs its residue if its partner does.
        let mut live = floor_ok.clone();
        for &(m, a) in &map.coupling {
            if live[m] || live[a] {
                live[m] = true;
                live[a] = true;
            }
        }

        // The residues, a submap at a time.
        let mut spec: Vec<Vec<f32>> = vec![vec![0f32; half]; ch];
        for (s, sm) in map.submaps.iter().enumerate() {
            let chans: Vec<usize> = (0..ch).filter(|&c| map.mux[c] == s).collect();
            if chans.is_empty() {
                continue;
            }
            let skip: Vec<bool> = chans.iter().map(|&c| !live[c]).collect();
            let mut vs: Vec<Vec<f32>> = chans.iter().map(|&c| core::mem::take(&mut spec[c])).collect();
            let r = self.setup.residues[sm.residue].decode(&mut b, &self.setup.books, half, &skip, &mut vs);
            for (v, &c) in vs.into_iter().zip(&chans) {
                spec[c] = v;
            }
            r?;
        }

        // Undo the channel coupling, last step first.
        for &(m, a) in map.coupling.iter().rev() {
            for i in 0..half {
                let (mv, av) = (spec[m][i], spec[a][i]);
                let (nm, na) = if mv > 0.0 {
                    if av > 0.0 {
                        (mv, mv - av)
                    } else {
                        (mv + av, mv)
                    }
                } else if av > 0.0 {
                    (mv, mv + av)
                } else {
                    (mv - av, mv)
                };
                spec[m][i] = nm;
                spec[a][i] = na;
            }
        }

        // Floor times residue, then to the time domain, windowed.
        let window = self.window(n, long, prev_long, next_long);
        let mdct = if long { &self.mdct1 } else { &self.mdct0 };
        let mut blocks: Vec<Vec<f32>> = Vec::with_capacity(ch);
        for c in 0..ch {
            let mut y = vec![0f32; n];
            if floor_ok[c] {
                for i in 0..half {
                    spec[c][i] *= curves[c][i];
                }
                mdct.inverse(&spec[c], &mut y);
                for i in 0..n {
                    y[i] *= window[i];
                }
            }
            blocks.push(y);
        }

        // Overlap with the end of the block before, and hand out what is complete.
        if self.prev_n != 0 {
            let pn = self.prev_n;
            let len = pn / 4 + n / 4;
            let shift = (pn / 4) as isize - (n / 4) as isize;
            out.reserve(len * ch);
            for i in 0..len {
                let idx = i as isize - shift;
                for c in 0..ch {
                    let mut v = self.pending[c].get(i).copied().unwrap_or(0.0);
                    if idx >= 0 && (idx as usize) < n {
                        v += blocks[c][idx as usize];
                    }
                    out.push(v);
                }
            }
        }
        for c in 0..ch {
            self.pending[c] = blocks[c][half..].to_vec();
        }
        self.prev_n = n;
        Some(())
    }

    /// The window of a block of `n` samples: slopes up and down, flat between,
    /// and a short slope where a long block meets a short one.
    fn window(&self, n: usize, long: bool, prev_long: bool, next_long: bool) -> Vec<f32> {
        let left = if long && !prev_long { self.bs0 / 2 } else { n / 2 };
        let right = if long && !next_long { self.bs0 / 2 } else { n / 2 };
        let slope_of = |len: usize| if len == self.bs0 / 2 { &self.slope0 } else { &self.slope1 };
        let mut w = vec![0f32; n];
        let ls = n / 4 - left / 2;
        let sl = slope_of(left);
        for i in 0..n / 2 {
            w[i] = if i < ls {
                0.0
            } else if i < ls + left {
                sl[i - ls]
            } else {
                1.0
            };
        }
        let rs = 3 * n / 4 - right / 2;
        let sr = slope_of(right);
        for i in n / 2..n {
            w[i] = if i < rs {
                1.0
            } else if i < rs + right {
                sr[right - 1 - (i - rs)]
            } else {
                0.0
            };
        }
        w
    }
}
