// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::KeyCode;
use svolder_ipc::TuiConfig;

use crate::backend::DeviceRow;
use crate::tui::test_support::presets::hear_id;
use crate::tui::test_support::{press_from as press, profile_named};

use super::*;

/// The raw steps of the six bands of a flat Loki Max.
const FLAT: [i64; 6] = [15; 6];
/// What Treble Boost reads.
const TREBLE: [i64; 6] = [15, 15, 15, 17, 21, 28];
/// What Bass Boost reads.
const BASS: [i64; 6] = [28, 21, 17, 15, 15, 15];

/// A Loki Max whose six bands read `bands`, on the device screen.
fn loki(bands: [i64; 6]) -> App {
    let mut app = App::new(
        profile_named("lokimax"),
        "lokimax".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("lokimax")],
    );
    hear_bands(&mut app, bands);
    app
}

/// The function a band's reading is kept under.
fn band_id(app: &App, band: usize) -> FunctionId {
    app.profile
        .control_by_key(&format!("band{band}"))
        .expect("the Loki Max has six bands")
        .id
}

/// Report the six bands as `raw`.
fn hear_bands(app: &mut App, raw: [i64; 6]) {
    for (at, raw) in raw.into_iter().enumerate() {
        let id = band_id(app, at + 1);
        hear_id(app, id, raw);
    }
}

/// What the row the focus is on says.
fn focused(app: &App) -> String {
    let list = app.presets.device_list.as_ref().expect("the list is open");
    rows(&app.profile)[list.row].label(&app.profile)
}

/// Open the list and move the focus to the row called `label`.
fn focus(app: &mut App, label: &str) {
    if app.presets.device_list.is_none() {
        press(app, KeyCode::Char('p'));
    }
    for _ in 0..rows(&app.profile).len() {
        press(app, KeyCode::Up);
    }
    for _ in 0..rows(&app.profile).len() {
        if focused(app) == label {
            return;
        }
        press(app, KeyCode::Down);
    }
    panic!("the list has no row called {label}");
}

/// The toast showing, as text.
fn toast(app: &App) -> Option<&str> {
    app.toast().map(|toast| toast.text.as_str())
}

/// A raw write of a band, as the list sends it.
fn raw(band: usize, value: i64) -> Intent {
    Intent::Raw {
        id: format!("6.{band}"),
        value_type: "u8".into(),
        value: value.to_string(),
    }
}

/// `Enter` on a slot recalls it, closes the list and says so.
#[test]
fn enter_on_a_slot_recalls_it_and_says_so() {
    let mut app = loki(FLAT);
    focus(&mut app, "Preset 2");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Set {
            key: "recall".into(),
            value: "2".into(),
            step: 0
        }]
    );
    assert!(app.presets.device_list.is_none());
    assert_eq!(toast(&app), Some("Recalled preset 2"));
}

/// A built-in is written band by band: every band going down, then every band going up.
#[test]
fn enter_on_a_builtin_writes_its_bands_down_then_up() {
    let mut app = loki(TREBLE);
    focus(&mut app, "Bass Boost");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![
            raw(4, 15),
            raw(5, 15),
            raw(6, 15),
            raw(1, 28),
            raw(2, 21),
            raw(3, 17),
        ]
    );
    assert!(app.presets.device_list.is_none());
    assert_eq!(toast(&app), Some("Bass Boost on"));
}

/// A built-in the bands already read sends nothing, and the toast says that.
#[test]
fn enter_on_a_builtin_already_on_sends_nothing_and_says_so() {
    let mut app = loki(BASS);
    focus(&mut app, "Bass Boost");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.presets.device_list.is_none());
    assert_eq!(toast(&app), Some("Bass Boost is already on"));
}

/// Pressing a built-in a second time before the device has answered plans from what the first
/// press asked for, so it sends nothing again.
#[test]
fn a_builtin_pressed_twice_before_the_device_answers_is_not_sent_twice() {
    let mut app = loki(TREBLE);
    focus(&mut app, "Bass Boost");
    assert_eq!(press(&mut app, KeyCode::Enter).len(), 6);
    focus(&mut app, "Bass Boost");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(toast(&app), Some("Bass Boost is already on"));
}

