// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How the writers treat the file: its mode, concurrent saves, the lock and failed writes.

use super::*;

/// A file that would not load again is not written: the Settings box would have saved a
/// configuration the next run refuses to start on.
#[test]
fn a_file_the_result_would_not_load_from_is_left_alone() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "schema = 1\n[[device]]\nname = \"x\"\nudid = \"nothex\"\n";
    std::fs::write(&path, original).unwrap();
    let err = Config::update_tui(&path, |t| *t = TuiConfig::default()).unwrap_err();
    assert!(matches!(err, ConfigError::Parse(_)), "got {err:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
}

/// The new text is written beside the file and renamed over it, so the file is never a
/// truncated half of either version — and the directory is left holding the configuration
/// alone, with no `.tmp` beside it for the next save or a reader to trip over.
#[test]
fn a_save_leaves_the_directory_holding_the_configuration_alone() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();

    let mut tui = TuiConfig {
        palette: "ice".into(),
        ..TuiConfig::default()
    };
    Config::update_tui(&path, |t| *t = tui.clone()).unwrap();
    // And again, over a file this writer itself wrote.
    tui.monochrome = true;
    Config::update_tui(&path, |t| *t = tui.clone()).unwrap();

    assert_eq!(Config::load_from(&path).unwrap().tui, tui);
    let mut left: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        ["config.toml", "config.toml.lock"],
        "a temporary file was left behind"
    );
}

/// A file that cannot be written is reported as the I/O failure it is, for the settings
/// screen to show.
#[test]
fn a_path_that_cannot_be_written_is_reported() {
    let dir = temp_dir("config-writer");
    let blocked = dir.join("config.toml");
    std::fs::write(&blocked, "schema = 1\n").unwrap();
    // The parent of this path is a file, so neither the directory nor the file under it
    // can be made.
    let path = blocked.join("nested/config.toml");
    let err = Config::update_tui(&path, |t| *t = TuiConfig::default()).unwrap_err();
    assert!(matches!(err, ConfigError::Io(_, _)), "got {err:?}");
}

/// A config can carry a device passphrase, so a user who has locked it down with
/// `chmod 600` keeps that mode across a save: the replacement takes the target's
/// permissions before it is renamed over it.
#[cfg(unix)]
#[test]
fn saving_over_a_locked_down_file_keeps_its_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    let tui = TuiConfig {
        monochrome: true,
        ..TuiConfig::default()
    };
    Config::update_tui(&path, |t| *t = tui.clone()).unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "the save changed the file's mode");
    let mut left: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        ["config.toml", "config.toml.lock"],
        "a temporary file was left behind"
    );
}

/// A file that does not exist yet has no mode to carry forward, so a save to a fresh path
/// still succeeds and simply leaves the new file at whatever the umask gives it.
#[cfg(unix)]
#[test]
fn saving_a_file_that_does_not_exist_yet_succeeds() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::update_tui(&path, |t| *t = TuiConfig::default()).unwrap();
    assert!(path.exists());
    assert!(Config::parse(&std::fs::read_to_string(&path).unwrap()).is_ok());
}

/// Clearing a key is the other half of setting one: the commented line the template shows
/// it at goes back where the live key stood, so a round trip leaves the file as it was.
#[test]
fn clearing_a_key_puts_its_commented_default_back() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, Config::template()).unwrap();
    Config::set_adapter(&path, Some("hci1")).unwrap();
    Config::set_adapter(&path, None).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        Config::template(),
        "setting the adapter and clearing it again did not leave the template as it was"
    );

    // What a save that overrides no accent leaves behind, to compare against.
    let plain = dir.join("plain.toml");
    std::fs::write(&plain, Config::template()).unwrap();
    Config::update_tui(&plain, |t| *t = TuiConfig::default()).unwrap();

    std::fs::write(&path, Config::template()).unwrap();
    let overridden = TuiConfig {
        accent: Some("bright-cyan".into()),
        accents: FamilyAccents {
            dac: Some("red".into()),
            ..FamilyAccents::default()
        },
        spectrum: vec!["#000000".into(), "#ffffff".into()],
        ..TuiConfig::default()
    };
    Config::update_tui(&path, |t| *t = overridden.clone()).unwrap();
    let set = std::fs::read_to_string(&path).unwrap();
    assert!(
        set.contains("accent = \"bright-cyan\"") && set.contains("dac = \"red\""),
        "{set}"
    );
    assert!(set.contains("stops = [\"#000000\", \"#ffffff\"]"), "{set}");
    assert_eq!(Config::parse(&set).unwrap().tui, overridden);

    Config::update_tui(&path, |t| *t = TuiConfig::default()).unwrap();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        std::fs::read_to_string(&plain).unwrap(),
        "clearing the accents and the stops did not put their commented defaults back"
    );
}

