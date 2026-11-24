// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer side's strip and panel, against the approved mockups: the rows the strip and the
//! panel take at every size, the last row blank, the graph keeping its rows, and each stage's
//! words.

use super::*;
use crate::tui::app::{Space, Stage, Xf};
use svolder_host::eq::{BypassView, ChainLevels, HeldStages};

/// An app on the computer side with the stages the mockups hold, the keys in the panel of
/// `stage`.
pub(super) fn stages_app(space: Space, stage: Stage) -> App {
    let mut app = computer_app();
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq_stages(space));
    app.key(KeyEvent::from(KeyCode::Char('C')));
    app.enter_stage(stage);
    app
}

/// [`stages_app`] with the bypass on: every stage off, the three that played held, the chain a
/// pass-through climbing to 0 dB.
pub(super) fn bypassed_app() -> App {
    let mut app = stages_app(Space::Speakers, Stage::Bypass);
    let eq = app.host.computer.eq.as_mut().expect("a view");
    eq.on = false;
    eq.loudness.as_mut().expect("loudness").on = false;
    eq.speakers.as_mut().expect("speakers").on = false;
    eq.bypass = Some(BypassView {
        on: true,
        held: HeldStages {
            eq: true,
            loudness: true,
            crossfeed: false,
            speakers: true,
        },
    });
    let mut levels = ChainLevels::of(true, 0.0, 0.0, -7.6, true);
    levels.climb_to_db = Some(0.0);
    levels.words = "climbing to 0.0 dB".to_string();
    eq.levels = Some(levels);
    app
}

/// The rows of `rows` from the one that starts with `first` to the one that starts with `last`,
/// both included.
fn between<'a>(rows: &'a [String], first: &str, last: &str) -> &'a [String] {
    let top = rows
        .iter()
        .position(|row| row.starts_with(first))
        .unwrap_or_else(|| panic!("no row starts {first:?}: {rows:#?}"));
    let bottom = rows[top..]
        .iter()
        .position(|row| row.starts_with(last))
        .unwrap_or_else(|| panic!("no row starts {last:?} after {first:?}: {rows:#?}"));
    &rows[top..=top + bottom]
}

/// The inner rows of the panel at a window `height` rows tall, from the mockups: 9 below 40 rows,
/// 13 from 40, 17 from 48.
fn inner_rows(height: u16) -> usize {
    match height {
        h if h >= 48 => 17,
        h if h >= 40 => 13,
        _ => 9,
    }
}

#[test]
fn every_size_keeps_the_last_row_blank_the_graph_whole_and_the_panel_its_height() {
    let stages = [
        (Space::Speakers, Stage::Eq),
        (Space::Speakers, Stage::Loudness),
        (Space::Crossfeed, Stage::Space),
        (Space::Speakers, Stage::Space),
        (Space::Speakers, Stage::Bypass),
    ];
    for (width, height) in [
        (80, 24),
        (100, 30),
        (120, 30),
        (120, 40),
        (160, 45),
        (160, 50),
    ] {
        for (space, stage) in stages {
            let app = stages_app(space, stage);
            let rows = rows_of(&app, width, height);
            let tag = format!("{stage:?} {width}x{height}");
            assert_eq!(rows.len(), usize::from(height), "{tag}");
            assert!(
                rows[rows.len() - 1].trim().is_empty(),
                "{tag}: the last row stays blank: {rows:#?}"
            );
            let panel = between(&rows, "┏", "┗");
            assert_eq!(panel.len(), inner_rows(height) + 2, "{tag}: {panel:#?}");
            assert!(
                rows[rows.len() - 2].starts_with('┗'),
                "{tag}: the panel ends on the row above the blank one"
            );
            // the strip is the row over the panel's top border
            let top = rows.iter().position(|row| row.starts_with('┏')).unwrap();
            assert!(rows[top - 1].contains("Bypass"), "{tag}: {}", rows[top - 1]);
            // the Response block keeps at least four data rows (8 rows with its borders and axis)
            let graph = between(&rows, "┌ Response", "└");
            assert!(graph.len() >= 7, "{tag}: {graph:#?}");
            // keys in the strip or the header leave the panel and the graph where they were
            let mut moved = stages_app(space, stage);
            moved.focus_strip(stage);
            let strip_rows = rows_of(&moved, width, height);
            assert_eq!(
                between(&strip_rows, "┌ Response", "└").len(),
                graph.len(),
                "{tag}: the graph never loses a row to the panel"
            );
        }
    }
}