/// After the TUI sends a recall every band is unread until it reports: a built-in that the old
/// readings say is already on is written whole, the cuts first.
#[test]
fn a_builtin_right_after_a_recall_writes_every_band_not_yet_reported() {
    let mut app = loki(BASS);
    focus(&mut app, "Preset 1");
    press(&mut app, KeyCode::Enter);
    focus(&mut app, "Bass Boost");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![
            raw(4, 15),
            raw(5, 15),
            raw(6, 15),
            raw(1, 28),
            raw(2, 21),
            raw(3, 17),
        ]
    );

    // The device then reports the three raised bands at what they read, and answers the writes:
    // those bands are known again, and the ones still unreported are written.
    let mut app = loki(BASS);
    focus(&mut app, "Preset 1");
    press(&mut app, KeyCode::Enter);
    for band in 1..=3 {
        let id = band_id(&app, band);
        hear_id(&mut app, id, BASS[band - 1]);
    }
    focus(&mut app, "Bass Boost");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![raw(4, 15), raw(5, 15), raw(6, 15)]
    );
}

/// A refresh reads every band afresh, so nothing is left unreported.
#[test]
fn a_refresh_after_a_recall_makes_the_bands_known_again() {
    let mut app = loki(BASS);
    focus(&mut app, "Preset 1");
    press(&mut app, KeyCode::Enter);
    let values = (1..=6)
        .map(|band| {
            let mut value = crate::tui::test_support::vo("0", None, serde_json::json!(0), "0");
            value.id = band_id(&app, band).to_string();
            value.raw = serde_json::json!(BASS[band - 1]);
            value
        })
        .collect();
    app.replace_all(values, std::time::Instant::now());
    focus(&mut app, "Bass Boost");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
}

/// `u` on a slot asks first; `y` then sends the save and says so.
#[test]
fn u_on_a_slot_asks_before_saving_and_y_saves() {
    let mut app = loki(FLAT);
    focus(&mut app, "Preset 2");
    assert!(press(&mut app, KeyCode::Char('u')).is_empty());
    assert_eq!(
        app.presets.device_list.as_ref().map(|list| &list.step),
        Some(&Step::Asking(Question::Save(2)))
    );
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        vec![Intent::Set {
            key: "save".into(),
            value: "2".into(),
            step: 0
        }]
    );
    assert_eq!(toast(&app), Some("Saved to preset 2"));
    assert_eq!(
        app.presets.device_list.as_ref().map(|list| &list.step),
        Some(&Step::Choosing),
        "the list stays up, as the other list does after an update"
    );
}

/// Anything but `y` leaves the slot alone: `n`, `Esc` and `Enter` go back to the list, and every
/// other key is swallowed with the question still up.
#[test]
fn u_then_n_on_a_slot_sends_nothing() {
    for answer in [KeyCode::Char('n'), KeyCode::Esc, KeyCode::Enter] {
        let mut app = loki(FLAT);
        focus(&mut app, "Preset 3");
        assert!(press(&mut app, KeyCode::Char('u')).is_empty());
        assert!(
            press(&mut app, answer).is_empty(),
            "{answer:?} sent a write"
        );
        assert_eq!(
            app.presets.device_list.as_ref().map(|list| &list.step),
            Some(&Step::Choosing),
            "{answer:?} did not go back to the list"
        );
        assert_ne!(toast(&app), Some("Saved to preset 3"));
    }
    let mut app = loki(FLAT);
    focus(&mut app, "Preset 3");
    press(&mut app, KeyCode::Char('u'));
    for other in [KeyCode::Char('x'), KeyCode::Char('u'), KeyCode::Down] {
        assert!(press(&mut app, other).is_empty());
        assert_eq!(
            app.presets.device_list.as_ref().map(|list| &list.step),
            Some(&Step::Asking(Question::Save(3))),
            "{other:?} answered the question"
        );
    }
}

/// A built-in is not a slot: `u` on it writes nothing and says what saves where.
#[test]
fn u_on_a_builtin_sends_nothing_and_says_why() {
    let mut app = loki(FLAT);
    focus(&mut app, "Bass Boost");
    assert!(press(&mut app, KeyCode::Char('u')).is_empty());
    assert_eq!(
        toast(&app),
        Some("Bass Boost is built in; u saves on a device slot")
    );
    assert_eq!(
        app.presets.device_list.as_ref().map(|list| &list.step),
        Some(&Step::Choosing)
    );
}

/// The device never says which slot it holds, so no slot is marked; a built-in is marked while
/// every band reads its steps, and none while a band is unread.
#[test]
fn the_list_marks_the_builtin_the_bands_match_and_no_slot() {
    let app = loki(BASS);
    let marked = on(&app).map(|at| Row::Setting(at).label(&app.profile));
    assert_eq!(marked.as_deref(), Some("Bass Boost"));

    let mut unread = App::new(
        profile_named("lokimax"),
        "lokimax".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("lokimax")],
    );
    for band in 1..=5 {
        let id = band_id(&unread, band);
        hear_id(&mut unread, id, 15);
    }
    assert_eq!(on(&unread), None, "the sixth band is unread");

    let mut custom = loki(BASS);
    let id = band_id(&custom, 6);
    hear_id(&mut custom, id, 3);
    assert_eq!(on(&custom), None, "no built-in has these bands");

    // The list opens on the one that is on.
    let mut opened = loki(BASS);
    press(&mut opened, KeyCode::Char('p'));
    assert_eq!(focused(&opened), "Bass Boost");

    // A recall sent from here leaves the old readings unreported: none is marked.
    focus(&mut opened, "Preset 1");
    press(&mut opened, KeyCode::Enter);
    assert_eq!(on(&opened), None);
}

