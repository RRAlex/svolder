// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The flags the app raises for the animation layer, and the mascot that shows the device.

use super::*;

/// An app on a profile with an over-current fault, which no DAC profile has.
fn faulty_app() -> App {
    App::new(
        with_overcurrent(profile()),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![row("gungnir")],
    )
}

#[test]
fn a_new_true_fault_schedules_a_header_flash() {
    let mut app = faulty_app();
    let t0 = Instant::now();
    let fault = |on: bool| vo("230.1", Some("fault_overcurrent"), json!(on), "");
    feed(&mut app, fault(false), t0);
    assert!(
        app.frame_flags.pending_flash.is_none(),
        "a clear fault does not flash"
    );
    assert!(app.faults().is_empty());
    let active = t0 + Duration::from_secs(1);
    feed(&mut app, fault(true), active);
    assert_eq!(app.frame_flags.pending_flash, Some(active));
    let names: Vec<&str> = app.faults().iter().map(|c| c.key.as_str()).collect();
    assert_eq!(names, ["fault_overcurrent"]);
    app.frame_flags.pending_flash = None;
    feed(&mut app, fault(true), t0 + Duration::from_secs(2));
    assert!(
        app.frame_flags.pending_flash.is_none(),
        "a fault that was already active does not flash again"
    );
}

/// A refresh reports every fault the device holds, the ones already shown included, so the
/// header flashes only for a fault that was not standing before it.
#[test]
fn a_refresh_flashes_only_for_a_fault_that_was_not_already_active() {
    let mut app = faulty_app();
    let t0 = Instant::now();
    let fault = |on: bool| vo("230.1", Some("fault_overcurrent"), json!(on), "");
    feed(&mut app, fault(true), t0);
    assert!(app.frame_flags.pending_flash.is_some(), "the fault came on");
    app.frame_flags.pending_flash = None;

    app.replace_all(vec![fault(true)], t0 + Duration::from_secs(1));
    assert!(
        app.frame_flags.pending_flash.is_none(),
        "a refresh flashed for a fault that was already standing"
    );
    assert_eq!(app.faults().len(), 1, "the fault is still shown");

    app.replace_all(vec![fault(false)], t0 + Duration::from_secs(2));
    assert!(app.faults().is_empty(), "the fault cleared");
    app.replace_all(vec![fault(true)], t0 + Duration::from_secs(3));
    assert_eq!(
        app.frame_flags.pending_flash,
        Some(t0 + Duration::from_secs(3)),
        "a fault that was not standing before the refresh does flash"
    );
}

/// The keys moving to another pane is the user's own doing, and it is flagged where they
/// do it: `Tab` from the keyboard and a click on a pane's title alike. A click landing in
/// the pane the keys are already in has moved them nowhere, and a screen putting its own
/// focus back on its first pane is not the user moving it either.
#[test]
fn the_focus_moving_to_another_pane_is_flagged_for_the_light_that_runs_round_it() {
    let mut app = app();
    assert!(
        !app.frame_flags.pane_changed,
        "the keys have not been anywhere"
    );

    press(&mut app, KeyCode::Tab);
    assert_eq!(app.pane, Pane::Tone, "Tab moved the keys on");
    assert!(
        app.frame_flags.pane_changed,
        "Tab is the user moving the keys"
    );
    app.frame_flags.pane_changed = false;

    app.focus(Pane::Controls, 1);
    assert!(
        app.frame_flags.pane_changed,
        "a click on a pane's title is one too"
    );
    app.frame_flags.pane_changed = false;

    app.focus(Pane::Controls, 2);
    assert!(
        !app.frame_flags.pane_changed,
        "a click inside the pane the keys are in moved them to another pane"
    );
}

/// The two flags the animation layer reads are raised where the events happen: a screen
/// that changed, and a link that came back. Neither is raised by an event that changed
/// nothing, so no effect fires over a frame that looks exactly like the one before it.
#[test]
fn a_screen_change_and_a_reconnect_are_flagged_for_the_animation_layer() {
    let mut app = app();
    assert!(!app.frame_flags.screen_changed, "nothing has changed yet");
    assert!(
        !app.frame_flags.reconnected,
        "the link has not been anywhere"
    );

    ch(&mut app, 't');
    assert!(
        app.frame_flags.screen_changed,
        "the telemetry screen is a screen change"
    );
    app.frame_flags.screen_changed = false;
    ch(&mut app, 't');
    assert!(
        !app.frame_flags.screen_changed,
        "the screen already showing is not a change"
    );
    app.show(Screen::Device);
    assert!(
        app.frame_flags.screen_changed,
        "a click on a tab header is one too"
    );

    let now = Instant::now();
    app.apply(Update::Connection { connected: true }, now);
    assert!(!app.frame_flags.reconnected, "the link never went away");
    app.apply(Update::Connection { connected: false }, now);
    assert!(
        !app.frame_flags.reconnected,
        "and going away is not coming back"
    );
    app.apply(Update::Connection { connected: true }, now);
    assert!(app.frame_flags.reconnected, "the link came back");
}

