// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use svolder_ipc::TuiConfig;
use svolder_profiles::{EqMode, EqParam};

use crate::recovery::{Failure, Kind};
use crate::tui::app::Screen;
use crate::tui::keys;
use crate::tui::test_support::presets::{
    band_params, carried_out, eq_of, focus, hear_flat, hear_id, imported, names, own, resting, step,
};
use crate::tui::test_support::{press_from as press, profile_named, type_text};

use super::*;

/// The list offers the built-ins first, then the file's own that have the tone control's
/// band count, in the file's order: on a five-band tone control and a three-band one.
#[test]
fn the_list_offers_the_builtins_then_the_files_own_of_this_band_count() {
    let library = vec![
        own("HD650 warm", 5, 1.0),
        own("Mimir bright", 3, 1.0),
        own("Late night", 5, -2.0),
    ];
    let mut five = resting("gungnir2-v2", EqMode::Eq);
    five.presets.saved = library.clone();
    assert_eq!(
        names(&five),
        [
            "Flat",
            "Warm",
            "Bass",
            "Presence",
            "HD650 warm",
            "Late night"
        ]
    );
    let builtin: Vec<bool> = entries(&five, &eq_of(&five))
        .iter()
        .map(|entry| entry.builtin)
        .collect();
    assert_eq!(builtin, [true, true, true, true, false, false]);

    let mut three = resting("mimir", EqMode::Eq);
    three.presets.saved = library;
    assert_eq!(
        names(&three),
        ["Flat", "Warm", "Bass", "Presence", "Mimir bright"]
    );
    // The three-band built-ins are this tone control's own, on its own centres.
    let flat = &entries(&three, &eq_of(&three))[0].preset;
    assert_eq!((flat.bands, flat.band.len()), (3, 3));
}

/// `p` opens the list from the tone-control screen and from the device screen, on the
/// preset that is on; on a device with no tone control it opens nothing, and the key lines
/// list it dimmed.
#[test]
fn p_opens_the_list_on_the_preset_that_is_on() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    press(&mut app, KeyCode::Char('p'));
    assert_eq!(
        app.presets.preset_list,
        Some(PresetList {
            row: 0,
            step: Step::Choosing
        }),
        "the device reads flat in EQ mode, which is the Flat preset"
    );
    assert!(app.modal_open());

    let mut device = resting("gungnir2-v2", EqMode::Eq);
    device.presets.saved = vec![own("HD650 warm", 5, 1.0)];
    device.show(Screen::Device);
    let eq = eq_of(&device);
    // The device now carries the user's preset, so the list opens on its row.
    let plan = preset::plan_recall(&eq, |id| device.raw_of(id), &device.presets.saved[0])
        .expect("the preset fits");
    for write in plan.writes {
        let id = write.id(&eq).expect("a function of the tone control");
        hear_id(&mut device, id, i64::from(write.raw()));
    }
    press(&mut device, KeyCode::Char('p'));
    assert_eq!(
        device.presets.preset_list.as_ref().map(|list| list.row),
        Some(4)
    );
    assert_eq!(device.screen, Screen::Device);

    let mut lyr = App::new(
        profile_named("lyr5"),
        "lyr".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    assert!(press(&mut lyr, KeyCode::Char('p')).is_empty());
    assert!(
        lyr.presets.preset_list.is_none(),
        "a device with no tone control opened a list"
    );
    let scene = keys::Scene::of(&lyr);
    for screen in [Screen::Device, Screen::Tone] {
        let p = keys::hints_for(screen)
            .into_iter()
            .find(|hint| hint.label == "p")
            .unwrap_or_else(|| panic!("{screen:?} lists no p"));
        assert!(!keys::available_in(p, scene), "{screen:?}: p is not dimmed");
    }
    let zero = keys::hints_for(Screen::Tone)
        .into_iter()
        .find(|hint| hint.label == "0")
        .expect("the tone screen lists 0");
    assert!(
        !keys::available_in(zero, scene),
        "the tone screen's 0 is not dimmed"
    );
}

/// `Enter` loads the focused preset and closes the list: the recall's writes as intents in
/// the plan's order — the Q changes, then the gains going up, then the mode, last because
/// the preset is in EQ mode — each value the one its raw step stands for, and nothing for a
/// parameter the device already reads.
#[test]
fn enter_loads_the_focused_preset_in_the_plans_order_and_closes_the_list() {
    let mut app = resting("gungnir2-v2", EqMode::Flat);
    let eq = eq_of(&app);
    focus(&mut app, "Bass");
    let intents = press(&mut app, KeyCode::Enter);

    assert_eq!(
        band_params(&intents),
        [(1, "q"), (2, "q"), (1, "g"), (2, "g")],
        "{intents:?}"
    );
    assert_eq!(
        intents.last(),
        Some(&Intent::EqMode("eq".into())),
        "the mode is not last"
    );
    assert_eq!(intents.len(), 6, "{intents:?}");
    for intent in &intents {
        if let Intent::EqSet { band, param, value } = intent {
            let (asked, got) = match param.as_str() {
                "q" => (eq.q_raw(0.7), eq.q_raw(*value)),
                _ => {
                    let db = if *band == 1 { 4.0 } else { 2.0 };
                    (eq.gain_raw(db), eq.gain_raw(*value))
                }
            };
            assert_eq!(got, asked, "band {band} {param} is not the preset's step");
        }
    }
    assert!(app.presets.preset_list.is_none(), "the list stayed open");
    assert_eq!(app.presets.loaded_preset.as_deref(), Some("Bass"));
    let loading = app
        .readings
        .loading
        .as_ref()
        .expect("the header says it is loading");
    assert_eq!(
        (loading.name.as_str(), loading.last),
        ("Bass", eq.mode_id())
    );
}

