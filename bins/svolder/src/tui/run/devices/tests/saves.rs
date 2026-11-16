// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What saving a device or a preset says, what the reopened app shows, and the writes a daemon
//! takes as ops.

use super::*;

/// Saving the editor says, right then, that the passphrase it wrote does not match the code the
/// device advertises, when the daemon holding the device says so of the entry it took in, and
/// says nothing when it does not: the editor has no advertisement to judge a code by as it is
/// typed, so the note comes with the save.
#[tokio::test]
async fn saving_a_passphrase_the_daemon_judges_a_mismatch_says_so_at_once() {
    use crate::test_support::stub_daemon::{CANONICAL, OTHER};
    use svolder_ipc::PassphraseNote;

    let daemon = stub_daemon().await;
    let path = config_naming(&[CANONICAL, OTHER]);
    daemon.applies_device_ops_to(&path);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, Some(CANONICAL));
    let _ = relisted(&path, &mut watching.app);

    let edited = DeviceConfig {
        passphrase: "9999".into(),
        ..device_config(CANONICAL, 0)
    };
    daemon.notes(Some(PassphraseNote::Differs));
    let mut watching = carried_on(
        taken(
            Intent::UpdateDevice(CANONICAL.into(), edited.clone()),
            &opener,
            watching,
        )
        .await,
    );
    assert_eq!(
        toast_of(&watching).as_deref(),
        Some("fake: the passphrase does not match the code the device advertises")
    );

    // A save the daemon has no note for says nothing.
    daemon.notes(None);
    watching.app.dismiss_toast();
    let watching = carried_on(
        taken(
            Intent::UpdateDevice(CANONICAL.into(), edited),
            &opener,
            watching,
        )
        .await,
    );
    assert_eq!(toast_of(&watching), None);

    // The note is the one of the device the editor saved, not of the one being watched: the
    // daemon has one for the watched device and none for the other, which is the one saved.
    daemon.notes(Some(PassphraseNote::Differs));
    let other = DeviceConfig {
        passphrase: "9999".into(),
        ..device_config(OTHER, 1)
    };
    let watching =
        carried_on(taken(Intent::UpdateDevice(OTHER.into(), other), &opener, watching).await);
    assert_eq!(toast_of(&watching), None);
    watching.close().await;
}

