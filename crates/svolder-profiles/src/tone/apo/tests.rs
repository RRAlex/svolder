// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

fn ok(text: &str) -> Parsed {
    parse(text).unwrap_or_else(|e| panic!("{text:?} should parse: {e}"))
}

fn err(text: &str) -> String {
    parse(text)
        .expect_err(&format!("{text:?} should be refused"))
        .to_string()
}

fn filter(kind: FilterKind, f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter { kind, f, g, q }
}

#[test]
fn reads_a_preamp_and_every_accepted_filter_form() {
    let parsed = ok("Preamp: -6.2 dB\n\
         Filter 1: ON PK Fc 105 Hz Gain 3.5 dB Q 0.7\n\
         Filter 2: ON LSC Fc 105 Hz Gain -2 dB Q 0.71\n\
         Filter 3: ON HSC Fc 8000 Hz Gain 4 dB Q 0.5\n\
         Filter 4: ON PEQ Fc 1000 Hz Gain 1 dB Q 2\n\
         Filter 5: ON Modal Fc 2000 Hz Gain 1 dB Q 2\n\
         Filter 6: ON LS Fc 90 Hz Gain 1 dB Q 0.7\n\
         Filter 7: ON HS Fc 9000 Hz Gain 1 dB Q 0.7\n");
    assert!(parsed.notes.is_empty());
    assert_eq!(parsed.correction.preamp_db, -6.2);
    use FilterKind::*;
    let kinds: Vec<FilterKind> = parsed.correction.filters.iter().map(|f| f.kind).collect();
    assert_eq!(
        kinds,
        [
            Peaking, LowShelf, HighShelf, Peaking, Peaking, LowShelf, HighShelf
        ]
    );
    assert_eq!(
        parsed.correction.filters[0],
        filter(Peaking, 105.0, 3.5, 0.7)
    );
}

#[test]
fn no_preamp_means_zero_and_the_unit_may_hug_the_number() {
    assert_eq!(
        ok("Filter 1: ON PK Fc 100 Hz Gain 1 dB Q 1")
            .correction
            .preamp_db,
        0.0
    );
    let parsed = ok("preamp: -3db\nfilter 1: on pk fc 100hz gain 1db q 1");
    assert_eq!(parsed.correction.preamp_db, -3.0);
    assert_eq!(
        parsed.correction.filters[0],
        filter(FilterKind::Peaking, 100.0, 1.0, 1.0)
    );
}

#[test]
fn a_real_world_file_with_every_quirk_at_once_parses() {
    let text = "\u{feff}# AutoEq style header\r\n\
        \r\n\
        Preamp: -4.9 dB   \r\n\
        Filter 1: ON lsc Fc 105.0 Hz Gain 4.1 dB Q 0.70   \r\n\
        Filter 2: OFF PK Fc 200 Hz Gain 1 dB Q 1\r\n\
        Filter 10: on pk fc 1000 hz gain -0.0 db q 1.5\r\n\
        \t\r\n\
        FILTER 11: ON HSC Fc 10000 Hz Gain -3 dB Q 0.7\r\n";
    let parsed = ok(text);
    assert!(parsed.notes.is_empty(), "{:?}", parsed.notes);
    assert_eq!(parsed.correction.preamp_db, -4.9);
    let filters = &parsed.correction.filters;
    assert_eq!(filters.len(), 3);
    assert_eq!(filters[0].kind, FilterKind::LowShelf);
    assert_eq!(filters[1], filter(FilterKind::Peaking, 1000.0, 0.0, 1.5));
    assert!(filters[1].g.is_sign_positive(), "-0.0 reads as 0.0");
    assert_eq!(filters[2].kind, FilterKind::HighShelf);
}

#[test]
fn off_filters_vanish_without_a_note() {
    let parsed =
        ok("Filter 1: OFF PK Fc 100 Hz Gain 1 dB Q 1\nFilter 2: ON PK Fc 100 Hz Gain 1 dB Q 1");
    assert_eq!(parsed.correction.filters.len(), 1);
    assert!(parsed.notes.is_empty());
}

