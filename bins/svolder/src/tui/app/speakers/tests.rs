// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::KeyCode;
use svolder_ipc::HostEqOp;

use super::*;
use crate::tui::app::{Screen, ToneRow};
use crate::tui::test_support::{
    app_showing, computer_eq_with_crossfeed, computer_eq_with_speakers, press,
};

const SETS: [(&str, &str, &str); 3] = [
    ("sadie-d2", "no", "SADIE II, D2"),
    ("sadie-d1", "no", "SADIE II, D1"),
    ("room-1", "with", "Control room 1"),
];

/// An app on `screen`, computer side, with the speakers as `eq` says.
fn app_with(screen: Screen, eq: Box<svolder_host::eq::EqView>) -> App {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(eq);
    app.flip_side();
    if screen == Screen::Tone {
        app.show(Screen::Tone);
        app.tone.row = ToneRow::Mode;
    }
    app
}

fn speakers_on_tone(on: bool, chosen: Option<&str>) -> App {
    app_with(Screen::Tone, computer_eq_with_speakers(on, chosen, &SETS))
}

fn op(intents: &[Intent]) -> HostEqOp {
    match intents {
        [Intent::HostEq(op)] => (**op).clone(),
        other => panic!("one computer change wanted, got {other:?}"),
    }
}

fn speakers_op(on: Option<bool>, set: Option<&str>) -> HostEqOp {
    HostEqOp::Speakers {
        on,
        set: set.map(str::to_string),
        done: true,
    }
}

#[test]
fn the_choice_is_shown_only_where_the_view_carries_speakers() {
    let old = app_with(
        Screen::Tone,
        computer_eq_with_crossfeed(false, "bauer", None),
    );
    assert!(old.crossfeed_shown() && !old.space_shown());
    assert_eq!(
        old.xf_first(),
        Xf::Space,
        "Plays is crossfeed's own on/off row here"
    );
    assert_eq!(old.xf_stops(), [&[Xf::Space][..], &Xf::ALL].concat());
    let new = speakers_on_tone(false, None);
    assert!(new.space_shown());
    assert_eq!(new.xf_first(), Xf::Space);
}

#[test]
fn what_plays_is_the_choice() {
    assert_eq!(speakers_on_tone(false, None).space_now(), Space::Off);
    assert_eq!(
        speakers_on_tone(true, Some("sadie-d2")).space_now(),
        Space::Speakers
    );
    let mut eq = computer_eq_with_speakers(false, None, &SETS);
    eq.crossfeed.as_mut().expect("crossfeed").on = true;
    assert_eq!(app_with(Screen::Tone, eq).space_now(), Space::Crossfeed);
}

#[test]
fn the_stops_follow_the_choice() {
    let off = speakers_on_tone(false, Some("sadie-d2"));
    assert_eq!(
        off.xf_stops(),
        [
            Xf::Space,
            Xf::Model,
            Xf::Preset,
            Xf::Level,
            Xf::Crossover,
            Xf::Delay
        ]
    );
    let playing = speakers_on_tone(true, Some("sadie-d2"));
    assert_eq!(playing.xf_stops(), [Xf::Space, Xf::Set]);
    let pane = app_with(Screen::Device, computer_eq_with_speakers(true, None, &SETS));
    assert_eq!(pane.xf_stops(), [Xf::Space]);
}

/// A `←` or `→` on the Plays row only moves the highlight and sends nothing; `Enter` then applies
/// the highlighted choice. Answers what `Enter` sent.
fn highlight_then_apply(app: &mut App, arrow: KeyCode) -> Vec<Intent> {
    assert!(press(app, arrow).is_empty(), "the arrows only highlight");
    press(app, KeyCode::Enter)
}

#[test]
fn the_arrows_only_highlight_on_plays_and_enter_applies() {
    let mut app = speakers_on_tone(false, Some("sadie-d2"));
    app.focus_crossfeed(Xf::Space);
    assert!(press(&mut app, KeyCode::Right).is_empty());
    assert_eq!(app.plays_highlight(), Space::Crossfeed);
    assert!(press(&mut app, KeyCode::Right).is_empty());
    assert_eq!(app.plays_highlight(), Space::Speakers);
    assert!(press(&mut app, KeyCode::Right).is_empty(), "the end");
    assert_eq!(app.plays_highlight(), Space::Speakers);
    assert_eq!(
        op(&press(&mut app, KeyCode::Enter)),
        speakers_op(Some(true), None)
    );
    assert_eq!(
        app.plays_highlight(),
        Space::Off,
        "applied: back on what plays"
    );
    // a move to another row drops the highlight
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.plays_highlight(), app.space_now());
}