/// 80x24, EQ open: rows 1-3 header, 4-11 Response, 12 strip, 13-23 panel, 24 blank.
#[test]
fn at_80_by_24_the_eq_panel_is_the_mockups() {
    let rows = rows_of(&stages_app(Space::Speakers, Stage::Eq), 80, 24);
    assert!(rows[3].starts_with("┌ Response"), "{rows:#?}");
    assert!(rows[10].starts_with('└'), "{rows:#?}");
    assert_eq!(
        rows[11].trim_end(),
        " EQ ● −7.8 │ Loudness ● 60 % │ Space ● speakers −7.0 │ Bypass ○ │ all −14.8 dB"
    );
    assert!(
        rows[12].starts_with("┏ EQ · 9 of 16 points · 1 more ↓ ━")
            && rows[12].ends_with("━ preamp −7.8 dB ┓"),
        "{}",
        rows[12]
    );
    assert!(rows[13].starts_with("┃   #  Type  Freq"), "{}", rows[13]);
    assert!(rows[14].starts_with("┃ ▶  1  PK"), "{}", rows[14]);
    assert_eq!(
        rows[22],
        "┗ ↑↓ row · ←→ value · Enter type · a add · x remove · ␣ on/off · Esc strip ━━━━┛"
    );
    assert!(rows[23].trim().is_empty());
}

#[test]
fn at_80_by_24_the_loudness_panel_is_the_mockups() {
    let rows = rows_of(&stages_app(Space::Speakers, Stage::Loudness), 80, 24);
    assert!(rows[12].starts_with("┏ Loudness ━"), "{}", rows[12]);
    assert!(rows[13].contains("▶ Loudness   ‹ on ›"), "{}", rows[13]);
    assert!(
        rows[14].contains("Strength   ‹ 60 % ›  ██████░░░░"),
        "{}",
        rows[14]
    );
    assert!(
        rows[15]
            .contains("Reference  ‹ 97 % ›  the volume you mixed at · r: the volume now (42 %)"),
        "{}",
        rows[15]
    );
    assert!(
        rows[16].contains("Now        at 42 %: bass +"),
        "{}",
        rows[16]
    );
    assert_eq!(
        rows[22],
        "┗ ↑↓ row · ←→ change · Enter type · ␣ on/off · r volume now · Esc strip ━━━━━━━┛"
    );
}

#[test]
fn at_80_by_24_the_space_panel_is_the_mockups_for_crossfeed() {
    let rows = rows_of(&stages_app(Space::Crossfeed, Stage::Space), 80, 24);
    assert_eq!(
        rows[11].trim_end(),
        " EQ ● −7.8 │ Loudness ● 60 % │ Space ● crossfeed −3.1 │ Bypass ○ │ all −10.9 dB"
    );
    assert!(
        rows[12].starts_with("┏ Space · crossfeed ━")
            && rows[12].ends_with("plays 3.1 dB quieter ┓"),
        "{}",
        rows[12]
    );
    let wanted = [
        "▶ Plays      ‹ Crossfeed ›   Off · Crossfeed · Virtual speakers",
        "  Model      ‹ Mid/Side bass ›",
        "  Preset     Default (the only one)",
        "  Level      ‹ 50 % ›",
        "  Crossover  ‹ 120 Hz ›",
        "  Delay      none",
        "  Plays at   3.1 dB quieter",
    ];
    for (at, want) in wanted.into_iter().enumerate() {
        assert!(rows[13 + at].contains(want), "{at}: {}", rows[13 + at]);
    }
    assert_eq!(
        rows[22],
        "┗ ↑↓ row · ←→ change · Enter apply or type · ␣ on/off · S sets · Esc strip ━━━━┛"
    );
}

#[test]
fn at_80_by_24_the_bypass_panel_is_the_mockups_while_it_is_on() {
    let rows = rows_of(&bypassed_app(), 80, 24);
    assert_eq!(
        rows[11].trim_end(),
        " EQ ○ │ Loudness ○ │ Space ○ │ Bypass ● │ climbing to 0.0 dB"
    );
    assert!(rows[12].starts_with("┏ Bypass ━"), "{}", rows[12]);
    assert!(
        rows[13].contains("▶ Bypass     ‹ on ›    f anywhere on the computer side"),
        "{}",
        rows[13]
    );
    assert!(
        rows[14].contains("Brings back  EQ Bass · Loudness 60 % · Space virtual speakers"),
        "{}",
        rows[14]
    );
    assert!(
        rows[15].contains("Now        climbing to 0.0 dB"),
        "{}",
        rows[15]
    );
    assert_eq!(
        rows[16].trim_matches(|c| c == '┃' || c == ' '),
        "",
        "nothing under the three rows"
    );
    assert!(
        rows[22].starts_with("┗ ␣ or f bypass on/off · Esc strip ━"),
        "{}",
        rows[22]
    );
}

