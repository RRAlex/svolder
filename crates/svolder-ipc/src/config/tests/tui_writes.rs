// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Saving `[tui]` settings: what the writers keep byte for byte, and what they change.

use super::*;

#[test]
fn saving_tui_settings_preserves_everything_else_byte_for_byte() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "# my devices\nschema = 1\nadapter = \"hci1\" # the dongle\n\n[[device]]\nname = \"bifrost\"\nudid = \"0102030405060708\"\n\n[tui]\naccent = \"blue\"\n";
    std::fs::write(&path, original).unwrap();
    let mut tui = Config::parse(original).unwrap().tui;
    tui.palette = "neon".into();
    tui.effects.shimmer = false;
    tui.intro = false;
    Config::update_tui(&path, |t| *t = tui.clone()).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    let (head, _) = saved.split_once("[tui]").unwrap();
    assert_eq!(head, original.split_once("[tui]").unwrap().0);
    let reread = Config::parse(&saved).unwrap().tui;
    assert_eq!(reread.palette, "neon");
    assert!(!reread.effects.shimmer && !reread.intro && reread.effects.eased);
}

/// The `[tui]` table is as often the first thing in a hand-written file as the last, and a
/// writer that appended its keys to the end of the document would swallow every table that
/// follows it. What is written lands inside `[tui]`, so the comment and the device after it
/// stay exactly where their author put them.
#[test]
fn saving_a_tui_table_that_comes_first_leaves_what_follows_it_alone() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "schema = 1\n\n[tui]\npalette = \"ice\"\n\n# my devices\n[[device]]\nname = \"bifrost\"\nudid = \"0102030405060708\"\n\n[device.inputs]\n3 = \"TV optical\"\n";
    std::fs::write(&path, original).unwrap();
    let mut tui = Config::parse(original).unwrap().tui;
    tui.monochrome = true;
    // An accent is written as a dotted key, among `[tui]`'s own keys.
    tui.accents.dac = Some("#445566".into());
    Config::update_tui(&path, |t| *t = tui.clone()).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    let (_, tail) = saved.split_once("# my devices").unwrap();
    assert_eq!(tail, original.split_once("# my devices").unwrap().1);
    assert!(saved.starts_with("schema = 1\n"), "{saved}");
    let (head, _) = saved.split_once("# my devices").unwrap();
    assert!(
        head.contains("\naccents.dac = \"#445566\"\n"),
        "the accent is not written dotted among [tui]'s keys:\n{saved}"
    );
    let reread = Config::parse(&saved).unwrap();
    assert!(reread.tui.monochrome && reread.tui.palette == "ice");
    assert_eq!(reread.tui.accents.dac.as_deref(), Some("#445566"));
    assert_eq!(
        reread.devices[0].inputs.get(&3).map(String::as_str),
        Some("TV optical")
    );
}

/// A key inside `[tui]` keeps what its author wrote around it: the comment above it and the
/// one after it on the line are the reason to hand-edit a config at all, and only the value
/// is the writer's to change. A key the file does not carry is appended plain.
#[test]
fn saving_keeps_the_comments_around_a_key_it_rewrites() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "schema = 1\n\n[tui]\n# I like ice\npalette = \"ice\" # the cool one\n";
    std::fs::write(&path, original).unwrap();
    let mut tui = Config::parse(original).unwrap().tui;
    tui.palette = "neon".into();
    Config::update_tui(&path, |t| *t = tui.clone()).unwrap();

    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(
        saved.contains("# I like ice\npalette = \"neon\" # the cool one\n"),
        "the comments around the key did not survive its new value:\n{saved}"
    );
    assert!(
        !saved.contains("intro"),
        "a key the change left alone is not appended:\n{saved}"
    );
    assert_eq!(Config::parse(&saved).unwrap().tui, tui);
}

