// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `[tui]` block: its defaults, effects, accents and other keys, parsed and round-tripped.

use super::*;

#[test]
fn tui_defaults_when_the_block_is_absent() {
    let c = Config::parse("").unwrap();
    assert_eq!(
        c.tui,
        TuiConfig {
            accent: None,
            palette: "amber".into(),
            accents: FamilyAccents::default(),
            spectrum: Vec::new(),
            colors: ColorDepth::Auto,
            monochrome: false,
            theme_background: true,
            mouse: true,
            intro: true,
            intro_sound: false,
            intro_effect: IntroEffect::Random,
            transition: Transition::Trace,
            title_meter: true,
            confirm_quit: true,
            host_audio: true,
            visualizer: true,
            graphs: true,
            graph_span: GraphSpan::M5,
            link_span: LinkSpan::H12,
            effects: EffectsConfig::default()
        }
    );
}

/// The transition is the trace while the file says nothing, and each of the four words
/// reads as its own transition, is written back as that word where a hand edit would put
/// it, and reads back as itself.
#[test]
fn the_transition_word_round_trips_through_the_file() {
    assert_eq!(
        Config::parse("schema = 1\n").unwrap().tui.transition,
        Transition::Trace
    );
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    for (word, transition) in [
        ("none", Transition::None),
        ("trace", Transition::Trace),
        ("noise", Transition::Noise),
        ("melt", Transition::Melt),
    ] {
        let text = format!("schema = 1\n[tui]\ntransition = \"{word}\"\n");
        assert_eq!(
            Config::parse(&text).unwrap().tui.transition,
            transition,
            "{word}"
        );

        // A file that says another word, so the saved word is a change to write.
        let other = if word == "melt" { "none" } else { "melt" };
        std::fs::write(
            &path,
            format!("schema = 1\n[tui]\ntransition = \"{other}\"\n"),
        )
        .unwrap();
        let tui = TuiConfig {
            transition,
            ..TuiConfig::default()
        };
        Config::update_tui(&path, |t| *t = tui.clone()).unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(
            saved.contains(&format!("\ntransition = \"{word}\"\n")),
            "{word} was not written as its own word:\n{saved}"
        );
        assert_eq!(Config::parse(&saved).unwrap().tui, tui, "{saved}");
    }

    // Written over the template, the word lands where the template shows it.
    let fresh = temp_dir("config-writer");
    let templated = fresh.join("config.toml");
    Config::create_from_template(&templated).unwrap();
    let tui = TuiConfig {
        transition: Transition::Melt,
        ..TuiConfig::default()
    };
    Config::update_tui(&templated, |t| *t = tui.clone()).unwrap();
    let saved = std::fs::read_to_string(&templated).unwrap();
    assert!(
        saved.contains("noise or melt.\ntransition = \"melt\"\n"),
        "the word was not written where the template shows it:\n{saved}"
    );
}

/// A word that names none of the four transitions is a configuration error that names
/// them all, the way a palette outside the presets is.
#[test]
fn a_transition_outside_the_four_is_rejected_naming_them() {
    for word in ["wipe", "Trace", ""] {
        let text = format!("schema = 1\n[tui]\ntransition = \"{word}\"\n");
        let err = Config::parse(&text).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m == "tui.transition must be one of none, trace, noise, melt"),
            "{word:?}: got {err:?}"
        );
    }
}

#[test]
fn effects_accept_a_bool_or_a_table() {
    let off = Config::parse("schema = 1\n[tui]\neffects = false\n")
        .unwrap()
        .tui
        .effects;
    assert!(!off.enabled && !off.eased);
    let some =
        Config::parse("schema = 1\n[tui]\neffects = { shimmer = false, hue_drift = true }\n")
            .unwrap()
            .tui
            .effects;
    assert!(some.enabled && some.eased && !some.shimmer && some.hue_drift);
    let d = TuiConfig::default().effects;
    assert!(
        d.enabled
            && d.eased
            && d.glow
            && d.shimmer
            && d.breathing
            && d.frames
            && d.fault_presence
            && d.peak_hold
            && !d.hue_drift
            && !d.spectrum_drift
            && d.list_fade
            && d.mascot
    );
}

/// The template documents the mascot switch among the effects.
#[test]
fn the_template_documents_the_mascot_switch() {
    assert!(Config::template().contains(
        "# mascot: Ratatoskr, the squirrel that comes out when called.\n# mascot = true\n"
    ));
}