/// A load tells the daemon which preset it recalled before it writes the curve, so every other
/// front end shows it.
#[test]
fn a_load_reports_its_preset_first() {
    let mut app = resting("gungnir2-v2", EqMode::Flat);
    focus(&mut app, "Bass");
    let intents = press(&mut app, KeyCode::Enter);
    assert_eq!(
        intents.first(),
        Some(&Intent::Recalled("Bass".into())),
        "{intents:?}"
    );
}

/// A Loudness preset writes its strength the way the loudness row's own arrows do, just
/// before the mode, both before the bands.
#[test]
fn a_loudness_preset_writes_its_strength_as_the_loudness_row_does() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    let mut late = own("Late night", 5, 0.0);
    late.mode = EqMode::Loudness;
    late.loudness = 40;
    late.band = entries(&app, &eq)[0].preset.band.clone();
    app.presets.saved = vec![late];
    focus(&mut app, "Late night");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        [
            Intent::Recalled("Late night".into()),
            app.loudness_intent(&eq, 40),
            Intent::EqMode("loudness".into())
        ]
    );
}

/// A load is loading until the device echoes the function its last write addresses, and no
/// other reading ends it; nor does a load of the curve the device already carries, which
/// writes nothing and so waits for nothing.
#[test]
fn a_load_is_loading_until_the_echo_of_its_last_write() {
    let mut app = resting("gungnir2-v2", EqMode::Flat);
    let eq = eq_of(&app);
    focus(&mut app, "Bass");
    press(&mut app, KeyCode::Enter);
    let band = eq.band_id(0, EqParam::Gain).expect("band 1's gain");
    hear_id(&mut app, band, i64::from(eq.gain_raw(4.0)));
    assert!(
        app.readings.loading.is_some(),
        "a reading of another function ended it"
    );
    hear_id(&mut app, eq.mode_id(), i64::from(EqMode::Eq as u8));
    assert!(
        app.readings.loading.is_none(),
        "the last write's echo did not end it"
    );

    let mut on = resting("gungnir2-v2", EqMode::Eq);
    focus(&mut on, "Flat");
    assert_eq!(
        press(&mut on, KeyCode::Enter),
        [Intent::Recalled("Flat".into())]
    );
    assert!(
        on.readings.loading.is_none(),
        "a load that wrote nothing is waiting"
    );
    assert_eq!(on.presets.loaded_preset.as_deref(), Some("Flat"));
}

/// A load that never hears its echo stops saying it is loading: a write that failed ends it
/// at once, and a device that never answers ends it once its lifetime has passed. A failure
/// that is not the device's — the configuration file refusing a write — ends nothing.
#[test]
fn loading_ends_on_a_failed_write_or_after_its_lifetime() {
    let mut failed = resting("gungnir2-v2", EqMode::Flat);
    focus(&mut failed, "Bass");
    press(&mut failed, KeyCode::Enter);
    failed.notify_failure(&Failure::of(
        Kind::ConfigUnreadable,
        "config: a preset named \"mine\" is already configured",
    ));
    assert!(
        failed.readings.loading.is_some(),
        "a refused rename ended it"
    );
    assert!(
        !failed.readings.pending.is_empty(),
        "a refused rename ended the wait"
    );
    failed.write_failed(&Failure::of(Kind::Dropped, "the link went away"));
    assert!(
        failed.readings.loading.is_none(),
        "a failed write left it loading"
    );
    assert!(
        failed.readings.pending.is_empty(),
        "a failed write left steps pending"
    );

    let mut silent = resting("gungnir2-v2", EqMode::Flat);
    focus(&mut silent, "Bass");
    press(&mut silent, KeyCode::Enter);
    let since = silent.readings.loading.as_ref().expect("loading").since;
    silent.tick(since + ANSWER_LIFETIME - Duration::from_millis(1));
    assert!(
        silent.readings.loading.is_some(),
        "it gave up before its time"
    );
    silent.tick(since + ANSWER_LIFETIME);
    assert!(silent.readings.loading.is_none(), "it never gave up");
}

