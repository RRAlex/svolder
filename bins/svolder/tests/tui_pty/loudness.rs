// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's loudness in the real TUI, over the demo daemon's simulated host: the Tone
//! pane's row over the crossfeed row, the tone screen's Loudness panel, and a
//! change from the keys and from the mouse that goes through the daemon and comes back in the
//! view.

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

fn row_of(tui: &Tui, needle: &str) -> u16 {
    tui.find(needle).1
}

fn has(tui: &Tui, needle: &str) -> bool {
    tui.screen_text().contains(needle)
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

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_tone_panes_loudness_row_sits_under_the_points_and_over_the_crossfeed_row() {
    for (cols, rows) in SIZES {
        let tui = on_computer(cols, rows);
        let screen = tui.screen_text();
        let table = row_of(&tui, "Type");
        let loudness = row_of(&tui, "Loudness");
        assert!(
            loudness > table,
            "{cols}x{rows}: under the points:\n{screen}"
        );
        assert!(
            loudness < row_of(&tui, "Space "),
            "{cols}x{rows}: over the Space row:\n{screen}"
        );
        assert!(screen.contains("‹ off ›"), "{screen}");
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn tab_opens_the_tone_screens_loudness_panel_in_the_chains_order() {
    for (cols, rows) in SIZES {
        let tui = on_computer(cols, rows);
        open_tone_screen(&tui);
        assert!(!has(&tui, "Reference"), "{cols}x{rows}: the EQ panel");
        // the keys open on the points: Tab goes to the Loudness panel
        tui.send_keys("\t");
        tui.wait_for_text("Reference");
        let screen = tui.screen_text();
        assert!(
            row_of(&tui, "Response") < row_of(&tui, "Bypass")
                && row_of(&tui, "Bypass") < row_of(&tui, "Loudness   ‹")
                && row_of(&tui, "Loudness   ‹") < row_of(&tui, "Strength")
                && row_of(&tui, "Strength") < row_of(&tui, "Reference"),
            "{cols}x{rows}: the chain's order:\n{screen}"
        );
        // the strip names the stages in the chain's order, along one row
        let stops: Vec<(u16, u16)> = ["EQ ○", "Loudness ○", "Space ○", "Bypass ○"]
            .iter()
            .map(|stop| tui.find(stop))
            .collect();
        assert!(
            stops
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0 && pair[0].1 == pair[1].1),
            "{cols}x{rows}: the strip's order: {stops:?}\n{screen}"
        );
        // Esc goes to the strip first and the panel stays (a further Esc would leave the screen)
        tui.send_keys("\x1b");
        tui.wait_for("the strip", WAIT, |s| s.contents().contains("┌ Loudness"));
        assert!(has(&tui, "Reference"), "the panel stays");
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_loudness_change_from_the_keys_goes_through_the_daemon_and_comes_back_in_the_view() {
    for (cols, rows) in SIZES {
        let tui = on_computer(cols, rows);
        open_tone_screen(&tui);
        tui.send_keys("L");
        tui.wait_for_text("Reference");
        assert!(has(&tui, "‹ off ›"), "{}", tui.screen_text());
        // Space turns it on; the view says so
        tui.send_keys(" ");
        tui.wait_for("on in the view", WAIT, |s| s.contents().contains("‹ on ›"));
        // one step of the strength (5 %) and back: the strength row is under the on/off row
        tui.send_keys("\x1b[B\x1b[C");
        tui.wait_for("one step up", WAIT, |s| s.contents().contains("55 %"));
        tui.send_keys("\x1b[D");
        tui.wait_for("one step back", WAIT, |s| s.contents().contains("50 %"));
        // the Reference row steps the reference by one
        tui.send_keys("\x1b[B");
        tui.wait_for("the cursor on Reference", WAIT, |s| {
            s.contents().contains("▶ Reference")
        });
        tui.send_keys("\x1b[D");
        tui.wait_for("the reference one down", WAIT, |s| {
            s.contents().contains("‹ 99 % ›")
        });
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_click_on_the_switch_and_on_the_bar_changes_loudness_through_the_daemon() {
    let tui = on_computer(120, 36);
    open_tone_screen(&tui);
    tui.send_keys("L");
    tui.wait_for_text("Reference");
    // a click on the `›` of the on/off row turns it on
    let (col, row) = tui.find("‹ off ›");
    tui.send_mouse(Mouse::Click, col + 6, row);
    tui.wait_for("on in the view", WAIT, |s| s.contents().contains("‹ on ›"));
    // the bar's first cell stands for the lowest step above 0: 5 %
    let (bar, bar_row) = tui.find("█████░");
    tui.send_mouse(Mouse::Click, bar, bar_row);
    tui.wait_for("the strength from the cell", WAIT, |s| {
        s.contents().contains(" 5 %")
    });
    // a drag past the bar's left end gives the range's end, saved on the release
    let (bar, bar_row) = tui.find("░");
    tui.send_mouse(
        Mouse::DragTo(bar.saturating_sub(8), bar_row),
        bar + 4,
        bar_row,
    );
    tui.wait_for("0 % from the drag", WAIT, |s| s.contents().contains(" 0 %"));
    tui.quit();
}
