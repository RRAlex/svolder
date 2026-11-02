// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shimmer, the glow and the trail the loop arms from each frame's rows, and the rows they
//! follow.

use super::*;

/// The shimmer plays over the row the keys are acting on, which on the device screen is
/// the focused pane's own: the Controls pane's selected row, the Tone pane's focused band
/// over in the right-hand pane, and nothing at all in the Status pane, whose lines no key
/// moves through.
#[test]
fn the_shimmer_follows_the_focused_pane_to_its_own_row() {
    let mut app = gungnir();
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let cursor_row =
        |app: &App, areas: &FrameAreas| app.cursor_id().and_then(|id| row_rect(areas, id));

    let areas = row_areas(&app);
    let controls = cursor_row(&app, &areas).expect("the focused control's row");

    app.pane = Pane::Tone;
    let areas = row_areas(&app);
    let band = cursor_row(&app, &areas).expect("the focused band's row");
    assert!(
        band.x >= controls.right(),
        "the Tone pane's band is not in the pane beside the controls: {band:?} after {controls:?}"
    );
    let mut effects = shimmer_only();
    effects.on_key(t0);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0 + REMINDERS[0]);
    assert!(effects.active(), "the focused band did not shimmer");

    app.pane = Pane::Status;
    let areas = row_areas(&app);
    assert_eq!(cursor_row(&app, &areas), None, "the Status pane has no row");
    let mut effects = shimmer_only();
    effects.on_key(t0);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0 + REMINDERS[0]);
    assert!(
        !effects.active(),
        "the Status pane shimmered a row the keys are not on"
    );
}

/// An animation layer with the shimmer alone switched on and no transition, so that what
/// it is running is the shimmer and nothing else.
fn shimmer_only() -> Effects {
    let mut effects = capable_over_no_colour(&EffectsConfig {
        eased: false,
        glow: false,
        frames: false,
        ..EffectsConfig::default()
    });
    effects.set_transition(Transition::None);
    effects
}

/// What each frame earns is armed from the areas that frame drew: the row a value changed
/// on glows, the row the cursor left trails, and the shimmer answers what the user did —
/// the row their key wrote to, and the row the cursor moved onto.
#[test]
fn a_frames_rows_arm_the_glow_the_trail_and_the_shimmer_the_user_earned() {
    let mut app = gungnir();
    let cursor = app.cursor_id().expect("a row is focused from the start");
    let areas = FrameAreas {
        rows: vec![(cursor, Rect::new(0, 2, 30, 1))],
        ..FrameAreas::default()
    };
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let fresh = || Effects::new(true, &EffectsConfig::default());

    let mut effects = fresh();
    app.frame_flags.changed_rows.push(cursor);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(effects.active(), "the changed row did not glow");
    assert!(app.frame_flags.changed_rows.is_empty(), "the row was taken");

    let mut effects = fresh();
    app.frame_flags.left_row = Some(cursor);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(effects.active(), "the row the cursor left did not trail");
    assert_eq!(app.frame_flags.left_row, None, "the row was taken");

    let mut effects = shimmer_only();
    app.frame_flags.touched_row = Some(cursor);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(effects.active(), "the row the user changed did not sweep");
    assert_eq!(app.frame_flags.touched_row, None, "the row was taken");

    let mut effects = shimmer_only();
    app.frame_flags.left_row = Some(cursor);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(
        effects.active(),
        "the row the cursor moved onto did not sweep"
    );

    let mut effects = shimmer_only();
    let bare = FrameAreas::default();
    app.frame_flags.touched_row = Some(cursor);
    arm_effects(&mut app, &mut effects, &bare, &theme, t0);
    assert!(
        !effects.active(),
        "a frame that drew no rows has none to animate"
    );
}

