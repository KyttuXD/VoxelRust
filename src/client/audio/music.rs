//! Music for mods: one track at a time, streamed.
//!
//! The mixer has two lanes. A lane is a ring of 32768 stereo frames (about
//! 1.5 seconds) that the audio callback reads and the game thread fills a
//! few thousand frames a frame, decoding the file a block at a time
//! (`util/audio/music_feed.rs`). Only the compressed file and that ring are
//! in memory, whatever the length of the track, and the ring is only
//! allocated while a track plays. The second lane is for crossfades: the old
//! track fades out on its lane while the new one fades in on the other.
//!
//! Loudness is the track's own volume (`play_music`), times the mod's
//! `set_music_volume`, times the MUSIC VOLUME setting with the master volume,
//! times the fade. The game's own ambience ducks by 3 dB while music is
//! audible (see `ambient.rs`).

use super::*;
use crate::wav::fade::{Curve, Fade};
use crate::wav::music_feed::MusicFeed;
use crate::wav::music_source::{MusicSource, MAX_MUSIC_BYTES};
use alloc::string::String;
use alloc::vec;

/// Stereo frames a lane holds (a power of two).
pub(super) const RING: usize = 32768;

/// Frames the game thread tries to keep queued ahead of the callback.
const KEEP_AHEAD: usize = 16384;
/// Most frames made, and most blocks decoded, in one frame of the game.
const FILL_FRAMES: usize = 8192;
const FILL_BLOCKS: usize = 6;

/// Music is mixed at this share of full scale before the volumes (so the
/// default setting leaves room for effects).
const LEVEL: f32 = 0.8;

/// A crossfade is never shorter than this, so one track does not cut the other off.
pub const MIN_CROSSFADE: f32 = 0.25;

/// The mixer's side of a lane: what the audio callback reads.
pub(super) struct Lane {
    l: Vec<i16>,
    r: Vec<i16>,
    rd: usize,
    wr: usize,
    gain: f32,
    gain_t: f32,
}

impl Lane {
    pub(super) const fn empty() -> Lane {
        Lane { l: Vec::new(), r: Vec::new(), rd: 0, wr: 0, gain: 0.0, gain_t: 0.0 }
    }

    fn with_ring() -> Lane {
        Lane { l: vec![0; RING], r: vec![0; RING], rd: 0, wr: 0, gain: 0.0, gain_t: 0.0 }
    }

    /// A lane with these frames queued, at this loudness (for tests of the mixer).
    #[cfg(test)]
    pub(super) fn filled(frames: &[(i16, i16)], gain: f32) -> Lane {
        let mut lane = Lane::with_ring();
        for f in frames {
            lane.push(f.0, f.1);
        }
        lane.gain = gain;
        lane.gain_t = gain;
        lane
    }

    fn buffered(&self) -> usize {
        self.wr.wrapping_sub(self.rd)
    }

    fn space(&self) -> usize {
        if self.l.is_empty() {
            0
        } else {
            RING - self.buffered().min(RING)
        }
    }

    fn push(&mut self, l: i16, r: i16) {
        let i = self.wr & (RING - 1);
        self.l[i] = l;
        self.r[i] = r;
        self.wr = self.wr.wrapping_add(1);
    }

    /// Add the next frames to the sums (the audio callback). Runs dry
    /// quietly: what is not there is silence.
    pub(super) fn mix(&mut self, sl: &mut [i32], sr: &mut [i32]) {
        if self.l.is_empty() {
            return;
        }
        let dg = (self.gain_t - self.gain) / sl.len().max(1) as f32;
        for (l, r) in sl.iter_mut().zip(sr.iter_mut()) {
            if self.rd == self.wr {
                break;
            }
            self.gain += dg;
            let i = self.rd & (RING - 1);
            *l += (self.l[i] as f32 * self.gain) as i32;
            *r += (self.r[i] as f32 * self.gain) as i32;
            self.rd = self.rd.wrapping_add(1);
        }
        self.gain = self.gain_t;
    }
}

