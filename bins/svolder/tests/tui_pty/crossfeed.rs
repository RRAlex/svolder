// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's crossfeed in the real TUI, over the demo daemon's simulated host: the Tone
//! pane's row under the EQ, the tone screen's folded section, and a change that goes through
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

/// Press `e` and wait for the tone screen to be drawn whole: the screen is read in chunks, so
/// `(X)` can show while the device screen's rows and its hint box still stand under it.
fn open_tone_screen(tui: &Tui) {
    tui.send_keys("e");
    tui.wait_for("the tone screen, drawn whole", WAIT, |s| {
        let text = s.contents();
        text.contains("(X)")
            && text.contains("Points · computer")
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
fn tab_and_x_unfold_and_fold_the_tone_screens_section_and_the_graph_keeps_its_height() {
    for (cols, rows) in SIZES {
        let tui = on_computer(cols, rows);
        open_tone_screen(&tui);
        let graph = row_of(&tui, "(X)") - row_of(&tui, "Response");
        assert!(
            !has(&tui, "Level"),
            "{cols}x{rows}: folded:\n{}",
            tui.screen_text()
        );
        // The keys open on the points: Tab goes to the header, the next into the section,
        // which unfolds
        tui.send_keys("\t\t");
        tui.wait_for_text("Level");
        let open = row_of(&tui, "Space ") - row_of(&tui, "Response");
        // the Space row leads the section: the Crossfeed row is under it whole, and in the narrow
        // tier the three value rows too, from the graph's rows
        match cols >= 120 {
            true => assert_eq!(
                open + 1,
                graph,
                "{cols}x{rows}: the Space row's second line"
            ),
            false => assert_eq!(
                open + 4,
                graph,
                "{cols}x{rows}: the rows come from the graph"
            ),
        }
        // Tab on leaves it and folds it
        tui.send_keys("\t");
        tui.wait_for("folded", WAIT, |s| !s.contents().contains("Level"));
        assert_eq!(row_of(&tui, "(X)") - row_of(&tui, "Response"), graph);
        // X jumps in, X again folds
        tui.send_keys("X");
        tui.wait_for_text("Level");
        tui.send_keys("X");
        tui.wait_for("folded again", WAIT, |s| !s.contents().contains("Level"));
        // Esc folds before it leaves the screen
        tui.send_keys("X");
        tui.wait_for_text("Level");
        tui.send_keys("\x1b");
        tui.wait_for("Esc folds", WAIT, |s| !s.contents().contains("Level"));
        assert!(has(&tui, "(X)"), "still on the tone screen");
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_crossfeed_change_goes_through_the_daemon_and_comes_back_in_the_view() {
    for (cols, rows) in SIZES {
        let tui = on_computer(cols, rows);
        open_tone_screen(&tui);
        let folded = tui.screen_text();
        assert!(folded.contains("Space      off"), "{folded}");
        // Space in the section turns it on; the folded line then reads the numbers
        tui.send_keys("X");
        tui.wait_for_text("Level");
        tui.send_keys(" ");
        tui.wait_for("on in the view", WAIT, |s| {
            s.contents().contains("Crossfeed on ")
        });
        // a step of Level goes through the daemon and comes back as exactly one step (1 %); the
        // Crossfeed row is the stop under the Space row
        tui.send_keys("\x1b[B\x1b[B");
        tui.wait_for("the cursor on Level", WAIT, |s| {
            s.contents().contains("▶ Level")
        });
        tui.wait_for_text("‹ 37 %");
        tui.send_keys("\x1b[C");
        tui.wait_for("one step up", WAIT, |s| s.contents().contains("‹ 38 %"));
        tui.send_keys("\x1b[D");
        tui.wait_for("one step back", WAIT, |s| s.contents().contains("‹ 37 %"));
        tui.send_keys("X");
        tui.wait_for("folded", WAIT, |s| !s.contents().contains("Level"));
        assert!(has(&tui, "Space      Crossfeed"), "{}", tui.screen_text());
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
    open_tone_screen(&tui);
    // a write the old bug sent would be answered by now: unfold and fold twice, each a round trip
    for _ in 0..2 {
        tui.send_keys("X");
        tui.wait_for_text("Level");
        tui.send_keys("X");
        tui.wait_for("folded", WAIT, |s| !s.contents().contains("Level"));
    }
    assert!(has(&tui, "Space      off"), "{}", tui.screen_text());
    assert!(!has(&tui, "Level"), "folded on the way back");
    tui.quit();
}
