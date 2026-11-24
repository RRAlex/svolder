// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Settings changed while no chain was ever applied (no state file, no chain): kept in a file of
//! their own, loaded back at the next start, and never a curve.

use super::*;

fn fresh(dir: &Dir) -> (EqControl, FakeChain) {
    let chain = FakeChain::new();
    let control = EqControl::new(
        Arc::new(chain.clone()),
        Box::new(FakeParams::new()),
        Keep::File(dir.state()),
    );
    (control, chain)
}

fn restarted(dir: &Dir) -> EqControl {
    fresh(dir).0
}

#[tokio::test]
async fn a_crossfeed_set_with_nothing_running_survives_a_restart() {
    let dir = Dir::new();
    let (mut control, _) = fresh(&dir);
    control
        .crossfeed(CrossfeedEdit {
            amount_pct: Some(33.0),
            done: true,
            ..CrossfeedEdit::default()
        })
        .await
        .unwrap();
    let before = control.view().crossfeed.unwrap();
    assert_eq!(before.amount_pct, 33.0);
    let after = restarted(&dir).view().crossfeed.unwrap();
    assert_eq!(after, before);
    assert!(!after.on, "the stage never starts by itself");
}

#[tokio::test]
async fn a_speakers_set_chosen_with_nothing_running_survives_a_restart() {
    let dir = Dir::new();
    let store = Arc::new(svolder_speakers::Store::new(&dir.0.join("speakers")));
    vs::put_set(&store, "set-a", "Dummy head, A");
    let new = |dir: &Dir| {
        EqControl::new(
            Arc::new(FakeChain::new()),
            Box::new(FakeParams::new()),
            Keep::File(dir.state()),
        )
        .with_speakers(Arc::clone(&store))
    };
    let mut control = new(&dir);
    control
        .speakers(SpeakersEdit {
            on: None,
            set: Some("set-a".into()),
            done: true,
        })
        .await
        .unwrap();
    assert_eq!(
        control.view().speakers.unwrap().set.as_deref(),
        Some("set-a")
    );
    let after = new(&dir).view().speakers.unwrap();
    assert_eq!(after.set.as_deref(), Some("set-a"));
    assert!(!after.on);
}

#[tokio::test]
async fn a_loudness_strength_set_with_nothing_running_survives_a_restart() {
    let dir = Dir::new();
    let (mut control, _) = fresh(&dir);
    control
        .loudness(LoudnessEdit {
            strength_pct: Some(20.0),
            done: true,
            ..LoudnessEdit::default()
        })
        .await
        .unwrap();
    let after = restarted(&dir).view().loudness.unwrap();
    assert_eq!(after.strength_pct, 20.0);
    assert!(!after.on);
}

#[tokio::test]
async fn kept_settings_invent_no_curve_and_apply_no_chain() {
    let dir = Dir::new();
    let (mut control, chain) = fresh(&dir);
    control
        .loudness(LoudnessEdit {
            strength_pct: Some(20.0),
            done: true,
            ..LoudnessEdit::default()
        })
        .await
        .unwrap();
    // Not a state file: that would hold a curve, a target and a route that were never chosen.
    assert_eq!(HostEqState::load(&dir.state()).unwrap(), None);
    let text = std::fs::read_to_string(dir.settings()).unwrap();
    for word in ["filters", "preamp", "target", "route"] {
        assert!(!text.contains(word), "{word} in {text}");
    }
    let (mut next, next_chain) = fresh(&dir);
    let view = next.view();
    assert!(!view.on, "no chain is claimed");
    assert!(view.points.len() <= 1, "no curve: {:?}", view.points);
    for _ in 0..3 * WATCH_EVERY {
        next.tick().await;
    }
    assert!(chain.calls().is_empty(), "{:?}", chain.calls());
    assert!(next_chain.calls().is_empty(), "{:?}", next_chain.calls());
    assert_eq!(view.loudness.unwrap().strength_pct, 20.0);
}

#[tokio::test]
async fn the_first_state_carries_the_settings_and_takes_the_settings_file_away() {
    let dir = Dir::new();
    let (mut control, _) = fresh(&dir);
    control
        .loudness(LoudnessEdit {
            strength_pct: Some(20.0),
            done: true,
            ..LoudnessEdit::default()
        })
        .await
        .unwrap();
    assert!(dir.settings().exists());
    control.set_on(true).await.unwrap();
    assert!(!dir.settings().exists());
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert_eq!(state.loudness.map(|s| s.strength_pct), Some(20.0));
}

#[tokio::test]
async fn a_settings_file_that_says_loudness_is_on_starts_it_off() {
    let dir = Dir::new();
    std::fs::write(
        dir.settings(),
        "[loudness]\non = true\nstrength_pct = 20.0\nreference_pct = 100.0\n",
    )
    .unwrap();
    let (mut next, next_chain) = fresh(&dir);
    for _ in 0..3 * WATCH_EVERY {
        next.tick().await;
    }
    let view = next.view().loudness.unwrap();
    assert!(!view.on);
    assert_eq!(view.strength_pct, 20.0);
    assert!(next_chain.calls().is_empty(), "{:?}", next_chain.calls());
}

