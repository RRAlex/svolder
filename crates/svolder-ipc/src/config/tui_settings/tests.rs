// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::super::*;
use crate::test_support::{TempPath, temp_dir};

/// One key of the table with a value other than its default: the TOML text of the default,
/// and of the new value.
struct Case {
    key: &'static TuiSetting,
    old: String,
    new: String,
}

impl Case {
    /// Set the new value in `tui`.
    fn edit(&self, tui: &mut TuiConfig) {
        match self.key.kind {
            TuiSettingKind::Accent => tui.accent = Some(ACCENT_COLOUR.into()),
            _ => self.key.apply(tui, self.value()),
        }
    }

    /// The new value, for a key that takes one through [`TuiSetting::apply`].
    fn value(&self) -> TuiValue {
        let default = TuiConfig::default();
        match self.key.kind {
            TuiSettingKind::Bool { get, .. } => TuiValue::Bool(!get(&default)),
            TuiSettingKind::Word { get, words, .. } => TuiValue::Word(
                words()
                    .into_iter()
                    .find(|word| *word != get(&default))
                    .expect("a list of words has one besides the default"),
            ),
            TuiSettingKind::Accent => unreachable!("the accent is not applied"),
        }
    }
}

/// The accent the `accent` case sets.
const ACCENT_COLOUR: &str = "#e0341c";

/// Every key of the table, in the table's order, each with a non-default value.
fn cases() -> Vec<Case> {
    let default = TuiConfig::default();
    TUI_SETTINGS
        .iter()
        .map(|key| {
            let toml = |tui: &TuiConfig| match key.kind {
                TuiSettingKind::Bool { get, .. } => get(tui).to_string(),
                TuiSettingKind::Word { get, .. } => format!("{:?}", get(tui)),
                TuiSettingKind::Accent => format!("{:?}", tui.accent.as_deref().unwrap_or("")),
            };
            let mut case = Case {
                key,
                old: toml(&default),
                new: String::new(),
            };
            let mut changed = default.clone();
            case.edit(&mut changed);
            case.new = toml(&changed);
            assert_ne!(case.old, case.new, "{}", key.key);
            case
        })
        .collect()
}

/// Run `case`'s change against the file at `path`.
fn update(path: &std::path::Path, case: &Case) {
    Config::update_tui(path, |tui| case.edit(tui)).expect("the change is written");
}

/// The file's text after a change, which also parses back to what the file held with the one
/// value set.
fn written(dir: &TempPath, before: Option<&str>, case: &Case) -> String {
    let path = dir.join(format!("{}.toml", case.key.key));
    if let Some(text) = before {
        std::fs::write(&path, text).expect("the file is written");
    }
    let mut expected = match before {
        Some(text) => Config::parse(text).expect("the file parses").tui,
        None => TuiConfig::default(),
    };
    case.edit(&mut expected);
    update(&path, case);
    let text = std::fs::read_to_string(&path).expect("the file is read");
    let parsed = Config::parse(&text).unwrap_or_else(|e| panic!("{}: {e}\n{text}", case.key.key));
    assert_eq!(parsed.tui, expected, "{}\n{text}", case.key.key);
    text
}

/// Into a file that says only `schema = 1`, a key lands in a `[tui]` table made for it, as
/// `schema = 1`, a blank line, `[tui]` and `key = value`.
#[test]
fn a_key_lands_in_a_new_table_of_a_bare_file() {
    let dir = temp_dir("tui-settings");
    for case in cases() {
        let text = written(&dir, Some("schema = 1\n"), &case);
        let expected = format!("schema = 1\n\n[tui]\n{} = {}\n", case.key.key, case.new);
        assert_eq!(text, expected, "{}", case.key.key);
    }
}

/// The template with `case`'s commented line, `# key = default`, written live as `key = value`
/// where it stood.
fn template_with(case: &Case) -> String {
    let line = format!("# {} = {}\n", case.key.key, case.old);
    let template = Config::template();
    assert_eq!(template.matches(&line).count(), 1, "{}", case.key.key);
    template.replace(&line, &format!("{} = {}\n", case.key.key, case.new))
}

/// Into the template, the key's commented line is replaced where it stands, and nothing else
/// changes.
#[test]
fn a_key_replaces_its_commented_line_in_the_template() {
    let dir = temp_dir("tui-settings");
    for case in cases() {
        let text = written(&dir, Some(&Config::template()), &case);
        assert_eq!(text, template_with(&case), "{}", case.key.key);
    }
}

/// A file that is not there yet starts from the template, with the one key changed.
#[test]
fn a_missing_file_gets_the_template_with_the_one_change() {
    let dir = temp_dir("tui-settings");
    for case in cases() {
        let text = written(&dir, None, &case);
        assert_eq!(text, template_with(&case), "{}", case.key.key);
    }
}

