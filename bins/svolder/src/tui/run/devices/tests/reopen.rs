// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a reopen keeps or seeds (histories, the link strip), and the preset list's AutoEq import.

use super::*;

/// An app watching a device whose unit id is `unit`, holding one history sample, switched to
/// `name`; the app that replaces it, with the id of the sample.
async fn switched_with_history(unit: &str, name: &str) -> (Watching, svolder_proto::FunctionId) {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    let opener = opener_over(fake_open(Log::default(), Arc::clone(&gate), &[]));
    app.session.unit = Some(unit.into());
    let id = "99.9".parse().expect("a function id");
    app.readings
        .histories
        .record(&app.profile, id, std::time::Instant::now(), 3.0);
    let mut watching = watching(backend, app, Some("gungnir"));
    watching.app.devices.rows = device_rows();
    let switched = switch_device(&opener, &mut watching, name, &look(), &mut ()).await;
    assert_eq!(switched, Switched::Opened);
    assert_eq!(watching.app.session.unit.as_deref(), Some(OPENED_UNIT));
    (watching, id)
}

/// A reopen of the same device (the unit id the old app had) carries the histories on.
#[tokio::test(start_paused = true)]
async fn a_reopen_of_the_same_device_keeps_the_histories() {
    let (watching, id) = switched_with_history(OPENED_UNIT, "gungnir").await;
    assert!(watching.app.readings.histories.reading(id).is_some());
    watching.close().await;
}

/// A switch to a device with another unit id starts the histories empty.
#[tokio::test(start_paused = true)]
async fn a_switch_to_another_device_starts_the_histories_empty() {
    let (watching, id) = switched_with_history("0807060504030201", "bifrost").await;
    assert!(watching.app.readings.histories.reading(id).is_none());
    watching.close().await;
}

/// A reopen over a daemon that knows `history` seeds the strip from its changes.
#[tokio::test]
async fn a_reopen_over_a_daemon_seeds_the_link_strip_from_its_history() {
    use svolder_ipc::LinkChange;

    let daemon = stub_daemon().await;
    daemon.history(vec![
        LinkChange {
            at_ms: 1_000,
            connected: true,
        },
        LinkChange {
            at_ms: 9_000,
            connected: false,
        },
    ]);
    let (spare, gate, _) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let watching = opener.reopen(None, None).await.expect("the daemon opens");
    let at = |ms| {
        watching
            .app
            .readings
            .link
            .columns(ms, Duration::from_secs(10), 10)
    };
    assert_eq!(at(10_000)[1], Some(1.0));
    assert_eq!(at(10_000)[9], Some(0.0), "the daemon's last word is down");
    assert_eq!(daemon.history_spans(), [24 * 3600]);
    watching.close().await;
}

/// An older daemon's error leaves the strip on what this session saw: the link as it was when
/// the session opened.
#[tokio::test]
async fn a_reopen_over_an_older_daemon_falls_back_to_its_own_observations() {
    crate::tui::widgets::graph::hold_wall_ms(50_000);
    let daemon = stub_daemon().await;
    let (spare, gate, _) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let watching = opener.reopen(None, None).await.expect("the daemon opens");
    let columns = watching
        .app
        .readings
        .link
        .columns(55_000, Duration::from_secs(10), 10);
    assert_eq!(columns[..5], [None; 5]);
    assert_eq!(columns[5..], [Some(1.0); 5]);
    watching.close().await;
}

/// A daemon with no record of the device does not erase what this session saw: the carried
/// link history stays and the link is recorded up as of the reopen.
#[tokio::test]
async fn an_empty_daemon_history_keeps_the_carried_link() {
    crate::tui::widgets::graph::hold_wall_ms(50_000);
    let daemon = stub_daemon().await;
    daemon.history(Vec::new());
    let (spare, gate, mut old) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    old.session.unit = Some(OPENED_UNIT.into());
    old.readings.link.record(10_000, true);
    old.readings.link.record(20_000, false);
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let watching = opener
        .reopen(None, Some(&old))
        .await
        .expect("the daemon opens");
    let columns = watching
        .app
        .readings
        .link
        .columns(60_000, Duration::from_secs(60), 6);
    assert_eq!(
        columns,
        vec![None, Some(1.0), Some(0.0), Some(0.0), Some(0.0), Some(1.0)]
    );
    watching.close().await;
}

