//! Big skins on the wire: announcing one, cutting it into pieces, putting the
//! pieces back together, and the limits that go with it (see docs/PLAYING.md,
//! "Skins", and docs/IMAGES.md).
//!
//! A skin that fits in one plain [`Msg::Skin`] (a 64 x 64 one always does) is
//! sent as that. A bigger one is announced with [`Msg::SkinBegin`] (its size
//! in bytes and its side in pixels) and followed at once by [`Msg::SkinPart`]s
//! of up to [`MAX_SKIN_PART`] bytes, like the pieces of a content pack. The
//! receiver checks, before it keeps a single byte:
//!
//! - the declared size is at most [`MAX_SKIN`] (1 MiB) and the declared side
//!   a skin's (a multiple of 64 up to 4096);
//! - the first piece starts with a PNG header (checksum right) that says the
//!   same side;
//! - the pieces never add up to more than the declared size;
//! - one transfer at a time per player, and one that stops for
//!   [`STALE`] seconds is dropped.
//!
//! Everything here is plain functions and structs, tested without sockets.

use crate::proto::{Msg, MAX_SKIN, MAX_SKIN_PART};
use crate::skin_grid;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Seconds a player must wait between two skin changes.
pub const COOLDOWN: f32 = 5.0;
/// Seconds without a piece after which a transfer is given up.
pub const STALE: f32 = 20.0;
/// The most transfers a client keeps open at once (one per other player, but
/// never more than this), and the most declared bytes in all of them.
pub const MAX_OPEN: usize = 8;
pub const MAX_PENDING_BYTES: usize = 4 << 20;
/// The smallest possible PNG that is a skin: a header and a little more.
const MIN_PNG: usize = 40;

/// Why a skin or a piece of one was not taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refuse {
    /// Declared bigger than [`MAX_SKIN`] (or smaller than any picture).
    Size,
    /// Declared side that is not a skin's.
    Side,
    /// The first piece is not a PNG header, or says another side.
    Header,
    /// More bytes than were announced.
    Overrun,
    /// The last piece came and the skin is not whole.
    Short,
    /// Another transfer is going on.
    Busy,
    /// Too soon after the last skin.
    Cooldown,
    /// A piece with no transfer to belong to.
    Stray,
}

/// The side of a PNG that is a skin picture (square, a valid side, not
/// interlaced), from its header alone.
pub fn side_of_png(png: &[u8]) -> Option<usize> {
    let h = crate::image::header::header(png)?;
    let (w, hh) = (h.w as usize, h.h as usize);
    if h.interlaced || w != hh || !skin_grid::valid_side(w) {
        return None;
    }
    Some(w)
}

/// How long a player must still wait before their next skin.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cooldown {
    left: f32,
}

impl Cooldown {
    pub fn tick(&mut self, dt: f32) {
        // (A NaN or negative step changes nothing.)
        if dt > 0.0 && dt < 3600.0 {
            self.left = (self.left - dt).max(0.0);
        }
    }

    /// Use the allowance: false (and nothing used) if it is too soon.
    pub fn try_use(&mut self) -> bool {
        if self.left > 0.0 {
            return false;
        }
        self.left = COOLDOWN;
        true
    }

    pub fn left(&self) -> f32 {
        self.left
    }
}

/// A skin being received.
#[derive(Clone, Debug)]
pub struct Incoming {
    pub side: usize,
    pub size: usize,
    pub slim: bool,
    buf: Vec<u8>,
    idle: f32,
}

impl Incoming {
    /// Take the announcement: its size and side must be possible.
    pub fn begin(size: u32, side: u16, slim: bool) -> Result<Incoming, Refuse> {
        let size = size as usize;
        if size < MIN_PNG || size > MAX_SKIN {
            return Err(Refuse::Size);
        }
        if !skin_grid::valid_side(side as usize) {
            return Err(Refuse::Side);
        }
        Ok(Incoming { side: side as usize, size, slim, buf: Vec::new(), idle: 0.0 })
    }

    /// Bytes received so far.
    pub fn have(&self) -> usize {
        self.buf.len()
    }