/// What is written back reads as what was read: `effects` in whichever of its three shapes
/// means what this config holds, and an accent nothing set is no key at all rather than a
/// null. `config show --json` and the settings writer both render a config through this.
#[test]
fn the_tui_block_is_written_in_the_shape_the_file_writes_it() {
    let default =
        toml::Value::try_from(TuiConfig::default()).expect("the default [tui] block serialises");
    assert_eq!(
        default.get("effects"),
        Some(&toml::Value::Boolean(true)),
        "the defaults are one word, not a table"
    );
    assert_eq!(
        default.get("accent"),
        None,
        "an accent nothing set is no key at all"
    );
    let shown = serde_json::to_value(Config::default()).expect("the default config serialises");
    assert_eq!(
        shown.pointer("/tui/effects"),
        Some(&serde_json::json!(true)),
        "config show --json says what the file says"
    );
    assert_eq!(
        shown.pointer("/tui/accents"),
        Some(&serde_json::json!({})),
        "a family that overrides nothing is no key at all, as `accent` is"
    );

    assert_eq!(
        shown.pointer("/tui/spectrum"),
        None,
        "a spectrum nothing set is no key at all, as `accent` is"
    );
    let stops = Config::parse("schema = 1\n[tui.spectrum]\nstops = [\"#102030\", \"#e0a030\"]\n")
        .expect("a spectrum of two stops parses");
    assert_eq!(
        serde_json::to_value(stops)
            .expect("the config serialises")
            .pointer("/tui/spectrum"),
        Some(&serde_json::json!({ "stops": ["#102030", "#e0a030"] })),
        "the spectrum is written as the table the file spells it in"
    );

    let named = Config::parse("schema = 1\n[tui.accents]\namp = \"bright-cyan\"\n").unwrap();
    assert_eq!(
        serde_json::to_value(named)
            .expect("the config serialises")
            .pointer("/tui/accents"),
        Some(&serde_json::json!({ "amp": "bright-cyan" })),
        "only the family the file names is written"
    );

    let off = Config::parse("schema = 1\n[tui]\neffects = false\n")
        .unwrap()
        .tui
        .effects;
    assert_eq!(toml::Value::try_from(off), Ok(toml::Value::Boolean(false)));

    let some = Config::parse("schema = 1\n[tui]\neffects = { shimmer = false }\n")
        .unwrap()
        .tui
        .effects;
    let table = toml::Value::try_from(some).expect("a part-switched set serialises");
    assert_eq!(
        table.get("shimmer"),
        Some(&toml::Value::Boolean(false)),
        "a set that is neither shape is written out in full: {table}"
    );
    let written = format!("schema = 1\n[tui]\neffects = {table}\n");
    assert_eq!(
        Config::parse(&written).unwrap().tui.effects,
        some,
        "the table did not read back as what wrote it"
    );
}

#[test]
fn intro_defaults_on_and_can_be_switched_off() {
    assert!(Config::parse("schema = 1\n").unwrap().tui.intro);
    assert!(
        !Config::parse("schema = 1\n[tui]\nintro = false\n")
            .unwrap()
            .tui
            .intro
    );
}

/// The intro sound is off while the file says nothing, the key turns it on, a word that is no
/// bool is refused, and the switch is written alone and reads back as itself.
#[test]
fn the_intro_sound_is_off_by_default_and_the_key_turns_it_on() {
    assert!(!Config::parse("schema = 1\n").unwrap().tui.intro_sound);
    assert!(
        Config::parse("schema = 1\n[tui]\nintro_sound = true\n")
            .unwrap()
            .tui
            .intro_sound
    );
    assert!(Config::parse("schema = 1\n[tui]\nintro_sound = \"yes\"\n").is_err());

    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "schema = 1\n\n[tui]\nintro = true\n";
    std::fs::write(&path, original).unwrap();
    Config::update_tui(&path, |t| t.intro_sound = true).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("intro_sound = true"), "{saved}");
    assert!(Config::parse(&saved).unwrap().tui.intro_sound);
}

/// The template comments the key beside `intro`, and `docs/usage/config.md` carries the same line
/// and the same comment.
#[test]
fn the_template_and_usage_name_the_intro_sound_alike() {
    let template = Config::template();
    let block = "# Whether a short sound plays with the wordmark intro, through PipeWire's pw-play.\n# intro_sound = false\n";
    assert!(template.contains(block), "{template}");
    let at = |key: &str| template.find(key).unwrap();
    assert!(at("# intro = true") < at("# intro_sound = false"));
    let usage = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/usage/config.md"),
    )
    .expect("docs/usage/config.md reads");
    assert!(
        usage.contains(block),
        "usage/config.md's template block lacks the intro sound"
    );
}

