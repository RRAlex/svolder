// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's side of the screen: the switch, the Points table and the target row.

use super::*;

/// `Host EQ NAME` is in the wide tier's header while the computer EQ is on, and in no other
/// tier, nor once it is off.
#[test]
fn the_wide_header_names_the_computer_eq_while_it_is_on() {
    let mut app = flat_app();
    app.host.computer.eq = Some(host_eq_view(true));
    let wide = rows_of(&app, 170, 40);
    assert!(holds(&wide, "Host EQ Phones"), "{wide:?}");
    let narrow = rows_of(&app, 80, 24);
    assert!(!holds(&narrow, "Host EQ"), "{narrow:?}");
    app.host.computer.eq = Some(host_eq_view(false));
    let off = rows_of(&app, 170, 40);
    assert!(!holds(&off, "Host EQ"), "{off:?}");
}

/// What the computer EQ's view holds for `Phones`.
fn host_eq_view(on: bool) -> Box<svolder_host::eq::EqView> {
    let mut eq = crate::tui::test_support::computer_eq(on, &[("PK", 1000.0, 2.0, 1.0)]);
    eq.preset = Some("Phones".into());
    eq
}

/// The top line names the shown device's own name and the computer, whichever side is shown, and
/// reads as the DAC's own line where the computer's controls are not there.
#[test]
fn the_eq_tab_reads_eq_for_the_device_or_computer() {
    let mut app = computer_app();
    let name = app.session.product.clone();
    let line = format!("Target ‹ {name} | Computer ›");
    assert!(holds(&rows_of(&app, 120, 30), &line), "{line}");
    app.key(KeyEvent::from(KeyCode::Char('C')));
    assert!(app.on_computer());
    assert!(holds(&rows_of(&app, 120, 30), &line));
    let bare = flat_app();
    assert!(!holds(&rows_of(&bare, 120, 30), "Target"));
}

/// `C` flips the side and writes nothing, and a click on either name of the switch shows that
/// side.
#[test]
fn c_and_a_click_flip_the_side() {
    let mut app = computer_app();
    assert!(app.key(KeyEvent::from(KeyCode::Char('C'))).is_empty());
    assert!(app.on_computer());
    assert!(app.key(KeyEvent::from(KeyCode::Char('C'))).is_empty());
    assert!(!app.on_computer());
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    let (y, row) = text(terminal.backend().buffer())
        .into_iter()
        .enumerate()
        .find(|(_, row)| row.contains("Target"))
        .expect("the switch is drawn");
    let y = u16::try_from(y).unwrap();
    let byte = row.find("Computer").expect("the switch is drawn");
    let at = u16::try_from(row[..byte].chars().count()).unwrap();
    assert_eq!(
        hits.hit_at(at, y),
        Some(&Hit::Side(crate::tui::app::Side::Computer))
    );
    let click = crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: at,
        row: y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    assert!(matches!(
        crate::tui::mouse::resolve(&hits, click, &app),
        Some(crate::tui::mouse::MouseAction::Target(Some(
            crate::tui::app::Side::Computer
        )))
    ));
}

/// On the computer the mode is written `‹ Off | EQ ›` and there is no Loudness.
#[test]
fn the_computer_mode_reads_off_or_eq_with_no_loudness() {
    let mut app = computer_app();
    assert!(holds(&rows_of(&app, 120, 30), "Loudness"));
    app.key(KeyEvent::from(KeyCode::Char('C')));
    let rows = rows_of(&app, 120, 30);
    assert!(holds(&rows, "Mode ‹ Off | EQ ›"), "{rows:?}");
    assert!(!holds(&rows, "Loudness"));
}

/// The Points pane has the columns, a row per point in its type, and its title carries the count.
#[test]
fn the_points_pane_has_type_freq_gain_q() {
    let mut app = computer_app();
    app.key(KeyEvent::from(KeyCode::Char('C')));
    let rows = rows_of(&app, 120, 30);
    for word in ["Points · computer", "3 of 16", "#  Type  Freq", "Gain", "Q"] {
        assert!(holds(&rows, word), "{word}: {rows:?}");
    }
    for kind in ["PK", "LSC", "HSC"] {
        assert!(holds(&rows, &format!("  {kind} ")), "{kind}");
    }
    assert!(holds(&rows, "preamp \u{2212}6.0 dB"));
}

