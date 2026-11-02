// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::KeyCode;
use svolder_host::eq::{HostEqState, Manager, Route};
use svolder_profiles::EqMode;

use crate::commands::host_eq::host_correction;
use crate::recovery::Kind;
use crate::tui::boxes::presets::{Done, done};
use crate::tui::test_support::presets::{band_params, carried_out, focus, imported, resting, step};
use crate::tui::test_support::press_from as press;

use super::*;

/// What the computer EQ's state file holds after `name` was applied.
fn state(name: &str) -> HostEqState {
    HostEqState {
        on: true,
        name: name.to_string(),
        origin: None,
        target: "alsa_output.test".into(),
        route: Route::Default,
        manager: Manager::Process,
        previous_default: None,
        preamp_db: -3.0,
        filters: Vec::new(),
    }
}

/// A Gungnir 2 flat in EQ mode with the imported preset `Phones` in the file, the list open.
fn listing() -> App {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.saved = vec![imported("Phones")];
    focus(&mut app, "Phones");
    app
}

/// `h` applies the focused preset in front of the computer's output, with the correction the
/// computer takes, and the answer puts it on the computer; the row says so afterwards.
#[test]
fn h_applies_the_focused_preset_on_the_computer() {
    let mut app = listing();
    let intents = press(&mut app, KeyCode::Char('h'));
    let [
        Intent::HostApply {
            name,
            origin,
            correction,
        },
    ] = intents.as_slice()
    else {
        panic!("expected one apply, got {intents:?}");
    };
    assert_eq!(name, "Phones");
    assert_eq!(origin, &None);
    assert_eq!(correction, &host_correction(&app.presets.saved[0]).unwrap());
    assert!(app.presets.computer_busy);
    assert!(
        press(&mut app, KeyCode::Char('h')).is_empty(),
        "a second apply started"
    );
    assert!(
        app.toast()
            .is_some_and(|t| t.text.contains("still changing"))
    );

    assert!(
        done(
            &mut app,
            Done::Applied("Phones".into(), -3.0, Ok(state("Phones")))
        )
        .is_empty()
    );
    assert!(!app.presets.computer_busy);
    assert_eq!(app.presets.computer.as_deref(), Some("Phones"));
    assert_eq!(
        app.toast().map(|t| t.text.clone()),
        Some("Phones is on the computer".to_string())
    );
    assert_eq!(step(&app), Some(&Step::Choosing), "the box closed");
}

/// `h` on the preset that is on the computer takes it off.
#[test]
fn h_removes_the_preset_that_is_on_the_computer() {
    let mut app = listing();
    app.presets.computer = Some("phones".into());
    assert_eq!(
        press(&mut app, KeyCode::Char('h')),
        [Intent::HostRemove { then_load: None }]
    );
    done(
        &mut app,
        Done::Removed {
            then_load: None,
            result: Ok(()),
        },
    );
    assert_eq!(app.presets.computer, None);
    assert_eq!(
        app.toast().map(|t| t.text.clone()),
        Some("computer EQ removed".to_string())
    );
}

/// Where the computer EQ cannot run, and on a built-in, `h` warns with the reason and asks for
/// nothing; a failed apply is the failure's own words.
#[test]
fn h_warns_where_the_computer_eq_cannot_run_or_the_preset_is_built_in() {
    let mut app = listing();
    app.presets.computer_off = Some("host EQ is off: SVOLDER_HOST=off".into());
    assert!(press(&mut app, KeyCode::Char('h')).is_empty());
    assert_eq!(
        app.toast().map(|t| t.text.clone()),
        Some("host EQ is off: SVOLDER_HOST=off".to_string())
    );
    assert!(!app.presets.computer_busy);

    app.presets.computer_off = None;
    focus(&mut app, "Flat");
    assert!(press(&mut app, KeyCode::Char('h')).is_empty());
    let said = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(said.starts_with("Flat is built in"), "{said}");

    focus(&mut app, "Phones");
    press(&mut app, KeyCode::Char('h'));
    let failure = Failure::of(Kind::Usage, "PipeWire is not installed".to_string());
    done(&mut app, Done::Applied("Phones".into(), 0.0, Err(failure)));
    assert!(!app.presets.computer_busy);
    assert_eq!(app.presets.computer, None);
    assert_eq!(
        app.toast().map(|t| t.text.clone()),
        Some("PipeWire is not installed".to_string())
    );
}