/// A load the device never answers gives up its steps with its `loading`: once its time has
/// passed nothing of it is pending, and a band key steps from what the device reads rather
/// than from the step the load asked for and the device never confirmed — the mode too, so
/// on a device still reading Flat mode the key switches it to EQ. Before its time the key
/// still follows the load.
#[test]
fn a_load_left_unanswered_gives_up_its_steps_with_its_loading() {
    let mut app = resting("gungnir2-v2", EqMode::Flat);
    let eq = eq_of(&app);
    focus(&mut app, "Bass");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('1'));
    let since = app.readings.loading.as_ref().expect("loading").since;
    assert!(!app.readings.pending.is_empty());

    app.tick(since + ANSWER_LIFETIME - Duration::from_millis(1));
    assert!(app.readings.loading.is_some() && !app.readings.pending.is_empty());
    assert_eq!(
        app.nudge_band(0, -1),
        [Intent::EqSet {
            band: 1,
            param: "g".into(),
            value: eq.gain_db(eq.gain_raw(4.0) - 1),
        }],
        "before its time the key follows the load"
    );

    app.tick(since + ANSWER_LIFETIME);
    assert!(app.readings.loading.is_none(), "the load is still loading");
    assert!(
        app.readings.pending.is_empty(),
        "{:?}",
        app.readings.pending
    );
    assert_eq!(
        press(&mut app, KeyCode::Left),
        [
            Intent::EqSet {
                band: 1,
                param: "g".into(),
                value: eq.gain_db(eq.gain_zero - 1),
            },
            Intent::EqMode("eq".into())
        ],
        "← stepped from a step or a mode the device never confirmed"
    );
}

/// The header names the preset that is on; once the curve moves away from the one loaded
/// it names that one with a `*`, until the curve matches it again; with neither, it says
/// the curve is custom. While a load is being confirmed it says so.
#[test]
fn the_header_names_the_preset_on_marks_a_change_and_otherwise_says_custom() {
    let text = |app: &App| standing(app).expect("a tone control").text();
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    assert_eq!(text(&app), "Preset Flat");

    hear_flat(&mut app, EqMode::Flat);
    assert_eq!(
        text(&app),
        "Preset custom",
        "Flat mode is not the Flat preset"
    );

    focus(&mut app, "Bass");
    let intents = press(&mut app, KeyCode::Enter);
    assert_eq!(text(&app), "loading Bass");
    // The device carries every write out and echoes each one.
    let plan = preset::plan_recall(&eq, |id| app.raw_of(id), &entries(&app, &eq)[2].preset)
        .expect("Bass fits");
    assert_eq!(
        plan.writes.len() + 1,
        intents.len(),
        "the plan's writes and the report"
    );
    for write in plan.writes {
        hear_id(
            &mut app,
            write.id(&eq).expect("a function"),
            i64::from(write.raw()),
        );
    }
    assert_eq!(text(&app), "Preset Bass");

    // An edit from the tone screen's own arrows, echoed by the device.
    let gain = eq.band_id(0, EqParam::Gain).expect("band 1's gain");
    let raised = app.raw_of(gain).expect("a gain") + 1;
    hear_id(&mut app, gain, raised);
    assert_eq!(text(&app), "Preset Bass*");
    hear_id(&mut app, gain, raised - 1);
    assert_eq!(text(&app), "Preset Bass", "matching again drops the mark");

    // A curve another preset describes names that one, whatever was loaded.
    hear_flat(&mut app, EqMode::Eq);
    assert_eq!(text(&app), "Preset Flat");

    // Of two presets that both match, the one loaded is the one named.
    let mut mine = entries(&app, &eq)[0].preset.clone();
    mine.name = "My flat".into();
    app.presets.saved = vec![mine];
    assert_eq!(text(&app), "Preset Flat", "the first that matches");
    app.presets.loaded_preset = Some("my FLAT".into());
    assert_eq!(
        text(&app),
        "Preset My flat",
        "the one loaded, as the file spells it"
    );
    app.presets.saved.clear();

    // A loaded preset that has gone from the file is no name to mark.
    app.presets.loaded_preset = Some("gone".into());
    hear_id(&mut app, gain, raised);
    assert_eq!(text(&app), "Preset custom");
}

/// `0` brings every band back to 0 dB on its centre frequency and centre Q — the boosts first,
/// then the frequencies and Q, then the cuts — and leaves the mode alone.
#[test]
fn zero_flattens_every_band_onto_its_centre_and_leaves_the_mode() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    let gain = |band| eq.band_id(band, EqParam::Gain).expect("a gain");
    hear_id(&mut app, gain(1), i64::from(eq.gain_zero) - 10);
    hear_id(&mut app, gain(3), i64::from(eq.gain_zero) + 10);
    let q = eq.band_id(3, EqParam::Q).expect("a Q");
    hear_id(&mut app, q, 20);
    let writes = press(&mut app, KeyCode::Char('0'));
    assert_eq!(
        writes,
        [
            Intent::EqSet {
                band: 4,
                param: "g".into(),
                value: 0.0
            },
            Intent::EqSet {
                band: 4,
                param: "q".into(),
                value: eq.q(64)
            },
            Intent::EqSet {
                band: 2,
                param: "g".into(),
                value: 0.0
            },
        ]
    );
    carried_out(&mut app, &writes);
    assert_eq!(
        standing(&app).expect("a tone control").text(),
        "Preset Flat",
        "the curve is Flat's in every step"
    );
    assert!(press(&mut app, KeyCode::Char('0')).is_empty());
    hear_flat(&mut app, EqMode::Eq);
    assert!(
        press(&mut app, KeyCode::Char('0')).is_empty(),
        "a flat curve wrote something"
    );
}

