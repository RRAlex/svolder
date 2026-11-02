// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Imported (AutoEq) presets: filter blocks that parse and round-trip, and the AutoEq table's
//! refresh.

use super::*;

/// The block of an imported preset, as svolder writes it.
const IMPORTED_BLOCK: &str = "[[preset]]\nname = \"Sennheiser HD 650\"\n\
    origin = \"AutoEq · oratory1990 · over-ear\"\npreamp = -6.1\nfilter = [\n  \
    { type = \"LSC\", f = 105.0, g = 6.0, q = 0.7 },\n  \
    { type = \"PK\", f = 180.0, g = -2.1, q = 0.9 },\n]\n";

/// An imported preset reads as filters, and a band preset is read and written as it was.
#[test]
fn an_imported_preset_block_parses_to_its_filters() {
    let config = Config::parse(IMPORTED_BLOCK).unwrap();
    let [preset] = config.presets.as_slice() else {
        panic!("one preset");
    };
    assert_eq!(
        (preset.bands, preset.band.len(), preset.loudness),
        (0, 0, 0)
    );
    assert_eq!(preset.mode, EqMode::Eq);
    let imported = preset.source.as_ref().unwrap();
    assert_eq!(
        imported.origin.as_deref(),
        Some("AutoEq · oratory1990 · over-ear")
    );
    assert_eq!(imported.correction.preamp_db, -6.1);
    assert_eq!(imported.correction.filters.len(), 2);
    assert_eq!(imported.correction.filters[1].g, -2.1);
}

/// A written block reads back to the same preset, and writing it again is byte for byte the
/// same, with the file's other bytes and a band preset beside it untouched.
#[test]
fn an_imported_block_round_trips_byte_for_byte() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "# mine\nschema = 1\n\n[tui]\npalette = \"ice\"\n";
    std::fs::write(&path, original).unwrap();
    let preset = Config::parse(IMPORTED_BLOCK).unwrap().presets.remove(0);
    Config::add_preset(&path, &hd650_warm()).unwrap();
    let with_band = std::fs::read_to_string(&path).unwrap();
    Config::add_preset(&path, &preset).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with(&with_band), "{text}");
    assert!(text.ends_with(IMPORTED_BLOCK), "{text}");
    let loaded = Config::parse(&text).unwrap();
    assert_eq!(loaded.presets, vec![hd650_warm(), preset.clone()]);

    Config::update_preset(&path, "sennheiser hd 650", &preset).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);

    let mut plain = preset.clone();
    plain.source.as_mut().unwrap().origin = None;
    Config::update_preset(&path, "Sennheiser HD 650", &plain).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("origin"), "{text}");
    assert_eq!(Config::parse(&text).unwrap().presets[1], plain);

    Config::remove_preset(&path, "Sennheiser HD 650").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), with_band);
}

/// Updating a band preset into an imported one, and back, leaves none of the other kind's keys.
#[test]
fn a_block_changes_kind_without_keeping_the_old_keys() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "").unwrap();
    let imported = Config::parse(IMPORTED_BLOCK).unwrap().presets.remove(0);
    let band = Preset {
        name: imported.name.clone(),
        ..hd650_warm()
    };
    Config::add_preset(&path, &band).unwrap();
    Config::update_preset(&path, &band.name, &imported).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        Config::parse(&text).unwrap().presets,
        vec![imported.clone()]
    );
    for key in ["bands", "mode", "loudness", "band ="] {
        assert!(!text.contains(key), "{key}: {text}");
    }
    Config::update_preset(&path, &band.name, &band).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(Config::parse(&text).unwrap().presets, vec![band]);
    for key in ["origin", "preamp", "filter"] {
        assert!(!text.contains(key), "{key}: {text}");
    }
}

