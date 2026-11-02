// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A change made in the Settings box: what the loop applies on the screen, what it writes to the
//! configuration file, and what it says when the file will not take it.

use super::*;

/// A settings change never reaches the device: the loop takes it into the `[tui]` table it
/// draws from, rebuilds the theme and the animation layer around it, marks the sweep a
/// palette earns, and writes the table back to the configuration file.
#[tokio::test(start_paused = true)]
async fn a_settings_change_is_applied_to_the_screen_and_written_to_the_file() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut mouse = SpyMouse::default();
    let mut look = look();
    let path = look.path.clone();
    let mut watching = watching(backend, app, None);
    watching.app.frame_flags.screen_changed = false;

    let flow = take_intent(
        Intent::Tui(TuiChange::Palette("neon".into())),
        &opener_over(never_opens()),
        watching,
        &mut mouse,
        &mut look,
        &mut (),
    )
    .await;
    let Flow::Continue(watching) = flow else {
        panic!("a settings change does not end the loop");
    };

    assert_eq!(look.tui.palette, "neon");
    assert_eq!(
        watching.app.tui.palette, "neon",
        "the screen draws the new palette"
    );
    assert!(
        watching.app.frame_flags.screen_changed,
        "a palette earns the sweep"
    );
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.clone()),
        None,
        "{:?}",
        watching.app.toast().map(|toast| toast.text.clone())
    );
    let saved = Config::parse(&std::fs::read_to_string(&path).expect("the file was written"))
        .expect("what was written loads");
    assert_eq!(saved.tui.palette, "neon");

    // Mouse capture is turned where the loop turns it, and the switches the app draws
    // from follow the table.
    let flow = take_intent(
        Intent::Tui(TuiChange::Mouse(false)),
        &opener_over(never_opens()),
        watching,
        &mut mouse,
        &mut look,
        &mut (),
    )
    .await;
    let Flow::Continue(watching) = flow else {
        panic!("a settings change does not end the loop");
    };
    assert_eq!(mouse.on, Some(false));
    assert!(!watching.app.mouse);
    assert!(
        !Config::parse(&std::fs::read_to_string(&path).unwrap())
            .unwrap()
            .tui
            .mouse
    );

    watching.close().await;
}

/// The animation layer's own switch is turned the way every other setting is, and saved
/// where a hand edit would have put it: `E` is the Settings box's row under another
/// name.
#[tokio::test(start_paused = true)]
async fn the_effects_switch_flips_on_the_screen_and_in_the_file() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut look = look();
    let path = look.path.clone();
    assert!(look.tui.effects.enabled, "the default has them on");

    let watching = change(
        &mut look,
        watching(backend, app, None),
        TuiChange::Effects(false),
    )
    .await;

    assert!(!look.tui.effects.enabled, "the layer is still switched on");
    assert!(
        !watching.app.tui.effects.enabled,
        "the screen still says on"
    );
    assert!(!watching.app.effects, "the layer is still running");
    let saved = Config::parse(&std::fs::read_to_string(&path).expect("the file was written"))
        .expect("what was written loads");
    assert!(!saved.tui.effects.enabled, "the switch was not saved");
    watching.close().await;
}

/// The title meter's own switch is turned the way every other setting is and saved where a
/// hand edit would have put it: `V` is the Settings box's row under another name.
#[tokio::test(start_paused = true)]
async fn the_title_meter_switch_flips_on_the_screen_and_in_the_file() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut look = look();
    let path = look.path.clone();
    assert!(look.tui.title_meter, "the default has it on");

    let watching = change(
        &mut look,
        watching(backend, app, None),
        TuiChange::Set(&TITLE_METER, TuiValue::Bool(false)),
    )
    .await;

    assert!(!look.tui.title_meter, "the meter is still switched on");
    assert!(
        !watching.app.tui.title_meter,
        "the settings row still says on"
    );
    let saved = Config::parse(&std::fs::read_to_string(&path).expect("the file was written"))
        .expect("what was written loads");
    assert!(!saved.tui.title_meter, "the switch was not saved");
    watching.close().await;
}

