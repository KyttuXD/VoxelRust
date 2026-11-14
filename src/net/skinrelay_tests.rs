//! The server's skin relay decisions, tested without sockets: which form of a
//! skin goes to which protocol level, who is let go when a queue overflows,
//! who is owed a refusal, and what a newcomer is owed.

use crate::net::skinqueue::{may_queue, relay_batch, relay_wanted, Batch, SkinQueue, MAX_QUEUED};
use crate::net::skinslot::{sends_refusal, SkinSlot, HD_PROTOCOL};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;

/// A batch whose one message is `tag` repeated (so the form can be told apart).
fn batch(tag: u8) -> Batch {
    Arc::new(vec![vec![tag; 10]])
}

/// A 64 x 64 skin picture, plain colour.
fn small_skin() -> Vec<u8> {
    crate::png::encode(64, 64, &vec![200u8; 64 * 64 * 4])
}

#[test]
fn protocol_13_is_sent_the_small_form_and_14_and_later_the_original() {
    let original = batch(1);
    let small = batch(2);
    for proto in [0u16, 12, 13] {
        let got = relay_batch(proto, &original, &small);
        assert!(Arc::ptr_eq(&got, &small), "protocol {proto} gets the 64 x 64 form");
    }
    for proto in [HD_PROTOCOL, HD_PROTOCOL + 1, 99] {
        let got = relay_batch(proto, &original, &small);
        assert!(Arc::ptr_eq(&got, &original), "protocol {proto} gets the original");
    }
}

#[test]
fn a_relayed_skin_goes_to_joined_players_other_than_its_sender() {
    // Connection 0 is the sender; 1 has joined; 2 is still joining; 3 has gone.
    let joined = [false, true, false, true];
    let gone = [false, false, false, true];
    let wanted: Vec<bool> = (0..4).map(|k| relay_wanted(k, 0, joined[k], gone[k])).collect();
    assert_eq!(wanted, [false, true, false, false]);
    // The sender is never sent their own skin, even when joined and here.
    assert!(!relay_wanted(2, 2, true, false));
}

#[test]
fn a_queue_that_overflows_is_let_go_and_one_within_the_limit_is_kept() {
    assert!(may_queue(0, MAX_QUEUED, MAX_QUEUED), "exactly the limit still fits");
    assert!(!may_queue(1, MAX_QUEUED, MAX_QUEUED), "one byte over is too much");
    assert!(!may_queue(usize::MAX, 1, MAX_QUEUED), "no wrap-around");
    // The same decision made by the queue itself: a slow connection fills up and is refused.
    let mut q = SkinQueue::default();
    let one_mib = || Arc::new(vec![vec![0u8; 1 << 20]]);
    let mut pushed = 0;
    while q.push(one_mib()) {
        pushed += 1;
        assert!(pushed < 100);
    }
    assert_eq!(pushed, MAX_QUEUED >> 20);
}

#[test]
fn a_newcomers_backlog_does_not_count_against_the_usual_limit() {
    let mut q = SkinQueue::default();
    // Seven players' skins of 1 MiB are owed at the join: all of them wait.
    q.allow_backlog(7 << 20);
    for _ in 0..7 {
        assert!(q.push(Arc::new(vec![vec![0u8; 1 << 20]])));
    }
    // The usual 8 MiB is still there for skins relayed later.
    let mut more = 0;
    while q.push(Arc::new(vec![vec![0u8; 1 << 20]])) {
        more += 1;
        assert!(more < 100);
    }
    assert_eq!(more, 8);
}

#[test]
fn only_clients_of_protocol_14_are_told_of_a_refusal() {
    assert!(!sends_refusal(0));
    assert!(!sends_refusal(13));
    assert!(sends_refusal(HD_PROTOCOL));
    assert!(sends_refusal(HD_PROTOCOL + 1));
}

#[test]
fn a_skin_too_soon_after_the_last_is_noted_once_for_the_refusal() {
    let mut s = SkinSlot::default();
    assert!(s.on_whole(small_skin(), false));
    assert!(!s.take_refused(), "a skin taken is not refused");
    assert!(!s.on_whole(small_skin(), false), "too soon");
    assert!(s.take_refused(), "the refusal is owed");
    assert!(!s.take_refused(), "and owed only once");
    s.tick(5.1);
    assert!(s.on_whole(small_skin(), false), "after the cooldown it is taken again");
}

#[test]
fn a_bad_header_and_a_transfer_that_goes_wrong_are_refusals() {
    let mut s = SkinSlot::default();
    assert!(!s.on_whole(b"not a picture, but long enough to be looked at".to_vec(), false));
    assert!(s.take_refused());
    // A piece with no transfer behind it.
    assert!(!s.on_part(&[1, 2, 3], true));
    assert!(s.take_refused());
    // A transfer that goes quiet is dropped and noted.
    assert!(s.on_begin(5000, 128, false));
    s.tick(25.0);
    assert!(s.take_refused());
}

#[test]
fn a_dropped_transfer_is_noted_and_the_slot_is_free_again() {
    let mut s = SkinSlot::default();
    assert!(s.on_begin(5000, 128, false));
    assert!(!s.on_begin(5000, 128, false), "one transfer at a time: refused");
    assert!(s.take_refused());
    s.abort();
    assert!(s.take_refused(), "an aborted transfer is noted");
    s.tick(5.1);
    assert!(s.on_begin(5000, 128, false), "and the next one may start");
}

#[test]
fn a_refusal_can_be_noted_from_outside_the_slot() {
    let mut s = SkinSlot::default();
    s.refuse_now();
    assert!(s.take_refused());
    assert!(!s.take_refused());
}
