// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! User presets: parsing their blocks, what is refused, lookup and the preset writers.

use super::*;

/// A whole five-band `[[preset]]` block, integer `f = 40` included, parses to the values it
/// spells out.
#[test]
fn a_whole_five_band_preset_block_parses_to_its_values() {
    let text = "[[preset]]\nname = \"HD650 warm\"\nbands = 5\nmode = \"eq\"\n\
                     loudness = 0\nband = [\n  { f = 40, g = 3.0, q = 0.7 },\n  \
                     { f = 160, g = 1.5, q = 0.7 },\n  { f = 632, g = 0.0, q = 4.3 },\n  \
                     { f = 2515, g = 0.0, q = 4.3 },\n  { f = 10000, g = 2.0, q = 0.7 },\n]\n";
    assert_eq!(Config::parse(text).unwrap().presets, vec![hd650_warm()]);
}

#[test]
fn a_preset_without_a_name_is_refused() {
    let text = "[[preset]]\nbands = 3\nmode = \"eq\"\nband = \
                     [{ f = 63, g = 0.0, q = 1.0 }, { f = 630, g = 0.0, q = 1.0 }, \
                     { f = 6300, g = 0.0, q = 1.0 }]\n";
    let err = Config::parse(text).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("needs a name")),
        "{err:?}"
    );
}

#[test]
fn an_unknown_preset_key_is_refused_and_names_the_preset() {
    let text = three_band_block("mine").replace("loudness = 0\n", "loudness = 0\nsparkle = true\n");
    let err = Config::parse(&text).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("\"mine\"") && m.contains("sparkle")),
        "{err:?}"
    );
}

#[test]
fn an_unknown_band_key_is_refused_and_names_the_preset_and_the_band() {
    let text = three_band_block("mine").replace(
        "{ f = 63, g = 1.0, q = 0.7 }",
        "{ f = 63, g = 1.0, q = 0.7, x = 1 }",
    );
    let err = Config::parse(&text).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("\"mine\"") && m.contains("band 1") && m.contains("x")),
        "{err:?}"
    );
}

#[test]
fn bands_other_than_3_or_5_are_refused() {
    let text = three_band_block("mine").replace("bands = 3", "bands = 4");
    let err = Config::parse(&text).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("\"mine\"") && m.contains("3 or 5")),
        "{err:?}"
    );
}

#[test]
fn a_band_count_not_matching_bands_is_refused() {
    let text = three_band_block("mine").replace("  { f = 6300, g = 0.0, q = 4.3 },\n", "");
    let err = Config::parse(&text).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("\"mine\"") && m.contains("2 band entries")),
        "{err:?}"
    );
}

#[test]
fn a_preset_without_a_mode_is_refused() {
    let text = three_band_block("mine").replace("mode = \"eq\"\n", "");
    let err = Config::parse(&text).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("\"mine\"") && m.contains("flat, eq, or loudness")),
        "{err:?}"
    );
}

#[test]
fn an_unknown_mode_word_is_refused() {
    let text = three_band_block("mine").replace("mode = \"eq\"", "mode = \"warp\"");
    let err = Config::parse(&text).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("\"mine\"") && m.contains("flat, eq, or loudness")),
        "{err:?}"
    );
}

/// `mode` accepts any case, as `EqMode`'s own parsing does everywhere else.
#[test]
fn the_mode_word_is_read_in_any_case() {
    let text = three_band_block("mine").replace("mode = \"eq\"", "mode = \"EQ\"");
    assert_eq!(Config::parse(&text).unwrap().presets[0].mode, EqMode::Eq);
}

#[test]
fn a_preset_with_no_loudness_key_defaults_to_zero() {
    let text = three_band_block("mine").replace("loudness = 0\n", "");
    assert_eq!(Config::parse(&text).unwrap().presets[0].loudness, 0);
}

