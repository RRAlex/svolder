// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Host group's row, the transport buttons, their clicks and the player's absence, and the
//! Volume row on the computer's side.

use super::*;

/// An app linked to a host whose sink reads `volume` (1.0 = 100 %), unmuted.
fn hosted(volume: f32) -> App {
    let mut app = app();
    app.host.linked = true;
    app.host.fold_read(
        Some(svolder_host::SinkState {
            volume,
            muted: false,
            name: "USB DAC".into(),
            target: None,
        }),
        true,
    );
    app
}

/// What `code` asks for.
fn host_press(app: &mut App, code: KeyCode) -> Vec<Intent> {
    app.key(KeyEvent::from(code))
}

/// Host audio switched off while the Controls cursor is on a Host row takes the row away,
/// and the cursor comes back to the device's last row rather than resting on nothing.
#[test]
fn host_audio_off_brings_a_cursor_on_a_host_row_back_to_the_last_row() {
    let mut app = hosted(0.54);
    let own = app.rows().len();
    app.selected = own;
    assert_eq!(app.host_row(), Some(HostRow::Transport));
    let mut tui = app.tui.clone();
    tui.host_audio = false;
    app.set_look(&tui, &Effects::new(false, &tui.effects));
    assert_eq!(app.selected, own - 1, "the cursor was left past the rows");
    assert!(
        app.selected_control().is_some(),
        "no row carries the cursor"
    );
}

/// `↓` past the device's own rows reaches the Host group's and goes no further; with host
/// audio off the device's own last row is as far as it goes.
#[test]
fn the_host_group_keeps_transport_only() {
    assert_eq!(hosted(0.54).host_rows(), [HostRow::Transport]);
    let mut app = hosted(0.54);
    let own = app.rows().len();
    for _ in 0..own + 3 {
        host_press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.selected, own);
    assert_eq!(app.host_row(), Some(HostRow::Transport));
    assert_eq!(app.selected_control(), None, "a device row is selected too");

    let mut off = hosted(0.54);
    off.tui.host_audio = false;
    for _ in 0..own + 3 {
        host_press(&mut off, KeyCode::Down);
    }
    assert_eq!(
        off.selected,
        own - 1,
        "the keys went past the device's rows"
    );
    assert_eq!(off.host_row(), None);
}

/// [`hosted`] with the computer's controls there and its side shown, the cursor on Volume.
fn on_the_computer(volume: f32) -> App {
    let mut app = hosted(volume);
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq(
        false,
        &[("PK", 1000.0, 0.0, 1.0)],
    ));
    app.flip_side();
    assert!(app.on_computer());
    app.selected = row_of(&app, "volume");
    app
}

/// On the computer's side `←→`, `+` and `-` and the wheel on the Volume row step the computer's
/// volume one percent, never above 100 and nothing before it has been read; on the device's side
/// they step the device's.
#[test]
fn the_computer_volume_steps_one_percent_and_never_above_100() {
    let mut app = on_the_computer(0.54);
    let volume = app.selected;
    assert_eq!(
        host_press(&mut app, KeyCode::Right),
        [Intent::HostVolume(55)]
    );
    assert_eq!(
        host_press(&mut app, KeyCode::Left),
        [Intent::HostVolume(53)]
    );
    assert_eq!(
        host_press(&mut app, KeyCode::Char('+')),
        [Intent::HostVolume(55)]
    );
    assert_eq!(
        host_press(&mut app, KeyCode::Char('-')),
        [Intent::HostVolume(53)]
    );
    assert_eq!(app.nudge_row(volume, 1), [Intent::HostVolume(55)]);
    assert_eq!(app.nudge_row(volume, -1), [Intent::HostVolume(53)]);

    let mut full = on_the_computer(1.0);
    assert_eq!(host_press(&mut full, KeyCode::Right), [], "never above 100");
    assert_eq!(
        host_press(&mut full, KeyCode::Left),
        [Intent::HostVolume(99)]
    );
    let mut boosted = on_the_computer(1.2);
    assert_eq!(host_press(&mut boosted, KeyCode::Right), []);
    assert_eq!(
        host_press(&mut boosted, KeyCode::Left),
        [Intent::HostVolume(100)]
    );

    app.host.sink = None;
    assert_eq!(host_press(&mut app, KeyCode::Right), [], "not read yet");
    assert_eq!(app.nudge_row(volume, 1), []);

    let mut dac = hosted(0.54);
    dac.host.computer.eq = app.host.computer.eq.clone();
    dac.selected = row_of(&dac, "volume");
    assert_eq!(host_press(&mut dac, KeyCode::Right), [set("volume", "up")]);
}