/// The game thread's side of a lane: the track and how it is faded.
struct Track {
    feed: MusicFeed,
    owner: String,
    name: String,
    /// Fades in at the start, and out when the track is stopped or replaced.
    fade: Fade,
    /// The volume the mod asked for with this track.
    vol: f32,
    /// On its way out (replaced or stopped): removed when the fade is done.
    leaving: bool,
}

pub(super) struct Deck {
    tracks: [Option<Track>; 2],
    /// The lane of the track that is meant to be playing (not one fading out).
    cur: Option<usize>,
    /// The mod's `set_music_volume`.
    user: f32,
    /// MUSIC VOLUME with the master volume, 0..1.
    bus: f32,
    /// The track ended by itself (or failed) since the game last asked.
    ended: bool,
    /// Frames made this frame, before they go to the mixer.
    stage: Vec<i16>,
}

impl Deck {
    pub(super) fn new() -> Deck {
        Deck { tracks: [None, None], cur: None, user: 1.0, bus: 0.0, ended: false, stage: Vec::new() }
    }

    /// Bytes held: compressed files, decoded blocks and the rings.
    pub(super) fn bytes(&self) -> usize {
        self.tracks.iter().flatten().map(|t| t.feed.bytes() + RING * 4).sum()
    }

    /// Whether any music is loud enough to hear.
    pub(super) fn audible(&self) -> bool {
        self.tracks.iter().flatten().any(|t| t.fade.value() * t.vol * self.user * self.bus > 0.02)
    }
}

/// How loud a lane should be: all the volumes together.
fn lane_gain(t: &Track, user: f32, bus: f32) -> f32 {
    t.fade.value() * t.vol * user * bus * LEVEL
}

