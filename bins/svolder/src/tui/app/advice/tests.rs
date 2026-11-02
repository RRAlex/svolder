// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use serde_json::json;
use svolder_ipc::TuiConfig;
use svolder_profiles::{Family, Registry};

use super::*;
use crate::backend::{DeviceRow, Update};
use crate::tui::test_support::{plain_vo, press, profile, vo, with_overcurrent};

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn app_on(profile: svolder_profiles::Profile) -> App {
    App::new(
        profile,
        "dev".into(),
        &TuiConfig::default(),
        vec![DeviceRow {
            name: "dev".into(),
            ..DeviceRow::default()
        }],
    )
}

fn named(name: &str) -> App {
    app_on(
        Registry::builtin()
            .expect("the built-in profiles load")
            .get(name)
            .unwrap_or_else(|| panic!("{name} is built in"))
            .clone(),
    )
}

fn say(app: &mut App) -> String {
    press(app, KeyCode::Char('a'));
    app.toast().expect("a toast").text.clone()
}

#[test]
fn since_has_four_forms() {
    assert_eq!(since(secs(30)), "under a minute");
    assert_eq!(since(secs(59)), "under a minute");
    assert_eq!(since(secs(60)), "1 min");
    assert_eq!(since(secs(59 * 60)), "59 min");
    assert_eq!(since(secs(3600)), "1 h 0 min");
    assert_eq!(since(secs(23 * 3600 + 59 * 60)), "23 h 59 min");
    assert_eq!(since(secs(24 * 3600)), "1 d 0 h");
    assert_eq!(since(secs(3 * 86400 + 5 * 3600)), "3 d 5 h");
}

#[test]
fn a_profile_with_fault_controls_says_all_clear_and_one_without_says_connected() {
    let mut app = app_on(with_overcurrent(profile()));
    assert!(say(&mut app).starts_with("All clear for under a minute · tip: "));
    let mut app = app_on(profile());
    assert!(say(&mut app).starts_with("Connected for under a minute · tip: "));
}

#[test]
fn the_time_counts_from_the_last_clear_else_from_opening() {
    let mut app = app_on(with_overcurrent(profile()));
    let t0 = app.notices.opened_at;
    assert!(idle_line(&app, t0 + secs(120), 1).starts_with("All clear for 2 min"));
    app.notices.fault_cleared_at = Some(t0 + secs(100));
    assert!(idle_line(&app, t0 + secs(120), 1).starts_with("All clear for under a minute"));
}

#[test]
fn the_toast_is_info_and_lasts_six_seconds() {
    let mut app = app_on(profile());
    let t0 = Instant::now();
    app.tick(t0);
    say(&mut app);
    assert_eq!(
        app.toast().map(|toast| toast.kind),
        Some(crate::tui::boxes::toast::ToastKind::Info)
    );
    app.tick(t0 + secs(4));
    assert!(app.toast().is_some(), "still there at 4 s");
    app.tick(t0 + secs(6));
    assert!(app.toast().is_none(), "gone at 6 s");
}

#[test]
fn four_tips_then_a_quip_then_the_fifth_tip() {
    let mut app = app_on(profile());
    let tips: Vec<String> = (0..6).map(|_| say(&mut app)).collect();
    let tail = |n: usize| tips[n].split(" · ").skip(1).collect::<Vec<_>>().join(" · ");
    let all = super::tips();
    for (press, tip) in [(0, 0), (1, 1), (2, 2), (3, 3), (5, 4)] {
        assert_eq!(tail(press), format!("tip: {}", all[tip]), "press {press}");
    }
    assert!(tips[4].ends_with("Bits are flowing. Nothing to fix."));
}

#[test]
fn the_tip_list_wraps_and_names_only_keys_in_the_table() {
    let all = super::tips();
    let n = all.len();
    assert_eq!(tip(n), tip(0));
    let table = keys::all();
    for tip in &all[..table.len()] {
        assert!(
            table
                .iter()
                .any(|hint| *tip == format!("{} · {}", hint.label, hint.does))
        );
    }
    assert!(all.iter().any(|tip| tip.starts_with("L-drag · ")));
}

#[test]
fn each_family_has_its_quip() {
    for (name, family, quip) in [
        (
            "gungnir2-v2",
            Family::Dac,
            "Bits are flowing. Nothing to fix.",
        ),
        (
            "amp-stereo",
            Family::Amp,
            "Nothing to report. Enjoy the music.",
        ),
        (
            "preamp-common",
            Family::Preamp,
            "Signal's clean. Nothing to fix.",
        ),
    ] {
        let app = named(name);
        assert_eq!(app.profile.family, family, "{name}");
        assert!(
            idle_line(&app, app.notices.opened_at, 5).ends_with(quip),
            "{name}"
        );
    }
}

fn mode(app: &mut App, id: &str, raw: i64) {
    app.apply(Update::Status(plain_vo(id, raw)), Instant::now());
}

