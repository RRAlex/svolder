// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use svolder_proto::UnitId;

/// An advertisement whose passphrase field carries `code` (`None` for the raw value 0).
fn advert(code: Option<&str>) -> ManufacturerData {
    let unit = UnitId::new([1, 2, 3, 4, 5, 6, 7, 8]);
    let encrypted = match code {
        Some(code) => Passphrase::encrypt(
            Passphrase::from_text(code).unwrap().encoded(),
            Passphrase::advert_key(unit),
        ),
        None => 0,
    };
    ManufacturerData {
        product_id: 2,
        incomplete_dfu: false,
        unit_id: unit,
        encrypted_passphrase: encrypted,
        firmware_build: 3,
        module_hardware: 1,
        module_firmware: 7,
    }
}

/// Every pairing of a configured value with an advertisement gets the verdict the design
/// gives it: `auto` never a note, `none` against a code, digits that differ, digits against no
/// code, and nothing where the advertisement says nothing that decodes.
#[test]
fn the_verdict_of_every_configured_value_against_every_advertisement() {
    use PassphraseNote::{Differs, NoCodeAdvertised, NoneConfigured};
    let table = [
        ("auto", Some("1234"), None),
        ("auto", None, None),
        ("none", Some("1234"), Some(NoneConfigured)),
        ("none", None, None),
        ("1234", Some("1234"), None),
        ("1234", Some("4321"), Some(Differs)),
        ("0000", Some("0001"), Some(Differs)),
        ("1234", None, Some(NoCodeAdvertised)),
    ];
    for (configured, advertised, want) in table {
        assert_eq!(
            passphrase_verdict(configured, &advert(advertised)),
            want,
            "configured {configured:?} against {advertised:?}"
        );
    }
}

/// A raw advertised value that does not decode as a code earns no verdict, whatever the entry
/// says, and neither does an entry that is none of `auto`, `none` or four digits.
#[test]
fn a_value_that_decodes_to_nothing_earns_no_verdict() {
    let mut garbled = advert(Some("1234"));
    // Any raw value whose decoding is not a well-formed code.
    garbled.encrypted_passphrase = 0x0000_0001;
    assert!(!garbled.passphrase().is_none() && !garbled.passphrase().is_well_formed());
    for configured in ["none", "1234"] {
        assert_eq!(passphrase_verdict(configured, &garbled), None);
    }
    assert_eq!(passphrase_verdict("12", &advert(Some("1234"))), None);
}

/// The words never carry a digit, and the line names the device and the fix.
#[test]
fn the_words_carry_no_digits_and_the_line_names_the_device() {
    for note in [
        PassphraseNote::Differs,
        PassphraseNote::NoneConfigured,
        PassphraseNote::NoCodeAdvertised,
    ] {
        assert!(!note.line("amp").chars().any(|c| c.is_ascii_digit()));
        assert!(note.line("amp").starts_with("amp: "));
        assert!(note.line("amp").contains("`auto`"));
    }
}

/// The note travels as a plain word, so an older peer that reads it as an unknown field and a
/// newer one that reads it agree.
#[test]
fn the_note_travels_as_a_word() {
    assert_eq!(
        serde_json::to_string(&PassphraseNote::NoCodeAdvertised).unwrap(),
        r#""no_code_advertised""#
    );
}

/// A note a newer daemon invents decodes as the unknown one, so the rest of the reply still
/// decodes, and its words stay neutral and carry the `auto` fix.
#[test]
fn a_note_from_a_newer_daemon_decodes_as_unknown() {
    let note: PassphraseNote = serde_json::from_str(r#""stale_advertisement""#).unwrap();
    assert_eq!(note, PassphraseNote::Unknown);
    assert!(note.words().contains("may not match"));
    assert!(note.line("amp").contains("`auto`"));
    assert!(!note.line("amp").chars().any(|c| c.is_ascii_digit()));
}