/// The theme background switch is saved alone, where a hand edit would put it, and the theme
/// every screen draws through stops painting at once.
#[tokio::test(start_paused = true)]
async fn the_theme_background_switch_flips_in_the_theme_and_in_the_file() {
    use crate::tui::theme::Surface;

    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut look = colourful_look();
    let path = look.path.clone();
    let before = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        look.theme.surface(Surface::Ground).is_some(),
        "areas by default"
    );

    let watching = change(
        &mut look,
        watching(backend, app, None),
        TuiChange::Set(&THEME_BACKGROUND, TuiValue::Bool(false)),
    )
    .await;

    assert!(!look.tui.theme_background);
    assert!(!watching.app.tui.theme_background, "the row still says on");
    assert_eq!(
        look.theme.surface(Surface::Ground),
        None,
        "the theme still paints"
    );
    let text = std::fs::read_to_string(&path).expect("the file was written");
    let saved = Config::parse(&text).expect("what was written loads");
    assert!(!saved.tui.theme_background, "the switch was not saved");
    assert_eq!(
        Config::parse(&before).expect("the file loaded").tui,
        TuiConfig {
            theme_background: true,
            ..saved.tui.clone()
        },
        "another key changed"
    );
    watching.close().await;
}

/// Host audio and the visualizer are turned the way every other setting is and saved where
/// a hand edit would have put them: the Settings box's rows are the only way to them.
#[tokio::test(start_paused = true)]
async fn the_host_audio_and_visualizer_switches_flip_on_the_screen_and_in_the_file() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut look = look();
    let path = look.path.clone();
    assert!(
        look.tui.host_audio && look.tui.visualizer,
        "the default has both on"
    );

    let watching = change(
        &mut look,
        watching(backend, app, None),
        TuiChange::Set(&HOST_AUDIO, TuiValue::Bool(false)),
    )
    .await;
    let watching = change(
        &mut look,
        watching,
        TuiChange::Set(&VISUALIZER, TuiValue::Bool(false)),
    )
    .await;

    assert!(!look.tui.host_audio && !look.tui.visualizer);
    assert!(
        !watching.app.tui.host_audio && !watching.app.tui.visualizer,
        "the settings rows still say on"
    );
    let saved = Config::parse(&std::fs::read_to_string(&path).expect("the file was written"))
        .expect("what was written loads");
    assert!(!saved.tui.host_audio, "host audio was not saved");
    assert!(!saved.tui.visualizer, "the visualizer was not saved");
    watching.close().await;
}

/// The transition is chosen the way every other setting is and saved where a hand edit
/// would have put it, and the animation layer takes it at once: with `none` a screen
/// change traces nothing, and with the trace back it does.
#[tokio::test(start_paused = true)]
async fn the_transition_word_reaches_the_layer_and_the_file() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut look = colourful_look();
    let path = look.path.clone();
    let mut watching = watching(backend, app, None);
    watching.app.set_look(&look.tui, &look.effects);
    let areas = row_areas(&watching.app);
    let theme = Theme::new(false);

    watching = change(
        &mut look,
        watching,
        TuiChange::Set(&TRANSITION, TuiValue::Word(Transition::None.name())),
    )
    .await;
    assert_eq!(watching.app.tui.transition, Transition::None);
    let saved = Config::parse(&std::fs::read_to_string(&path).expect("the file was written"))
        .expect("what was written loads");
    assert_eq!(
        saved.tui.transition,
        Transition::None,
        "the word was not saved"
    );
    watching.app.frame_flags.screen_changed = true;
    arm_effects(
        &mut watching.app,
        &mut look.effects,
        &areas,
        &theme,
        Instant::now(),
    );
    // The light the arrival earns runs at once; the trace and the sweep are what none lacks.
    let drawn = animated(&watching.app, &mut look.effects, Duration::from_millis(20));
    assert!(
        drawn.content().iter().all(|cell| cell.fg != SWEPT_FROM),
        "a screen change traced under none"
    );

    watching = change(
        &mut look,
        watching,
        TuiChange::Set(&TRANSITION, TuiValue::Word(Transition::Trace.name())),
    )
    .await;
    watching.app.frame_flags.screen_changed = true;
    arm_effects(
        &mut watching.app,
        &mut look.effects,
        &areas,
        &theme,
        Instant::now(),
    );
    assert!(
        look.effects.active(),
        "the trace named again traced nothing"
    );
    watching.close().await;
}

