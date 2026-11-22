// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The track's time on the Transport row: `2:41 / 9:22` after the buttons and a thin bar in what
//! is left of the row, counted on from the player's own reading while it plays.

use svolder_host::Progress;

use super::host_group::{daemon_hosting, gungnir_route};
use super::*;

/// A daemon hosting a player that is `status`, `position_us` into a track `length_us` long, read
/// at the daemon's clock `10_000` ms.
fn playing_at(status: &str, position_us: Option<u64>, length_us: Option<u64>) -> App {
    let mut app = daemon_hosting(gungnir_route(|_| {}));
    app.host.fold_daemon(
        svolder_ipc::host::HostView {
            volume: None,
            eq: None,
            available: true,
            playing: Some(svolder_ipc::host::PlayingView {
                title: "So What".into(),
                artist: "Miles Davis".into(),
                player: "mpv".into(),
                status: status.into(),
                length_us,
                progress: position_us.map(|position_us| Progress {
                    position_us,
                    rate_milli: 1000,
                    read_at_ms: 10_000,
                }),
                ..Default::default()
            }),
            route: Some(gungnir_route(|_| {})),
            now_ms: 10_000,
        },
        app.now,
    );
    app
}

/// The text of the Transport row of a frame, from its label to the pane's right border.
fn transport(app: &App, width: u16, height: u16) -> String {
    let rows = text(&frame(app, width, height));
    let row = rows
        .iter()
        .find(|row| row.contains(" Transport "))
        .unwrap_or_else(|| panic!("no Transport row: {rows:#?}"));
    let pane = row.split('│').nth(1).unwrap_or("");
    pane.trim_end()
        .split_once("Transport")
        .unwrap()
        .1
        .to_string()
}

/// A track 2:41 into 9:22 while it plays: the time after the buttons and the bar in what is left,
/// at 120 and 160 columns; the row is as wide as its pane.
#[test]
fn the_time_and_a_bar_follow_the_buttons() {
    let app = playing_at("playing", Some(161_000_000), Some(562_000_000));
    for (width, height) in [(120, 40), (160, 50)] {
        let row = transport(&app, width, height);
        assert!(
            row.contains("◀◀  ▶  ▮▮  ▶▶  2:41 / 9:22  "),
            "{width}: {row:?}"
        );
        let bar: String = row.split("9:22  ").nth(1).unwrap().trim_end().to_string();
        assert!(
            bar.starts_with('━') && bar.ends_with('─'),
            "{width}: {bar:?}"
        );
        let (filled, track) = (bar.matches('━').count(), bar.matches('─').count());
        // 161 of 562 s: 29 % of the bar, to the cell
        let share = filled as f64 / (filled + track) as f64;
        assert!(
            (share - 0.2865).abs() < 0.5 / (filled + track) as f64 + 1e-9,
            "{share}"
        );
    }
}

/// Playing, the time counts on from the reading at the rate: one second a second; the same
/// reading seen again does not start the count over.
#[test]
fn the_time_advances_while_it_plays() {
    let mut app = playing_at("playing", Some(161_000_000), Some(562_000_000));
    assert!(transport(&app, 160, 50).contains("2:41 / 9:22"));
    app.tick(app.now + Duration::from_millis(3_000));
    assert!(transport(&app, 160, 50).contains("2:44 / 9:22"));
    // The same reading folded again (a redraw's worth of news that says nothing new).
    let before = app.now;
    app.host.fold_daemon(
        svolder_ipc::host::HostView {
            volume: None,
            eq: None,
            available: true,
            playing: app
                .host
                .playing
                .clone()
                .map(|p| svolder_ipc::host::PlayingView {
                    title: p.title,
                    artist: p.artist,
                    player: p.identity,
                    status: "playing".into(),
                    length_us: p.length_us,
                    progress: p.progress,
                    ..Default::default()
                }),
            route: Some(gungnir_route(|_| {})),
            now_ms: 10_000,
        },
        before,
    );
    assert!(transport(&app, 160, 50).contains("2:44 / 9:22"));
}

/// Paused or stopped, the time holds where the player was.
#[test]
fn a_paused_player_holds_its_time() {
    for status in ["paused", "stopped"] {
        let mut app = playing_at(status, Some(161_000_000), Some(562_000_000));
        app.tick(app.now + Duration::from_secs(30));
        assert!(transport(&app, 160, 50).contains("2:41 / 9:22"), "{status}");
    }
}

/// A player that gave no length has the time elapsed alone, no bar; one that gave no position has
/// nothing after its buttons.
#[test]
fn no_length_has_the_elapsed_time_and_no_position_nothing() {
    let app = playing_at("playing", Some(161_000_000), None);
    let row = transport(&app, 160, 50);
    assert_eq!(row.trim_end(), "    ◀◀  ▶  ▮▮  ▶▶  2:41");
    let app = playing_at("playing", None, Some(562_000_000));
    let row = transport(&app, 160, 50);
    assert_eq!(row.trim_end(), "    ◀◀  ▶  ▮▮  ▶▶");
}

/// A track an hour long or more reads `h:mm:ss`, and the time never runs past the length.
#[test]
fn an_hour_reads_h_mm_ss_and_the_end_is_the_length() {
    let app = playing_at("paused", Some(3_725_000_000), Some(7_322_000_000));
    assert!(transport(&app, 160, 50).contains("1:02:05 / 2:02:02"));
    let mut app = playing_at("playing", Some(561_000_000), Some(562_000_000));
    app.tick(app.now + Duration::from_secs(60));
    assert!(transport(&app, 160, 50).contains("9:22 / 9:22"));
}

/// At 80 columns the row has no room for the bar nor the total: the elapsed time stays; a row
/// with room for the time and not the bar carries `2:41 / 9:22` alone.
#[test]
fn a_narrow_row_drops_the_bar_then_the_total() {
    let app = playing_at("playing", Some(161_000_000), Some(562_000_000));
    assert!(
        transport(&app, 80, 24).trim_end().ends_with("▶▶  2:41"),
        "{:?}",
        transport(&app, 80, 24)
    );
    let row = transport(&app, 100, 28);
    assert!(
        row.contains("2:41 / 9:22") && !row.contains('━') && !row.contains('─'),
        "{row:?}"
    );
}

/// The clock holds a player's time as the web does: a direct session counts from this machine's
/// clock.
#[test]
fn the_unix_clock_is_the_hosts_own_on_a_direct_session() {
    let mut app = daemon_hosting(gungnir_route(|_| {}));
    let now = app.now;
    let mut playing = app.host.playing.clone().expect("a player");
    playing.length_us = Some(562_000_000);
    playing.progress = Some(Progress {
        position_us: 161_000_000,
        rate_milli: 1000,
        read_at_ms: svolder_host::unix_ms(),
    });
    playing.status = svolder_host::Status::Playing;
    app.host.found_bus(Some(playing), now);
    let time = app
        .host
        .track_time(now + Duration::from_secs(5))
        .expect("a time");
    // the read is at the moment it was folded, so five seconds on is five seconds in
    assert!(
        (time.elapsed_us as i64 - 166_000_000).abs() < 1_500_000,
        "{time:?}"
    );
}