#[test]
fn a_loudness_above_127_is_refused() {
    let text = three_band_block("mine").replace("loudness = 0", "loudness = 200");
    let err = Config::parse(&text).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("\"mine\"") && m.contains("0-127")),
        "{err:?}"
    );
}

/// TOML can write `nan`, `inf` and `-inf` as float literals, which a derived `Deserialize`
/// would accept as ordinary `f64`s; validation rejects every one of them, by name.
#[test]
fn a_non_finite_band_value_is_refused() {
    for bad in ["nan", "inf", "-inf"] {
        let text = three_band_block("mine").replace("g = 1.0", &format!("g = {bad}"));
        let err = Config::parse(&text).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m.contains("\"mine\"") && m.contains("finite")),
            "{bad}: {err:?}"
        );
    }
}

#[test]
fn a_non_numeric_band_value_is_refused() {
    let text = three_band_block("mine").replace("g = 1.0", "g = \"loud\"");
    let err = Config::parse(&text).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("\"mine\"") && m.contains("number")),
        "{err:?}"
    );
}

#[test]
fn duplicate_preset_names_are_refused_case_insensitively() {
    let text = format!("{}\n{}", three_band_block("mine"), three_band_block("MINE"));
    let err = Config::parse(&text).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("\"MINE\"") && m.contains("already configured")),
        "{err:?}"
    );
}

#[test]
fn a_built_in_name_is_refused_for_a_user_preset_in_any_case() {
    for name in ["Flat", "flat", "FLAT", "Warm", "bass", "PRESENCE"] {
        let text = three_band_block(name);
        let err = Config::parse(&text).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m.contains("built-in")),
            "{name}: {err:?}"
        );
    }
}

/// A name of up to forty characters loads; one longer is a config error naming the preset.
#[test]
fn a_preset_name_longer_than_forty_characters_is_refused() {
    let longest = "é".repeat(NAME_MAX);
    let config = Config::parse(&three_band_block(&longest)).expect("forty characters load");
    assert_eq!(config.presets[0].name, longest);
    let long = "x".repeat(NAME_MAX + 1);
    let err = Config::parse(&three_band_block(&long)).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m)
                if m.contains(&long) && m.contains("at most 40 characters, not 41")),
        "{err:?}"
    );
}

/// Every writer refuses a name longer than a preset's may be, and leaves the file as it
/// was, so no writer can leave behind a file that no longer loads.
#[test]
fn the_preset_writers_refuse_a_name_longer_than_forty_characters() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let warm = hd650_warm();
    Config::add_preset(&path, &warm).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();
    let long = "x".repeat(NAME_MAX + 1);
    let mut longer = warm.clone();
    longer.name = long.clone();
    for (what, wrote) in [
        ("add", Config::add_preset(&path, &longer)),
        ("update", Config::update_preset(&path, &warm.name, &longer)),
        ("rename", Config::rename_preset(&path, &warm.name, &long)),
    ] {
        let err = wrote.unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m.contains("at most 40 characters")),
            "{what}: {err:?}"
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            before,
            "{what}: a refused write changed the file"
        );
    }
    let mut forty = warm.clone();
    forty.name = "x".repeat(NAME_MAX);
    Config::add_preset(&path, &forty).expect("forty characters are a name");
    Config::load_from(&path).expect("and the file still loads");
}

/// A name with a control character inside it is a config error naming the preset; the
/// blanks round a name are no part of it.
#[test]
fn a_preset_name_holding_a_control_character_is_refused_and_blanks_round_one_are_dropped() {
    for name in ["new\nline", "tab\there"] {
        let err = Config::parse(&three_band_block(name)).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m)
                    if m.contains(&format!("{name:?}")) && m.contains("control character")),
            "{name:?}: {err:?}"
        );
    }
    let config = Config::parse(&three_band_block(" padded ")).expect("a padded name loads");
    assert_eq!(config.presets[0].name, "padded");
    let err = Config::parse(&three_band_block("   ")).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m == "a preset needs a name"),
        "{err:?}"
    );
}