/// Renaming an imported preset keeps its filters, and removing one takes its filters out and
/// leaves another imported preset's alone.
#[test]
fn renaming_and_deleting_an_imported_preset_keeps_or_removes_its_filters() {
    let first = Config::parse(IMPORTED_BLOCK).unwrap().presets.remove(0);
    let mut second = first.clone();
    second.name = "Other".into();
    second.source.as_mut().unwrap().correction.filters[0].g = 3.0;
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    Config::add_preset(&path, &first).unwrap();
    Config::add_preset(&path, &second).unwrap();

    Config::rename_preset(&path, &first.name, "Renamed").unwrap();
    let renamed = Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let mut expected = first.clone();
    expected.name = "Renamed".into();
    assert_eq!(renamed.presets, vec![expected.clone(), second.clone()]);

    Config::remove_preset(&path, "Renamed").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(Config::parse(&text).unwrap().presets, vec![second.clone()]);
    assert_eq!(
        text.matches("type =").count(),
        second.source.as_ref().unwrap().correction.filters.len(),
        "{text}"
    );
    Config::remove_preset(&path, "Other").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    for key in ["filter", "preamp", "origin"] {
        assert!(!text.contains(key), "{key}: {text}");
    }
}

/// `filter` with any key of a band preset is refused with the importer's words, naming the preset.
#[test]
fn filter_with_band_keys_is_refused() {
    for extra in [
        "bands = 3\n",
        "mode = \"eq\"\n",
        "loudness = 0\n",
        "band = [{ f = 63, g = 0.0, q = 1.0 }]\n",
    ] {
        let text = IMPORTED_BLOCK.replace("preamp", &format!("{extra}preamp"));
        let err = Config::parse(&text).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m)
                if m == "preset \"Sennheiser HD 650\": an imported preset holds filter, not band"),
            "{extra}: {err:?}"
        );
    }
}

/// `type` reads in any case and is written upper case; other words refuse.
#[test]
fn a_filter_type_reads_in_any_case_and_writes_upper_case() {
    let lower = IMPORTED_BLOCK
        .replace("\"LSC\"", "\"lsc\"")
        .replace("\"PK\"", "\"Pk\"");
    let preset = Config::parse(&lower).unwrap().presets.remove(0);
    assert_eq!(preset, Config::parse(IMPORTED_BLOCK).unwrap().presets[0]);
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "").unwrap();
    Config::add_preset(&path, &preset).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("type = \"LSC\"") && text.contains("type = \"PK\""),
        "{text}"
    );
    for bad in ["\"PEQ\"", "\"LS\"", "\"notch\"", "3"] {
        let err = Config::parse(&IMPORTED_BLOCK.replace("\"LSC\"", bad)).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m.contains("filter 1")),
            "{bad}: {err:?}"
        );
    }
}

/// The bounds of an imported preset refuse, naming the preset and the filter.
#[test]
fn imported_bounds_refuse() {
    let cases = [
        ("f = 105.0", "f = 5.0"),
        ("f = 105.0", "f = 30000.0"),
        ("g = 6.0", "g = 31.0"),
        ("q = 0.7 }", "q = 0.0 }"),
        ("q = 0.7 }", "q = 21.0 }"),
        ("preamp = -6.1", "preamp = nan"),
        ("preamp = -6.1", "preamp = 31.0"),
        ("preamp = -6.1", "preamp = -1000.0"),
        ("g = 6.0", "g = inf"),
    ];
    for (from, to) in cases {
        let err = Config::parse(&IMPORTED_BLOCK.replace(from, to)).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m.starts_with("preset \"Sennheiser HD 650\": ")),
            "{to}: {err:?}"
        );
    }
    let empty = "[[preset]]\nname = \"x\"\nfilter = []\n";
    assert!(
        matches!(Config::parse(empty).unwrap_err(), ConfigError::Parse(m) if m.contains("no filter"))
    );
    let many: String = (0..64)
        .map(|_| "{ type = \"PK\", f = 1000.0, g = 1.0, q = 1.0 }, ")
        .collect();
    let text = format!("[[preset]]\nname = \"x\"\nfilter = [{many}]\n");
    assert!(
        matches!(Config::parse(&text).unwrap_err(), ConfigError::Parse(m) if m.contains("64 filters"))
    );
    let unknown = IMPORTED_BLOCK.replace("q = 0.7 }", "q = 0.7, x = 1 }");
    assert!(
        matches!(Config::parse(&unknown).unwrap_err(), ConfigError::Parse(m) if m.contains("filter 1") && m.contains("\"x\""))
    );
    let origin_alone = "[[preset]]\nname = \"x\"\norigin = \"a\"\nbands = 3\nmode = \"eq\"\n";
    assert!(
        matches!(Config::parse(origin_alone).unwrap_err(), ConfigError::Parse(m) if m.contains("origin"))
    );
}