/// `Space` and `m` on the computer's side turn the computer's mute the other way, once its volume
/// has been read; on the device's side `m` is the device's.
#[test]
fn space_and_m_on_the_computer_volume_turn_its_mute() {
    let mut app = on_the_computer(0.54);
    assert_eq!(host_press(&mut app, KeyCode::Char(' ')), [Intent::HostMute]);
    assert_eq!(host_press(&mut app, KeyCode::Char('m')), [Intent::HostMute]);
    app.host.sink = None;
    assert_eq!(host_press(&mut app, KeyCode::Char(' ')), []);
    assert_eq!(host_press(&mut app, KeyCode::Char('m')), []);
}

/// What a player plays: So What, in `status`.
fn so_what(status: svolder_host::Status) -> svolder_host::NowPlaying {
    svolder_host::NowPlaying {
        genre: Vec::new(),
        title: "So What".into(),
        artist: "Miles Davis".into(),
        identity: "mpv".into(),
        status,
        ..Default::default()
    }
}

/// On the Transport row `←→` move its cursor between the four buttons and ask for
/// nothing, and `Enter` or `Space` press the one under it on the followed player.
#[test]
fn the_transport_rows_keys_move_between_the_buttons_and_press_one() {
    let mut app = hosted(0.54);
    app.host
        .found_bus(Some(so_what(svolder_host::Status::Playing)), app.now);
    app.selected = app.rows().len();
    assert_eq!(app.host_row(), Some(HostRow::Transport));
    for _ in 0..3 {
        assert_eq!(host_press(&mut app, KeyCode::Right), []);
    }
    assert_eq!(
        host_press(&mut app, KeyCode::Enter),
        [Intent::HostPress(Button::Next)]
    );
    assert_eq!(host_press(&mut app, KeyCode::Left), []);
    assert_eq!(
        host_press(&mut app, KeyCode::Char(' ')),
        [Intent::HostPress(Button::Pause)]
    );
    assert_eq!(app.toast().map(|toast| toast.text.clone()), None);
}

/// With no player to take it a press sends nothing, and the key line says `no player`.
#[test]
fn a_press_with_no_player_sends_nothing_and_says_so() {
    let mut app = hosted(0.54);
    app.host.found_bus(None, app.now);
    app.selected = app.rows().len();
    assert_eq!(host_press(&mut app, KeyCode::Enter), []);
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some(host::NO_PLAYER)
    );
}

/// `Enter` on the device screen is `Space`: it turns the toggle the cursor is on.
#[test]
fn enter_turns_a_toggle_as_space_does() {
    let mut app = hosted(0.54);
    let mute = app
        .rows()
        .iter()
        .position(|row| row.control.key == "mute")
        .expect("a mute row");
    app.selected = mute;
    let space = host_press(&mut app, KeyCode::Char(' '));
    assert!(!space.is_empty());
    assert_eq!(host_press(&mut app, KeyCode::Enter), space);
}

/// A click on a transport button focuses the Transport row on it and presses it; it asks
/// for nothing while host audio is not shown.
#[test]
fn a_click_on_a_transport_button_focuses_it_and_presses_it() {
    let mut app = hosted(0.54);
    app.host
        .found_bus(Some(so_what(svolder_host::Status::Paused)), app.now);
    app.pane = Pane::Status;
    assert_eq!(app.click_transport(1), [Intent::HostPress(Button::Play)]);
    assert_eq!(app.pane, Pane::Controls);
    assert_eq!(app.host_row(), Some(HostRow::Transport));
    assert_eq!(app.host.button(), Button::Play);
    app.tui.host_audio = false;
    assert_eq!(app.click_transport(3), []);
}
