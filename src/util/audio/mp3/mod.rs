//! MP3: MPEG-1, MPEG-2 and MPEG-2.5 audio layer III, any bitrate, mono or stereo (joint stereo
//! with mid/side and intensity coding included). ID3 tags are skipped, the Xing/Info frame that
//! encoders put first is not played and, when it carries the LAME tag, the encoder's silence at
//! the start and end is cut off, as players do. Free-format streams (where the frame length is
//! not in the header) are refused.
//!
//! The decoder follows the standard (ISO 11172-3 and 13818-3) step by step, one file per step:
//!
//! - `header`: frame headers and side information
//! - `scalefac`: scale factors
//! - `huffman` (with the codes in `tables`): the quantised spectrum
//! - `requant`: the spectrum as numbers; `stereo`: mid/side and intensity
//! - `imdct`: block reordering, alias reduction and the inverse MDCT
//! - `synth`: the polyphase filter bank that makes samples
//! - `frame`: one frame, and what the decoder remembers between frames
//!
//! Nothing here may panic or loop for ever on damaged data: every read says `None` at the end
//! of the data, every table index is kept in range, and a frame that cannot be decoded is
//! replaced by silence.

mod bits;
mod frame;
mod header;
mod huffman;
mod imdct;
mod requant;
mod scalefac;
mod stereo;
mod synth;
mod tables;
#[cfg(test)]
mod tests;

use super::{Pcm, MAX_SECONDS};
use alloc::vec;
use alloc::vec::Vec;
use frame::Decoder;
use header::Header;

/// Does this look like an MP3 file: an ID3 tag, or the sync bits of a frame header?
pub(super) fn is_mp3(b: &[u8]) -> bool {
    b.starts_with(b"ID3") || (b.len() >= 2 && b[0] == 0xFF && b[1] & 0xE0 == 0xE0)
}

/// Where the audio starts: after the ID3v2 tags.
fn skip_id3(b: &[u8]) -> usize {
    let mut at = 0usize;
    while b.len() >= at + 10 && &b[at..at + 3] == b"ID3" {
        let s = &b[at + 6..at + 10];
        if s.iter().any(|&v| v & 0x80 != 0) {
            break;
        }
        let size = ((s[0] as usize) << 21) | ((s[1] as usize) << 14) | ((s[2] as usize) << 7) | s[3] as usize;
        let footer = if b[at + 5] & 0x10 != 0 { 10 } else { 0 };
        at += 10 + size + footer;
    }
    at
}

/// The first frame at or after `from` that is really one: it fits in the data and what follows
/// it is another frame of the same kind (or the end, or an ID3v1 tag).
fn find_frame(b: &[u8], from: usize) -> Option<(usize, Header)> {
    let mut i = from;
    while i + 4 <= b.len() {
        if b[i] == 0xFF {
            if let Some(h) = header::parse(b, i) {
                let next = i + h.frame_len;
                if next <= b.len() {
                    let ok = match header::parse(b, next) {
                        Some(h2) => h2.same_stream(&h),
                        None => b.len() - next < 4 || b[next..].starts_with(b"TAG") || b[next..].starts_with(b"ID3"),
                    };
                    if ok {
                        return Some((i, h));
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// Where the "Xing" or "Info" tag of a frame is, if it has one: right after the side
/// information (encoders put it there whether or not the header promises a checksum).
fn xing_at(frame: &[u8], h: &Header) -> Option<usize> {
    let si = 4 + h.side_info_len();
    [si, si + 2].into_iter().find(|&at| matches!(frame.get(at..at + 4), Some(t) if t == b"Xing" || t == b"Info"))
}

/// Is this the frame of Xing/Info/VBRI data that encoders put before the audio?
fn is_info_frame(frame: &[u8], h: &Header) -> bool {
    xing_at(frame, h).is_some() || frame.get(36..40).map_or(false, |t| t == b"VBRI")
}

/// The encoder's delay and padding, in samples, from the LAME tag in the Xing/Info frame.
fn lame_delays(frame: &[u8], h: &Header) -> Option<(usize, usize)> {
    let at = xing_at(frame, h)?;
    let f = frame.get(at + 4..at + 8)?;
    let flags = u32::from_be_bytes([f[0], f[1], f[2], f[3]]);
    let mut off = at + 8;
    for (bit, len) in [(1u32, 4usize), (2, 4), (4, 100), (8, 4)] {
        if flags & bit != 0 {
            off += len;
        }
    }
    let e = frame.get(off..off + 24)?;
    if !e[0].is_ascii_alphabetic() {
        return None; // no encoder name: no tag after all
    }
    let delay = ((e[21] as usize) << 4) | ((e[22] as usize) >> 4);
    let padding = (((e[22] as usize) & 15) << 8) | e[23] as usize;
    if (delay == 0 && padding == 0) || delay > 2880 || padding > 2880 {
        return None;
    }
    Some((delay, padding))
}

/// Decode an MP3 file. `None` if there is no layer III audio in it, or it is too long.
pub(super) fn mp3(b: &[u8]) -> Option<Pcm> {
    let mut pos = skip_id3(b);
    let mut stream: Option<Header> = None;
    let mut decoder: Option<Decoder> = None;
    let mut out: Vec<f32> = Vec::new();
    let mut decoded = 0usize;
    let mut trim: Option<(usize, usize)> = None;
    let mut frames_seen = 0usize;

    loop {
        // The next frame is normally right where the last one ended.
        let found = match stream {
            Some(s) => match header::parse(b, pos) {
                Some(h) if h.same_stream(&s) && pos + h.frame_len <= b.len() => Some((pos, h)),
                _ => find_frame(b, pos),
            },
            None => find_frame(b, pos),
        };
        let Some((at, h)) = found else { break };
        let s = *stream.get_or_insert(h);
        if !h.same_stream(&s) {
            pos = at + 1;
            continue;
        }
        pos = at + h.frame_len;
        frames_seen += 1;
        let frame = &b[at..pos];
        if is_info_frame(frame, &h) {
            if frames_seen == 1 {
                trim = lame_delays(frame, &h);
            }
            continue;
        }
        let dec = decoder.get_or_insert_with(Decoder::new);
        match dec.frame(&h, frame, &mut out) {
            Some(()) => decoded += 1,
            None => {
                // Keep the time right with silence, once there is sound to keep it with.
                if decoded > 0 {
                    out.extend(vec![0.0f32; h.granules() * 576 * h.channels]);
                }
            }
        }
        if out.len() > s.rate as usize * MAX_SECONDS as usize * s.channels + 2 * 1152 * s.channels {
            return None; // too long
        }
    }

    let s = stream?;
    if decoded == 0 {
        return None;
    }
    if let Some((delay, padding)) = trim {
        let ch = s.channels;
        let skip = (delay + 529).min(out.len() / ch);
        out.drain(..skip * ch);
        let n = out.len() / ch;
        let cut = padding.saturating_sub(529).min(n);
        out.truncate((n - cut) * ch);
    }
    if out.is_empty() {
        return None;
    }
    Some(Pcm { rate: s.rate, channels: s.channels, data: out })
}
