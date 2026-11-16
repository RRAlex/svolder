// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The work one frame of the device screen costs while the Spectrum row moves under a cover: the
//! screen is laid out once and each pane's lines are built no more often than drawing them needs.

use super::art::{covered, with_art};
use super::*;
use crate::tui::test_support::work;

/// The mockup's app playing a track with its cover in the box a frame of `area` gives it, the
/// capture running and its bands on the Spectrum row.
fn playing_covered(area: Rect) -> App {
    let mut app = covered();
    app.host.capturing = true;
    let block = svolder_host::Block {
        bands: std::array::from_fn(|band| band as f32 / 7.0),
        left: 0.5,
        right: 0.5,
    };
    app.heard_block(&block, app.now);
    with_art(app, area)
}

/// One frame of `app`, `width` by `height`, and how often each counted piece of work ran for it.
fn work_of(app: &App, width: u16, height: u16) -> std::collections::BTreeMap<&'static str, usize> {
    work::take();
    frame(app, width, height);
    work::take()
}

/// A frame lays the screen out once, however many of its parts (the panes, their frames for the
/// animation layer, the squirrel's home, the cover) ask where they are, and builds each pane's
/// lines no more than twice: once to measure the room the cover grows into and once to draw.
#[test]
fn a_frame_of_the_device_screen_lays_it_out_once_and_builds_each_pane_at_most_twice() {
    for (width, height) in [(80, 24), (120, 36), (200, 60)] {
        let app = playing_covered(Rect::new(0, 0, width, height));
        let done = work_of(&app, width, height);
        let at = format!("{width}x{height}: {done:?}");
        assert_eq!(done.get("device_layout"), Some(&1), "{at}");
        assert!(done.get("controls_lines").is_some_and(|n| *n <= 2), "{at}");
        assert!(done.get("tone_lines").is_some_and(|n| *n <= 2), "{at}");
    }
}

/// Knowing the terminal's size costs no layout of the help overlay while it is closed: only an
/// open overlay holds its offset inside what the size lets it show.
#[test]
fn the_size_lays_out_the_help_overlay_only_while_it_is_open() {
    let mut app = playing_covered(Rect::new(0, 0, 200, 60));
    work::take();
    app.resize(200, 60);
    app.resize(120, 36);
    assert_eq!(work::take().get("help_view"), None);
    app.overlays.help_open = true;
    app.resize(200, 60);
    assert_eq!(work::take().get("help_view"), Some(&1));
}

/// A tick moves the `Now playing` marquee without laying the screen out for the cover when the
/// track's words fit beside any picture: they stand still whatever the picture takes.
#[test]
fn a_tick_lays_nothing_out_for_words_that_fit_beside_any_cover() {
    let mut app = playing_covered(Rect::new(0, 0, 200, 60));
    app.resize(200, 60);
    work::take();
    let now = app.now;
    app.tick(now);
    assert_eq!(work::take().get("device_layout"), None);
}

/// Beside any picture the `Now playing` row keeps [`words_beside_art`] cells for the track's
/// words, which is what lets a tick skip the layout for words that short.
#[test]
fn the_words_beside_a_cover_keep_their_floor_at_every_size() {
    for (width, height) in [
        (80, 24),
        (100, 45),
        (120, 36),
        (160, 50),
        (200, 60),
        (300, 90),
    ] {
        let mut app = playing_covered(Rect::new(0, 0, width, height));
        app.resize(width, height);
        let reserved = art_reserved(&app);
        assert!(
            reserved > 0,
            "{width}x{height}: no picture beside the words"
        );
        let room = now_playing_room((width, height)).saturating_sub(reserved);
        assert!(room >= words_beside_art(), "{width}x{height}: {room} cells");
    }
}