/// A state file as `Host::apply` writes it before any setting was kept in it: no tables.
fn bare_state() -> HostEqState {
    HostEqState {
        on: false,
        name: "test".into(),
        origin: None,
        target: String::new(),
        route: crate::eq::Route::Smart,
        manager: crate::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: vec![pk(100.0, 1.0, 1.0)],
        slots: Vec::new(),
        crossfeed: None,
        crossfeed_on: false,
        crossfeed_loaded: None,
        flat: false,
        speakers: None,
        speakers_on: false,
        speakers_loaded: None,
        loudness: None,
        bypass: None,
    }
}

const SETTINGS: &str = "speakers = \"set-a\"\n[crossfeed]\nmodel = \"bauer\"\namount = 0.33\n\
                        [loudness]\non = true\nstrength_pct = 20.0\nreference_pct = 100.0\n";

#[tokio::test]
async fn a_state_written_from_outside_takes_the_kept_settings_it_has_no_table_for() {
    // A no-daemon `host eq apply`, or a crash before the fold: a state with no tables, beside
    // the settings file.
    let dir = Dir::new();
    bare_state().save(&dir.state()).unwrap();
    std::fs::write(dir.settings(), SETTINGS).unwrap();
    let chain = FakeChain::with_state(bare_state());
    let mut control = EqControl::new(
        Arc::new(chain),
        Box::new(FakeParams::new()),
        Keep::File(dir.state()),
    );
    let view = control.view();
    assert_eq!(view.crossfeed.unwrap().amount_pct, 33.0);
    let loudness = view.loudness.unwrap();
    assert_eq!(loudness.strength_pct, 20.0);
    assert!(!loudness.on, "a start never begins on");
    // The next save writes them into the state, and only then is the file taken away.
    control.edit(Edit::Add(pk(1000.0, 2.0, 1.0))).await.unwrap();
    let state = HostEqState::load(&dir.state()).unwrap().unwrap();
    assert_eq!(state.loudness.map(|l| l.strength_pct), Some(20.0));
    assert_eq!(state.speakers.as_deref(), Some("set-a"));
    assert!(state.crossfeed.is_some());
    assert!(!dir.settings().exists());
}

#[tokio::test]
async fn a_state_that_has_a_table_keeps_it_over_the_settings_file() {
    let dir = Dir::new();
    let mut state = bare_state();
    state.loudness = Some(crate::eq::loudness::LoudnessSettings {
        strength_pct: 80.0,
        ..Default::default()
    });
    state.save(&dir.state()).unwrap();
    std::fs::write(dir.settings(), SETTINGS).unwrap();
    let control = EqControl::new(
        Arc::new(FakeChain::with_state(state)),
        Box::new(FakeParams::new()),
        Keep::File(dir.state()),
    );
    assert_eq!(control.view().loudness.unwrap().strength_pct, 80.0);
}

#[test]
fn the_effective_settings_read_the_state_then_the_settings_file() {
    let dir = Dir::new();
    // Neither file: nothing.
    assert_eq!(
        crate::eq::HostEqSettings::effective(&dir.state()),
        crate::eq::HostEqSettings::default()
    );
    // The settings file alone: its tables, loudness off.
    std::fs::write(dir.settings(), SETTINGS).unwrap();
    let alone = crate::eq::HostEqSettings::effective(&dir.state());
    assert_eq!(alone.speakers.as_deref(), Some("set-a"));
    assert_eq!(alone.crossfeed.map(|c| c.amount), Some(0.33));
    assert_eq!(
        alone.loudness.map(|l| (l.on, l.strength_pct)),
        Some((false, 20.0))
    );
    // A state with no tables: the settings file fills them. A table the state has wins.
    let mut state = bare_state();
    state.speakers = Some("set-b".into());
    state.save(&dir.state()).unwrap();
    let both = crate::eq::HostEqSettings::effective(&dir.state());
    assert_eq!(both.speakers.as_deref(), Some("set-b"));
    assert_eq!(both.loudness.map(|l| l.strength_pct), Some(20.0));
}

#[tokio::test]
async fn a_malformed_settings_file_is_ignored_and_the_next_save_replaces_it() {
    let dir = Dir::new();
    std::fs::write(dir.settings(), "[loudness\nnot toml").unwrap();
    let (mut control, _) = fresh(&dir);
    assert_eq!(control.view().loudness.unwrap().strength_pct, 50.0);
    // The reason is kept for a daemon's log and shown once in the view's note; nothing is printed.
    assert!(
        control
            .settings_unread()
            .unwrap()
            .contains("host-eq-settings.toml")
    );
    assert!(
        control
            .view()
            .note
            .unwrap()
            .contains("kept settings were ignored")
    );
    control
        .loudness(LoudnessEdit {
            strength_pct: Some(20.0),
            done: true,
            ..LoudnessEdit::default()
        })
        .await
        .unwrap();
    assert_eq!(restarted(&dir).view().loudness.unwrap().strength_pct, 20.0);
}

#[test]
fn load_returns_the_reason_and_a_missing_file_is_no_reason() {
    let dir = Dir::new();
    assert_eq!(crate::eq::HostEqSettings::load(&dir.state()), Ok(None));
    assert_eq!(crate::eq::HostEqSettings::unreadable(&dir.state()), None);
    std::fs::write(dir.settings(), "[loudness\nnot toml").unwrap();
    let why = crate::eq::HostEqSettings::load(&dir.state()).unwrap_err();
    assert!(why.contains("host-eq-settings.toml"), "{why}");
    assert_eq!(
        crate::eq::HostEqSettings::unreadable(&dir.state()),
        Some(why)
    );
}
