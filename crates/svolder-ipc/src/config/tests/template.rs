// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The template: that it is the default configuration, and how the writers start from it.

use super::*;

/// The template is the file a reader learns the configuration from: every key is there,
/// commented out at the value svolder uses while it is absent, so uncommenting the lot
/// leaves exactly the defaults behind.
#[test]
fn the_template_uncommented_is_the_default_config() {
    let text: String = Config::template()
        .lines()
        .map(|l| {
            l.strip_prefix("# ")
                // A line that only closes an array — the commented `[[preset]]` example's
                // `band` array spans several lines — is uncommented too, so the array it
                // closes is not left open.
                .filter(|r| r.contains(" = ") || r.starts_with('[') || r.trim_end() == "]")
                .unwrap_or(l)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let parsed = Config::parse(&text).unwrap();
    // The stops are the one key the template shows at a value rather than at nothing:
    // they are the default palette's own, which is what the glow travels while the key is
    // absent, so uncommenting them changes no colour either.
    assert_eq!(parsed.tui.spectrum, template_stops());
    assert_eq!(
        TuiConfig {
            spectrum: Vec::new(),
            ..parsed.tui
        },
        TuiConfig::default()
    );
    assert!(parsed.daemon.autospawn);
    assert_eq!(parsed.adapter, None);
    // `max_volume_db` is the one `[mqtt]` key the template shows at an example rather
    // than at nothing: a number has no blank spelling for "no cap", unlike every other
    // optional key above, so uncommenting it does change one thing.
    assert_eq!(parsed.mqtt.max_volume_db, Some(-10.0));
    assert_eq!(
        MqttConfig {
            max_volume_db: None,
            ..parsed.mqtt
        },
        MqttConfig::default()
    );
    // `max_volume_db` is the one `[web]` key shown at an example, as in `[mqtt]`.
    assert_eq!(parsed.web.max_volume_db, Some(-10.0));
    assert_eq!(
        WebConfig {
            max_volume_db: None,
            ..parsed.web
        },
        WebConfig::default()
    );
    assert!(Config::template().contains("\n[mqtt]\n"));
    assert!(Config::template().contains("# port = 1883"));
    assert!(Config::template().contains("# base_topic = \"svolder\""));
    assert!(Config::template().contains("# discovery_prefix = \"homeassistant\""));
    assert!(Config::template().contains(
        "# The broker password, when it wants one; SVOLDER_MQTT_PASSWORD overrides this.\n\
             # password = \"\"\n"
    ));
    assert!(Config::template().contains("# max_volume_db = -10.0"));
    assert!(Config::template().contains("# palette = \"amber\""));
    assert!(Config::template().contains(
        "# Whether the terminal's title carries one bar per channel of the device's meters, \
             else of what the host plays.\n# title_meter = true\n"
    ));
    assert!(Config::template().contains(
        "# Whether q asks before svolder tui quits; Ctrl-C quits at once either way.\n\
             # confirm_quit = true\n"
    ));
    assert!(Config::template().contains(
        "# Whether svolder tui shows what the host plays, with its volume and transport.\n\
             # host_audio = true\n"
    ));
    assert!(Config::template().contains(
        "# Whether the device screen draws a spectrum of what the host plays.\n\
             # visualizer = true\n"
    ));
    assert!(Config::template().contains("# autospawn = true"));
    assert!(Config::template().contains(
        "# How one screen gives way to the next: none, trace, noise or melt.\n\
             # transition = \"trace\"\n"
    ));
    assert!(Config::template().contains("\n[tui.spectrum]\n"));
    assert!(Config::template().contains("# stops = [\"#5a1a08\", "));
    assert!(Config::template().contains(
        "# Whether a light runs once round the frame the keys move or arrive to.\n\
             # frames = true\n"
    ));
    assert!(Config::template().contains(
        "# Whether the tone curve's colours scroll: round the hue circle, or along red's and \
             neon's ramps.\n# spectrum_drift = false\n"
    ));
    assert!(Config::template().contains(
        "# Whether the rows of a list dim with their distance from the selected row.\n\
             # list_fade = true\n"
    ));
    // The example hops uncomment into a declared one and a linked one.
    assert_eq!(parsed.hops.len(), 2);
    assert_eq!(parsed.hops[0].id(), "HD 650");
    assert_eq!(parsed.hops[1].id(), "vali");
    // The example preset's own block uncomments into one real preset.
    assert!(Config::template().contains("# [[preset]]"));
    assert!(Config::template().contains("# name = \"HD650 warm\""));
    assert_eq!(parsed.presets.len(), 2);
    let imported = &parsed.presets[1];
    assert_eq!(imported.name, "Sennheiser HD 650");
    assert_eq!(
        imported.source.as_ref().unwrap().correction.filters.len(),
        2
    );
    let example = &parsed.presets[0];
    assert_eq!(example.name, "HD650 warm");
    assert_eq!(example.bands, 5);
    assert_eq!(example.mode, EqMode::Eq);
    assert_eq!(example.loudness, 0);
    assert_eq!(
        example.band,
        vec![
            PresetBand {
                f: 40.0,
                g: 3.0,
                q: 0.7
            },
            PresetBand {
                f: 160.0,
                g: 1.5,
                q: 0.7
            },
            PresetBand {
                f: 632.0,
                g: 0.0,
                q: 4.3
            },
            PresetBand {
                f: 2515.0,
                g: 0.0,
                q: 4.3
            },
            PresetBand {
                f: 10000.0,
                g: 2.0,
                q: 0.7
            },
        ]
    );
}

/// Left as it is, the template is a configuration with nothing set: the example device is
/// commented out header and all, so a first run reads the defaults off it.
#[test]
fn the_template_as_it_stands_is_the_default_config() {
    assert_eq!(
        Config::parse(&Config::template()).unwrap(),
        Config::default()
    );
}

/// The first file a host gets is the template itself, directories and all; a file already
/// there is never written over by the writer that makes the first one.
#[test]
fn the_first_config_file_is_the_template_and_an_existing_one_is_left_alone() {
    let dir = temp_dir("config-writer");
    let path = dir.join("nested").join("config.toml");
    Config::create_from_template(&path).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), Config::template());

    std::fs::write(&path, "schema = 1\nadapter = \"hci1\"\n").unwrap();
    assert!(Config::create_from_template(&path).is_err());
    assert_eq!(
        Config::load_from(&path).unwrap().adapter,
        Some("hci1".to_string())
    );
}