#[test]
fn saving_creates_the_file_and_the_table_when_absent() {
    let dir = temp_dir("config-writer");
    let path = dir.join("fresh/config.toml");
    Config::update_tui(&path, |t| *t = TuiConfig::default()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\nschema = 1\n"), "{text}");
    assert!(text.contains("[tui]\n"), "{text}");
    assert!(Config::parse(&text).is_ok());
}

/// Every setting is written in the shape a hand edit would use and reads back as itself:
/// the effects as one word when they are all on or all off and as a table otherwise, an
/// accent per family only for the families that have one, and each colour depth as its own
/// name.
#[test]
fn every_tui_setting_round_trips_through_the_file() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    // A file with nothing but its schema, rather than the commented template, so that what
    // is asserted about is what the writer put there.
    std::fs::write(&path, "schema = 1\n").unwrap();
    let mut tui = TuiConfig::default();
    Config::update_tui(&path, |t| *t = tui.clone()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text, "schema = 1\n", "defaults over defaults write nothing");
    assert!(
        !text.contains("accent"),
        "nothing overrides an accent yet: {text}"
    );

    tui.palette = "red".into();
    tui.intro = false;
    tui.monochrome = true;
    tui.mouse = false;
    tui.colors = ColorDepth::Ansi256;
    tui.accents.amp = Some("#5fc9c9".into());
    tui.effects.shimmer = false;
    Config::update_tui(&path, |t| *t = tui.clone()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("shimmer = false"),
        "a part-switched set is a table: {text}"
    );
    assert_eq!(Config::parse(&text).unwrap().tui, tui, "{text}");

    tui.effects = EffectsConfig::off();
    tui.accents.amp = None;
    for depth in [
        ColorDepth::Auto,
        ColorDepth::Ansi16,
        ColorDepth::Ansi256,
        ColorDepth::TrueColor,
    ] {
        tui.colors = depth;
        Config::update_tui(&path, |t| *t = tui.clone()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("effects = false"), "{text}");
        assert!(
            !text.contains("amp"),
            "an accent nothing sets is no key at all: {text}"
        );
        assert_eq!(Config::parse(&text).unwrap().tui, tui, "{text}");
    }
}

/// A change writes only the key it touches, into the `[tui]` the file holds: the session's
/// own values for the other keys never reach it.
#[test]
fn update_tui_changes_one_key_of_the_files_own_table() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n[tui]\npalette = \"ice\"\n").unwrap();

    Config::update_tui(&path, |tui| tui.mouse = false).unwrap();

    let text = std::fs::read_to_string(&path).unwrap();
    let tui = Config::parse(&text).unwrap().tui;
    assert!(!tui.mouse);
    assert_eq!(tui.palette, "ice");
    assert!(
        tui.intro,
        "a key the change did not touch stays as the file had it"
    );
    assert!(!text.contains("intro = false"), "{text}");
}

/// A file that is gone comes back as the template with the one change, whatever the session
/// held.
#[test]
fn update_tui_on_a_missing_file_is_the_template_plus_the_change() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");

    Config::update_tui(&path, |tui| tui.mouse = false).unwrap();

    let written = Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut expected = Config::parse(&Config::template()).unwrap().tui;
    expected.mouse = false;
    assert_eq!(written.tui, expected);
    assert!(written.tui.intro);
    assert_eq!(written.tui.palette, TuiConfig::default().palette);
}

/// A bare `accent` the change replaces with a family's own is dropped from the file.
#[test]
fn update_tui_drops_a_bare_accent_for_a_family_accent() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n[tui]\naccent = \"#112233\"\n").unwrap();

    Config::update_tui(&path, |tui| {
        tui.accent = None;
        tui.accents.dac = Some("#445566".into());
    })
    .unwrap();

    let tui = Config::load_from(&path).unwrap().tui;
    assert_eq!(tui.accent, None);
    assert_eq!(tui.accents.dac.as_deref(), Some("#445566"));
}

/// A file that does not parse is left byte for byte as it was, and the call says so.
#[test]
fn update_tui_leaves_a_file_that_does_not_parse_alone() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "schema = 1\n[tui\npalette = \n";
    std::fs::write(&path, original).unwrap();

    let err = Config::update_tui(&path, |tui| tui.mouse = false).unwrap_err();

    assert!(matches!(err, ConfigError::Parse(_)), "got {err:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
}

/// A key the change does not touch keeps every byte its author wrote: the quote style of a
/// bare `accent` and a multi-line `stops` array with its comment survive a `mouse` change.
#[test]
fn update_tui_leaves_every_other_byte_of_the_file_alone() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "schema = 1\n\n[tui]\naccent = '#112233'\nmouse = true\n\n[tui.spectrum]\nstops = [\n    \"#5a1a08\", # dark\n    \"#ffffff\",\n]\n";
    std::fs::write(&path, original).unwrap();

    Config::update_tui(&path, |tui| tui.mouse = false).unwrap();

    let expected = original.replace("mouse = true", "mouse = false");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
}