/// A direct session judges the entry just saved against the advertisement it holds, for the
/// device it is on and no other: `auto` earns nothing, and a code the device's own does not
/// match is said.
#[tokio::test]
async fn saving_a_passphrase_a_direct_session_judges_a_mismatch_says_so_at_once() {
    let (mut backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
    backend.configured_as("fake");
    let app = app_for(&backend);
    let path = config_naming(&["fake", "other"]);
    let opener = opener_on(path.clone(), never_opens());
    let mut watching = watching(backend, app, Some("fake"));
    let _ = relisted(&path, &mut watching.app);

    let says = |name: &str, passphrase: &str| {
        let entry = DeviceConfig {
            passphrase: passphrase.into(),
            // "fake" keeps unit ...00, the watched device's; any other name gets ...01, since a
            // file holds a unit once.
            ..device_config(name, u8::from(name != "fake"))
        };
        Intent::UpdateDevice(name.into(), entry)
    };
    // The fake device's own code is 1234.
    for (name, passphrase, said) in [
        ("fake", "9999", true),
        ("fake", "1234", false),
        ("fake", "auto", false),
        ("other", "9999", false),
    ] {
        watching.app.dismiss_toast();
        watching = carried_on(taken(says(name, passphrase), &opener, watching).await);
        assert_eq!(
            toast_of(&watching).is_some(),
            said,
            "{name} {passphrase}: {:?}",
            toast_of(&watching)
        );
        // The Status pane's copy follows the session that took the entry in, for the device the
        // session is on.
        if name == "fake" {
            assert_eq!(
                watching.app.session.passphrase_note.is_some(),
                said,
                "{name} {passphrase}: the Status pane's note is stale"
            );
        }
    }
    watching.close().await;
}

/// An app the opener builds names the file the opener reads and the look writes, not
/// whatever the environment resolves to.
#[tokio::test(start_paused = true)]
async fn a_reopened_app_shows_the_file_its_opener_reads() {
    let (_backend, gate, _app) = fake(SHORT_DEADLINE).await;
    let path = scratch_config();
    let opener = opener_on(
        path.clone(),
        fake_open(Log::default(), Arc::clone(&gate), &[]),
    );
    let watching = opener
        .reopen(Some("gungnir"), None)
        .await
        .expect("the device opens");
    assert_eq!(watching.app.config_path, path);
    watching.close().await;
}

/// The landing app names the opener's file as well.
#[tokio::test(start_paused = true)]
async fn a_landing_app_shows_the_file_its_opener_reads() {
    let (_backend, gate, _app) = fake(SHORT_DEADLINE).await;
    let path = scratch_config();
    let opener = opener_on(path.clone(), fake_open(Log::default(), gate, &[]));
    let watching = land(&opener, Landing::Devices { why: None }).await;
    assert_eq!(watching.app.config_path, path);
    watching.close().await;
}

/// A reopen over a daemon that keeps readings starts the graphs with them: the volume's series
/// holds the daemon's samples before what this session reads itself.
#[tokio::test]
async fn a_reopen_over_a_daemon_seeds_the_graphs_from_its_readings() {
    let daemon = stub_daemon().await;
    let volume: svolder_proto::FunctionId = "10".parse().expect("the volume's id");
    daemon.readings(svolder_ipc::Response::Readings {
        device: "fake".into(),
        now_ms: 100_000,
        bin_ms: 1000,
        series: vec![svolder_ipc::SeriesOut {
            id: "10".into(),
            key: Some("volume".into()),
            aggregate: svolder_profiles::history::Aggregate::Max,
            points: vec![(40_000, 40.0), (90_000, 50.0)],
        }],
        recalls: Vec::new(),
        link: Vec::new(),
        signal: Vec::new(),
    });
    let (spare, gate, _) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let watching = opener.reopen(None, None).await.expect("the daemon opens");
    let series = watching
        .app
        .readings
        .histories
        .reading(volume)
        .expect("the volume's series");
    let values: Vec<f64> = series.samples().map(|(_, value)| value).collect();
    assert_eq!(values[..2], [40.0, 50.0], "{values:?}");
    assert_eq!(
        series.aggregate(),
        svolder_profiles::history::Aggregate::Max
    );
    let (first, _) = series.samples().next().expect("a sample");
    assert!(
        first.elapsed() >= Duration::from_secs(59),
        "the first sample is a minute old: {:?}",
        first.elapsed()
    );
    watching.close().await;
}

/// A same-device reopen keeps the histories it carries as they are: the daemon's readings, which
/// the carried ones already cover, are not laid before them.
#[tokio::test]
async fn a_reopen_that_carries_the_histories_does_not_seed_them_again() {
    let daemon = stub_daemon().await;
    let volume: svolder_proto::FunctionId = "10".parse().expect("the volume's id");
    daemon.readings(svolder_ipc::Response::Readings {
        device: "fake".into(),
        now_ms: 100_000,
        bin_ms: 1000,
        series: vec![svolder_ipc::SeriesOut {
            id: "10".into(),
            key: Some("volume".into()),
            aggregate: svolder_profiles::history::Aggregate::Max,
            points: vec![(40_000, 40.0)],
        }],
        recalls: Vec::new(),
        link: Vec::new(),
        signal: Vec::new(),
    });
    let (spare, gate, mut old) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    old.session.unit = Some(OPENED_UNIT.into());
    let profile = old.profile.clone();
    old.readings
        .histories
        .record(&profile, volume, Instant::now(), 7.0);
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let watching = opener
        .reopen(None, Some(&old))
        .await
        .expect("the daemon opens");
    let series = watching
        .app
        .readings
        .histories
        .reading(volume)
        .expect("the volume's series");
    assert!(
        series.samples().all(|(_, value)| value != 40.0),
        "the carried series was seeded again"
    );
    watching.close().await;
}

/// A daemon's memory names the preset at connect, and a recall another front end makes while the
/// session runs replaces it in the header.
#[tokio::test]
async fn the_header_follows_the_daemons_recall_memory() {
    let daemon = stub_daemon().await;
    daemon.recalled("Warm");
    let (spare, gate, _) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let mut watching = opener.reopen(None, None).await.expect("the daemon opens");
    assert_eq!(watching.app.presets.loaded_preset.as_deref(), Some("Warm"));
    daemon.push_recalled("Bass");
    let backend = watching.backend.as_mut().expect("a backend");
    let update = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match backend.next_status().await {
                Ok(Some(Update::Recalled(name))) => return name,
                Ok(Some(_)) => continue,
                other => panic!("the feed gave {other:?}"),
            }
        }
    })
    .await
    .expect("a recalled update");
    watching.app.apply(Update::Recalled(update), Instant::now());
    assert_eq!(watching.app.presets.loaded_preset.as_deref(), Some("Bass"));
    watching.close().await;
}