/// A change on a session that started before the file was deleted writes the template with
/// that one change, not the session's other values.
#[tokio::test(start_paused = true)]
async fn a_change_after_the_file_is_deleted_writes_the_template_and_that_change() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    let mut mouse = SpyMouse::default();
    let mut look = look();
    look.tui.intro = false;
    look.tui.palette = "neon".into();
    assert!(!look.path.exists());

    look.change(&TuiChange::Mouse(false), &mut app, &mut mouse);

    let written = Config::load_from(&look.path).unwrap().tui;
    let mut expected = Config::parse(&Config::template()).unwrap().tui;
    expected.mouse = false;
    assert_eq!(written, expected);
    assert!(!look.tui.intro, "the session keeps what it holds");
    drop(backend);
}

/// Turning the intro sound on from the Settings box saves the one key and holds it for the
/// session.
#[tokio::test(start_paused = true)]
async fn the_intro_sound_setting_is_saved_and_held() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    let mut mouse = SpyMouse::default();
    let mut look = look();
    std::fs::create_dir_all(look.path.parent().expect("the file has a directory")).unwrap();
    std::fs::write(&look.path, "schema = 1\n").unwrap();

    look.change(
        &TuiChange::Set(&INTRO_SOUND, TuiValue::Bool(true)),
        &mut app,
        &mut mouse,
    );

    assert!(Config::load_from(&look.path).unwrap().tui.intro_sound);
    assert!(look.tui.intro_sound);
    drop(backend);
}

/// A bare `accent` the file carries survives a change to another key, as the quote style it
/// was written in.
#[tokio::test(start_paused = true)]
async fn a_change_to_another_key_leaves_a_bare_accent_on_file() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    let mut mouse = SpyMouse::default();
    let mut look = look();
    std::fs::create_dir_all(look.path.parent().expect("the file has a directory")).unwrap();
    let original = "schema = 1\n[tui]\naccent = '#112233'\nmouse = true\n";
    std::fs::write(&look.path, original).unwrap();

    look.change(&TuiChange::Mouse(false), &mut app, &mut mouse);

    assert_eq!(
        std::fs::read_to_string(&look.path).unwrap(),
        original.replace("mouse = true", "mouse = false")
    );
    drop(backend);
}

/// A change that could not be saved is said on the key line and still applies: the user
/// asked for it, and a file that will not take it is no reason to refuse.
#[tokio::test(start_paused = true)]
async fn a_settings_change_that_cannot_be_saved_is_reported_and_still_applies() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut mouse = SpyMouse::default();
    let mut look = look();
    // The parent of this path is a file, so neither the directory nor the file under it
    // can be made.
    std::fs::create_dir_all(look.path.parent().expect("the file has a directory")).unwrap();
    std::fs::write(&look.path, "schema = 1\n").unwrap();
    let blocked = look.path.clone();
    look.path = blocked.join("nested/config.toml");

    let flow = take_intent(
        Intent::Tui(TuiChange::Set(&INTRO, TuiValue::Bool(false))),
        &opener_over(never_opens()),
        watching(backend, app, None),
        &mut mouse,
        &mut look,
        &mut (),
    )
    .await;
    let Flow::Continue(watching) = flow else {
        panic!("a settings change does not end the loop");
    };

    assert!(!look.tui.intro, "the setting stands for the session");
    assert!(!watching.app.tui.intro);
    let notice = watching
        .app
        .toast()
        .map(|toast| toast.text.clone())
        .expect("a failed save is said");
    assert!(notice.starts_with("settings not saved: "), "got {notice:?}");
    watching.close().await;
}

