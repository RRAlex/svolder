// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Adding, updating, renaming and removing a device in the file.

use super::*;

/// The device writers edit the `[[device]]` blocks and nothing else: the comments, the
/// keys and the tables around them come through byte for byte.
#[test]
fn devices_are_added_updated_and_removed_without_touching_the_rest() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "# mine\nschema = 1\nadapter = \"hci1\" # dongle\n\n[[device]]\nname = \"amp\"\nudid = \"0102030405060708\"\n\n[tui]\npalette = \"ice\"\n";
    std::fs::write(&path, original).unwrap();
    let dac = DeviceConfig {
        name: "dac".into(),
        udid: "1112131415161718".into(),
        profile: Some("bifrost3".into()),
        passphrase: String::new(),
        inputs: BTreeMap::new(),
    };
    Config::add_device(&path, &dac).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.starts_with("# mine\nschema = 1\nadapter = \"hci1\" # dongle\n"),
        "{text}"
    );
    assert!(text.contains("[tui]\npalette = \"ice\"\n"), "{text}");
    assert_eq!(Config::parse(&text).unwrap().devices.len(), 2);

    let renamed = DeviceConfig {
        name: "bifrost".into(),
        ..dac.clone()
    };
    Config::update_device(&path, "dac", &renamed).unwrap();
    assert_eq!(
        Config::parse(&std::fs::read_to_string(&path).unwrap())
            .unwrap()
            .devices[1]
            .name,
        "bifrost"
    );

    Config::remove_device(&path, "amp").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let cfg = Config::parse(&text).unwrap();
    assert_eq!(cfg.devices.len(), 1);
    assert!(text.contains("adapter = \"hci1\" # dongle"), "{text}");
    assert!(matches!(
        Config::remove_device(&path, "nobody"),
        Err(ConfigError::Parse(_))
    ));
    assert!(matches!(
        Config::update_device(&path, "nobody", &dac),
        Err(ConfigError::Parse(_))
    ));
}

/// A device written into a file the template made sits at the end of it, after the
/// comments that explain the rest — and taking it out again puts the file back as it was,
/// rather than carrying those comments off with the block.
#[test]
fn a_device_added_to_a_template_and_taken_out_again_leaves_the_file_as_it_was() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::add_device(&path, &device("dac", "1112131415161718")).unwrap();
    let added = std::fs::read_to_string(&path).unwrap();
    assert!(
            added.contains("# stops = [\"#5a1a08\", \"#c04a10\", \"#e0a030\", \"#f6dca0\"]\n\n[[device]]\nname = \"dac\"\n"),
            "the block goes after the comments that were at the end of the file:\n{added}"
        );

    Config::remove_device(&path, "dac").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), Config::template());

    // A comment written right above a block is the block's own and goes with it; one a
    // blank line sets apart from it is the file's, and is handed to what follows.
    let original = "schema = 1\n\n# my devices\n\n# the study dac\n[[device]]\nname = \"dac\"\nudid = \"1112131415161718\"\n\n[[device]]\nname = \"amp\"\nudid = \"0102030405060708\"\n";
    std::fs::write(&path, original).unwrap();
    Config::remove_device(&path, "dac").unwrap();
    let left = std::fs::read_to_string(&path).unwrap();
    assert!(left.contains("# my devices"), "{left}");
    assert!(!left.contains("# the study dac"), "{left}");
    assert_eq!(Config::parse(&left).unwrap().devices.len(), 1);
}

/// Two devices a selector cannot tell apart never reach the file, whether the second one is
/// added under a taken name or renamed onto one.
#[test]
fn a_device_whose_name_is_taken_is_refused() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::add_device(&path, &device("dac", "1112131415161718")).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();
    let err = Config::add_device(&path, &device("DAC", "0102030405060708")).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("dac")),
        "got {err:?}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);

    Config::add_device(&path, &device("amp", "0102030405060708")).unwrap();
    let before = std::fs::read_to_string(&path).unwrap();
    let err = Config::update_device(&path, "dac", &device("AMP", "1112131415161718")).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("amp")),
        "got {err:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        before,
        "a refused rename wrote to the file"
    );

    // Renaming a block to the name it has, or to another spelling of it, is no collision,
    // and neither is a rename onto a name nothing carries.
    Config::update_device(&path, "dac", &device("dac", "1112131415161718")).unwrap();
    Config::update_device(&path, "dac", &device("DAC", "1112131415161718")).unwrap();
    Config::update_device(&path, "DAC", &device("bifrost", "1112131415161718")).unwrap();
    let names: Vec<String> = Config::parse(&std::fs::read_to_string(&path).unwrap())
        .unwrap()
        .devices
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(names, ["bifrost", "amp"]);
}