#[test]
fn the_arrows_step_the_choice_and_send_only_the_one_asked_for() {
    let mut app = speakers_on_tone(false, Some("sadie-d2"));
    app.focus_crossfeed(Xf::Space);
    let to_crossfeed = highlight_then_apply(&mut app, KeyCode::Right);
    assert_eq!(
        op(&to_crossfeed),
        HostEqOp::Crossfeed {
            on: Some(true),
            model: None,
            preset: None,
            amount_pct: None,
            crossover_hz: None,
            delay_ms: None,
            done: true
        }
    );
    // from the crossfeed the next step is the speakers, with the chosen set
    let mut eq = computer_eq_with_speakers(false, Some("sadie-d2"), &SETS);
    eq.crossfeed.as_mut().expect("crossfeed").on = true;
    let mut app = app_with(Screen::Tone, eq);
    app.focus_crossfeed(Xf::Space);
    assert_eq!(
        op(&highlight_then_apply(&mut app, KeyCode::Right)),
        speakers_op(Some(true), None)
    );
    // and at the end of the list nothing goes
    let mut app = speakers_on_tone(true, Some("sadie-d2"));
    app.focus_crossfeed(Xf::Space);
    assert!(highlight_then_apply(&mut app, KeyCode::Right).is_empty());
}

#[test]
fn stepping_back_from_the_speakers_turns_them_off_and_from_the_crossfeed_it_does() {
    let mut app = speakers_on_tone(true, Some("sadie-d2"));
    app.focus_crossfeed(Xf::Space);
    // Speakers -> Crossfeed: the crossfeed op, which the controller answers by turning the
    // speakers off in the same update
    let back = highlight_then_apply(&mut app, KeyCode::Left);
    assert!(
        matches!(op(&back), HostEqOp::Crossfeed { on: Some(true), .. }),
        "{back:?}"
    );
    let mut eq = computer_eq_with_speakers(false, Some("sadie-d2"), &SETS);
    eq.crossfeed.as_mut().expect("crossfeed").on = true;
    let mut app = app_with(Screen::Tone, eq);
    app.focus_crossfeed(Xf::Space);
    let off = highlight_then_apply(&mut app, KeyCode::Left);
    assert!(
        matches!(
            op(&off),
            HostEqOp::Crossfeed {
                on: Some(false),
                ..
            }
        ),
        "{off:?}"
    );
}

#[test]
fn space_turns_what_plays_off_and_brings_back_the_speakers_where_a_set_is_chosen() {
    let mut app = speakers_on_tone(true, Some("sadie-d2"));
    app.focus_crossfeed(Xf::Space);
    assert_eq!(
        op(&press(&mut app, KeyCode::Char(' '))),
        speakers_op(Some(false), None)
    );
    let mut app = speakers_on_tone(false, Some("sadie-d2"));
    app.focus_crossfeed(Xf::Space);
    assert_eq!(
        op(&press(&mut app, KeyCode::Char(' '))),
        speakers_op(Some(true), None)
    );
    // with no set chosen it is the crossfeed that comes on
    let mut app = speakers_on_tone(false, None);
    app.focus_crossfeed(Xf::Space);
    assert!(matches!(
        op(&press(&mut app, KeyCode::Char(' '))),
        HostEqOp::Crossfeed { on: Some(true), .. }
    ));
}

#[test]
fn turning_the_speakers_on_with_no_set_chosen_chooses_the_first_fetched_one() {
    let mut app = speakers_on_tone(false, None);
    assert_eq!(
        op(&app.space_go(Space::Speakers)),
        speakers_op(Some(true), Some("sadie-d2"))
    );
}

#[test]
fn turning_the_speakers_on_with_nothing_fetched_opens_the_set_list_and_says_so() {
    let mut app = app_with(Screen::Tone, computer_eq_with_speakers(false, None, &[]));
    assert!(app.space_go(Space::Speakers).is_empty());
    assert!(app.speakers_box.list.is_some(), "the list opens");
    assert!(
        app.toast()
            .is_some_and(|toast| toast.text.contains("no virtual speaker set")),
        "{:?}",
        app.toast().map(|t| t.text.clone())
    );
}

