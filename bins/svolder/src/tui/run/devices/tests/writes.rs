// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device list and the preset lists as the daemon or the file writes them: asking again, edits,
//! queued writes and the notices a refused write gives.

use super::*;

/// Only a daemon's rows are asked for again, and only while the list shows them: a direct
/// session's rows are the file's, which say nothing about how a device is doing.
#[tokio::test]
async fn only_a_daemon_s_list_on_screen_is_asked_again() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.screen = Screen::Devices;
    let direct = watching(backend, app, None);
    assert!(
        !relisting(&direct),
        "a direct session's list was asked again"
    );
    direct.close().await;

    let daemon = stub_daemon().await;
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut remote = watching(backend, app, None);
    assert!(!relisting(&remote), "the list was asked for off its screen");
    remote.app.screen = Screen::Devices;
    assert!(
        relisting(&remote),
        "the daemon's list on screen was not asked again"
    );
    remote.close().await;
}
/// Asking the daemon again takes what it says about every device it lists, the wait before
/// a next attempt included, and leaves a device it does not list as the list had it.
#[tokio::test]
async fn asking_the_daemon_again_takes_its_rows_and_keeps_the_ones_it_does_not_list() {
    use crate::test_support::stub_daemon::{CANONICAL, OTHER};

    let daemon = stub_daemon().await;
    *daemon.other_retry().lock().expect("the wait") = Some(30);
    let backend = Backend::stub_remote(&daemon.socket).await;
    let mut app = app_for(&backend);
    app.devices.rows = vec![
        DeviceRow::for_test(CANONICAL),
        DeviceRow::for_test(OTHER),
        DeviceRow::for_test("unheld"),
    ];
    let mut watching = watching(backend, app, None);

    relist_held(&mut watching).await;

    let rows = &watching.app.devices.rows;
    assert_eq!(rows[0].connected, Some(true));
    assert_eq!(rows[0].product.as_deref(), Some("Gungnir 2"));
    assert_eq!(
        (rows[1].connected, rows[1].retry_in_secs),
        (Some(false), Some(30))
    );
    assert_eq!(
        rows[2],
        DeviceRow::for_test("unheld"),
        "an unlisted row changed"
    );
    let facts = watching
        .app
        .session
        .facts
        .as_ref()
        .expect("the open device's facts");
    assert_eq!(
        (facts.name.as_str(), facts.product.as_deref()),
        (CANONICAL, Some("Gungnir 2")),
        "the Status pane's facts did not follow the same answer"
    );
    watching.close().await;
}
/// An edited device is the daemon's write (an op that rewrites the file it reads), and the list
/// and the editor's records are then the daemon's view of the devices, which never carries a
/// passphrase but its kind.
#[tokio::test]
async fn an_edited_device_is_written_by_the_daemon_and_the_list_is_its_view() {
    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir", "bifrost"]);
    daemon.applies_device_ops_to(&path);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let mut app = app_for(&backend);
    app.session.device_name = "bifrost".into();
    let mut watching = watching(backend, app, Some("bifrost"));
    let _ = relisted(&path, &mut watching.app);

    let edited = DeviceConfig {
        name: "dac".into(),
        passphrase: "1234".into(),
        ..device_config("gungnir", 0)
    };
    let watching = carried_on(
        taken(
            Intent::UpdateDevice("gungnir".into(), edited),
            &opener,
            watching,
        )
        .await,
    );

    assert_eq!(
        named(&path),
        ["dac", "bifrost"],
        "the file was not rewritten"
    );
    assert_eq!(
        watching.app.devices.configured[0].passphrase, "set",
        "the editor's records are not the daemon's view (a code is only its kind)"
    );
    assert_eq!(
        watching
            .app
            .devices
            .rows
            .iter()
            .map(|row| row.name.clone())
            .collect::<Vec<_>>(),
        ["dac", "bifrost"],
        "the list is not the daemon's view"
    );
    assert_eq!(daemon.reloads(), 1, "the daemon was not told to reload");
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.clone()),
        None,
        "{:?}",
        watching.app.toast().map(|toast| toast.text.clone())
    );
    watching.close().await;
}
/// While a daemon runs the preset list's writes are the daemon's: each is a `preset_write` op,
/// the daemon writes the file and reloads, and the list is drawn from what is on disk. Each
/// intent only queues the write; the loop is not held for it.
#[tokio::test]
async fn the_preset_lists_writes_are_the_daemons_and_the_list_follows() {
    use svolder_profiles::EqMode;
    use svolder_profiles::tone::preset::{Preset, PresetBand};

    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir"]);
    daemon.applies_preset_ops_to(&path);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, None);
    let _ = relisted(&path, &mut watching.app);
    assert!(watching.app.presets.saved.is_empty());
    let curve = |name: &str, g: f64| Preset {
        name: name.into(),
        source: None,
        bands: 5,
        mode: EqMode::Eq,
        loudness: 0,
        band: vec![
            PresetBand {
                f: 1000.0,
                g,
                q: 1.0
            };
            5
        ],
    };
    let on_disk = |path: &Path| -> Vec<(String, f64)> {
        Config::load_from(path)
            .expect("the config loads")
            .presets
            .into_iter()
            .map(|preset| (preset.name, preset.band[0].g))
            .collect()
    };

    let steps = [
        (
            Intent::SavePreset(curve("custom 1", 2.0)),
            vec![("custom 1".to_string(), 2.0)],
        ),
        (
            Intent::UpdatePreset("custom 1".into(), curve("custom 1", -1.5)),
            vec![("custom 1".to_string(), -1.5)],
        ),
        (
            Intent::RenamePreset("custom 1".into(), "HD650 warm".into()),
            vec![("HD650 warm".to_string(), -1.5)],
        ),
        (Intent::RemovePreset("HD650 warm".into()), Vec::new()),
    ];
    for (reloads, (intent, wanted)) in (1..).zip(steps) {
        let what = format!("{intent:?}");
        watching = carried_on(taken(intent, &opener, watching).await);
        let done = crate::tui::run::jobs::next(&mut watching.jobs.ended).await;
        assert!(folded(done, &opener, &mut watching).is_empty());
        assert_eq!(on_disk(&path), wanted, "{what}: the file");
        let listed: Vec<(String, f64)> = watching
            .app
            .presets
            .saved
            .iter()
            .map(|preset| (preset.name.clone(), preset.band[0].g))
            .collect();
        assert_eq!(listed, wanted, "{what}: the list is not what the file says");
        assert_eq!(daemon.reloads(), reloads, "{what}: the daemon was not told");
        assert_eq!(
            watching.app.toast().map(|toast| toast.text.clone()),
            None,
            "{what}: {:?}",
            watching.app.toast().map(|toast| toast.text.clone())
        );
    }
    assert_eq!(
        daemon.preset_ops().len(),
        4,
        "every write was the daemon's: {:?}",
        daemon.preset_ops()
    );
    assert_eq!(
        named(&path),
        ["gungnir"],
        "a preset write touched the devices"
    );
    watching.close().await;
}
/// The daemon is the one writer: a name it refuses is a notice in the words it gives, which are
/// the core's, and nothing is written behind its back.
#[tokio::test]
async fn a_name_the_daemon_refuses_is_its_words_and_nothing_is_written_behind_it() {
    use svolder_ipc::Reason;
    use svolder_profiles::tone::preset;

    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir"]);
    let words = preset::new_name("evening", ["Evening"], None)
        .unwrap_err()
        .to_string();
    daemon.refuses_preset_ops(
        Response::error(svolder_ipc::CODE_USAGE, words.clone()).with_reason(Reason::Taken),
    );
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let watching = watching(backend, app, None);
    let watching = written_through(
        Intent::RenamePreset("Evening".into(), "evening".into()),
        &opener,
        watching,
    )
    .await;
    assert_eq!(toast_of(&watching).as_deref(), Some(words.as_str()));
    assert!(
        Config::load_from(&path).unwrap().presets.is_empty(),
        "the loop wrote the file itself"
    );
    assert_eq!(daemon.preset_ops().len(), 1);
    watching.close().await;
}
/// A daemon from before `preset_write` is refused in words, and the loop writes nothing behind
/// it: not the file, and not a request it would not understand.
#[tokio::test]
async fn a_daemon_without_preset_write_is_refused_in_words_and_nothing_is_written() {
    use svolder_profiles::EqMode;
    use svolder_profiles::tone::preset::Preset;

    let daemon = stub_daemon().await;
    daemon.lists_features(&["device_crud"]);
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, None);
    let preset = Preset {
        name: "evening".into(),
        source: None,
        bands: 0,
        mode: EqMode::Eq,
        loudness: 0,
        band: Vec::new(),
    };
    for intent in [
        Intent::SavePreset(preset.clone()),
        Intent::UpdatePreset("evening".into(), preset.clone()),
        Intent::RenamePreset("evening".into(), "night".into()),
        Intent::RemovePreset("evening".into()),
    ] {
        let what = format!("{intent:?}");
        watching = written_through(intent, &opener, watching).await;
        let said = toast_of(&watching).unwrap_or_default();
        assert!(said.contains("cannot write presets"), "{what}: {said}");
        assert!(
            Config::load_from(&path).unwrap().presets.is_empty(),
            "{what}"
        );
    }
    assert!(
        daemon.preset_ops().is_empty(),
        "a request went to a daemon that lacks it"
    );
    watching.close().await;
}
/// With no daemon the loop's own writers do the work, as the CLI's do, still off the loop: each
/// of the four reaches the file and the list, and a name the file refuses is a notice.
#[tokio::test(start_paused = true)]
async fn without_a_daemon_the_four_writes_are_made_in_process() {
    use svolder_profiles::EqMode;
    use svolder_profiles::tone::preset::{Preset, PresetBand};

    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let mut watching = watching(backend, app, None);
    let curve = |name: &str, g: f64| Preset {
        name: name.into(),
        source: None,
        bands: 5,
        mode: EqMode::Eq,
        loudness: 0,
        band: vec![
            PresetBand {
                f: 1000.0,
                g,
                q: 1.0
            };
            5
        ],
    };
    let listed = |watching: &Watching| -> Vec<(String, f64)> {
        watching
            .app
            .presets
            .saved
            .iter()
            .map(|p| (p.name.clone(), p.band[0].g))
            .collect()
    };
    for (intent, wanted) in [
        (Intent::SavePreset(curve("one", 2.0)), vec![("one", 2.0)]),
        (
            Intent::UpdatePreset("one".into(), curve("one", -1.0)),
            vec![("one", -1.0)],
        ),
        (
            Intent::RenamePreset("one".into(), "two".into()),
            vec![("two", -1.0)],
        ),
        (Intent::RemovePreset("two".into()), vec![]),
    ] {
        let what = format!("{intent:?}");
        watching = written_through(intent, &opener, watching).await;
        let wanted: Vec<(String, f64)> = wanted
            .into_iter()
            .map(|(n, g)| (n.to_string(), g))
            .collect();
        assert_eq!(listed(&watching), wanted, "{what}");
        assert_eq!(toast_of(&watching), None, "{what}");
    }
    watching = written_through(Intent::SavePreset(curve("a", 1.0)), &opener, watching).await;
    watching = written_through(Intent::SavePreset(curve("A", 1.0)), &opener, watching).await;
    let said = toast_of(&watching).expect("a taken name is a notice");
    assert!(said.contains('A') || said.contains('a'), "{said}");
    assert_eq!(listed(&watching).len(), 1);
    watching.close().await;
}
/// Two writes queued back to back reach the daemon in the order they were queued.
#[tokio::test]
async fn writes_queued_back_to_back_reach_the_daemon_in_order() {
    use svolder_ipc::PresetOp;

    let daemon = stub_daemon().await;
    let path = config_naming(&["gungnir"]);
    daemon.applies_preset_ops_to(&path);
    let opener = opener_on(path.clone(), never_opens());
    let backend = Backend::stub_remote(&daemon.socket).await;
    let app = app_for(&backend);
    let mut watching = watching(backend, app, None);
    let intents = [
        Intent::RemovePreset("a".into()),
        Intent::RenamePreset("b".into(), "c".into()),
        Intent::RemovePreset("c".into()),
    ];
    for intent in intents {
        watching = carried_on(taken(intent, &opener, watching).await);
    }
    for _ in 0..3 {
        let done = crate::tui::run::jobs::next(&mut watching.jobs.ended).await;
        let _ = folded(done, &opener, &mut watching);
    }
    let asked: Vec<String> = daemon
        .preset_ops()
        .into_iter()
        .map(|op| match op {
            PresetOp::Delete { name } => format!("delete {name}"),
            PresetOp::Rename { from, to } => format!("rename {from} {to}"),
            PresetOp::Save { .. } | PresetOp::Update { .. } => "save".to_string(),
        })
        .collect();
    assert_eq!(asked, ["delete a", "rename b c", "delete c"]);
    watching.close().await;
}
/// Opening the preset list reads the file again, so a preset another front end wrote since
/// is listed — and focused, being the curve the device carries — and a file that no longer
/// reads is a notice, with the list still up on what the session held.
#[tokio::test(start_paused = true)]
async fn opening_the_preset_list_reads_what_another_front_end_wrote() {
    use crossterm::event::{KeyCode, KeyEvent};
    use svolder_profiles::tone::preset;

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let _ = relisted(&path, &mut app);
    app.show(Screen::Tone);
    let eq = app.profile.eq.clone().expect("the fake has a tone control");
    let theirs = preset::from_readings(&eq, |id| app.raw_of(id), "theirs")
        .expect("the fake reports its whole curve");
    Config::add_preset(&path, &theirs).expect("another front end writes it");
    assert!(app.presets.saved.is_empty(), "the session read it already");

    let intents = app.key(KeyEvent::from(KeyCode::Char('p')));
    assert_eq!(intents, [Intent::ReadPresets]);
    let mut watching = watching(backend, app, None);
    for intent in intents {
        watching = carried_on(taken(intent, &opener, watching).await);
    }
    let names: Vec<&str> = watching
        .app
        .presets
        .saved
        .iter()
        .map(|preset| preset.name.as_str())
        .collect();
    assert_eq!(names, ["theirs"]);
    assert_eq!(
        watching
            .app
            .presets
            .preset_list
            .as_ref()
            .map(|list| list.row),
        Some(4),
        "the preset the device carries is not focused"
    );

    std::fs::write(&path, "[[preset]\n").expect("the file is broken by hand");
    watching = carried_on(taken(Intent::ReadPresets, &opener, watching).await);
    assert!(
        watching.app.toast().is_some(),
        "an unreadable file said nothing"
    );
    assert!(
        watching.app.presets.preset_list.is_some(),
        "the list closed"
    );
    assert_eq!(
        watching.app.presets.saved.len(),
        1,
        "the session's presets were dropped"
    );
    watching.close().await;
}
/// A preset write the file refuses is a notice on the key line, as a device write is, and
/// the file is left as it was.
#[tokio::test(start_paused = true)]
async fn a_preset_write_the_file_refuses_is_a_notice() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());
    let watching = written_through(
        Intent::RenamePreset("nobody".into(), "somebody".into()),
        &opener,
        watching(backend, app, None),
    )
    .await;
    let notice = watching
        .app
        .toast()
        .map(|toast| toast.text.clone())
        .expect("a notice was set");
    assert!(notice.contains("nobody"), "{notice}");
    assert!(Config::load_from(&path).unwrap().presets.is_empty());
    watching.close().await;
}
/// Reading the file again keeps the device list's cursor on a row the list still has, and
/// leaves any other screen's focused row alone: a preset written from the device screen
/// does not move the Controls pane's cursor.
#[tokio::test(start_paused = true)]
async fn reading_the_file_again_moves_only_the_device_lists_own_cursor() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    backend.close().await;
    let path = config_naming(&["gungnir"]);
    app.show(Screen::Device);
    app.set_selected(4);
    relisted(&path, &mut app).expect("the file reads");
    assert_eq!(app.selected(), 4, "the Controls pane's cursor moved");
    app.show(Screen::Devices);
    app.set_selected(4);
    relisted(&path, &mut app).expect("the file reads");
    assert_eq!(app.selected(), 0, "the list's cursor is past its one row");
}
/// A write the file refuses is a notice on the key line: nothing crashes, and nothing is
/// quietly dropped.
#[tokio::test(start_paused = true)]
async fn a_write_the_file_refuses_is_a_notice() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let opener = opener_on(path.clone(), never_opens());

    let watching = carried_on(
        taken(
            Intent::UpdateDevice("nobody".into(), device_config("nobody", 9)),
            &opener,
            watching(backend, app, None),
        )
        .await,
    );

    let notice = watching
        .app
        .toast()
        .map(|toast| toast.text.clone())
        .expect("a notice was set");
    assert!(notice.contains("nobody"), "{notice}");
    assert_eq!(named(&path), ["gungnir"], "the file was written anyway");
    watching.close().await;
}
