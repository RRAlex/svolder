// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's crossfeed in the real TUI, over the demo daemon's simulated host: the Tone
//! pane's row under the EQ, the tone screen's Space panel, and a change that goes through
//! the daemon and comes back in the view.

use crate::harness::*;
use crate::tap::*;

/// The sizes of the cases: the smallest terminal and a wide, tall one.
const SIZES: [(u16, u16); 2] = [(80, 24), (120, 36)];

/// A demo on the computer's side of the device screen.
fn on_computer(cols: u16, rows: u16) -> Tui {
    let tui = Tui::start(cols, rows, Responder::None);
    tui.wait_ready();
    tui.send_keys("C");
    tui.wait_for_text("Space");
    tui
}

/// The screen row of the first line holding `needle`, once it is there.
fn row_of(tui: &Tui, needle: &str) -> u16 {
    tui.find(needle).1
}

/// Press `e` and wait for the tone screen to be drawn whole: the screen is read in chunks, so the
/// EQ panel's top border can show while the device screen's rows and its hint box still stand
/// under it. The keys open in the EQ panel.
fn open_tone_screen(tui: &Tui) {
    tui.send_keys("e");
    tui.wait_for("the tone screen, drawn whole", WAIT, |s| {
        let text = s.contents();
        text.contains("┏ EQ")
            && text.contains("Esc strip")
            && !text.contains("Level from")
            && !text.contains("tui --join")
    });
}

fn has(tui: &Tui, needle: &str) -> bool {
    tui.screen_text().contains(needle)
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_tone_panes_crossfeed_row_sits_under_the_eq() {
    for (cols, rows) in SIZES {
        let tui = on_computer(cols, rows);
        let table = row_of(&tui, "Type");
        let row = row_of(&tui, "Space ");
        assert!(
            row > table,
            "{cols}x{rows}: under the points table:\n{}",
            tui.screen_text()
        );
        assert!(
            row_of(&tui, "Mode") < table,
            "the EQ keeps its place at the top"
        );
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn tab_and_x_open_and_leave_the_tone_screens_space_panel_and_the_graph_keeps_its_height() {
    for (cols, rows) in SIZES {
        let tui = on_computer(cols, rows);
        open_tone_screen(&tui);
        // the strip is the row over the panel's top border: the graph is what is above it
        let graph = row_of(&tui, "┏") - row_of(&tui, "Response");
        assert!(
            !has(&tui, "Level"),
            "{cols}x{rows}: the EQ panel:\n{}",
            tui.screen_text()
        );
        // X puts the keys in the Space panel: its rows replace the Points, the graph stays
        tui.send_keys("X");
        tui.wait_for_text("Level");
        assert_eq!(
            row_of(&tui, "┏") - row_of(&tui, "Response"),
            graph,
            "{cols}x{rows}"
        );
        // Tab goes on to the next stage's panel
        tui.send_keys("\t");
        tui.wait_for("the Bypass panel", WAIT, |s| {
            s.contents().contains("┏ Bypass")
        });
        assert!(!has(&tui, "Level"));
        assert_eq!(
            row_of(&tui, "┏") - row_of(&tui, "Response"),
            graph,
            "{cols}x{rows}"
        );
        // X jumps back in; X again goes to the strip, the panel staying (thin border)
        tui.send_keys("X");
        tui.wait_for_text("Level");
        tui.send_keys("X");
        tui.wait_for("the keys on the strip", WAIT, |s| {
            s.contents().contains("┌ Space")
        });
        assert_eq!(
            row_of(&tui, "┌ Space") - row_of(&tui, "Response"),
            graph,
            "{cols}x{rows}"
        );
        // Esc on the strip leaves the screen
        tui.send_keys("\x1b");
        tui.wait_for("the device screen", WAIT, |s| {
            let text = s.contents();
            text.contains("┤ Controls ├") && !text.contains("┌ Space") && !text.contains("┏ Space")
        });
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_crossfeed_change_goes_through_the_daemon_and_comes_back_in_the_view() {
    for (cols, rows) in SIZES {
        let tui = on_computer(cols, rows);
        open_tone_screen(&tui);
        let start = tui.screen_text();
        assert!(start.contains("Space ○"), "{start}");
        // Right highlights Crossfeed on the Plays row and Enter plays it; the strip then says so
        tui.send_keys("X");
        tui.wait_for_text("Level");
        tui.send_keys("\x1b[C\r");
        tui.wait_for("on in the view", WAIT, |s| {
            s.contents().contains("Space ● crossfeed")
        });
        // a step of Level goes through the daemon and comes back as exactly one step (1 %); the
        // Level row is the third under Plays
        tui.send_keys("\x1b[B\x1b[B\x1b[B");
        tui.wait_for("the cursor on Level", WAIT, |s| {
            s.contents().contains("▶ Level")
        });
        tui.wait_for_text("‹ 37 %");
        tui.send_keys("\x1b[C");
        tui.wait_for("one step up", WAIT, |s| s.contents().contains("‹ 38 %"));
        tui.send_keys("\x1b[D");
        tui.wait_for("one step back", WAIT, |s| s.contents().contains("‹ 37 %"));
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_crossfeed_focus_stays_on_the_tone_screen() {
    let tui = on_computer(80, 24);
    open_tone_screen(&tui);
    tui.send_keys("X");
    tui.wait_for_text("Level");
    // on Telemetry the arrows and Space keep the screen's meaning: crossfeed stays off
    tui.send_keys("t");
    tui.wait_for_text("┤ Telemetry ├");
    tui.send_keys(" ");
    // the tone screen comes back with the Space panel it was left on, the keys on the header
    tui.send_keys("e");
    tui.wait_for("the Space panel, drawn whole", WAIT, |s| {
        let text = s.contents();
        text.contains("Space · off") && text.contains("Esc strip") && !text.contains("tui --join")
    });
    // a write the old bug sent would be answered by now: go into the panel and out to the strip
    // twice; each wait is for a change the key itself makes (the heavy border, then the thin one)
    for _ in 0..2 {
        tui.send_keys("X");
        tui.wait_for("the keys in the panel", WAIT, |s| {
            s.contents().contains("┏ Space")
        });
        tui.send_keys("X");
        tui.wait_for("the strip", WAIT, |s| s.contents().contains("┌ Space"));
    }
    assert!(has(&tui, "Space ○"), "{}", tui.screen_text());
    tui.quit();
}