/// Sixteen points do not grow the table: it scrolls, keeping the focused row in view, with a mark
/// for the rows out of sight.
#[test]
fn sixteen_points_scroll_with_the_selection_in_view() {
    let mut app = computer_app();
    let sixteen: Vec<(&str, f64, f64, f64)> = (0..16)
        .map(|n| ("PK", 100.0 * f64::from(n + 1), 1.0, 1.0))
        .collect();
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq(true, &sixteen));
    app.key(KeyEvent::from(KeyCode::Char('C')));
    let top = rows_of(&app, 120, 30);
    assert!(holds(&top, "↓ more") && !holds(&top, "↑ more"), "{top:?}");
    assert!(holds(&top, "16 of 16"));
    app.focus_point(15);
    let bottom = rows_of(&app, 120, 30);
    assert!(holds(&bottom, "↑ more") && !holds(&bottom, "↓ more"));
    assert!(
        bottom
            .iter()
            .any(|row| row.contains("▶") && row.contains("1600 Hz")),
        "the last point is in view and marked: {bottom:?}"
    );
    assert_eq!(
        rows_of(&app, 120, 30).len(),
        top.len(),
        "the frame keeps its size"
    );
}

/// The Correction pane says nothing is fitted on the computer, in the wide tier.
#[test]
fn the_correction_pane_says_nothing_is_fitted_on_the_computer() {
    let mut app = computer_app();
    app.key(KeyEvent::from(KeyCode::Char('C')));
    let rows = rows_of(&app, 140, 40);
    assert!(holds(&rows, "Correction"), "{rows:?}");
    let joined = rows.join(" ");
    let joined: Vec<&str> = joined.split_whitespace().collect();
    let said = "on the computer nothing is fitted: every filter plays as written";
    for word in said.split_whitespace() {
        assert!(joined.contains(&word), "{word}");
    }
}

/// A chain whose running could not be told is shown as the state file says and marked not
/// verified; a verified one carries no mark.
#[test]
fn an_unknown_chain_is_marked_not_verified() {
    let mut app = computer_app();
    app.key(KeyEvent::from(KeyCode::Char('C')));
    assert!(!holds(&rows_of(&app, 120, 30), "not verified"));
    app.host
        .computer
        .eq
        .as_mut()
        .expect("an eq")
        .running_unknown = true;
    let rows = rows_of(&app, 120, 30);
    assert!(holds(&rows, "Mode ‹ Off | EQ › not verified"), "{rows:?}");
}

/// A click on a point's type cell cycles it, one on a value focuses the point; the cells are hits.
#[test]
fn a_point_s_cells_are_clickable() {
    let mut app = computer_app();
    app.key(KeyEvent::from(KeyCode::Char('C')));
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    let rows = text(terminal.backend().buffer());
    let y = rows
        .iter()
        .position(|row| row.contains(" LSC "))
        .expect("the LSC row");
    let byte = rows[y].find("LSC").expect("its type");
    let x = u16::try_from(rows[y][..byte].chars().count()).unwrap();
    assert_eq!(
        hits.hit_at(x, u16::try_from(y).unwrap()),
        Some(&Hit::PointField(1, 0))
    );
    let intents = app.click_point_field(1, 0);
    assert!(
        matches!(&intents[..], [Intent::HostEq(op)] if matches!(&**op, svolder_ipc::HostEqOp::Set { index: 2, kind: Some(k), .. } if k == "HSC"))
    );
}

/// The switch is a row of its own above Mode: `↑` from Mode reaches it, `←→` pick the side,
/// `↓` goes back to Mode whose `←→` keep their own values, and `C` still flips.
#[test]
fn the_tone_screens_switch_is_a_row_above_mode() {
    let mut app = computer_app();
    app.screen = Screen::Tone;
    app.tone.row = ToneRow::Mode;
    let key = |app: &mut App, code| app.key(KeyEvent::from(code));
    assert!(!app.switch_focused());
    key(&mut app, KeyCode::Up);
    assert!(app.switch_focused());
    assert!(key(&mut app, KeyCode::Right).is_empty());
    assert!(app.on_computer());
    assert!(key(&mut app, KeyCode::Left).is_empty());
    assert!(!app.on_computer());
    key(&mut app, KeyCode::Down);
    assert!(!app.switch_focused());
    assert_eq!(app.tone.row, ToneRow::Mode);
    assert!(
        !key(&mut app, KeyCode::Right).is_empty(),
        "Mode steps its own values"
    );
    assert!(!app.on_computer());
    key(&mut app, KeyCode::Char('C'));
    assert!(app.on_computer());
}