/// A key a table of the same name has taken is not one a value can be written over, so
/// nothing is written — and the commented line that would have made room for it is not
/// spent either.
#[test]
fn a_key_a_table_of_the_same_name_holds_is_left_alone() {
    let text =
        "schema = 1\n\n[tui]\n# about palette\n# palette = \"amber\"\n\n[tui.palette]\nnope = 1\n";
    let mut doc = text.parse::<DocumentMut>().unwrap();
    set_key_at(&mut doc, &["tui"], "palette", Value::from("neon")).unwrap();
    assert_eq!(doc.to_string(), text);
}

/// A config can carry a device passphrase, so every writer keeps the mode a user who has
/// locked it down with `chmod 600` gave the file.
#[cfg(unix)]
#[test]
fn every_writer_keeps_the_files_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, Config::template()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

    Config::update_tui(&path, |t| *t = TuiConfig::default()).unwrap();
    Config::set_adapter(&path, Some("hci1")).unwrap();
    Config::add_device(&path, &device("dac", "1112131415161718")).unwrap();
    Config::update_device(&path, "dac", &device("dac", "0102030405060708")).unwrap();
    Config::remove_device(&path, "dac").unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "a writer changed the file's mode");
    let mut left: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        ["config.toml", "config.toml.lock"],
        "a temporary file was left behind"
    );
}

/// A config written where none existed is private whatever the umask says.
#[cfg(unix)]
#[test]
fn a_new_config_is_created_at_0600() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::set_adapter(&path, Some("hci1")).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    let init = dir.join("init.toml");
    Config::create_from_template(&init).unwrap();
    let mode = std::fs::metadata(&init).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

/// A directory the write has to create is private too.
#[cfg(unix)]
#[test]
fn a_directory_the_write_creates_is_0700() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("config-writer");
    let path = dir.join("a/b/config.toml");
    Config::set_adapter(&path, Some("hci1")).unwrap();
    for made in [dir.join("a"), dir.join("a/b")] {
        let mode = std::fs::metadata(&made).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "{}", made.display());
    }
}

/// Two writers saving to one path leave one of the two texts whole, and no temporary file.
#[cfg(unix)]
#[test]
fn concurrent_saves_leave_one_whole_text_and_no_temporary_file() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let texts = ["a = 1\n".repeat(4000), "b = 2\n".repeat(5000)];
    std::thread::scope(|scope| {
        for text in &texts {
            let path = &path;
            scope.spawn(move || {
                for _ in 0..50 {
                    write_atomically(path, text).unwrap();
                }
            });
        }
    });
    let left = std::fs::read_to_string(&path).unwrap();
    assert!(texts.contains(&left), "the file is neither text whole");
    let names: Vec<String> = std::fs::read_dir(&*dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["config.toml"], "a temporary file was left behind");
}

/// A save that cannot create its temporary file reports the config path, not a temporary one.
#[cfg(unix)]
#[test]
fn a_failed_save_names_the_config_path() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = temp_dir("config-writer");
    std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    let path = dir.join("config.toml");
    let err = Config::set_adapter(&path, Some("hci1")).unwrap_err();
    std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    match err {
        ConfigError::Io(named, _) => assert_eq!(named, path),
        other => panic!("got {other:?}"),
    }
    assert!(std::fs::read_dir(&*dir).unwrap().next().is_none());
}

/// Have the writers of `path` meet after reading it.
fn meet_at(path: &Path) {
    MEETINGS.lock().unwrap().push((
        path.to_path_buf(),
        std::sync::Arc::new((std::sync::Mutex::new(0), std::sync::Condvar::new())),
    ));
}