#[test]
fn the_set_row_steps_the_fetched_sets_and_stops_at_the_ends() {
    let mut app = speakers_on_tone(true, Some("sadie-d2"));
    app.focus_crossfeed(Xf::Set);
    assert_eq!(
        op(&press(&mut app, KeyCode::Right)),
        speakers_op(None, Some("sadie-d1"))
    );
    assert!(press(&mut app, KeyCode::Left).is_empty(), "the first set");
    let mut app = speakers_on_tone(true, Some("room-1"));
    app.focus_crossfeed(Xf::Set);
    assert!(press(&mut app, KeyCode::Right).is_empty(), "the last set");
}

#[test]
fn a_refusal_the_view_states_is_said_and_nothing_is_sent() {
    let mut eq = computer_eq_with_speakers(false, Some("sadie-d2"), &SETS);
    eq.speakers.as_mut().expect("speakers").unavailable = Some("no output to play on".into());
    let mut app = app_with(Screen::Tone, eq);
    app.focus_crossfeed(Xf::Space);
    assert!(app.space_go(Space::Speakers).is_empty());
    assert!(app.set_step(1).is_empty());
    assert!(
        app.toast()
            .is_some_and(|toast| toast.text.contains("no output to play on"))
    );
}

#[test]
fn a_row_the_section_no_longer_has_gives_the_keys_back_to_the_choice() {
    let mut app = speakers_on_tone(false, Some("sadie-d2"));
    app.focus_crossfeed(Xf::Level);
    // the speakers come on elsewhere: Level is not a row any more
    app.host.computer.eq = Some(computer_eq_with_speakers(true, Some("sadie-d2"), &SETS));
    app.note_computer_view();
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Space));
    // and the set row goes the same way when they go off
    app.focus_crossfeed(Xf::Set);
    app.host.computer.eq = Some(computer_eq_with_speakers(false, Some("sadie-d2"), &SETS));
    app.note_computer_view();
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Space));
}

#[test]
fn the_reason_the_speakers_turned_themselves_off_is_said_once() {
    let mut app = speakers_on_tone(false, Some("sadie-d2"));
    let mut eq = computer_eq_with_speakers(false, Some("sadie-d2"), &SETS);
    eq.speakers.as_mut().expect("speakers").note =
        Some("virtual speakers off: SADIE II, D2 is missing or damaged, fetch it again".into());
    app.host.computer.eq = Some(eq.clone());
    app.note_computer_view();
    let said = app.toast().map(|toast| toast.text.clone());
    assert!(said.is_some_and(|text| text.contains("missing or damaged")));
    app.dismiss_toast();
    app.host.computer.eq = Some(eq);
    app.note_computer_view();
    assert!(app.toast().is_none(), "the same note is not said again");
}

#[test]
fn s_opens_the_set_list_from_either_screen_only_where_the_choice_is_shown() {
    let mut pane = app_with(
        Screen::Device,
        computer_eq_with_speakers(false, None, &SETS),
    );
    press(&mut pane, KeyCode::Char('S'));
    assert!(pane.speakers_box.list.is_some());
    let mut old = app_with(
        Screen::Tone,
        computer_eq_with_crossfeed(false, "bauer", None),
    );
    press(&mut old, KeyCode::Char('S'));
    assert!(old.speakers_box.list.is_none());
}

#[test]
fn the_texts_say_what_the_view_says() {
    let app = speakers_on_tone(true, Some("sadie-d2"));
    let view = app.speakers().expect("speakers");
    assert_eq!(set_words(view), "No room \u{b7} HRTF \u{b7} SADIE II, D2");
    assert_eq!(level_text(view), "about 7 dB quieter than Off");
    assert_eq!(level_short(view), "7 dB quieter");
    assert_eq!(length_text(view), "5.3 ms");
    assert_eq!(licence_text(view), "Apache-2.0 \u{b7} sample credit words");
    let mut cut = view.clone();
    cut.cut = true;
    cut.taps_ms = 500.0;
    assert_eq!(length_text(&cut), "cut to 0.5 s");
    cut.taps_ms = 1000.0;
    assert_eq!(length_text(&cut), "cut to 1.0 s");
    cut.credit = None;
    assert_eq!(licence_text(&cut), "Apache-2.0");
}