/// `n` asks for a name, prefilled with the first `custom N` the file does not carry in any
/// band count; `Enter` saves the device's curve under it as the one loaded, and the list
/// focuses the row it will land on.
#[test]
fn n_saves_the_curve_under_the_next_free_custom_name() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    app.presets.saved = vec![own("custom 1", 3, 0.0), own("Custom 2", 5, 0.0)];
    let gain = eq.band_id(0, EqParam::Gain).expect("band 1's gain");
    hear_id(&mut app, gain, i64::from(eq.gain_zero) + 5);
    press(&mut app, KeyCode::Char('p'));
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(
        step(&app),
        Some(&Step::Naming(NameLine {
            renaming: None,
            import: None,
            text: "custom 3".into(),
            caret: 8
        }))
    );
    let intents = press(&mut app, KeyCode::Enter);
    let [Intent::SavePreset(saved)] = intents.as_slice() else {
        panic!("expected one preset to be saved, got {intents:?}");
    };
    assert_eq!(saved.name, "custom 3");
    assert_eq!(
        Some(saved),
        preset::from_readings(&eq, |id| app.raw_of(id), "custom 3").as_ref(),
        "the curve saved is not the device's"
    );
    assert_eq!(
        step(&app),
        Some(&Step::Choosing),
        "the list did not come back"
    );
    assert_eq!(
        app.presets.preset_list.as_ref().map(|list| list.row),
        Some(5),
        "the focus is not on the row the new preset lands on"
    );
    assert_eq!(app.presets.loaded_preset.as_deref(), Some("custom 3"));
}

/// The name prompt takes every key as typing, and refuses what the file would refuse where
/// it was typed: a blank name, a built-in's in any case, and another preset's in any case.
#[test]
fn the_name_prompt_refuses_a_blank_a_built_in_and_a_taken_name() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.saved = vec![own("HD650 warm", 3, 0.0)];
    press(&mut app, KeyCode::Char('p'));
    press(&mut app, KeyCode::Char('n'));
    for _ in 0.."custom 1".len() {
        press(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "  ");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some(preset::NameError::Blank.to_string().as_str())
    );

    for (typed, why) in [
        (
            "BASS",
            preset::NameRefusal::BuiltIn("BASS".into()).to_string(),
        ),
        (
            "hd650 WARM",
            preset::NameRefusal::Taken("HD650 warm".into()).to_string(),
        ),
    ] {
        press(&mut app, KeyCode::End);
        for _ in 0..20 {
            press(&mut app, KeyCode::Backspace);
        }
        type_text(&mut app, typed);
        assert!(
            press(&mut app, KeyCode::Enter).is_empty(),
            "{typed} was saved"
        );
        assert_eq!(
            app.toast().map(|toast| toast.text.as_str()),
            Some(why.as_str())
        );
        assert!(
            matches!(step(&app), Some(Step::Naming(_))),
            "the prompt closed on a refused name"
        );
    }

    // The letters that are keys elsewhere are letters here, and `Esc` leaves the prompt
    // for the list with nothing written.
    type_text(&mut app, "qtdm");
    assert_eq!(app.screen, Screen::Tone);
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert_eq!(step(&app), Some(&Step::Choosing));
}

/// `c` renames the focused preset through the same prompt, prefilled with its name; the one
/// being renamed may take its own name in another spelling, and the loaded name follows.
#[test]
fn c_renames_through_the_prompt_prefilled_with_the_name() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.saved = vec![own("hd650 warm", 5, 1.0)];
    app.presets.loaded_preset = Some("HD650 WARM".into());
    focus(&mut app, "hd650 warm");
    press(&mut app, KeyCode::Char('c'));
    assert_eq!(
        step(&app),
        Some(&Step::Naming(NameLine {
            renaming: Some("hd650 warm".into()),
            import: None,
            text: "hd650 warm".into(),
            caret: 10
        }))
    );
    for _ in 0..10 {
        press(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "HD650 warm");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        [Intent::RenamePreset(
            "hd650 warm".into(),
            "HD650 warm".into()
        )]
    );
    assert_eq!(app.presets.loaded_preset.as_deref(), Some("HD650 warm"));
    assert_eq!(step(&app), Some(&Step::Choosing));
}