/// The row the cursor leaves is remembered for the trail it earns, whether a key or a
/// click moved it, and a key that moves the cursor nowhere leaves nothing behind.
#[test]
fn the_row_the_cursor_leaves_is_remembered_for_its_trail() {
    let mut app = app();
    let first = app.cursor_id().expect("a row is focused from the start");
    press(&mut app, KeyCode::Down);
    let second = app.cursor_id();
    assert_eq!(
        app.frame_flags.left_row.take(),
        Some(first),
        "the cursor moved down"
    );
    ch(&mut app, '+');
    assert_eq!(
        app.frame_flags.left_row, None,
        "volume does not move the cursor"
    );
    app.focus(Pane::Controls, 0);
    assert_eq!(
        app.frame_flags.left_row, second,
        "a click moves the cursor too"
    );
    assert_eq!(app.cursor_id(), Some(first), "and lands where it clicked");
}

#[test]
fn an_accent_change_sweeps_like_a_palette_change() {
    assert!(TuiChange::Accent(Family::Dac, Some("#ff0000".into())).sweeps());
}

/// The squirrel's view reads what the device has said.
#[test]
fn the_mascot_is_shown_the_device() {
    use crate::tui::test_support::hear_one;

    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let volume = svolder_profiles::roles::volume(&app.profile).expect("a volume");
    let (min, max) = volume.range.expect("a range");
    let id = volume.id.to_string();
    hear_one(&mut app, &id, min + 3);
    let view = app.mascot_view();
    assert_eq!(view.volume, Some((min + 3, min, max)));
    assert!(view.screen_is_home);
    assert_eq!(view.link, crate::tui::decoration::mascot::Link::Connected);
    assert!(view.eq.is_none(), "no tone reading has been heard");
    assert!(!app.mascot_on(), "hidden until the terminal can show it");
}

/// The hour's toast is told only while the squirrel may be seen, and never over a Failure toast.
#[test]
fn the_hour_is_told_only_when_the_mascot_is_on_and_never_covers_a_failure() {
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let t0 = app.now;
    let day = (3, 4);
    app.observe_mascot(t0, day);
    app.observe_mascot(t0 + Duration::from_secs(3600), day);
    assert!(app.toast().is_none(), "the mascot is off");

    app.mascot.capable = true;
    assert!(app.mascot_on());
    app.warn("a failure");
    app.observe_mascot(t0 + Duration::from_secs(7200), day);
    assert_eq!(app.toast().map(|t| t.kind), Some(ToastKind::Failure));
    app.dismiss_toast();
    app.observe_mascot(t0 + Duration::from_secs(10800), day);
    assert_eq!(
        app.toast().map(|t| t.text.as_str()),
        Some("3 h in. Ratatoskr stretches.")
    );
}

/// A squirrel whose home box has no room is away from home: the view says so, a called one
/// floats the input visit, and the hour's words come with a stretch that is drawn.
#[test]
fn a_home_with_no_room_is_away_from_home() {
    use crate::tui::decoration::mascot::tricks::Trick;
    use crate::tui::decoration::mascot::{Code, Place, What};
    use crate::tui::test_support::hear_one;

    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    assert!(app.mascot_view().screen_is_home, "the pane has room");
    let eq = app.profile.eq.as_mut().expect("a tone control");
    let band = eq.bands[0].clone();
    eq.bands.extend(std::iter::repeat_n(band, 3));
    assert!(!app.mascot_view().screen_is_home, "the pane has none");

    let input = app.profile.control_by_key("input").expect("an input").id;
    hear_one(&mut app, &input.to_string(), 1);
    app.mascot.capable = true;
    let t0 = app.now;
    app.observe_mascot(t0, (3, 4));
    app.mascot.code(Code::Konami, t0);
    app.observe_mascot(t0 + Duration::from_secs(10), (3, 4));
    hear_one(&mut app, &input.to_string(), 2);
    app.observe_mascot(t0 + Duration::from_secs(20), (3, 4));
    assert_eq!(
        app.mascot.what(),
        Some((What::Trick(Trick::Messenger), Place::Float))
    );
    app.observe_mascot(t0 + Duration::from_secs(3600), (3, 4));
    assert_eq!(
        app.mascot.what(),
        Some((What::Trick(Trick::Stretch), Place::Float))
    );
    assert!(
        app.toast()
            .is_some_and(|toast| toast.text.contains("stretches"))
    );
}
