// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn pk(f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter {
        kind: FilterKind::Peaking,
        f,
        g,
        q,
    }
}

fn state(filters: Vec<SourceFilter>, slots: Vec<u8>) -> HostEqState {
    HostEqState {
        on: true,
        name: "x".into(),
        origin: None,
        target: "sink".into(),
        route: super::super::Route::Smart,
        manager: super::super::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters,
        slots,
    }
}

#[test]
fn points_keep_their_slots_when_they_pass_each_other() {
    let mut points = Points::from_state(&state(
        vec![pk(100.0, 1.0, 1.0), pk(1000.0, 2.0, 1.0)],
        vec![1, 2],
    ));
    points
        .set(0, None, Some(5000.0), None, None)
        .expect("point 1 moves");
    assert!(points.filters[0].f > points.filters[1].f);
    assert_eq!(points.slots, vec![1, 2]);
}

#[test]
fn an_added_point_takes_the_lowest_free_slot() {
    let mut points = Points::from_state(&state(
        vec![
            pk(100.0, 0.0, 1.0),
            pk(200.0, 0.0, 1.0),
            pk(300.0, 0.0, 1.0),
        ],
        vec![1, 2, 3],
    ));
    assert_eq!(points.remove(1), Ok(2));
    let index = points.add(pk(400.0, 0.0, 1.0)).expect("there is room");
    assert_eq!(index, 2);
    assert_eq!(points.slots, vec![1, 3, 2]);
    points.add(pk(500.0, 0.0, 1.0)).expect("there is room");
    assert_eq!(points.slots, vec![1, 3, 2, 4]);
}

#[test]
fn the_seventeenth_point_is_refused() {
    let mut points = Points::lone();
    for i in 1..16 {
        points.add(pk(100.0 * i as f64, 0.0, 1.0)).expect("room");
    }
    assert_eq!(points.filters.len(), 16);
    assert_eq!(
        points.add(pk(900.0, 0.0, 1.0)),
        Err("the computer EQ holds 16 points".to_string())
    );
    assert_eq!(points.filters.len(), 16);
}

#[test]
fn the_lone_point_is_set_flat_by_a_remove() {
    let mut points = Points::lone();
    points.set(0, None, None, Some(5.0), Some(2.0)).unwrap();
    assert_eq!(points.remove(0), Ok(1));
    assert_eq!(points.filters.len(), 1);
    assert_eq!(
        (
            points.filters[0].f,
            points.filters[0].g,
            points.filters[0].q
        ),
        (1000.0, 0.0, 2.0)
    );
}

#[test]
fn a_state_in_any_order_reads_sorted_with_its_slots() {
    let points = Points::from_state(&state(
        vec![
            pk(226.0, 1.0, 1.0),
            pk(8000.0, 2.0, 1.0),
            pk(2000.0, 3.0, 1.0),
        ],
        vec![1, 2, 3],
    ));
    let order: Vec<f64> = points.filters.iter().map(|f| f.f).collect();
    assert_eq!(order, vec![226.0, 2000.0, 8000.0]);
    assert_eq!(points.slots, vec![1, 3, 2]);
}

#[test]
fn editing_one_field_leaves_the_others_as_written() {
    let shelf = SourceFilter {
        kind: FilterKind::LowShelf,
        f: 105.0,
        g: 14.0,
        q: 0.7,
    };
    let mut points = Points::from_state(&state(vec![shelf], vec![1]));
    let change = points.set(0, None, Some(200.0), None, None).expect("set");
    assert_eq!(change, Change::Values);
    assert_eq!(points.filters[0].g, 14.0, "the gain stays as written");
    assert_eq!(points.filters[0].q, 0.7);
    assert_eq!(points.filters[0].f, 200.0);
    points.set(0, None, None, Some(30.0), None).expect("set");
    assert_eq!(
        points.filters[0].g, 12.0,
        "a given field is held to the hand's range"
    );
    points
        .set(0, None, Some(5.0), None, Some(50.0))
        .expect("set");
    assert_eq!((points.filters[0].f, points.filters[0].q), (20.0, 10.0));
}

#[test]
fn a_new_type_is_a_kind_change() {
    let mut points = Points::lone();
    let change = points
        .set(0, Some(FilterKind::HighShelf), None, None, None)
        .expect("set");
    assert_eq!(change, Change::Kind);
    assert_eq!(points.filters[0].kind, FilterKind::HighShelf);
    assert!(points.set(0, None, Some(f64::NAN), None, None).is_err());
    assert!(points.set(3, None, None, None, None).is_err());
}

#[test]
fn an_old_state_file_reads_with_slots_one_to_n() {
    let text = "on = true\nname = \"x\"\ntarget = \"sink\"\nroute = \"smart\"\nmanager = \"child\"\npreamp_db = -1.0\n\n[[filters]]\nkind = \"Peaking\"\nf = 100.0\ng = 3.0\nq = 1.0\n\n[[filters]]\nkind = \"LowShelf\"\nf = 50.0\ng = 2.0\nq = 0.7\n";
    let old: HostEqState = toml::from_str(text).expect("an old state reads");
    assert!(old.slots.is_empty());
    let points = Points::from_state(&old);
    // The file lists 100 Hz then 50 Hz in slots 1 and 2; the points read by frequency.
    assert_eq!(points.slots, vec![2, 1]);
    assert_eq!(points.filters[0].f, 50.0);
    assert_eq!(points.filters.len(), 2);
    // Slots that are not each of 1 to 16 once are not trusted.
    let broken = Points::from_state(&state(
        vec![pk(1.0, 0.0, 1.0), pk(2.0, 0.0, 1.0)],
        vec![3, 3],
    ));
    assert_eq!(broken.slots, vec![1, 2]);
    // No filters at all is the lone point.
    assert_eq!(
        Points::from_state(&state(Vec::new(), Vec::new())),
        Points::lone()
    );
}

#[test]
fn the_correction_carries_the_stated_preamp() {
    let points = Points::lone();
    let correction = points.correction(-3.0);
    assert_eq!(correction.preamp_db, -3.0);
    assert_eq!(correction.filters, points.filters);
}
