//! What the server keeps of one player's skin, and what it sends of it to
//! whom (see `net/skinxfer.rs` for the transfer itself).
//!
//! A player sends either a plain [`Msg::Skin`] or a [`Msg::SkinBegin`] with
//! its pieces. A skin is accepted only if its header says a size a skin may
//! have (and, for a big one, the whole picture decodes at that size), no more
//! than once in [`COOLDOWN`] seconds, one transfer at a time. The server keeps
//! two versions: the one the player sent (`hd`, only if it is bigger than
//! 64 x 64) and a 64 x 64 one made from it, which is what clients of protocol
//! 13 are sent. Clients of protocol 14 and later get the original.

use crate::net::skinxfer::{self, Cooldown, Incoming};
use crate::proto::{Msg, MAX_SKIN_MSG};
use alloc::vec::Vec;

/// The first protocol that knows big skins in pieces.
pub const HD_PROTOCOL: u16 = 14;

/// A big skin waiting to be unpacked off the tick (see `net/skinwork.rs`).
pub struct Pending {
    /// Which skin this is: a newer one from the same player makes it stale.
    pub ticket: u32,
    pub png: Vec<u8>,
    pub side: usize,
    pub slim: bool,
}

#[derive(Default)]
pub struct SkinSlot {
    /// The 64 x 64 skin (empty until the player sends one).
    legacy: Vec<u8>,
    /// The skin as sent when it is bigger than 64 x 64, else empty.
    hd: Vec<u8>,
    side: usize,
    slim: bool,
    incoming: Option<Incoming>,
    cool: Cooldown,
    /// Big skins are not unpacked in `on_whole` / `on_part` but handed out by
    /// [`SkinSlot::take_job`] (the server's tick must not stop for a decode).
    defer: bool,
    /// The number of the last big skin that came, and the one waiting to be taken.
    ticket: u32,
    job: Option<Pending>,
    /// A skin or piece of this player's was refused or a transfer dropped since
    /// the server last asked (see [`SkinSlot::take_refused`]); the server then
    /// tells the player (protocol 14 and later) so they can send it again.
    refused: bool,
}

/// Whether a client speaking `proto` is told when its skin is refused (only
/// clients of protocol 14 and later know the message).
pub fn sends_refusal(proto: u16) -> bool {
    proto >= HD_PROTOCOL
}

impl SkinSlot {
    /// A slot whose big skins are unpacked by a worker.
    pub fn deferred() -> SkinSlot {
        SkinSlot { defer: true, ..Default::default() }
    }

    /// The big skin that wants unpacking, if one came.
    pub fn take_job(&mut self) -> Option<Pending> {
        self.job.take()
    }

    /// Whether a refusal is owed since the last call (and clears it).
    pub fn take_refused(&mut self) -> bool {
        core::mem::take(&mut self.refused)
    }

    /// Note a refusal from outside (a job the worker could not take, a piece
    /// dropped for going too fast).
    pub fn refuse_now(&mut self) {
        self.refused = true;
    }

    /// Drop the transfer in progress (if any) and note the refusal.
    pub fn abort(&mut self) {
        self.incoming = None;
        self.refused = true;
    }

    fn refuse(&mut self) -> bool {
        self.refused = true;
        false
    }

    /// The worker is done with `p`: its 64 x 64 version, or `None` if the
    /// picture was not what its header said. True if the skin was taken (to
    /// be passed on); a skin that a newer one has replaced is ignored.
    pub fn finish(&mut self, p: Pending, legacy: Option<Vec<u8>>) -> bool {
        let Some(legacy) = legacy else { return self.refuse() };
        if p.ticket != self.ticket {
            return false;
        }
        self.legacy = legacy;
        self.hd = p.png;
        self.side = p.side;
        self.slim = p.slim;
        true
    }

    pub fn has_skin(&self) -> bool {
        !self.legacy.is_empty()
    }

    /// Side of the skin kept (0 for none).
    pub fn side(&self) -> usize {
        if self.legacy.is_empty() {
            0
        } else {
            self.side
        }
    }