/// A key the file carries only as the template's commented line is written where that line
/// stood, under the comment that explains it, rather than being appended to the table.
#[test]
fn setting_a_key_replaces_its_commented_default_in_place() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, Config::template()).unwrap();
    let tui = TuiConfig {
        palette: "neon".into(),
        ..TuiConfig::default()
    };
    Config::update_tui(&path, |t| *t = tui.clone()).unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("palette = \"neon\""), "{saved}");
    assert!(!saved.contains("# palette = \"amber\""), "{saved}");
    // The explaining comment stays above.
    let before = saved
        .lines()
        .position(|l| l.contains("colour preset"))
        .unwrap();
    let live = saved
        .lines()
        .position(|l| l == "palette = \"neon\"")
        .unwrap();
    assert_eq!(live, before + 1, "{saved}");
}

/// The decoration search reads the run of comments above each of a table's own keys and
/// then the run the document prints after them, and splits the one it finds at the line it
/// found.
#[test]
fn the_decor_search_finds_a_commented_key_above_a_key_and_after_one() {
    let text = "schema = 1\n\n[tui]\n# above\n# palette = \"amber\"\n# about mouse\nmouse = true\n\n[tui.effects]\n# about the trail\n# glow = true\n";
    let mut doc = text.parse::<DocumentMut>().unwrap();

    // `palette` is commented above a key the table does carry.
    let table = table_at_mut(doc.as_table_mut(), &["tui"]).unwrap();
    let (slot, head, tail) = commented_key(table, "", "palette").unwrap();
    assert!(matches!(slot, Slot::Before(ref key) if key == "mouse"));
    assert_eq!(head, "# above\n");
    assert_eq!(tail, "# about mouse\n");

    // `glow` is commented after the last key `[tui.effects]` carries, which is none, so it
    // lives in the run the document prints next — here its own trailing decoration.
    let following = doc.trailing().as_str().unwrap().to_string();
    let table = table_at_mut(doc.as_table_mut(), &["tui", "effects"]).unwrap();
    let (slot, head, tail) = commented_key(table, &following, "glow").unwrap();
    assert!(matches!(slot, Slot::Following));
    assert_eq!(head, "# about the trail\n");
    assert_eq!(tail, "");

    // A key nothing comments out is nowhere to be found.
    assert!(commented_key(table, &following, "shimmer").is_none());
    // And a key is not found in a longer one that starts with its name.
    assert!(!comments_out("# glowing = true", "glow"));
    assert!(comments_out("#glow=true", "glow"));
}