/// `u` and `x` ask first, and only `y` writes: `u` the device's curve over the focused
/// preset under its own name, `x` its removal. `n`, `Esc` and `Enter` — no is the default —
/// go back to the list, and every other key is swallowed.
#[test]
fn u_and_x_ask_first_and_only_y_writes() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    app.presets.saved = vec![own("HD650 warm", 5, 1.0)];
    focus(&mut app, "HD650 warm");
    for no in [KeyCode::Char('n'), KeyCode::Esc, KeyCode::Enter] {
        press(&mut app, KeyCode::Char('u'));
        assert_eq!(
            step(&app),
            Some(&Step::Asking(Question::Update("HD650 warm".into())))
        );
        assert!(press(&mut app, KeyCode::Char('q')).is_empty());
        assert!(matches!(step(&app), Some(Step::Asking(_))), "q answered");
        assert!(press(&mut app, no).is_empty(), "{no:?} wrote");
        assert_eq!(step(&app), Some(&Step::Choosing), "{no:?}");
    }
    press(&mut app, KeyCode::Char('u'));
    let curve = preset::from_readings(&eq, |id| app.raw_of(id), "HD650 warm")
        .expect("the whole curve is read");
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        [Intent::UpdatePreset("HD650 warm".into(), curve)]
    );
    assert_eq!(app.presets.loaded_preset.as_deref(), Some("HD650 warm"));

    press(&mut app, KeyCode::Char('x'));
    assert_eq!(
        step(&app),
        Some(&Step::Asking(Question::Delete("HD650 warm".into())))
    );
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        [Intent::RemovePreset("HD650 warm".into())]
    );
    assert_eq!(
        app.presets.loaded_preset, None,
        "a deleted preset is still the loaded one"
    );
    assert_eq!(step(&app), Some(&Step::Choosing));
}

/// `u`, `c` and `x` refuse a built-in and say why, with the list still up.
#[test]
fn u_c_and_x_refuse_a_built_in() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    focus(&mut app, "Presence");
    for letter in ['u', 'c', 'x'] {
        app.dismiss_toast();
        assert!(press(&mut app, KeyCode::Char(letter)).is_empty());
        assert_eq!(
            app.toast().map(|toast| toast.text.clone()),
            Some(read_only("Presence")),
            "{letter}"
        );
        assert_eq!(step(&app), Some(&Step::Choosing), "{letter}");
    }
}

/// While a load is still being confirmed, `n` and `u` are refused with a notice, since each
/// would save the readings the device had before it; `c` and `x`, which only touch the
/// file, go on as ever, as do `0` and another load, which plan from the steps the load asked
/// for. Once the device has echoed the load the refused keys work again.
#[test]
fn a_load_still_being_confirmed_holds_the_keys_that_save_the_curve() {
    let mut app = resting("gungnir2-v2", EqMode::Flat);
    let eq = eq_of(&app);
    app.presets.saved = vec![own("HD650 warm", 5, 1.0)];
    focus(&mut app, "Bass");
    assert!(!press(&mut app, KeyCode::Enter).is_empty());
    let held = "still loading Bass";

    press(&mut app, KeyCode::Char('p'));
    app.dismiss_toast();
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some(held),
        "n"
    );
    assert_eq!(step(&app), Some(&Step::Choosing), "n moved on");
    press(&mut app, KeyCode::Esc);
    focus(&mut app, "HD650 warm");
    app.dismiss_toast();
    press(&mut app, KeyCode::Char('u'));
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some(held),
        "u"
    );
    assert_eq!(step(&app), Some(&Step::Choosing), "u asked anyway");
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(
        step(&app),
        Some(&Step::Asking(Question::Delete("HD650 warm".into()))),
        "x is held too"
    );
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('c'));
    assert!(matches!(step(&app), Some(Step::Naming(_))), "c is held too");
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);

    app.dismiss_toast();
    let flattened = press(&mut app, KeyCode::Char('0'));
    assert_eq!(
        band_params(&flattened),
        [(1, "g"), (2, "g"), (1, "q"), (2, "q")],
        "0 brings down the gains Bass asked for, then recentres its Q: {flattened:?}"
    );
    assert_ne!(
        app.toast().map(|toast| toast.text.as_str()),
        Some(held),
        "0 was held"
    );

    hear_id(&mut app, eq.mode_id(), i64::from(EqMode::Eq as u8));
    assert!(app.readings.loading.is_none());
    press(&mut app, KeyCode::Char('p'));
    press(&mut app, KeyCode::Char('n'));
    assert!(
        matches!(step(&app), Some(Step::Naming(_))),
        "n is still held"
    );
}