    pub fn slim(&self) -> bool {
        self.slim
    }

    /// Let `dt` seconds pass.
    pub fn tick(&mut self, dt: f32) {
        self.cool.tick(dt);
        if let Some(i) = self.incoming.as_mut() {
            if i.tick(dt) {
                self.incoming = None;
                self.refused = true;
            }
        }
    }

    /// A plain [`Msg::Skin`]. True if it was taken (to be passed on); a
    /// refused one is noted for [`SkinSlot::take_refused`].
    pub fn on_whole(&mut self, png: Vec<u8>, slim: bool) -> bool {
        if png.len() > MAX_SKIN_MSG || self.incoming.is_some() {
            return self.refuse();
        }
        let Some(side) = skinxfer::side_of_png(&png) else { return self.refuse() };
        if !self.cool.try_use() {
            return self.refuse();
        }
        self.keep(png, side, slim)
    }

    /// A [`Msg::SkinBegin`]. True if the pieces are welcome now.
    pub fn on_begin(&mut self, size: u32, side: u16, slim: bool) -> bool {
        if self.incoming.is_some() {
            return self.refuse();
        }
        let Ok(inc) = Incoming::begin(size, side, slim) else { return self.refuse() };
        if !self.cool.try_use() {
            return self.refuse();
        }
        self.incoming = Some(inc);
        true
    }

    /// A [`Msg::SkinPart`]. True if it finished a skin that was taken (to be
    /// passed on). A transfer that goes wrong is dropped and noted.
    pub fn on_part(&mut self, data: &[u8], last: bool) -> bool {
        if self.incoming.is_none() {
            return self.refuse();
        }
        let Some(inc) = self.incoming.as_mut() else { return false };
        match inc.part(data, last) {
            Ok(Some(png)) => {
                let (side, slim) = (inc.side, inc.slim);
                self.incoming = None;
                self.keep(png, side, slim)
            }
            Ok(None) => false,
            Err(_) => {
                self.incoming = None;
                self.refuse()
            }
        }
    }

    /// Keep a finished skin of `side`, making the 64 x 64 version. False if
    /// the picture is not what its header said (noted as a refusal).
    fn keep(&mut self, png: Vec<u8>, side: usize, slim: bool) -> bool {
        if self.defer && side > 64 {
            self.ticket = self.ticket.wrapping_add(1);
            self.job = Some(Pending { ticket: self.ticket, png, side, slim });
            return false;
        }
        let Some(legacy) = skinxfer::legacy_version(&png, side) else { return self.refuse() };
        self.legacy = legacy;
        self.hd = if side > 64 { png } else { Vec::new() };
        self.side = side;
        self.slim = slim;
        true
    }

    /// The same, as one encoded message each (what the server queues: a big
    /// skin is sent a few pieces to a tick, see `net/skinqueue.rs`).
    pub fn messages_for(&self, id: u16, protocol: u16) -> Vec<Vec<u8>> {
        if self.legacy.is_empty() {
            return Vec::new();
        }
        if protocol >= HD_PROTOCOL && !self.hd.is_empty() {
            return skinxfer::messages(id, &self.hd, self.side, self.slim).iter().map(|m| m.encode()).collect();
        }
        alloc::vec![Msg::Skin { id, png: self.legacy.clone(), slim: self.slim }.encode()]
    }