/// A palette change made in the Settings box plays no screen sweep: the box is drawn in the
/// theme's own colours and shows the change on the next frame.
#[tokio::test(start_paused = true)]
async fn a_change_made_in_the_settings_box_sweeps_nothing() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.key(crossterm::event::KeyEvent::from(KeyCode::Char('s')));
    assert!(app.overlays.settings.is_some());
    let mut mouse = SpyMouse::default();
    let mut look = look();
    let mut watching = watching(backend, app, None);
    watching.app.frame_flags.screen_changed = false;
    for change in [
        TuiChange::Palette("neon".into()),
        TuiChange::Colors(ColorDepth::Ansi256),
    ] {
        let flow = take_intent(
            Intent::Tui(change),
            &opener_over(never_opens()),
            watching,
            &mut mouse,
            &mut look,
            &mut (),
        )
        .await;
        let Flow::Continue(next) = flow else {
            panic!("a settings change does not end the loop");
        };
        watching = next;
        assert!(
            !watching.app.frame_flags.screen_changed,
            "the box swept for a change"
        );
    }
    assert_eq!(look.tui.palette, "neon", "the change still applied");
    watching.close().await;
}

/// A failed save inside the Settings box shows its toast over the box at full strength, and
/// the toast takes no key: the next key still goes to the box.
#[tokio::test(start_paused = true)]
async fn a_failed_save_inside_the_settings_box_is_a_toast_over_the_box_taking_no_key() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.key(crossterm::event::KeyEvent::from(KeyCode::Char('s')));
    let mut mouse = SpyMouse::default();
    let mut look = look();
    std::fs::create_dir_all(look.path.parent().expect("the file has a directory")).unwrap();
    std::fs::write(&look.path, "schema = 1\n").unwrap();
    let blocked = look.path.clone();
    look.path = blocked.join("nested/config.toml");

    let flow = take_intent(
        Intent::Tui(TuiChange::Set(&INTRO, TuiValue::Bool(false))),
        &opener_over(never_opens()),
        watching(backend, app, None),
        &mut mouse,
        &mut look,
        &mut (),
    )
    .await;
    let Flow::Continue(mut watching) = flow else {
        panic!("a settings change does not end the loop");
    };
    let toast = watching
        .app
        .toast()
        .map(|toast| toast.text.clone())
        .expect("a failed save is said");
    assert!(toast.starts_with("settings not saved: "), "{toast:?}");
    assert!(
        watching.app.overlays.settings.is_some(),
        "the box stays open"
    );

    let shown = turn(&mut watching.app, &mut look, SIZE, Duration::ZERO);
    let (area, _) = crate::tui::boxes::toast::area(Rect::new(0, 0, SIZE.0, SIZE.1), &toast);
    let rows = crate::tui::test_support::text(&shown);
    assert!(
        rows[usize::from(area.y) + 1].contains("settings not saved: "),
        "{rows:#?}"
    );
    // The toast's cells are the ones it draws on a screen with no box under it.
    let mut plain = gungnir();
    plain.warn(toast.clone());
    let plain = bare(&plain, &look, SIZE);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            assert_eq!(shown.cell((x, y)), plain.cell((x, y)), "({x},{y})");
        }
    }

    // A key goes to the box, not to the toast.
    watching
        .app
        .key(crossterm::event::KeyEvent::from(KeyCode::Down));
    assert_eq!(
        watching.app.overlays.settings.map(|at| at.row),
        Some(1),
        "the toast took the key"
    );
    watching.close().await;
}