/// Every writer writes a name without the blanks round it, and refuses a blank name or one
/// holding a control character with those words rather than the file's, leaving the file
/// as it was.
#[test]
fn the_preset_writers_trim_a_name_and_refuse_a_blank_or_control_one() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let mut warm = hd650_warm();
    warm.name = " HD650 warm ".into();
    Config::add_preset(&path, &warm).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let written = |text: &str, name: &str| {
        text.lines()
            .filter(|line| line.starts_with("name = "))
            .any(|line| line == format!("name = {name:?}"))
    };
    assert!(written(&text, "HD650 warm"), "{text}");
    assert!(!text.contains("\" HD650 warm \""), "{text}");
    assert_eq!(
        Config::load_from(&path).unwrap().presets,
        vec![hd650_warm()],
        "the file carries the name trimmed"
    );

    let before = std::fs::read_to_string(&path).unwrap();
    let named = |name: &str| Preset {
        name: name.into(),
        ..hd650_warm()
    };
    for (name, words) in [
        ("  ", "a preset needs a name"),
        ("new\nline", "control character"),
        ("tab\there", "control character"),
    ] {
        for (what, wrote) in [
            ("add", Config::add_preset(&path, &named(name))),
            (
                "update",
                Config::update_preset(&path, "HD650 warm", &named(name)),
            ),
            ("rename", Config::rename_preset(&path, "HD650 warm", name)),
        ] {
            let err = wrote.unwrap_err();
            assert!(
                matches!(&err, ConfigError::Parse(m) if m.contains(words)),
                "{what} {name:?}: {err:?}"
            );
            assert_eq!(
                std::fs::read_to_string(&path).unwrap(),
                before,
                "{what} {name:?}: a refused write changed the file"
            );
        }
    }

    Config::update_preset(&path, " hd650 WARM ", &named("  HD650 warm\t")).unwrap();
    Config::rename_preset(&path, " HD650 warm ", " Late night ").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(written(&text, "Late night"), "{text}");
    assert!(!text.contains("\" Late night \""), "{text}");
    Config::remove_preset(&path, "late night ").unwrap();
    assert!(Config::load_from(&path).unwrap().presets.is_empty());
}

/// A block the file spells with blanks round its name — written by hand, or by a writer
/// that kept them — is found by the name it loads as: every writer and the lookup reach
/// it, and a new preset of that name is refused as taken.
#[test]
fn a_block_spelled_with_blanks_round_its_name_is_found_by_the_name_it_loads_as() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let block = three_band_block(" padded ");
    std::fs::write(&path, &block).unwrap();
    let config = Config::load_from(&path).unwrap();
    assert!(config.preset("PADDED").is_some());
    assert!(config.preset(" padded ").is_some());

    let mut three = config.presets[0].clone();
    three.name = "Padded".into();
    let err = Config::add_preset(&path, &three).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("\"padded\"") && m.contains("already")),
        "{err:?}"
    );
    Config::update_preset(&path, "padded", &three).unwrap();
    Config::rename_preset(&path, "padded", "tidy").unwrap();
    assert_eq!(Config::load_from(&path).unwrap().presets[0].name, "tidy");

    std::fs::write(&path, &block).unwrap();
    Config::remove_preset(&path, "padded").unwrap();
    assert!(Config::load_from(&path).unwrap().presets.is_empty());
}

#[test]
fn preset_lookup_is_case_insensitive_and_user_presets_for_filters_by_band_count() {
    let five = hd650_warm();
    let three = Preset {
        name: "three".into(),
        bands: 3,
        band: vec![
            PresetBand {
                f: 63.0,
                g: 1.0,
                q: 0.7,
            },
            PresetBand {
                f: 630.0,
                g: 0.0,
                q: 4.3,
            },
            PresetBand {
                f: 6300.0,
                g: 0.0,
                q: 4.3,
            },
        ],
        ..hd650_warm()
    };
    let cfg = Config {
        presets: vec![five.clone(), three.clone()],
        ..Config::default()
    };
    assert_eq!(cfg.preset("hd650 WARM"), Some(&five));
    assert_eq!(cfg.preset("nope"), None);
    assert_eq!(cfg.user_presets_for(5).collect::<Vec<_>>(), vec![&five]);
    assert_eq!(cfg.user_presets_for(3).collect::<Vec<_>>(), vec![&three]);
    assert_eq!(
        cfg.user_presets_for(4).collect::<Vec<_>>(),
        Vec::<&Preset>::new()
    );
}

