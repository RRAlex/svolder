// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::KeyCode;
use svolder_profiles::EqMode;
use svolder_profiles::tone::preset::host_correction;

use crate::tui::boxes::presets::PresetList;
use crate::tui::host::FromDaemon;
use crate::tui::test_support::presets::{band_params, carried_out, focus, imported, resting, step};
use crate::tui::test_support::{computer_eq, press_from as press};

use super::*;

/// A Gungnir 2 flat in EQ mode with the imported preset `Phones` in the file, the list open.
fn listing() -> App {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.saved = vec![imported("Phones")];
    focus(&mut app, "Phones");
    app
}

/// [`listing`] with the computer's controls there, its EQ off.
fn offered() -> App {
    let mut app = listing();
    app.host.computer.eq = Some(computer_eq(false, &[("PK", 1000.0, 0.0, 1.0)]));
    app
}

/// [`offered`] on the computer's side, the list still open.
fn on_the_computer() -> App {
    let mut app = offered();
    app.flip_side();
    assert!(app.on_computer());
    app
}

/// `Phones` as the computer's EQ holds it, on.
fn phones_on_the_computer(app: &mut App) {
    let mut eq = computer_eq(true, &[("PK", 1000.0, 2.0, 1.0)]);
    eq.preset = Some("Phones".into());
    app.host.computer.eq = Some(eq);
}

/// `h` is not a key of the list: the computer is the side switch's, and a preset goes there with
/// `Enter` on that side.
#[test]
fn h_does_nothing_in_the_preset_list() {
    let mut app = offered();
    let before = app.snapshot_for_test();
    assert!(press(&mut app, KeyCode::Char('h')).is_empty());
    assert!(app.snapshot_for_test() == before, "`h` moved something");
    assert_eq!(step(&app), Some(&Step::Choosing));
}

/// On the computer's side `Enter` puts the focused preset on the computer with the correction the
/// computer takes, closes the list, and leaves the DAC alone.
#[test]
fn enter_on_the_computer_side_recalls_on_the_computer() {
    let mut app = on_the_computer();
    let intents = press(&mut app, KeyCode::Enter);
    let preset = imported("Phones");
    assert_eq!(
        intents,
        [Intent::eq(HostEqOp::Apply {
            name: "Phones".into(),
            origin: preset
                .source
                .as_ref()
                .and_then(|source| source.origin.clone()),
            correction: host_correction(&preset).expect("an import has a correction"),
            sink: None,
        })]
    );
    assert!(app.presets.preset_list.is_none(), "the list closed");
    assert!(
        band_params(&intents).is_empty(),
        "nothing was written to the DAC"
    );
}

/// A computer where the controls cannot be used says why and sends nothing.
#[test]
fn a_computer_that_cannot_be_used_says_why() {
    let mut app = on_the_computer();
    app.host.computer.eq.as_mut().expect("an eq").unavailable =
        Some("computer controls unavailable: no pw-cli on this computer".into());
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        app.toast().map(|t| t.text.clone()),
        Some("computer controls unavailable: no pw-cli on this computer".to_string())
    );
}

/// The preset on the DAC asks before it goes to the computer; `n`, `Enter` and `Esc` change
/// nothing, and `y` flattens the DAC and then applies.
#[test]
fn the_same_preset_on_the_dac_asks_then_confirms() {
    for answer in [KeyCode::Char('n'), KeyCode::Enter, KeyCode::Esc] {
        let mut app = offered();
        let loaded = press(&mut app, KeyCode::Enter);
        carried_out(&mut app, &loaded);
        app.flip_side();
        app.presets.preset_list = Some(PresetList {
            row: 0,
            step: Step::Choosing,
        });
        focus(&mut app, "Phones");
        assert!(press(&mut app, KeyCode::Enter).is_empty());
        assert_eq!(
            step(&app),
            Some(&Step::Asking(Question::MoveToComputer("Phones".into())))
        );
        assert!(
            press(&mut app, answer).is_empty(),
            "{answer:?} asked for something"
        );
        assert_eq!(step(&app), Some(&Step::Choosing));
    }

    let mut app = offered();
    let loaded = press(&mut app, KeyCode::Enter);
    carried_out(&mut app, &loaded);
    app.flip_side();
    app.presets.preset_list = Some(PresetList {
        row: 0,
        step: Step::Choosing,
    });
    focus(&mut app, "Phones");
    press(&mut app, KeyCode::Enter);
    let intents = press(&mut app, KeyCode::Char('y'));
    let (flatten, apply) = intents.split_at(intents.len() - 1);
    assert!(
        !band_params(flatten).is_empty(),
        "the DAC was not flattened: {intents:?}"
    );
    assert!(matches!(
        apply,
        [Intent::HostEq(op)] if matches!(&**op, HostEqOp::Apply { name, .. } if name == "Phones")
    ));
    // The DAC is left on Flat's values: every band centred at 0 dB, so nothing more to flatten.
    carried_out(&mut app, flatten);
    assert!(app.flatten().is_empty(), "the DAC is not centred");
}

