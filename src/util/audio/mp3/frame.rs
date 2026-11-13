//! Decoding one frame: the part that holds the decoder's memory between frames (the bit
//! reservoir, the overlap of the transforms, the filter bank).

use super::bits::BitReader;
use super::header::{read_side_info, Header};
use super::huffman::Huffman;
use super::imdct::Imdct;
use super::requant::Requant;
use super::scalefac::{self, Scalefacs};
use super::stereo;
use super::synth::Synth;
use super::tables::{SFB_LONG, SFB_SHORT};
use alloc::vec::Vec;

/// The most the bit reservoir can reach back.
const RESERVOIR: usize = 511;

pub(super) struct Decoder {
    huff: Huffman,
    requant: Requant,
    imdct: Imdct,
    synth: Synth,
    /// The main data of the last frames.
    reservoir: Vec<u8>,
    scratch: Vec<u8>,
    overlap: [[[f32; 18]; 32]; 2],
    filter: [[f32; 1024]; 2],
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder {
            huff: Huffman::new(),
            requant: Requant::new(),
            imdct: Imdct::new(),
            synth: Synth::new(),
            reservoir: Vec::new(),
            scratch: Vec::new(),
            overlap: [[[0.0; 18]; 32]; 2],
            filter: [[0.0; 1024]; 2],
        }
    }

    /// Decode a whole frame (`frame` is exactly `h.frame_len` bytes) and add its samples,
    /// interleaved, to `out`. `None` if the frame cannot be decoded (nothing is added then).
    pub fn frame(&mut self, h: &Header, frame: &[u8], out: &mut Vec<f32>) -> Option<()> {
        let si = read_side_info(frame, h)?;
        let main = frame.get(h.side_info_at() + h.side_info_len()..)?;

        // The main data starts `main_data_begin` bytes before this frame's own.
        let have = si.main_data_begin <= self.reservoir.len();
        let mut data = core::mem::take(&mut self.scratch);
        data.clear();
        if have {
            data.extend_from_slice(&self.reservoir[self.reservoir.len() - si.main_data_begin..]);
            data.extend_from_slice(main);
        }
        self.reservoir.extend_from_slice(main);
        if self.reservoir.len() > RESERVOIR {
            let cut = self.reservoir.len() - RESERVOIR;
            self.reservoir.drain(..cut);
        }
        if !have {
            self.scratch = data;
            return None;
        }

        let result = self.granules(h, &si, &data, out);
        self.scratch = data;
        result
    }

    fn granules(&mut self, h: &Header, si: &super::header::SideInfo, data: &[u8], out: &mut Vec<f32>) -> Option<()> {
        let nch = h.channels;
        let sfl = &SFB_LONG[h.sr_index];
        let sfs = &SFB_SHORT[h.sr_index];
        let uses_intensity = h.mode == 1 && h.mode_ext & 1 != 0;
        let mut br = BitReader::new(data);
        let mut first = [Scalefacs::zero(); 2];
        let mut mine: Vec<f32> = Vec::with_capacity(h.granules() * 576 * nch);

        for gran in 0..h.granules() {
            let mut xr = [[0.0f32; 576]; 2];
            let mut sf = [Scalefacs::zero(); 2];
            for c in 0..nch {
                let g = &si.gr[gran][c];
                let end = br.pos() + g.part23_len;
                let (s, pre) = scalefac::read(&mut br, g, h, gran, &si.scfsi[c], &first[c], uses_intensity && c == 1)?;
                if gran == 0 {
                    first[c] = s;
                }
                sf[c] = s;
                let mut is = [0i32; 576];
                self.huff.decode(&mut br, g, sfl, end, &mut is)?;
                self.requant.requant(&is, g, &s, pre, h, sfl, sfs, &mut xr[c]);
                br.set_pos(end);
            }
            if nch == 2 && h.mode == 1 {
                stereo::process(&mut xr, &si.gr[gran][1], &sf[1], h, sfl, sfs);
            }
            let mut pcm = [[0.0f32; 576]; 2];
            for c in 0..nch {
                let g = &si.gr[gran][c];
                let mixed = g.is_short() && g.mixed;
                // Subbands that are transformed as long blocks.
                let long_sb = if !g.is_short() {
                    32
                } else if mixed {
                    sfl[if h.mpeg1() { 8 } else { 6 }] as usize / 18
                } else {
                    0
                };
                if g.is_short() {
                    self.imdct.reorder(&mut xr[c], mixed, sfs);
                }
                self.imdct.antialias(&mut xr[c], long_sb);
                let mut bands = [[0.0f32; 18]; 32];
                self.imdct.transform(&xr[c], g, long_sb, &mut self.overlap[c], &mut bands);
                self.synth.granule(&mut self.filter[c], &bands, &mut pcm[c]);
            }
            for t in 0..576 {
                for c in 0..nch {
                    mine.push(pcm[c][t]);
                }
            }
        }
        out.extend_from_slice(&mine);
        Some(())
    }
}