/// The preset writers edit the `[[preset]]` blocks and nothing else: the comments, the
/// keys and the tables around them come through byte for byte, the way
/// [`devices_are_added_updated_and_removed_without_touching_the_rest`] shows for devices.
#[test]
fn presets_are_added_updated_renamed_and_removed_without_touching_the_rest() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "# mine\nschema = 1\nadapter = \"hci1\" # dongle\n\n\
                         [[device]]\nname = \"amp\"\nudid = \"0102030405060708\"\n\n\
                         [tui]\npalette = \"ice\"\n";
    std::fs::write(&path, original).unwrap();

    let warm = hd650_warm();
    Config::add_preset(&path, &warm).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.starts_with("# mine\nschema = 1\nadapter = \"hci1\" # dongle\n"),
        "{text}"
    );
    assert!(text.contains("[tui]\npalette = \"ice\"\n"), "{text}");
    assert_eq!(Config::parse(&text).unwrap().presets, vec![warm.clone()]);

    let mut updated = warm.clone();
    updated.band[0].g = 5.0;
    Config::update_preset(&path, "HD650 warm", &updated).unwrap();
    let after_update = Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(after_update.presets[0].band[0].g, 5.0);

    Config::rename_preset(&path, "hd650 warm", "renamed").unwrap();
    let after_rename = Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(after_rename.presets[0].name, "renamed");
    assert_eq!(
        after_rename.presets[0].band[0].g, 5.0,
        "renaming left the curve alone"
    );

    Config::remove_preset(&path, "RENAMED").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(Config::parse(&text).unwrap().presets.is_empty());
    assert!(text.contains("adapter = \"hci1\" # dongle"), "{text}");
    assert!(text.contains("[[device]]\nname = \"amp\"\n"), "{text}");

    assert!(matches!(
        Config::remove_preset(&path, "nobody"),
        Err(ConfigError::Parse(_))
    ));
    assert!(matches!(
        Config::update_preset(&path, "nobody", &warm),
        Err(ConfigError::Parse(_))
    ));
    assert!(matches!(
        Config::rename_preset(&path, "nobody", "x"),
        Err(ConfigError::Parse(_))
    ));
}

/// A preset written into a file the template made sits at the end of it, after the
/// comments that explain the rest — and taking it out again puts the file back as it was,
/// rather than carrying those comments off with the block. A comment right above a block
/// goes with it; one a blank line sets apart from it is the file's and is handed on, the
/// way [`a_device_added_to_a_template_and_taken_out_again_leaves_the_file_as_it_was`] shows
/// for a device.
#[test]
fn a_preset_added_to_a_template_and_taken_out_again_leaves_the_file_as_it_was() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::add_preset(&path, &hd650_warm()).unwrap();
    let added = std::fs::read_to_string(&path).unwrap();
    assert!(
        added.contains(
            "# stops = [\"#5a1a08\", \"#c04a10\", \"#e0a030\", \"#f6dca0\"]\n\n\
                 [[preset]]\nname = \"HD650 warm\"\n"
        ),
        "the block goes after the comments that were at the end of the file:\n{added}"
    );

    Config::remove_preset(&path, "HD650 warm").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), Config::template());

    let original = "schema = 1\n\n# my presets\n\n# the warm one\n\
                         [[preset]]\nname = \"a\"\nbands = 3\nmode = \"eq\"\nloudness = 0\n\
                         band = [\n  { f = 63, g = 1.0, q = 0.7 },\n  { f = 630, g = 0.0, q = 4.3 },\n  \
                         { f = 6300, g = 0.0, q = 4.3 },\n]\n\n\
                         [[preset]]\nname = \"b\"\nbands = 3\nmode = \"eq\"\nloudness = 0\n\
                         band = [\n  { f = 63, g = 1.0, q = 0.7 },\n  { f = 630, g = 0.0, q = 4.3 },\n  \
                         { f = 6300, g = 0.0, q = 4.3 },\n]\n";
    std::fs::write(&path, original).unwrap();
    Config::remove_preset(&path, "a").unwrap();
    let left = std::fs::read_to_string(&path).unwrap();
    assert!(left.contains("# my presets"), "{left}");
    assert!(!left.contains("# the warm one"), "{left}");
    assert_eq!(Config::parse(&left).unwrap().presets.len(), 1);
}