#[test]
fn other_filter_types_are_skipped_with_one_note_each() {
    let parsed = ok("Filter 1: ON PK Fc 100 Hz Gain 1 dB Q 1\n\
         Filter 7: ON LP Fc 20000 Hz\n\
         Filter 8: ON HP Fc 20 Hz Q 0.7\n\
         Filter 9: ON NO Fc 50 Hz\n\
         Filter 12: ON LS Fc 100 Hz Gain 3 dB\n\
         Filter 13: ON PK Fc 100 Hz Gain 3 dB BW Oct 0.5\n\
         Filter 14: ON LSC 12 dB Fc 300 Hz Gain 5 dB\n\
         Filter 15: ON HSC 12.0 dB Fc 300 Hz Gain 5 dB\n\
         Filter 16: ON LS 6dB Fc 300 Hz Gain 5 dB\n");
    assert_eq!(parsed.correction.filters.len(), 1);
    assert_eq!(parsed.notes.len(), 8);
    assert_eq!(
        parsed.notes[5..],
        [
            "filter 14: LSC skipped (only peaking and shelf filters are read)",
            "filter 15: HSC skipped (only peaking and shelf filters are read)",
            "filter 16: LS skipped (only peaking and shelf filters are read)",
        ]
    );
    assert_eq!(
        parsed.notes[0],
        "filter 7: LP skipped (only peaking and shelf filters are read)"
    );
    assert!(parsed.notes[1].starts_with("filter 8: HP skipped"));
    assert!(parsed.notes[2].starts_with("filter 9: NO skipped"));
    assert!(
        parsed.notes[3].starts_with("filter 12: LS skipped"),
        "{}",
        parsed.notes[3]
    );
    assert!(
        parsed.notes[4].starts_with("filter 13: PK skipped"),
        "{}",
        parsed.notes[4]
    );
}

#[test]
fn a_filter_without_a_written_number_is_numbered_by_place() {
    let parsed = ok("Filter: ON PK Fc 100 Hz Gain 1 dB Q 1\nFilter: ON IIR Fc 100 Hz\n");
    assert_eq!(
        parsed.notes,
        ["filter 2: IIR skipped (only peaking and shelf filters are read)"]
    );
}

#[test]
fn every_refusal_names_its_line() {
    let cases = [
        (
            "Channel: L\nFilter 1: ON PK Fc 1 Hz Gain 1 dB Q 1",
            "line 1: Channel",
        ),
        ("# c\nGraphicEQ: 20 0; 100 1", "line 2: GraphicEQ"),
        ("Include: other.txt", "line 1: Include"),
        ("\nDevice: speakers", "line 2: Device"),
        ("Preamp: -1 dB\nPreamp: -2 dB", "line 2: a second Preamp"),
        ("Preamp: loud", "line 1: cannot read the preamp"),
        ("Preamp: 1,5 dB", "line 1: cannot read the preamp"),
        ("what is this", "line 1: cannot read"),
        (
            "Filter 1: ON PK Fc 100 Hz Gain 1 dB Q 1\nFilter 2: MAYBE PK",
            "line 2: a filter is ON or OFF",
        ),
        ("Filter 1: ON", "line 1: the filter has no type"),
        (
            "Filter 1: ON PK Fc 100 Hz Gain 1 dB",
            "line 1: a peaking filter needs a Q",
        ),
        (
            "Filter 1: ON PK Gain 1 dB Q 1",
            "line 1: a filter needs Fc and Gain",
        ),
        (
            "Filter 1: ON PK Fc 100,5 Hz Gain 1 dB Q 1",
            "line 1: fc needs a number",
        ),
        (
            "Filter 1: ON PK Fc 100 Hz Fc 200 Hz Gain 1 dB Q 1",
            "line 1: fc given twice",
        ),
        (
            "Filter 1: ON PK Fc 100 Hz Gain 1 dB Q 1 Extra 3",
            "line 1: cannot read \"extra\"",
        ),
        (
            "Filter x: ON PK Fc 100 Hz Gain 1 dB Q 1",
            "line 1: cannot read",
        ),
        ("Filter 1: ON PK Fc 9 Hz Gain 1 dB Q 1", "line 1: Fc 9 Hz"),
        (
            "Filter 1: ON PK Fc 24001 Hz Gain 1 dB Q 1",
            "line 1: Fc 24001 Hz",
        ),
        (
            "\n\nFilter 1: ON PK Fc 100 Hz Gain 30.5 dB Q 1",
            "line 3: Gain 30.5 dB",
        ),
        (
            "Filter 1: ON PK Fc 100 Hz Gain -30.5 dB Q 1",
            "line 1: Gain -30.5 dB",
        ),
        (
            "Filter 1: ON PK Fc 100 Hz Gain 1 dB Q 0",
            "line 1: Q 0 is outside",
        ),
        (
            "Filter 1: ON PK Fc 100 Hz Gain 1 dB Q -1",
            "line 1: Q -1 is outside",
        ),
        (
            "Filter 1: ON PK Fc 100 Hz Gain 1 dB Q 20.1",
            "line 1: Q 20.1 is outside",
        ),
        (
            "Filter 1: ON PK Fc NaN Hz Gain 1 dB Q 1",
            "line 1: fc needs a number",
        ),
        (
            "Filter 1: ON PK Fc 100 Hz Gain inf dB Q 1",
            "line 1: gain needs a number",
        ),
    ];
    for (text, expected) in cases {
        let got = err(text);
        assert!(
            got.starts_with(expected),
            "{text:?}: expected {expected:?}, got {got:?}"
        );
    }
}