/// Each one-shot flag arms its effect once: the frame after a change arms nothing more unless
/// something changes again.
#[test]
fn each_one_shot_flag_arms_its_effect_once() {
    let mut app = gungnir();
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let mut effects = Effects::new(true, &EffectsConfig::default());
    let areas = row_areas(&app);
    let cursor = app.cursor_id().expect("a row is focused from the start");
    assert!(
        areas.rows.iter().any(|(id, _)| *id == cursor),
        "the focused row is drawn"
    );
    // The screen's own frames arrive first.
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    run_out(&mut effects);

    type Raise = Box<dyn Fn(&mut App)>;
    let sets: Vec<(&str, Raise)> = vec![
        (
            "changed_rows",
            Box::new(move |app| app.frame_flags.changed_rows.push(cursor)),
        ),
        (
            "left_row",
            Box::new(move |app| app.frame_flags.left_row = Some(cursor)),
        ),
        (
            "touched_row",
            Box::new(move |app| app.frame_flags.touched_row = Some(cursor)),
        ),
        (
            "pending_flash",
            Box::new(move |app| app.frame_flags.pending_flash = Some(t0)),
        ),
        (
            "reconnected",
            Box::new(|app| app.frame_flags.reconnected = true),
        ),
        (
            "screen_changed",
            Box::new(|app| app.frame_flags.screen_changed = true),
        ),
        (
            "pane_changed",
            Box::new(|app| app.frame_flags.pane_changed = true),
        ),
    ];
    for (name, set) in sets {
        set(&mut app);
        arm_effects(&mut app, &mut effects, &areas, &theme, t0);
        assert!(effects.active(), "{name} armed nothing");
        run_out(&mut effects);
        arm_effects(&mut app, &mut effects, &areas, &theme, t0);
        assert!(
            !effects.active(),
            "{name} armed again on a frame with no change"
        );
    }

    let mut effects = shimmer_only();
    let listed = FrameAreas {
        cursor: Some(Selection {
            kind: Selected::Devices,
            index: 0,
            area: Rect::new(0, 2, 30, 1),
        }),
        ..FrameAreas::default()
    };
    arm_effects(&mut app, &mut effects, &listed, &theme, t0);
    run_out(&mut effects);
    app.frame_flags.touched_cursor = true;
    arm_effects(&mut app, &mut effects, &listed, &theme, t0);
    assert!(effects.active(), "touched_cursor armed nothing");
    run_out(&mut effects);
    arm_effects(&mut app, &mut effects, &listed, &theme, t0);
    assert!(!effects.active(), "touched_cursor armed twice");
}

/// A reading the device pushed by itself moves its own row and sweeps nothing: the
/// shimmer says the user changed something, and the row says the device did.
#[test]
fn a_reading_the_device_pushed_glows_its_row_and_sweeps_nothing() {
    let mut app = gungnir();
    let cursor = app.cursor_id().expect("a row is focused from the start");
    let areas = FrameAreas {
        rows: vec![(cursor, Rect::new(0, 2, 30, 1))],
        ..FrameAreas::default()
    };
    let theme = Theme::new(false);
    let t0 = Instant::now();

    let pushed = |raw: i64| {
        Update::Status(ValueObject {
            heard_at_ms: None,
            id: cursor.to_string(),
            key: None,
            label: "?".into(),
            value_type: "u8".into(),
            raw: json!(raw),
            display: raw.to_string(),
            value: None,
            unit: None,
        })
    };
    // The first reading of a function is not a change: the second one is.
    app.apply(pushed(3), t0);
    app.apply(pushed(4), t0);
    assert_eq!(
        app.frame_flags.changed_rows,
        vec![cursor],
        "the device's reading did not change the row"
    );
    assert_eq!(
        app.frame_flags.touched_row, None,
        "the device's reading touched a row"
    );

    let mut effects = shimmer_only();
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(!effects.active(), "a reading the device pushed swept a row");
}

/// The row the user's own key wrote to is the one that sweeps, wherever the cursor is:
/// the mute key turns the device's mute from any screen, so it is the mute's row that
/// answers rather than the row under the cursor.
#[test]
fn the_row_a_key_wrote_to_is_the_one_that_sweeps() {
    let mut app = gungnir();
    let mute = app
        .profile
        .control_by_key("mute")
        .expect("the Gungnir 2 has a mute")
        .id;
    app.selected = 0;
    assert_ne!(app.cursor_id(), Some(mute), "the cursor is on the mute row");

    app.key(KeyEvent::from(KeyCode::Char('m')));
    assert_eq!(
        app.frame_flags.touched_row,
        Some(mute),
        "the mute key did not name the row it wrote to"
    );

    let row = Rect::new(0, 4, 30, 1);
    let areas = FrameAreas {
        rows: vec![(mute, row)],
        ..FrameAreas::default()
    };
    let mut effects = shimmer_only();
    arm_effects(
        &mut app,
        &mut effects,
        &areas,
        &Theme::new(false),
        Instant::now(),
    );
    assert!(
        effects.active(),
        "the row the mute key wrote to did not sweep"
    );
}

