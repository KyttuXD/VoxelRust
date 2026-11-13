//! Proximity voice chat, the server's half.
//!
//! The server does all the thinking so that clients do almost nothing: it
//! knows where everyone is (from their `Move` messages), decides who is
//! close enough to hear a talker, works out how loud they are from the
//! distance, and forwards each 20 ms frame, untouched, with that loudness
//! in a byte. A client only decodes and multiplies. Nothing is recorded or
//! stored: a frame is passed on and forgotten.
//!
//! Everything here is plain functions and small structs, so it can be
//! tested without sockets; `server.rs` calls into it.

use alloc::string::String;
use alloc::vec::Vec;

/// How far a voice carries, in blocks, unless an operator changes it.
pub const DEFAULT_RANGE: f32 = 24.0;
/// The largest range an operator may set.
pub const MAX_RANGE: f32 = 128.0;
/// Within this share of the range a voice is at full loudness.
const NEAR: f32 = 0.2;
/// Quieter than this is not worth sending.
const MIN_GAIN: u8 = 4;

/// Frames a talker may send per second on average (a 20 ms frame is 50).
pub const FRAMES_PER_SECOND: f32 = 56.0;
/// Frames that may arrive in a burst (the network delivers in lumps).
pub const BURST: f32 = 12.0;

/// Distance between two positions.
pub fn distance(a: [f64; 3], b: [f64; 3]) -> f32 {
    let (dx, dy, dz) = (a[0] - b[0], a[1] - b[1], a[2] - b[2]);
    crate::math::sqrtf((dx * dx + dy * dy + dz * dz) as f32)
}

/// How loud a voice is at `dist` blocks when it carries `range` blocks:
/// 255 up close, then falling off smoothly (squared) to nothing at `range`.
/// 0 means it is not heard at all.
pub fn gain_at(dist: f32, range: f32) -> u8 {
    if !(range > 0.0) || !(dist >= 0.0) || dist >= range {
        return 0;
    }
    let near = range * NEAR;
    let t = if dist <= near { 0.0 } else { (dist - near) / (range - near) };
    let g = (1.0 - t) * (1.0 - t) * 255.0;
    let g = g as u32;
    if g < MIN_GAIN as u32 {
        0
    } else {
        g.min(255) as u8
    }
}

/// Who hears a talker at `from`: for each listener `(position, hears)`,
/// `Some(gain)` if they are in range and have voice on, else `None`.
/// The talker themself is left out by the caller.
pub fn audience(from: [f64; 3], listeners: &[Option<([f64; 3], bool)>], range: f32) -> Vec<Option<u8>> {
    listeners
        .iter()
        .map(|l| match l {
            Some((pos, true)) => Some(gain_at(distance(from, *pos), range)).filter(|&g| g > 0),
            _ => None,
        })
        .collect()
}

/// A talker's allowance of frames: a token bucket, so a modified client
/// cannot flood the server (or the players near it) with audio.
#[derive(Clone, Copy, Debug)]
pub struct Bucket {
    tokens: f32,
}

impl Default for Bucket {
    fn default() -> Bucket {
        Bucket { tokens: BURST }
    }
}

impl Bucket {
    /// Let `dt` seconds pass.
    pub fn refill(&mut self, dt: f32) {
        self.tokens = (self.tokens + dt.max(0.0) * FRAMES_PER_SECOND).min(BURST);
    }

    /// Take one frame's worth, if there is any.
    pub fn allow(&mut self) -> bool {
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Whether a frame is the right shape: a 3-byte header and a few samples,
/// at most one 20 ms frame's worth (and then some).
pub fn frame_ok(frame: &[u8]) -> bool {
    frame.len() >= 4 && frame.len() <= crate::proto::MAX_VOICE_FRAME && frame[2] <= 88
}

/// The server's voice settings: who is muted and how far a voice carries.
#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    pub range: f32,
    /// Lower-case names of players who may not talk.
    pub muted: Vec<String>,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules { range: DEFAULT_RANGE, muted: Vec::new() }
    }
}

impl Rules {
    pub fn is_muted(&self, name: &str) -> bool {
        self.muted.iter().any(|m| m.eq_ignore_ascii_case(name))
    }

    /// Mute (true) or unmute (false); returns whether anything changed.
    pub fn set_muted(&mut self, name: &str, mute: bool) -> bool {
        let had = self.is_muted(name);
        if mute && !had {
            self.muted.push(name.to_ascii_lowercase());
        } else if !mute && had {
            self.muted.retain(|m| !m.eq_ignore_ascii_case(name));
        }
        had != mute
    }