    /// The next piece. `Ok(Some(png))` when the skin is whole.
    pub fn part(&mut self, data: &[u8], last: bool) -> Result<Option<Vec<u8>>, Refuse> {
        if self.buf.is_empty() {
            // The first piece must show what it is before the rest is kept.
            if side_of_png(data) != Some(self.side) {
                return Err(Refuse::Header);
            }
        }
        if self.buf.len() + data.len() > self.size {
            return Err(Refuse::Overrun);
        }
        self.buf.extend_from_slice(data);
        self.idle = 0.0;
        if self.buf.len() == self.size {
            return Ok(Some(core::mem::take(&mut self.buf)));
        }
        if last {
            return Err(Refuse::Short);
        }
        Ok(None)
    }

    /// Let `dt` seconds pass; true when the transfer has been quiet for too long.
    pub fn tick(&mut self, dt: f32) -> bool {
        if dt > 0.0 && dt < 3600.0 {
            self.idle += dt;
        }
        self.idle > STALE
    }
}

/// The messages that carry a skin: a [`Msg::SkinBegin`] and its pieces.
pub fn messages(id: u16, png: &[u8], side: usize, slim: bool) -> Vec<Msg> {
    let mut out = Vec::with_capacity(png.len() / MAX_SKIN_PART + 2);
    out.push(Msg::SkinBegin { id, size: png.len() as u32, side: side as u16, slim });
    let pieces: Vec<&[u8]> = png.chunks(MAX_SKIN_PART).collect();
    let n = pieces.len();
    for (k, p) in pieces.into_iter().enumerate() {
        out.push(Msg::SkinPart { id, last: k + 1 == n, data: p.to_vec() });
    }
    out
}

/// The same, already encoded and joined, for sending in one go.
pub fn wire(id: u16, png: &[u8], side: usize, slim: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(png.len() + 64);
    for m in messages(id, png, side, slim) {
        out.extend_from_slice(&m.encode());
    }
    out
}

/// The 64 x 64 picture of a skin `side` wide, as a PNG, for those who know
/// only that size (older clients); None if `png` is not a skin of that side.
pub fn legacy_version(png: &[u8], side: usize) -> Option<Vec<u8>> {
    if side == 64 {
        return Some(png.to_vec());
    }
    let img = crate::png::decode(png)?;
    if img.w != side || img.h != side {
        return None;
    }
    let small = crate::image::resize::downscale_square(&img.rgba, side, 64);
    Some(crate::png::encode(64, 64, &small))
}

/// The skins a client is receiving, by the player they are from.
#[derive(Default)]
pub struct Transfers {
    open: BTreeMap<u16, Incoming>,
}

impl Transfers {
    /// A player's announcement. A new one from the same player replaces their old one.
    pub fn begin(&mut self, id: u16, size: u32, side: u16, slim: bool) -> Result<(), Refuse> {
        self.open.remove(&id);
        let inc = Incoming::begin(size, side, slim)?;
        let pending: usize = self.open.values().map(|i| i.size).sum();
        if self.open.len() >= MAX_OPEN || pending + inc.size > MAX_PENDING_BYTES {
            return Err(Refuse::Busy);
        }
        self.open.insert(id, inc);
        Ok(())
    }

    /// A piece. A transfer that goes wrong is forgotten.
    pub fn part(&mut self, id: u16, data: &[u8], last: bool) -> Result<Option<(Vec<u8>, bool)>, Refuse> {
        let Some(inc) = self.open.get_mut(&id) else { return Err(Refuse::Stray) };
        let slim = inc.slim;
        match inc.part(data, last) {
            Ok(Some(png)) => {
                self.open.remove(&id);
                Ok(Some((png, slim)))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                self.open.remove(&id);
                Err(e)
            }
        }
    }

    /// Let time pass: transfers that went quiet are dropped.
    pub fn tick(&mut self, dt: f32) {
        self.open.retain(|_, i| !i.tick(dt));
    }

    /// A player left: their transfer goes too.
    pub fn forget(&mut self, id: u16) {
        self.open.remove(&id);
    }