/// While the user rests, the selected row sweeps again on the reminder schedule — once
/// at each of its instants — and nothing sweeps under an overlay they are reading or
/// typing into, or on the setup wizard.
#[test]
fn the_selected_row_is_reminded_on_the_schedule_and_never_under_an_overlay() {
    let mut app = gungnir();
    let cursor = app.cursor_id().expect("a row is focused from the start");
    let areas = FrameAreas {
        rows: vec![(cursor, Rect::new(0, 2, 30, 1))],
        ..FrameAreas::default()
    };
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let mut effects = shimmer_only();
    effects.on_key(t0);

    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(!effects.active(), "the shimmer swept on the key itself");

    for (at, due) in REMINDERS.iter().enumerate() {
        let mut effects = shimmer_only();
        effects.on_key(t0);
        for earlier in REMINDERS.iter().take(at) {
            arm_effects(&mut app, &mut effects, &areas, &theme, t0 + *earlier);
            run_out(&mut effects);
        }
        arm_effects(&mut app, &mut effects, &areas, &theme, t0 + *due);
        assert!(effects.active(), "no reminder {due:?} after the last key");
        run_out(&mut effects);
        arm_effects(&mut app, &mut effects, &areas, &theme, t0 + *due);
        assert!(!effects.active(), "the reminder {due:?} came round twice");
    }

    app.overlays.help_open = true;
    assert!(
        !reminding(&app, &FrameAreas::default()),
        "the overlay is up and the shimmer reminds"
    );
    let mut effects = shimmer_only();
    effects.on_key(t0);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0 + REMINDERS[0]);
    assert!(!effects.active(), "a reminder swept under the keys");
    app.overlays.help_open = false;

    let profile = app.profile.clone();
    let mut app = App::new(
        profile,
        "gungnir".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("gungnir")],
    );
    app.show(Screen::Devices);
    app.devices.configured = vec![DeviceConfig {
        name: "gungnir".into(),
        udid: "0102030405060708".into(),
        profile: None,
        passphrase: "auto".into(),
        inputs: std::collections::BTreeMap::new(),
    }];
    crate::tui::boxes::editor::open(&mut app);
    assert!(
        app.devices.editor.is_some(),
        "the device editor did not open"
    );
    assert!(
        !reminding(&app, &FrameAreas::default()),
        "the editor is up and the shimmer reminds"
    );
    app.devices.editor = None;

    app.devices.confirm = Some("gungnir".into());
    assert!(
        !reminding(&app, &FrameAreas::default()),
        "a removal is waiting and the shimmer reminds"
    );
    app.devices.confirm = None;

    app.show(Screen::Setup);
    assert!(
        !reminding(&app, &FrameAreas::default()),
        "the wizard is up and the shimmer reminds"
    );
}

/// The selections beyond the device screen's function rows, each with the app showing it and a
/// key that moves it: the Telemetry rows, the Devices list, the menu's entries, the Settings
/// box's rows, the preset list and the AutoEq search results.
fn selections() -> Vec<(&'static str, App, KeyCode)> {
    use crate::tui::app::SettingsBox;
    use crate::tui::test_support::app_showing;
    use crate::tui::test_support::presets::{own, resting, searching, two_headphones};
    use svolder_profiles::EqMode;

    let mut menu = app_showing(Screen::Device);
    menu.overlays.menu = Some(0);
    let mut settings = app_showing(Screen::Device);
    settings.overlays.settings = Some(SettingsBox::default());
    let mut presets = resting("gungnir2-v2", EqMode::Eq);
    presets.presets.saved = vec![own("HD650 warm", 5, 1.0), own("Bright", 5, -1.0)];
    presets.key(KeyEvent::from(KeyCode::Char('p')));
    let mut search = searching(two_headphones());
    for letter in "hd 6".chars() {
        search.key(KeyEvent::from(KeyCode::Char(letter)));
    }
    vec![
        (
            "the Telemetry rows",
            app_showing(Screen::Telemetry),
            KeyCode::Down,
        ),
        (
            "the Devices list",
            app_showing(Screen::Devices),
            KeyCode::Down,
        ),
        ("the menu", menu, KeyCode::Down),
        ("the Settings box", settings, KeyCode::Down),
        ("the preset list", presets, KeyCode::Down),
        ("the search results", search, KeyCode::Down),
    ]
}

/// The cells of `buffer` the shimmer's band is on: white and bold, which no screen drawn in no
/// colour has by itself.
fn banded(buffer: &Buffer) -> Vec<(u16, u16)> {
    buffer
        .content()
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.fg == Color::White && cell.modifier.contains(Modifier::BOLD))
        .map(|(at, _)| {
            let width = usize::from(buffer.area.width);
            ((at % width) as u16, (at / width) as u16)
        })
        .collect()
}