/// Two writers adding different presets to one file at once, meeting after each has read it:
/// the lock lets them through one at a time, so both presets are in the file.
#[test]
fn two_writers_adding_presets_at_once_both_keep_theirs() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, Config::template()).unwrap();
    meet_at(&path);
    let mut second = hd650_warm();
    second.name = "second".into();
    std::thread::scope(|scope| {
        for preset in [hd650_warm(), second] {
            let path = &path;
            scope.spawn(move || Config::add_preset(path, &preset).expect("a preset added"));
        }
    });
    let config = Config::load_from(&path).unwrap();
    assert!(config.preset("HD650 warm").is_some(), "the first was lost");
    assert!(config.preset("second").is_some(), "the second was lost");
}

/// A preset added and a `[tui]` change made at once both land.
#[test]
fn a_preset_and_a_settings_change_at_once_both_land() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, Config::template()).unwrap();
    meet_at(&path);
    std::thread::scope(|scope| {
        let path = &path;
        scope.spawn(move || Config::add_preset(path, &hd650_warm()).expect("a preset added"));
        scope.spawn(move || {
            Config::update_tui(path, |tui| tui.monochrome = true).expect("a settings change")
        });
    });
    let config = Config::load_from(&path).unwrap();
    assert!(config.preset("HD650 warm").is_some(), "the preset was lost");
    assert!(config.tui.monochrome, "the settings change was lost");
}

/// Two writers adding the same new name at once: one adds it and the other, reading the file
/// after it under the lock, is refused, so the file holds one preset of that name.
#[test]
fn two_writers_adding_the_same_name_at_once_leave_one_preset_and_one_refusal() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, Config::template()).unwrap();
    meet_at(&path);
    let results: Vec<Result<(), ConfigError>> = std::thread::scope(|scope| {
        let writers: Vec<_> = (0..2)
            .map(|_| {
                let path = &path;
                scope.spawn(move || Config::add_preset(path, &hd650_warm()))
            })
            .collect();
        writers
            .into_iter()
            .map(|writer| writer.join().expect("a writer"))
            .collect()
    });
    assert_eq!(
        results.iter().filter(|r| r.is_ok()).count(),
        1,
        "{results:?}"
    );
    assert!(
        results
            .iter()
            .any(|r| matches!(r, Err(ConfigError::Parse(m)) if m.contains("already exists"))),
        "{results:?}"
    );
    let presets = Config::load_from(&path).unwrap().presets;
    assert_eq!(presets.iter().filter(|p| p.name == "HD650 warm").count(), 1);
}

/// A writer that waits more than two seconds for another svolder holding the file's lock gives
/// up saying so, and writes nothing.
#[test]
fn a_writer_that_waits_out_the_lock_says_another_svolder_is_writing() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    let held = flock::lock_beside(&path, flock::Wait::Not).unwrap();
    let started = std::time::Instant::now();
    let err = Config::set_adapter(&path, Some("hci1")).unwrap_err();
    assert!(started.elapsed() >= std::time::Duration::from_secs(2));
    assert_eq!(
        err.to_string(),
        format!(
            "config io {}: another svolder is writing this file",
            path.display()
        )
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "schema = 1\n");
    drop(held);
    Config::set_adapter(&path, Some("hci1")).unwrap();
}

/// A configuration made read-only is read-only on purpose: every writer refuses it, saying so,
/// and leaves it as it was.
#[cfg(unix)]
#[test]
fn a_read_only_config_is_refused_and_left_as_it_was() {
    use std::os::unix::fs::PermissionsExt;
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    std::fs::write(&path, "schema = 1\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    let err = Config::set_adapter(&path, Some("hci1")).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!("config {} is read-only", path.display())
    );
    let err = Config::add_preset(&path, &hd650_warm()).unwrap_err();
    assert!(matches!(err, ConfigError::ReadOnly(_)), "{err:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "schema = 1\n");
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o444);
}

/// The daemon asks whether it may write the file before it offers to save a preset: a file made
/// read-only, and one in a directory it cannot write, are refused in words that say which.
#[cfg(unix)]
#[test]
fn writable_names_why_a_config_cannot_be_written() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: geteuid reads no memory of ours.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    assert_eq!(
        Config::writable(&path),
        Ok(()),
        "a missing file in a writable directory"
    );
    std::fs::write(&path, "schema = 1\n").unwrap();
    assert_eq!(Config::writable(&path), Ok(()));
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(
        Config::writable(&path),
        Err(format!("{} is read-only (0444)", path.display()))
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o500)).unwrap();
    let words = Config::writable(&path);
    std::fs::set_permissions(&*dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        words,
        Err(format!("{}: its directory is not writable", path.display()))
    );
}
