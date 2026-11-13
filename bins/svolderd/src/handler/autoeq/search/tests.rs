// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::SystemTime;

use super::*;

fn entry(name: &str, source: &str, rig: Option<&str>) -> Entry {
    Entry {
        name: name.into(),
        source: source.into(),
        form: "over-ear".into(),
        rig: rig.map(str::to_string),
    }
}

fn index(entries: Vec<Entry>) -> Index {
    Index {
        entries,
        fetched: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000),
        note: Some("cached".into()),
    }
}

/// Names equal as lowercase letters and digits are one headphone, however their case or spacing
/// differs; each source stays under it, in the index's order.
#[test]
fn measurements_group_by_the_plain_name() {
    let found = answer(
        &index(vec![
            entry("Sennheiser HD 650", "oratory1990", None),
            entry("Sennheiser HD 600", "oratory1990", None),
            entry("sennheiser hd-650", "crinacle", Some("GRAS")),
            entry("Sennheiser HD 650", "Rtings", None),
        ]),
        "hd650",
    );
    assert_eq!(found.groups.len(), 1);
    let group = &found.groups[0];
    assert_eq!(group.key, "sennheiserhd650");
    assert_eq!(group.name, "Sennheiser HD 650");
    let words: Vec<_> = group.variants.iter().map(|v| v.words.as_str()).collect();
    assert_eq!(
        words,
        [
            "oratory1990 · over-ear",
            "crinacle · over-ear · GRAS",
            "Rtings · over-ear"
        ],
        "the index's order, though the names are spelled differently"
    );
    assert_eq!(found.measurements, 4);
    assert_eq!(found.fetched_unix, 1000);
    assert_eq!(found.note.as_deref(), Some("cached"));
    assert_eq!(found.more, 0);
}

#[test]
fn the_groups_are_capped_and_the_rest_counted() {
    let entries = (0..AUTOEQ_GROUPS_MAX + 30)
        .map(|n| entry(&format!("Phone {n}"), "oratory1990", None))
        .collect();
    let found = answer(&index(entries), "phone");
    assert_eq!(found.groups.len(), AUTOEQ_GROUPS_MAX);
    assert_eq!(found.more, 30);
}

/// Groups past the byte bound are counted, not sent, so the answer always fits a line.
#[test]
fn the_answer_is_trimmed_to_the_line() {
    let long = "x".repeat(200);
    let entries = (0..AUTOEQ_GROUPS_MAX)
        .flat_map(|n| {
            (0..20).map(move |m| {
                entry(
                    &format!("Phone {n} {}", "y".repeat(80)),
                    &format!("s{m}{}", "z".repeat(80)),
                    Some("r"),
                )
            })
        })
        .chain(std::iter::once(entry(&long, "a", None)))
        .collect();
    let found = answer(&index(entries), "phone");
    let line = serde_json::to_string(&svolder_ipc::Response::AutoEqSearch(Box::new(
        found.clone(),
    )))
    .unwrap();
    assert!(line.len() < 1024 * 1024, "{}", line.len());
    assert!(found.more > 0, "something was trimmed");
}

#[test]
fn a_variant_is_found_by_all_its_fields() {
    let e = entry("A", "s", Some("r"));
    let v = variant_of(&e);
    assert!(is(&e, &v));
    assert!(!is(&entry("A", "s", None), &v));
}