/// While a daemon runs the editor's save, a removal and a default go to it as `device_config`
/// ops, never into the file by this side; clearing the default is its own op.
#[tokio::test]
async fn device_writes_go_to_the_daemon_as_ops_while_one_runs() {
    use svolder_ipc::DeviceConfigOp;

    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir", "bifrost"]);
    // No file is named for the daemon: an op it takes changes nothing here.
    daemon.lists_features(&["device_crud"]);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, Some("bifrost"));
    let _ = relisted(&path, &mut watching.app);
    let before = std::fs::read_to_string(&path).unwrap();

    let edited = DeviceConfig {
        name: "dac".into(),
        passphrase: "1234".into(),
        ..device_config("gungnir", 0)
    };
    for intent in [
        Intent::UpdateDevice("gungnir".into(), edited),
        Intent::DefaultDevice(Some("bifrost".into())),
        Intent::DefaultDevice(None),
        Intent::RemoveDevice("bifrost".into()),
    ] {
        watching = carried_on(taken(intent, &opener, watching).await);
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        before,
        "the file was written behind the daemon"
    );
    let ops = daemon.device_ops();
    assert_eq!(ops.len(), 4, "{ops:?}");
    assert!(matches!(&ops[0], DeviceConfigOp::Update { device, .. } if device == "gungnir"));
    assert!(matches!(&ops[1], DeviceConfigOp::SetDefault { device } if device == "bifrost"));
    assert_eq!(ops[2], DeviceConfigOp::ClearDefault);
    assert!(matches!(&ops[3], DeviceConfigOp::Remove { device } if device == "bifrost"));
    watching.close().await;
}

/// A refusal is worded from its typed reason and says nothing of a passphrase; a daemon that
/// has no `device_crud` is told to restart and nothing is written behind it.
#[tokio::test]
async fn a_refused_device_write_is_worded_and_an_old_daemon_is_never_written_around() {
    use svolder_ipc::{CODE_USAGE, Reason};

    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir"]);
    let before = std::fs::read_to_string(&path).unwrap();
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, Some("gungnir"));
    let _ = relisted(&path, &mut watching.app);

    // A daemon that lists no device_crud.
    let remove = Intent::RemoveDevice("gungnir".into());
    watching = carried_on(taken(remove.clone(), &opener, watching).await);
    assert!(
        toast_of(&watching).is_some_and(|text| text.contains("cannot edit the devices")),
        "{:?}",
        toast_of(&watching)
    );
    assert_eq!(
        watching.app.notices.advice.as_deref(),
        Some("Run `svolder daemon restart`")
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);

    for (reason, words) in [
        (Reason::Taken, "another device already has that name"),
        (Reason::Missing, "no longer configured"),
        (Reason::FileBusy, "busy with another writer"),
        (Reason::Stopping, "the daemon is stopping"),
    ] {
        daemon.refuses_device_ops(Response::error(CODE_USAGE, "x").with_reason(reason));
        watching.app.dismiss_toast();
        watching = carried_on(taken(remove.clone(), &opener, watching).await);
        assert!(
            toast_of(&watching).is_some_and(|text| text.contains(words)),
            "{reason:?}: {:?}",
            toast_of(&watching)
        );
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    watching.close().await;
}

