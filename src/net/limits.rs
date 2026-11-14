//! What one connection may send, so that a modified client cannot flood the
//! server (or everyone else) with messages: token buckets per kind of
//! message, and a check of a skin picture's header before it is decoded.
//!
//! Voice frames have their own bucket (see `net::voice`). Plain functions and
//! small structs, tested without sockets.

/// The kinds of message that are rationed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// Positions, emotes, status effects, voice switches.
    Motion,
    /// One block change (an explosion sends hundreds at once).
    Block,
    /// A chat line or command.
    Chat,
    /// Shared state (creatures, chests, mods' own messages).
    State,
    /// A skin picture, or the announcement of a big one.
    Skin,
    /// A mod's message to the same mod on other machines (`net::modmsg`).
    Mod,
    /// One piece of a big skin (`net::skinxfer`): a skin of 1 MiB is 27 of them.
    SkinPart,
}

const CLASSES: usize = 7;

/// `(tokens per second, most tokens held)` of each class, in `Class` order.
const RATES: [(f32, f32); CLASSES] = [(100.0, 200.0), (1500.0, 6000.0), (2.0, 8.0), (300.0, 600.0), (0.5, 3.0), (crate::net::modmsg::PER_SECOND, crate::net::modmsg::PER_SECOND), (30.0, 60.0)];

/// Dropped messages in a row (less what the time since has forgiven) after
/// which the connection is cut instead of merely ignored.
const ABUSE_LIMIT: f32 = 4000.0;

/// One connection's allowances.
#[derive(Clone, Debug)]
pub struct Throttle {
    tokens: [f32; CLASSES],
    abuse: f32,
}

impl Default for Throttle {
    fn default() -> Throttle {
        let mut tokens = [0.0; CLASSES];
        for (t, r) in tokens.iter_mut().zip(RATES.iter()) {
            *t = r.1;
        }
        Throttle { tokens, abuse: 0.0 }
    }
}

impl Throttle {
    /// Let `dt` seconds pass.
    pub fn refill(&mut self, dt: f32) {
        // (NaN or a negative step must not poison the buckets.)
        let dt = if dt > 0.0 && dt < 3600.0 { dt } else { 0.0 };
        for (t, r) in self.tokens.iter_mut().zip(RATES.iter()) {
            *t = (*t + dt * r.0).min(r.1);
        }
        self.abuse = (self.abuse - dt * 200.0).max(0.0);
    }

    /// Take one message's worth; false means drop the message.
    pub fn allow(&mut self, class: Class) -> bool {
        let t = &mut self.tokens[class as usize];
        if *t >= 1.0 {
            *t -= 1.0;
            true
        } else {
            self.abuse += 1.0;
            false
        }
    }

    /// Whether this connection keeps sending far more than it may.
    pub fn abusive(&self) -> bool {
        self.abuse > ABUSE_LIMIT
    }
}

/// Whether `png` starts like a PNG of a size a skin may have: square, a
/// multiple of 64 from 64 to 4096, not interlaced (see `content::skin_grid`).
/// Checked before a skin is decoded, because a PNG header can claim a picture
/// of 4096 x 4096 in a few bytes, and decoding that costs tens of megabytes.
pub fn skin_header_ok(png: &[u8]) -> bool {
    crate::net::skinxfer::side_of_png(png).is_some()
}

