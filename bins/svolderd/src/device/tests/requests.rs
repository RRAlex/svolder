// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn the_firmware_family_and_the_passphrase_function_are_refused() {
    let refused = refuse(FunctionId::two(210, 1)).expect("210.1 is restricted");
    assert!(
        matches!(&refused, Response::Error { code: CODE_USAGE, message, .. } if message.contains("firmware")),
        "got {refused:?}"
    );
    let sensitive = refuse(FunctionId::one(250)).expect("250 is sensitive");
    assert!(
        matches!(&sensitive, Response::Error { code: CODE_USAGE, message, .. }
                if message == "function 250 changes the passphrase; use `svolder passphrase set`"),
        "got {sensitive:?}"
    );
    assert!(refuse(FunctionId::one(10)).is_none(), "a normal id passes");
    assert!(
        refuse(FunctionId::three(210, 2, 1)).is_none(),
        "the version query is not restricted"
    );
}

#[test]
fn raw_requests_parse_the_way_the_cli_parses_them() {
    let m = build_raw("2", "u8", "4").expect("a valid raw request");
    assert_eq!(m.id, FunctionId::one(2));
    assert_eq!(m.value, Value::U8(4));
    for (id, ty, value) in [("nope", "u8", "1"), ("2", "u9", "1"), ("2", "u8", "300")] {
        let err = build_raw(id, ty, value).unwrap_err();
        assert_eq!(err.code, 1, "{id} {ty} {value}: {}", err.message);
    }
}

#[test]
fn link_failures_map_to_the_codes_the_cli_exits_with() {
    assert_eq!(code_for(&BleError::NotFound), CODE_NOT_FOUND);
    assert_eq!(code_for(&BleError::Connection("x".into())), CODE_LINK);
    assert_eq!(code_for(&BleError::Bootloader), CODE_LINK);
    assert_eq!(code_for(&BleError::PassphraseRejected), CODE_LINK);
    assert_eq!(code_for(&BleError::Adapter("x".into())), CODE_LINK);
    assert_eq!(
        code_for(&BleError::Timeout(Duration::from_secs(1))),
        CODE_TIMEOUT
    );
    assert_eq!(
        code_for(&BleError::Proto(ProtoError::BadUnitId)),
        CODE_USAGE
    );
    assert_eq!(code_for(&BleError::Profile(ProfileError::NoEq)), CODE_USAGE);
}

/// The daemon refuses a device with no tone control in the profile crate's own words, the
/// ones the CLI's direct path shows too.
#[test]
fn no_tone_control_is_refused_in_the_profile_crates_words() {
    let said = ProfileError::NoEq.to_string();
    assert!(
        matches!(no_eq(), Response::Error { code: CODE_USAGE, ref message, .. } if *message == said),
        "got {:?}",
        no_eq()
    );
}