fn clean_volume(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

impl Audio {
    /// Set the MUSIC VOLUME (0..=10; the master volume is multiplied in).
    pub fn set_music_volume_setting(&mut self, level: i32) {
        self.deck.bus = self.master * level.clamp(0, 10) as f32 / 10.0;
    }

    /// Whether a track is playing (or fading out).
    pub fn music_active(&self) -> bool {
        self.deck.tracks.iter().any(|t| t.is_some())
    }

    /// Start a track of `owner` (a mod): the file `music/<name>` or
    /// `sounds/<name>` of its folder, any of `.ogg`, `.mp3`, `.wav`. A track
    /// already playing fades out as this one fades in (a crossfade of
    /// `fade_in` seconds, at least a quarter of one). The same name again
    /// only changes the volume. Returns false if the file is not there or not
    /// music this can read.
    pub fn play_music(&mut self, owner: &str, name: &str, volume: f32, fade_in: f32, looping: bool) -> bool {
        let vol = clean_volume(volume);
        if let Some(c) = self.deck.cur {
            if let Some(t) = self.deck.tracks[c].as_mut() {
                if !t.leaving && t.owner == owner && t.name == name {
                    t.vol = vol;
                    t.feed.set_looping(looping);
                    return true;
                }
            }
        }
        if self.device == 0 {
            return false;
        }
        let Some(bytes) = self.music_bytes_of(owner, name) else {
            self.note_missing(owner, name, "music");
            return false;
        };
        let Some(src) = MusicSource::open(bytes) else {
            crate::logf!("voxelrust: music `{}` of mod `{}` is not a track this reads (Ogg Vorbis, MP3 or WAV, up to {} MB)", name, owner, MAX_MUSIC_BYTES >> 20);
            return false;
        };
        let replacing = self.deck.cur.is_some();
        let fade_in = if fade_in.is_finite() { fade_in.clamp(0.0, 60.0) } else { 0.0 };
        let secs = if replacing { fade_in.max(MIN_CROSSFADE) } else { fade_in };
        // The lane to use: not the one that plays now; if the other still
        // holds a track on its way out, that one is cut.
        let lane = match self.deck.cur {
            Some(c) => 1 - c,
            None => {
                if self.deck.tracks[0].is_none() {
                    0
                } else {
                    1
                }
            }
        };
        if let Some(c) = self.deck.cur {
            if let Some(t) = self.deck.tracks[c].as_mut() {
                t.fade.retarget(0.0, secs, Curve::Equal);
                t.leaving = true;
            }
        }
        let fade = if secs > 0.0 { Fade::new(0.0, 1.0, secs, Curve::Equal) } else { Fade::hold(1.0) };
        let track = Track { feed: MusicFeed::new(src, looping), owner: String::from(owner), name: String::from(name), fade, vol, leaving: false };
        // The ring is made before the audio lock is taken.
        let fresh = Lane::with_ring();
        let api = sdl();
        let old = unsafe {
            (api.SDL_LockAudioDevice)(self.device);
            let old = core::mem::replace(&mut (*self.mixer).music[lane], fresh);
            (api.SDL_UnlockAudioDevice)(self.device);
            old
        };
        drop(old);
        self.deck.tracks[lane] = Some(track);
        self.deck.cur = Some(lane);
        true
    }

    /// Fade the music out over `secs` (0: at once, with a few milliseconds
    /// so it does not click).
    pub fn stop_music(&mut self, secs: f32) {
        let secs = if secs.is_finite() { secs.clamp(0.0, 60.0) } else { 0.0 };
        for t in self.deck.tracks.iter_mut().flatten() {
            t.fade.retarget(0.0, secs.max(0.03), Curve::Smooth);
            t.leaving = true;
        }
        self.deck.cur = None;
    }

    /// The mod's own music volume, 0..1.
    pub fn set_music_volume(&mut self, v: f32) {
        self.deck.user = clean_volume(v);
    }

    /// Whether the music ended by itself (or could not play) since this was last asked.
    pub fn take_music_ended(&mut self) -> bool {
        core::mem::take(&mut self.deck.ended)
    }

    /// Stop all music at once and let go of its memory (leaving a world).
    pub(super) fn silence_music(&mut self) {
        self.deck.tracks = [None, None];
        self.deck.cur = None;
        self.deck.ended = false;
        self.deck.user = 1.0;
        // (The lanes are cleared with the rest of the mixer by `silence`.)
    }

    /// Keep the lanes fed and their loudness moving; once a frame.
    pub(super) fn music_tick(&mut self, dt: f32) {
        if self.device == 0 || self.deck.tracks.iter().all(|t| t.is_none()) {
            return;
        }
        let api = sdl();
        let (user, bus) = (self.deck.user, self.deck.bus);
        for i in 0..2 {
            let Some(t) = self.deck.tracks[i].as_mut() else { continue };
            t.fade.tick(dt);
            let gain = lane_gain(t, user, bus);
            // How much room the ring has, and set the loudness.
            let (space, buffered) = unsafe {
                (api.SDL_LockAudioDevice)(self.device);
                let lane = &mut (*self.mixer).music[i];
                lane.gain_t = gain;
                let r = (lane.space(), lane.buffered());
                (api.SDL_UnlockAudioDevice)(self.device);
                r
            };
            // Make what is missing, off the audio lock.
            let want = if buffered < KEEP_AHEAD { space.min(FILL_FRAMES) } else { 0 };
            let mut stage = core::mem::take(&mut self.deck.stage);
            stage.clear();
            if want > 0 {
                t.feed.fill(want, FILL_BLOCKS, &mut |l, r| {
                    stage.push(l);
                    stage.push(r);
                });
            }
            let pushed = !stage.is_empty();
            if pushed {
                unsafe {
                    (api.SDL_LockAudioDevice)(self.device);
                    let lane = &mut (*self.mixer).music[i];
                    for f in stage.chunks_exact(2) {
                        if lane.space() == 0 {
                            break;
                        }
                        lane.push(f[0], f[1]);
                    }
                    (api.SDL_UnlockAudioDevice)(self.device);
                }
            }
            self.deck.stage = stage;
            // Done: faded out, or played to its end and heard to its end.
            let finished = t.feed.finished() && !pushed && space == RING && buffered == 0;
            let faded = t.leaving && t.fade.done();
            if faded || finished {
                if finished && !t.leaving {
                    self.deck.ended = true;
                }
                let gone = self.deck.tracks[i].take();
                if self.deck.cur == Some(i) {
                    self.deck.cur = None;
                }
                let old = unsafe {
                    (api.SDL_LockAudioDevice)(self.device);
                    let old = core::mem::replace(&mut (*self.mixer).music[i], Lane::empty());
                    (api.SDL_UnlockAudioDevice)(self.device);
                    old
                };
                drop(old);
                drop(gone);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lane_plays_what_was_pushed_in_order_then_falls_silent() {
        let mut lane = Lane::with_ring();
        for i in 0..100 {
            lane.push(i as i16 * 10, -(i as i16) * 10);
        }
        lane.gain = 1.0;
        lane.gain_t = 1.0;
        let (mut l, mut r) = ([0i32; 64], [0i32; 64]);
        lane.mix(&mut l, &mut r);
        assert_eq!((l[0], l[5], r[5]), (0, 50, -50));
        let (mut l2, mut r2) = ([0i32; 64], [0i32; 64]);
        lane.mix(&mut l2, &mut r2);
        assert_eq!(l2[0], 640);
        assert_eq!(l2[35], 990);
        assert!(l2[36..].iter().all(|&v| v == 0), "after the end: silence");
        assert_eq!(lane.buffered(), 0);
    }

    #[test]
    fn the_ring_wraps_and_never_holds_more_than_it_has_room_for() {
        let mut lane = Lane::with_ring();
        assert_eq!(lane.space(), RING);
        for i in 0..RING {
            lane.push(i as i16, 0);
        }
        assert_eq!(lane.space(), 0);
        lane.gain = 1.0;
        lane.gain_t = 1.0;
        let (mut l, mut r) = ([0i32; 100], [0i32; 100]);
        lane.mix(&mut l, &mut r);
        assert_eq!(lane.space(), 100);
        for _ in 0..100 {
            lane.push(7, 7);
        }
        assert_eq!(lane.buffered(), RING);
        // An empty lane costs nothing and makes nothing.
        let mut e = Lane::empty();
        let (mut a, mut b) = ([5i32; 8], [5i32; 8]);
        e.mix(&mut a, &mut b);
        assert_eq!(a, [5; 8]);
        assert_eq!(e.space(), 0);
    }

    #[test]
    fn the_loudness_glides_over_a_chunk_instead_of_jumping() {
        let mut lane = Lane::with_ring();
        for _ in 0..200 {
            lane.push(10000, 10000);
        }
        lane.gain = 0.0;
        lane.gain_t = 1.0;
        let (mut l, mut r) = ([0i32; 100], [0i32; 100]);
        lane.mix(&mut l, &mut r);
        assert!(l[0] < 200 && l[99] > 9800, "{} {}", l[0], l[99]);
        assert!(l.windows(2).all(|w| w[1] >= w[0]));
        assert_eq!(lane.gain, 1.0);
    }

    #[test]
    fn the_volumes_multiply() {
        let feed = MusicFeed::new(MusicSource::open(crate::wav::make_wav(22_050, 1, 16, &[0u8; 400])).unwrap(), false);
        let t = Track { feed, owner: String::new(), name: String::new(), fade: Fade::hold(0.5), vol: 0.5, leaving: false };
        let g = lane_gain(&t, 0.5, 0.4);
        assert!((g - 0.5 * 0.5 * 0.5 * 0.4 * LEVEL).abs() < 1e-6, "{g}");
        assert_eq!(clean_volume(f32::NAN), 1.0);
        assert_eq!(clean_volume(7.0), 1.0);
        assert_eq!(clean_volume(-1.0), 0.0);
    }
}
