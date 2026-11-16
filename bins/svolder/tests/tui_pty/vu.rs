// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The VU meter in the real TUI: a direct session whose host is faked by two small programs, a
//! `wpctl` that names a sink and a `pw-record` that plays a sine at -18 dBFS RMS or, once asked,
//! a full-scale square. The rows are there from 100 by 45 and nowhere below, the reading settles
//! near 0 VU, the lamp lights and goes dark, and the capture stops when no row needs it.

use std::path::Path;

use crate::common::TempDir;
use crate::harness::*;
use crate::tap::*;

/// Frames in one chunk the fake capture writes: 20 ms at 48 kHz, a whole number of periods of the
/// 1 kHz tones, so the stream is continuous across chunks.
const CHUNK: usize = 960;

/// The peak of a sine whose RMS is -18 dBFS: -15 dBFS.
const SINE_PEAK: f32 = 0.177_827_94;

/// One chunk of both channels as 32-bit floats, little endian: a 1 kHz tone of `peak`, a sine or a
/// square.
fn chunk(peak: f32, square: bool) -> Vec<u8> {
    (0..CHUNK)
        .flat_map(|frame| {
            let phase = (frame % 48) as f32 / 48.0;
            let wave = match square {
                true if phase < 0.5 => 1.0,
                true => -1.0,
                false => (phase * std::f32::consts::TAU).sin(),
            };
            let sample = (wave * peak).to_le_bytes();
            [sample, sample]
        })
        .flatten()
        .collect()
}

impl Drop for Rig {
    /// Stop the fake `pw-record` if the program under test left it running.
    fn drop(&mut self) {
        let folder = self.dir.path().display().to_string();
        let ours = |pid: u32| {
            std::fs::read(format!("/proc/{pid}/cmdline"))
                .is_ok_and(|cmdline| String::from_utf8_lossy(&cmdline).contains(&folder))
        };
        // Only a process whose command line names this rig's folder is the one it started.
        if let Some(pid) = self.recording().filter(|pid| ours(*pid)) {
            let _ = std::process::Command::new("kill")
                .arg(pid.to_string())
                .status();
        }
    }
}

/// The folder of the fake programs and their tones. A `square` file in it switches the capture to
/// the square.
struct Rig {
    dir: TempDir,
}

impl Rig {
    fn new() -> Rig {
        let dir = TempDir::new("vu-rig");
        std::fs::write(dir.join("sine.raw"), chunk(SINE_PEAK, false)).expect("write the sine");
        std::fs::write(dir.join("square.raw"), chunk(1.0, true)).expect("write the square");
        let wpctl = "#!/bin/sh\n\
            case \"$1\" in\n\
              get-volume) echo 'Volume: 0.54' ;;\n\
              inspect) printf '  node.name = \"vu_sink\"\\n  node.description = \"VU sink\"\\n  media.class = \"Audio/Sink\"\\n' ;;\n\
              *) exit 1 ;;\n\
            esac\n";
        // The loop ends with its reader (a failed write ends `cat`) and with the folder, so a case
        // that fails leaves no process behind.
        let record = format!(
            "#!/bin/sh\n\
             echo $$ > '{dir}/record.pid'\n\
             while [ -d '{dir}' ]; do\n\
               if [ -e '{dir}/square' ]; then tone='{dir}/square.raw'; else tone='{dir}/sine.raw'; fi\n\
               cat \"$tone\" || exit 0\n\
               sleep 0.02\n\
             done\n",
            dir = dir.path().display()
        );
        for (name, text) in [("wpctl", wpctl.to_string()), ("pw-record", record)] {
            let path = dir.join(name);
            std::fs::write(&path, text).expect("write a fake program");
            make_executable(&path);
        }
        Rig { dir }
    }

    /// The terminal of `cols` x `rows`, a direct session whose host is this rig's, with nothing
    /// but the VU rows asking for the capture.
    fn start(&self, cols: u16, rows: u16) -> Tui {
        let wpctl = self.dir.join("wpctl");
        let record = self.dir.join("pw-record");
        Tui::start_plain_with_env(
            cols,
            rows,
            Responder::None,
            "title_meter = false\nvisualizer = false\ncover_art = \"off\"\neffects = false",
            &[
                ("SVOLDER_HOST", "on"),
                ("SVOLDER_WPCTL", wpctl.to_str().expect("a path")),
                ("SVOLDER_PW_RECORD", record.to_str().expect("a path")),
            ],
        )
    }

    /// Play the square from now on, or the sine again.
    fn square(&self, on: bool) {
        let flag = self.dir.join("square");
        match on {
            true => std::fs::write(&flag, "").expect("flip to the square"),
            false => {
                let _ = std::fs::remove_file(&flag);
            }
        }
    }

    /// The fake `pw-record`'s process id, once it has run.
    fn recording(&self) -> Option<u32> {
        std::fs::read_to_string(self.dir.join("record.pid"))
            .ok()?
            .trim()
            .parse()
            .ok()
    }
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// Whether a process is still running.
fn alive(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        stat.split_whitespace()
            .nth(2)
            .is_none_or(|state| state != "Z")
    })
}