/// A save sends only what the person changed: a code another front end wrote since this session
/// read the file is not sent back with a rename, and the file keeps it.
#[tokio::test]
async fn a_rename_sends_the_name_alone_and_leaves_a_newer_passphrase_be() {
    use svolder_ipc::DeviceConfigOp;

    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir", "bifrost"]);
    daemon.applies_device_ops_to(&path);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, Some("bifrost"));
    let _ = relisted(&path, &mut watching.app);

    // Another front end changes the code after this session read the file.
    let mut newer = device_config("gungnir", 0);
    newer.passphrase = "4821".into();
    Config::update_device(&path, "gungnir", &newer).expect("the other front end writes");

    let renamed = DeviceConfig {
        name: "dac".into(),
        ..watching.app.devices.configured[0].clone()
    };
    let watching = carried_on(
        taken(
            Intent::UpdateDevice("gungnir".into(), renamed),
            &opener,
            watching,
        )
        .await,
    );
    assert_eq!(
        daemon.device_ops(),
        [DeviceConfigOp::Update {
            device: "gungnir".into(),
            name: Some("dac".into()),
            profile: None,
            passphrase: None,
        }]
    );
    let written = Config::load_from(&path).unwrap();
    assert_eq!(written.devices[0].name, "dac");
    assert_eq!(
        written.devices[0].passphrase, "4821",
        "the newer code was written back"
    );
    watching.close().await;
}

/// A device whose code the daemon holds opens the editor on a code with no digits, and saving
/// without touching that row sends no passphrase; typing one does.
#[tokio::test]
async fn an_untouched_code_the_daemon_holds_is_not_sent_back() {
    use crate::tui::boxes::editor;
    use crossterm::event::KeyCode;

    let daemon = stub_daemon().await;
    let backend = Backend::stub_remote(&daemon.socket).await;
    let mut app = app_for(&backend);
    app.devices.configured = vec![DeviceConfig {
        passphrase: "set".into(),
        ..device_config("gungnir", 0)
    }];
    app.devices.rows = vec![crate::backend::DeviceRow {
        name: "gungnir".into(),
        ..Default::default()
    }];
    editor::open(&mut app);
    let intents = editor::key(&mut app, KeyCode::Enter);
    let [Intent::UpdateDevice(_, device)] = intents.as_slice() else {
        panic!("the save did not ask for one update: {intents:?}");
    };
    assert_eq!(device.passphrase, "set", "an untouched code was replaced");
    backend.close().await;
}