#[test]
fn add_and_rename_refuse_a_built_ins_name_and_a_name_already_taken() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let warm = hd650_warm();
    Config::add_preset(&path, &warm).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();

    let mut flat = warm.clone();
    flat.name = "Flat".into();
    let err = Config::add_preset(&path, &flat).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("built-in")),
        "{err:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        before,
        "a refused add wrote to the file"
    );

    let mut second = warm.clone();
    second.name = "second".into();
    Config::add_preset(&path, &second).unwrap();

    let err = Config::rename_preset(&path, "second", "HD650 WARM").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("already exists")),
        "{err:?}"
    );
    let err = Config::rename_preset(&path, "second", "flat").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("built-in")),
        "{err:?}"
    );

    // Renaming a preset onto the name it already has, or another spelling of it, is no
    // collision.
    Config::rename_preset(&path, "second", "second").unwrap();
    Config::rename_preset(&path, "second", "SECOND").unwrap();
}

#[test]
fn update_preset_refuses_a_name_another_preset_already_carries_but_not_its_own() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::add_preset(&path, &hd650_warm()).unwrap();
    let mut second = hd650_warm();
    second.name = "second".into();
    Config::add_preset(&path, &second).unwrap();

    let mut clash = second.clone();
    clash.name = "HD650 warm".into();
    let err = Config::update_preset(&path, "second", &clash).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("already exists")),
        "{err:?}"
    );

    let mut same = hd650_warm();
    same.band[0].g = 9.0;
    Config::update_preset(&path, "hd650 warm", &same).unwrap();
    let cfg = Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(cfg.preset("HD650 warm").unwrap().band[0].g, 9.0);
}

#[test]
fn update_preset_refuses_a_built_ins_name() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::add_preset(&path, &hd650_warm()).unwrap();
    let mut flat = hd650_warm();
    flat.name = "Flat".into();
    let err = Config::update_preset(&path, "HD650 warm", &flat).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("built-in")),
        "{err:?}"
    );
}

/// `band` is written as an inline array of inline tables, one per line,
/// with a trailing comma and the closing bracket on a line of its own.
#[test]
fn the_band_array_is_written_as_one_inline_table_per_line() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::add_preset(&path, &hd650_warm()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains(
            "band = [\n  { f = 40.0, g = 3.0, q = 0.7 },\n  { f = 160.0, g = 1.5, q = 0.7 },\n  \
                 { f = 632.0, g = 0.0, q = 4.3 },\n  { f = 2515.0, g = 0.0, q = 4.3 },\n  \
                 { f = 10000.0, g = 2.0, q = 0.7 },\n]\n"
        ),
        "{text}"
    );
}