/// A file with no `[tui]` gets one holding the changed key alone.
#[test]
fn update_tui_adds_a_table_holding_the_changed_key_alone() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();

    Config::update_tui(&path, |tui| tui.mouse = false).unwrap();

    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "schema = 1\n\n[tui]\nmouse = false\n"
    );
}

/// An edit that changes nothing leaves the file byte for byte, a missing `[tui]` included.
#[test]
fn update_tui_with_no_change_leaves_the_file_byte_for_byte() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "schema = 1\n# mine\n[tui]\npalette = 'ice'\n";
    std::fs::write(&path, original).unwrap();
    Config::update_tui(&path, |_| {}).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);

    std::fs::write(&path, "schema = 1\n").unwrap();
    Config::update_tui(&path, |_| {}).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "schema = 1\n");
}

/// Inside a `[tui.effects]` table, toggling one switch rewrites that switch's line alone.
#[test]
fn update_tui_rewrites_one_switch_of_an_effects_table() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original =
        "schema = 1\n[tui.effects]\nenabled = true\nglow = true # pulse\nshimmer = true\n";
    std::fs::write(&path, original).unwrap();

    Config::update_tui(&path, |tui| tui.effects.glow = false).unwrap();

    let expected = original.replace("glow = true", "glow = false");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
}

/// Flipping `glow` in `original` through `update_tui` changes that one line and nothing else.
fn assert_only_glow_changes(original: &str) {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, original).unwrap();

    Config::update_tui(&path, |tui| tui.effects.glow = false).unwrap();

    let saved = std::fs::read_to_string(&path).unwrap();
    assert_eq!(saved, original.replace("glow = true", "glow = false"));
    assert!(!Config::parse(&saved).unwrap().tui.effects.glow);
}

/// An `[tui.effects]` table beside the template's `[tui.accents]` header: the switch changes
/// and the accents table stays as written.
#[test]
fn update_tui_rewrites_a_switch_beside_an_accents_header() {
    assert_only_glow_changes(
        "schema = 1\n[tui.effects]\nglow = true\n[tui.accents]\ndac = \"#112233\"\n",
    );
}

/// The same switch change beside an inline `accents` table.
#[test]
fn update_tui_rewrites_a_switch_beside_an_inline_accents_table() {
    assert_only_glow_changes(
        "schema = 1\n[tui]\naccents = { dac = \"#112233\" }\n[tui.effects]\nglow = true\n",
    );
}

/// The same switch change beside a top-level dotted `tui.accent`.
#[test]
fn update_tui_rewrites_a_switch_beside_a_dotted_accent() {
    assert_only_glow_changes("schema = 1\ntui.accent = \"#112233\"\n[tui.effects]\nglow = true\n");
}

/// An `update_tui` that changes nothing leaves the file alone: a symbolic `config.toml` stays a
/// link and the bytes stay.
#[test]
fn a_no_op_update_tui_leaves_a_symlinked_config_a_link() {
    let dir = temp_dir("config-writer");
    let real = dir.join("real.toml");
    let link = dir.join("config.toml");
    std::fs::write(&real, "schema = 1\n[tui]\n# a comment\n").unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();

    Config::update_tui(&link, |_| {}).unwrap();

    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(
        std::fs::read_to_string(&real).unwrap(),
        "schema = 1\n[tui]\n# a comment\n"
    );
}

/// Turning the list fade off writes `list_fade` alone: into the `[tui.effects]` table when the
/// file has one, as the `effects` word rewritten whole when it has not.
#[test]
fn saving_the_list_fade_writes_only_that_switch() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n[tui.effects]\nglow = false\n").unwrap();
    Config::update_tui(&path, |t| t.effects.list_fade = false).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(
        saved.contains("glow = false") && saved.contains("list_fade = false"),
        "{saved}"
    );
    assert!(!saved.contains("shimmer"), "{saved}");

    std::fs::write(&path, "schema = 1\n[tui]\neffects = true\n").unwrap();
    Config::update_tui(&path, |t| t.effects.list_fade = false).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("list_fade = false"), "{saved}");
    assert!(!Config::parse(&saved).unwrap().tui.effects.list_fade);
    assert!(Config::parse(&saved).unwrap().tui.effects.glow);
}
