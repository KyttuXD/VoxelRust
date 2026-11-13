//! Mappings (how channels are coupled and which floor and residue each uses)
//! and modes (a block size plus a mapping).

use super::bits::{ilog, Bits};
use alloc::vec::Vec;

pub(super) struct Submap {
    pub(super) floor: usize,
    pub(super) residue: usize,
}

pub(super) struct Mapping {
    /// (magnitude channel, angle channel), in the order they were coupled.
    pub(super) coupling: Vec<(usize, usize)>,
    /// The submap of each channel.
    pub(super) mux: Vec<usize>,
    pub(super) submaps: Vec<Submap>,
}

pub(super) struct Mode {
    pub(super) long: bool,
    pub(super) mapping: usize,
}

impl Mapping {
    pub(super) fn parse(b: &mut Bits, channels: usize, nfloors: usize, nresidues: usize) -> Option<Mapping> {
        if b.read(16) != 0 {
            return None;
        }
        let nsub = if b.bit() == 1 { b.read(4) as usize + 1 } else { 1 };
        let mut coupling = Vec::new();
        if b.bit() == 1 {
            let steps = b.read(8) + 1;
            let bits = ilog(channels as u32 - 1);
            for _ in 0..steps {
                let m = b.read(bits) as usize;
                let a = b.read(bits) as usize;
                if m == a || m >= channels || a >= channels {
                    return None;
                }
                coupling.push((m, a));
            }
        }
        if b.read(2) != 0 {
            return None;
        }
        let mut mux = Vec::new();
        for _ in 0..channels {
            let m = if nsub > 1 { b.read(4) as usize } else { 0 };
            if m >= nsub {
                return None;
            }
            mux.push(m);
        }
        let mut submaps = Vec::new();
        for _ in 0..nsub {
            b.read(8); // a time-domain setting nobody uses
            let floor = b.read(8) as usize;
            let residue = b.read(8) as usize;
            if floor >= nfloors || residue >= nresidues {
                return None;
            }
            submaps.push(Submap { floor, residue });
        }
        if b.eop {
            return None;
        }
        Some(Mapping { coupling, mux, submaps })
    }
}

impl Mode {
    pub(super) fn parse(b: &mut Bits, nmappings: usize) -> Option<Mode> {
        let long = b.bit() == 1;
        if b.read(16) != 0 || b.read(16) != 0 {
            return None;
        }
        let mapping = b.read(8) as usize;
        if mapping >= nmappings || b.eop {
            return None;
        }
        Some(Mode { long, mapping })
    }
}