    /// The bytes to send a client speaking `protocol`, as the skin of player
    /// `id` (empty if there is no skin).
    pub fn wire_for(&self, id: u16, protocol: u16) -> Vec<u8> {
        if self.legacy.is_empty() {
            return Vec::new();
        }
        if protocol >= HD_PROTOCOL && !self.hd.is_empty() {
            return skinxfer::wire(id, &self.hd, self.side, self.slim);
        }
        Msg::Skin { id, png: self.legacy.clone(), slim: self.slim }.encode()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{Inbox, Next, MAX_SKIN_PART};

    fn skin(side: usize, noisy: bool) -> Vec<u8> {
        let mut s = 5u32;
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

    fn messages(bytes: &[u8]) -> Vec<Msg> {
        let mut i = Inbox::default();
        i.push(bytes);
        let mut out = Vec::new();
        while let Next::Msg(m) = i.next() {
            out.push(m);
        }
        out
    }

    #[test]
    fn a_plain_64_skin_goes_to_everyone_as_it_is() {
        let png = skin(64, true);
        let mut s = SkinSlot::default();
        assert!(s.on_whole(png.clone(), true));
        for proto in [13, 14] {
            assert_eq!(messages(&s.wire_for(3, proto)), alloc::vec![Msg::Skin { id: 3, png: png.clone(), slim: true }]);
        }
    }

    #[test]
    fn a_big_skin_goes_whole_to_new_clients_and_downscaled_to_old_ones() {
        let png = skin(256, true);
        assert!(png.len() > MAX_SKIN_PART);
        let mut s = SkinSlot::default();
        assert!(s.on_begin(png.len() as u32, 256, false));
        let pieces: Vec<&[u8]> = png.chunks(MAX_SKIN_PART).collect();
        for (k, p) in pieces.iter().enumerate() {
            let done = s.on_part(p, k + 1 == pieces.len());
            assert_eq!(done, k + 1 == pieces.len());
        }
        assert_eq!(s.side(), 256);
        // Protocol 13 gets one plain 64 x 64 skin.
        let old = messages(&s.wire_for(9, 13));
        assert_eq!(old.len(), 1);
        match &old[0] {
            Msg::Skin { id: 9, png, .. } => {
                assert_eq!(skinxfer::side_of_png(png), Some(64));
                assert!(png.len() <= MAX_SKIN_MSG);
            }
            other => panic!("{other:?}"),
        }
        // Protocol 14 gets the original in pieces.
        let new = messages(&s.wire_for(9, 14));
        assert!(matches!(new[0], Msg::SkinBegin { id: 9, side: 256, .. }));
        let joined: Vec<u8> = new[1..].iter().flat_map(|m| if let Msg::SkinPart { data, .. } = m { data.clone() } else { Vec::new() }).collect();
        assert_eq!(joined, png);
    }

    #[test]
    fn big_skins_can_also_arrive_in_one_plain_message_if_small_enough() {
        let png = skin(128, false);
        assert!(png.len() < MAX_SKIN_MSG);
        let mut s = SkinSlot::default();
        assert!(s.on_whole(png.clone(), false));
        assert_eq!(s.side(), 128);
        assert!(matches!(messages(&s.wire_for(1, 14))[0], Msg::SkinBegin { .. }));
        assert!(matches!(messages(&s.wire_for(1, 13))[0], Msg::Skin { .. }));
    }

    #[test]
    fn skins_are_taken_once_in_five_seconds_one_transfer_at_a_time() {
        let a = skin(64, false);
        let mut s = SkinSlot::default();
        assert!(s.on_whole(a.clone(), false));
        assert!(!s.on_whole(a.clone(), false), "too soon");
        assert!(!s.on_begin(a.len() as u32, 64, false), "too soon");
        s.tick(5.1);
        assert!(s.on_begin(a.len() as u32, 64, false));
        assert!(!s.on_begin(a.len() as u32, 64, false), "one transfer at a time");
        assert!(!s.on_whole(a.clone(), false), "a transfer is going on");
        s.tick(5.1);
        // A transfer that goes quiet is dropped, and the way is clear again.
        s.tick(25.0);
        assert!(s.on_whole(a, false));
    }

    #[test]
    fn nonsense_is_refused_and_the_old_skin_stays() {
        let good = skin(64, false);
        let mut s = SkinSlot::default();
        assert!(s.on_whole(good.clone(), true));
        s.tick(6.0);
        // Not a skin at all, a picture of the wrong shape, and a lie about the size.
        assert!(!s.on_whole(b"not a picture, but long enough to be looked at".to_vec(), false));
        s.tick(6.0);
        let wide = crate::png::encode(128, 64, &[1u8; 128 * 64 * 4]);
        assert!(!s.on_whole(wide, false));
        s.tick(6.0);
        assert!(!s.on_begin(10, 128, false));
        assert!(!s.on_begin(5000, 130, false));
        assert!(!s.on_part(&[1, 2, 3], true), "a piece with no transfer");
        s.tick(6.0);
        // The picture's header says 128 but the picture inside is cut short.
        let big = skin(128, true);
        assert!(s.on_begin(big.len() as u32 - 30, 128, false));
        assert!(!s.on_part(&big[..big.len() - 30], true) || s.side() == 128);
        assert_eq!(messages(&s.wire_for(1, 13)).len(), 1);
        assert!(s.has_skin());
    }

    #[test]
    fn a_deferred_slot_hands_big_skins_to_a_worker_and_ignores_stale_answers() {
        let png = skin(128, true);
        let mut s = SkinSlot::deferred();
        // A 64 skin is taken at once.
        let small = skin(64, false);
        assert!(s.on_whole(small, false));
        assert!(s.take_job().is_none());
        s.tick(6.0);
        // A 128 one waits for the worker: nothing is passed on yet.
        assert!(s.on_begin(png.len() as u32, 128, true));
        assert!(!s.on_part(&png, true));
        assert_eq!(s.side(), 64);
        let first = s.take_job().expect("a job");
        assert_eq!((first.side, first.slim), (128, true));
        assert!(s.take_job().is_none());
        // A newer skin arrives before the answer: the old answer is ignored.
        s.tick(6.0);
        assert!(!s.on_whole(skin(128, false), false));
        let second = s.take_job().unwrap();
        let legacy = skinxfer::legacy_version(&first.png, 128);
        assert!(!s.finish(first, legacy));
        assert_eq!(s.side(), 64);
        let legacy = skinxfer::legacy_version(&second.png, 128);
        assert!(s.finish(second, legacy));
        assert_eq!(s.side(), 128);
        assert!(matches!(messages(&s.wire_for(1, 14))[0], Msg::SkinBegin { .. }));
        // A picture that does not decode is refused.
        s.tick(6.0);
        assert!(!s.on_whole(skin(128, false), false));
        let p = s.take_job().unwrap();
        assert!(!s.finish(p, None));
        assert_eq!(s.side(), 128);
    }

    #[test]
    fn messages_for_are_the_messages_of_wire_for() {
        let png = skin(256, true);
        let mut s = SkinSlot::default();
        assert!(s.on_begin(png.len() as u32, 256, false));
        for (k, p) in png.chunks(MAX_SKIN_PART).enumerate() {
            s.on_part(p, (k + 1) * MAX_SKIN_PART >= png.len());
        }
        for proto in [13, 14] {
            let joined: Vec<u8> = s.messages_for(2, proto).concat();
            assert_eq!(joined, s.wire_for(2, proto));
        }
        assert!(SkinSlot::default().messages_for(1, 14).is_empty());
    }

    #[test]
    fn a_64_skin_that_came_in_pieces_is_packed_small_enough_for_one_message() {
        // A noisy 64 x 64 picture stored uncompressed-ish is under the limit; pad it past
        // the limit with a trailing chunk the decoder ignores.
        let mut png = skin(64, true);
        let iend = png.len() - 12;
        let tail = png.split_off(iend);
        let junk = alloc::vec![7u8; 30_000];
        png.extend_from_slice(&(junk.len() as u32).to_be_bytes());
        let mut body = b"teXt".to_vec();
        body.extend_from_slice(&junk);
        png.extend_from_slice(&body);
        png.extend_from_slice(&crate::png::crc32(&body).to_be_bytes());
        png.extend_from_slice(&tail);
        assert!(png.len() > MAX_SKIN_MSG);
        let legacy = skinxfer::legacy_version(&png, 64).expect("packed again");
        assert!(legacy.len() <= MAX_SKIN_MSG);
        assert_eq!(skinxfer::side_of_png(&legacy), Some(64));
        assert_eq!(crate::png::decode(&legacy).unwrap().rgba, crate::png::decode(&png).unwrap().rgba);
    }
}