/// On the tone screen the Target row is the first stop: `↓` from it goes to Mode and down the
/// rows, `↑` back up the same ones to it, and it has a cursor of its own for the trail.
#[test]
fn the_tone_screens_target_row_is_the_first_stop_both_ways() {
    let mut app = computer_app();
    app.screen = Screen::Tone;
    app.tone.row = ToneRow::Target;
    let key = |app: &mut App, code| app.key(KeyEvent::from(code));
    assert!(app.switch_focused());
    let target = app.cursor_id().expect("the Target row has a cursor");
    let mut seen = vec![app.tone.row];
    for _ in 0..8 {
        key(&mut app, KeyCode::Down);
        if seen.last() != Some(&app.tone.row) {
            seen.push(app.tone.row);
        }
    }
    assert_eq!(seen[..2], [ToneRow::Target, ToneRow::Mode], "{seen:?}");
    assert!(seen.len() > 2, "{seen:?}");
    for _ in 0..12 {
        key(&mut app, KeyCode::Up);
    }
    assert!(
        app.switch_focused(),
        "↑ reaches Target again: {:?}",
        app.tone.row
    );
    key(&mut app, KeyCode::Down);
    assert_eq!(
        app.frame_flags.left_row,
        Some(target),
        "the trail leaves Target"
    );
    key(&mut app, KeyCode::Up);
    assert_eq!(app.cursor_id(), Some(target));
    assert!(app.frame_flags.left_row.is_some(), "the trail leaves Mode");
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut rows = Vec::new();
    terminal
        .draw(|f| {
            rows = render::draw(f, &app, &Theme::new(false), &mut hits).rows;
        })
        .expect("the frame draws");
    assert!(
        rows.iter()
            .any(|(id, rect)| *id == target && rect.height == 1),
        "the Target row is drawn as a row the trail can play over"
    );
}

/// The selected Target line boxes the side shown and leaves the other plain dim; unselected the
/// side shown is bold, in colour and in monochrome.
#[test]
fn the_selected_target_line_boxes_the_side_shown_on_the_tone_screen() {
    use crate::tui::test_support::target_sides;
    for mono in [false, true] {
        let theme = Theme::new(mono);
        let mut app = computer_app();
        app.screen = Screen::Tone;
        app.tone.row = ToneRow::Mode;
        let name = app.session.product.clone();
        let shot = |app: &App| target_sides(&frame_with_theme(app, 120, 30, &theme), &name);
        let (line, dac, computer) = shot(&app);
        eprintln!("mono={mono} unselected, device : {line}");
        assert!(dac.0.contains(Modifier::BOLD) && !dac.0.contains(Modifier::REVERSED));
        assert!(!computer.0.contains(Modifier::BOLD));
        app.key(KeyEvent::from(KeyCode::Up));
        let (line, dac, computer) = shot(&app);
        eprintln!("mono={mono} selected,   device : {line}");
        assert!(
            dac.0.contains(Modifier::BOLD | Modifier::REVERSED),
            "{dac:?}"
        );
        assert!(!computer.0.contains(Modifier::REVERSED), "{computer:?}");
        assert_ne!(dac, computer);
        app.key(KeyEvent::from(KeyCode::Right));
        let (line, dac, computer) = shot(&app);
        eprintln!("mono={mono} selected,   computer: {line}");
        assert!(
            computer.0.contains(Modifier::BOLD | Modifier::REVERSED),
            "{computer:?}"
        );
        assert!(!dac.0.contains(Modifier::REVERSED));
    }
}