#[test]
fn a_file_with_nothing_to_read_is_refused_without_a_line() {
    for text in [
        "",
        "   \n\n# only a comment\n",
        "Preamp: -3 dB",
        "Filter 1: OFF PK Fc 1000 Hz Gain 1 dB Q 1",
    ] {
        assert_eq!(err(text), "no filter to read", "{text:?}");
    }
    assert_eq!(
        err("Filter 1: ON LP Fc 1000 Hz"),
        "no filter to read",
        "skipped filters leave nothing"
    );
}

#[test]
fn the_filter_count_is_bounded_at_63() {
    let line = |n: usize| format!("Filter {n}: ON PK Fc 1000 Hz Gain 1 dB Q 1\n");
    let text: String = (1..=63).map(line).collect();
    assert_eq!(ok(&text).correction.filters.len(), 63);
    let text: String = (1..=64).map(line).collect();
    assert_eq!(err(&text), "line 64: more than 63 filters");
    // Skipped and OFF filters do not count.
    let mut text: String = (1..=63).map(line).collect();
    text.push_str("Filter 64: OFF PK Fc 1 Hz Gain 1 dB Q 1\nFilter 65: ON LP Fc 100 Hz\n");
    assert_eq!(ok(&text).correction.filters.len(), 63);
}

#[test]
fn the_size_is_bounded_at_64_kib() {
    let line = "# padding padding padding padding padding padding padding\n";
    let mut text = String::from("Filter 1: ON PK Fc 100 Hz Gain 1 dB Q 1\n");
    while text.len() <= MAX_BYTES {
        text.push_str(line);
    }
    assert!(err(&text).contains("more than the 65536"), "{}", err(&text));
    text.truncate(MAX_BYTES);
    assert!(parse(&text).is_ok(), "exactly 64 KiB reads");
}

#[test]
fn the_bounds_are_inclusive_at_their_edges() {
    let parsed = ok(
        "Filter 1: ON PK Fc 10 Hz Gain -30 dB Q 20\nFilter 2: ON PK Fc 24000 Hz Gain 30 dB Q 0.01",
    );
    assert_eq!(parsed.correction.filters.len(), 2);
}

