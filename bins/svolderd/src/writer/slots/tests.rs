// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

type S = Slots<&'static str, &'static str>;

fn id(n: u8) -> FunctionId {
    FunctionId::one(n)
}

fn latest(item: &'static str) -> Queued<&'static str> {
    Queued {
        mode: Mode::Latest,
        held: false,
        barrier: false,
        echoes: Vec::new(),
        item,
    }
}

fn ordered(item: &'static str) -> Queued<&'static str> {
    Queued {
        mode: Mode::Ordered,
        held: false,
        barrier: false,
        echoes: Vec::new(),
        item,
    }
}

fn tone(item: &'static str) -> Queued<&'static str> {
    Queued {
        mode: Mode::Latest,
        held: true,
        barrier: false,
        echoes: Vec::new(),
        item,
    }
}

fn typed_tone(item: &'static str) -> Queued<&'static str> {
    Queued {
        mode: Mode::Ordered,
        held: true,
        barrier: false,
        echoes: Vec::new(),
        item,
    }
}

/// What a list of actions says, in words a test reads.
fn said(actions: Vec<Action<&'static str, &'static str>>) -> Vec<String> {
    actions
        .into_iter()
        .map(|action| match action {
            Action::Send(id, item) => format!("send {id} {item}"),
            Action::Supersede(item) => format!("supersede {item}"),
            Action::Start(recall) => format!("start {recall}"),
            Action::Replace(recall) => format!("replace {recall}"),
        })
        .collect()
}

#[test]
fn writes_to_different_functions_go_out_at_once_in_arrival_order() {
    let mut slots = S::default();
    assert_eq!(
        said(slots.submit(id(10), latest("volume 60"))),
        ["send 10 volume 60"]
    );
    assert_eq!(
        said(slots.submit(id(5), ordered("mute on"))),
        ["send 5 mute on"]
    );
    assert_eq!(slots.flying(), [id(5), id(10)]);
}

#[test]
fn a_latest_write_replaces_the_one_waiting_and_the_newest_goes_next() {
    let mut slots = S::default();
    slots.submit(id(10), latest("60"));
    assert!(said(slots.submit(id(10), latest("61"))).is_empty());
    assert_eq!(said(slots.submit(id(10), latest("62"))), ["supersede 61"]);
    assert_eq!(said(slots.answered(id(10))), ["send 10 62"]);
    assert!(said(slots.answered(id(10))).is_empty());
    assert!(slots.flying().is_empty());
}

#[test]
fn an_ordered_write_is_never_dropped_and_drops_a_waiting_latest_one() {
    let mut slots = S::default();
    slots.submit(id(10), latest("60"));
    slots.submit(id(10), latest("61"));
    assert_eq!(said(slots.submit(id(10), ordered("up"))), ["supersede 61"]);
    assert!(said(slots.submit(id(10), ordered("up again"))).is_empty());
    assert!(
        said(slots.submit(id(10), latest("70"))).is_empty(),
        "behind the ordered ones"
    );
    assert_eq!(said(slots.answered(id(10))), ["send 10 up"]);
    assert_eq!(said(slots.answered(id(10))), ["send 10 up again"]);
    assert_eq!(said(slots.answered(id(10))), ["send 10 70"]);
}

#[test]
fn a_tone_write_during_a_recall_is_held_and_released_after_it_in_first_arrival_order() {
    let mut slots = S::default();
    assert_eq!(said(slots.pick("Warm", |_| false)), ["start Warm"]);
    assert!(said(slots.submit(id(51), tone("mode flat"))).is_empty());
    assert!(said(slots.submit(id(23), tone("band 2 g 1"))).is_empty());
    assert_eq!(
        said(slots.submit(id(23), tone("band 2 g 2"))),
        ["supersede band 2 g 1"]
    );
    assert_eq!(
        said(slots.submit(id(10), latest("volume 50"))),
        ["send 10 volume 50"],
        "not a tone write"
    );
    assert_eq!(slots.held(), 2);
    assert_eq!(
        said(slots.recall_done()),
        ["send 51 mode flat", "send 23 band 2 g 2"]
    );
}

#[test]
fn a_pick_drops_the_held_tone_writes_it_rewrites_and_keeps_the_others() {
    let mut slots = S::default();
    slots.pick("Warm", |_| false);
    slots.submit(id(23), tone("band"));
    slots.submit(id(52), tone("loudness"));
    slots.submit(
        id(24),
        Queued {
            mode: Mode::Ordered,
            held: true,
            barrier: false,
            echoes: Vec::new(),
            item: "typed band",
        },
    );
    assert_eq!(
        said(slots.pick("Bass", |id| id == FunctionId::one(52))),
        ["supersede band"]
    );
    assert_eq!(slots.held(), 2);
    assert_eq!(
        said(slots.recall_done()),
        ["send 52 loudness", "send 24 typed band"],
        "what arrived before Bass goes out before it"
    );
    assert!(said(slots.answered(id(52))).is_empty());
    assert_eq!(said(slots.answered(id(24))), ["start Bass"]);
    assert!(said(slots.recall_done()).is_empty());
}

#[test]
fn a_tone_write_held_since_a_pick_waits_for_that_pick_to_be_done() {
    let mut slots = S::default();
    slots.pick("Warm", |_| false);
    slots.submit(id(23), tone("band before"));
    assert_eq!(
        said(slots.pick("Bass", |_| false)),
        ["supersede band before"],
        "Bass rewrites it"
    );
    assert!(said(slots.submit(id(23), tone("band after"))).is_empty());
    assert!(said(slots.submit(id(21), tone("band 1 after"))).is_empty());
    assert_eq!(
        said(slots.recall_done()),
        ["start Bass"],
        "nothing held from before Bass is left"
    );
    assert_eq!(
        said(slots.recall_done()),
        ["send 23 band after", "send 21 band 1 after"]
    );
}

#[test]
fn a_write_replacing_a_kept_one_after_a_pick_goes_out_after_that_pick() {
    let mut slots = S::default();
    slots.pick("Warm", |_| false);
    slots.submit(id(23), tone("band before"));
    assert!(said(slots.pick("Bass", |_| true)).is_empty());
    assert_eq!(
        said(slots.submit(id(23), tone("band after"))),
        ["supersede band before"]
    );
    assert_eq!(said(slots.recall_done()), ["start Bass"]);
    assert_eq!(said(slots.recall_done()), ["send 23 band after"]);
}

#[test]
fn a_recall_waiting_on_a_tone_write_in_flight_starts_after_the_writes_held_before_it() {
    let mut slots = S::default();
    assert_eq!(said(slots.submit(id(23), tone("band"))), ["send 23 band"]);
    assert!(said(slots.pick("Warm", |_| false)).is_empty());
    assert!(said(slots.submit(id(21), typed_tone("typed"))).is_empty());
    assert_eq!(said(slots.pick("Bass", |_| false)), ["replace Warm"]);
    assert_eq!(said(slots.answered(id(23))), ["send 21 typed"]);
    assert_eq!(said(slots.answered(id(21))), ["start Bass"]);
    assert!(said(slots.recall_done()).is_empty());
}

#[test]
fn a_write_held_while_the_earlier_ones_go_out_waits_for_the_recall_that_replaced_its_pick() {
    let mut slots = S::default();
    slots.pick("A", |_| false);
    slots.submit(id(21), typed_tone("w1"));
    assert!(said(slots.pick("B", |_| false)).is_empty());
    assert_eq!(said(slots.recall_done()), ["send 21 w1"]);
    assert!(said(slots.submit(id(22), typed_tone("w2"))).is_empty());
    assert_eq!(said(slots.pick("C", |_| false)), ["replace B"]);
    assert_eq!(
        said(slots.answered(id(21))),
        ["send 22 w2"],
        "w2 arrived before C"
    );
    assert_eq!(said(slots.answered(id(22))), ["start C"]);
}

#[test]
fn a_pick_waits_for_a_tone_write_in_flight_and_a_later_pick_replaces_it() {
    let mut slots = S::default();
    assert_eq!(said(slots.submit(id(23), tone("band"))), ["send 23 band"]);
    assert!(said(slots.pick("Warm", |_| false)).is_empty());
    assert_eq!(said(slots.pick("Presence", |_| false)), ["replace Warm"]);
    assert_eq!(slots.waiting(), Some(&"Presence"));
    assert!(
        said(slots.submit(id(21), tone("band 1"))).is_empty(),
        "held behind the pick"
    );
    assert_eq!(said(slots.answered(id(23))), ["start Presence"]);
    assert_eq!(said(slots.recall_done()), ["send 21 band 1"]);
}

#[test]
fn a_pick_behind_a_running_recall_starts_when_it_is_done() {
    let mut slots = S::default();
    slots.pick("Warm", |_| false);
    assert!(said(slots.pick("Bass", |_| false)).is_empty());
    assert!(slots.running());
    assert_eq!(said(slots.recall_done()), ["start Bass"]);
    assert!(said(slots.recall_done()).is_empty());
    assert!(!slots.running());
}

#[test]
fn draining_hands_back_every_write_and_recall_left() {
    let mut slots = S::default();
    slots.submit(id(10), latest("60"));
    slots.submit(id(10), latest("61"));
    slots.pick("Warm", |_| false);
    slots.pick("Bass", |_| false);
    slots.submit(id(23), tone("band"));
    let (writes, recalls) = slots.drain();
    assert_eq!(writes, ["61", "band"]);
    assert_eq!(recalls, ["Bass"]);
}

#[test]
fn a_later_latest_write_held_behind_an_ordered_one_goes_out_after_it() {
    let mut slots = S::default();
    slots.pick("Warm", |_| false);
    assert!(said(slots.submit(id(23), tone("x1"))).is_empty());
    assert_eq!(
        said(slots.submit(
            id(23),
            Queued {
                mode: Mode::Ordered,
                held: true,
                barrier: false,
                echoes: Vec::new(),
                item: "y",
            }
        )),
        ["supersede x1"]
    );
    assert!(said(slots.submit(id(23), tone("x2"))).is_empty());
    assert_eq!(said(slots.recall_done()), ["send 23 y"]);
    assert_eq!(said(slots.answered(id(23))), ["send 23 x2"]);
}

#[test]
fn a_write_waiting_behind_its_function_is_not_overtaken_by_a_later_write_to_another() {
    let mut slots = S::default();
    assert_eq!(
        said(slots.submit(id(10), latest("volume -10"))),
        ["send 10 volume -10"]
    );
    assert!(said(slots.submit(id(10), latest("volume -40"))).is_empty());
    assert!(
        said(slots.submit(id(5), ordered("mute off"))).is_empty(),
        "behind the volume"
    );
    assert_eq!(
        said(slots.answered(id(10))),
        ["send 10 volume -40", "send 5 mute off"]
    );
}

#[test]
fn a_latest_replacement_keeps_the_place_of_the_write_it_replaced() {
    let mut slots = S::default();
    slots.submit(id(10), latest("a"));
    slots.submit(id(10), latest("b1"));
    slots.submit(id(5), ordered("mute"));
    assert_eq!(said(slots.submit(id(10), latest("b2"))), ["supersede b1"]);
    assert_eq!(said(slots.answered(id(10))), ["send 10 b2", "send 5 mute"]);
}

#[test]
fn the_writes_waiting_for_each_function_are_listed_oldest_first_without_the_one_in_flight() {
    let mut slots = S::default();
    slots.submit(id(10), ordered("a"));
    slots.submit(id(10), ordered("b"));
    slots.submit(id(5), ordered("mute"));
    slots.submit(id(10), ordered("c"));
    assert_eq!(
        slots.waiting_for(id(10)).copied().collect::<Vec<_>>(),
        ["b", "c"]
    );
    assert_eq!(
        slots.waiting_for(id(5)).copied().collect::<Vec<_>>(),
        ["mute"]
    );
    assert_eq!(slots.waiting_for(id(7)).count(), 0);
    slots.answered(id(10));
    assert_eq!(
        slots.waiting_for(id(10)).copied().collect::<Vec<_>>(),
        ["c"]
    );
}

fn barrier(item: &'static str) -> Queued<&'static str> {
    Queued {
        mode: Mode::Ordered,
        held: true,
        barrier: true,
        echoes: Vec::new(),
        item,
    }
}

#[test]
fn a_barrier_write_waits_until_no_held_write_is_in_flight_and_holds_the_line_behind_it() {
    let mut slots = S::default();
    assert_eq!(said(slots.submit(id(23), tone("band"))), ["send 23 band"]);
    assert_eq!(
        said(slots.submit(id(24), tone("band 2"))),
        ["send 24 band 2"]
    );
    assert!(said(slots.submit(id(70), barrier("recall"))).is_empty());
    assert!(
        said(slots.submit(id(5), ordered("mute"))).is_empty(),
        "not overtaking the barrier"
    );
    assert!(
        said(slots.answered(id(23))).is_empty(),
        "one band is still in flight"
    );
    assert_eq!(
        said(slots.answered(id(24))),
        ["send 70 recall", "send 5 mute"]
    );
}

#[test]
fn a_barrier_write_goes_at_once_when_no_held_write_is_in_flight() {
    let mut slots = S::default();
    assert_eq!(said(slots.submit(id(5), ordered("mute"))), ["send 5 mute"]);
    assert_eq!(
        said(slots.submit(id(70), barrier("recall"))),
        ["send 70 recall"]
    );
}

#[test]
fn an_awaited_echo_holds_a_barrier_write_that_waits_for_it_and_nothing_else() {
    let mut slots = S::default();
    assert_eq!(said(slots.submit(id(23), tone("band"))), ["send 23 band"]);
    slots.await_echo(id(23));
    assert!(said(slots.answered(id(23))).is_empty());
    assert_eq!(
        said(slots.submit(id(23), tone("band again"))),
        ["send 23 band again"],
        "the next write on the band goes at once"
    );
    assert!(said(slots.answered(id(23))).is_empty());
    let waits = Queued {
        echoes: vec![id(23)],
        ..barrier("recall")
    };
    assert!(said(slots.submit(id(70), waits)).is_empty());
    assert!(said(slots.submit(id(5), ordered("mute"))).is_empty());
    assert_eq!(
        said(slots.echo_in(id(23))),
        ["send 70 recall", "send 5 mute"]
    );
}

#[test]
fn an_awaited_echo_on_another_control_does_not_hold_a_barrier_write() {
    let mut slots = S::default();
    slots.await_echo(id(24));
    let waits = Queued {
        echoes: vec![id(23)],
        ..barrier("recall")
    };
    assert_eq!(said(slots.submit(id(70), waits)), ["send 70 recall"]);
}