/// A time of day from the network as a number in 0..1 (anything else, such as
/// NaN or 1e30, would upset the sky): the fraction of it, or the default.
pub fn clean_time(t: f32) -> f32 {
    if !t.is_finite() {
        return 0.12;
    }
    let f = t - crate::math::floorf(t);
    if f >= 0.0 && f < 1.0 {
        f
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_is_allowed_then_a_flood_is_dropped_and_time_forgives() {
        let mut t = Throttle::default();
        let ok = (0..50).filter(|_| t.allow(Class::Chat)).count();
        assert_eq!(ok, 8, "a few chat lines at once");
        assert!(!t.allow(Class::Chat));
        // The other classes are not touched.
        assert!(t.allow(Class::Motion) && t.allow(Class::Block));
        t.refill(1.0);
        assert_eq!((0..10).filter(|_| t.allow(Class::Chat)).count(), 2, "two a second");
        // A bomb of blocks is allowed in one go (explosions), not forever.
        let mut t = Throttle::default();
        assert_eq!((0..10_000).filter(|_| t.allow(Class::Block)).count(), 6000);
    }

    #[test]
    fn mods_may_send_eight_messages_a_second() {
        let mut t = Throttle::default();
        assert_eq!((0..100).filter(|_| t.allow(Class::Mod)).count(), 8);
        t.refill(0.5);
        assert_eq!((0..100).filter(|_| t.allow(Class::Mod)).count(), 4);
        // Their messages do not use up the allowance of the shared state.
        assert!(t.allow(Class::State));
    }

    #[test]
    fn only_a_sustained_flood_is_abusive() {
        let mut t = Throttle::default();
        for _ in 0..1000 {
            t.allow(Class::Skin);
        }
        assert!(!t.abusive());
        for _ in 0..10_000 {
            t.allow(Class::Skin);
        }
        assert!(t.abusive());
        t.refill(60.0);
        assert!(!t.abusive());
        // Nonsense time steps change nothing.
        let mut u = Throttle::default();
        u.refill(f32::NAN);
        u.refill(-5.0);
        assert!(u.allow(Class::Motion));
    }

    #[test]
    fn skins_must_say_they_are_a_size_a_skin_may_be() {
        let good = crate::png::encode(64, 64, &alloc::vec![255u8; 64 * 64 * 4]);
        assert!(skin_header_ok(&good));
        for side in [128usize, 192, 512] {
            assert!(skin_header_ok(&crate::png::encode(side, side, &alloc::vec![255u8; side * side * 4])), "{side}");
        }
        // Wrong shapes: not a multiple of 64, not square.
        assert!(!skin_header_ok(&crate::png::encode(100, 100, &alloc::vec![255u8; 100 * 100 * 4])));
        assert!(!skin_header_ok(&crate::png::encode(128, 64, &alloc::vec![255u8; 128 * 64 * 4])));
        assert!(!skin_header_ok(&good[..20]));
        assert!(!skin_header_ok(b"not a picture at all, but long enough"));
        // Headers that claim too much (the checksum no longer fits either: both are refused).
        let mut bomb = good.clone();
        bomb[16..20].copy_from_slice(&8192u32.to_be_bytes());
        bomb[20..24].copy_from_slice(&8192u32.to_be_bytes());
        assert!(!skin_header_ok(&bomb));
        let mut bomb = good.clone();
        let crc = crate::png::crc32(&{
            let mut b = bomb[12..29].to_vec();
            b[4..8].copy_from_slice(&8192u32.to_be_bytes());
            b[8..12].copy_from_slice(&8192u32.to_be_bytes());
            b
        });
        bomb[16..20].copy_from_slice(&8192u32.to_be_bytes());
        bomb[20..24].copy_from_slice(&8192u32.to_be_bytes());
        bomb[29..33].copy_from_slice(&crc.to_be_bytes());
        assert!(!skin_header_ok(&bomb), "a header with a right checksum but a size no skin has");
    }

    #[test]
    fn pieces_of_a_big_skin_have_room_but_not_for_ever() {
        let mut t = Throttle::default();
        assert_eq!((0..200).filter(|_| t.allow(Class::SkinPart)).count(), 60);
        assert!(t.allow(Class::Skin) && t.allow(Class::Motion));
        t.refill(1.0);
        assert_eq!((0..200).filter(|_| t.allow(Class::SkinPart)).count(), 30);
    }

    #[test]
    fn times_are_brought_into_the_day() {
        assert_eq!(clean_time(0.25), 0.25);
        assert_eq!(clean_time(1.5), 0.5);
        assert_eq!(clean_time(-0.25), 0.75);
        assert_eq!(clean_time(f32::NAN), 0.12);
        assert_eq!(clean_time(f32::INFINITY), 0.12);
        let big = clean_time(1.0e30);
        assert!((0.0..1.0).contains(&big));
    }
}