    pub fn len(&self) -> usize {
        self.open.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A skin of `side` pixels as a PNG that is bigger than one piece when `noisy`.
    fn skin(side: usize, noisy: bool) -> Vec<u8> {
        let mut s = 99u32;
        let px: Vec<u8> = (0..side * side * 4)
            .map(|i| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                if i % 4 == 3 || !noisy {
                    255
                } else {
                    (s >> 24) as u8
                }
            })
            .collect();
        crate::png::encode(side, side, &px)
    }

    #[test]
    fn a_skin_is_cut_and_put_together_again() {
        let png = skin(128, true);
        assert!(png.len() > MAX_SKIN_PART, "the test skin must need more than one piece: {}", png.len());
        let msgs = messages(7, &png, 128, true);
        assert!(msgs.len() >= 3);
        let Msg::SkinBegin { id, size, side, slim } = msgs[0].clone() else { panic!() };
        assert_eq!((id, size as usize, side, slim), (7, png.len(), 128, true));
        let mut inc = Incoming::begin(size, side, slim).unwrap();
        let mut got = None;
        for (k, m) in msgs[1..].iter().enumerate() {
            let Msg::SkinPart { last, data, .. } = m else { panic!() };
            assert!(data.len() <= MAX_SKIN_PART);
            assert_eq!(*last, k + 2 == msgs.len());
            got = inc.part(data, *last).unwrap();
        }
        assert_eq!(got.as_deref(), Some(&png[..]));
        // Every piece is a message the other side takes.
        let bytes = wire(7, &png, 128, true);
        let mut inbox = crate::proto::Inbox::default();
        inbox.push(&bytes);
        let mut n = 0;
        while let crate::proto::Next::Msg(_) = inbox.next() {
            n += 1;
        }
        assert_eq!(n, msgs.len());
    }

    #[test]
    fn announcements_that_cannot_be_skins_are_refused() {
        assert_eq!(Incoming::begin(MAX_SKIN as u32 + 1, 128, false).unwrap_err(), Refuse::Size);
        assert_eq!(Incoming::begin(3, 128, false).unwrap_err(), Refuse::Size);
        assert_eq!(Incoming::begin(0, 128, false).unwrap_err(), Refuse::Size);
        assert_eq!(Incoming::begin(5000, 100, false).unwrap_err(), Refuse::Side);
        assert_eq!(Incoming::begin(5000, 8192, false).unwrap_err(), Refuse::Side);
        assert_eq!(Incoming::begin(5000, 0, false).unwrap_err(), Refuse::Side);
        assert!(Incoming::begin(MAX_SKIN as u32, 4096, false).is_ok());
    }

    #[test]
    fn the_first_piece_must_be_a_png_of_the_announced_side() {
        let png = skin(128, false);
        let mut inc = Incoming::begin(png.len() as u32, 256, false).unwrap();
        assert_eq!(inc.part(&png, true).unwrap_err(), Refuse::Header);
        let mut inc = Incoming::begin(png.len() as u32, 128, false).unwrap();
        assert_eq!(inc.part(b"definitely not a picture, but long enough to look at", false).unwrap_err(), Refuse::Header);
        // A header that says 4096 x 4096 when 128 was announced.
        let mut bomb = png.clone();
        bomb[16..20].copy_from_slice(&4096u32.to_be_bytes());
        bomb[20..24].copy_from_slice(&4096u32.to_be_bytes());
        let mut inc = Incoming::begin(bomb.len() as u32, 128, false).unwrap();
        assert!(inc.part(&bomb, true).is_err(), "a header whose checksum does not fit");
        // A small first piece that is not even a whole header.
        let mut inc = Incoming::begin(png.len() as u32, 128, false).unwrap();
        assert_eq!(inc.part(&png[..20], false).unwrap_err(), Refuse::Header);
    }