/// A mouse toggle the terminal refuses changes nothing the screen shows, and a Failure toast says
/// capture did not change.
#[tokio::test(start_paused = true)]
async fn a_refused_mouse_toggle_leaves_the_capture_as_it_was_and_says_so() {
    let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
    app.mouse = true;
    let mut look = look();
    look.tui.mouse = true;

    look.change(&TuiChange::Mouse(false), &mut app, &mut RefusingMouse);

    assert!(app.mouse, "the capture is still on");
    assert!(look.tui.mouse, "the session's table says so too");
    let toast = app.toast().expect("a toast says capture did not change");
    assert_eq!(toast.kind, crate::tui::boxes::toast::ToastKind::Failure);
    assert!(toast.text.contains("did not change"), "{}", toast.text);
    drop(backend);
}

/// The AutoEq refresh is applied to the app and written to `[autoeq]` of the file by the writer
/// that keeps the file's other bytes.
#[tokio::test(start_paused = true)]
async fn the_autoeq_refresh_is_applied_and_written_to_the_file() {
    use svolder_ipc::config::RefreshWeeks;
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut mouse = SpyMouse::default();
    let mut look = look();
    let path = look.path.clone();
    std::fs::create_dir_all(path.parent().expect("a directory")).unwrap();
    std::fs::write(&path, "schema = 1\n# my own note\n[tui]\nintro = false\n").unwrap();

    let flow = take_intent(
        Intent::AutoEqRefresh(RefreshWeeks::Weeks(26)),
        &opener_over(never_opens()),
        watching(backend, app, None),
        &mut mouse,
        &mut look,
        &mut (),
    )
    .await;
    let Flow::Continue(watching) = flow else {
        panic!("a setting does not end the loop");
    };
    assert_eq!(watching.app.presets.autoeq_weeks, RefreshWeeks::Weeks(26));
    assert_eq!(watching.app.toast().map(|toast| toast.text.clone()), None);
    let written = std::fs::read_to_string(&path).unwrap();
    assert!(
        written.contains("# my own note\n[tui]\nintro = false\n"),
        "{written}"
    );
    let saved = Config::parse(&written).expect("what was written loads");
    assert_eq!(saved.autoeq.refresh_weeks, RefreshWeeks::Weeks(26));
    watching.close().await;
}

/// `never` is written as the word, and a file that cannot be written is said while the session
/// keeps the value.
#[tokio::test(start_paused = true)]
async fn the_autoeq_refresh_never_is_a_word_and_a_failed_save_is_said() {
    use svolder_ipc::config::RefreshWeeks;
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut mouse = SpyMouse::default();
    let mut look = look();
    let path = look.path.clone();
    let flow = take_intent(
        Intent::AutoEqRefresh(RefreshWeeks::Never),
        &opener_over(never_opens()),
        watching(backend, app, None),
        &mut mouse,
        &mut look,
        &mut (),
    )
    .await;
    let Flow::Continue(first) = flow else {
        panic!("a setting does not end the loop");
    };
    let saved = Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved.autoeq.refresh_weeks, RefreshWeeks::Never);
    first.close().await;

    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let blocked = look.path.clone();
    look.path = blocked.join("nested/config.toml");
    let flow = take_intent(
        Intent::AutoEqRefresh(RefreshWeeks::Weeks(3)),
        &opener_over(never_opens()),
        watching(backend, app, None),
        &mut mouse,
        &mut look,
        &mut (),
    )
    .await;
    let Flow::Continue(watching) = flow else {
        panic!("a setting does not end the loop");
    };
    assert_eq!(watching.app.presets.autoeq_weeks, RefreshWeeks::Weeks(3));
    assert!(
        watching
            .app
            .toast()
            .is_some_and(|toast| toast.text.starts_with("settings not saved: "))
    );
    watching.close().await;
}