/// The preset on the DAC asks before it goes to the computer; `n`, `Enter` and `Esc` change
/// nothing, and `y` flattens the DAC and then applies.
#[test]
fn the_preset_on_the_dac_asks_before_it_moves_to_the_computer() {
    for answer in [KeyCode::Char('n'), KeyCode::Enter, KeyCode::Esc] {
        let mut app = listing();
        let loaded = press(&mut app, KeyCode::Enter);
        carried_out(&mut app, &loaded);
        focus(&mut app, "Phones");
        assert!(press(&mut app, KeyCode::Char('h')).is_empty());
        assert_eq!(
            step(&app),
            Some(&Step::Asking(Question::MoveToComputer("Phones".into())))
        );
        assert!(
            press(&mut app, answer).is_empty(),
            "{answer:?} asked for something"
        );
        assert_eq!(step(&app), Some(&Step::Choosing));
        assert!(!app.presets.computer_busy);
    }

    let mut app = listing();
    let loaded = press(&mut app, KeyCode::Enter);
    carried_out(&mut app, &loaded);
    focus(&mut app, "Phones");
    press(&mut app, KeyCode::Char('h'));
    let intents = press(&mut app, KeyCode::Char('y'));
    let (flatten, apply) = intents.split_at(intents.len() - 1);
    assert!(
        !band_params(flatten).is_empty(),
        "the DAC was not flattened: {intents:?}"
    );
    assert!(
        flatten
            .iter()
            .all(|i| matches!(i, Intent::EqSet { param, .. } if ["g", "f", "q"].contains(&param.as_str()))),
        "{flatten:?}"
    );
    assert!(matches!(apply, [Intent::HostApply { name, .. }] if name == "Phones"));
    // The DAC is left on Flat's values: every band centred at 0 dB, so nothing more to flatten.
    carried_out(&mut app, flatten);
    assert!(app.flatten().is_empty(), "the DAC is not centred");
}

/// The preset on the computer asks before it goes to the DAC; `y` takes it off the computer
/// and, once that is done, loads it; a failed removal loads nothing.
#[test]
fn the_preset_on_the_computer_asks_before_it_moves_to_the_dac() {
    for answer in [KeyCode::Char('n'), KeyCode::Enter, KeyCode::Esc] {
        let mut app = listing();
        app.presets.computer = Some("Phones".into());
        assert!(press(&mut app, KeyCode::Enter).is_empty());
        assert_eq!(
            step(&app),
            Some(&Step::Asking(Question::MoveToDac("Phones".into())))
        );
        assert!(press(&mut app, answer).is_empty());
        assert_eq!(step(&app), Some(&Step::Choosing));
        assert_eq!(
            app.presets.computer.as_deref(),
            Some("Phones"),
            "{answer:?} changed the computer"
        );
        assert!(!app.presets.computer_busy);
    }

    let mut app = listing();
    app.presets.computer = Some("Phones".into());
    press(&mut app, KeyCode::Enter);
    let intents = press(&mut app, KeyCode::Char('y'));
    assert_eq!(
        intents,
        [Intent::HostRemove {
            then_load: Some("Phones".into())
        }]
    );
    assert!(
        app.presets.loaded_preset.is_none(),
        "it was loaded before the computer let go"
    );

    let intents = done(
        &mut app,
        Done::Removed {
            then_load: Some("Phones".into()),
            result: Ok(()),
        },
    );
    assert!(
        !band_params(&intents).is_empty(),
        "the preset was not loaded: {intents:?}"
    );
    assert_eq!(app.presets.computer, None);
    assert_eq!(app.presets.loaded_preset.as_deref(), Some("Phones"));

    let mut app = listing();
    app.presets.computer = Some("Phones".into());
    let failure = Failure::of(Kind::Usage, "the unit would not stop".to_string());
    let intents = done(
        &mut app,
        Done::Removed {
            then_load: Some("Phones".into()),
            result: Err(failure),
        },
    );
    assert!(intents.is_empty());
    assert_eq!(app.presets.computer.as_deref(), Some("Phones"));
    assert_eq!(
        app.toast().map(|t| t.text.clone()),
        Some("the unit would not stop".to_string())
    );
}