/// The theme background is on while the file says nothing, one word turns it off, a word that
/// is no bool is refused, and the switch is written alone and reads back as itself.
#[test]
fn the_theme_background_defaults_on_and_round_trips_through_the_file() {
    assert!(Config::parse("schema = 1\n").unwrap().tui.theme_background);
    assert!(
        !Config::parse("schema = 1\n[tui]\ntheme_background = false\n")
            .unwrap()
            .tui
            .theme_background
    );
    assert!(Config::parse("schema = 1\n[tui]\ntheme_background = \"yes\"\n").is_err());
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n[tui]\nmouse = false\n").unwrap();
    Config::update_tui(&path, |t| t.theme_background = false).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        text, "schema = 1\n[tui]\nmouse = false\ntheme_background = false\n",
        "{text}"
    );
    assert!(!Config::parse(&text).unwrap().tui.theme_background);
    assert!(Config::template().contains(
        "# Whether each palette paints its own dark greys under the screen, its panes and its boxes.\n\
             # theme_background = true\n"
    ));
}

/// The title meter is on while the file says nothing, and one word turns it off.
#[test]
fn the_title_meter_defaults_on_and_can_be_switched_off() {
    assert!(Config::parse("schema = 1\n").unwrap().tui.title_meter);
    assert!(
        !Config::parse("schema = 1\n[tui]\ntitle_meter = false\n")
            .unwrap()
            .tui
            .title_meter
    );
}

/// The switch is written where a hand edit would put it and reads back as itself.
#[test]
fn the_title_meter_round_trips_through_the_file() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    let tui = TuiConfig {
        title_meter: false,
        ..TuiConfig::default()
    };
    Config::update_tui(&path, |t| *t = tui.clone()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("title_meter = false"), "{text}");
    assert_eq!(Config::parse(&text).unwrap().tui, tui, "{text}");
}

/// The quit question is on while the file says nothing, one word turns it off, and the
/// switch is written where a hand edit would put it and reads back as itself.
#[test]
fn the_quit_question_defaults_on_and_round_trips_through_the_file() {
    assert!(Config::parse("schema = 1\n").unwrap().tui.confirm_quit);
    assert!(
        !Config::parse("schema = 1\n[tui]\nconfirm_quit = false\n")
            .unwrap()
            .tui
            .confirm_quit
    );
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::create_from_template(&path).unwrap();
    let tui = TuiConfig {
        confirm_quit: false,
        ..TuiConfig::default()
    };
    Config::update_tui(&path, |t| *t = tui.clone()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("Ctrl-C quits at once either way.\nconfirm_quit = false\n"),
        "the switch was not written where the template shows it:\n{text}"
    );
    assert_eq!(Config::parse(&text).unwrap().tui, tui, "{text}");
}

/// Host audio and the visualizer are on while the file says nothing, one word turns each
/// off, and each switch is written where the template shows it and reads back as itself.
#[test]
fn host_audio_and_the_visualizer_default_on_and_round_trip_through_the_file() {
    let defaults = Config::parse("schema = 1\n").unwrap().tui;
    assert!(defaults.host_audio && defaults.visualizer);
    let off = Config::parse("schema = 1\n[tui]\nhost_audio = false\nvisualizer = false\n")
        .unwrap()
        .tui;
    assert!(!off.host_audio && !off.visualizer);
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::create_from_template(&path).unwrap();
    let tui = TuiConfig {
        host_audio: false,
        visualizer: false,
        ..TuiConfig::default()
    };
    Config::update_tui(&path, |t| *t = tui.clone()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("with its volume and transport.\nhost_audio = false\n"),
        "host_audio was not written where the template shows it:\n{text}"
    );
    assert!(
        text.contains("a spectrum of what the host plays.\nvisualizer = false\n"),
        "visualizer was not written where the template shows it:\n{text}"
    );
    assert_eq!(Config::parse(&text).unwrap().tui, tui, "{text}");
}

