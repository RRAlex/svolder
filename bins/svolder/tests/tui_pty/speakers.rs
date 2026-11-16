// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's virtual speakers in the real TUI, over the demo daemon's simulated host, which
//! serves two synthetic sets (`fake-no-room`, `fake-with-room`) and reaches no network: the Space
//! choice on the Tone pane and the tone screen, and the set list box.

use crate::harness::*;
use crate::tap::*;

/// The sizes of the cases: the smallest terminal and a wide, tall one.
const SIZES: [(u16, u16); 2] = [(80, 24), (120, 36)];

/// A demo on the computer's side of the device screen, the Space row drawn.
fn on_computer(cols: u16, rows: u16) -> Tui {
    let tui = Tui::start(cols, rows, Responder::None);
    tui.wait_ready();
    tui.send_keys("C");
    tui.wait_for_text("Space");
    tui
}

fn has(tui: &Tui, needle: &str) -> bool {
    tui.screen_text().contains(needle)
}

/// Press `e` and wait for the tone screen to be drawn whole.
fn open_tone_screen(tui: &Tui) {
    tui.send_keys("e");
    tui.wait_for("the tone screen, drawn whole", WAIT, |s| {
        let text = s.contents();
        text.contains("(X)") && text.contains("Points · computer") && !text.contains("tui --join")
    });
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_tone_panes_space_row_replaces_the_crossfeed_row() {
    for (cols, rows) in SIZES {
        let tui = on_computer(cols, rows);
        let text = tui.screen_text();
        assert!(
            text.contains("\u{2039} Off \u{203a}"),
            "{cols}x{rows}:\n{text}"
        );
        assert!(
            !text.contains("Crossfeed \u{2039}"),
            "{cols}x{rows}:\n{text}"
        );
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_space_choice_steps_through_the_daemon_to_the_virtual_speakers() {
    for (cols, rows) in SIZES {
        let tui = on_computer(cols, rows);
        open_tone_screen(&tui);
        // X goes into the section, the arrow steps the choice: Crossfeed, then the speakers,
        // which the daemon chooses a set for (the first fetched one)
        tui.send_keys("X");
        tui.wait_for_text("Off \u{b7} Crossfeed \u{b7} Virtual speakers");
        tui.send_keys("\x1b[C");
        tui.wait_for("crossfeed on in the view", WAIT, |s| {
            s.contents().contains("\u{2039} Crossfeed \u{203a}")
        });
        tui.send_keys("\x1b[C");
        tui.wait_for("the speakers in the view", WAIT, |s| {
            s.contents().contains("\u{2039} Virtual speakers \u{203a}")
        });
        // the screen is read in chunks: wait for it drawn whole, the crossfeed rows gone
        tui.wait_for("the speakers section, drawn whole", WAIT, |s| {
            let t = s.contents();
            t.contains("quieter than Off")
                && t.contains("S sets")
                && t.contains("Licence")
                && !t.contains("Crossfeed on")
        });
        let text = tui.screen_text();
        assert!(
            text.contains("No room \u{b7} HRTF"),
            "{cols}x{rows}:\n{text}"
        );
        assert!(text.contains("quieter than Off"), "{cols}x{rows}:\n{text}");
        assert!(text.contains("S sets"), "{cols}x{rows}:\n{text}");
        // the arrow back is the crossfeed again, then off
        tui.send_keys("\x1b[D");
        tui.wait_for("the crossfeed", WAIT, |s| {
            s.contents().contains("\u{2039} Crossfeed \u{203a}")
        });
        tui.send_keys("\x1b[D");
        tui.wait_for("off", WAIT, |s| {
            s.contents().contains("\u{2039} Off \u{203a}")
        });
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_set_list_chooses_a_fetched_set_refuses_to_forget_the_one_that_plays_and_says_why_a_fetch_cannot_start()
 {
    for (cols, rows) in SIZES {
        let tui = on_computer(cols, rows);
        tui.send_keys("S");
        tui.wait_for_text("Virtual speakers sets");
        // the synthetic sets are the person's own, listed after the catalogue
        tui.wait_for_text("Synthetic, no room");
        let text = tui.screen_text();
        assert!(
            text.contains("With room \u{b7} BRIR"),
            "{cols}x{rows}:\n{text}"
        );
        assert!(
            text.contains("No room \u{b7} HRTF"),
            "{cols}x{rows}:\n{text}"
        );
        // Tab filters: only the sets with a room
        tui.send_keys("\t");
        tui.wait_for("the filter", WAIT, |s| {
            let t = s.contents();
            t.contains("Synthetic, with room") && !t.contains("Synthetic, no room")
        });
        // Enter chooses the focused fetched set: the box closes, the speakers play it
        tui.send_keys("\x1b[B");
        tui.send_keys("\r");
        tui.wait_for("the box closed", WAIT, |s| {
            !s.contents().contains("Virtual speakers sets")
        });
        tui.wait_for("the set in the pane", WAIT, |s| {
            s.contents()
                .contains("With room \u{b7} BRIR \u{b7} Synthetic")
        });
        // `f` on the set that plays is refused in words and asks nothing
        tui.send_keys("S");
        tui.wait_for_text("Virtual speakers sets");
        tui.send_keys("f");
        tui.wait_for_text("plays now");
        assert!(
            !has(&tui, "Forget "),
            "{cols}x{rows}:\n{}",
            tui.screen_text()
        );
        // the demo daemon is set up to fetch nothing: Enter on a catalogue set says so in the
        // catalogue's own words and the box stays
        tui.send_keys("\x1b[A\x1b[A\x1b[A\x1b[A\x1b[A\x1b[A\x1b[A\x1b[A\x1b[A\x1b[A\x1b[A\x1b[A");
        tui.send_keys("\r");
        tui.wait_for_text("discard");
        assert!(
            has(&tui, "Virtual speakers sets"),
            "the box stays:\n{}",
            tui.screen_text()
        );
        tui.send_keys("\x1b");
        tui.wait_for("closed", WAIT, |s| {
            !s.contents().contains("Virtual speakers sets")
        });
        tui.quit();
    }
}