/// An app on the device screen with the Host group shown and its cursor on the Host EQ row.
fn on_the_host_eq_row() -> App {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.preset_list = None;
    app.screen = crate::tui::app::Screen::Device;
    app.host.linked = true;
    app.tui.host_audio = true;
    let at = app.rows().len()
        + crate::tui::host::ROWS
            .iter()
            .position(|row| *row == crate::tui::host::HostRow::HostEq)
            .expect("the Host EQ row");
    app.focus(crate::tui::app::Pane::Controls, at);
    app
}

/// `Enter` and `Space` on the Host EQ row take the computer EQ off while it is on.
#[test]
fn enter_and_space_on_the_host_eq_row_remove_it_while_on() {
    for key in [KeyCode::Enter, KeyCode::Char(' ')] {
        let mut app = on_the_host_eq_row();
        app.take_host_eq(Some(state("Phones")));
        let intents = press(&mut app, key);
        assert!(
            matches!(intents.as_slice(), [Intent::HostRemove { then_load: None }]),
            "{key:?}: {intents:?}"
        );
        assert!(app.presets.computer_busy);
    }
}

/// With it off, the last one goes back on, built from the filters its state kept: the name, the
/// origin and the correction, with no preset in the list to take them from.
#[test]
fn the_host_eq_row_puts_the_last_one_back_from_its_state() {
    use svolder_profiles::tone::apo::{FilterKind, SourceFilter};
    let mut app = on_the_host_eq_row();
    let mut last = state("Phones");
    last.on = false;
    last.origin = Some("oratory1990".into());
    last.filters = vec![SourceFilter {
        kind: FilterKind::Peaking,
        f: 3000.0,
        g: -3.0,
        q: 2.0,
    }];
    app.take_host_eq(Some(last.clone()));
    assert_eq!(
        app.presets.computer, None,
        "off, so no preset is on the computer"
    );
    let intents = press(&mut app, KeyCode::Enter);
    let [
        Intent::HostApply {
            name,
            origin,
            correction,
        },
    ] = intents.as_slice()
    else {
        panic!("expected one apply, got {intents:?}");
    };
    assert_eq!(name, "Phones");
    assert_eq!(origin.as_deref(), Some("oratory1990"));
    assert_eq!(correction, &last.correction());
    assert!(app.presets.computer_busy);
}

/// With none to put back the row hints how to make one, and asks for nothing.
#[test]
fn the_host_eq_row_hints_when_there_is_none() {
    let mut app = on_the_host_eq_row();
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        app.toast().map(|toast| toast.text.clone()),
        Some("p then h on a preset".to_string())
    );
    assert!(!app.presets.computer_busy);
}

/// A computer where the EQ cannot run says why, and one still changing says so, each without
/// asking for anything.
#[test]
fn the_host_eq_row_says_why_it_cannot() {
    let mut app = on_the_host_eq_row();
    app.take_host_eq(Some(state("Phones")));
    app.presets.computer_off = Some("host EQ is off: SVOLDER_HOST=off".into());
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        app.toast().map(|toast| toast.text.clone()),
        Some("host EQ is off: SVOLDER_HOST=off".to_string())
    );
    app.presets.computer_off = None;
    app.presets.computer_busy = true;
    app.dismiss_toast();
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        app.toast().map(|toast| toast.text.clone()),
        Some(BUSY.to_string())
    );
}