#[test]
fn a_palette_outside_the_four_presets_is_rejected() {
    for name in ["amber", "ice", "red", "neon"] {
        let text = format!("[tui]\npalette = \"{name}\"\n");
        assert_eq!(Config::parse(&text).unwrap().tui.palette, name);
    }
    let err = Config::parse("schema = 1\n[tui]\npalette = \"sepia\"\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m == "tui.palette must be one of amber, ice, red, neon"),
        "got {err:?}"
    );
}

/// The `[tui.spectrum]` table names the stops the glow travels in place of the palette's
/// own, and every one of them is a `#rrggbb` colour: the stops are blended into one
/// another, and an ANSI colour name is a terminal slot rather than a colour to blend from.
#[test]
fn a_spectrum_of_stops_parses_and_a_stop_that_is_no_colour_is_refused() {
    let c =
        Config::parse("schema = 1\n[tui.spectrum]\nstops = [\"#5a1a08\", \"#ffffff\"]\n").unwrap();
    assert_eq!(c.tui.spectrum, ["#5a1a08", "#ffffff"]);
    assert!(
        Config::parse("schema = 1\n[tui]\npalette = \"ice\"\n")
            .unwrap()
            .tui
            .spectrum
            .is_empty(),
        "a config that names no stops travels the palette's own"
    );
    let err = Config::parse("schema = 1\n[tui.spectrum]\nstops = [\"teal\"]\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("teal")),
        "got {err:?}"
    );
}

#[test]
fn family_accents_parse_and_are_checked_like_the_single_accent() {
    let c = Config::parse("[tui.accents]\ndac = \"#123456\"\namp = \"bright-cyan\"\n").unwrap();
    assert_eq!(c.tui.accents.dac.as_deref(), Some("#123456"));
    assert_eq!(c.tui.accents.amp.as_deref(), Some("bright-cyan"));
    assert_eq!(c.tui.accents.preamp, None);
    assert!(matches!(
        Config::parse("[tui.accents]\npreamp = \"chartreuse\"\n").unwrap_err(),
        ConfigError::Parse(_)
    ));
}

#[test]
fn an_absent_accent_is_no_override_and_a_named_one_still_parses() {
    assert_eq!(Config::parse("").unwrap().tui.accent, None);
    assert_eq!(Config::parse("[tui]\n").unwrap().tui.accent, None);
    assert_eq!(
        Config::parse("[tui]\naccent = \"blue\"\n")
            .unwrap()
            .tui
            .accent
            .as_deref(),
        Some("blue"),
        "a config that names an accent overrides every family with it"
    );
}

#[test]
fn tui_block_parses_and_rejects_an_unknown_colour() {
    let c =
        Config::parse("[tui]\naccent = \"magenta\"\ncolors = \"256\"\nmouse = false\n").unwrap();
    assert_eq!(c.tui.accent.as_deref(), Some("magenta"));
    assert_eq!(c.tui.colors, ColorDepth::Ansi256);
    assert!(!c.tui.mouse);
    assert_eq!(
        Config::parse("[tui]\naccent = \"#5fafff\"")
            .unwrap()
            .tui
            .accent
            .as_deref(),
        Some("#5fafff")
    );
    assert!(matches!(
        Config::parse("[tui]\naccent = \"chartreuse\"").unwrap_err(),
        ConfigError::Parse(_)
    ));
    assert!(matches!(
        Config::parse("[tui]\ncolors = \"millions\"").unwrap_err(),
        ConfigError::Parse(_)
    ));
}

/// The message a mistyped key under `[tui.effects]` gets, naming the key and every valid one.
#[test]
fn an_unknown_effects_key_names_itself_and_the_valid_ones() {
    let err = Config::parse("schema = 1\n[tui]\neffects = { breathng = false }\n")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains(
            r#"tui.effects: unknown key "breathng"; the keys are enabled, eased, glow, shimmer, breathing, frames, fault_presence, peak_hold, hue_drift, spectrum_drift, list_fade, mascot"#
        ),
        "{err}"
    );
    let err = Config::parse("schema = 1\n[tui.effects]\nglw = true\n")
        .unwrap_err()
        .to_string();
    assert!(err.contains(r#"unknown key "glw""#), "{err}");
}

#[test]
fn a_non_boolean_effects_value_names_its_key() {
    let err = Config::parse("schema = 1\n[tui]\neffects = { glow = \"yes\" }\n")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("tui.effects.glow must be true or false"),
        "{err}"
    );
    let err = Config::parse("schema = 1\n[tui]\neffects = 3\n")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("expected tui.effects to be true, false or a table"),
        "{err}"
    );
}