/// 120x40: rows 1-4 header, 5-23 Response (19 rows), 24 strip, 25-39 panel (13 inner rows), 40
/// blank.
#[test]
fn at_120_by_40_the_space_panel_with_the_virtual_speakers_is_the_mockups() {
    let rows = rows_of(&stages_app(Space::Speakers, Stage::Space), 120, 40);
    assert!(rows[4].starts_with("┌ Response"), "{}", rows[4]);
    assert!(rows[22].starts_with('└'), "{}", rows[22]);
    assert_eq!(
        rows[23].trim_end(),
        " EQ ● −7.8 dB  │  Loudness ● 60 %  │  Space ● virtual speakers −7.0 dB  │  Bypass ○  │  plays −14.8 dB in all"
    );
    assert!(
        rows[24].starts_with("┏ Space · virtual speakers ━")
            && rows[24].ends_with("about 7.0 dB quieter ┓"),
        "{}",
        rows[24]
    );
    assert!(
        rows[25].contains("▶ Plays      ‹ Virtual speakers ›   Off · Crossfeed · Virtual speakers")
    );
    assert!(
        rows[25].contains("Same ear · other ear"),
        "the curves' side box: {}",
        rows[25]
    );
    assert!(
        rows[26].contains("Set        ‹ No room · HRTF"),
        "{}",
        rows[26]
    );
    assert!(rows[26].contains("S sets…"), "{}", rows[26]);
    assert!(
        rows[38].starts_with(
            "┗ ↑↓ row · ←→ change · Enter apply or open · ␣ on/off · S sets · Esc strip"
        ),
        "{}",
        rows[38]
    );
    assert!(rows[39].trim().is_empty());
}

#[test]
fn at_120_by_40_the_eq_and_loudness_panels_are_the_mockups() {
    let rows = rows_of(&stages_app(Space::Speakers, Stage::Eq), 120, 40);
    assert!(
        rows[24].starts_with("┏ EQ · Bass · 9 of 16 points ━")
            && rows[24].ends_with("preamp −7.8 dB ┓"),
        "{}",
        rows[24]
    );
    // 13 inner rows: the column titles and the nine points, nothing scrolled away
    assert!(!rows[24].contains("more"), "{}", rows[24]);
    assert!(
        rows[38].starts_with("┗ ↑↓ row · ←→ value · Enter type · a add · x remove · y type · [ ] Q · ␣ EQ on/off · Esc strip"),
        "{}",
        rows[38]
    );
    let rows = rows_of(&stages_app(Space::Speakers, Stage::Loudness), 120, 40);
    assert!(
        rows[24].starts_with("┏ Loudness ━") && rows[24].ends_with("follows the computer volume ┓"),
        "{}",
        rows[24]
    );
    assert!(
        rows[27].contains("the volume you mixed at · r: use the volume now (42 %)"),
        "{}",
        rows[27]
    );
    assert!(
        rows[38].starts_with(
            "┗ ↑↓ row · ←→ change · Enter type a number · ␣ on/off · r volume now · Esc strip"
        ),
        "{}",
        rows[38]
    );
}

#[test]
fn at_120_by_40_the_bypass_panel_says_what_it_turns_off_while_it_is_off() {
    let mut app = stages_app(Space::Speakers, Stage::Bypass);
    app.focus_strip(Stage::Bypass);
    app.enter_stage(Stage::Bypass);
    let rows = rows_of(&app, 120, 40);
    assert!(rows[24].starts_with("┏ Bypass ━"), "{}", rows[24]);
    assert!(
        rows[25].contains("▶ Bypass     ‹ off ›    f from anywhere on the computer side"),
        "{}",
        rows[25]
    );
    assert!(
        rows[26].contains("Turns off  EQ Bass · Loudness 60 % · Space virtual speakers, and brings each back as it was"),
        "{}",
        rows[26]
    );
    assert!(
        rows[27].contains("Level      plays −14.8 dB in all; bypassed it climbs to 0.0 dB, or to the level the loaded curve allows"),
        "{}",
        rows[27]
    );
}

