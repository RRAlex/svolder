// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The licences that allow any use: the only ones a catalogue entry may carry.
const ANY_USE: [&str; 5] = [
    "Apache-2.0",
    "CC-BY-4.0",
    "CC-BY-SA-3.0",
    "LicenseRef-MIT-Media-Lab-free",
    "LicenseRef-CIPIC-any-purpose",
];

#[test]
fn ids_are_unique_and_safe_as_file_names() {
    let mut seen = std::collections::HashSet::new();
    for entry in entries() {
        assert!(seen.insert(entry.id), "{} twice", entry.id);
        assert!(crate::store::valid_id(entry.id), "{}", entry.id);
        assert_eq!(find(entry.id), Some(entry));
    }
    assert_eq!(find("nope"), None);
}

#[test]
fn every_entry_is_pinned_https_and_within_the_download_cap() {
    for entry in entries() {
        assert!(entry.url.starts_with("https://"), "{}", entry.id);
        assert_eq!(entry.sha256.len(), 64, "{}", entry.id);
        assert!(
            entry
                .sha256
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert!(
            entry.bytes > 0 && entry.bytes <= 64 * 1024 * 1024,
            "{}",
            entry.id
        );
        assert!(entry.source_page.starts_with("https://"), "{}", entry.id);
        assert!(!entry.label.is_empty() && !entry.credit.is_empty());
    }
}

#[test]
fn only_any_use_licences_are_listed() {
    for entry in entries() {
        assert!(
            ANY_USE.contains(&entry.licence),
            "{}: {}",
            entry.id,
            entry.licence
        );
        assert!(!entry.licence.contains("NC"), "{}", entry.id);
    }
}

#[test]
fn a_zip_entry_names_a_sofa_member_and_the_rooms_match_the_angles() {
    for entry in entries() {
        if let Container::Zip { member } = entry.container {
            assert!(member.ends_with(".sofa"), "{}", entry.id);
        }
        assert_eq!(
            entry.right_az,
            360.0 - entry.left_az,
            "{}: mirrored speakers",
            entry.id
        );
        assert!(entry.recipe().tolerance <= 10.0);
    }
    let with: Vec<_> = entries().iter().filter(|e| e.room == Room::With).collect();
    let without = entries().iter().filter(|e| e.room == Room::No).count();
    assert!(!with.is_empty() && without >= 5);
    assert!(
        with.iter()
            .all(|e| matches!(e.container, Container::Zip { .. }))
    );
}

/// The makers and products no text may name, as the SHA-256 of each lower-case word with its
/// length in characters, so the list names none of them itself.
const BANNED: [(usize, &str); 7] = [
    (
        5,
        "53558b8b04e008d575bc0d1b114f2479762ea3f33d02a6fe307e15417526675e",
    ),
    (
        7,
        "282f8f55fc677f237c05eb4b7d6620ce35139a49681083ae438d24dd40437771",
    ),
    (
        4,
        "1a51fc091b8e29f527a3cae718b517d29177262297a8aaa777c63cf310758191",
    ),
    (
        10,
        "f30a32ce3bf65d4a26a134be501cf8bb7671f16dc8a96ff6b7d3f5c78a895331",
    ),
    (
        7,
        "e9139f2de9972a7db710559cc3903fc587f9583ca96ddaaff762faaf7c9b0040",
    ),
    (
        14,
        "400362acf1ea46d2af00243ff3f7bae36322948b323a15baa1c9a701d0b14d8e",
    ),
    (
        6,
        "279814ddd5e95844fcbfd5d4cc4fe9840c7050170802ce6f2b848568f8464dac",
    ),
];

/// Whether `text`, in lower case, holds a run of characters whose length and SHA-256 are one of
/// `banned`'s.
fn names_any<S: AsRef<str>>(text: &str, banned: &[(usize, S)]) -> bool {
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    banned.iter().any(|(len, hash)| {
        chars.windows(*len).any(|run| {
            crate::sha::sha256_hex(run.iter().collect::<String>().as_bytes()) == hash.as_ref()
        })
    })
}

#[test]
fn no_product_or_maker_is_named_in_any_text_the_person_reads() {
    let dummy = [(5, crate::sha::sha256_hex(b"dummy"))];
    assert!(names_any("A Dummy head", &dummy), "the guard finds a word");
    assert!(!names_any("a dumm head", &dummy));
    for entry in entries() {
        for text in [entry.label, entry.credit, entry.id] {
            assert!(!names_any(text, &BANNED), "{}: {text}", entry.id);
        }
    }
}

#[test]
fn the_choices_carry_their_technical_terms() {
    assert_eq!(
        (Room::With.label(), Room::With.term(), Room::With.wire()),
        ("With room", "BRIR", "with")
    );
    assert_eq!(
        (Room::No.label(), Room::No.term(), Room::No.wire()),
        ("No room", "HRTF", "no")
    );
}