#[test]
fn effects_still_take_a_switch_or_a_table_of_known_keys() {
    let all = "enabled = true, eased = true, glow = true, shimmer = true, breathing = true, \
               frames = true, fault_presence = true, peak_hold = true, hue_drift = true, \
               spectrum_drift = true, list_fade = false";
    let table = Config::parse(&format!("schema = 1\n[tui]\neffects = {{ {all} }}\n"))
        .unwrap()
        .tui
        .effects;
    assert!(table.hue_drift && table.spectrum_drift && table.glow && !table.list_fade);
    assert_eq!(super::super::parse::EFFECT_KEYS[10], "list_fade");
    assert!(!EffectsConfig::off().list_fade);
    assert_eq!(
        Config::parse("schema = 1\n[tui]\neffects = true\n")
            .unwrap()
            .tui
            .effects,
        EffectsConfig::default()
    );
    assert_eq!(
        Config::parse("schema = 1\n[tui]\neffects = false\n")
            .unwrap()
            .tui
            .effects,
        EffectsConfig::off()
    );
}

/// Every key the effects table names in its error message is one the parser takes: the list
/// and the visitor's match arms cannot drift apart.
#[test]
fn every_effect_key_parses() {
    for key in super::super::parse::EFFECT_KEYS {
        let text = format!("[tui]\neffects = {{ {key} = true }}");
        Config::parse(&text).unwrap_or_else(|e| panic!("{key}: {e}"));
    }
}

/// The graphs are on, spanning five minutes and twelve hours, while the file says nothing;
/// each key reads as itself, is written where the template shows it, and only when it changed.
#[test]
fn the_graph_keys_default_read_and_round_trip_through_the_file() {
    let defaults = Config::parse("schema = 1\n").unwrap().tui;
    assert!(defaults.graphs);
    assert_eq!(defaults.graph_span, GraphSpan::M5);
    assert_eq!(defaults.link_span, LinkSpan::H12);
    assert_eq!(
        defaults.graph_span.duration(),
        std::time::Duration::from_secs(300)
    );
    assert_eq!(
        defaults.link_span.duration(),
        std::time::Duration::from_secs(12 * 3600)
    );
    let set = Config::parse(
        "schema = 1\n[tui]\ngraphs = false\ngraph_span = \"15m\"\nlink_span = \"6h\"\n",
    )
    .unwrap()
    .tui;
    assert!(!set.graphs);
    assert_eq!(set.graph_span, GraphSpan::M15);
    assert_eq!(set.link_span, LinkSpan::H6);

    let template = Config::template();
    for line in [
        "# graphs = true",
        "# graph_span = \"5m\"",
        "# link_span = \"12h\"",
    ] {
        assert!(template.contains(line), "the template lacks {line}");
    }

    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::create_from_template(&path).unwrap();
    Config::update_tui(&path, |t| t.graph_span = GraphSpan::M60).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("\ngraph_span = \"60m\"\n"), "{text}");
    assert!(
        !text.contains("\ngraphs = ") && !text.contains("\nlink_span = "),
        "{text}"
    );
    assert_eq!(Config::parse(&text).unwrap().tui.graph_span, GraphSpan::M60);
}

/// A span outside its fixed set is a configuration error that names the set.
#[test]
fn a_span_outside_its_set_is_rejected_naming_the_set() {
    for (key, word, set) in [
        ("graph_span", "7m", "1m, 2m, 3m, 5m, 10m, 15m, 30m, 60m"),
        ("graph_span", "5M", "1m, 2m, 3m, 5m, 10m, 15m, 30m, 60m"),
        ("link_span", "2h", "1h, 3h, 6h, 12h, 24h"),
        ("link_span", "", "1h, 3h, 6h, 12h, 24h"),
    ] {
        let text = format!("schema = 1\n[tui]\n{key} = \"{word}\"\n");
        let err = Config::parse(&text).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if *m == format!("tui.{key} must be one of {set}")),
            "{key} {word:?}: got {err:?}"
        );
    }
}