/// Joined to a daemon, the recall is a request by the preset's name; the daemon's question opens
/// the list on it, and `y` repeats the request with the yes, `n` sends nothing.
#[test]
fn a_joined_recall_goes_by_name_and_the_daemons_question_is_asked_here() {
    let mut app = on_the_computer();
    app.host.daemon = Some(FromDaemon { route: None });
    let intents = press(&mut app, KeyCode::Enter);
    let device = app.session.device_name.clone();
    assert_eq!(
        intents,
        [Intent::eq(HostEqOp::Preset {
            name: "Phones".into(),
            device: Some(device.clone()),
            confirm: false,
        })]
    );
    assert!(app.presets.preset_list.is_none());

    // The daemon answers that a device plays it too.
    asked(&mut app, "Phones");
    assert_eq!(
        step(&app),
        Some(&Step::Asking(Question::MoveToComputer("Phones".into())))
    );
    assert!(press(&mut app, KeyCode::Char('n')).is_empty());
    asked(&mut app, "Phones");
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        [Intent::eq(HostEqOp::Preset {
            name: "Phones".into(),
            device: Some(device),
            confirm: true,
        })]
    );
    assert!(app.presets.preset_list.is_none());
}

/// The preset on the computer asks before it goes to the DAC; `y` turns the computer EQ off and
/// loads it on the DAC.
#[test]
fn the_preset_on_the_computer_asks_before_it_moves_to_the_dac() {
    for answer in [KeyCode::Char('n'), KeyCode::Enter, KeyCode::Esc] {
        let mut app = offered();
        phones_on_the_computer(&mut app);
        app.presets.preset_list = Some(PresetList {
            row: 0,
            step: Step::Choosing,
        });
        focus(&mut app, "Phones");
        assert!(press(&mut app, KeyCode::Enter).is_empty());
        assert_eq!(
            step(&app),
            Some(&Step::Asking(Question::MoveToDac("Phones".into())))
        );
        assert!(press(&mut app, answer).is_empty());
        assert_eq!(step(&app), Some(&Step::Choosing));
        assert!(app.computer_preset().is_some(), "{answer:?} changed it");
    }

    let mut app = offered();
    phones_on_the_computer(&mut app);
    app.presets.preset_list = Some(PresetList {
        row: 0,
        step: Step::Choosing,
    });
    focus(&mut app, "Phones");
    press(&mut app, KeyCode::Enter);
    let intents = press(&mut app, KeyCode::Char('y'));
    assert_eq!(intents, [Intent::MoveToDac("Phones".into())]);
    assert!(
        app.presets.loaded_preset.is_none(),
        "nothing is loaded before the computer EQ is off"
    );
    // Once the computer says it is off, the DAC's load goes out.
    let loads = moved(&mut app, "Phones");
    assert!(!band_params(&loads).is_empty(), "{loads:?}");
    assert_eq!(app.presets.loaded_preset.as_deref(), Some("Phones"));
}

/// More than 16 filters are refused with the count, on the computer's side and by the move from
/// the DAC, and the DAC is not touched.
#[test]
fn seventeen_filters_are_refused_by_name_before_the_dac_is_touched() {
    use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
    let mut app = on_the_computer();
    let filter = SourceFilter {
        kind: FilterKind::Peaking,
        f: 1000.0,
        g: 1.0,
        q: 1.0,
    };
    app.presets.saved = vec![preset::from_computer(
        "Long",
        Correction {
            preamp_db: 0.0,
            filters: vec![filter; 17],
        },
    )];
    focus(&mut app, "Long");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        app.toast().map(|t| t.text.clone()),
        Some("17 filters; the computer EQ holds 16".to_string())
    );
    assert!(move_to_computer(&mut app, "Long").is_empty());
    assert_eq!(app.presets.computer_flattened, None);
}

/// Flat changes nothing, so it is put on the computer with no question while the DAC is flat.
#[test]
fn flat_is_put_on_the_computer_with_no_question_while_the_dac_is_flat() {
    let mut app = on_the_computer();
    focus(&mut app, "Flat");
    let intents = press(&mut app, KeyCode::Enter);
    assert!(
        matches!(&intents[..], [Intent::HostEq(op)] if matches!(&**op, HostEqOp::Apply { name, .. } if name == "Flat")),
        "{intents:?}"
    );
}

/// Flat on the computer changes nothing, so Enter loads it on the DAC with no question; a real
/// preset on both asks (the test above).
#[test]
fn flat_on_the_computer_goes_to_the_dac_with_no_question() {
    let mut app = offered();
    let mut eq = computer_eq(true, &[("PK", 1000.0, 0.0, 1.0)]);
    eq.preset = Some("Flat".into());
    app.host.computer.eq = Some(eq);
    focus(&mut app, "Flat");
    press(&mut app, KeyCode::Enter);
    assert_ne!(
        step(&app),
        Some(&Step::Asking(Question::MoveToDac("Flat".into())))
    );
}