/// Bass is on, band 1 at +4.0 dB, and Flat is loaded: while the device is still
/// answering, `←` on band 1 steps from the 0 dB Flat asked for, not from the +4.0 dB it
/// still reads, so once every write has landed band 1 is one step below 0 dB — not one step
/// below +4.0 dB, which would undo the load on that band. The other way round, `→` during a
/// load of Bass over Flat lands one step above +4.0 dB. The Shift arrows, the wheel and
/// the Q keys step from the load's steps the same way.
#[test]
fn a_band_key_during_a_load_steps_from_what_the_load_asked_for() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    let gain = eq.band_id(0, EqParam::Gain).expect("band 1's gain");
    focus(&mut app, "Bass");
    let intents = press(&mut app, KeyCode::Enter);
    carried_out(&mut app, &intents);
    let lifted = i64::from(eq.gain_raw(4.0));
    assert_eq!(app.raw_of(gain), Some(lifted));
    assert!(app.readings.loading.is_none() && app.readings.pending.is_empty());

    focus(&mut app, "Flat");
    let mut sent = press(&mut app, KeyCode::Enter);
    assert_eq!(
        standing(&app).map(|standing| standing.text()).as_deref(),
        Some("loading Flat")
    );
    press(&mut app, KeyCode::Char('1'));
    let trim = press(&mut app, KeyCode::Left);
    assert_eq!(
        trim,
        [Intent::EqSet {
            band: 1,
            param: "g".into(),
            value: eq.gain_db(eq.gain_zero - 1),
        }],
        "← stepped from the reading, not from the step Flat asked for"
    );
    sent.extend(trim);
    carried_out(&mut app, &sent);
    assert_eq!(app.raw_of(gain), Some(i64::from(eq.gain_zero) - 1));
    assert!(app.readings.loading.is_none() && app.readings.pending.is_empty());

    let mut app = resting("gungnir2-v2", EqMode::Eq);
    focus(&mut app, "Bass");
    let mut sent = press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('1'));
    sent.extend(press(&mut app, KeyCode::Right));
    carried_out(&mut app, &sent);
    assert_eq!(app.raw_of(gain), Some(lifted + 1));

    // The Q keys, their Shift-arrow aliases and the wheel over a band's row, during the load:
    // Bass asks bands 1 and 2 for Q 0.70 and band 2 for +2.0 dB, and the device still
    // reads Q 4.30 and 0 dB.
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    focus(&mut app, "Bass");
    press(&mut app, KeyCode::Enter);
    let asked_q = eq.q(eq.q_raw(0.7));
    let q_of = |intents: &[Intent], what: &str| match intents {
        [Intent::EqSet { param, value, .. }] if param == "q" => *value,
        _ => panic!("{what} wrote no Q alone: {intents:?}"),
    };
    press(&mut app, KeyCode::Char('1'));
    let wider = q_of(&press(&mut app, KeyCode::Char('[')), "[");
    assert!(
        (wider - asked_q / 1.1).abs() < 0.01,
        "[ stepped from the Q the device reads, not the one Bass asked for: {wider}"
    );
    press(&mut app, KeyCode::Char('2'));
    let narrower = q_of(
        &app.key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT)),
        "⇧↓",
    );
    assert!(
        (narrower - asked_q * 1.1).abs() < 0.01,
        "⇧↓ stepped from the Q the device reads: {narrower}"
    );
    assert_eq!(
        app.nudge_band(1, -1),
        [Intent::EqSet {
            band: 2,
            param: "g".into(),
            value: eq.gain_db(eq.gain_raw(2.0) - 1),
        }],
        "the wheel stepped from the gain the device reads"
    );
}

/// `0` pressed after band steps the device has not answered yet brings the band they asked
/// for back to 0 dB, rather than finding the reading already there and writing nothing.
#[test]
fn zero_after_unanswered_presses_flattens_the_band_they_asked_for() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    let gain = eq.band_id(0, EqParam::Gain).expect("band 1's gain");
    press(&mut app, KeyCode::Char('1'));
    let mut sent = Vec::new();
    for _ in 0..5 {
        sent.extend(press(&mut app, KeyCode::Right));
    }
    let flattened = press(&mut app, KeyCode::Char('0'));
    assert_eq!(
        flattened,
        [Intent::EqSet {
            band: 1,
            param: "g".into(),
            value: eq.gain_db(eq.gain_zero),
        }]
    );
    sent.extend(flattened);
    carried_out(&mut app, &sent);
    assert_eq!(app.raw_of(gain), Some(i64::from(eq.gain_zero)));
    assert!(app.readings.pending.is_empty());
}