/// The arrows pass over the `Built in` header.
#[test]
fn the_header_row_takes_no_focus() {
    let mut app = loki(FLAT);
    press(&mut app, KeyCode::Char('p'));
    assert_eq!(
        rows(&app.profile)
            .iter()
            .map(|row| row.label(&app.profile))
            .collect::<Vec<_>>(),
        [
            "Preset 1",
            "Preset 2",
            "Preset 3",
            "Built in",
            "Flat",
            "Bass Boost",
            "Treble Boost",
            "Smiley",
            "Loudness"
        ]
    );
    focus(&mut app, "Preset 3");
    press(&mut app, KeyCode::Down);
    assert_eq!(focused(&app), "Flat");
    press(&mut app, KeyCode::Up);
    assert_eq!(focused(&app), "Preset 3");
    for _ in 0..10 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(
        focused(&app),
        "Loudness",
        "and the list stops at its last row"
    );
    for _ in 0..10 {
        press(&mut app, KeyCode::Up);
    }
    assert_eq!(focused(&app), "Preset 1", "and at its first");
}

/// `Esc` closes the list and writes nothing; every other key the list has no use for is swallowed,
/// so it never reaches the screen under the box.
#[test]
fn esc_closes_the_list_and_other_keys_reach_nothing_under_it() {
    let mut app = loki(FLAT);
    press(&mut app, KeyCode::Char('p'));
    for key in [KeyCode::Char('0'), KeyCode::Char('+'), KeyCode::Char('m')] {
        assert!(press(&mut app, key).is_empty());
        assert!(app.presets.device_list.is_some());
    }
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert!(app.presets.device_list.is_none());
}

/// A device with neither a tone control nor presets of its own opens no list.
#[test]
fn p_on_a_device_with_no_presets_opens_nothing() {
    let mut app = App::new(
        profile_named("lyr5"),
        "lyr5".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("lyr5")],
    );
    assert!(press(&mut app, KeyCode::Char('p')).is_empty());
    assert!(app.presets.device_list.is_none() && app.presets.preset_list.is_none());
    assert!(!app.modal_open());
}

/// A recall the loop could not send leaves the device as it was: the bands read what they read,
/// so the built-in they match is marked and applying it sends nothing.
#[test]
fn a_recall_that_could_not_be_sent_leaves_the_bands_trusted() {
    let mut app = loki(BASS);
    focus(&mut app, "Preset 1");
    let intents = press(&mut app, KeyCode::Enter);
    assert_eq!(
        on(&app),
        None,
        "the recall was assumed to have moved the bands"
    );
    for intent in &intents {
        unsent(&mut app, intent);
    }
    assert_eq!(
        on(&app)
            .map(|at| Row::Setting(at).label(&app.profile))
            .as_deref(),
        Some("Bass Boost")
    );
    focus(&mut app, "Bass Boost");
    assert!(press(&mut app, KeyCode::Enter).is_empty());

    // Another intent that fails changes nothing.
    let mut app = loki(BASS);
    focus(&mut app, "Preset 1");
    press(&mut app, KeyCode::Enter);
    unsent(&mut app, &raw(1, 15));
    assert_eq!(on(&app), None);
}

/// A recall sent after a built-in was asked for drops what the built-in asked of the bands, so a
/// built-in applied next is planned from the bands as unread, not from the older asked-for steps.
#[test]
fn a_recall_drops_the_steps_an_earlier_built_in_asked_for() {
    let mut app = loki(TREBLE);
    focus(&mut app, "Bass Boost");
    assert_eq!(press(&mut app, KeyCode::Enter).len(), 6);
    focus(&mut app, "Preset 2");
    press(&mut app, KeyCode::Enter);
    assert!(
        app.readings.pending.is_empty(),
        "the asked-for steps are dropped"
    );
    focus(&mut app, "Bass Boost");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![
            raw(4, 15),
            raw(5, 15),
            raw(6, 15),
            raw(1, 28),
            raw(2, 21),
            raw(3, 17),
        ]
    );
}