/// An imported preset is offered to a tone control of any band count that has bands.
#[test]
fn user_presets_for_lists_imported_presets_for_every_band_count() {
    let text = format!("{IMPORTED_BLOCK}\n{}", three_band_block("mine"));
    let config = Config::parse(&text).unwrap();
    for (bands, expected) in [(3, 2), (5, 1), (7, 1), (0, 0)] {
        assert_eq!(config.user_presets_for(bands).count(), expected, "{bands}");
    }
}

/// `refresh_weeks` reads an integer from 1 to 99 or the word `never` in any case, defaults to
/// thirteen weeks, and refuses everything else naming the key.
#[test]
fn autoeq_refresh_weeks_reads_a_number_or_never() {
    let weeks = |body: &str| Config::parse(&format!("schema = 1\n[autoeq]\n{body}\n"));
    assert_eq!(
        Config::parse("schema = 1\n").unwrap().autoeq.refresh_weeks,
        RefreshWeeks::Weeks(13)
    );
    assert_eq!(
        weeks("refresh_weeks = 1").unwrap().autoeq.refresh_weeks,
        RefreshWeeks::Weeks(1)
    );
    assert_eq!(
        weeks("refresh_weeks = 99").unwrap().autoeq.refresh_weeks,
        RefreshWeeks::Weeks(99)
    );
    assert_eq!(
        weeks("refresh_weeks = \"Never\"")
            .unwrap()
            .autoeq
            .refresh_weeks,
        RefreshWeeks::Never
    );
    for bad in ["0", "100", "-3", "300", "\"soon\"", "2.5", "true", "\"13\""] {
        let err = weeks(&format!("refresh_weeks = {bad}"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("autoeq.refresh_weeks"), "{bad}: {err}");
    }
}

/// The writer sets the one key where the template shows it, keeps every other byte, and is a
/// no-op when nothing changes.
#[test]
fn update_autoeq_keeps_the_files_bytes() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::create_from_template(&path).unwrap();
    let template = std::fs::read_to_string(&path).unwrap();

    Config::update_autoeq(&path, |a| a.refresh_weeks = RefreshWeeks::Weeks(13)).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), template);

    Config::update_autoeq(&path, |a| a.refresh_weeks = RefreshWeeks::Weeks(4)).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        saved,
        template.replace("# refresh_weeks = 13", "refresh_weeks = 4")
    );
    assert_eq!(
        Config::parse(&saved).unwrap().autoeq.refresh_weeks,
        RefreshWeeks::Weeks(4)
    );

    Config::update_autoeq(&path, |a| a.refresh_weeks = RefreshWeeks::Never).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("refresh_weeks = \"never\""), "{saved}");

    let own = "# mine\nschema = 1\n\n[tui]\nintro = false # quiet\n";
    std::fs::write(&path, own).unwrap();
    Config::update_autoeq(&path, |a| a.refresh_weeks = RefreshWeeks::Weeks(2)).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.starts_with(own), "{saved}");
    assert!(saved.contains("[autoeq]"), "{saved}");
    assert_eq!(
        Config::parse(&saved).unwrap().autoeq.refresh_weeks,
        RefreshWeeks::Weeks(2)
    );
}

/// The template carries the table, commented out at the default.
#[test]
fn the_template_shows_the_autoeq_table() {
    let template = Config::template();
    assert!(template.contains("\n[autoeq]\n"), "{template}");
    assert!(template.contains("# refresh_weeks = 13\n"), "{template}");
}