/// The daemon's view replaces what the file said: a device only the daemon has is listed and
/// can be edited, one it does not have is gone, and the default follows its flag.
#[tokio::test]
async fn the_daemons_view_is_the_list_and_the_editors_records() {
    use svolder_ipc::{ConfiguredDevice, PassphraseKind};

    let (backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
    let mut app = app_for(&backend);
    app.devices.configured = vec![device_config("only-in-my-file", 0)];
    app.devices.rows = vec![crate::backend::DeviceRow {
        name: "only-in-my-file".into(),
        ..Default::default()
    }];
    let view = |name: &str, unit: &str, default: bool| ConfiguredDevice {
        unit: unit.into(),
        name: name.into(),
        profile: Some("bifrost3".into()),
        passphrase: PassphraseKind::Set,
        default,
        connected: true,
        not_connected_since_ms: None,
    };
    apply_view(
        &mut app,
        &[
            view("desk", "0A0B0C0D0E0F1011", false),
            view("amp", "0A0B0C0D0E0F1012", true),
        ],
    );
    let names: Vec<_> = app
        .devices
        .rows
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(names, ["desk", "amp"]);
    assert_eq!(app.devices.default_device.as_deref(), Some("amp"));
    assert_eq!(app.devices.configured[0].passphrase, "set");
    assert_eq!(
        app.devices.configured[0].profile.as_deref(),
        Some("bifrost3")
    );
    backend.close().await;
}

/// The daemon's list is asked in a task and taken once it has answered: the call that starts the
/// ask returns at once, the next one that finds it answered applies it, and a daemon that cannot
/// list is not asked again.
#[tokio::test]
async fn the_daemons_list_is_asked_in_a_task_and_taken_when_it_answers() {
    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir", "bifrost"]);
    daemon.applies_device_ops_to(&path);
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, Some("bifrost"));

    super::take_view(&mut watching);
    assert!(watching.view_ask.is_some(), "the ask was not started");
    assert!(
        watching.app.devices.rows.is_empty(),
        "the list was taken before an answer"
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        while watching
            .view_ask
            .as_ref()
            .is_some_and(|ask| !ask.is_finished())
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the ask answers");
    super::take_view(&mut watching);
    let names: Vec<_> = watching
        .app
        .devices
        .rows
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(names, ["gungnir", "bifrost"]);

    // A daemon without `device_crud` answers once, and is not asked again.
    daemon.lists_features(&[]);
    super::take_view(&mut watching);
    tokio::time::timeout(Duration::from_secs(10), async {
        while watching
            .view_ask
            .as_ref()
            .is_some_and(|ask| !ask.is_finished())
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the ask answers");
    super::take_view(&mut watching);
    assert!(watching.view_off);
    assert!(watching.view_ask.is_none());
    watching.close().await;
}

/// A save is measured against the record the editor opened on, not against the list as the
/// daemon's view refreshed it since: a code another front end set while the editor was open is
/// not sent back as the old `auto`.
#[tokio::test]
async fn a_save_is_measured_against_the_record_the_editor_opened_on() {
    use crate::tui::boxes::editor;
    use crossterm::event::KeyCode;
    use svolder_ipc::{ConfiguredDevice, DeviceConfigOp, PassphraseKind};

    let daemon = stub_daemon().await;
    daemon.lists_features(&["device_crud"]);
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, Some("gungnir"));
    let _ = relisted(&path, &mut watching.app);
    watching.app.set_selected(0);
    editor::open(&mut watching.app);
    // Another front end sets a code while the editor is open; the list is refreshed.
    super::apply_view(
        &mut watching.app,
        &[ConfiguredDevice {
            unit: "0102030405060700".into(),
            name: "gungnir".into(),
            profile: None,
            passphrase: PassphraseKind::Set,
            default: false,
            connected: true,
            not_connected_since_ms: None,
        }],
    );
    let intents = editor::key(&mut watching.app, KeyCode::Enter);
    let [Intent::UpdateDevice(name, device)] = intents.as_slice() else {
        panic!("the save did not ask for one update: {intents:?}");
    };
    let renamed = DeviceConfig {
        name: "dac".into(),
        ..device.clone()
    };
    let watching = carried_on(
        taken(
            Intent::UpdateDevice(name.clone(), renamed),
            &opener,
            watching,
        )
        .await,
    );
    let ops = daemon.device_ops();
    assert!(
        matches!(
            &ops[..],
            [DeviceConfigOp::Update {
                passphrase: None,
                ..
            }]
        ),
        "{ops:?}"
    );
    watching.close().await;
}

/// One failed ask (the file busy, a daemon restarting) is tried again at the next relist: only a
/// daemon without `device_crud` turns the list off.
#[tokio::test]
async fn a_failed_list_ask_is_tried_again_at_the_next_relist() {
    use svolder_ipc::{CODE_USAGE, Reason, Response};

    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir"]);
    daemon.applies_device_ops_to(&path);
    daemon.refuses_device_ops(Response::error(CODE_USAGE, "busy").with_reason(Reason::FileBusy));
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, Some("gungnir"));

    super::take_view(&mut watching);
    tokio::time::timeout(Duration::from_secs(10), async {
        while watching
            .view_ask
            .as_ref()
            .is_some_and(|ask| !ask.is_finished())
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the ask answers");
    super::take_view(&mut watching);
    assert!(!watching.view_off, "one failed ask turned the list off");
    assert!(watching.view_ask.is_some(), "no new ask was started");
    watching.close().await;
}

/// A daemon that reads another configuration file is not written to: each of the preset list's
/// writes is refused in the words the other writes use, nothing is queued and nothing sent.
#[tokio::test]
async fn a_daemon_reading_another_file_is_sent_no_preset_write() {
    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir"]);
    daemon.applies_preset_ops_to(&path);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, None);
    watching.app.overlays.conn.elsewhere = Some("/other/config.toml".into());
    for intent in [
        Intent::RenamePreset("a".into(), "b".into()),
        Intent::RemovePreset("a".into()),
    ] {
        watching = carried_on(taken(intent, &opener, watching).await);
        assert_eq!(
            toast_of(&watching).as_deref(),
            Some("read-only here: the daemon reads /other/config.toml")
        );
        assert!(
            watching.jobs.ended.try_recv().is_err(),
            "a write was queued"
        );
    }
    assert!(daemon.preset_ops().is_empty());
    watching.close().await;
}

/// `u` on a preset another front end deleted says it is gone, with a daemon and without one, and
/// writes nothing.
#[tokio::test(start_paused = true)]
async fn an_update_of_a_preset_that_is_gone_is_refused_with_and_without_a_daemon() {
    use svolder_profiles::EqMode;
    use svolder_profiles::tone::preset::Preset;

    let curve = Preset {
        name: "gone".into(),
        source: None,
        bands: 5,
        mode: EqMode::Eq,
        loudness: 0,
        band: vec![
            svolder_profiles::tone::preset::PresetBand {
                f: 1000.0,
                g: 1.0,
                q: 1.0
            };
            5
        ],
    };
    // With a daemon: the stub applies the op as the daemon's writers do.
    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir"]);
    daemon.applies_preset_ops_to(&path);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let held = watching(backend, app, None);
    let held = written_through(
        Intent::UpdatePreset("gone".into(), curve.clone()),
        &opener,
        held,
    )
    .await;
    let said = toast_of(&held).expect("a refusal");
    assert!(said.contains("gone"), "{said}");
    assert!(Config::load_from(&path).unwrap().presets.is_empty());
    held.close().await;
    // Without one.
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let watching = written_through(
        Intent::UpdatePreset("gone".into(), curve),
        &opener,
        watching(backend, app, None),
    )
    .await;
    let said = toast_of(&watching).expect("a refusal");
    assert!(said.contains("gone"), "{said}");
    assert!(Config::load_from(&path).unwrap().presets.is_empty());
    watching.close().await;
}

/// A walk opened over the session shows the key line's tooltips while the TUI's `tooltips`
/// setting and mouse capture are on, and none with the setting off.
#[tokio::test(start_paused = true)]
async fn the_walk_over_the_session_shows_the_key_line_tooltips_by_the_setting() {
    for (tips, shown) in [(true, true), (false, false)] {
        let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
        let path = config_naming(&[]);
        let opener = opener_on(path.clone(), never_opens());
        let mut watching = watching(backend, app, None);
        watching.app.mouse = true;
        watching.app.tui.tooltips = tips;
        watching.app.show(Screen::Devices);
        let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
        let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
        let rest = Event::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Moved,
            column: 68,
            row: SIZE.1 - 1,
            modifiers: crossterm::event::KeyModifiers::NONE,
        });
        // The pointer rests on `q` for longer than the delay, then `q` leaves the walk.
        let mut events: BoxStream<'static, io::Result<Event>> = Box::pin(
            stream::iter(vec![
                (Duration::from_secs(1), Ok::<Event, io::Error>(rest)),
                (Duration::from_millis(1500), key('q')),
            ])
            .then(|(wait, event)| async move {
                tokio::time::sleep(wait).await;
                event
            }),
        );
        let added = add_device(
            &adds(path.clone()),
            &opener,
            &mut watching,
            &mut terminal,
            &mut events,
            &mut look(),
        )
        .await;
        assert!(matches!(added, Added::Carried), "the walk ended the run");
        assert_eq!(
            drawn(&frames)
                .iter()
                .any(|frame| frame.contains("quit · q")),
            shown,
            "tooltips {tips}"
        );
        watching.close().await;
    }
}