/// Every selection the keys move shimmers its row when the cursor moves onto it, and again on
/// the reminder schedule while the user rests; a redraw of the same selection sweeps nothing.
#[test]
fn every_selection_shimmers_on_a_cursor_move_and_on_a_reminder() {
    let theme = Theme::new(false);
    let t0 = Instant::now();
    for (what, mut app, code) in selections() {
        app.frame_flags.screen_changed = false;
        let mut effects = shimmer_only();
        let areas = row_areas(&app);
        assert!(areas.cursor.is_some(), "{what} reported no selected row");
        arm_effects(&mut app, &mut effects, &areas, &theme, t0);
        run_out(&mut effects);

        // A redraw with the cursor where it was sweeps nothing.
        effects.on_key(t0);
        let areas = row_areas(&app);
        arm_effects(&mut app, &mut effects, &areas, &theme, t0);
        assert!(!effects.active(), "{what}: a redraw swept the row");

        // The cursor moving onto the next row sweeps that row and no other.
        effects.on_key(t0);
        app.key(KeyEvent::from(code));
        let moved = row_areas(&app);
        let cursor = moved.cursor.expect("the selection is drawn").area;
        assert_ne!(
            Some(cursor),
            areas.cursor.map(|c| c.area),
            "{what} did not move"
        );
        arm_effects(&mut app, &mut effects, &moved, &theme, t0);
        assert!(
            effects.active(),
            "{what}: the cursor's move did not shimmer"
        );
        let lit = banded(&animated(&app, &mut effects, Duration::from_millis(100)));
        assert!(!lit.is_empty(), "{what}: the band is nowhere");
        assert!(
            lit.iter().all(|(x, y)| cursor.contains((*x, *y).into())),
            "{what}: the band left the selected row {cursor:?}: {lit:?}"
        );
        run_out(&mut effects);

        // At rest the selected row is swept again on the schedule.
        arm_effects(
            &mut app,
            &mut effects,
            &moved,
            &theme,
            t0 + REMINDERS[0] / 2,
        );
        assert!(
            !effects.active(),
            "{what}: swept before the reminder was due"
        );
        arm_effects(&mut app, &mut effects, &moved, &theme, t0 + REMINDERS[0]);
        assert!(effects.active(), "{what}: the reminder did not shimmer");
    }
}

/// A value the user writes from a Settings row sweeps that row, and a reading the device pushes
/// sweeps nothing on the Telemetry list.
#[test]
fn a_settings_write_shimmers_its_row_and_a_pushed_reading_sweeps_nothing() {
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let (_, mut app, _) = selections()
        .into_iter()
        .find(|(what, ..)| *what == "the Settings box")
        .expect("the Settings box is one of the selections");
    app.frame_flags.screen_changed = false;
    let mut effects = shimmer_only();
    let areas = row_areas(&app);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    run_out(&mut effects);
    effects.on_key(t0);
    app.key(KeyEvent::from(KeyCode::Right));
    let areas = row_areas(&app);
    let row = areas.cursor.expect("the row is drawn").area;
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(
        effects.active(),
        "a value written from the row did not shimmer"
    );
    let lit = banded(&animated(&app, &mut effects, Duration::from_millis(100)));
    assert!(!lit.is_empty() && lit.iter().all(|(x, y)| row.contains((*x, *y).into())));

    let mut app = crate::tui::test_support::app_showing(Screen::Telemetry);
    app.frame_flags.screen_changed = false;
    let mut effects = shimmer_only();
    let areas = row_areas(&app);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    run_out(&mut effects);
    crate::tui::test_support::hear(
        &mut app,
        vec![crate::tui::test_support::plain_vo("99.1", 7)],
    );
    let areas = row_areas(&app);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(!effects.active(), "a reading the device pushed swept a row");
}

/// The rows under an open menu never sweep, the menu's own selection does, and the help overlay,
/// which has no selection, never sweeps at all.
#[test]
fn a_row_under_an_open_menu_does_not_shimmer_but_the_menus_own_selection_does() {
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let mut app = crate::tui::test_support::app_showing(Screen::Telemetry);
    app.frame_flags.screen_changed = false;
    app.overlays.menu = Some(0);
    let mut effects = shimmer_only();
    let areas = row_areas(&app);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    run_out(&mut effects);
    // The list under the menu moves with the menu open: nothing sweeps it.
    effects.on_key(t0);
    app.selected += 1;
    let areas = row_areas(&app);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(!effects.active(), "a row under the menu swept");
    // At rest it is the menu's own entry that is swept.
    let menu = areas.cursor.expect("the menu's entry is drawn").area;
    arm_effects(&mut app, &mut effects, &areas, &theme, t0 + REMINDERS[0]);
    assert!(effects.active(), "the menu's own selection did not shimmer");
    let lit = banded(&animated(&app, &mut effects, Duration::from_millis(100)));
    assert!(!lit.is_empty() && lit.iter().all(|(x, y)| menu.contains((*x, *y).into())));

    app.overlays.menu = None;
    app.overlays.help_open = true;
    let mut effects = shimmer_only();
    let areas = row_areas(&app);
    assert!(
        areas.cursor.is_none(),
        "the help overlay reported a selection"
    );
    effects.on_key(t0);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0 + REMINDERS[0]);
    assert!(!effects.active(), "the help overlay swept");
}
