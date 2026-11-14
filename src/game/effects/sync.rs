//! Telling the other players which effects are on this one: the list that
//! `proto::Msg::Effects` carries. Only a change of the set (an effect
//! starting, ending or changing level) is sent, not the passing of time;
//! the seconds in the message are what was left when it was sent.

use super::Effects;
use alloc::string::String;
use alloc::vec::Vec;

/// Most effects in one message.
pub const MAX_WIRE: usize = 16;
/// Longest effect id sent.
pub const MAX_ID: usize = 32;

/// One effect as the message has it: id, level (255 at most), seconds left
/// (rounded up).
pub type Entry = (String, u8, u32);

/// The effects as they go on the wire: at most [`MAX_WIRE`], with levels
/// held to what a byte can say.
pub fn to_wire(fx: &Effects) -> Vec<Entry> {
    fx.list
        .iter()
        .filter(|a| a.id.len() <= MAX_ID)
        .take(MAX_WIRE)
        .map(|a| {
            let whole = a.secs.max(0.0) as u32;
            let secs = if a.secs > whole as f32 { whole.saturating_add(1) } else { whole };
            (a.id.clone(), a.level.clamp(1, 255) as u8, secs)
        })
        .collect()
}

/// What a received list is kept as: ids that could be typed, the first
/// [`MAX_WIRE`] of them, levels of at least 1. A hostile server cannot make
/// a client hold more than that for a player.
pub fn clean(list: Vec<Entry>) -> Vec<Entry> {
    list.into_iter()
        .filter(|(id, _, _)| !id.is_empty() && id.len() <= MAX_ID && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
        .take(MAX_WIRE)
        .map(|(id, level, secs)| (id, level.max(1), secs.min(super::MAX_SECS as u32)))
        .collect()
}

/// The part of a list that is worth telling again when it changes.
pub fn key(list: &[Entry]) -> Vec<(String, u8)> {
    let mut k: Vec<(String, u8)> = list.iter().map(|(id, level, _)| (id.clone(), *level)).collect();
    k.sort();
    k
}

/// Tell the others if the set of effects changed since the last time. Does
/// nothing outside a shared game.
pub fn send_if_changed(g: &mut crate::session::Session) {
    if g.net.is_none() {
        return;
    }
    let list = to_wire(&g.fx);
    let k = key(&list);
    if k == g.fx.sent {
        return;
    }
    g.fx.sent = k;
    if let Some(n) = g.net.as_mut() {
        n.send_effects(&list);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::effects::data::parse_text;

    #[test]
    fn the_list_goes_on_the_wire_rounded_up_and_held_in_range() {
        let defs = parse_text("effect a\neffect b\n").0;
        let mut fx = Effects::default();
        fx.give(&defs[0], 12.2, 3);
        fx.give(&defs[1], 5.0, 255);
        let w = to_wire(&fx);
        assert_eq!(w, [(String::from("a"), 3, 13), (String::from("b"), 255, 5)]);
        assert!(to_wire(&Effects::default()).is_empty());
    }

    #[test]
    fn no_more_than_the_limit_is_sent_or_kept() {
        let mut fx = Effects::default();
        for i in 0..30 {
            fx.give(&crate::game::effects::EffectDef::plain(&alloc::format!("e{i}")), 5.0, 1);
        }
        assert_eq!(to_wire(&fx).len(), MAX_WIRE);
        let many: Vec<Entry> = (0..100).map(|i| (alloc::format!("e{i}"), 1, 5)).collect();
        assert_eq!(clean(many).len(), MAX_WIRE);
    }

    #[test]
    fn a_received_list_is_cleaned() {
        let got = clean(alloc::vec![
            (String::from("poison"), 2, 10),
            (String::from("Bad Id"), 1, 5),
            (String::new(), 1, 5),
            (String::from("x".repeat(40)), 1, 5),
            (String::from("burning"), 0, u32::MAX),
        ]);
        assert_eq!(got, [(String::from("poison"), 2, 10), (String::from("burning"), 1, 86_400)]);
    }

    #[test]
    fn only_a_change_of_the_set_counts_not_the_time() {
        let a = alloc::vec![(String::from("b"), 1, 30), (String::from("a"), 2, 9)];
        let b = alloc::vec![(String::from("a"), 2, 1), (String::from("b"), 1, 3)];
        assert_eq!(key(&a), key(&b));
        let c = alloc::vec![(String::from("a"), 3, 1), (String::from("b"), 1, 3)];
        assert_ne!(key(&a), key(&c));
    }
}