/// Every writer starts an absent file from the template, so the first configuration
/// svolder writes explains itself.
#[test]
fn a_writer_starts_an_absent_file_from_the_template() {
    let dir = temp_dir("config-writer");
    for (name, write) in [("tui.toml", 0), ("device.toml", 1), ("adapter.toml", 2)] {
        let path = dir.join(name);
        match write {
            0 => Config::update_tui(&path, |t| *t = TuiConfig::default()).unwrap(),
            1 => Config::add_device(&path, &device("dac", "1112131415161718")).unwrap(),
            _ => Config::set_adapter(&path, Some("hci1")).unwrap(),
        }
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("\nschema = 1\n") && text.contains("[tui.accents]"),
            "{name} did not start from the template:\n{text}"
        );
    }
    // The adapter took the template's commented line's place rather than being appended.
    let text = std::fs::read_to_string(dir.join("adapter.toml")).unwrap();
    assert!(text.contains("\nadapter = \"hci1\"\n"), "{text}");
    assert!(!text.contains("# adapter = "), "{text}");
    assert_eq!(
        Config::parse(&text).unwrap().adapter.as_deref(),
        Some("hci1")
    );
    // And it goes away again, leaving the rest of the file as it was.
    Config::set_adapter(&dir.join("adapter.toml"), None).unwrap();
    let gone = std::fs::read_to_string(dir.join("adapter.toml")).unwrap();
    assert!(!gone.contains("adapter = \"hci1\""), "{gone}");
    assert_eq!(Config::parse(&gone).unwrap().adapter, None);
}

/// The template carries the `[host]` table with its one key commented out at the default, and
/// `docs/usage/config.md` and the daemon page's sample show the same.
#[test]
fn template_has_the_host_table() {
    let template = Config::template();
    let block = "\n# Whether svolderd watches the computer's music player and the route its sound \
                 takes to the DAC.\n[host]\n\n\
                 # Whether the daemon reads the player and the route.\n# enabled = true\n";
    assert!(template.contains(block), "{template}");
    let at = |key: &str| template.find(key).unwrap();
    assert!(at("\n[autoeq]\n") < at("\n[host]\n") && at("\n[host]\n") < at("\n[tui]\n"));
    let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/usage");
    let usage = std::fs::read_to_string(docs.join("config.md")).expect("usage/config.md reads");
    assert!(
        usage.contains(block),
        "usage/config.md's template block lacks [host]"
    );
    let daemon = std::fs::read_to_string(docs.join("daemon.md")).expect("usage/daemon.md reads");
    assert!(
        daemon.contains("\n[host]  "),
        "usage/daemon.md's sample lacks [host]"
    );
}