/// The switch row is above the Mode row on both sides and at 80 columns, `Target` leads it, and
/// the focused one wears the cursor while Mode does not.
#[test]
fn the_switch_row_draws_above_mode_on_both_sides_and_narrow() {
    let mut app = computer_app();
    app.screen = Screen::Tone;
    app.tone.row = ToneRow::Mode;
    for (width, height) in [(120, 30), (80, 24)] {
        for computer in [false, true] {
            if app.on_computer() != computer {
                app.key(KeyEvent::from(KeyCode::Char('C')));
            }
            let rows = rows_of(&app, width, height);
            let switch = rows
                .iter()
                .position(|r| r.contains("Target ‹ "))
                .expect("switch");
            let mode = rows.iter().position(|r| r.contains("Mode ")).expect("mode");
            assert!(switch < mode, "{rows:#?}");
            assert!(!rows[switch].contains("Mode"), "{}", rows[switch]);
            assert!(rows[switch].contains("mouse:"));
            if width == 80 {
                let words = rows.iter().filter(|r| r.contains("svolder")).count();
                assert_eq!(words, 1, "the word is on the switch's row only: {rows:#?}");
            }
        }
    }
    let name = app.session.product.clone();
    app.key(KeyEvent::from(KeyCode::Up));
    let rows = rows_of(&app, 120, 30);
    assert!(holds(&rows, &format!("▶ Target ‹ {name}")));
}

/// Where the computer's controls cannot be used, the header keeps one line with the reason, and
/// `↑` from Mode does not focus it.
#[test]
fn the_tone_screens_reason_line_is_not_focusable() {
    let mut app = computer_app();
    app.host.computer.eq.as_mut().expect("an eq").unavailable =
        Some("computer controls unavailable: no pw-cli on this computer".into());
    app.screen = Screen::Tone;
    app.tone.row = ToneRow::Mode;
    let rows = rows_of(&app, 120, 30);
    assert!(
        rows.iter()
            .any(|r| r.contains("Target") && r.contains("unavailable"))
    );
    app.key(KeyEvent::from(KeyCode::Up));
    assert!(!app.switch_focused());
}

/// Moving to a point with the switch focused leaves the switch: `←` then edits the point.
#[test]
fn adding_a_point_from_the_target_row_leaves_it() {
    let mut app = computer_app();
    app.screen = Screen::Tone;
    app.key(KeyEvent::from(KeyCode::Char('C')));
    assert!(app.on_computer());
    app.tone.row = ToneRow::Mode;
    app.key(KeyEvent::from(KeyCode::Up));
    assert!(app.switch_focused());
    app.key(KeyEvent::from(KeyCode::Char('a')));
    assert!(
        matches!(app.tone.row, ToneRow::Point(_)),
        "{:?}",
        app.tone.row
    );
    assert!(!app.switch_focused());
    app.key(KeyEvent::from(KeyCode::Left));
    assert!(
        app.on_computer(),
        "←← edits the point, it does not flip the side"
    );
}

/// Tab, a click on a band and a screen change take the cursor off the switch; the reason line
/// is never reached, and its return does not focus the switch again.
#[test]
fn the_tone_screens_target_row_goes_with_the_focus() {
    let mut app = computer_app();
    app.screen = Screen::Tone;
    app.tone.row = ToneRow::Mode;
    let up = |app: &mut App| app.key(KeyEvent::from(KeyCode::Up));
    up(&mut app);
    assert!(app.switch_focused());
    app.key(KeyEvent::from(KeyCode::Tab));
    assert!(!app.switch_focused());
    app.tone.row = ToneRow::Mode;
    up(&mut app);
    app.focus_band(2);
    assert!(!app.switch_focused());
    app.tone.row = ToneRow::Mode;
    up(&mut app);
    app.focus_loudness();
    assert!(!app.switch_focused());
    app.tone.row = ToneRow::Mode;
    up(&mut app);
    app.show(Screen::Device);
    assert_ne!(app.tone.row, ToneRow::Target);
    app.show(Screen::Tone);
    assert!(!app.switch_focused());
    // unavailable and back
    app.tone.row = ToneRow::Mode;
    up(&mut app);
    app.host.computer.eq.as_mut().expect("an eq").unavailable = Some("no pw-cli".into());
    app.key(KeyEvent::from(KeyCode::Left));
    app.host.computer.eq.as_mut().expect("an eq").unavailable = None;
    assert!(!app.switch_focused());
}

/// On the Target row `m`, `+` and `-` do nothing.
#[test]
fn only_the_switchs_own_keys_act_on_the_target_row() {
    let mut app = computer_app();
    app.screen = Screen::Tone;
    app.tone.row = ToneRow::Mode;
    app.key(KeyEvent::from(KeyCode::Up));
    for key in ['m', '+', '-'] {
        assert!(
            app.key(KeyEvent::from(KeyCode::Char(key))).is_empty(),
            "{key}"
        );
    }
    assert!(app.switch_focused());
}