/// A load made while another is still being answered plans from the steps the first asked
/// for: Flat after Bass on a device resting in Flat mode brings back the two bands
/// Bass moved, writes no mode — Bass has already asked for EQ — and the device
/// ends on the Flat preset, the header saying so. A load of the curve the first is already
/// on its way to writes nothing, and waits for the first under its own name.
#[test]
fn a_load_during_a_load_plans_from_the_first_ones_steps() {
    let mut app = resting("gungnir2-v2", EqMode::Flat);
    focus(&mut app, "Bass");
    let mut sent = press(&mut app, KeyCode::Enter);
    focus(&mut app, "Flat");
    let second = press(&mut app, KeyCode::Enter);
    assert_eq!(
        band_params(&second),
        [(1, "g"), (2, "g"), (1, "q"), (2, "q")],
        "{second:?}"
    );
    assert!(
        !second
            .iter()
            .any(|intent| matches!(intent, Intent::EqMode(_))),
        "{second:?}"
    );
    sent.extend(second);
    carried_out(&mut app, &sent);
    assert_eq!(
        standing(&app).map(|standing| standing.text()).as_deref(),
        Some("Preset Flat")
    );
    assert!(app.readings.pending.is_empty());

    let mut app = resting("gungnir2-v2", EqMode::Flat);
    focus(&mut app, "Bass");
    let sent = press(&mut app, KeyCode::Enter);
    focus(&mut app, "Bass");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        [Intent::Recalled("Bass".into())]
    );
    let text = |app: &App| standing(app).map(|standing| standing.text());
    assert_eq!(text(&app).as_deref(), Some("loading Bass"));
    carried_out(&mut app, &sent);
    assert_eq!(text(&app).as_deref(), Some("Preset Bass"));
}

/// A device that echoes a load's last write only after the echo of an earlier write to the
/// same function — a band key pressed before the load — keeps the load loading until its
/// own write is answered.
#[test]
fn an_earlier_echo_of_the_loads_last_function_does_not_end_it() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    press(&mut app, KeyCode::Char('2'));
    let mut sent = press(&mut app, KeyCode::Right);
    focus(&mut app, "Bass");
    sent.extend(press(&mut app, KeyCode::Enter));
    let last = app.readings.loading.as_ref().expect("loading").last;
    assert_eq!(last, eq.band_id(1, EqParam::Gain).expect("band 2's gain"));
    let (key, load) = sent.split_at(1);
    carried_out(&mut app, key);
    assert!(
        app.readings.loading.is_some(),
        "the band key's echo ended the load"
    );
    let (before, own) = load.split_at(load.len() - 1);
    carried_out(&mut app, before);
    assert!(
        app.readings.loading.is_some(),
        "another function's echo ended the load"
    );
    carried_out(&mut app, own);
    assert!(
        app.readings.loading.is_none(),
        "its own last write's echo did not end it"
    );
    assert!(app.readings.pending.is_empty());
}

/// A band edit made while a load is being answered switches the tone control to EQ as
/// the mode that load asked for says: a band key during a load of a Flat-mode preset also
/// writes the mode to EQ, so the edit is heard, and one during a load of an EQ preset onto a
/// device in Flat mode writes no mode — the load already asks for EQ. The loudness row's
/// arrows step from the strength a Loudness preset asked for.
#[test]
fn a_band_edit_during_a_load_switches_from_the_mode_the_load_asked_for() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let mut bypass = own("Bypass", 5, 0.0);
    bypass.mode = EqMode::Flat;
    bypass.band = entries(&app, &eq_of(&app))[0].preset.band.clone();
    app.presets.saved = vec![bypass];
    focus(&mut app, "Bypass");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        [
            Intent::Recalled("Bypass".into()),
            Intent::EqMode("flat".into())
        ]
    );
    press(&mut app, KeyCode::Char('1'));
    let edit = press(&mut app, KeyCode::Right);
    assert_eq!(edit.last(), Some(&Intent::EqMode("eq".into())), "{edit:?}");
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some(preset::SWITCHED_TO_EQ)
    );

    let mut app = resting("gungnir2-v2", EqMode::Flat);
    focus(&mut app, "Bass");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char('1'));
    app.dismiss_toast();
    let edit = press(&mut app, KeyCode::Right);
    assert_eq!(band_params(&edit), [(1, "g")]);
    assert_eq!(edit.len(), 1, "a mode was written: {edit:?}");
    assert!(app.toast().is_none());

    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    let mut late = own("Late night", 5, 0.0);
    late.mode = EqMode::Loudness;
    late.loudness = 40;
    late.band = entries(&app, &eq)[0].preset.band.clone();
    app.presets.saved = vec![late];
    focus(&mut app, "Late night");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.step_loudness(1), [app.loudness_intent(&eq, 41)]);
}

/// The name prompt takes no more characters than a preset's name may have, and refuses a
/// longer name handed to it.
#[test]
fn the_name_prompt_stops_at_the_longest_name_a_preset_takes() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    press(&mut app, KeyCode::Char('p'));
    press(&mut app, KeyCode::Char('n'));
    type_text(&mut app, &"y".repeat(50));
    let Some(Step::Naming(line)) = step(&app) else {
        panic!("the prompt closed");
    };
    assert_eq!(line.text.chars().count(), preset::NAME_MAX, "{}", line.text);
    assert!(line.text.starts_with("custom 1"));
    assert!(matches!(
        press(&mut app, KeyCode::Enter).as_slice(),
        [Intent::SavePreset(saved)] if saved.name.chars().count() == preset::NAME_MAX
    ));
    assert_eq!(
        refused_name(&[], &"z".repeat(preset::NAME_MAX + 1), None),
        Err(preset::NameError::TooLong(preset::NAME_MAX + 1).to_string())
    );
    assert_eq!(
        refused_name(&[], "  HD650 warm ", None),
        Ok("HD650 warm"),
        "the blanks round a name are no part of it"
    );
    assert_eq!(
        refused_name(&[], "new\nline", None),
        Err(preset::NameError::Control.to_string())
    );
    assert_eq!(
        refused_name(&[own("HD650 warm", 5, 0.0)], " hd650 WARM ", None),
        Err(preset::NameRefusal::Taken("HD650 warm".into()).to_string())
    );
}