    /// Set the range from text (whole blocks, 2 to [`MAX_RANGE`]).
    pub fn set_range(&mut self, text: &str) -> bool {
        match text.trim().parse::<f32>() {
            Ok(r) if r >= 2.0 && r <= MAX_RANGE => {
                self.range = r;
                true
            }
            _ => false,
        }
    }

    /// Lines to keep with the permissions (see `Server::perm_text`).
    pub fn text(&self) -> String {
        let mut t = alloc::format!("voicerange {}\n", self.range);
        for m in &self.muted {
            t.push_str(&alloc::format!("mute {}\n", m));
        }
        t
    }

    /// Read one line written by [`Rules::text`]; false if it is not one.
    pub fn load_line(&mut self, line: &str) -> bool {
        match line.split_once(' ') {
            Some(("voicerange", r)) => {
                self.set_range(r);
            }
            Some(("mute", n)) => {
                self.set_muted(n.trim(), true);
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loudness_falls_off_with_distance() {
        assert_eq!(gain_at(0.0, 24.0), 255);
        assert_eq!(gain_at(4.0, 24.0), 255, "close enough to be at full volume");
        let mid = gain_at(14.0, 24.0);
        let far = gain_at(20.0, 24.0);
        assert!(mid < 255 && mid > far && far > 0, "{mid} {far}");
        assert_eq!(gain_at(24.0, 24.0), 0);
        assert_eq!(gain_at(500.0, 24.0), 0);
        // Never louder when further away.
        let mut last = 255;
        for d in 0..30 {
            let g = gain_at(d as f32, 24.0);
            assert!(g <= last);
            last = g;
        }
        // Nonsense in, silence out.
        assert_eq!(gain_at(f32::NAN, 24.0), 0);
        assert_eq!(gain_at(1.0, 0.0), 0);
        assert_eq!(gain_at(-1.0, 24.0), 0);
    }

    #[test]
    fn only_listeners_in_range_with_voice_on_hear() {
        let talker = [0.0, 64.0, 0.0];
        let listeners = [
            Some(([3.0, 64.0, 0.0], true)),   // close
            Some(([10.0, 64.0, 10.0], true)), // 14 blocks
            Some(([30.0, 64.0, 0.0], true)),  // too far
            Some(([1.0, 64.0, 0.0], false)),  // close but has voice off
            None,                             // never said where they are
            Some(([0.0, 164.0, 0.0], true)),  // far above
        ];
        let a = audience(talker, &listeners, 24.0);
        assert_eq!(a[0], Some(255));
        assert!(a[1].map_or(false, |g| g < 255 && g > 0));
        assert_eq!(&a[2..], &[None, None, None, None]);
        // A bigger range reaches further.
        assert!(audience(talker, &listeners, 48.0)[2].is_some());
    }

    #[test]
    fn the_rate_limit_allows_speech_and_stops_floods() {
        let mut b = Bucket::default();
        // Normal speech: one frame every 20 ms forever.
        for _ in 0..500 {
            b.refill(0.02);
            assert!(b.allow());
        }
        // A flood: a thousand frames at once.
        let allowed = (0..1000).filter(|_| b.allow()).count();
        assert!(allowed <= BURST as usize, "{allowed}");
        // And it recovers.
        b.refill(1.0);
        assert!(b.allow());
    }

    #[test]
    fn frames_must_be_the_right_shape() {
        assert!(frame_ok(&[0, 0, 10, 1, 2, 3]));
        assert!(!frame_ok(&[0, 0, 10]));
        assert!(!frame_ok(&[0, 0, 200, 1, 2, 3]), "step index out of range");
        assert!(!frame_ok(&alloc::vec![0; crate::proto::MAX_VOICE_FRAME + 1]));
    }

    #[test]
    fn muting_and_the_range_are_kept() {
        let mut r = Rules::default();
        assert!(!r.is_muted("Bob"));
        assert!(r.set_muted("Bob", true));
        assert!(!r.set_muted("bob", true), "already muted");
        assert!(r.is_muted("BOB"));
        assert!(r.set_range("40"));
        assert!(!r.set_range("0"));
        assert!(!r.set_range("many"));
        assert!(!r.set_range("100000"));
        assert_eq!(r.range, 40.0);
        let mut back = Rules::default();
        for line in r.text().lines() {
            assert!(back.load_line(line), "{line}");
        }
        assert_eq!(back, r);
        assert!(r.set_muted("bob", false));
        assert!(!r.is_muted("Bob"));
        assert!(!back.load_line("something else"));
    }
}
