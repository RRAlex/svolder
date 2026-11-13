// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The search answer: the matching measurements grouped by headphone, each source under it.

use std::collections::HashMap;
use std::time::UNIX_EPOCH;

use svolder_autoeq::{Entry, Index, search};
use svolder_ipc::{AUTOEQ_GROUPS_MAX, AutoEqGroup, AutoEqMeasurement, AutoEqSearch, AutoEqVariant};

/// The most bytes of groups an answer holds, as JSON: the line a response is sent in is 1 MiB,
/// and the answer's other fields and the envelope are far below the rest.
const GROUPS_MAX_BYTES: usize = 900 * 1024;

/// The measurement `entry` as it is named on the wire.
pub(super) fn variant_of(entry: &Entry) -> AutoEqVariant {
    AutoEqVariant {
        name: entry.name.clone(),
        source: entry.source.clone(),
        form: entry.form.clone(),
        rig: entry.rig.clone(),
    }
}

/// Whether `entry` is the measurement `variant` names.
pub(super) fn is(entry: &Entry, variant: &AutoEqVariant) -> bool {
    entry.name == variant.name
        && entry.source == variant.source
        && entry.form == variant.form
        && entry.rig == variant.rig
}

/// The answer to a search for `query` in `index`: the matches in the search's order, one group
/// per [`Entry::group_key`] (the first match of a key places it, its measurements in the index's
/// order, which puts AutoEq's recommended one first), at most [`AUTOEQ_GROUPS_MAX`] groups and
/// [`GROUPS_MAX_BYTES`] of them, the rest counted.
pub(super) fn answer(index: &Index, query: &str) -> AutoEqSearch {
    // Where each entry stands in the index: the search ranks names, and the measurements of one
    // headphone are told in the index's own order whatever spelling their names have.
    let position: HashMap<*const Entry, usize> = index
        .entries
        .iter()
        .enumerate()
        .map(|(at, entry)| (std::ptr::from_ref(entry), at))
        .collect();
    let mut groups: Vec<AutoEqGroup> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    let ranked = search(&index.entries, query);
    let mut place: HashMap<String, usize> = HashMap::new();
    for (n, entry) in ranked.iter().enumerate() {
        place.entry(entry.group_key()).or_insert(n);
    }
    let mut found = ranked;
    found.sort_by_key(|entry| {
        (
            place[&entry.group_key()],
            position[&std::ptr::from_ref(*entry)],
        )
    });
    for entry in found {
        let key = entry.group_key();
        let measurement = AutoEqMeasurement {
            variant: variant_of(entry),
            words: entry.measurement(),
        };
        match at.get(&key) {
            Some(&found) => groups[found].variants.push(measurement),
            None => {
                at.insert(key.clone(), groups.len());
                groups.push(AutoEqGroup {
                    key,
                    name: entry.name.clone(),
                    variants: vec![measurement],
                });
            }
        }
    }
    let total = groups.len();
    let mut bytes = 0usize;
    let keep = groups
        .iter()
        .take(AUTOEQ_GROUPS_MAX)
        .take_while(|group| {
            bytes = bytes
                .saturating_add(serde_json::to_string(group).map_or(usize::MAX, |text| text.len()));
            bytes <= GROUPS_MAX_BYTES
        })
        .count();
    groups.truncate(keep);
    AutoEqSearch {
        groups,
        more: total - keep,
        measurements: index.entries.len(),
        fetched_unix: index
            .fetched
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs()),
        note: index.note.clone(),
    }
}

#[cfg(test)]
mod tests;