/// The default device is written and cleared where the template comments it out, and the
/// file is only ever left naming a device it also configures.
#[test]
fn the_default_device_is_written_where_the_template_comments_it_out() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, Config::template()).unwrap();
    Config::add_device(&path, &device("dac", "1112131415161718")).unwrap();

    Config::set_default_device(&path, Some("dac")).unwrap();
    let set = std::fs::read_to_string(&path).unwrap();
    assert!(set.contains("\ndefault_device = \"dac\"\n"), "{set}");
    assert!(!set.contains("# default_device = "), "{set}");
    let explained = set
        .lines()
        .position(|l| l.contains("when it names none"))
        .unwrap();
    let live = set
        .lines()
        .position(|l| l == "default_device = \"dac\"")
        .unwrap();
    assert_eq!(live, explained + 1, "{set}");
    assert_eq!(
        Config::parse(&set).unwrap().default_device.as_deref(),
        Some("dac")
    );

    // A default naming a device the file does not configure is written; it loads with a
    // warning.
    Config::set_default_device(&path, Some("nobody")).unwrap();
    let dangling = Config::load_from(&path).unwrap();
    assert_eq!(dangling.dangling_default(), Some("nobody"));

    Config::set_default_device(&path, None).unwrap();
    let cleared = std::fs::read_to_string(&path).unwrap();
    assert!(cleared.contains("# default_device = \"\"\n"), "{cleared}");
    assert_eq!(Config::parse(&cleared).unwrap().default_device, None);
    Config::remove_device(&path, "dac").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), Config::template());
}

/// The device writers carry `default_device` along: a rename takes it to the new name and a
/// removal puts its commented line back, so the file is never left naming a default it does
/// not configure — which is a file that would not load.
#[test]
fn renaming_and_removing_a_device_carry_the_default_along() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "# mine\nschema = 1\ndefault_device = \"dac\"\nadapter = \"hci1\" # dongle\n\n[[device]]\nname = \"dac\"\nudid = \"1112131415161718\"\n\n[[device]]\nname = \"amp\"\nudid = \"0102030405060708\"\n";
    std::fs::write(&path, original).unwrap();

    Config::update_device(&path, "dac", &device("bifrost", "1112131415161718")).unwrap();
    let renamed = std::fs::read_to_string(&path).unwrap();
    assert!(
        renamed.contains("default_device = \"bifrost\""),
        "{renamed}"
    );
    assert!(
        renamed.starts_with("# mine\nschema = 1\n")
            && renamed.contains("adapter = \"hci1\" # dongle"),
        "the rest of the file moved:\n{renamed}"
    );
    let config = Config::parse(&renamed).unwrap();
    assert_eq!(config.default_device.as_deref(), Some("bifrost"));

    // A default naming another device is left where it is.
    Config::set_default_device(&path, Some("amp")).unwrap();
    Config::update_device(&path, "bifrost", &device("dac", "1112131415161718")).unwrap();
    let other = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        Config::parse(&other).unwrap().default_device.as_deref(),
        Some("amp")
    );

    Config::set_default_device(&path, Some("dac")).unwrap();
    Config::remove_device(&path, "dac").unwrap();
    let gone = std::fs::read_to_string(&path).unwrap();
    assert!(gone.contains("# default_device = \"\"\n"), "{gone}");
    let config = Config::parse(&gone).unwrap();
    assert_eq!(config.default_device, None);
    assert_eq!(config.devices.len(), 1);
}
