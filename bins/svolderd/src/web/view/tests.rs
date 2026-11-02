// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;

use svolder_ipc::{DeviceInfo, LinkChange, RecallOut, SeriesOut};
use svolder_profiles::Profile;
use svolder_profiles::history::Aggregate;
use svolder_profiles::tone::preset;
use svolder_profiles::{EqMode, EqParam, units};
use svolder_proto::{Message, Value};

use super::*;
use crate::test_support::profile_named;
use crate::writer::ClimbView;

/// A whole minute of the unix clock, in milliseconds, to put stamps in.
const MINUTE_MS: u64 = 1_700_000_040_000;

pub(super) use super::test_support::{NOW_MS, OPEN, info, read, shot, tone_on};

/// The page writes a minus as `−` where it signs a number, and leaves other hyphens alone.
#[test]
fn the_page_prints_a_typographic_minus() {
    assert_eq!(minus("-24.1 dB"), "−24.1 dB");
    assert_eq!(minus("-0.20 dB"), "−0.20 dB");
    assert_eq!(minus("+6.0 dB"), "+6.0 dB");
    assert_eq!(minus("fake-five"), "fake-five");
    assert_eq!(minus("silence"), "silence");
}

/// A name goes into a URL as one segment: a space and a slash are encoded.
#[test]
fn a_name_is_one_path_segment() {
    assert_eq!(segment("Living room/DAC"), "Living%20room%2FDAC");
    assert_eq!(segment("gungnir"), "gungnir");
}

/// Words with a command in backticks split into the text and the code.
#[test]
fn backticks_mark_a_command() {
    let parts: Vec<(bool, String)> = spans("No devices yet. `svolder setup` adds one.")
        .into_iter()
        .map(|s| (s.code, s.text))
        .collect();
    assert_eq!(
        parts,
        [
            (false, "No devices yet. ".to_string()),
            (true, "svolder setup".to_string()),
            (false, " adds one.".to_string())
        ]
    );
}

/// The Gungnir 2's controls by kind, in profile order: the input (five values) a segmented track,
/// the toggles switches, the balance a slider from its centre, the readouts as words; the
/// volume and the mute are the knob's, not the list's.
#[test]
fn the_gungnir_shows_each_control_by_its_kind() {
    let p = profile_named("gungnir2-v2");
    let readings = vec![
        read(&p, "input", "3"),
        read(&p, "balance", "70"),
        read(&p, "usb1_connected", "true"),
        read(&p, "spdif_locked", "false"),
        read(&p, "sample_rate", "4"),
    ];
    let info = info("Gungnir 2");
    let screen = screen(&shot(&info, &p, &readings));
    let keys: Vec<&str> = screen.controls.iter().map(ControlView::key).collect();
    assert!(
        !keys.contains(&"volume") && !keys.contains(&"mute"),
        "{keys:?}"
    );
    let input = screen.controls.iter().find(|c| c.key() == "input").unwrap();
    let ControlView::Segments { options, .. } = input else {
        panic!("{input:?}");
    };
    assert_eq!(options.len(), 5);
    assert!(options.iter().find(|o| o.label == "Optical").unwrap().on);
    assert_eq!(
        options.iter().find(|o| o.label == "Optical").unwrap().json,
        r#"{"value":"Optical"}"#
    );
    let balance = screen
        .controls
        .iter()
        .find(|c| c.key() == "balance")
        .unwrap();
    let ControlView::Slider(slider) = balance else {
        panic!("{balance:?}");
    };
    assert_eq!((slider.centre, slider.text.as_str()), (Some(64), "6"));
    assert_eq!(slider.ends, Some(("L".to_string(), "R".to_string())));
    let readout = |key| match screen.controls.iter().find(|c| c.key() == key) {
        Some(ControlView::Readout { text, tone, .. }) => (text.clone(), *tone),
        other => panic!("{other:?}"),
    };
    assert_eq!(readout("usb1_connected"), ("connected".to_string(), "good"));
    assert_eq!(readout("usb2_connected"), ("–".to_string(), ""));
    assert_eq!(readout("spdif_locked"), ("unlocked".to_string(), ""));
    assert_eq!(readout("sample_rate"), ("96 kHz".to_string(), ""));
    assert_eq!((screen.family, screen.family_word), ("dac", "DAC"));
    assert!(!screen.amp);
}

