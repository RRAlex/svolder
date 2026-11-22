// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::host::path::tests::clean;
use crate::host::{DacFacts, DacStage, SetupSummary, Verdict, computer_line, path_line};
use svolder_host::{PlayerMatch, SinkFacts};

/// The device the routes below end in, on its `USB 1` input, reading 48 kHz, with a volume of
/// `volume_db`.
fn dac(volume_db: Option<f64>) -> DacFacts {
    DacFacts {
        display: "Gungnir 2".into(),
        input: Some("USB 1".into()),
        rate_hz: Some(48_000),
        usb_ports: None,
        stage: DacStage {
            volume_db,
            ..DacStage::default()
        },
    }
}

/// A config check with `summary` and no lines.
fn check(summary: SetupSummary) -> SetupCheck {
    SetupCheck {
        sink: "Schiit Gungnir 2".into(),
        lines: Vec::new(),
        summary,
    }
}

fn word(text: &str, tone: Tone) -> RowWord {
    RowWord {
        text: text.into(),
        tone,
    }
}

/// The hops of a row as (label, words) pairs.
fn hops(row: &PathRow) -> Vec<(String, Vec<RowWord>)> {
    row.hops
        .iter()
        .map(|hop| (hop.label.clone(), hop.words.clone()))
        .collect()
}