/// The panel's border is heavy and drawn in the frame-the-keys-are-in role while the keys are in
/// it, and thin with the keys on the strip or the header.
#[test]
fn the_border_is_heavy_while_the_keys_are_in_the_panel_and_thin_otherwise() {
    let mut app = stages_app(Space::Speakers, Stage::Loudness);
    let heavy = rows_of(&app, 80, 24);
    assert!(heavy[12].starts_with('┏') && heavy[22].starts_with('┗'));
    app.focus_strip(Stage::Loudness);
    let thin = rows_of(&app, 80, 24);
    assert!(
        thin[12].starts_with("┌ Loudness") && thin[22].starts_with('└'),
        "{thin:#?}"
    );
    // the panel keeps its stage with the keys on the header
    app.tone.row = ToneRow::Mode;
    let header = rows_of(&app, 80, 24);
    assert!(header[12].starts_with("┌ Loudness"), "{header:#?}");
}

/// The strip's open stop is drawn reversed, in the selected role while the keys are on the strip.
#[test]
fn the_open_stop_is_reversed() {
    let app = stages_app(Space::Speakers, Stage::Loudness);
    let buffer = frame_with_theme(&app, 80, 24, &Theme::new(false));
    let (_, _, open) = crate::tui::test_support::style_of(&buffer, "EQ ●", "Loudness");
    let (_, _, other) = crate::tui::test_support::style_of(&buffer, "EQ ●", "Space");
    assert!(open.contains(Modifier::REVERSED), "{open:?}");
    assert!(!other.contains(Modifier::REVERSED), "{other:?}");
}

/// A toast on this screen is the last row's one line and never a box over the panel.
#[test]
fn a_toast_uses_the_last_row_and_never_covers_the_panel() {
    let mut app = stages_app(Space::Speakers, Stage::Eq);
    let before = rows_of(&app, 80, 24);
    app.warn("the computer's EQ holds 16 points");
    let rows = rows_of(&app, 80, 24);
    assert!(
        rows[23].contains("the computer's EQ holds 16 points"),
        "{rows:#?}"
    );
    assert_eq!(&rows[..23], &before[..23], "the panel above is untouched");
}

/// A stage the controller does not serve has no stop and the keys skip it.
#[test]
fn a_view_without_loudness_or_a_bypass_has_no_stop_for_them() {
    let mut app = computer_app();
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq_with_crossfeed(
        true, "bauer", None,
    ));
    app.key(KeyEvent::from(KeyCode::Char('C')));
    let rows = rows_of(&app, 80, 24);
    let strip = rows[11].trim_end();
    assert!(
        strip.contains("EQ ●") && strip.contains("Space ●"),
        "{strip}"
    );
    assert!(
        !strip.contains("Loudness") && !strip.contains("Bypass"),
        "{strip}"
    );
}

/// The DAC's side has no strip and keeps the band table.
#[test]
fn the_dacs_side_has_no_strip() {
    let app = lifted_app();
    let rows = rows_of(&app, 80, 24);
    assert!(!holds(&rows, "Bypass"), "{rows:#?}");
    assert!(holds(&rows, "Bands"), "{rows:#?}");
}

/// A click on a stop of the strip opens that stage's panel with the keys in it, and a click on a
/// row of the panel focuses the row.
#[test]
fn a_click_on_a_stop_opens_its_panel_and_a_click_on_a_row_focuses_it() {
    use crate::tui::mouse::{HitMap, MouseAction, resolve};
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    let app = stages_app(Space::Speakers, Stage::Eq);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a terminal");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    let rows = text(terminal.backend().buffer());
    let strip = rows
        .iter()
        .position(|row| row.contains("Bypass ○"))
        .expect("the strip");
    let at = rows[strip].find("Loudness").expect("the stop");
    let (x, y) = (
        u16::try_from(rows[strip][..at].chars().count()).unwrap(),
        u16::try_from(strip).unwrap(),
    );
    assert_eq!(hits.hit_at(x, y), Some(&Hit::Stage(Stage::Loudness)));
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    assert!(matches!(
        resolve(&hits, click, &app),
        Some(MouseAction::Stage(Stage::Loudness))
    ));
    let mut app = app;
    app.enter_stage(Stage::Loudness);
    assert_eq!(app.tone.row, ToneRow::Ld(crate::tui::app::Ld::On));
}