#[test]
fn the_tube_quip_follows_the_modes_value() {
    let mut lyr = named("lyr5");
    let tube = "Tubes warm, nothing to report.";
    assert!(
        !idle_line(&lyr, lyr.notices.opened_at, 5).ends_with(tube),
        "unread"
    );
    mode(&mut lyr, "7", 1);
    assert!(
        !idle_line(&lyr, lyr.notices.opened_at, 5).ends_with(tube),
        "Solid State"
    );
    mode(&mut lyr, "7", 2);
    assert!(
        idle_line(&lyr, lyr.notices.opened_at, 5).ends_with(tube),
        "Tube"
    );

    let mut freya = named("freya2");
    for (raw, tubes) in [(0, false), (1, false), (2, true), (3, true), (4, true)] {
        mode(&mut freya, "3.1", raw);
        assert_eq!(
            idle_line(&freya, freya.notices.opened_at, 5).ends_with(tube),
            tubes,
            "mode {raw}"
        );
    }
}

#[test]
fn a_fault_around_a_press() {
    let mut app = app_on(with_overcurrent(profile()));
    let fault = |on: bool| vo("230.1", Some("fault_overcurrent"), json!(on), "x");
    app.apply(Update::Status(fault(true)), Instant::now());
    press(&mut app, KeyCode::Char('a'));
    assert!(
        app.notices.show_advice && app.toast().is_none(),
        "toggles, no toast"
    );
    app.apply(Update::Status(fault(false)), Instant::now());
    assert!(app.notices.fault_cleared_at.is_some());
    assert!(say(&mut app).starts_with("All clear for under a minute"));
    assert!(!app.notices.show_advice);
}

#[test]
fn a_failures_step_shows_once_then_the_idle_toast_takes_over() {
    let failure = |what: &str| crate::recovery::Failure::of(crate::recovery::Kind::Dropped, what);
    let mut app = app_on(profile());
    app.notify_failure(&failure("first"));
    app.dismiss_toast();
    press(&mut app, KeyCode::Char('a'));
    assert!(
        app.notices.show_advice && app.toast().is_none(),
        "the step, no toast"
    );
    let one = say(&mut app);
    let two = say(&mut app);
    assert!(!app.notices.show_advice);
    let all = super::tips();
    assert!(one.ends_with(&format!("tip: {}", all[0])), "{one}");
    assert!(two.ends_with(&format!("tip: {}", all[1])), "{two}");
    app.notify_failure(&failure("second"));
    app.dismiss_toast();
    press(&mut app, KeyCode::Char('a'));
    assert!(
        app.notices.show_advice && app.toast().is_none(),
        "its own step again"
    );
}

#[test]
fn a_step_already_on_screen_counts_as_shown() {
    let failure = crate::recovery::Failure::of(crate::recovery::Kind::Dropped, "first");
    let mut app = app_on(with_overcurrent(profile()));
    let fault = |on: bool| vo("230.1", Some("fault_overcurrent"), json!(on), "x");
    app.apply(Update::Status(fault(true)), Instant::now());
    app.notify_failure(&failure);
    app.dismiss_toast();
    press(&mut app, KeyCode::Char('a'));
    assert!(
        app.notices.show_advice && app.toast().is_none(),
        "the fault's advice"
    );
    app.apply(Update::Status(fault(false)), Instant::now());
    // The failure's step is what the Status pane shows now; the next press hides it and says
    // the idle line at once.
    assert!(say(&mut app).starts_with("All clear for under a minute"));
    assert!(!app.notices.show_advice);
    // The step has been seen: the next press is another idle line.
    assert!(say(&mut app).starts_with("All clear for under a minute"));
    assert!(!app.notices.show_advice);
}

/// An app whose mascot may be seen on a day that is not quiet.
fn mascot_app() -> App {
    let mut app = app_on(profile());
    app.mascot.capable = true;
    app.mascot.today = Some((3, 3));
    app
}

#[test]
fn the_tenth_press_with_the_mascot_on_says_its_quip_and_they_rotate() {
    let mut app = mascot_app();
    let said: Vec<String> = (1..=10).map(|_| say(&mut app)).collect();
    assert!(
        said[9].ends_with("Ratatoskr ran the whole tree. Nothing to report."),
        "{}",
        said[9]
    );
    for (press, quip) in [
        (
            20,
            "Ratatoskr says the eagle and the serpent are bickering again. Your gear is fine.",
        ),
        (30, "Ratatoskr checked every branch. All clear."),
        (40, "Ratatoskr found an acorn in the EQ and left it there."),
        (50, "Ratatoskr ran the whole tree. Nothing to report."),
    ] {
        let line = idle_line(&app, app.notices.opened_at, press);
        assert!(line.ends_with(quip), "press {press}: {line}");
    }
    assert!(
        idle_line(&app, app.notices.opened_at, 5).ends_with("Bits are flowing. Nothing to fix."),
        "other multiples of five keep the family quip"
    );
}

#[test]
fn a_quiet_day_or_the_mascot_off_keeps_the_family_quip_on_the_tenth() {
    let family = "Bits are flowing. Nothing to fix.";
    let mut quiet = mascot_app();
    quiet.mascot.today = Some((9, 30));
    let mut off = mascot_app();
    off.tui.effects.mascot = false;
    for app in [&quiet, &off] {
        let line = idle_line(app, app.notices.opened_at, 10);
        assert!(line.ends_with(family), "{line}");
    }
}
