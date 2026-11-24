// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The words and the JSON of `svolder host bypass`; the verbs on a daemon are in
//! `tests/host_eq_cli.rs`.

use super::*;
use svolder_host::eq::HostEqState;

fn view(on: bool, eq: bool, loudness: bool, crossfeed: bool, speakers: bool) -> BypassView {
    BypassView {
        on,
        held: HeldStages {
            eq,
            loudness,
            crossfeed,
            speakers,
        },
    }
}

#[test]
fn off_says_what_plays() {
    let levels = ChainLevels::of(true, -7.8, -7.0, -14.8, false);
    assert_eq!(
        status_line(&view(false, false, false, false, false), Some(&levels)),
        "bypass: off \u{b7} plays \u{2212}14.8 dB in all"
    );
}

#[test]
fn on_names_the_stages_it_brings_back_and_the_pass_through() {
    let levels = ChainLevels::of(true, -7.6, 0.0, -7.6, true);
    assert_eq!(
        status_line(&view(true, true, true, false, true), Some(&levels)),
        "bypass: on \u{b7} brings back EQ, loudness, speakers \u{b7} pass-through \u{2212}7.6 dB \
         until the music pauses"
    );
}

#[test]
fn with_no_levels_the_line_is_the_bypass_alone() {
    assert_eq!(
        status_line(&view(true, false, false, true, false), None),
        "bypass: on \u{b7} brings back crossfeed"
    );
    assert_eq!(
        status_line(&view(false, false, false, false, false), None),
        "bypass: off"
    );
}

#[test]
fn the_json_is_the_bypass_and_the_levels() {
    let levels = ChainLevels::of(true, -7.8, -7.0, -14.8, false);
    let json: serde_json::Value = serde_json::from_str(&json_of(
        Some(&view(false, false, false, false, false)),
        Some(&levels),
    ))
    .unwrap();
    assert_eq!(json["bypass"]["on"], false);
    assert_eq!(json["levels"]["total_db"], -14.8);
    assert_eq!(json["levels"]["words"], "plays \u{2212}14.8 dB in all");
}

#[test]
fn an_older_daemon_is_told_so_before_the_op_goes() {
    let older = EqView::default();
    let failure = serves_bypass(Some(&older)).unwrap_err();
    assert_eq!(failure.what, "the daemon does not serve the bypass");
    let serving = EqView {
        bypass: Some(BypassView::default()),
        ..EqView::default()
    };
    assert!(serves_bypass(Some(&serving)).is_ok());
    assert!(serves_bypass(None).is_ok(), "the op says why there is none");
}

fn saved(
    on: bool,
    crossfeed_on: bool,
    speakers_on: bool,
    bypass: Option<HeldStages>,
) -> HostEqState {
    HostEqState {
        on,
        name: "Saved".into(),
        origin: None,
        target: "sink".into(),
        route: svolder_host::eq::Route::Smart,
        manager: svolder_host::eq::Manager::Child,
        previous_default: None,
        preamp_db: 0.0,
        filters: Vec::new(),
        slots: Vec::new(),
        crossfeed: None,
        crossfeed_on,
        flat: false,
        speakers: None,
        speakers_on,
        speakers_loaded: None,
        crossfeed_loaded: None,
        loudness: None,
        bypass,
    }
}

/// A command-line apply keeps the held set; with a stage on in the saved state the bypass is
/// not on, as the daemon reads it when it starts.
#[test]
fn a_saved_bypass_is_ignored_while_the_saved_state_has_a_stage_on() {
    let held = HeldStages {
        eq: true,
        ..HeldStages::default()
    };
    let idle = saved(false, false, false, Some(held));
    assert_eq!(bypass_of_state(Some(&idle)), BypassView { on: true, held });
    for on in [
        saved(true, false, false, Some(held)),
        saved(false, true, false, Some(held)),
        saved(false, false, true, Some(held)),
    ] {
        assert_eq!(bypass_of_state(Some(&on)), BypassView::default());
    }
    assert_eq!(bypass_of_state(None), BypassView::default());
}
