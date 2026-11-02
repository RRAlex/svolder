// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const NOW: u64 = 10_000_000_000;

fn advert_aged(secs: u64) -> Signal {
    Signal {
        dbm: -62,
        source: SignalSource::Advert,
        heard_at_ms: NOW - secs * 1000,
    }
}

/// A live signal reads `-58 dBm live` while it is current; an advert always says how old it is;
/// no signal is an em dash. The minus is the character the caller asks for.
#[test]
fn signal_forms() {
    let live = Signal {
        dbm: -58,
        source: SignalSource::Live,
        heard_at_ms: NOW - 2_000,
    };
    assert_eq!(live.describe(NOW, '-'), "-58 dBm live");
    assert_eq!(live.describe(NOW, '\u{2212}'), "\u{2212}58 dBm live");
    assert_eq!(
        advert_aged(180).describe(NOW, '-'),
        "-62 dBm advert, 3 min ago"
    );
    assert_eq!(
        advert_aged(180).describe(NOW, '\u{2212}'),
        "\u{2212}62 dBm advert, 3 min ago"
    );
    assert_eq!(describe_signal(None, NOW, '-'), "\u{2014}");
    assert_eq!(
        describe_signal(Some(&advert_aged(5)), NOW, '-'),
        "-62 dBm advert, 5 s ago"
    );
}

/// The unit changes at 90 s, 90 min and 36 h, counts round down, and a stamp in the future reads
/// as no age.
#[test]
fn signal_ages_change_unit_at_their_edges() {
    let age = |secs: u64| advert_aged(secs).describe(NOW, '-');
    assert_eq!(age(0), "-62 dBm advert, 0 s ago");
    assert_eq!(age(89), "-62 dBm advert, 89 s ago");
    assert_eq!(age(90), "-62 dBm advert, 1 min ago");
    assert_eq!(age(90 * 60 - 1), "-62 dBm advert, 89 min ago");
    assert_eq!(age(90 * 60), "-62 dBm advert, 1 h ago");
    assert_eq!(age(36 * 3600 - 1), "-62 dBm advert, 35 h ago");
    assert_eq!(age(36 * 3600), "-62 dBm advert, 1 day ago");
    assert_eq!(age(3 * 86_400), "-62 dBm advert, 3 days ago");
    let ahead = Signal {
        heard_at_ms: NOW + 5_000,
        ..advert_aged(0)
    };
    assert_eq!(ahead.describe(NOW, '-'), "-62 dBm advert, 0 s ago");
}

/// A live signal that is no longer current says how old it is, and a non-negative strength has
/// no sign.
#[test]
fn an_old_live_signal_says_its_age() {
    let live = |secs: u64, dbm: i16| Signal {
        dbm,
        source: SignalSource::Live,
        heard_at_ms: NOW - secs * 1000,
    };
    assert_eq!(live(29, -58).describe(NOW, '-'), "-58 dBm live");
    assert_eq!(live(30, -58).describe(NOW, '-'), "-58 dBm live, 30 s ago");
    assert_eq!(
        live(600, -58).describe(NOW, '-'),
        "-58 dBm live, 10 min ago"
    );
    assert_eq!(live(0, 0).describe(NOW, '-'), "0 dBm live");
}

/// With no clock to age it by, a signal says where it came from and nothing of its age.
#[test]
fn a_signal_can_be_worded_without_an_age() {
    assert_eq!(advert_aged(180).describe_undated('-'), "-62 dBm advert");
    let live = Signal {
        dbm: -58,
        source: SignalSource::Live,
        heard_at_ms: 0,
    };
    assert_eq!(live.describe_undated('\u{2212}'), "\u{2212}58 dBm live");
}

/// The list form leaves out where the strength was measured: a fresh live signal is the bare
/// number, an advert or an old live signal adds its age, and none is an em dash.
#[test]
fn the_short_form_is_the_number_and_an_age_when_there_is_one() {
    let live = |secs: u64| Signal {
        dbm: -58,
        source: SignalSource::Live,
        heard_at_ms: NOW - secs * 1000,
    };
    assert_eq!(live(2).describe_short(NOW, '\u{2212}'), "\u{2212}58 dBm");
    assert_eq!(live(29).describe_short(NOW, '-'), "-58 dBm");
    assert_eq!(live(30).describe_short(NOW, '-'), "-58 dBm 30 s");
    assert_eq!(
        advert_aged(180).describe_short(NOW, '\u{2212}'),
        "\u{2212}62 dBm 3 min"
    );
    assert_eq!(advert_aged(5).describe_short(NOW, '-'), "-62 dBm 5 s");
    assert_eq!(describe_signal_short(None, NOW, '-'), "\u{2014}");
    assert_eq!(
        describe_signal_short(Some(&advert_aged(180)), NOW, '-'),
        "-62 dBm 3 min"
    );
}

/// A source a newer daemon adds does not break an older client: the whole signal still decodes,
/// with the source read as unknown, and is worded as such.
#[test]
fn a_source_this_build_does_not_know_decodes_as_unknown() {
    let signal: Signal =
        serde_json::from_str(r#"{"dbm":-60,"source":"radar","heard_at_ms":9999995000}"#)
            .expect("an unknown source is no decode error");
    assert_eq!(signal.source, SignalSource::Unknown);
    assert_eq!(signal.source.word(), "unknown");
    assert_eq!(signal.describe(NOW, '-'), "-60 dBm unknown, 5 s ago");
    assert_eq!(signal.describe_short(NOW, '-'), "-60 dBm 5 s");
    assert_eq!(signal.describe_undated('-'), "-60 dBm unknown");
}

/// Each source has one word, the one serde writes it as.
#[test]
fn every_source_has_its_one_word() {
    for (source, word) in [
        (SignalSource::Advert, "advert"),
        (SignalSource::Live, "live"),
        (SignalSource::Unknown, "unknown"),
    ] {
        assert_eq!(source.word(), word);
        assert_eq!(
            serde_json::to_string(&source).unwrap(),
            format!("\"{word}\"")
        );
    }
}