/// A same-device reopen over a restarted daemon keeps what the TUI held: the daemon's one change
/// ("up since the restart") joins it, and the gap between, which the old feed's close recorded,
/// draws down.
#[tokio::test]
async fn a_reopen_over_a_restarted_daemon_keeps_the_carried_strip_and_shows_the_gap() {
    use svolder_ipc::LinkChange;

    crate::tui::widgets::graph::hold_wall_ms(50_000);
    let daemon = stub_daemon().await;
    daemon.history(vec![LinkChange {
        at_ms: 40_000,
        connected: true,
    }]);
    let (spare, gate, mut old) = fake(SHORT_DEADLINE).await;
    spare.close().await;
    old.session.unit = Some(OPENED_UNIT.into());
    old.readings.link.record(1_000, true);
    old.readings.link.record(10_000, false);
    old.readings.link.record(20_000, true);
    old.readings.link.record(30_000, false);
    let opener = opener_over(remote_opener(Log::default(), gate, daemon.socket.clone()));
    let watching = opener
        .reopen(None, Some(&old))
        .await
        .expect("the daemon opens");
    let columns = watching
        .app
        .readings
        .link
        .columns(50_000, Duration::from_secs(50), 5);
    assert_eq!(
        columns,
        vec![Some(1.0), Some(0.0), Some(1.0), Some(0.0), Some(1.0)]
    );
    watching.close().await;
}
/// Carry out `intents` one after the other, as the loop does for one key.
async fn carried_all<O: Open>(
    intents: Vec<Intent>,
    opener: &Opener<O>,
    mut watching: Watching,
) -> Watching {
    for intent in intents {
        watching = carried_on(taken(intent, opener, watching).await);
    }
    watching
}

/// The preset list's AutoEq search and import through the loop, with a stub
/// daemon that serves AutoEq: the intents start tasks, the tasks' ends come back the way the
/// loop's own arm takes them, and the daemon's import lands in the configuration file as an
/// imported preset, unloaded, which the loop then reads again.
#[tokio::test]
async fn the_preset_list_imports_from_autoeq_through_the_loop() {
    use crossterm::event::{KeyCode, KeyEvent};

    use crate::tui::run::jobs::next;
    use crate::tui::run::test_support::on_a_daemon;

    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.autoeq().index(&[
        "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990",
    ]);
    daemon.autoeq().saves("Sennheiser HD 650", 1);
    daemon.autoeq().writes_into(
        path.clone(),
        "Preamp: -3.0 dB\nFilter 1: ON PK Fc 1000 Hz Gain 2.0 dB Q 1.0\n",
    );
    let opener = opener_on(path.clone(), never_opens());
    watching.app.show(Screen::Tone);

    let press = async |mut watching: Watching, code: KeyCode| {
        let intents = watching.app.key(KeyEvent::from(code));
        carried_all(intents, &opener, watching).await
    };
    for code in [KeyCode::Char('p'), KeyCode::Char('a')] {
        watching = press(watching, code).await;
    }
    // The first question, sent at once, ends; its answer is folded in where the loop's arm
    // folds it.
    let done = next(&mut watching.jobs.ended).await;
    let intents = presets::done(&mut watching.app, done);
    watching = carried_all(intents, &opener, watching).await;
    // Every key asks, and the loop sends only the last question once the typing pauses.
    for letter in "hd650".chars() {
        watching = press(watching, KeyCode::Char(letter)).await;
    }
    assert_eq!(daemon.autoeq().searches(), [(String::new(), false)]);
    watching.jobs.send_due();
    let done = next(&mut watching.jobs.ended).await;
    let intents = presets::done(&mut watching.app, done);
    watching = carried_all(intents, &opener, watching).await;
    assert_eq!(
        daemon.autoeq().searches(),
        [(String::new(), false), ("hd650".to_string(), false)]
    );
    watching = press(watching, KeyCode::Enter).await;
    let done = next(&mut watching.jobs.ended).await;
    let intents = presets::done(&mut watching.app, done);
    watching = carried_all(intents, &opener, watching).await;

    let saved = Config::load_from(&path).expect("the config loads").presets;
    let [imported] = saved.as_slice() else {
        panic!("expected one preset in the file, got {saved:?}");
    };
    assert_eq!(imported.name, "Sennheiser HD 650");
    assert_eq!(
        imported
            .source
            .as_ref()
            .and_then(|source| source.origin.as_deref()),
        Some("AutoEq · oratory1990 · over-ear")
    );
    assert_eq!(
        watching.app.presets.loaded_preset, None,
        "the import was loaded"
    );

    watching.close().await;
}