    #[test]
    fn too_many_bytes_or_too_few_end_the_transfer() {
        let png = skin(128, false);
        let mut inc = Incoming::begin(png.len() as u32 - 5, 128, false).unwrap();
        assert_eq!(inc.part(&png, true).unwrap_err(), Refuse::Overrun);
        let mut inc = Incoming::begin(png.len() as u32 + 5, 128, false).unwrap();
        assert_eq!(inc.part(&png, true).unwrap_err(), Refuse::Short);
        // Pieces that never end are stopped by the size, whatever they claim.
        let first = skin(64, false);
        let mut inc = Incoming::begin(first.len() as u32 + 50, 64, false).unwrap();
        assert_eq!(inc.part(&first, false).map(|x| x.is_none()), Ok(true));
        assert_eq!(inc.part(&[0u8; 100], false).unwrap_err(), Refuse::Overrun);
    }

    #[test]
    fn a_quiet_transfer_is_dropped_and_progress_keeps_it_alive() {
        let png = skin(128, true);
        let mut t = Transfers::default();
        t.begin(3, png.len() as u32, 128, false).unwrap();
        assert!(t.part(3, &png[..MAX_SKIN_PART], false).unwrap().is_none());
        t.tick(15.0);
        assert_eq!(t.len(), 1);
        // A piece is progress: the clock starts again.
        assert!(t.part(3, &png[MAX_SKIN_PART..MAX_SKIN_PART + 10], false).unwrap().is_none());
        t.tick(15.0);
        assert_eq!(t.len(), 1);
        t.tick(10.0);
        assert_eq!(t.len(), 0);
        assert_eq!(t.part(3, &[1], false).unwrap_err(), Refuse::Stray);
    }

    #[test]
    fn a_client_keeps_few_transfers_and_few_bytes() {
        let mut t = Transfers::default();
        for id in 0..MAX_OPEN as u16 {
            t.begin(id, 100_000, 128, false).unwrap();
        }
        assert_eq!(t.begin(99, 100_000, 128, false), Err(Refuse::Busy));
        // The same player starting over replaces their own, and is not counted twice.
        assert!(t.begin(0, 100_000, 128, false).is_ok());
        assert_eq!(t.len(), MAX_OPEN);
        t.forget(1);
        assert!(t.begin(99, 100_000, 128, false).is_ok());
        // Declared bytes are limited too.
        let mut t = Transfers::default();
        for id in 0..4u16 {
            t.begin(id, MAX_SKIN as u32, 4096, false).unwrap();
        }
        assert_eq!(t.begin(9, 1000, 128, false), Err(Refuse::Busy));
    }

    #[test]
    fn a_whole_skin_through_the_client_side_table() {
        let png = skin(192, true);
        let mut t = Transfers::default();
        let msgs = messages(5, &png, 192, true);
        let mut out = None;
        for m in msgs {
            match m {
                Msg::SkinBegin { id, size, side, slim } => t.begin(id, size, side, slim).unwrap(),
                Msg::SkinPart { id, last, data } => out = t.part(id, &data, last).unwrap(),
                _ => unreachable!(),
            }
        }
        assert_eq!(out, Some((png, true)));
        assert_eq!(t.len(), 0);
    }

    #[test]
    fn the_cooldown_allows_one_skin_in_five_seconds() {
        let mut c = Cooldown::default();
        assert!(c.try_use());
        assert!(!c.try_use());
        c.tick(4.9);
        assert!(!c.try_use());
        c.tick(0.2);
        assert!(c.try_use());
        c.tick(f32::NAN);
        c.tick(-3.0);
        assert!(c.left() > 4.9, "nonsense steps change nothing");
    }

    #[test]
    fn older_clients_get_the_64_by_64_version() {
        let png = skin(256, true);
        let small = legacy_version(&png, 256).unwrap();
        assert_eq!(side_of_png(&small), Some(64));
        assert!(small.len() <= crate::proto::MAX_SKIN_MSG, "{}", small.len());
        // A 64 skin is its own legacy version; a lie about the side is refused.
        let s64 = skin(64, true);
        assert_eq!(legacy_version(&s64, 64).unwrap(), s64);
        assert!(legacy_version(&png, 128).is_none());
        assert!(legacy_version(b"nonsense", 128).is_none());
    }
}
