// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn pk(f: f64, g: f64) -> PointView {
    PointView {
        slot: 0,
        kind: "PK".into(),
        f,
        g,
        q: 1.0,
    }
}

#[test]
fn the_focus_stays_where_the_list_did_not_change() {
    let list = [pk(100.0, 0.0), pk(1000.0, 1.0), pk(8000.0, 2.0)];
    assert_eq!(followed(&list, &list, 1), 1);
}

#[test]
fn a_point_whose_frequency_passed_another_is_followed_to_its_new_place() {
    // the focused point (1 kHz) is moved to 9 kHz and listed last
    let before = [pk(100.0, 0.0), pk(1000.0, 1.0), pk(8000.0, 2.0)];
    let after = [pk(100.0, 0.0), pk(8000.0, 2.0), pk(9000.0, 1.0)];
    assert_eq!(followed(&before, &after, 1), 2);
    // and the first moved to the middle
    let before = [pk(100.0, 0.0), pk(1000.0, 1.0), pk(8000.0, 2.0)];
    let after = [pk(1000.0, 1.0), pk(4000.0, 0.0), pk(8000.0, 2.0)];
    assert_eq!(followed(&before, &after, 0), 1);
}

#[test]
fn another_front_ends_move_of_a_different_point_keeps_the_focused_point() {
    let before = [pk(100.0, 0.0), pk(1000.0, 1.0), pk(8000.0, 2.0)];
    // the 100 Hz point went to 9 kHz: the 1 kHz point is point 1 now
    let after = [pk(1000.0, 1.0), pk(8000.0, 2.0), pk(9000.0, 0.0)];
    assert_eq!(followed(&before, &after, 1), 0);
}

#[test]
fn a_list_that_is_all_new_keeps_the_index_held_inside_it() {
    let before = [pk(100.0, 0.0), pk(1000.0, 1.0), pk(8000.0, 2.0)];
    let after = [pk(200.0, 0.0), pk(300.0, 0.0)];
    assert_eq!(followed(&before, &after, 2), 1);
    assert_eq!(followed(&[], &after, 5), 5);
}

#[test]
fn an_added_point_is_followed_to_where_its_frequency_lists_it() {
    let before = [pk(100.0, 0.0), pk(8000.0, 2.0)];
    let after = [pk(100.0, 0.0), pk(1000.0, 0.0), pk(8000.0, 2.0)];
    assert_eq!(followed(&before, &after, 2), 1);
}

fn slotted(slot: u8, f: f64) -> PointView {
    PointView { slot, ..pk(f, 0.0) }
}

#[test]
fn a_point_is_followed_by_its_slot_whatever_the_values_do() {
    let before = [slotted(1, 100.0), slotted(2, 1000.0), slotted(3, 8000.0)];
    // another front end moved the 100 Hz point to the end: the 1 kHz point (slot 2) is point 1
    let after = [slotted(2, 1000.0), slotted(3, 8000.0), slotted(1, 9000.0)];
    assert_eq!(followed(&before, &after, 1), 0);
    assert_eq!(followed(&before, &after, 2), 1);
    // a point added is the slot that was not there
    let added = [
        slotted(1, 100.0),
        slotted(4, 500.0),
        slotted(2, 1000.0),
        slotted(3, 8000.0),
    ];
    assert_eq!(followed(&before, &added, 3), 1);
    // a point removed by another front end: the focus stays inside the table
    let gone = [slotted(1, 100.0), slotted(2, 1000.0)];
    assert_eq!(followed(&before, &gone, 2), 1);
}