/// On the computer's side `n` saves the computer's points, never the DAC's curve: with no daemon as
/// a preset built from them, joined as the daemon's `SaveAs`; `u` is inert there with a hint.
#[test]
fn n_on_the_computer_saves_its_points_and_u_does_nothing() {
    let mut app = on_the_computer();
    let mut eq = computer_eq(true, &[("LSC", 100.0, 2.0, 0.7), ("PK", 1000.0, -3.0, 1.4)]);
    eq.preamp_db = -3.0;
    app.host.computer.eq = Some(eq);
    press(&mut app, KeyCode::Char('n'));
    for _ in 0..12 {
        press(&mut app, KeyCode::Backspace);
    }
    for letter in "Mine".chars() {
        press(&mut app, KeyCode::Char(letter));
    }
    let intents = press(&mut app, KeyCode::Enter);
    let [Intent::SavePreset(saved)] = intents.as_slice() else {
        panic!("a preset of the computer's points was expected: {intents:?}");
    };
    assert_eq!(saved.name, "Mine");
    let correction = host_correction(saved).expect("it plays on the computer");
    assert_eq!(correction.filters.len(), 2);
    assert_eq!(correction.filters[1].g, -3.0);
    assert!(
        band_params(&intents).is_empty(),
        "the DAC's curve was not saved"
    );

    // `u` writes nothing and says where the save is.
    let mut app = on_the_computer();
    focus(&mut app, "Phones");
    app.presets.saved = vec![preset::from_computer(
        "Mine",
        host_correction(&imported("Phones")).unwrap(),
    )];
    focus(&mut app, "Mine");
    assert!(press(&mut app, KeyCode::Char('u')).is_empty());
    assert_eq!(step(&app), Some(&Step::Choosing));
    assert!(app.toast().is_some());

    // Joined, the daemon saves.
    let mut app = on_the_computer();
    app.host.daemon = Some(FromDaemon { route: None });
    press(&mut app, KeyCode::Char('n'));
    for _ in 0..12 {
        press(&mut app, KeyCode::Backspace);
    }
    for letter in "Mine".chars() {
        press(&mut app, KeyCode::Char(letter));
    }
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        [Intent::eq(HostEqOp::SaveAs {
            name: "Mine".into()
        })]
    );
}

/// The DAC was flattened for a move: the advice goes once the computer shows the preset, and with
/// any other computer change, so a later refusal does not carry it.
#[test]
fn the_flattened_advice_ends_with_the_move() {
    let mut app = offered();
    app.presets.computer_flattened = Some("Phones".into());
    phones_on_the_computer(&mut app);
    app.note_computer_view();
    assert_eq!(app.presets.computer_flattened, None);

    let mut other = offered();
    other.presets.computer_flattened = Some("Phones".into());
    other.note_computer_view();
    assert_eq!(
        other.presets.computer_flattened.as_deref(),
        Some("Phones"),
        "not yet on the computer"
    );
}

/// On a device with no tone control of its own (a Loki Max) `p` on the computer's side opens the
/// shared preset list, never the device's own, and nothing in it writes to the device.
#[test]
fn p_on_the_computer_side_of_a_loki_max_opens_the_shared_list() {
    use crate::tui::app::Screen;
    let mut app = crate::tui::test_support::app_showing_on("lokimax", Screen::Device);
    assert!(app.profile.eq.is_none());
    app.host.computer.eq = Some(computer_eq(false, &[("PK", 1000.0, 0.0, 1.0)]));
    app.presets.saved = vec![imported("Phones")];
    // On the device's side `p` is the device's own list.
    press(&mut app, KeyCode::Char('p'));
    assert!(app.presets.device_list.is_some() && app.presets.preset_list.is_none());
    press(&mut app, KeyCode::Esc);
    app.flip_side();
    assert!(app.on_computer());
    press(&mut app, KeyCode::Char('p'));
    assert!(
        app.presets.device_list.is_none(),
        "the device's list stays shut"
    );
    assert!(app.presets.preset_list.is_some(), "the shared list is open");
    let spec = app.preset_spec().expect("the shared list's spec");
    let row = entries(&app, &spec)
        .iter()
        .position(|entry| entry.preset.name == "Phones")
        .expect("Phones is listed");
    app.presets.preset_list = Some(PresetList {
        row,
        step: Step::Choosing,
    });
    let intents = press(&mut app, KeyCode::Enter);
    assert!(
        matches!(&intents[..], [Intent::HostEq(op)] if matches!(&**op, HostEqOp::Apply { name, .. } if name == "Phones")),
        "{intents:?}"
    );
    // The keys that would change the device are inert or the computer's.
    let mut again = crate::tui::test_support::app_showing_on("lokimax", Screen::Device);
    again.host.computer.eq = app.host.computer.eq.clone();
    again.presets.saved = vec![imported("Phones")];
    again.flip_side();
    press(&mut again, KeyCode::Char('p'));
    for key in ['n', 'u'] {
        let sent = press(&mut again, KeyCode::Char(key));
        assert!(
            sent.iter().all(|i| !matches!(
                i,
                Intent::Set { .. } | Intent::Raw { .. } | Intent::EqSet { .. } | Intent::EqMode(_)
            )),
            "{key}: {sent:?}"
        );
    }
}
