//! The payloads of the player-versus-player messages (little-endian; each
//! must have exactly its length, anything else is dropped).

use alloc::vec::Vec;

/// A player's armour as the server uses it, and whether they are alive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gear {
    pub defense: u8,
    pub toughness: u8,
    pub alive: bool,
}

impl Default for Gear {
    fn default() -> Gear {
        Gear { defense: 0, toughness: 0, alive: true }
    }
}

impl Gear {
    pub fn pack(&self) -> Vec<u8> {
        alloc::vec![self.defense, self.toughness, self.alive as u8]
    }
    pub fn unpack(d: &[u8]) -> Option<Gear> {
        match d {
            [defense, toughness, alive] if *alive <= 1 => Some(Gear { defense: *defense, toughness: *toughness, alive: *alive == 1 }),
            _ => None,
        }
    }
}

/// "I hit this player": the target, the item in hand (index plus one, 0 = bare
/// hand) and the damage the attacker's game worked out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    pub target: u16,
    pub item: u16,
    pub damage: u8,
}

impl Hit {
    pub fn pack(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(5);
        v.extend_from_slice(&self.target.to_le_bytes());
        v.extend_from_slice(&self.item.to_le_bytes());
        v.push(self.damage);
        v
    }
    pub fn unpack(d: &[u8]) -> Option<Hit> {
        if d.len() != 5 {
            return None;
        }
        Some(Hit { target: u16::from_le_bytes([d[0], d[1]]), item: u16::from_le_bytes([d[2], d[3]]), damage: d[4] })
    }
}

/// "You were hit": by whom, for how much (after armour), and the direction away
/// from the attacker (a unit vector in hundredths).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hurt {
    pub attacker: u16,
    pub damage: u8,
    pub dir: [i8; 2],
}

impl Hurt {
    pub fn pack(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(5);
        v.extend_from_slice(&self.attacker.to_le_bytes());
        v.push(self.damage);
        v.push(self.dir[0] as u8);
        v.push(self.dir[1] as u8);
        v
    }
    pub fn unpack(d: &[u8]) -> Option<Hurt> {
        if d.len() != 5 {
            return None;
        }
        Some(Hurt { attacker: u16::from_le_bytes([d[0], d[1]]), damage: d[2], dir: [d[3] as i8, d[4] as i8] })
    }
}

/// "This player was hit by that one", for everyone near.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    pub victim: u16,
    pub attacker: u16,
}

impl Seen {
    pub fn pack(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(4);
        v.extend_from_slice(&self.victim.to_le_bytes());
        v.extend_from_slice(&self.attacker.to_le_bytes());
        v
    }
    pub fn unpack(d: &[u8]) -> Option<Seen> {
        if d.len() != 4 {
            return None;
        }
        Some(Seen { victim: u16::from_le_bytes([d[0], d[1]]), attacker: u16::from_le_bytes([d[2], d[3]]) })
    }
}

/// A direction (unit vector, east/south components) as the two bytes `Hurt` carries.
pub fn pack_dir(d: [f32; 2]) -> [i8; 2] {
    let f = |v: f32| {
        let n = v * 100.0;
        (if n > 100.0 { 100.0 } else if n < -100.0 { -100.0 } else { n }) as i8
    };
    [f(d[0]), f(d[1])]
}

/// The direction in a `Hurt`, as a vector of at most length 1 (a made-up one is shortened).
pub fn unpack_dir(d: [i8; 2]) -> [f32; 2] {
    let (x, z) = (d[0] as f32 / 100.0, d[1] as f32 / 100.0);
    let len2 = x * x + z * z;
    if len2 > 1.0 {
        let l = crate::math::sqrtf(len2);
        [x / l, z / l]
    } else {
        [x, z]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{channel, Inbox, Msg, Next};

    #[test]
    fn every_payload_round_trips_and_only_at_its_own_length() {
        let h = Hit { target: 7, item: 301, damage: 9 };
        assert_eq!(Hit::unpack(&h.pack()), Some(h));
        assert_eq!(Hit::unpack(&h.pack()[..4]), None);
        let u = Hurt { attacker: 65_000, damage: 12, dir: [-100, 37] };
        assert_eq!(Hurt::unpack(&u.pack()), Some(u));
        assert_eq!(Hurt::unpack(&[0; 6]), None);
        let s = Seen { victim: 3, attacker: 4 };
        assert_eq!(Seen::unpack(&s.pack()), Some(s));
        let g = Gear { defense: 18, toughness: 6, alive: false };
        assert_eq!(Gear::unpack(&g.pack()), Some(g));
        assert_eq!(Gear::unpack(&[1, 2, 5]), None);
        assert_eq!(Gear::unpack(&[]), None);
    }

    #[test]
    fn the_messages_survive_the_wire_on_their_channels() {
        let mut inbox = Inbox::default();
        for (channel, data) in [
            (channel::PVP_HIT, Hit { target: 2, item: 0, damage: 1 }.pack()),
            (channel::PVP_HURT, Hurt { attacker: 1, damage: 3, dir: [100, 0] }.pack()),
            (channel::PVP_SEEN, Seen { victim: 2, attacker: 1 }.pack()),
            (channel::PVP_GEAR, Gear::default().pack()),
        ] {
            let msg = Msg::State { id: 0, channel, keep: false, key: alloc::string::String::new(), data };
            inbox.push(&msg.encode());
            match inbox.next() {
                Next::Msg(back) => assert_eq!(back, msg),
                _ => panic!("message lost"),
            }
        }
    }

    #[test]
    fn directions_are_packed_in_hundredths_and_never_longer_than_one() {
        assert_eq!(pack_dir([1.0, 0.0]), [100, 0]);
        assert_eq!(pack_dir([-0.6, 0.8]), [-60, 80]);
        assert_eq!(pack_dir([5.0, -5.0]), [100, -100]);
        let [x, z] = unpack_dir([-60, 80]);
        assert!((x + 0.6).abs() < 1e-6 && (z - 0.8).abs() < 1e-6);
        let [x, z] = unpack_dir([100, 100]);
        assert!(x * x + z * z <= 1.0001);
    }
}
