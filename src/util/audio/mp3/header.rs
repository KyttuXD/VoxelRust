//! The four-byte frame header, and the side information that follows it.

use super::bits::BitReader;

/// What a frame header says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Header {
    /// 3 for MPEG-1, 2 for MPEG-2, 0 for MPEG-2.5 (the header's own numbers).
    pub version: u8,
    /// Row of the scale factor band tables: 0..=2 MPEG-1, 3..=5 MPEG-2, 6..=8 MPEG-2.5.
    pub sr_index: usize,
    pub rate: u32,
    /// 0 stereo, 1 joint stereo, 2 dual channel, 3 mono.
    pub mode: u8,
    pub mode_ext: u8,
    pub channels: usize,
    /// A two-byte checksum follows the header.
    pub crc: bool,
    /// The whole frame, header included.
    pub frame_len: usize,
}

const BITRATE_1: [u32; 15] = [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];
const BITRATE_2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
const RATES: [[u32; 3]; 3] = [[44100, 48000, 32000], [22050, 24000, 16000], [11025, 12000, 8000]];

impl Header {
    pub fn mpeg1(&self) -> bool {
        self.version == 3
    }

    /// Granules per frame: 2 in MPEG-1, 1 in the others.
    pub fn granules(&self) -> usize {
        if self.mpeg1() {
            2
        } else {
            1
        }
    }

    /// Bytes of side information.
    pub fn side_info_len(&self) -> usize {
        match (self.mpeg1(), self.channels) {
            (true, 1) => 17,
            (true, _) => 32,
            (false, 1) => 9,
            (false, _) => 17,
        }
    }

    /// Where the side information starts in the frame.
    pub fn side_info_at(&self) -> usize {
        if self.crc {
            6
        } else {
            4
        }
    }

    /// The same stream: what must not change from frame to frame.
    pub fn same_stream(&self, o: &Header) -> bool {
        self.version == o.version && self.rate == o.rate && self.channels == o.channels
    }
}

/// The header at `at`, if there is a layer III one: `None` for anything else, free-format
/// streams (bitrate 0) included, since their frame length cannot be known.
pub(super) fn parse(b: &[u8], at: usize) -> Option<Header> {
    let h = b.get(at..at.checked_add(4)?)?;
    let w = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
    if w >> 21 != 0x7FF {
        return None;
    }
    let version = ((w >> 19) & 3) as u8;
    if version == 1 || (w >> 17) & 3 != 1 {
        return None; // reserved version, or not layer III
    }
    let bitrate_i = ((w >> 12) & 15) as usize;
    let rate_i = ((w >> 10) & 3) as usize;
    if bitrate_i == 0 || bitrate_i == 15 || rate_i == 3 {
        return None;
    }
    let padding = ((w >> 9) & 1) as usize;
    let mode = ((w >> 6) & 3) as u8;
    let group = match version {
        3 => 0,
        2 => 1,
        _ => 2,
    };
    let rate = RATES[group][rate_i];
    let kbps = if version == 3 { BITRATE_1[bitrate_i] } else { BITRATE_2[bitrate_i] };
    let per_frame: usize = if version == 3 { 144 } else { 72 };
    let frame_len = per_frame * kbps as usize * 1000 / rate as usize + padding;
    let channels = if mode == 3 { 1 } else { 2 };
    let hd = Header {
        version,
        sr_index: group * 3 + rate_i,
        rate,
        mode,
        mode_ext: ((w >> 4) & 3) as u8,
        channels,
        crc: (w >> 16) & 1 == 0,
        frame_len,
    };
    if hd.frame_len < hd.side_info_at() + hd.side_info_len() {
        return None;
    }
    Some(hd)
}

/// What a granule of one channel says about its coded data.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Granule {
    /// Bits of scale factors and Huffman codes together.
    pub part23_len: usize,
    pub big_values: usize,
    pub global_gain: i32,
    pub scalefac_compress: u32,
    pub window_switching: bool,
    pub block_type: u8,
    pub mixed: bool,
    pub table_select: [u8; 3],
    pub subblock_gain: [i32; 3],
    pub region0_count: usize,
    pub region1_count: usize,
    pub preflag: bool,
    pub scalefac_scale: bool,
    pub count1_table_b: bool,
}

impl Granule {
    /// Three short windows (mixed ones included).
    pub fn is_short(&self) -> bool {
        self.window_switching && self.block_type == 2
    }
}

pub(super) struct SideInfo {
    /// Bytes back into the earlier frames where this frame's main data starts.
    pub main_data_begin: usize,
    /// Per channel, the four groups of scale factors that are shared with the first granule.
    pub scfsi: [[bool; 4]; 2],
    /// `[granule][channel]`
    pub gr: [[Granule; 2]; 2],
}

pub(super) fn read_side_info(frame: &[u8], h: &Header) -> Option<SideInfo> {
    let at = h.side_info_at();
    let mut br = BitReader::new(frame.get(at..at + h.side_info_len())?);
    let mpeg1 = h.mpeg1();
    let nch = h.channels;
    let main_data_begin = br.bits(if mpeg1 { 9 } else { 8 })? as usize;
    br.bits(match (mpeg1, nch) {
        (true, 1) => 5,
        (true, _) => 3,
        (false, 1) => 1,
        (false, _) => 2,
    })?;
    let mut si = SideInfo { main_data_begin, scfsi: [[false; 4]; 2], gr: [[Granule::default(); 2]; 2] };
    if mpeg1 {
        for c in 0..nch {
            for k in 0..4 {
                si.scfsi[c][k] = br.bit()? == 1;
            }
        }
    }
    for g in 0..h.granules() {
        for c in 0..nch {
            let mut d = Granule {
                part23_len: br.bits(12)? as usize,
                big_values: br.bits(9)? as usize,
                global_gain: br.bits(8)? as i32,
                scalefac_compress: br.bits(if mpeg1 { 4 } else { 9 })?,
                ..Granule::default()
            };
            d.window_switching = br.bit()? == 1;
            if d.window_switching {
                d.block_type = br.bits(2)? as u8;
                d.mixed = br.bit()? == 1;
                d.table_select[0] = br.bits(5)? as u8;
                d.table_select[1] = br.bits(5)? as u8;
                for k in 0..3 {
                    d.subblock_gain[k] = br.bits(3)? as i32;
                }
                if d.block_type == 0 {
                    return None; // window switching must say which window
                }
                // The region boundaries are fixed: see `huffman`.
                d.region0_count = if d.block_type == 2 { 8 } else { 7 };
                d.region1_count = 36;
            } else {
                for k in 0..3 {
                    d.table_select[k] = br.bits(5)? as u8;
                }
                d.region0_count = br.bits(4)? as usize;
                d.region1_count = br.bits(3)? as usize;
            }
            d.preflag = if mpeg1 { br.bit()? == 1 } else { false };
            d.scalefac_scale = br.bit()? == 1;
            d.count1_table_b = br.bit()? == 1;
            si.gr[g][c] = d;
        }
    }
    Some(si)
}