/// The row of `screen` that holds `label`.
fn row_with(screen: &vt100::Screen, label: &str) -> Option<String> {
    screen
        .contents()
        .lines()
        .find(|line| line.contains(label))
        .map(str::to_string)
}

/// The VU reading on `line`: the number after the track, `None` for `<` and `—`.
fn reading(line: &str) -> Option<f64> {
    let after = line.split_once("VU ")?.1;
    let words: Vec<&str> = after.split_whitespace().collect();
    words
        .iter()
        .find_map(|word| word.parse::<f64>().ok().filter(|_| word.contains('.')))
}

/// The lamp `line` shows.
fn lamp(line: &str) -> Option<char> {
    line.chars().rev().find(|glyph| matches!(glyph, '●' | '○'))
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_vu_rows_read_zero_for_the_reference_level_and_light_the_lamp_for_a_full_scale_square() {
    let rig = Rig::new();
    let tui = rig.start(100, 45);
    tui.wait_for("both VU rows reading 0 VU with the lamps dark", WAIT, |s| {
        ["VU L", "VU R"].iter().all(|label| {
            row_with(s, label).is_some_and(|line| {
                reading(&line).is_some_and(|db| db.abs() <= 0.15) && lamp(&line) == Some('○')
            })
        })
    });
    // The rows sit in the Status pane, under the Level from row that says where 0 VU is.
    let text = tui.screen_text();
    let level = text
        .lines()
        .position(|line| line.contains("Level from"))
        .expect("Level from");
    let left = text
        .lines()
        .position(|line| line.contains("VU L"))
        .expect("VU L");
    assert!(level < left, "{text}");
    assert!(text.contains("(0 VU = -18 dBFS)"), "{text}");

    rig.square(true);
    tui.wait_for(
        "a lit lamp on both rows and the needle at the right stop",
        WAIT,
        |s| {
            ["VU L", "VU R"].iter().all(|label| {
                row_with(s, label).is_some_and(|line| {
                    lamp(&line) == Some('●') && reading(&line).is_some_and(|db| db > 3.0)
                })
            })
        },
    );
    rig.square(false);
    tui.wait_for(
        "the lamps dark again and the reading back at 0 VU",
        WAIT,
        |s| {
            row_with(s, "VU L").is_some_and(|line| {
                lamp(&line) == Some('○') && reading(&line).is_some_and(|db| db.abs() <= 0.15)
            })
        },
    );
    tui.quit();
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn below_100_by_45_there_are_no_vu_rows_and_no_level_line_reading() {
    let rig = Rig::new();
    for (cols, rows) in [(99, 45), (100, 44)] {
        let tui = rig.start(cols, rows);
        // The rows are the only thing that would hold the capture: with none, it is not run.
        tui.wait_for_text("Level from: ");
        let text = tui.screen_text();
        assert!(
            !text.contains("VU L") && !text.contains("VU R"),
            "{cols}x{rows}\n{text}"
        );
        assert!(!text.contains("0 VU ="), "{cols}x{rows}\n{text}");
        tui.quit();
    }
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn a_resize_below_100_by_45_removes_the_rows_and_stops_the_capture_and_back_brings_them() {
    let rig = Rig::new();
    let tui = rig.start(100, 45);
    tui.wait_for("a reading, so the capture is running", WAIT, |s| {
        row_with(s, "VU R").is_some_and(|line| reading(&line).is_some())
    });
    let pid = rig.recording().expect("the capture started for the rows");
    assert!(alive(pid));

    tui.resize(99, 45);
    tui.wait_for("the rows gone", WAIT, |s| !s.contents().contains("VU L"));
    tui.wait_for("the capture gone", WAIT, |_| !alive(pid));
    assert!(!alive(pid), "the capture went on with nothing to show");
    // With no capture the level comes from nowhere.
    tui.wait_for_text("Level from: —");

    tui.resize(100, 45);
    tui.wait_for_text("VU L");
    tui.wait_for("a reading again", WAIT, |s| {
        row_with(s, "VU L").is_some_and(|line| reading(&line).is_some())
    });
    tui.quit();
}

#[test]
#[ignore = "runs the real binary in a pty: make tui-harness"]
fn the_settings_vu_row_writes_the_reference_and_the_level_from_row_follows() {
    let rig = Rig::new();
    let tui = rig.start(100, 45);
    tui.wait_for_text("(0 VU = -18 dBFS)");
    // Settings (`s`), Host audio (`4`), down to the row after Cover art: VU reference.
    tui.send_keys("s");
    tui.wait_for_text("Palette");
    tui.send_keys("4");
    tui.wait_for_text("VU reference");
    tui.send_keys("\x1b[B\x1b[B\x1b[B");
    tui.wait_for("the VU reference row's explanation", WAIT, |s| {
        s.contents().contains("[tui] vu_reference = -18")
    });
    tui.send_keys("\x1b[C");
    tui.wait_for("the row at -17", WAIT, |s| {
        s.contents().contains("[tui] vu_reference = -17")
    });
    tui.send_keys("\x1b");
    tui.wait_for_text("(0 VU = -17 dBFS)");
    let config = std::fs::read_to_string(tui.home().join("config/svolder/config.toml"))
        .expect("the configuration file");
    assert!(
        config.lines().any(|line| line == "vu_reference = -17"),
        "{config}"
    );
    tui.quit();
}