/// Keys other than `key`, written out of the template's order, with `effects` in one word.
fn others(key: &str) -> (String, String) {
    let first = ["transition = \"noise\" # mine", "mouse = false"];
    let second = ["graph_span = \"2m\"", "effects = false # one word"];
    let pick = |lines: [&str; 2]| {
        lines
            .into_iter()
            .find(|line| !line.starts_with(key))
            .expect("one of two lines names another key")
            .to_string()
    };
    (pick(first), pick(second))
}

/// In a hand-written file, a key that is present with a comment after it, among keys out of
/// the template's order, changes its value and nothing else.
#[test]
fn a_hand_written_file_changes_only_the_one_value() {
    let dir = temp_dir("tui-settings");
    for case in cases() {
        let (first, second) = others(case.key.key);
        let before = format!(
            "# mine\nschema = 1\n\n[tui]\n{first}\n{} = {} # about {}\n{second}\n",
            case.key.key, case.old, case.key.key
        );
        let expected = before.replace(
            &format!("{} = {} #", case.key.key, case.old),
            &format!("{} = {} #", case.key.key, case.new),
        );
        let text = written(&dir, Some(&before), &case);
        assert_eq!(text, expected, "{}", case.key.key);
    }
}

/// A file with its own `[tui.effects]` table after the key keeps the table byte for byte when
/// the key changes.
#[test]
fn an_effects_table_after_the_key_stays_as_written() {
    let dir = temp_dir("tui-settings");
    for case in cases() {
        let before = format!(
            "schema = 1\n[tui]\n{} = {}\n\n[tui.effects]\nglow = false # quiet\n",
            case.key.key, case.old
        );
        let expected = before.replace(
            &format!("{} = {}\n", case.key.key, case.old),
            &format!("{} = {}\n", case.key.key, case.new),
        );
        let text = written(&dir, Some(&before), &case);
        assert_eq!(text, expected, "{}", case.key.key);
    }
}

/// Every key of the table, set to a value other than its default in `[tui]`, reads as that
/// value, and the rest of the table stays at its defaults.
#[test]
fn every_key_parses_to_the_value_it_is_set_to() {
    for case in cases() {
        let text = format!("schema = 1\n[tui]\n{} = {}\n", case.key.key, case.new);
        let tui = Config::parse(&text).expect("the file parses").tui;
        let mut expected = TuiConfig::default();
        case.edit(&mut expected);
        assert_eq!(tui, expected, "{}", case.key.key);
        assert_ne!(tui, TuiConfig::default(), "{}", case.key.key);
    }
}

/// A key the file leaves out reads as the default's own value.
#[test]
fn an_absent_key_reads_as_the_default() {
    for text in ["schema = 1\n", "schema = 1\n[tui]\n"] {
        assert_eq!(
            Config::parse(text).expect("the file parses").tui,
            TuiConfig::default()
        );
    }
}

/// With several keys wrong at once, the first error is the palette's, then the transition's,
/// the graph span's and the link span's: the order the keys are checked in.
#[test]
fn the_first_wrong_key_is_the_one_the_error_names() {
    let mut text = String::from(
        "schema = 1\n[tui]\npalette = \"x\"\ntransition = \"x\"\ngraph_span = \"x\"\nlink_span = \"x\"\n",
    );
    for (wrong, message) in [
        (
            "palette = \"x\"",
            "tui.palette must be one of amber, ice, red, neon",
        ),
        (
            "transition = \"x\"",
            "tui.transition must be one of none, trace, noise, melt",
        ),
        (
            "graph_span = \"x\"",
            "tui.graph_span must be one of 1m, 2m, 3m, 5m, 10m, 15m, 30m, 60m",
        ),
        (
            "link_span = \"x\"",
            "tui.link_span must be one of 1h, 3h, 6h, 12h, 24h",
        ),
    ] {
        match Config::parse(&text) {
            Err(ConfigError::Parse(found)) => assert_eq!(found, message),
            other => panic!("expected a parse error, got {other:?}"),
        }
        text = text.replace(&format!("{wrong}\n"), "");
    }
    assert!(Config::parse(&text).is_ok());
}

/// A word where a switch belongs is toml's own error, with its line, column and excerpt.
#[test]
fn a_word_for_a_switch_is_tomls_own_error() {
    match Config::parse("schema = 1\n[tui]\ntitle_meter = \"yes\"\n") {
        Err(ConfigError::Parse(found)) => assert_eq!(
            found,
            "TOML parse error at line 3, column 15\n  |\n3 | title_meter = \"yes\"\n  |               ^^^^^\ninvalid type: string \"yes\", expected a boolean\n"
        ),
        other => panic!("expected a parse error, got {other:?}"),
    }
}