/// With crossfeed playing the EQ panel says the EQ's own share (`levels.eq_db`), not the preamp
/// sent, which also holds crossfeed's headroom (finding 9).
#[test]
fn the_eq_panel_says_the_eqs_own_share_with_crossfeed_playing() {
    let app = stages_app(Space::Crossfeed, Stage::Eq);
    assert_eq!(
        app.computer_eq().map(|eq| eq.preamp_db.round()),
        Some(-11.0),
        "the fixture's preamp sent holds crossfeed's cost"
    );
    let rows = rows_of(&app, 80, 24);
    assert!(rows[12].ends_with("preamp −7.8 dB ┓"), "{}", rows[12]);
}

/// Below 120 columns the strip's tail is the daemon's words while the chain is untouched or
/// climbing, and the short form only for a pass-through that holds a cut.
#[test]
fn the_strip_below_120_columns_uses_the_daemons_words_when_untouched_or_climbing() {
    let mut app = bypassed_app();
    let levels = app
        .host
        .computer
        .eq
        .as_mut()
        .expect("a view")
        .levels
        .as_mut()
        .unwrap();
    levels.climb_to_db = None;
    levels.words = "pass-through −7.6 dB until the music pauses".to_string();
    let rows = rows_of(&app, 80, 24);
    assert!(
        rows[11].trim_end().ends_with("pass-through · all −7.6 dB"),
        "{}",
        rows[11]
    );
    let levels = app
        .host
        .computer
        .eq
        .as_mut()
        .unwrap()
        .levels
        .as_mut()
        .unwrap();
    levels.untouched = true;
    levels.words = "pass-through, untouched".to_string();
    let rows = rows_of(&app, 80, 24);
    assert!(
        rows[11]
            .trim_end()
            .ends_with("Bypass ● │ pass-through, untouched"),
        "{}",
        rows[11]
    );
}

/// The Bypass panel draws the daemon's words as they are; only the daemon says untouched.
#[test]
fn the_bypass_panel_draws_the_daemons_words_and_never_says_untouched_itself() {
    let mut app = bypassed_app();
    app.host
        .computer
        .eq
        .as_mut()
        .expect("a view")
        .levels
        .as_mut()
        .unwrap()
        .words = "pass-through −3.0 dB until the music pauses".to_string();
    let rows = rows_of(&app, 80, 24);
    assert!(
        rows[15].contains("Now        pass-through −3.0 dB until the music pauses"),
        "{}",
        rows[15]
    );
    assert!(!holds(&rows, "untouched"), "{rows:#?}");
}

/// An edit line is drawn in the panel's row, and a hand-edited preset says what it was edited from.
#[test]
fn the_space_panel_draws_an_edit_line_and_what_a_preset_was_edited_from() {
    let mut app = stages_app(Space::Crossfeed, Stage::Space);
    app.open_xf_line(Xf::Crossover);
    let rows = rows_of(&app, 80, 24);
    assert!(
        holds(&rows, "Crossover  [120] Hz  Enter writes · Esc cancels"),
        "{rows:#?}"
    );
    let mut app = stages_app(Space::Crossfeed, Stage::Space);
    let view = app
        .host
        .computer
        .eq
        .as_mut()
        .unwrap()
        .crossfeed
        .as_mut()
        .unwrap();
    view.preset = None;
    view.edited_from = Some("Chu Moy".to_string());
    let rows = rows_of(&app, 80, 24);
    assert!(
        holds(&rows, "Preset     ‹ your own ›  edited from Chu Moy"),
        "{rows:#?}"
    );
}

/// A set with no angles has a Length row and no Angles row; below 120 columns the curves are two
/// strips of blocks under the facts.
#[test]
fn the_speakers_panel_without_angles_and_the_narrow_strips() {
    let mut app = stages_app(Space::Speakers, Stage::Space);
    app.host
        .computer
        .eq
        .as_mut()
        .unwrap()
        .speakers
        .as_mut()
        .unwrap()
        .angles = String::new();
    let rows = rows_of(&app, 80, 24);
    assert!(
        !holds(&rows, "Angles") && holds(&rows, "Length"),
        "{rows:#?}"
    );
    assert!(
        holds(&rows, "Same ear  ") && holds(&rows, "Other ear"),
        "{rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|r| r.contains('▇') || r.contains('▆') || r.contains('▅') || r.contains('▄')),
        "{rows:#?}"
    );
}
