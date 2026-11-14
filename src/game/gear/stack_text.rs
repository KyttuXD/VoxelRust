//! A stack as one word of text, for the player's save, chests and what is
//! sent over the network where chest contents go.
//!
//! `-` is an empty slot. Otherwise `item_id:count`, then `@wear` if the
//! stack is worn, then `#key=value&key=value` if it has item data. Keys and
//! values are percent-escaped (every byte but letters, digits, `_`, `-` and
//! `.` becomes `%XX`), so a word never holds a space, a newline or any of the
//! signs above. A save from before wear and data (`item_id:count` only) is
//! read as it was.

use super::item_data::{self, DataMap};
use crate::block::reg;
use crate::inventory::{Slot, Stack};
use alloc::string::String;
use alloc::vec::Vec;

fn escape(s: &str, out: &mut String) {
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.') {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
            out.push(char::from_digit((b & 15) as u32, 16).unwrap_or('0'));
        }
    }
}

fn unescape(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hi = (*b.get(i + 1)? as char).to_digit(16)?;
            let lo = (*b.get(i + 2)? as char).to_digit(16)?;
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The word for a slot.
pub fn write(slot: &Slot) -> String {
    let Some(s) = slot else { return String::from("-") };
    // An item that is not in the registry any more cannot be named.
    let Some(it) = reg().items.get(s.item as usize) else { return String::from("-") };
    let mut out = alloc::format!("{}:{}", it.id, s.count);
    if s.wear > 0 {
        out.push_str(&alloc::format!("@{}", s.wear));
    }
    let map = item_data::map_of(s.data);
    if !map.0.is_empty() {
        out.push('#');
        for (k, (key, value)) in map.0.iter().enumerate() {
            if k > 0 {
                out.push('&');
            }
            escape(key, &mut out);
            out.push('=');
            escape(value, &mut out);
        }
    }
    out
}

/// The slot a word stands for; an item that does not exist, a count of 0 or
/// a word that is not understood gives an empty slot.
pub fn read(word: &str) -> Slot {
    let (head, data_text) = match word.split_once('#') {
        Some((h, d)) => (h, Some(d)),
        None => (word, None),
    };
    let (head, wear_text) = match head.split_once('@') {
        Some((h, w)) => (h, Some(w)),
        None => (head, None),
    };
    let mut p = head.splitn(2, ':');
    let id = p.next().unwrap_or("");
    let n = p.next().and_then(|n| n.parse::<u32>().ok()).unwrap_or(0);
    let items = &reg().items;
    let item = items.iter().position(|it| it.id == id)?;
    if n == 0 {
        return None;
    }
    let mut stack = Stack::new(item as u16, n.min(items[item].stack.max(1) as u32) as u16);
    let durability = items[item].x.durability;
    if durability > 0 {
        // Worn through is not a state the game keeps: it would have broken.
        stack.wear = wear_text.and_then(|w| w.parse::<u32>().ok()).unwrap_or(0).min(durability as u32 - 1) as u16;
    }
    if let Some(d) = data_text {
        let mut map = DataMap::default();
        for pair in d.split('&').take(item_data::MAX_KEYS) {
            if let Some((k, v)) = pair.split_once('=') {
                if let (Some(k), Some(v)) = (unescape(k), unescape(v)) {
                    let _ = map.set(&k, &v);
                }
            }
        }
        stack.data = item_data::intern(&map).unwrap_or(0);
    }
    Some(stack)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> u16 {
        reg().items.iter().position(|i| i.id == id).unwrap() as u16
    }

    #[test]
    fn an_old_word_reads_as_a_plain_stack() {
        assert_eq!(read("stone:5"), Some(Stack::new(item("stone"), 5)));
        assert_eq!(read("-"), None);
        assert_eq!(read("stone:0"), None);
        assert_eq!(read("no_such_item:3"), None);
        assert_eq!(write(&Some(Stack::new(item("stone"), 5))), "stone:5");
    }

    #[test]
    fn data_and_odd_characters_round_trip() {
        let mut map = DataMap::default();
        map.set("gt_owner", "Kyttu Q & co: 100% #1 = ok").unwrap();
        map.set("gt_n", "7").unwrap();
        let s = Stack { data: item_data::intern(&map).unwrap(), ..Stack::new(item("stone"), 3) };
        let w = write(&Some(s));
        assert!(!w.contains(' ') && !w.contains('\n'));
        assert_eq!(read(&w), Some(s));
    }

    #[test]
    fn wear_round_trips_for_an_item_that_wears_and_is_dropped_for_one_that_does_not() {
        let sword = item("iron_sword");
        let max = reg().items[sword as usize].x.durability;
        assert!(max > 3);
        let s = Stack { wear: 3, ..Stack::new(sword, 1) };
        assert_eq!(read(&write(&Some(s))), Some(s));
        // A wear too big is kept just short of breaking.
        assert_eq!(read(&alloc::format!("iron_sword:1@{}", max as u32 + 50)).unwrap().wear, max - 1);
        // Stone does not wear.
        assert_eq!(read("stone:1@5").unwrap().wear, 0);
    }

    #[test]
    fn bad_data_is_skipped_not_fatal() {
        let s = read("stone:2#%zz=1&=x&ok=%41").unwrap();
        assert_eq!(item_data::get(s.data, "ok").as_deref(), Some("A"));
        assert_eq!(read("stone:2#").unwrap().data, 0);
    }
}