/// `update_preset` replaces the whole `band` array rather than editing it entry by entry,
/// whatever shape a hand-written file wrote it in.
#[test]
fn update_preset_rewrites_the_band_array_whole() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let hand = "schema = 1\n\n[[preset]]\nname = \"mine\"\nbands = 3\nmode = \"eq\"\n\
                     loudness = 0\nband = [{ f = 63, g = 1.0, q = 0.7 }, { f = 630, g = 0.0, q = 4.3 }, \
                     { f = 6300, g = 0.0, q = 4.3 }]\n";
    std::fs::write(&path, hand).unwrap();
    let updated = Preset {
        name: "mine".into(),
        bands: 3,
        band: vec![
            PresetBand {
                f: 63.0,
                g: 2.0,
                q: 0.7,
            },
            PresetBand {
                f: 630.0,
                g: 0.0,
                q: 4.3,
            },
            PresetBand {
                f: 6300.0,
                g: 0.0,
                q: 4.3,
            },
        ],
        ..hd650_warm()
    };
    Config::update_preset(&path, "mine", &updated).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains(
            "band = [\n  { f = 63.0, g = 2.0, q = 0.7 },\n  { f = 630.0, g = 0.0, q = 4.3 },\n  \
                 { f = 6300.0, g = 0.0, q = 4.3 },\n]\n"
        ),
        "{text}"
    );
}

#[test]
fn a_preset_writer_starts_an_absent_file_from_the_template() {
    let dir = temp_dir("config-writer");
    let path = dir.join("preset.toml");
    Config::add_preset(&path, &hd650_warm()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("\nschema = 1\n") && text.contains("[tui.accents]"),
        "{text}"
    );
}

#[test]
fn fitting_presets_are_the_built_ins_then_the_files_own_of_the_same_band_count() {
    let registry = svolder_profiles::Registry::builtin().unwrap();
    let five = registry.get("gungnir2-v2").unwrap();
    let mut config = Config::default();
    let mut mine = hd650_warm();
    mine.name = "mine".to_string();
    let mut three = hd650_warm();
    three.name = "three".to_string();
    three.bands = 3;
    config.presets = vec![mine, three];
    let listed = config.fitting_presets(five);
    assert_eq!(
        listed.last().map(|(p, b)| (p.name.as_str(), *b)),
        Some(("mine", false))
    );
    assert!(listed.iter().all(|(p, _)| p.name != "three"));
    assert_eq!(listed.iter().filter(|(_, built_in)| *built_in).count(), 4);
    let no_eq = registry.get("fake-mono").unwrap();
    assert!(config.fitting_presets(no_eq).is_empty());
}

/// `custom` is no name for a preset a writer adds or renames to, in any case; a file that already
/// has one still loads, and its block can be renamed or removed.
#[test]
fn custom_is_refused_as_a_new_name_and_an_old_one_still_loads() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let mut custom = hd650_warm();
    custom.name = "Custom".into();
    let err = Config::add_preset(&path, &custom).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("stands for a curve no preset matches")),
        "{err:?}"
    );
    std::fs::write(&path, three_band_block("custom")).unwrap();
    assert!(Config::load_from(&path).unwrap().preset("custom").is_some());
    Config::rename_preset(&path, "custom", "mine").unwrap();
    assert!(Config::load_from(&path).unwrap().preset("mine").is_some());
}

/// A file's old block named `custom` can be updated under its own name, in the spelling it has or
/// another, but not renamed onto `custom` from another name.
#[test]
fn a_legacy_custom_block_can_be_updated_under_its_own_name() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, three_band_block("custom")).unwrap();
    let mut curve = Config::load_from(&path).unwrap().presets[0].clone();
    curve.band[0].g = 2.5;
    Config::update_preset(&path, "custom", &curve).unwrap();
    curve.name = "Custom".into();
    Config::update_preset(&path, "custom", &curve).unwrap();
    let loaded = Config::load_from(&path).unwrap();
    assert_eq!(loaded.presets.len(), 1);
    assert_eq!(loaded.presets[0].band[0].g, 2.5);
    std::fs::write(
        &path,
        format!(
            "{}\n{}",
            three_band_block("mine"),
            three_band_block("custom")
        ),
    )
    .unwrap();
    curve.name = "custom".into();
    let err = Config::update_preset(&path, "mine", &curve).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("stands for a curve no preset matches")),
        "{err:?}"
    );
}