/// The mascot switch defaults on, reads from the table and the inline form, and goes off with
/// effects off.
#[test]
fn the_mascot_switch_parses_defaults_on_and_goes_off_with_the_rest() {
    assert!(EffectsConfig::default().mascot);
    assert!(!EffectsConfig::off().mascot);
    let off = Config::parse("schema = 1\n[tui.effects]\nmascot = false\n")
        .unwrap()
        .tui
        .effects;
    assert!(!off.mascot && off.glow);
    let on = Config::parse("schema = 1\n[tui]\neffects = { glow = false }\n")
        .unwrap()
        .tui
        .effects;
    assert!(on.mascot);
}

/// The mascot switch is saved with the other effects, whichever shape the file holds them in.
#[test]
fn the_mascot_switch_is_saved_with_the_other_effects() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::create_from_template(&path).unwrap();
    Config::update_tui(&path, |t| t.effects.mascot = false).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("mascot = false"), "{saved}");
    assert!(!Config::parse(&saved).unwrap().tui.effects.mascot);

    // A file with no table of its own and every switch at its default keeps the one word.
    let plain = dir.join("plain.toml");
    std::fs::write(&plain, "schema = 1\n[tui]\neffects = false\n").unwrap();
    Config::update_tui(&plain, |t| t.effects = EffectsConfig::default()).unwrap();
    let kept = std::fs::read_to_string(&plain).unwrap();
    assert!(kept.contains("effects = true"), "{kept}");
    assert!(!kept.contains("mascot"), "{kept}");

    // One switch off writes the table with all twelve.
    Config::update_tui(&plain, |t| t.effects.mascot = false).unwrap();
    let table = std::fs::read_to_string(&plain).unwrap();
    assert!(table.contains("mascot = false"), "{table}");
    assert!(!Config::parse(&table).unwrap().tui.effects.mascot);
}

/// The intro effect is random while the file says nothing, each of the three words reads as
/// its effect, and any other word is refused with the three named.
#[test]
fn the_intro_effect_defaults_to_random_and_reads_each_word() {
    assert_eq!(
        Config::parse("schema = 1\n").unwrap().tui.intro_effect,
        IntroEffect::Random
    );
    for (word, effect) in [
        ("random", IntroEffect::Random),
        ("fade", IntroEffect::Fade),
        ("particles", IntroEffect::Particles),
    ] {
        let text = format!("schema = 1\n[tui]\nintro_effect = \"{word}\"\n");
        assert_eq!(
            Config::parse(&text).unwrap().tui.intro_effect,
            effect,
            "{word}"
        );
    }
    let error = Config::parse("schema = 1\n[tui]\nintro_effect = \"plasma\"\n")
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("tui.intro_effect must be one of random, fade, particles"),
        "{error}"
    );
}

/// The key is written alone, sits between the intro and its sound in the template, and
/// docs/usage/config.md carries the same two lines.
#[test]
fn the_intro_effect_is_written_alone_and_sits_between_the_intro_and_its_sound() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n[tui]\nintro = true\n").unwrap();
    Config::update_tui(&path, |t| t.intro_effect = IntroEffect::Particles).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        text, "schema = 1\n[tui]\nintro = true\nintro_effect = \"particles\"\n",
        "{text}"
    );
    let template = Config::template();
    let block = "# Which effect builds the wordmark intro: random, fade or particles.\n\
                 # intro_effect = \"random\"\n";
    assert!(template.contains(block), "{template}");
    let at = |key: &str| template.find(key).unwrap();
    assert!(at("# intro = true") < at("# intro_effect = "));
    assert!(at("# intro_effect = ") < at("# intro_sound = "));
    let usage = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/usage/config.md"),
    )
    .expect("docs/usage/config.md reads");
    assert!(
        usage.contains(block),
        "usage/config.md's template block lacks the intro effect"
    );
}

/// `get` and `set` reach the field of each family, and only that one.
#[test]
fn family_accents_are_read_and_written_by_family() {
    use svolder_profiles::Family;
    let mut accents = FamilyAccents::default();
    for (family, colour) in [
        (Family::Dac, "#111111"),
        (Family::Amp, "#222222"),
        (Family::Preamp, "#333333"),
    ] {
        accents.set(family, Some(colour.into()));
        assert_eq!(accents.get(family), Some(colour));
    }
    assert_eq!(accents.dac.as_deref(), Some("#111111"));
    assert_eq!(accents.amp.as_deref(), Some("#222222"));
    assert_eq!(accents.preamp.as_deref(), Some("#333333"));
    accents.set(Family::Amp, None);
    assert_eq!(accents.get(Family::Amp), None);
    assert_eq!(accents.get(Family::Dac), Some("#111111"));
}
