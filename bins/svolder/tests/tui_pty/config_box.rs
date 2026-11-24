// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The PipeWire config box in the real TUI: the Path row's click, `k` to open and close the
//! box, and the box's own read of `pw-dump` where no daemon speaks for the host (a fake `pw-dump`
//! that prints a capture of a private PipeWire instance).

use std::path::Path;

use crate::common::TempDir;
use crate::harness::*;
use crate::tap::*;

/// Wide and tall enough for the Path row to hold its hops.
const SIZE: (u16, u16) = (160, 40);

/// The lines of the box, by name.
const LABELS: [&str; 7] = [
    "rates",
    "forced rate",
    "dither",
    "24-bit output",
    "no plug",
    "stereo to stereo",
    "no filter in the sink",
];

fn has(tui: &Tui, needle: &str) -> bool {
    tui.screen_text().contains(needle)
}

/// The demo daemon's simulated host reads every line as a pass: the row carries no config tag and
/// `k` opens the box with seven passing lines; `k` and `Esc` close it.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn k_opens_the_config_box_over_the_demo_daemons_route_and_closes_it() {
    let tui = Tui::start(SIZE.0, SIZE.1, Responder::None);
    tui.wait_ready();
    tui.wait_for_text("→ DAC");
    assert!(!has(&tui, "config ✓") && !has(&tui, "config 1"));
    assert!(has(&tui, "k check"), "{}", tui.screen_text());
    tui.send_keys("k");
    tui.wait_for_text("PipeWire config lets it through untouched");
    let text = tui.screen_text();
    for label in LABELS {
        assert!(text.contains(label), "{label}:\n{text}");
    }
    assert!(text.contains("k or esc close"), "{text}");
    // The demo's fake DAC sits below 0 dB, so the box has a `What changes the sound` section with
    // the DAC's row (the Path row under the box keeps its label and its volume on other lines).
    assert!(text.contains("What changes the sound"), "{text}");
    assert!(
        text.lines()
            .any(|line| line.contains("DAC USB") && line.contains("volume -")),
        "{text}"
    );
    tui.send_keys("k");
    tui.wait_for("the box closed", WAIT, |s| {
        !s.contents().contains("lets it through untouched")
    });
    tui.send_keys("k");
    tui.wait_for_text("PipeWire config lets it through untouched");
    tui.send_keys("\x1b");
    tui.wait_for("the box closed by Esc", WAIT, |s| {
        !s.contents().contains("lets it through untouched")
    });
    // Esc closed the box and did not open the menu under it.
    assert!(!has(&tui, "Quit"), "{}", tui.screen_text());
    tui.quit();
}

/// A click on the Path row opens the box, and a click outside it closes it.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_click_on_the_path_row_opens_the_box_and_a_click_outside_closes_it() {
    let tui = Tui::start(SIZE.0, SIZE.1, Responder::None);
    tui.wait_ready();
    tui.wait_for_text("→ DAC");
    let (col, row) = tui.find("→ DAC");
    tui.send_mouse(Mouse::Click, col + 2, row);
    tui.wait_for_text("PipeWire config lets it through untouched");
    tui.send_mouse(Mouse::Click, 0, SIZE.1 - 1);
    tui.wait_for("the box closed", WAIT, |s| {
        !s.contents().contains("lets it through untouched")
    });
    tui.quit();
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// A direct session (no daemon) over a fake host: `wpctl` names a sink and `pw-dump` prints the
/// capture, so the box reads the graph itself when it opens.
#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn without_a_daemon_the_box_reads_pw_dump_itself() {
    let dir = TempDir::new("config-box");
    let wpctl = "#!/bin/sh\n\
        case \"$1\" in\n\
          get-volume) echo 'Volume: 0.54' ;;\n\
          inspect) printf '  node.name = \"cfg_sink\"\\n  node.description = \"Config sink\"\\n  media.class = \"Audio/Sink\"\\n' ;;\n\
          *) exit 1 ;;\n\
        esac\n";
    let capture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/svolder-host/tests/fixtures/pw-dump/setup-pipe.json"
    );
    let dump = format!("#!/bin/sh\ncat '{capture}'\n");
    for (name, text) in [("wpctl", wpctl.to_string()), ("pw-dump", dump)] {
        let path = dir.join(name);
        std::fs::write(&path, text).expect("write a fake program");
        make_executable(&path);
    }
    let asound = dir.join("asound");
    std::fs::create_dir_all(&asound).expect("an empty ALSA tree");
    let wpctl = dir.join("wpctl");
    let dump = dir.join("pw-dump");
    let tui = Tui::start_plain_with_env(
        SIZE.0,
        SIZE.1,
        Responder::None,
        "title_meter = false\nvisualizer = false\ncover_art = \"off\"\neffects = false",
        &[
            ("SVOLDER_HOST", "on"),
            ("SVOLDER_WPCTL", wpctl.to_str().expect("a path")),
            ("SVOLDER_PW_DUMP", dump.to_str().expect("a path")),
            (
                svolder_host::ASOUND_DIR_ENV,
                asound.to_str().expect("a path"),
            ),
        ],
    );
    tui.wait_ready();
    tui.send_keys("k");
    tui.wait_for("the box with the graph's own lines", WAIT, |s| {
        let text = s.contents();
        text.contains("PipeWire config") && text.contains("svolder e2e sink")
    });
    for label in LABELS {
        assert!(has(&tui, label), "{label}:\n{}", tui.screen_text());
    }
    tui.send_keys("k");
    tui.wait_for("the box closed", WAIT, |s| {
        !s.contents().contains("PipeWire config")
    });
    tui.quit();
}