/// An imported preset on the computer's side: `Enter` sends it to the loop's own controller, whose
/// view names it as the one on the computer.
#[tokio::test]
async fn an_imported_preset_goes_to_the_computer_through_the_loop() {
    use crossterm::event::{KeyCode, KeyEvent};

    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    let path = config_naming(&["gungnir"]);
    let kept = svolder_profiles::tone::preset::Preset::imported(
        "Sennheiser HD 650",
        svolder_profiles::tone::preset::Imported {
            origin: Some("AutoEq · oratory1990 · over-ear".into()),
            correction: svolder_profiles::tone::apo::parse(
                "Preamp: -3.0 dB\nFilter 1: ON PK Fc 1000 Hz Gain 2.0 dB Q 1.0\n",
            )
            .expect("the correction parses")
            .correction,
        },
    );
    Config::add_preset(&path, &kept).expect("the scratch config takes the preset");
    let opener = opener_on(path.clone(), never_opens());
    let _ = relisted(&path, &mut app);
    app.show(Screen::Tone);
    let mut watching = watching(backend, app, None);
    let press = async |mut watching: Watching, code: KeyCode| {
        let intents = watching.app.key(KeyEvent::from(code));
        carried_all(intents, &opener, watching).await
    };

    // On the computer's side `Enter` on it sends it to the loop's own controller, whose view
    // names it as the one on the computer.
    let chain = Arc::new(svolder_host::fake::FakeChain::new());
    watching.computer = Box::new(crate::tui::run::computer::ComputerLink::over(
        svolder_host::eq::EqControl::new(
            chain.clone(),
            Box::new(svolder_host::fake::FakeParams::new()),
            svolder_host::eq::Keep::Memory,
        ),
    ));
    watching.app.host.computer.eq = watching.computer.view().map(Box::new);
    watching = press(watching, KeyCode::Char('C')).await;
    assert!(watching.app.on_computer());
    watching = press(watching, KeyCode::Char('p')).await;
    for _ in 0..20 {
        watching = press(watching, KeyCode::Down).await;
    }
    watching = press(watching, KeyCode::Enter).await;
    let seen = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let view = crate::tui::run::computer::next_view(&mut watching.computer.views).await;
            if view.on {
                return view;
            }
        }
    })
    .await
    .expect("the controller applied it");
    assert_eq!(seen.preset.as_deref(), Some("Sennheiser HD 650"));
    assert!(
        chain
            .calls()
            .iter()
            .any(|call| matches!(call, svolder_host::fake::ChainCall::Apply(_)))
    );
    watching.close().await;
}

/// A search the daemon is slow to answer (it may be fetching AutoEq's index, 10 to 60 s) holds
/// nothing of the loop: the intent returns at once, keys go on being taken meanwhile, and the
/// answer arrives when the daemon gives it.
#[tokio::test]
async fn a_slow_search_never_holds_the_loop() {
    use crossterm::event::{KeyCode, KeyEvent};

    use crate::tui::run::jobs::next;
    use crate::tui::run::test_support::on_a_daemon;

    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.autoeq().index(&[
        "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990",
    ]);
    daemon.autoeq().holds_searches();
    let opener = opener_on(path, never_opens());
    watching.app.show(Screen::Tone);

    for code in [KeyCode::Char('p'), KeyCode::Char('a'), KeyCode::Char('h')] {
        let intents = watching.app.key(KeyEvent::from(code));
        watching = carried_all(intents, &opener, watching).await;
    }
    assert!(
        presets::search::showing(&watching.app),
        "the box is up while the daemon is still working"
    );
    assert!(watching.jobs.ended.try_recv().is_err());

    daemon.autoeq().lets_searches_go();
    let done = next(&mut watching.jobs.ended).await;
    assert!(matches!(done, presets::Done::Searched(..)));
    watching.close().await;
}

/// Leaving the search box leaves no question behind it: `Esc` within the pause sends nothing, and
/// the question already in flight is stopped, as the loop settles it each turn.
#[tokio::test]
async fn leaving_the_search_box_drops_its_questions() {
    use crossterm::event::{KeyCode, KeyEvent};

    let (daemon, path, mut watching) = crate::tui::run::test_support::on_a_daemon().await;
    daemon.autoeq().index(&[
        "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990",
    ]);
    daemon.autoeq().holds_searches();
    let opener = opener_on(path, never_opens());
    watching.app.show(Screen::Tone);
    for code in [KeyCode::Char('p'), KeyCode::Char('a'), KeyCode::Char('h')] {
        let intents = watching.app.key(KeyEvent::from(code));
        watching = carried_all(intents, &opener, watching).await;
    }
    assert!(
        watching.jobs.due_at().is_some(),
        "a question waits for the typing"
    );
    assert!(
        watching.jobs.is_searching(),
        "the opening question is held by the daemon"
    );
    // While the box shows, the loop leaves them be.
    watching.settle_search();
    assert!(watching.jobs.due_at().is_some());
    assert!(watching.jobs.is_searching());

    let intents = watching.app.key(KeyEvent::from(KeyCode::Esc));
    watching = carried_all(intents, &opener, watching).await;
    watching.settle_search();
    assert!(
        watching.jobs.due_at().is_none(),
        "the waiting question outlived the box"
    );
    // An aborted task ends at its next poll.
    tokio::task::yield_now().await;
    assert!(
        !watching.jobs.is_searching(),
        "the question in flight outlived the box"
    );
    watching.jobs.send_due();
    daemon.autoeq().lets_searches_go();
    tokio::task::yield_now().await;
    assert!(
        watching.jobs.ended.try_recv().is_err(),
        "an answer came for a box that is gone"
    );
    // The typed question never reached the daemon (the opening one may have, or been stopped).
    assert!(
        daemon
            .autoeq()
            .searches()
            .iter()
            .all(|(query, _)| query.is_empty())
    );
    watching.close().await;
}
