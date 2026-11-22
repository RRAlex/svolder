// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Scenes box in the real TUI against the demo's own daemon: `z`, save, the lit row and the
//! Status line, a refusal in the daemon's words, a recall that writes no volume or mute, and a
//! delete through its question, with the config file the daemon writes read after each step.

use crate::cases::demo_config;
use crate::harness::*;
use crate::tap::*;

/// The size the case runs at, and the other for the box's look at its widest.
const SMALL: (u16, u16) = (80, 24);
const LARGE: (u16, u16) = (120, 40);

fn has(tui: &Tui, needle: &str) -> bool {
    tui.screen_text().contains(needle)
}

/// The file's text, or nothing while it has none.
fn config_of(tui: &Tui) -> String {
    std::fs::read_to_string(demo_config(tui)).unwrap_or_default()
}

/// Wait until the daemon's scenes have arrived: the Esc menu has the Scenes row, between Settings
/// and Help. Leaves the menu closed.
fn wait_for_scenes(tui: &Tui) {
    tui.send_keys("\x1b");
    tui.wait_for_text("Scenes z");
    let text = tui.screen_text();
    let at = |word: &str| text.find(word).unwrap_or_else(|| panic!("{word}:\n{text}"));
    assert!(
        at("Settings") < at("Scenes z") && at("Scenes z") < at("Help"),
        "{text}"
    );
    tui.send_keys("\x1b");
    tui.wait_for("the menu closed", WAIT, |s| {
        !s.contents().contains("Scenes z")
    });
}

/// Every control row of the Controls pane is on the screen.
fn assert_controls_in_view(tui: &Tui) {
    let screen = tui.screen_text();
    for label in [
        "Input",
        "Phase invert",
        "NOS filter",
        "Mute",
        "Target",
        "Volume",
        "Balance",
        "Transport",
        "Path",
    ] {
        assert!(
            screen.contains(label),
            "{label} is off the screen:\n{screen}"
        );
    }
}

/// Clear the name prompt's suggestion and type `name`.
fn type_name(tui: &Tui, name: &str) {
    for _ in 0..48 {
        tui.send_keys("\x7f");
    }
    tui.send_keys(name);
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn scenes_are_saved_recalled_refused_and_deleted_through_the_box() {
    let tui = Tui::start(SMALL.0, SMALL.1, Responder::None);
    tui.wait_ready();
    // The daemon lists scenes and its view arrives: the menu has the row.
    wait_for_scenes(&tui);
    // With no scene saved the Status pane has no Scene line, and every control row is in view.
    assert!(!has(&tui, "Scene  "), "{}", tui.screen_text());
    assert_controls_in_view(&tui);

    // An empty box says how to save one.
    tui.send_keys("z");
    tui.wait_for_text("no scenes yet");

    // Save: clear the suggestion, name it.
    tui.send_keys("n");
    tui.wait_for_text("Save current as:");
    type_name(&tui, "Late night");
    tui.send_keys("\r");
    tui.wait_for_text("Saved Late night.");
    tui.wait_for("the row lit", WAIT, |s| {
        s.contents().contains("● Late night")
    });
    tui.send_keys("\x1b");
    tui.wait_for("the box closed", WAIT, |s| {
        !s.contents().contains("↵ recall")
    });
    tui.wait_for_text("Scene  Late night");
    // The first scene gives the Status pane its Scene line, and no control row is pushed off.
    assert_controls_in_view(&tui);
    let config = config_of(&tui);
    assert!(
        config.contains("[[scene]]") && config.contains("Late night"),
        "{config}"
    );

    // The same name again: the daemon refuses in its own words and the box stays.
    tui.send_keys("z");
    tui.wait_for_text("↵ recall");
    tui.send_keys("n");
    tui.wait_for_text("Save current as:");
    type_name(&tui, "Late night");
    tui.send_keys("\r");
    tui.wait_for("a refusal toast", WAIT, |s| {
        let text = s.contents();
        text.contains("already")
            && !text.contains("unexpected reply")
            && !text.contains("Saved Late night.")
    });
    assert!(has(&tui, "↵ recall"), "{}", tui.screen_text());

    // Recall: change the sound and the levels first, so a recall has something to put back and
    // something to leave alone.
    tui.send_keys("\x1b");
    tui.wait_for("the box closed", WAIT, |s| {
        !s.contents().contains("↵ recall")
    });
    tui.send_keys("i");
    tui.wait_for_text("Phase invert [x] on");
    tui.wait_for_text("Scene  none");
    tui.send_keys("+");
    tui.wait_for_text("64/127");
    tui.send_keys("+");
    tui.wait_for_text("65/127");
    tui.send_keys("m");
    tui.wait_for_text("Mute         [x] on");
    // The recall puts phase back and lights the row, and leaves volume and mute where they were.
    tui.send_keys("z");
    tui.wait_for_text("↵ recall");
    tui.send_keys("\r");
    tui.wait_for_text("Late night recalled.");
    tui.wait_for("phase back off", WAIT, |s| {
        s.contents().contains("Phase invert [ ] off")
    });
    tui.wait_for_text("Scene  Late night");
    let after = tui.screen_text();
    assert!(
        after.contains("65/127"),
        "a recall moved the volume:\n{after}"
    );
    assert!(
        after.contains("Mute         [x] on"),
        "a recall moved the mute:\n{after}"
    );

    // Delete through the question.
    tui.send_keys("z");
    tui.wait_for_text("↵ recall");
    // The recalled scene's row is lit again.
    tui.wait_for_text("● Late night");
    tui.send_keys("x");
    tui.wait_for_text("Delete Late night? It is removed for every device. [y/N]");
    tui.send_keys("y");
    tui.wait_for_text("Deleted Late night.");
    // With the last scene gone the Scene line goes with it.
    tui.wait_for("no Scene line", WAIT, |s| !s.contents().contains("Scene  "));
    tui.send_keys("\x1b");
    tui.wait_for("the box closed", WAIT, |s| {
        !s.contents().contains("↵ recall")
    });
    let config = config_of(&tui);
    assert!(!config.contains("Late night"), "{config}");
    tui.quit();
}

/// The same box at 120x40: its corner and its rows.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_box_at_120_by_40_keeps_its_corner_and_its_rows() {
    let tui = Tui::start(LARGE.0, LARGE.1, Responder::None);
    tui.wait_ready();
    wait_for_scenes(&tui);
    tui.send_keys("z");
    tui.wait_for_text("no scenes yet");
    tui.send_keys("n");
    tui.wait_for_text("Save current as:");
    type_name(&tui, "Late night");
    tui.send_keys("\r");
    tui.wait_for("the row lit", WAIT, |s| {
        s.contents().contains("● Late night")
    });
    tui.wait_for_text("Saved Late night.");
    let text = tui.screen_text();
    assert!(text.contains("x delete ─┐"), "{text}");
    tui.send_keys("x");
    tui.wait_for_text("Delete Late night?");
    tui.send_keys("\x1b");
    tui.wait_for("the question answered no", WAIT, |s| {
        !s.contents().contains("Delete Late night?")
    });
    tui.send_keys("\x1b");
    tui.wait_for("the box closed", WAIT, |s| {
        !s.contents().contains("↵ recall")
    });
    tui.quit();
}