/// Opening the list asks for the presets to be read from the file again, and taking what
/// the file says focuses the preset that is on among them.
#[test]
fn opening_the_list_reads_the_presets_again_and_focuses_the_one_on() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    assert_eq!(press(&mut app, KeyCode::Char('p')), [Intent::ReadPresets]);
    assert_eq!(
        app.presets.preset_list.as_ref().map(|list| list.row),
        Some(0)
    );
    let mut mine = entries(&app, &eq)[0].preset.clone();
    mine.name = "My flat".into();
    app.presets.loaded_preset = Some("My flat".into());
    reread(&mut app, vec![own("Elsewhere", 5, 2.0), mine]);
    assert_eq!(names(&app)[4..], ["Elsewhere", "My flat"]);
    assert_eq!(
        app.presets.preset_list.as_ref().map(|list| list.row),
        Some(5),
        "the preset that is on is not focused"
    );
}

/// `Esc` closes the list; every key that is not one of its own is swallowed while it is
/// open, the ones that switch screen or quit elsewhere among them.
#[test]
fn esc_closes_the_list_and_it_swallows_every_other_key() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    press(&mut app, KeyCode::Char('p'));
    let before = app.snapshot_for_test();
    for code in [
        KeyCode::Char('q'),
        KeyCode::Char('t'),
        KeyCode::Char('m'),
        KeyCode::Char('f'),
        KeyCode::Char('0'),
        KeyCode::Char('?'),
        KeyCode::Char('E'),
        KeyCode::Tab,
        KeyCode::Left,
        KeyCode::Right,
    ] {
        assert!(
            press(&mut app, code).is_empty(),
            "{code:?} asked for a write"
        );
    }
    assert_eq!(
        app.snapshot_for_test(),
        before,
        "a key moved something under it"
    );
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert!(
        app.presets.preset_list.is_none(),
        "Esc did not close the list"
    );
    assert!(!app.modal_open());
    assert_eq!(app.screen, Screen::Tone, "Esc left the screen too");
}

/// Warm loaded and then flattened with `0` is Flat in every step, so the header reads Flat
/// (not `Warm*`) and the list marks Flat as the one that is on.
#[test]
fn zero_after_a_load_leaves_flat_the_preset_that_is_on() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    let eq = eq_of(&app);
    focus(&mut app, "Warm");
    let loaded = press(&mut app, KeyCode::Enter);
    carried_out(&mut app, &loaded);
    assert_eq!(app.presets.loaded_preset.as_deref(), Some("Warm"));
    assert_eq!(
        standing(&app).map(|standing| standing.text()).as_deref(),
        Some("Preset Warm")
    );

    let flattened = press(&mut app, KeyCode::Char('0'));
    assert!(!flattened.is_empty());
    carried_out(&mut app, &flattened);
    assert_eq!(
        standing(&app).map(|standing| standing.text()).as_deref(),
        Some("Preset Flat")
    );
    assert_eq!(on(&app, &eq, &entries(&app, &eq)).as_deref(), Some("Flat"));
}

/// An imported preset is listed on every tone control, loads as its fit, is the one that is on
/// afterwards, and `u` over it refuses, saying why.
#[test]
fn an_imported_preset_is_listed_loaded_and_matched_and_u_refuses_it() {
    for name in ["gungnir2-v2", "mesh-eq-3"] {
        let mut app = resting(name, EqMode::Flat);
        app.presets.saved = vec![imported("Phones")];
        assert_eq!(
            names(&app).last().map(String::as_str),
            Some("Phones"),
            "{name}"
        );
        focus(&mut app, "Phones");
        let intents = press(&mut app, KeyCode::Enter);
        assert!(!band_params(&intents).is_empty(), "{name}");
        carried_out(&mut app, &intents);
        let eq = eq_of(&app);
        assert_eq!(
            on(&app, &eq, &entries(&app, &eq)).as_deref(),
            Some("Phones"),
            "{name}"
        );

        focus(&mut app, "Phones");
        app.dismiss_toast();
        assert!(press(&mut app, KeyCode::Char('u')).is_empty());
        assert_eq!(
            app.toast().map(|toast| toast.text.clone()),
            Some("Phones is imported: save the curve under a new name".to_string()),
            "{name}"
        );
        assert_eq!(step(&app), Some(&Step::Choosing), "{name}");
    }
}