fn labels(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

/// Whether any word of the row, hop or value, is drawn in Warn.
fn any_warn(row: &PathRow) -> bool {
    row.hops
        .iter()
        .flat_map(|hop| hop.words.iter())
        .chain(row.values.iter())
        .any(|w| w.tone == Tone::Warn)
}

#[test]
fn case_untouched_end_to_end() {
    let line = path_line(&clean(), &dac(Some(0.0)));
    let row = path_row(&line, None);
    assert_eq!(
        row.label,
        RowLabel {
            tone: Tone::Good,
            words: "Path: untouched end to end".into()
        }
    );
    assert_eq!(
        hops(&row)
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        ["player", "computer", "DAC USB 1"]
    );
    assert!(row.hops.iter().all(|hop| hop.words.is_empty()));
    assert_eq!(
        row.values,
        vec![word("48 kHz", Tone::Dim), word("volume 0 dB", Tone::Dim)]
    );
    assert_eq!(row.words, None);
    assert_eq!(line.end_to_end(), EndToEnd::Untouched);
}

#[test]
fn case_dac_at_minus_24_with_tone_control() {
    let mut device = dac(Some(-24.1));
    device.stage.tone = true;
    let line = path_line(&clean(), &device);
    assert_eq!(line.verdict, Verdict::Untouched);
    let row = path_row(&line, None);
    assert_eq!(
        row.label,
        RowLabel {
            tone: Tone::Warn,
            words: "Path: the DAC changes the sound".into()
        }
    );
    assert!(row.hops.iter().all(|hop| hop.words.is_empty()));
    assert_eq!(
        row.values,
        vec![
            word("48 kHz", Tone::Dim),
            word("volume -24.1 dB", Tone::Warn),
            word("tone control", Tone::Warn)
        ]
    );
    assert_eq!(line.end_to_end(), EndToEnd::Changed);
}

#[test]
fn case_resampled_at_the_computer() {
    let mut route = clean();
    route.player.as_mut().unwrap().rate_hz = Some(44_100);
    let row = path_row(&path_line(&route, &dac(Some(0.0))), None);
    assert_eq!(
        row.label,
        RowLabel {
            tone: Tone::Warn,
            words: "Path: the computer changes the sound".into()
        }
    );
    assert_eq!(
        hops(&row),
        vec![
            ("player".to_string(), vec![]),
            ("computer".to_string(), vec![word("resampled", Tone::Warn)]),
            ("DAC USB 1".to_string(), vec![]),
        ]
    );
    assert_eq!(
        row.values,
        vec![
            word("44.1 → 48 kHz", Tone::Dim),
            word("volume 0 dB", Tone::Dim)
        ]
    );
}

#[test]
fn case_config_failing_on_an_untouched_path() {
    let line = path_line(&clean(), &dac(Some(0.0)));
    let row = path_row(&line, Some(&check(SetupSummary::Changes(1))));
    assert_eq!(
        row.label,
        RowLabel {
            tone: Tone::Warn,
            words: "Path: the PipeWire config check failed".into()
        }
    );
    assert!(row.hops.iter().all(|hop| hop.words.is_empty()));
    assert_eq!(
        row.values,
        vec![
            word("48 kHz", Tone::Dim),
            word("volume 0 dB", Tone::Dim),
            word("config 1", Tone::Warn)
        ]
    );
    // The check failing never changes the end-to-end word: it is the sound's, not the config's.
    assert_eq!(line.end_to_end(), EndToEnd::Untouched);
}

#[test]
fn a_config_not_read_adds_no_tag_and_keeps_the_label_green() {
    let line = path_line(&clean(), &dac(Some(0.0)));
    for summary in [
        SetupSummary::Unread(2),
        SetupSummary::NotRead,
        SetupSummary::OtherVersion,
        SetupSummary::Untouched,
    ] {
        let row = path_row(&line, Some(&check(summary)));
        assert_eq!(row.label.tone, Tone::Good, "{summary:?}");
        assert_eq!(row.values.len(), 2, "{summary:?}");
    }
}

#[test]
fn something_not_read_is_one_dim_question_mark_at_its_hop() {
    let mut route = clean();
    route.sink.as_mut().unwrap().volume_milli = None;
    let row = path_row(&path_line(&route, &dac(None)), None);
    assert_eq!(
        row.label,
        RowLabel {
            tone: Tone::Plain,
            words: "Path: not everything could be read".into()
        }
    );
    assert_eq!(
        hops(&row),
        vec![
            ("player".to_string(), vec![]),
            ("computer".to_string(), vec![word("?", Tone::Dim)]),
            ("DAC USB 1".to_string(), vec![]),
        ]
    );
    assert!(!any_warn(&row));
}

#[test]
fn the_players_volume_is_a_warn_word_at_the_player() {
    let mut route = clean();
    route.player.as_mut().unwrap().volume_milli = Some(500);
    let row = path_row(&path_line(&route, &dac(None)), None);
    assert_eq!(row.label.words, "Path: the player changes the sound");
    assert_eq!(row.label.tone, Tone::Warn);
    assert_eq!(row.hops[0].words, vec![word("volume 50 %", Tone::Warn)]);
}

#[test]
fn two_or_three_hops_changing_are_named_in_one_sentence() {
    let mut route = clean();
    route.player.as_mut().unwrap().rate_hz = Some(44_100);
    let mut device = dac(Some(-3.0));
    let row = path_row(&path_line(&route, &device), None);
    assert_eq!(
        row.label.words,
        "Path: the computer and the DAC change the sound"
    );
    route.player.as_mut().unwrap().volume_milli = Some(900);
    device.rate_hz = Some(44_100);
    let row = path_row(&path_line(&route, &device), None);
    assert_eq!(
        row.label.words,
        "Path: the player, the computer and the DAC change the sound"
    );
    assert_eq!(row.hops[2].words, vec![word("reads 44.1 kHz", Tone::Warn)]);
}

#[test]
fn another_sink_and_another_input_have_no_hops_and_a_plain_label() {
    let mut route = clean();
    route.sink = Some(SinkFacts {
        name: "alsa_output.pci".into(),
        description: "DELL S2721QS".into(),
        rate_hz: Some(48_000),
        format: None,
        volume_milli: Some(1000),
        muted: false,
        usb_vendor_id: None,
        usb_product: None,
    });
    let line = path_line(&route, &dac(None));
    let row = path_row(&line, Some(&check(SetupSummary::Changes(2))));
    assert_eq!(
        row.label,
        RowLabel {
            tone: Tone::Plain,
            words: "Path: the computer plays to DELL S2721QS".into()
        }
    );
    assert!(row.hops.is_empty() && row.values.is_empty());
    assert_eq!(row.words.as_deref(), Some("→ DELL S2721QS"));
    assert_eq!(line.end_to_end(), EndToEnd::NotThisDac);

    let mut optical = dac(None);
    optical.input = Some("Optical".into());
    let line = path_line(&clean(), &optical);
    let row = path_row(&line, None);
    assert_eq!(
        row.label,
        RowLabel {
            tone: Tone::Plain,
            words: "Path: the DAC listens to Optical".into()
        }
    );
    assert!(row.hops.is_empty() && row.values.is_empty());
    assert_eq!(row.words.as_deref(), Some("the DAC listens to Optical"));
    assert_eq!(line.end_to_end(), EndToEnd::OtherInput);
}

#[test]
fn no_route_and_no_sink_read_are_plain_with_their_words() {
    let row = PathRow::not_read();
    assert_eq!(row.label.tone, Tone::Plain);
    assert!(row.hops.is_empty() && row.values.is_empty());
    assert_eq!(row.words.as_deref(), Some("route not read"));
    let mut route = clean();
    route.sink = None;
    let line = path_line(&route, &dac(None));
    let row = path_row(&line, None);
    assert_eq!(row.label.tone, Tone::Plain);
    assert!(row.hops.is_empty());
    assert_eq!(
        row.words.as_deref(),
        Some("the computer's sink was not read")
    );
    assert_eq!(line.end_to_end(), EndToEnd::Unknown);
}

#[test]
fn a_format_that_differs_is_no_warn_word_anywhere() {
    // `clean` plays S16LE into an S32LE sink.
    let line = path_line(&clean(), &dac(Some(0.0)));
    let row = path_row(&line, None);
    assert_eq!(row.label.tone, Tone::Good);
    assert!(!any_warn(&row));
    assert!(row.values.iter().all(|w| !w.text.contains("S32")));
}

#[test]
fn the_dac_muted_and_nos_on_are_warn_values_in_the_stage_order() {
    let mut device = dac(Some(0.0));
    device.stage.balance = true;
    device.stage.muted = true;
    device.stage.phase = true;
    device.stage.nos = true;
    let row = path_row(&path_line(&clean(), &device), None);
    assert_eq!(
        row.values,
        vec![
            word("48 kHz", Tone::Dim),
            word("volume 0 dB", Tone::Dim),
            word("balance off centre", Tone::Warn),
            word("muted", Tone::Warn),
            word("phase inverted", Tone::Warn),
            word("NOS filter", Tone::Warn),
        ]
    );
    assert_eq!(row.label.words, "Path: the DAC changes the sound");
}

#[test]
fn a_dac_with_no_db_volume_shows_no_volume_word() {
    let row = path_row(&path_line(&clean(), &dac(None)), None);
    assert_eq!(row.values, vec![word("48 kHz", Tone::Dim)]);
    assert_eq!(row.label.tone, Tone::Good);
}

#[test]
fn a_silent_dac_says_volume_silent() {
    let row = path_row(&path_line(&clean(), &dac(Some(f64::NEG_INFINITY))), None);
    assert_eq!(row.values[1], word("volume silent", Tone::Warn));
}

#[test]
fn the_dac_reading_another_rate_is_a_warn_word_at_the_dac() {
    let mut device = dac(Some(0.0));
    device.rate_hz = Some(44_100);
    let row = path_row(&path_line(&clean(), &device), None);
    assert_eq!(row.hops[2].words, vec![word("reads 44.1 kHz", Tone::Warn)]);
    assert_eq!(row.values[0], word("48 kHz", Tone::Dim));
    assert_eq!(row.label.words, "Path: the DAC changes the sound");
}

#[test]
fn the_resample_row_says_what_the_words_say_under_a_forced_rate() {
    use crate::host::path::tests::clean_setup;
    // Player 44.1, sink 48, clock forced to 96: the words say 44.1 → 48, so does the row.
    let mut route = clean();
    let mut setup = clean_setup();
    route.player.as_mut().unwrap().rate_hz = Some(44_100);
    setup.clock.as_mut().unwrap().force_rate = Some(96_000);
    route.setup = Some(setup.clone());
    let line = path_line(&route, &dac(None));
    assert_eq!(
        line.words,
        vec!["resampled 44.1 → 48 kHz (rate forced)".to_string()]
    );
    let row = path_row(&line, None);
    assert_eq!(row.values[0], word("44.1 → 48 kHz", Tone::Dim));
    // The sink still reading the stream's rate under the forced clock: the words say → 96.
    route.sink.as_mut().unwrap().rate_hz = Some(44_100);
    let mut device = dac(None);
    device.rate_hz = Some(44_100);
    let row = path_row(&path_line(&route, &device), None);
    assert_eq!(row.values[0], word("44.1 → 96 kHz", Tone::Dim));
}

#[test]
fn unknown_with_a_dac_stage_change_reads_changed_and_the_label_names_the_dac() {
    let mut route = clean();
    route.sink.as_mut().unwrap().volume_milli = None;
    let row = path_row(&path_line(&route, &dac(Some(-24.1))), None);
    assert_eq!(
        row.label,
        RowLabel {
            tone: Tone::Warn,
            words: "Path: the DAC changes the sound".into()
        }
    );
    assert_eq!(row.hops[1].words, vec![word("?", Tone::Dim)]);
    assert!(row.values.contains(&word("volume -24.1 dB", Tone::Warn)));
    // A failing config check on top does not change the words: the DAC is named.
    let line = path_line(&route, &dac(Some(-24.1)));
    let row = path_row(&line, Some(&check(SetupSummary::Changes(1))));
    assert_eq!(row.label.words, "Path: the DAC changes the sound");
    assert!(row.values.contains(&word("config 1", Tone::Warn)));
}

#[test]
fn unknown_with_a_failing_config_check_is_a_warn_label_for_the_check() {
    let mut route = clean();
    route.sink.as_mut().unwrap().volume_milli = None;
    let line = path_line(&route, &dac(None));
    let row = path_row(&line, Some(&check(SetupSummary::Changes(1))));
    assert_eq!(
        row.label,
        RowLabel {
            tone: Tone::Warn,
            words: "Path: the PipeWire config check failed".into()
        }
    );
    assert_eq!(row.hops[1].words, vec![word("?", Tone::Dim)]);
    assert_eq!(row.values.last(), Some(&word("config 1", Tone::Warn)));
}

#[test]
fn short_forms_that_repeat_are_drawn_once_at_the_hop() {
    use crate::host::path::tests::clean_setup;
    use svolder_host::Dither;
    use svolder_host::route::ChainStages;
    let mut route = clean();
    let mut setup = clean_setup();
    route.host_eq = true;
    route.stages = Some(ChainStages {
        eq: true,
        loudness: true,
        crossfeed: true,
        speakers: true,
    });
    setup.dither = Some(Dither {
        method: "triangular".into(),
        noise: 3,
    });
    route.setup = Some(setup);
    let row = path_row(&path_line(&route, &dac(None)), None);
    assert_eq!(
        row.hops[1].words,
        vec![word("EQ", Tone::Warn), word("dither", Tone::Warn)]
    );
}

#[test]
fn the_label_of_a_port_the_computer_is_not_on_joins_its_words_with_semicolons() {
    let mut device = dac(None);
    device.input = Some("USB 1".into());
    device.usb_ports = Some([Some(false), Some(true)]);
    let line = path_line(&clean(), &device);
    let row = path_row(&line, None);
    assert_eq!(
        row.label.words,
        "Path: the DAC listens to USB 1; the computer is on USB 2"
    );
    assert_eq!(
        row.words.as_deref(),
        Some("the DAC listens to USB 1 · the computer is on USB 2")
    );
}

#[test]
fn the_computer_screen_has_two_hops_and_no_dac_values() {
    let line = computer_line(&clean());
    let row = path_row(&line, Some(&check(SetupSummary::Changes(1))));
    assert_eq!(
        row.hops
            .iter()
            .map(|h| h.label.as_str())
            .collect::<Vec<_>>(),
        labels(&["player", "computer"])
    );
    assert_eq!(
        row.values,
        vec![word("48 kHz", Tone::Dim), word("config 1", Tone::Warn)]
    );
    assert_eq!(row.label.tone, Tone::Warn);
}

#[test]
fn the_dac_hop_is_labelled_with_its_input_or_plain_dac() {
    let mut device = dac(None);
    device.input = None;
    let row = path_row(&path_line(&clean(), &device), None);
    assert_eq!(row.hops[2].label, "DAC");
    assert_eq!(row.hops[2].words, vec![word("?", Tone::Dim)]);
}

#[test]
fn end_to_end_follows_the_verdict_and_the_dac_stage() {
    let calm = dac(Some(0.0));
    let mut loud = dac(Some(0.0));
    loud.stage.tone = true;
    let mut resampled = clean();
    resampled.player.as_mut().unwrap().rate_hz = Some(44_100);
    let mut unknown = clean();
    unknown.sink.as_mut().unwrap().volume_milli = None;
    assert_eq!(path_line(&clean(), &calm).end_to_end(), EndToEnd::Untouched);
    assert_eq!(path_line(&clean(), &loud).end_to_end(), EndToEnd::Changed);
    assert_eq!(path_line(&resampled, &calm).end_to_end(), EndToEnd::Changed);
    assert_eq!(path_line(&resampled, &loud).end_to_end(), EndToEnd::Changed);
    assert_eq!(path_line(&unknown, &calm).end_to_end(), EndToEnd::Unknown);
    // A known change outranks something not read: the DAC's stage makes it changed.
    assert_eq!(path_line(&unknown, &loud).end_to_end(), EndToEnd::Changed);
    // The verdicts that name no sound pass through whatever the stage says.
    let mut elsewhere = clean();
    elsewhere.player_match = PlayerMatch::Elsewhere {
        sink: "Speakers".into(),
    };
    assert_eq!(
        path_line(&elsewhere, &loud).end_to_end(),
        EndToEnd::NotThisDac
    );
    let mut optical = loud.clone();
    optical.input = Some("Optical".into());
    assert_eq!(
        path_line(&clean(), &optical).end_to_end(),
        EndToEnd::OtherInput
    );
    // The computer-only line has no stage to add.
    assert_eq!(computer_line(&clean()).end_to_end(), EndToEnd::Untouched);
}

#[test]
fn the_end_to_end_keys_are_pinned() {
    let keys = [
        EndToEnd::Untouched,
        EndToEnd::Changed,
        EndToEnd::NotThisDac,
        EndToEnd::OtherInput,
        EndToEnd::Unknown,
    ]
    .map(EndToEnd::key);
    assert_eq!(keys, Verdict::KEYS);
}

#[test]
fn the_row_serializes_with_snake_case_tones() {
    let row = path_row(&path_line(&clean(), &dac(Some(0.0))), None);
    let json = serde_json::to_value(&row).unwrap();
    assert_eq!(json["label"]["tone"], "good");
    assert_eq!(
        json["values"][0],
        serde_json::json!({"text": "48 kHz", "tone": "dim"})
    );
    assert_eq!(json["words"], serde_json::Value::Null);
}