/// A switch posts the opposite of what it reads, `true` before a reading; so does the power
/// button.
#[test]
fn a_switch_and_the_power_button_post_the_opposite_of_what_they_read() {
    let p = profile_named("gungnir2-v2");
    let gungnir = info("Gungnir 2");
    let json = |readings: &[Message]| -> String {
        let screen = screen(&shot(&gungnir, &p, readings));
        match screen.controls.iter().find(|c| c.key() == "phase") {
            Some(ControlView::Switch { json, .. }) => json.clone(),
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(json(&[read(&p, "phase", "on")]), r#"{"value":false}"#);
    assert_eq!(json(&[read(&p, "phase", "off")]), r#"{"value":true}"#);
    assert_eq!(json(&[]), r#"{"value":true}"#);
    let tyr = profile_named("tyr");
    let tyr_info = info("Tyr");
    let power = |readings: &[Message]| {
        screen(&shot(&tyr_info, &tyr, readings))
            .power
            .expect("a power button")
            .json
    };
    assert_eq!(power(&[read(&tyr, "power", "on")]), r#"{"value":false}"#);
    assert_eq!(power(&[]), r#"{"value":true}"#);
}

/// The knob's `db` has one entry per step: silence at the bottom is `null`, the top a number, and
/// a control whose unit is not dB gives only `null`; the model carries it as `volume.db`.
#[test]
fn the_knob_carries_each_steps_db() {
    let p = profile_named("gungnir2-v2");
    let readings = vec![read(&p, "volume", "63"), read(&p, "mute", "off")];
    let info = info("Gungnir 2");
    let s = screen(&shot(&info, &p, &readings));
    let knob = s.knob.as_ref().expect("a knob");
    let db = &knob.levels;
    assert_eq!(db.len(), 128);
    assert_eq!(db[0], None);
    assert!(db[127].is_some_and(f64::is_finite));
    assert!(db[1..].iter().all(Option::is_some));
    assert!(db[1] < db[127]);
    let carried = &model_of(&s)["volume"]["db"];
    assert_eq!(carried.as_array().map(Vec::len), Some(128));
    assert!(
        carried[0].is_null() && carried[127].is_number(),
        "{carried}"
    );
    let mut plain = p.clone();
    for control in &mut plain.controls {
        control.unit = None;
    }
    let knob = screen(&shot(&info, &plain, &readings))
        .knob
        .expect("a knob");
    assert_eq!(knob.levels.len(), 128);
    assert!(knob.levels.iter().all(Option::is_none));
}

/// The model of `s` as the page is sent it, as JSON.
fn model_of(s: &Screen) -> serde_json::Value {
    serde_json::to_value(crate::web::model::device_model(s)).unwrap()
}

/// Each press of a segmented track, a switch and the power button carries the JSON it posts, to
/// the control its key names under the device's own URL; a list carries its values; none of it
/// posts for a guest, whose screen says it is inert. An amp's other switches live in the power
/// part and post from there, never from the controls (`ember-parts.mjs` holds the case
/// `the_controls_post_through_htmx_and_an_inert_one_posts_nothing` for the markup).
#[test]
fn the_controls_carry_what_they_post_and_an_inert_one_posts_nothing() {
    let mut p = profile_named("freya2");
    p.controls
        .iter_mut()
        .find(|c| c.key == "input")
        .unwrap()
        .values
        .push((6, "Input 6".into()));
    let name = info("Living room/Freya");
    let readings = [read(&p, "input", "1")];
    let open = model_of(&screen(&shot(&name, &p, &readings)));
    assert_eq!(open["url"], "Living%20room%2FFreya");
    assert_eq!(open["inert"], false);
    let input = open["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["key"] == "input")
        .expect("the input");
    assert_eq!(input["style"], "list");
    let options = input["options"].as_array().unwrap();
    assert_eq!(options.len(), 6);
    assert!(
        options.iter().all(|o| o["json"]
            .as_str()
            .is_some_and(|j| j.starts_with(r#"{"value":""#))),
        "{input}"
    );
    let guest = Snapshot {
        access: Access {
            writes: false,
            guest: true,
        },
        ..shot(&name, &p, &readings)
    };
    assert_eq!(model_of(&screen(&guest))["inert"], true);
    let g = profile_named("gungnir2-v2");
    let dac = info("Gungnir 2");
    let model = model_of(&screen(&shot(&dac, &g, &[read(&g, "input", "3")])));
    let control = |key: &str| {
        model["controls"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["key"] == key)
            .unwrap_or_else(|| panic!("no {key}"))
            .clone()
    };
    let optical = control("input")["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["label"] == "Optical")
        .expect("Optical")
        .clone();
    assert_eq!(optical["json"], r#"{"value":"Optical"}"#);
    assert_eq!(control("phase")["kind"], "toggle");
    let tyr = profile_named("tyr");
    let amp = info("Tyr");
    let model = model_of(&screen(&shot(&amp, &tyr, &[read(&tyr, "power", "on")])));
    assert_eq!(model["power"]["key"], "power");
    assert_eq!(model["power"]["json"], r#"{"value":false}"#);
    // An amp's other switches live in the power part and post from there, never from the
    // controls.
    let aegir = profile_named("aegir2");
    let amp = info("Aegir");
    let model = model_of(&screen(&shot(&amp, &aegir, &[])));
    let toggles: Vec<&str> = model["power"]["toggles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["key"].as_str().unwrap())
        .collect();
    assert!(!toggles.is_empty(), "{model}");
    for control in model["controls"].as_array().unwrap() {
        assert!(
            control["kind"] != "toggle" && !toggles.contains(&control["key"].as_str().unwrap()),
            "{control}"
        );
    }
}

/// The lost-link card is sent again each second while its countdown runs, with its Try now: the
/// page keeps the button across the redraw (`ember-parts.mjs` holds the case
/// `try_now_keeps_its_id_and_is_preserved_across_the_card_swap` for the markup that does).
#[test]
fn the_lost_link_card_carries_try_now_and_its_countdown() {
    let p = profile_named("gungnir2-v2");
    let lost = DeviceInfo {
        connected: false,
        retry_in_secs: Some(5),
        ..info("Gungnir 2")
    };
    let seen = vec![read(&p, "volume", "40")];
    let model = model_of(&screen(&shot(&lost, &p, &seen)));
    assert_eq!(model["card"]["try_now"], true);
    assert_eq!(model["card"]["retry_in"], 5);
}

/// A select with more than five values is a list.
#[test]
fn a_select_with_more_than_five_values_is_a_list() {
    let mut p = profile_named("freya2");
    let info = info("Freya 2");
    let input = p.controls.iter_mut().find(|c| c.key == "input").unwrap();
    input.values.push((6, "Input 6".into()));
    let screen = screen(&shot(&info, &p, &[]));
    assert!(matches!(
        screen.controls.iter().find(|c| c.key() == "input"),
        Some(ControlView::List { .. })
    ));
    assert_eq!(screen.family, "preamp");
}

/// The Gungnir 2's knob carries its dB marks at their true places, −∞ −60 −40 −20 −10 0, and
/// the volume's words for every step; the page's dial lights its ticks up to the level and draws
/// its arcs from them (`ember-parts.mjs` holds the same name).
#[test]
fn the_gungnir_dial_reads_its_labels_at_their_true_places() {
    let p = profile_named("gungnir2-v2");
    let readings = vec![read(&p, "volume", "63"), read(&p, "mute", "off")];
    let info = info("Gungnir 2");
    let knob = screen(&shot(&info, &p, &readings)).knob.expect("a knob");
    let mut marks = knob.marks.clone();
    marks.sort_by(|a, b| a.0.total_cmp(&b.0));
    let labels: Vec<&str> = marks.iter().map(|m| m.1.as_str()).collect();
    assert_eq!(labels, ["−∞", "−60", "−40", "−20", "−10", "0"]);
    assert_eq!(knob.words.len(), 128);
    assert_eq!(knob.words[0], "silence");
    assert!(knob.silence);
    assert_eq!(knob.raw, Some(63));
    assert!(knob.readout.starts_with('−'), "{}", knob.readout);
    assert_eq!(knob.unit.as_deref(), Some("dB"));
    assert!(!knob.muted);
    assert_eq!(knob.mute.as_deref(), Some("mute"));
}

/// Each dial mark carries the fractional step it stands at, which the page compares with a new
/// risk step to turn the labels red: the bottom's is the first step, the top's the last.
#[test]
fn a_dial_label_carries_the_step_it_stands_at() {
    let p = profile_named("gungnir2-v2");
    let readings = vec![read(&p, "volume", "63"), read(&p, "mute", "off")];
    let info = info("Gungnir 2");
    let knob = screen(&shot(&info, &p, &readings)).knob.expect("a knob");
    let mut at: Vec<f64> = knob.marks.iter().map(|m| m.0).collect();
    at.sort_by(f64::total_cmp);
    assert_eq!(at.first(), Some(&0.0));
    assert_eq!(at.last(), Some(&127.0));
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{at:?}");
    let model = &model_of(&screen(&shot(&info, &p, &readings)))["volume"]["marks"];
    assert_eq!(model.as_array().map(Vec::len), Some(6), "{model}");
}

/// An unread volume reads `–` with no level; a muted one at silence says so (`ember-parts.mjs`
/// holds that the dial lights nothing and draws no handle for either).
#[test]
fn an_unread_volume_reads_a_dash_and_a_muted_one_says_so() {
    let p = profile_named("gungnir2-v2");
    let info = info("Gungnir 2");
    let knob = screen(&shot(&info, &p, &[])).knob.unwrap();
    assert_eq!(
        (knob.raw, knob.readout.as_str(), knob.unit.as_deref()),
        (None, "–", None)
    );
    let readings = vec![read(&p, "volume", "0"), read(&p, "mute", "on")];
    let knob = screen(&shot(&info, &p, &readings)).knob.unwrap();
    assert_eq!(
        (knob.readout.as_str(), knob.unit.as_deref()),
        ("silence", None)
    );
    assert!(knob.muted);
    assert!(
        knob.silence && knob.raw == Some(0),
        "silence is the bottom step"
    );
    assert_eq!(knob.sub.as_deref(), Some("muted"));
    assert!(knob.aria.ends_with(", muted"), "{}", knob.aria);
}

/// A curve that boosts makes a risk zone from the step that can clip, with the note; a cap
/// stops the knob below it and says so; a climb names its target (`ember-parts.mjs` holds the same
/// name for the dial's ticks and arcs).
#[test]
fn the_risk_the_cap_and_a_climb_shape_the_knob() {
    let p = profile_named("gungnir2-v2");
    let mut readings = tone_on(&p, "Bass");
    readings.push(read(&p, "volume", "110"));
    readings.push(read(&p, "mute", "off"));
    let info = info("Gungnir 2");
    let knob = screen(&shot(&info, &p, &readings)).knob.unwrap();
    let risk = knob.risk.expect("Bass boosts");
    assert!(risk > 95 && risk <= 127, "{risk}");
    let note = &knob.notes[0];
    assert!(note.text.starts_with("This EQ peaks at +"), "{}", note.text);
    assert!(note.text.contains("loud tracks may clip"), "{}", note.text);
    assert_eq!(note.red, 110 >= risk);
    let capped = Snapshot {
        cap_db: Some(-10.0),
        ..shot(&info, &p, &readings)
    };
    let knob = screen(&capped).knob.unwrap();
    let cap = knob.cap.expect("a cap");
    assert!(
        knob.words[usize::try_from(cap).unwrap()].starts_with("−10"),
        "{}",
        knob.words[cap as usize]
    );
    assert!(cap < knob.hi, "the cap leaves steps above it");
    assert!(
        knob.notes
            .iter()
            .any(|n| n.text == "Capped at −10.0 dB by [web] max_volume_db")
    );
    let climb = ClimbView {
        key: "volume".into(),
        target: 120,
    };
    let climbing = Snapshot {
        climb: Some(&climb),
        ..shot(&info, &p, &readings)
    };
    let knob = screen(&climbing).knob.unwrap();
    assert_eq!(knob.target, Some(120));
    assert!(
        knob.raw.is_some_and(|raw| raw < 120),
        "a climb has steps to go"
    );
    assert!(
        knob.sub.as_deref().is_some_and(|s| s.starts_with("to −")),
        "{:?}",
        knob.sub
    );
    assert!(
        knob.notes[0].text.starts_with("Climbing to −"),
        "{}",
        knob.notes[0].text
    );
    assert!(knob.aria.contains(", climbing to −"), "{}", knob.aria);
}

/// The climb's note comes and goes with the climb, so it is a popup, not a line under the dial;
/// the cap's and the EQ's stay in place; a device with a tone control keeps room for the EQ's.
#[test]
fn only_the_climb_note_is_a_popup_and_an_eq_device_keeps_room() {
    let p = profile_named("gungnir2-v2");
    let mut readings = tone_on(&p, "Bass");
    readings.push(read(&p, "volume", "110"));
    readings.push(read(&p, "mute", "off"));
    let info = info("Gungnir 2");
    let climb = ClimbView {
        key: "volume".into(),
        target: 120,
    };
    let climbing = Snapshot {
        climb: Some(&climb),
        cap_db: Some(-10.0),
        ..shot(&info, &p, &readings)
    };
    let knob = screen(&climbing).knob.unwrap();
    let popups: Vec<&str> = knob
        .notes
        .iter()
        .filter(|n| n.popup)
        .map(|n| n.text.as_str())
        .collect();
    assert_eq!(popups.len(), 1, "{popups:?}");
    assert!(popups[0].starts_with("Climbing to −"), "{popups:?}");
    assert_eq!(knob.notes.len(), 3, "the climb, the EQ peak and the cap");
    assert_eq!(
        knob.notes.iter().filter(|n| n.eq).count(),
        1,
        "the EQ's note has the slot"
    );
    assert!(knob.room);
    let freya = profile_named("freya2");
    let readings = [read(&freya, "volume", "40")];
    let info = DeviceInfo {
        name: "Freya 2".into(),
        ..info
    };
    let knob = screen(&shot(&info, &freya, &readings)).knob.unwrap();
    assert!(
        !knob.room,
        "a device without a tone control reserves nothing"
    );
}

/// The amp leads with its meters and its power; a meter reads its fraction on its scale; the
/// state line says why there are no readings.
#[test]
fn the_aegir_leads_with_its_meters_and_its_power() {
    let p = profile_named("aegir2");
    let info = info("Aegir 2");
    let readings = vec![read(&p, "meter_1", "200"), read(&p, "meter_2", "160")];
    let s = screen(&shot(&info, &p, &readings));
    assert!(s.amp);
    assert_eq!(s.family, "amp");
    let meters = s.meters.expect("meters");
    assert_eq!(meters.arches.len(), 2);
    let left = &meters.arches[0];
    assert!(
        left.fraction.is_some_and(|f| f > 0.0 && f < 1.0),
        "{:?}",
        left.fraction
    );
    assert_eq!(meters.arches[1].text, "0");
    // The arch's segments and their classes are the page's to draw from the class-A share.
    assert!(
        left.class_a > 0.0 && left.class_a < meters::RED_FROM,
        "{}",
        left.class_a
    );
    assert!(s.power.is_some());
    let off = vec![read(&p, "power", "off")];
    let s = screen(&shot(&info, &p, &off));
    assert_eq!(s.meters.unwrap().state, Some("Standby: no readings"));
    let gone = DeviceInfo {
        connected: false,
        retry_in_secs: Some(12),
        ..info.clone()
    };
    let s = screen(&shot(&gone, &p, &readings));
    assert_eq!(s.meters.unwrap().state, Some("No readings without a link"));
}

/// Tyr has no volume: it leads with its meter, and its warming-up readout shows only while it is
/// powered (`only_while`).
#[test]
fn the_tyr_has_no_volume_and_a_gated_readout() {
    let p = profile_named("tyr");
    let info = info("Tyr");
    let readings = vec![read(&p, "power", "off"), read(&p, "warming_up", "true")];
    let s = screen(&shot(&info, &p, &readings));
    assert!(s.knob.is_none() && s.amp);
    let power = s.power.expect("power");
    assert_eq!(power.word, "Standby");
    let warming = power
        .readouts
        .iter()
        .find(|c| c.key() == "warming_up")
        .expect("the readout");
    assert!(
        matches!(warming, ControlView::Readout { text, .. } if text == "no"),
        "{warming:?}"
    );
}

/// A lost link is a card with the wait and Try now; a fault is a card with its advice; a
/// refused passphrase and a device never seen are cards too.
#[test]
fn the_states_are_cards() {
    let p = profile_named("aegir2");
    let lost = DeviceInfo {
        connected: false,
        retry_in_secs: Some(12),
        ..info("Aegir 2")
    };
    let seen = vec![read(&p, "fault_overtemp", "false")];
    let card = screen(&shot(&lost, &p, &seen)).card.expect("a card");
    assert_eq!(
        (card.kind, card.title.as_str()),
        ("warn", "Aegir 2 is out of reach")
    );
    assert_eq!(card.detail, "Not connected. Next attempt in 12 s.");
    assert_eq!((card.retry_in, card.try_now), (Some(12), true));
    // While an attempt is under way there is no wait to cut short: no Try now.
    let trying = DeviceInfo {
        retry_in_secs: None,
        ..lost.clone()
    };
    let card = screen(&shot(&trying, &p, &seen)).card.expect("a card");
    assert_eq!((card.retry_in, card.try_now), (None, false));
    let info = info("Aegir 2");
    let fault = vec![read(&p, "fault_overtemp", "true")];
    let card = screen(&shot(&info, &p, &fault)).card.expect("a card");
    assert_eq!((card.kind, card.title.as_str()), ("fault", "Fault"));
    let lines: Vec<(&str, bool)> = card
        .faults
        .iter()
        .map(|f| {
            (
                f.label.as_str(),
                f.advice.starts_with("Switch the device off"),
            )
        })
        .collect();
    assert_eq!(lines, [("Over-temperature", true)]);
    let refused = DeviceInfo {
        refused: true,
        ..info.clone()
    };
    assert_eq!(screen(&shot(&refused, &p, &[])).card.unwrap().kind, "dim");
    let unseen = Snapshot {
        profile: None,
        ..shot(&info, &p, &[])
    };
    let card = screen(&unseen).card.unwrap();
    assert_eq!(card.title, "Looking for Aegir 2…");
}

/// A configured device that has never connected is "not seen yet" in the dim card, whatever a
/// retry countdown says and even when an advertisement named its profile: not "out of reach",
/// which says a link was lost. It draws no knob, and no Try now.
#[test]
fn a_device_never_connected_is_not_seen_yet_not_out_of_reach() {
    let p = profile_named("gungnir2-v2");
    let waiting = DeviceInfo {
        connected: false,
        retry_in_secs: Some(12),
        ..info("Gungnir 2")
    };
    for profile in [None, Some(&p)] {
        let s = screen(&Snapshot {
            profile,
            ..shot(&waiting, &p, &[])
        });
        let card = s.card.expect("a card");
        assert_eq!(
            (card.kind, card.title.as_str(), card.try_now),
            ("dim", "Looking for Gungnir 2…", true),
            "{:?}",
            profile.map(|p| &p.name)
        );
        assert_eq!(card.retry_in, Some(12));
        assert!(
            card.detail.ends_with("Next attempt in 12 s."),
            "{}",
            card.detail
        );
        assert!(s.knob.is_none() && s.controls.is_empty());
    }
}

/// After a daemon restart a device that had a link has its profile (from an advertisement) and
/// no readings yet, and no link: it reads "Looking for …" like one never seen, and while a
/// retry is scheduled the card carries the countdown and Try now, which a guest does not get.
#[test]
fn a_device_after_a_restart_looks_for_the_device_and_keeps_try_now() {
    let p = profile_named("gungnir2-v2");
    let waiting = DeviceInfo {
        connected: false,
        retry_in_secs: Some(9),
        ..info("Gungnir 2")
    };
    let card = screen(&shot(&waiting, &p, &[])).card.expect("a card");
    assert_eq!(
        (card.kind, card.title.as_str()),
        ("dim", "Looking for Gungnir 2…")
    );
    assert_eq!((card.retry_in, card.try_now), (Some(9), true));
    assert!(
        card.detail.contains("Next attempt in 9 s."),
        "{}",
        card.detail
    );
    let guest = Snapshot {
        access: Access {
            writes: false,
            guest: true,
        },
        ..shot(&waiting, &p, &[])
    };
    assert!(!screen(&guest).card.unwrap().try_now);
    // Between attempts is not the only time: an attempt under way has no countdown and no Try now.
    let trying = DeviceInfo {
        retry_in_secs: None,
        ..waiting
    };
    let card = screen(&shot(&trying, &p, &[])).card.unwrap();
    assert_eq!((card.retry_in, card.try_now), (None, false));
    assert!(!card.detail.contains("Next attempt"), "{}", card.detail);
}

/// A daemon with no device says so.
#[test]
fn a_daemon_with_no_device_has_a_card() {
    assert_eq!(
        no_devices().detail,
        "No devices yet. `svolder setup` adds one."
    );
}

/// A guest and a page with writes off see every control inert, and no Try now.
#[test]
fn a_guest_and_a_read_only_page_see_the_controls_inert() {
    let p = profile_named("gungnir2-v2");
    let lost = DeviceInfo {
        connected: false,
        retry_in_secs: Some(5),
        ..info("Gungnir 2")
    };
    let seen = vec![read(&p, "volume", "40")];
    for access in [
        Access {
            writes: false,
            guest: true,
        },
        Access {
            writes: false,
            guest: false,
        },
    ] {
        let s = screen(&Snapshot {
            access,
            ..shot(&lost, &p, &seen)
        });
        assert!(s.inert, "{access:?}");
        assert!(!s.card.unwrap().try_now, "{access:?}");
    }
}

/// Every profile the tests name builds a screen.
#[test]
fn every_profile_builds_a_screen() {
    for name in ["gungnir2-v2", "aegir2", "tyr", "freya2", "fake-five"] {
        let p = profile_named(name);
        let info = info(name);
        let _ = screen(&shot(&info, &p, &[]));
    }
}

/// A mark that would sit within ten degrees of one kept before it is dropped by the page, which
/// spaces the marks round the dial (`ember-parts.mjs` holds the case `a_label_too_close_to_another_is_dropped`): on a dial that stops
/// at −9 dB both the −10 mark and the top's are candidates.
#[test]
fn a_dial_keeps_every_candidate_mark_for_the_page_to_space() {
    let mut p = profile_named("gungnir2-v2");
    let info = info("Gungnir 2");
    let volume = p.controls.iter_mut().find(|c| c.key == "volume").unwrap();
    volume.range = Some((0, 100));
    let knob = screen(&shot(&info, &p, &[])).knob.unwrap();
    let mut marks = knob.marks.clone();
    marks.sort_by(|a, b| a.0.total_cmp(&b.0));
    let labels: Vec<&str> = marks.iter().map(|m| m.1.as_str()).collect();
    assert_eq!(labels, ["−∞", "−60", "−40", "−20", "−10", "−9"]);
}

/// The presets' status line: the preset on; one recalled and then edited; Flat with none on; no
/// match; and a tone control still being read. Rows mark the one on, built-in or yours.
#[test]
fn the_presets_status_line_follows_the_curve_and_the_memory() {
    let p = profile_named("gungnir2-v2");
    let eq = p.eq.clone().unwrap();
    let fitting = preset::fitting(&eq, &[]);
    let info = info("Gungnir 2");
    let status = |readings: &[Message], recalled: Option<&str>| {
        let info = DeviceInfo {
            recalled: recalled.map(str::to_string),
            ..info.clone()
        };
        let s = screen(&Snapshot {
            presets: &fitting,
            ..shot(&info, &p, readings)
        });
        let presets = s.presets.expect("presets");
        (
            presets.status.lead.to_string(),
            presets.status.name,
            presets.status.tail.to_string(),
            presets.rows,
        )
    };
    let (lead, name, _, rows) = status(&tone_on(&p, "Warm"), None);
    assert_eq!((lead.as_str(), name.as_deref()), ("On: ", Some("Warm")));
    assert!(rows.iter().find(|r| r.name == "Warm").unwrap().on);
    assert!(rows.iter().all(|r| r.built_in));
    let mut edited = tone_on(&p, "Warm");
    let gain = eq.band_id(0, EqParam::Gain).unwrap();
    edited.retain(|m| m.id != gain);
    edited.push(Message::status(gain, svolder_proto::Value::U8(120)));
    let (lead, name, tail, _) = status(&edited, Some("Warm"));
    assert_eq!((lead.as_str(), name.as_deref()), ("", Some("Warm")));
    assert_eq!(tail, ", edited. Save the bands to keep them.");
    let (lead, ..) = status(&edited, None);
    assert_eq!(lead, "No preset matches these bands.");
    let (lead, ..) = status(&[], None);
    assert_eq!(lead, "Reading the tone control…");
}

/// The band table reads every band in the one wording, with the page's minus; in Flat the rows
/// are faint and the curve lies flat with the bands as a ghost; the notes ride in the model, empty
/// in EQ (`ember-parts.mjs` holds the same name for the markup that shows them).
#[test]
fn the_bands_and_the_curve_read_the_tone_control() {
    let p = profile_named("gungnir2-v2");
    let info = info("Gungnir 2");
    let mut readings = tone_on(&p, "Bass");
    let s = screen(&shot(&info, &p, &readings));
    let eq_model = model_of(&s);
    let bands = s.bands.expect("bands");
    assert_eq!(bands.rows.len(), 5);
    assert_eq!(bands.rows[0].band, 1);
    assert!(bands.rows[0].f.ends_with(" Hz") && bands.rows[0].g.ends_with(" dB"));
    assert!(!bands.faint);
    let tone = s.tone.expect("a tone control");
    assert_eq!(tone.curve.response.as_ref().map(Vec::len), Some(181));
    assert!(tone.curve.peak.is_some());
    let mode = p.eq.as_ref().unwrap().mode_id();
    readings.retain(|m| m.id != mode);
    readings.push(
        svolder_profiles::eq_mode(&p, EqMode::Flat)
            .map(|m| Message::status(m.id, m.value))
            .unwrap(),
    );
    let s = screen(&shot(&info, &p, &readings));
    let flat_model = model_of(&s);
    assert!(s.bands.unwrap().faint);
    let tone = s.tone.unwrap();
    // Flat keeps the bands' curve: the page draws the flat line and this one as the ghost.
    assert_eq!(tone.curve.response.as_ref().map(Vec::len), Some(181));
    assert!(tone.curve.peak.is_none());
    let loud = tone_on(&p, "Bass");
    let mut loud: Vec<_> = loud.into_iter().filter(|m| m.id != mode).collect();
    loud.push(
        svolder_profiles::eq_mode(&p, EqMode::Loudness)
            .map(|m| Message::status(m.id, m.value))
            .unwrap(),
    );
    let s = screen(&shot(&info, &p, &loud));
    let loud_model = model_of(&s);
    assert_eq!(s.tone.unwrap().mode, Some("loudness"));
    // The notes take no room in the pane: the model carries them, none in EQ, and the page shows
    // them as a message when the mode changes.
    assert_eq!(eq_model["tone"]["mode"], "eq");
    assert!(eq_model["tone"]["note"].is_null(), "{eq_model}");
    assert_eq!(
        flat_model["tone"]["note"],
        "Flat bypasses the bands; the faint line is what EQ would do. \
         Step a band and the mode switches to EQ."
    );
    assert_eq!(
        loud_model["tone"]["note"],
        "Loudness follows the volume: it adds more the lower you listen, and nothing at full \
         volume."
    );
    assert_eq!(flat_model["tone"]["bands"]["faint"], true);
}

/// A tone control read as `bands` ((Hz, dB, Q) per band) in `mode`; `mode` `None` leaves it unread.
fn read_bands(p: &Profile, mode: Option<EqMode>, bands: &[(f64, f64, f64)]) -> Vec<Message> {
    let eq = p.eq.as_ref().expect("a tone control");
    let mut readings = Vec::new();
    for (band, (hz, db, q)) in bands.iter().enumerate() {
        for (param, raw) in [
            (EqParam::Frequency, eq.frequency_raw(band, *hz).unwrap()),
            (EqParam::Gain, eq.gain_raw(*db)),
            (EqParam::Q, eq.q_raw(*q)),
        ] {
            readings.push(Message::status(
                eq.band_id(band, param).unwrap(),
                Value::U8(raw),
            ));
        }
    }
    if let Some(mode) = mode {
        let m = svolder_profiles::eq_mode(p, mode).expect("a mode");
        readings.push(Message::status(m.id, m.value));
    }
    readings
}

/// A band's point is at its own frequency and gain, not on the summed curve: with two boosting
/// bands on top of each other each sits at its own level in EQ and in Flat, and at 0 dB in
/// Loudness and while the mode is unread (`ember-parts.mjs` holds the same name for the height
/// the page draws it at).
#[test]
fn the_points_sit_at_each_bands_own_gain() {
    let p = profile_named("gungnir2-v2");
    let info = info("Gungnir 2");
    let eq = p.eq.as_ref().unwrap();
    let bands = [
        (60.0, 0.0, 1.0),
        (1000.0, 6.0, 0.7),
        (1100.0, 9.0, 0.7),
        (4000.0, -4.0, 1.0),
        (12000.0, 0.0, 1.0),
    ];
    let levels = |mode: Option<EqMode>| -> Vec<f64> {
        let readings = read_bands(&p, mode, &bands);
        let s = screen(&shot(&info, &p, &readings));
        let markers = s.tone.unwrap().curve.markers;
        assert_eq!(markers.len(), 5);
        assert_eq!(
            markers.iter().map(|m| m.band).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        markers.into_iter().map(|m| m.db).collect()
    };
    let own: Vec<f64> = bands
        .iter()
        .map(|(_, db, _)| eq.gain_db(eq.gain_raw(*db)))
        .collect();
    assert_eq!(levels(Some(EqMode::Eq)), own);
    assert_eq!(levels(Some(EqMode::Flat)), own);
    // The summed curve is higher than either boost at band 2's centre: the point is not there.
    let readings = read_bands(&p, Some(EqMode::Eq), &bands);
    let summed = svolder_profiles::tone::response::bands_of(eq, |id| {
        readings
            .iter()
            .find(|m| m.id == id)
            .and_then(|m| m.value.as_i64())
    })
    .unwrap();
    let at = svolder_profiles::tone::response::response_db(&summed, &[summed[1].f0])[0];
    assert_ne!(own[1], at, "band 2 sits on the summed curve");
    assert!(levels(Some(EqMode::Loudness)).iter().all(|db| *db == 0.0));
    assert!(levels(None).iter().all(|db| *db == 0.0));
}

/// A band that has not reported all three of its values draws no point; the others do.
#[test]
fn a_band_not_read_in_full_draws_no_point() {
    let p = profile_named("gungnir2-v2");
    let info = info("Gungnir 2");
    let eq = p.eq.as_ref().unwrap();
    let bands = [(100.0, 3.0, 1.0); 5];
    let mut readings = read_bands(&p, Some(EqMode::Eq), &bands);
    let q = eq.band_id(2, EqParam::Q).unwrap();
    readings.retain(|m| m.id != q);
    let s = screen(&shot(&info, &p, &readings));
    let markers = s.tone.unwrap().curve.markers;
    assert_eq!(
        markers.iter().map(|m| m.band).collect::<Vec<_>>(),
        [1, 2, 4, 5]
    );
}

/// The model carries the raw readings and the mode for the graph: five `[f, g, q]` triples, a
/// value not read as `null`, and no mode as `null`; the page's graph holds them as an attribute
/// (`ember-parts.mjs` holds the same name).
#[test]
fn the_readings_carry_the_raw_bands_and_the_mode() {
    let p = profile_named("gungnir2-v2");
    let info = info("Gungnir 2");
    let eq = p.eq.as_ref().unwrap();
    let bands = [
        (60.0, 2.0, 1.0),
        (250.0, -3.0, 0.7),
        (1000.0, 6.0, 0.7),
        (4000.0, -4.0, 2.0),
        (12000.0, 0.5, 1.0),
    ];
    let parsed = |readings: &[Message]| -> serde_json::Value {
        model_of(&screen(&shot(&info, &p, readings)))["tone"]["readings"].clone()
    };
    let all = read_bands(&p, Some(EqMode::Eq), &bands);
    let want: Vec<_> = bands
        .iter()
        .enumerate()
        .map(|(b, (hz, db, q))| {
            serde_json::json!([
                eq.frequency_raw(b, *hz).unwrap(),
                eq.gain_raw(*db),
                eq.q_raw(*q)
            ])
        })
        .collect();
    assert_eq!(
        parsed(&all),
        serde_json::json!({ "mode": "eq", "bands": want })
    );
    // The mode's names are the writes' own.
    for (mode, name) in [(EqMode::Flat, "flat"), (EqMode::Loudness, "loudness")] {
        assert_eq!(parsed(&read_bands(&p, Some(mode), &bands))["mode"], name);
    }
    // A band missing its Q reads null there and nowhere else.
    let q = eq.band_id(3, EqParam::Q).unwrap();
    let no_q: Vec<_> = all.iter().filter(|m| m.id != q).cloned().collect();
    let got = parsed(&no_q);
    assert_eq!(got["bands"][3][2], serde_json::Value::Null, "{got}");
    assert_eq!(got["bands"][3][0], want[3][0], "{got}");
    assert_eq!(got["bands"][3][1], want[3][1], "{got}");
    assert_eq!(got["bands"][2], want[2], "{got}");
    // No mode read.
    let got = parsed(&read_bands(&p, None, &bands));
    assert_eq!(got["mode"], serde_json::Value::Null, "{got}");
    assert_eq!(got["bands"], serde_json::Value::Array(want));
}

/// The step tables hold every raw step of every band with the core's value and its words in the
/// page's minus, and cannot close their `<script>` element.
#[test]
fn the_step_tables_hold_every_step_with_the_cores_words() {
    let p = profile_named("gungnir2-v2");
    let eq = p.eq.as_ref().unwrap();
    let json = tone::steps_json(eq);
    assert!(!json.contains('<'), "{json}");
    let steps: serde_json::Value = serde_json::from_str(&json).expect("JSON");
    let bands = steps["bands"].as_array().unwrap();
    assert_eq!(bands.len(), 5);
    assert_eq!(steps["gain"].as_array().unwrap().len(), 128);
    assert_eq!(steps["q"].as_array().unwrap().len(), 128);
    let words = |band: usize, param: EqParam, raw: usize| {
        minus(&units::band_text(eq, band, param, raw as i64).unwrap())
    };
    for (band, entry) in bands.iter().enumerate() {
        assert_eq!(entry["f"].as_array().unwrap().len(), 128);
        for raw in [0_usize, 64, 127] {
            let f = &entry["f"][raw];
            assert_eq!(f[0].as_f64(), eq.frequency(band, raw as u8), "{f}");
            assert_eq!(f[1], words(band, EqParam::Frequency, raw).as_str());
            let g = &steps["gain"][raw];
            assert_eq!(g[0].as_f64(), Some(eq.gain_db(raw as u8)), "{g}");
            assert_eq!(g[1], words(band, EqParam::Gain, raw).as_str());
            let q = &steps["q"][raw];
            assert_eq!(q[0].as_f64(), Some(eq.q(raw as u8)), "{q}");
            assert_eq!(q[1], words(band, EqParam::Q, raw).as_str());
        }
    }
    // The page's minus is in the words, not a hyphen.
    assert!(steps["gain"][0][1].as_str().unwrap().starts_with('−'));
}

/// The model carries the step tables once, where the tone part is drawn (`ember-parts.mjs` holds
/// the case `the_screen_carries_the_step_tables_where_the_tone_part_is` for the data block); an amp's layout, which has no tone part, carries none.
#[test]
fn the_model_carries_the_step_tables_where_the_tone_part_is() {
    let amp = info("Aegir 2");
    let p = profile_named("gungnir2-v2");
    let info = info("Gungnir 2");
    let model = model_of(&screen(&shot(&info, &p, &[])));
    assert_eq!(
        model["tone_steps"]["bands"].as_array().map(Vec::len),
        Some(5)
    );
    assert!(model["tone"].is_object(), "{model}");
    let aegir = profile_named("aegir2");
    let model = model_of(&screen(&shot(&amp, &aegir, &[])));
    assert!(
        model["tone_steps"].is_null() && model["tone"].is_null(),
        "{model}"
    );
}

/// A guest's tone part, and one on a daemon with writes off, is marked inert for the page, which
/// then does not let a point be grabbed; a writer's is not (`ember-parts.mjs` holds the same name
/// for the attribute).
#[test]
fn a_guests_tone_part_is_marked_inert() {
    let p = profile_named("gungnir2-v2");
    let info = info("Gungnir 2");
    let readings = read_bands(&p, Some(EqMode::Eq), &[(100.0, 3.0, 1.0); 5]);
    let open = shot(&info, &p, &readings);
    let model = model_of(&screen(&open));
    assert!(
        model["tone"].is_object() && model["inert"] == false,
        "{model}"
    );
    for access in [
        Access {
            writes: false,
            guest: true,
        },
        Access {
            writes: false,
            guest: false,
        },
        Access {
            writes: true,
            guest: true,
        },
    ] {
        let shut = Snapshot { access, ..open };
        let model = model_of(&screen(&shut));
        assert!(model["tone"].is_object(), "{model}");
        assert_eq!(model["inert"], true, "{access:?}: {model}");
    }
}

/// Loudness's strength is stepped by a round - and + on the Loudness button, inside the mode track
/// (`ember-parts.mjs` holds the case `loudness_steps_on_its_own_button_and_no_row_follows` for the buttons): the model says the strength only in
/// Loudness and only once it is read, and Flat and EQ have none.
#[test]
fn the_loudness_strength_is_in_the_model_only_in_loudness_once_read() {
    let p = profile_named("gungnir2-v2");
    let info = info("Gungnir 2");
    let eq = p.eq.as_ref().unwrap();
    let mode = eq.mode_id();
    let in_mode_with = |m: EqMode, strength: Option<u8>| {
        let mut readings: Vec<_> = tone_on(&p, "Bass")
            .into_iter()
            .filter(|r| r.id != mode)
            .collect();
        readings.push(
            svolder_profiles::eq_mode(&p, m)
                .map(|r| Message::status(r.id, r.value))
                .unwrap(),
        );
        if let Some(raw) = strength {
            readings.push(Message::status(eq.loudness_id(), Value::U8(raw)));
        }
        model_of(&screen(&shot(&info, &p, &readings)))["tone"].clone()
    };
    let in_mode = |m: EqMode| in_mode_with(m, None);
    let loud = in_mode(EqMode::Loudness);
    assert_eq!(loud["loud"], true, "{loud}");
    assert_eq!(loud["mode"], "loudness");
    for tone in [in_mode(EqMode::Flat), in_mode(EqMode::Eq)] {
        assert_eq!(tone["loud"], false, "{tone}");
    }
    // The strength's words show on the Loudness button once it is read, in Loudness only.
    assert!(loud["strength"].is_null(), "{loud}");
    let read = in_mode_with(EqMode::Loudness, Some(2));
    assert_eq!(read["strength"], "2", "{read}");
    for tone in [
        in_mode_with(EqMode::Flat, Some(2)),
        in_mode_with(EqMode::Eq, Some(2)),
    ] {
        assert!(tone["strength"].is_null(), "{tone}");
    }
}

/// A guest sees no preset editing; a configuration that cannot be written hides save, rename and
/// delete, keeps the recall, and says why.
#[test]
fn a_guest_and_a_read_only_config_cannot_edit_presets() {
    let p = profile_named("gungnir2-v2");
    let info = info("Gungnir 2");
    let fitting = preset::fitting(p.eq.as_ref().unwrap(), &[]);
    let guest = Snapshot {
        presets: &fitting,
        access: Access {
            writes: false,
            guest: true,
        },
        ..shot(&info, &p, &[])
    };
    let presets = screen(&guest).presets.unwrap();
    assert!(!presets.editable && !presets.recall);
    let locked = Err("config.toml is read-only (0444)".to_string());
    let s = screen(&Snapshot {
        presets: &fitting,
        writable: &locked,
        ..shot(&info, &p, &[])
    });
    let presets = s.presets.unwrap();
    assert!(!presets.editable && presets.recall);
    assert_eq!(
        presets.note.as_deref(),
        Some("Saving presets is off: config.toml is read-only (0444)")
    );
}

/// The strip: one bar a second of the volume's fraction, tinted by the sample rate, red where
/// the curve could clip, dim while muted; no bar while the link was down; marks for an input
/// change, a recall, a rate change and a lock.
#[test]
fn the_strip_draws_a_bar_a_second_and_marks_what_happened() {
    let p = profile_named("gungnir2-v2");
    let id = |key: &str| p.control_by_key(key).unwrap().id.to_string();
    let series = |key: &str, aggregate, points: Vec<(u64, f64)>| SeriesOut {
        id: id(key),
        key: Some(key.into()),
        aggregate,
        points,
    };
    let now_ms = 100_000;
    let all = vec![
        series(
            "volume",
            Aggregate::Max,
            vec![(90_000, 63.0), (95_000, 127.0)],
        ),
        series(
            "sample_rate",
            Aggregate::Last,
            vec![(90_000, 2.0), (94_000, 4.0)],
        ),
        series("input", Aggregate::Last, vec![(90_000, 1.0), (93_000, 3.0)]),
        series(
            "spdif_locked",
            Aggregate::Last,
            vec![(90_000, 0.0), (96_000, 1.0)],
        ),
        series("mute", Aggregate::Last, vec![(90_000, 0.0), (97_000, 1.0)]),
    ];
    let recalls = vec![RecallOut {
        at_ms: 92_500,
        name: "Warm".into(),
    }];
    let link = vec![
        LinkChange {
            at_ms: 10_000,
            connected: true,
        },
        LinkChange {
            at_ms: 98_000,
            connected: false,
        },
    ];
    let past = Past {
        now_ms,
        series: &all,
        recalls: &recalls,
        link: &link,
    };
    let strip = strip(&p, &past, 10);
    assert_eq!(strip.kind, "dac");
    let at = |t: u64| strip.bars.iter().find(|b| b.t == t);
    assert!(at(90).is_some_and(|b| (b.h - 63.0 / 127.0).abs() < 1e-9 && b.tint == "ramp"));
    assert_eq!(at(94).map(|b| b.tint), Some("gold"), "96 kHz");
    assert!(at(97).is_some_and(|b| b.dim), "muted");
    assert!(at(98).is_none() && at(99).is_none(), "the link was down");
    let marks: Vec<(u64, &str)> = strip.marks.iter().map(|m| (m.t, m.text.as_str())).collect();
    assert!(marks.contains(&(92, "Warm")), "{marks:?}");
    assert!(marks.contains(&(93, "Optical")), "{marks:?}");
    assert!(marks.contains(&(94, "96 kHz")), "{marks:?}");
    assert!(marks.contains(&(96, "locked")), "{marks:?}");
    assert_eq!(
        strip
            .marks
            .iter()
            .find(|m| m.text == "locked")
            .map(|m| m.tint),
        Some("good")
    );
}

/// The strip of the one second before `now_ms`, from series whose last points are all 10 s old
/// (what the ring answers for a span of 1 s over a device that has reported nothing since).
fn held_strip(p: &Profile, held: &[(&str, f64)], link: &[LinkChange]) -> Strip {
    let series: Vec<SeriesOut> = held
        .iter()
        .map(|(key, value)| SeriesOut {
            id: p.control_by_key(key).unwrap().id.to_string(),
            key: Some((*key).into()),
            aggregate: Aggregate::Last,
            points: vec![(90_000, *value)],
        })
        .collect();
    let past = Past {
        now_ms: 100_000,
        series: &series,
        recalls: &[],
        link,
    };
    strip(p, &past, 1)
}

/// A volume held for 10 s still draws the second's bar at its height, dim when it was muted
/// then; an amp's placing controls and meter held likewise draw one; a link that went down 10 s
/// ago draws none.
#[test]
fn a_held_reading_draws_the_bar_of_a_second_nothing_was_reported_in() {
    let p = profile_named("gungnir2-v2");
    let up = [LinkChange {
        at_ms: 1_000,
        connected: true,
    }];
    let held = held_strip(&p, &[("volume", 63.0)], &up);
    assert_eq!(held.bars.len(), 1, "{held:?}");
    assert_eq!(held.bars[0].t, 99);
    assert!((held.bars[0].h - 63.0 / 127.0).abs() < 1e-9);
    assert!(!held.bars[0].dim);
    let muted = held_strip(&p, &[("volume", 63.0), ("mute", 1.0)], &up);
    assert!(muted.bars[0].dim, "{muted:?}");
    let down = [
        LinkChange {
            at_ms: 1_000,
            connected: true,
        },
        LinkChange {
            at_ms: 90_000,
            connected: false,
        },
    ];
    assert!(held_strip(&p, &[("volume", 63.0)], &down).bars.is_empty());
    let amp = profile_named("wotan");
    let amp_held = held_strip(
        &amp,
        &[
            ("meter_floor_l", 20.0),
            ("meter_floor_r", 20.0),
            ("meter_1", 120.0),
        ],
        &up,
    );
    assert_eq!(amp_held.kind, "amp");
    assert_eq!(amp_held.bars.len(), 1, "{amp_held:?}");
    assert!(amp_held.bars[0].h > 0.0);
}

/// An input held for over 30 minutes and changed 3 s ago: the points the ring answers (the old
/// value from long before the span, then the change) draw the change's mark.
#[test]
fn an_input_changed_after_a_long_hold_is_marked() {
    let p = profile_named("gungnir2-v2");
    let now_ms = 10_000_000;
    let series = vec![SeriesOut {
        id: p.control_by_key("input").unwrap().id.to_string(),
        key: Some("input".into()),
        aggregate: Aggregate::Last,
        points: vec![(now_ms - 31 * 60_000, 1.0), (now_ms - 3_000, 3.0)],
    }];
    let link = [LinkChange {
        at_ms: 1_000,
        connected: true,
    }];
    let past = Past {
        now_ms,
        series: &series,
        recalls: &[],
        link: &link,
    };
    let marks: Vec<(u64, String)> = strip(&p, &past, 5)
        .marks
        .iter()
        .map(|m| (m.t, m.text.clone()))
        .collect();
    assert_eq!(marks, [(9_997, "Optical".to_string())]);
}

/// A preset's row carries the JSON its recall, rename and delete post, built by serde so a name
/// with quotes or markup stays one string.
#[test]
fn a_preset_row_posts_its_name_as_json() {
    let p = profile_named("gungnir2-v2");
    let info = info("Gungnir 2");
    let mut fitting = preset::fitting(p.eq.as_ref().unwrap(), &[]);
    fitting[1].0.name = "Say \"hi\" <b>".to_string();
    let s = screen(&Snapshot {
        presets: &fitting,
        ..shot(&info, &p, &[])
    });
    let row = &s.presets.unwrap().rows[1];
    assert_eq!(row.json, r#"{"name":"Say \"hi\" <b>"}"#);
    assert_eq!(row.rename_json, r#"{"from":"Say \"hi\" <b>"}"#);
}

/// The tone part's mode track and loudness stepper are in the model, and a guest's are inert. The
/// presets part posts a recall from each row, and offers rename, delete and save only on the
/// user's rows and only where the configuration can be written (`ember-parts.mjs` holds the case
/// `the_tone_and_preset_forms_post_and_a_guest_sees_none` for the markup).
#[test]
fn the_tone_and_preset_models_offer_forms_only_to_a_writer() {
    let p = profile_named("gungnir2-v2");
    let dac = info("Gungnir 2");
    let eq = p.eq.as_ref().unwrap();
    let mut fitting = preset::fitting(eq, &[]);
    fitting[1].0.name = "<b>Mine</b>".to_string();
    fitting[1].1 = false;
    let readings = [
        Message::status(eq.mode_id(), Value::U8(EqMode::Loudness as u8)),
        Message::status(eq.loudness_id(), Value::U8(40)),
    ];

    let open = Snapshot {
        presets: &fitting,
        ..shot(&dac, &p, &readings)
    };
    let model = model_of(&screen(&open));
    let modes: Vec<&str> = model["tone"]["modes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap())
        .collect();
    assert_eq!(modes, ["flat", "eq", "loudness"]);
    assert_eq!(model["tone"]["strength"], "40");
    let presets = &model["presets"];
    assert_eq!(presets["recall"], true);
    assert_eq!(presets["editable"], true);
    let user: Vec<&serde_json::Value> = presets["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["built_in"] == false)
        .collect();
    assert_eq!(
        user.len(),
        1,
        "rename and delete are on the user's rows only"
    );
    // The name is a string of the model, markup and all: the page escapes it when it draws it.
    assert_eq!(user[0]["name"], "<b>Mine</b>");
    let read_only = Snapshot {
        writable: &Err("read-only".to_string()),
        ..open
    };
    let presets = &model_of(&screen(&read_only))["presets"];
    assert_eq!(
        (presets["recall"].clone(), presets["editable"].clone()),
        (true.into(), false.into())
    );
    let guest = Snapshot {
        access: Access {
            writes: false,
            guest: true,
        },
        ..open
    };
    let model = model_of(&screen(&guest));
    assert_eq!(model["inert"], true);
    assert_eq!(model["presets"]["recall"], false);
    assert_eq!(model["presets"]["editable"], false);
}

/// An amp with nothing but meters is `lone` (the page centres them: `ember-parts.mjs` holds the
/// same name for the class it draws); one with a power button or controls is not.
#[test]
fn an_amp_with_only_meters_is_lone() {
    let full = profile_named("aegir2");
    let info = info("Aegir 2");
    assert!(!screen(&shot(&info, &full, &[])).lone);
    let mut meters = full.clone();
    meters.controls.retain(|c| c.kind == ControlKind::Meter);
    let s = screen(&shot(&info, &meters, &[]));
    assert!(
        s.amp && s.power.is_none() && s.controls.is_empty(),
        "the staged profile"
    );
    assert!(s.lone);
    assert_eq!(model_of(&s)["lone"], true);
}

/// A lost link dims the view and makes its columns inert; a fault's card leaves the view as it
/// is, in use (the meters and the power button are what the fault needs); `ember-parts.mjs`
/// holds the same name for the classes the page draws them with.
#[test]
fn a_lost_link_dims_and_locks_the_view_and_a_fault_leaves_it_alone() {
    let p = profile_named("aegir2");
    let lost = DeviceInfo {
        connected: false,
        retry_in_secs: Some(12),
        ..info("Aegir 2")
    };
    let seen = vec![read(&p, "fault_overtemp", "false")];
    let model = model_of(&screen(&shot(&lost, &p, &seen)));
    assert_eq!(model["card"]["kind"], "warn", "{model}");
    assert_eq!(
        (model["away"].clone(), model["locked"].clone()),
        (true.into(), true.into())
    );
    let fault = vec![read(&p, "fault_overtemp", "true")];
    let model = model_of(&screen(&shot(&info("Aegir 2"), &p, &fault)));
    assert_eq!(model["card"]["kind"], "fault", "{model}");
    assert_eq!(
        (model["away"].clone(), model["locked"].clone()),
        (false.into(), false.into())
    );
    let refused = DeviceInfo {
        refused: true,
        ..info("Aegir 2")
    };
    let model = model_of(&screen(&shot(&refused, &p, &[])));
    assert_eq!(model["away"], true, "{model}");
    assert_eq!(model["locked"], false, "{model}");
}

/// A page whose writes switch off, or on, is sent its screen again: the shape token changes with
/// the inert state, so a kept knob and sliders are not left looking as they did.
#[test]
fn the_shape_changes_when_the_page_turns_inert() {
    let p = profile_named("gungnir2-v2");
    let seen = vec![read(&p, "volume", "40")];
    let open = screen(&shot(&info("Gungnir 2"), &p, &seen)).shape;
    let read_only = screen(&Snapshot {
        access: Access {
            writes: false,
            guest: false,
        },
        ..shot(&info("Gungnir 2"), &p, &seen)
    })
    .shape;
    assert_ne!(open, read_only);
}

/// A cap below every step says so in plain words, not "capped at" a level the knob never shows.
#[test]
fn a_cap_below_every_step_is_worded_as_such() {
    let p = profile_named("freya2");
    let readings = [read(&p, "volume", "40")];
    let info = info("Freya 2");
    let capped = Snapshot {
        cap_db: Some(-200.0),
        ..shot(&info, &p, &readings)
    };
    let knob = screen(&capped).knob.unwrap();
    assert_eq!(knob.cap, None);
    let texts: Vec<&str> = knob.notes.iter().map(|n| n.text.as_str()).collect();
    assert!(
        texts.contains(
            &"[web] max_volume_db (−200.0 dB) is below every step: \
              the quietest step is the most it allows"
        ),
        "{texts:?}"
    );
    assert!(
        texts.iter().all(|t| !t.starts_with("Capped at")),
        "{texts:?}"
    );
}

/// Every active fault is in the one card, for every family: an amp with two faults, an amp with
/// no power toggle, and a DAC. The title counts them and each line carries the profile's advice.
#[test]
fn card_lists_every_active_fault() {
    let on = |p: &Profile| {
        vec![
            read(p, "fault_overcurrent", "true"),
            read(p, "fault_overtemp", "true"),
        ]
    };
    let amp = profile_named("aegir2");
    let card = screen(&shot(&info("Aegir 2"), &amp, &on(&amp)))
        .card
        .expect("a card");
    assert_eq!((card.kind, card.title.as_str()), ("fault", "2 faults"));
    let lines: Vec<&str> = card.faults.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(lines, ["Over-current", "Over-temperature"]);
    assert!(
        card.faults.iter().all(|f| !f.advice.is_empty()),
        "{:?}",
        card.faults
    );
    // One fault is "Fault", and a clear one is no fault.
    let one = vec![
        read(&amp, "fault_overcurrent", "false"),
        read(&amp, "fault_overtemp", "true"),
    ];
    let card = screen(&shot(&info("Aegir 2"), &amp, &one)).card.unwrap();
    assert_eq!(card.title, "Fault");
    assert_eq!(card.faults.len(), 1);
    // An amp with no power toggle shows its faults too.
    let bare = profile_named("fake-five");
    assert!(bare.controls.iter().all(|c| !c.powers_output));
    let card = screen(&shot(&info("Five"), &bare, &on(&bare)))
        .card
        .expect("a card");
    assert_eq!(card.faults.len(), 2, "{card:?}");
    // A DAC's card lists them as well.
    let dac = Profile {
        family: Family::Dac,
        ..profile_named("aegir2")
    };
    let screen = screen(&shot(&info("DAC"), &dac, &on(&dac)));
    assert_eq!(screen.card.expect("a card").faults.len(), 2);
    // The model carries one line a fault, which the page draws as they come
    // (`ember-parts.mjs` holds the same name).
    let model = model_of(&self::screen(&shot(&info("Aegir 2"), &amp, &on(&amp))));
    assert_eq!(model["card"]["title"], "2 faults", "{model}");
    assert_eq!(model["card"]["faults"].as_array().map(Vec::len), Some(2));
}

/// The Power section no longer lists faults: the card does.
#[test]
fn power_view_has_no_fault_rows() {
    let p = profile_named("aegir2");
    let readings = vec![
        read(&p, "fault_overcurrent", "true"),
        read(&p, "fault_overtemp", "true"),
    ];
    let s = screen(&shot(&info("Aegir 2"), &p, &readings));
    let power = s.power.expect("power");
    for row in &power.readouts {
        assert!(
            !matches!(row.key(), "fault_overcurrent" | "fault_overtemp"),
            "{row:?}"
        );
    }
    assert!(power.readouts.iter().any(|r| r.key() == "warming_up"));
}

/// The All readings table lists every reading the device holds, in function order: a named one
/// with its profile label and words, an unnamed one with `?` and its raw value; the model carries
/// the clock and each reading's stamp for the page to age them (`ember-parts.mjs` holds the same
/// name for the table).
#[test]
fn facts_list_every_reading_named_or_not() {
    let p = profile_named("gungnir2-v2");
    let readings = vec![
        read(&p, "input", "3"),
        Message::status(FunctionId::three(210, 2, 2), Value::U16(9)),
    ];
    let stamps = BTreeMap::from([
        (readings[0].id, NOW_MS - 7_000),
        (FunctionId::three(210, 2, 2), NOW_MS - 120_000),
    ]);
    let info = info("Gungnir 2");
    let s = screen(&Snapshot {
        heard_at: &stamps,
        ..shot(&info, &p, &readings)
    });
    let rows: Vec<(&str, &str, Option<u64>)> = s
        .facts
        .rows
        .iter()
        .map(|r| (r.id.as_str(), r.label.as_str(), r.heard_at_ms))
        .collect();
    assert_eq!(
        rows,
        [
            (
                readings[0].id.to_string().as_str(),
                "Input",
                Some(NOW_MS - 7_000)
            ),
            ("210.2.2", "?", Some(NOW_MS - 120_000)),
        ]
    );
    assert_eq!(s.facts.rows[1].value, "9");
    assert_eq!(s.facts.now_ms, NOW_MS);
    let facts = &model_of(&s)["facts"];
    assert_eq!(facts["now_ms"], 1_700_000_100_000_u64);
    assert_eq!(facts["rows"][0]["heard_at_ms"], 1_700_000_093_000_u64);
    assert_eq!(facts["rows"][1]["id"], "210.2.2");
    assert_eq!(facts["rows"][1]["label"], "?");
}

/// A device echoes a passphrase change with a status on function 250 that carries the new code
/// in its encoded form. The All readings table lists every other reading, but never that one: not
/// in the part, not anywhere in the model the page is sent, for the owner or for a guest.
#[test]
fn the_passphrase_never_reaches_a_remote_view_in_the_facts() {
    let p = profile_named("gungnir2-v2");
    let encoded: u32 = 0x00c0_ffee;
    let readings = vec![
        read(&p, "input", "3"),
        Message::status(svolder_proto::function::PASSPHRASE, Value::U32(encoded)),
    ];
    let info = info("Gungnir 2");
    let guest = Access {
        writes: false,
        guest: true,
    };
    for access in [OPEN, guest] {
        let s = screen(&Snapshot {
            access,
            ..shot(&info, &p, &readings)
        });
        let ids: Vec<&str> = s.facts.rows.iter().map(|r| r.id.as_str()).collect();
        assert!(!ids.iter().any(|id| id.starts_with("250")), "{ids:?}");
        assert!(
            !s.facts.rows.iter().any(|r| r.value == encoded.to_string()),
            "{:?}",
            s.facts.rows
        );
        let sent = crate::web::api::embed(&crate::web::model::device_model(&s));
        assert!(
            !sent.contains("\"250.") && !sent.contains(&encoded.to_string()),
            "{sent}"
        );
        assert!(!s.facts.key().contains(&encoded.to_string()));
    }
}

/// Firmware and DSP are the builds the device read on 210.2.1 and 210.2.2; before a reading the
/// firmware is the advertised build, said so, and the DSP is `null` for the page to dash
/// (`ember-parts.mjs` holds the same name).
#[test]
fn facts_show_firmware_and_dsp_when_read_and_dash_when_not() {
    let p = profile_named("gungnir2-v2");
    let info = info("Gungnir 2");
    let read_fw = vec![
        Message::status(FunctionId::three(210, 2, 1), Value::U16(11)),
        Message::status(FunctionId::three(210, 2, 2), Value::U16(4)),
    ];
    let s = screen(&shot(&info, &p, &read_fw));
    assert_eq!((s.facts.firmware.as_str(), s.facts.dsp), ("11", Some(4)));
    let facts = &model_of(&s)["facts"];
    assert_eq!(
        (facts["firmware"].clone(), facts["dsp"].clone()),
        ("11".into(), 4.into())
    );
    let s = screen(&shot(&info, &p, &[]));
    assert_eq!(
        (s.facts.firmware.as_str(), s.facts.dsp),
        ("3 (advertised)", None)
    );
    let facts = &model_of(&s)["facts"];
    assert_eq!(facts["firmware"], "3 (advertised)");
    assert!(facts["dsp"].is_null(), "{facts}");
}

/// A session over the fake asks for the firmware versions on connect, and what it reads is
/// what the facts show: the device's own build, no longer the advertised one.
#[tokio::test(start_paused = true)]
async fn the_firmware_a_connect_asks_for_replaces_the_advertised_build_in_the_facts() {
    let p = profile_named("gungnir2-v2");
    let unit = "0102030405060708".parse().expect("a unit id");
    let pass = svolder_proto::Passphrase::NONE;
    let transport = svolder_ble::FakeTransport::from_profile(&p, unit, 7, pass);
    let advert = svolder_ble::Advertisement {
        manufacturer: transport.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut session =
        svolder_ble::Session::new(std::sync::Arc::new(transport), advert, p.clone(), pass);
    session.connect().await.expect("the fake connects");
    let row = info("Gungnir 2");
    let before: Vec<Message> = session.state().messages().cloned().collect();
    let s = screen(&shot(&row, &p, &before));
    assert_eq!(s.facts.firmware, "3 (advertised)");
    // The answers come out of the session like any status.
    for _ in 0..p.firmware_subsystems.len() {
        session.recv().await.expect("a firmware answer");
    }
    let after: Vec<Message> = session.state().messages().cloned().collect();
    let s = screen(&shot(&row, &p, &after));
    assert_eq!((s.facts.firmware.as_str(), s.facts.dsp), ("7", Some(7)));
    assert_eq!(model_of(&s)["facts"]["firmware"], "7");
}

/// The facts say the device, its profile and its link in the words the TUI uses, the signal
/// through the one helper, and keep a guest from the unit id and the passphrase note
/// (`ember-parts.mjs` holds the same name for the guest's Identify button).
#[test]
fn facts_say_the_device_the_link_and_the_signal() {
    let p = profile_named("gungnir2-v2");
    let mut row = info("Gungnir 2");
    row.signal = Some(svolder_ipc::Signal {
        dbm: -62,
        source: svolder_ipc::SignalSource::Advert,
        heard_at_ms: NOW_MS - 180_000,
    });
    row.passphrase_note = Some(svolder_ipc::PassphraseNote::Differs);
    let s = screen(&shot(&row, &p, &[]));
    let f = &s.facts;
    assert_eq!(f.product.as_deref(), Some("Gungnir 2"));
    assert_eq!((f.product_id, f.unit.as_str()), (2, "0102030405060708"));
    assert_eq!(f.profile, "gungnir2-v2");
    assert_eq!(f.link, "connected");
    assert_eq!(f.signal, "\u{2212}62 dBm advert, 3 min ago");
    assert_eq!(
        f.passphrase_note.as_deref(),
        Some("Passphrase does not match the code the device advertises")
    );
    assert!(f.identify);
    for (refused, stale, words) in [
        (true, false, "connected \u{b7} passphrase refused"),
        (false, true, "connected \u{b7} readings stale"),
        (
            true,
            true,
            "connected \u{b7} passphrase refused \u{b7} readings stale",
        ),
    ] {
        let flagged = DeviceInfo {
            refused,
            stale,
            ..row.clone()
        };
        assert_eq!(screen(&shot(&flagged, &p, &[])).facts.link, words);
    }
    let down = DeviceInfo {
        connected: false,
        stale: true,
        ..row.clone()
    };
    assert_eq!(
        screen(&shot(&down, &p, &[])).facts.link,
        "disconnected \u{b7} readings stale"
    );
    let none = DeviceInfo {
        signal: None,
        ..row.clone()
    };
    assert_eq!(screen(&shot(&none, &p, &[])).facts.signal, "\u{2014}");
    // A guest sees neither the unit id nor the passphrase note, and cannot identify.
    let guest = Snapshot {
        access: Access {
            writes: false,
            guest: true,
        },
        ..shot(&row, &p, &[])
    };
    let g = screen(&guest).facts;
    assert_eq!(
        (g.unit.as_str(), g.passphrase_note.as_deref(), g.identify),
        ("", None, false)
    );
    let facts = model_of(&screen(&guest))["facts"].to_string();
    assert!(
        !facts.contains("0102030405060708") && !facts.contains("Passphrase"),
        "{facts}"
    );
}

/// The fold-out is in the model of every device, a DAC's and an amp's; the page draws it as a
/// native `details`, closed by default (`ember-parts.mjs` holds the case `facts_are_closed_by_default`).
#[test]
fn the_facts_are_in_every_devices_model() {
    let p = profile_named("gungnir2-v2");
    let s = screen(&shot(&info("Gungnir 2"), &p, &[]));
    assert!(s.has("facts"));
    assert!(model_of(&s)["facts"].is_object());
    let amp = profile_named("aegir2");
    let s = screen(&shot(&info("Aegir 2"), &amp, &[]));
    assert!(s.has("facts"));
    assert!(model_of(&s)["facts"].is_object());
}

/// What the stream compares ignores the clock and moves with a reading's stamp only a minute at a
/// time: a fact or a reading's words change it, and so does a reading heard again in a later
/// minute.
#[test]
fn the_facts_key_follows_what_is_said_not_when() {
    let p = profile_named("gungnir2-v2");
    let readings = vec![read(&p, "input", "3")];
    let info = info("Gungnir 2");
    let key = |shot: Snapshot| screen(&shot).facts.key();
    let base = key(shot(&info, &p, &readings));
    // Heard again within the same minute, with the clock moved on: nothing to say.
    let first = BTreeMap::from([(readings[0].id, MINUTE_MS + 5_000)]);
    let stamped = key(Snapshot {
        heard_at: &first,
        ..shot(&info, &p, &readings)
    });
    let again = BTreeMap::from([(readings[0].id, MINUTE_MS + 55_000)]);
    let later = key(Snapshot {
        heard_at: &again,
        now_ms: NOW_MS + 60_000,
        ..shot(&info, &p, &readings)
    });
    assert_eq!(stamped, later);
    // Heard again in the next minute with the same value: a minute older on the page, so the
    // part is due again.
    let next = BTreeMap::from([(readings[0].id, MINUTE_MS + 65_000)]);
    let renewed = key(Snapshot {
        heard_at: &next,
        ..shot(&info, &p, &readings)
    });
    assert_ne!(stamped, renewed);
    let moved = vec![read(&p, "input", "2")];
    assert_ne!(base, key(shot(&info, &p, &moved)));
    let down = DeviceInfo {
        connected: false,
        ..info.clone()
    };
    assert_ne!(base, key(shot(&down, &p, &readings)));
}

/// A meter's reading changes the table and not the key: the meters show it already, so the stream
/// does not send the part again for it. Another reading's change still does.
#[test]
fn the_facts_key_ignores_a_meters_reading_but_the_table_lists_it() {
    let p = profile_named("fake-five");
    let info = info("Amp");
    let key = |readings: &[Message]| screen(&shot(&info, &p, readings)).facts.key();
    let quiet = [read(&p, "meter_1", "100")];
    let loud = [read(&p, "meter_1", "900")];
    assert_eq!(key(&quiet), key(&loud));
    let facts = screen(&shot(&info, &p, &loud)).facts;
    assert_eq!(facts.rows.len(), 1);
    assert_eq!(facts.rows[0].value, "900");
    assert!(facts.rows[0].meter);
    let mut other = loud.to_vec();
    other.push(Message::status(FunctionId::three(210, 2, 2), Value::U16(9)));
    assert_ne!(key(&loud), key(&other));
}

/// An active fault is in the card and nowhere else: no family shows it as a Controls row. A
/// preamp (the Lyr 5) and a DAC, which lists every control but the knob's, as the amp's Controls
/// section lists only its selects and levels.
#[test]
fn an_active_fault_is_in_the_card_and_never_in_controls() {
    let preamp = profile_named("lyr5");
    let dac = Profile {
        family: Family::Dac,
        ..preamp.clone()
    };
    for p in [preamp, dac, profile_named("aegir2")] {
        let faults: Vec<&str> = p
            .controls
            .iter()
            .filter(|c| c.kind == ControlKind::Fault)
            .map(|c| c.key.as_str())
            .collect();
        assert!(!faults.is_empty(), "{} has faults", p.name);
        let readings: Vec<Message> = faults.iter().map(|k| read(&p, k, "true")).collect();
        let s = screen(&shot(&info("Device"), &p, &readings));
        assert_eq!(s.card.as_ref().expect("a card").faults.len(), faults.len());
        for row in &s.controls {
            assert!(!faults.contains(&row.key()), "{}: {row:?}", p.name);
        }
        let controls = model_of(&s)["controls"].to_string();
        for key in faults {
            assert!(
                !controls.contains(&format!("\"key\":\"{key}\"")),
                "{}: {controls}",
                p.name
            );
        }
    }
}

/// A device whose profile offers no identify (a Mesh Card) has no Identify button, whoever looks;
/// every other device has one (`ember-parts.mjs` holds the same name for the button).
#[test]
fn a_profile_that_offers_no_identify_has_no_identify_button() {
    let card = profile_named("meshcard");
    assert!(!card.identify);
    let info = info("Mesh Card");
    let facts = &model_of(&screen(&shot(&info, &card, &[])))["facts"];
    assert_eq!(facts["identify_offered"], false, "{facts}");
    let dac = profile_named("gungnir2-v2");
    let facts = &model_of(&screen(&shot(&info, &dac, &[])))["facts"];
    assert_eq!(facts["identify_offered"], true, "{facts}");
}

/// The idle-gap the page waits after a press that sent nothing is the one the daemon keeps
/// (`ember-parts.mjs` holds the case `the_identify_button_carries_the_daemons_gap` for the button that carries it).
#[test]
fn the_facts_carry_the_daemons_identify_gap() {
    let p = profile_named("gungnir2-v2");
    let facts = &model_of(&screen(&shot(&info("Gungnir 2"), &p, &[])))["facts"];
    let gap = svolder_ble::IDENTIFY_GAP.as_secs();
    assert_eq!(facts["identify_gap_secs"], gap, "{facts}");
}

/// A profile's label and a value's name are text, not markup: the model carries them as they are
/// and the page that embeds it cannot be closed by them (`ember-parts.mjs` holds the case `facts_escape_what_a_label_and_a_value_say`
/// for the facts part that escapes them).
#[test]
fn facts_keep_what_a_label_and_a_value_say_as_text() {
    let mut p = profile_named("gungnir2-v2");
    let input = p
        .controls
        .iter_mut()
        .find(|c| c.key == "input")
        .expect("an input");
    input.label = "<script>alert(\"x\")</script>".to_string();
    input.values = vec![(3, "<img src=x onerror=\"y\">".to_string())];
    let readings = vec![read(&p, "input", "3")];
    let s = screen(&shot(&info("Gungnir 2"), &p, &readings));
    let facts = &model_of(&s)["facts"];
    let row = facts["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["label"] == "<script>alert(\"x\")</script>")
        .expect("the label as it is");
    assert_eq!(row["value"], "3 (<img src=x onerror=\"y\">)");
    let embedded = crate::web::api::embed(&crate::web::model::device_model(&s));
    assert!(
        !embedded.contains("<script>") && !embedded.contains("<img"),
        "{embedded}"
    );
}

/// The six bands of the Loki Max at `raws`, as the device reports them.
fn bands_at(p: &Profile, raws: [i64; 6]) -> Vec<Message> {
    raws.iter()
        .enumerate()
        .map(|(i, raw)| read(p, &format!("band{}", i + 1), &raw.to_string()))
        .collect()
}

/// A Loki Max's screen has its own presets part (three slots, the five built-in settings) where a
/// DAC's has the tone presets, and none of the tone control's parts; a DAC has no such part
/// (`ember-parts.mjs` holds the same name for the part the page draws from them).
#[test]
fn a_loki_max_screen_has_device_presets_and_no_tone_presets() {
    let p = profile_named("lokimax");
    let loki = info("Loki Max");
    let s = screen(&shot(&loki, &p, &[]));
    assert!(!s.amp, "the Loki Max takes the DAC layout");
    assert!(s.presets.is_none() && s.tone.is_none() && s.bands.is_none());
    assert!(s.has("device-presets") && !s.has("presets"));
    let presets = s.device_presets.as_ref().expect("device presets");
    let slots: Vec<&str> = presets.slots.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(slots, ["Preset 1", "Preset 2", "Preset 3"]);
    let builtins: Vec<&str> = presets.builtins.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        builtins,
        ["Flat", "Bass Boost", "Treble Boost", "Smiley", "Loudness"]
    );
    assert_eq!(presets.slots[1].recall_json, r#"{"slot":2}"#);
    assert_eq!(presets.slots[1].save_json, r#"{"confirm":true,"slot":2}"#);
    assert_eq!(presets.builtins[1].json, r#"{"name":"Bass Boost"}"#);
    let model = model_of(&s);
    assert_eq!(
        model["device_presets"]["slots"].as_array().map(Vec::len),
        Some(3)
    );
    assert_eq!(
        model["device_presets"]["builtins"].as_array().map(Vec::len),
        Some(5)
    );
    assert!(
        model["presets"].is_null() && model["tone"].is_null(),
        "{model}"
    );
    let dac = profile_named("gungnir2-v2");
    let dac_info = info("Gungnir 2");
    let s = screen(&shot(&dac_info, &dac, &[]));
    assert!(s.device_presets.is_none() && !s.has("device-presets"));
}

/// A guest, and a page with writes off, can recall a slot nowhere and sees no save: the model
/// offers neither, so the page's buttons post nothing and draw no save icon or form
/// (`ember-parts.mjs` holds the same name).
#[test]
fn a_guest_page_shows_no_save() {
    let p = profile_named("lokimax");
    let loki = info("Loki Max");
    let open = screen(&shot(&loki, &p, &[]));
    assert!(open.device_presets.as_ref().unwrap().save);
    assert_eq!(model_of(&open)["device_presets"]["save"], true);
    for access in [
        Access {
            writes: false,
            guest: true,
        },
        Access {
            writes: false,
            guest: false,
        },
    ] {
        let s = screen(&Snapshot {
            access,
            ..shot(&loki, &p, &[])
        });
        let presets = s.device_presets.as_ref().unwrap();
        assert!(!presets.save && !presets.recall);
        let model = model_of(&s);
        assert_eq!(model["device_presets"]["save"], false);
        assert_eq!(model["device_presets"]["recall"], false);
        assert_eq!(model["inert"], true);
    }
}

/// The built-in setting the six bands read is lit, none while a band is unread, and no slot ever
/// is: the device does not say which slot it holds (`ember-parts.mjs` holds the same name for the
/// lamps the page draws).
#[test]
fn the_builtin_the_bands_match_is_lit_and_no_slot_is() {
    let p = profile_named("lokimax");
    let loki = info("Loki Max");
    let lit = |readings: &[Message]| -> (Vec<String>, usize) {
        let s = screen(&shot(&loki, &p, readings));
        let presets = s.device_presets.as_ref().unwrap();
        let on = presets
            .builtins
            .iter()
            .filter(|r| r.on)
            .map(|r| r.name.clone())
            .collect();
        let model = &model_of(&s)["device_presets"];
        let lit_in_model = model["builtins"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|b| b["on"] == true)
            .count();
        assert!(
            model["slots"]
                .as_array()
                .unwrap()
                .iter()
                .all(|slot| slot.get("on").is_none()),
            "no slot carries a lamp: {model}"
        );
        (on, lit_in_model)
    };
    let bass = bands_at(&p, [28, 21, 17, 15, 15, 15]);
    assert_eq!(lit(&bass), (vec!["Bass Boost".to_string()], 1));
    let flat = bands_at(&p, [15; 6]);
    assert_eq!(lit(&flat), (vec!["Flat".to_string()], 1));
    assert_eq!(lit(&bands_at(&p, [15, 15, 15, 15, 15, 16])), (vec![], 0));
    assert_eq!(lit(&flat[..5]), (vec![], 0));
    assert_eq!(lit(&[]), (vec![], 0));
}