/// The table holds the seventeen keys in the order the template lists them.
#[test]
fn the_table_lists_the_keys_in_template_order() {
    let names: Vec<&str> = TUI_SETTINGS.iter().map(|key| key.key).collect();
    assert_eq!(
        names,
        [
            "palette",
            "accent",
            "colors",
            "monochrome",
            "theme_background",
            "mouse",
            "intro",
            "intro_effect",
            "intro_sound",
            "transition",
            "title_meter",
            "confirm_quit",
            "host_audio",
            "visualizer",
            "graphs",
            "graph_span",
            "link_span",
        ]
    );
    let template = Config::template();
    let mut from = 0;
    for key in TUI_SETTINGS {
        let line = format!("# {} = ", key.key);
        let at = template[from..].find(&line).unwrap_or_else(|| {
            panic!("{} is not in the template after the one before it", key.key)
        });
        from += at + line.len();
    }
}

/// Each key reads back what `apply` set, as a switch or as a word, and the accent as neither.
#[test]
fn a_key_reads_back_what_apply_set() {
    for case in cases() {
        let mut tui = TuiConfig::default();
        if matches!(case.key.kind, TuiSettingKind::Accent) {
            assert_eq!(case.key.flag(&tui), None);
            assert_eq!(case.key.word(&tui), None);
            continue;
        }
        let value = case.value();
        case.key.apply(&mut tui, value);
        match value {
            TuiValue::Bool(on) => {
                assert_eq!(case.key.flag(&tui), Some(on));
                assert_eq!(case.key.word(&tui), None);
            }
            TuiValue::Word(word) => {
                assert_eq!(case.key.word(&tui), Some(word));
                assert_eq!(case.key.flag(&tui), None);
            }
        }
    }
}

/// A word outside a key's list changes nothing, and every word of the list is taken.
#[test]
fn a_word_key_takes_only_its_own_words() {
    for key in TUI_SETTINGS {
        let TuiSettingKind::Word { get, set, words } = key.kind else {
            continue;
        };
        let mut tui = TuiConfig::default();
        assert!(!set(&mut tui, "no such word"), "{}", key.key);
        assert_eq!(tui, TuiConfig::default(), "{}", key.key);
        for word in words() {
            assert!(set(&mut tui, word), "{}: {word}", key.key);
            assert_eq!(get(&tui), word);
        }
    }
}

/// The words of a key are the ones its error message and the Settings box list.
#[test]
fn the_words_are_the_lists_the_box_steps_through() {
    let words = |key: &TuiSetting| match key.kind {
        TuiSettingKind::Word { words, .. } => words(),
        _ => panic!("{} holds no word", key.key),
    };
    assert_eq!(words(&PALETTE), ["amber", "ice", "red", "neon"]);
    assert_eq!(words(&COLORS), ["auto", "16", "256", "true"]);
    assert_eq!(words(&TRANSITION), ["none", "trace", "noise", "melt"]);
    assert_eq!(
        words(&GRAPH_SPAN),
        ["1m", "2m", "3m", "5m", "10m", "15m", "30m", "60m"]
    );
    assert_eq!(words(&LINK_SPAN), ["1h", "3h", "6h", "12h", "24h"]);
}

/// A value of the wrong kind is a bug the debug build stops at.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "mouse does not take Word")]
fn a_value_of_the_wrong_kind_panics_in_a_debug_build() {
    MOUSE.apply(&mut TuiConfig::default(), TuiValue::Word("on"));
}

/// Entries are the same key when the file spells them the same, and print as that key.
#[test]
fn entries_compare_and_print_by_their_key() {
    assert_eq!(MOUSE, MOUSE);
    assert_ne!(MOUSE, INTRO);
    assert_eq!(format!("{MOUSE:?}"), "TuiSetting(mouse)");
}

/// Every field of [`TuiConfig`] is a plain key in [`TUI_SETTINGS`] or one of the three that keep
/// their own writers.
#[test]
fn every_tui_config_field_is_covered() {
    let tui = TuiConfig {
        accent: Some("#112233".to_string()),
        spectrum: vec!["#000000".to_string(), "#ffffff".to_string()],
        ..TuiConfig::default()
    };
    let json = serde_json::to_value(&tui).expect("the config serialises");
    let mut fields: Vec<&str> = json
        .as_object()
        .expect("a table")
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    let mut covered: Vec<&str> = TUI_SETTINGS.iter().map(|key| key.key).collect();
    covered.extend(["accents", "spectrum", "effects"]);
    covered.sort_unstable();
    assert_eq!(
        fields, covered,
        "a TuiConfig field has no TUI_SETTINGS entry (or hand writer): add one in tui_settings.rs"
    );
}

/// Setting the accent back to nothing in a file the template wrote puts the template's
/// commented line back exactly as the template writes it.
#[test]
fn clearing_the_accent_restores_the_templates_commented_line() {
    let dir = temp_dir("tui-settings");
    let path = dir.join("accent.toml");
    let template = Config::template();
    let line = template
        .lines()
        .find(|line| line.starts_with("# accent = "))
        .expect("the template has a commented accent line");
    std::fs::write(&path, template.replace(line, "accent = \"#112233\""))
        .expect("the file is written");
    Config::update_tui(&path, |tui| tui.accent = None).expect("the change is written");
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file is read"),
        template
    );
}