#[test]
fn filter_kinds_parse_in_any_case_and_print_their_codes() {
    for (word, kind) in [
        ("pk", FilterKind::Peaking),
        ("PEQ", FilterKind::Peaking),
        ("Modal", FilterKind::Peaking),
        ("lsc", FilterKind::LowShelf),
        ("Ls", FilterKind::LowShelf),
        ("HSC", FilterKind::HighShelf),
        ("hs", FilterKind::HighShelf),
    ] {
        assert_eq!(word.parse::<FilterKind>().unwrap(), kind, "{word}");
    }
    assert!("lp".parse::<FilterKind>().is_err());
    assert_eq!(
        [
            FilterKind::Peaking,
            FilterKind::LowShelf,
            FilterKind::HighShelf
        ]
        .map(FilterKind::code),
        ["PK", "LSC", "HSC"]
    );
}

#[test]
fn validate_holds_the_same_bounds_without_a_line() {
    let good = Correction {
        preamp_db: -2.0,
        filters: vec![filter(FilterKind::Peaking, 1000.0, 3.0, 1.0)],
    };
    assert!(validate(&good).is_ok());
    let mutate = |change: &dyn Fn(&mut Correction)| {
        let mut c = good.clone();
        change(&mut c);
        validate(&c).expect_err("should be refused").to_string()
    };
    assert_eq!(mutate(&|c| c.filters.clear()), "no filter");
    assert!(mutate(&|c| c.preamp_db = f64::NAN).contains("preamp"));
    assert!(mutate(&|c| c.filters[0].f = 5.0).starts_with("filter 1: Fc 5 Hz"));
    assert!(mutate(&|c| c.filters[0].g = 31.0).contains("Gain 31 dB"));
    assert!(mutate(&|c| c.filters[0].q = 0.0).contains("Q 0 "));
    assert!(mutate(&|c| c.filters[0].q = f64::NAN).contains("Q NaN"));
    assert!(mutate(&|c| c.filters = vec![good.filters[0].clone(); 64]).contains("64 filters"));
    let mut full = good.clone();
    full.filters = vec![good.filters[0].clone(); 63];
    assert!(validate(&full).is_ok());
}

#[test]
fn the_error_displays_its_line_or_just_what() {
    assert_eq!(ApoError::at(4, "x").to_string(), "line 4: x");
    assert_eq!(ApoError::bare("x").to_string(), "x");
    assert_eq!(ApoError::at(4, "x").line(), Some(4));
}

#[test]
fn channel_all_is_accepted_in_any_case() {
    let parsed = ok("Channel: all\nCHANNEL: ALL\nFilter 1: ON PK Fc 100 Hz Gain 1 dB Q 1");
    assert_eq!(parsed.correction.filters.len(), 1);
}

#[test]
fn a_preamp_is_held_within_thirty_decibels_in_text_and_in_a_correction() {
    let line = "Filter 1: ON PK Fc 1000 Hz Gain 1 dB Q 1";
    assert_eq!(
        ok(&format!("Preamp: +6 dB\n{line}")).correction.preamp_db,
        6.0
    );
    assert_eq!(
        ok(&format!("Preamp: -30 dB\n{line}")).correction.preamp_db,
        -30.0
    );
    for text in ["Preamp: +30.5 dB", "Preamp: 1000 dB", "Preamp: -31 dB"] {
        assert!(
            err(&format!("{text}\n{line}")).contains("line 1: Preamp"),
            "{text}"
        );
    }
    let mut correction = ok(line).correction;
    correction.preamp_db = 31.0;
    assert!(
        validate(&correction)
            .unwrap_err()
            .to_string()
            .contains("Preamp 31 dB")
    );
    correction.preamp_db = -30.0;
    assert!(validate(&correction).is_ok());
}

#[test]
fn only_a_shelf_may_be_given_as_a_slope_a_peaking_line_with_a_bare_number_is_refused() {
    let said = err("Filter 1: ON PK 12 dB Fc 1000 Hz Gain 1 dB Q 1");
    assert!(said.contains("line 1"), "{said}");
    assert!(!said.contains("skipped"), "{said}");
    let shelf =
        ok("Filter 1: ON LSC 12 dB Fc 300 Hz Gain 5 dB\nFilter 2: ON PK Fc 1000 Hz Gain 1 dB Q 1");
    assert_eq!(shelf.notes.len(), 1);
}