/// The row reads `NAME · on` while the computer EQ is, and `off` otherwise.
#[test]
fn the_host_eq_row_reads_the_name_and_on_or_off() {
    let row = |app: &App| -> String {
        crate::tui::test_support::text(&crate::tui::test_support::frame(app, 80, 24))
            .into_iter()
            .find(|row| row.contains("Host EQ"))
            .expect("the Host EQ row")
    };
    let mut app = on_the_host_eq_row();
    assert!(row(&app).contains("Host EQ      off"), "{}", row(&app));
    app.take_host_eq(Some(state("Phones")));
    assert!(
        row(&app).contains("Host EQ      Phones · on"),
        "{}",
        row(&app)
    );
    let mut last = state("Phones");
    last.on = false;
    app.take_host_eq(Some(last));
    assert!(row(&app).contains("Host EQ      off"), "{}", row(&app));
}

/// An apply's answer and a removal's keep the row in step without waiting for the file to be
/// read again.
#[test]
fn an_answer_updates_the_row_at_once() {
    let mut app = on_the_host_eq_row();
    applied(&mut app, "Phones", 0.0, Ok(state("Phones")));
    assert_eq!(
        app.presets
            .host_eq
            .as_ref()
            .map(|state| (state.on, state.name.as_str())),
        Some((true, "Phones"))
    );
    assert_eq!(app.presets.computer.as_deref(), Some("Phones"));
    removed(&mut app, None, Ok(()));
    assert_eq!(
        app.presets.host_eq.as_ref().map(|state| state.on),
        Some(false)
    );
    assert_eq!(app.presets.computer, None);
}

/// `h` on the preset marked `computer` removes it even when the host became unavailable.
#[test]
fn h_removes_the_preset_on_the_computer_even_when_the_host_is_gone() {
    let mut app = listing();
    app.presets.computer = Some("Phones".into());
    app.presets.computer_off = Some("host EQ is off: SVOLDER_HOST=off".into());
    assert_eq!(
        press(&mut app, KeyCode::Char('h')),
        [Intent::HostRemove { then_load: None }]
    );
}

/// A band preset the DAC keeps flat or in loudness has no curve for the computer.
#[test]
fn h_refuses_a_band_preset_with_no_curve_of_its_own() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let mut own = crate::tui::test_support::presets::own("Loud", 5, 1.0);
    own.mode = EqMode::Loudness;
    app.presets.saved = vec![own];
    focus(&mut app, "Loud");
    assert!(press(&mut app, KeyCode::Char('h')).is_empty());
    let said = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(said.contains("no curve"), "{said}");
    assert!(!app.presets.computer_busy);
}

/// An apply that lowers the preamp says so in its toast.
#[test]
fn a_lowered_preamp_is_said_in_the_toast() {
    let mut app = listing();
    let mut applied = state("Phones");
    applied.preamp_db = -6.4;
    done(&mut app, Done::Applied("Phones".into(), -2.0, Ok(applied)));
    let said = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(
        said.contains("preamp lowered from -2.0 dB to -6.4 dB so the correction never clips"),
        "{said}"
    );
}

/// A move to the computer whose apply fails says the DAC was flattened and how to put the
/// curve back.
#[test]
fn a_failed_move_to_the_computer_says_the_dac_was_flattened() {
    let mut app = listing();
    let loaded = press(&mut app, KeyCode::Enter);
    carried_out(&mut app, &loaded);
    focus(&mut app, "Phones");
    press(&mut app, KeyCode::Char('h'));
    press(&mut app, KeyCode::Char('y'));
    let failure = Failure::of(Kind::Usage, "PipeWire is not installed".to_string());
    done(&mut app, Done::Applied("Phones".into(), 0.0, Err(failure)));
    let advice = app.notices.advice.clone().unwrap_or_default();
    assert!(
        advice.contains("DAC was flattened") && advice.contains("svolder eq load Phones"),
        "{advice}"
    );
    // A plain failure says nothing of the kind.
    let mut plain = listing();
    press(&mut plain, KeyCode::Char('h'));
    let failure = Failure::of(Kind::Usage, "PipeWire is not installed".to_string());
    done(
        &mut plain,
        Done::Applied("Phones".into(), 0.0, Err(failure)),
    );
    assert!(
        !plain
            .notices
            .advice
            .clone()
            .unwrap_or_default()
            .contains("flattened")
    );
}
