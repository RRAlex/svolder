// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The five stills, built from a hand-written correction.

use crate::tui::screenshots::scenes::{Still, still, still_with};
use crate::tui::screenshots::text::{HostDetails, guard};
use crate::tui::screenshots::world::stand_in;
use crate::tui::test_support::{text, theme_for};

fn says(still_: Still) -> String {
    text(&still(&stand_in(), still_).buffer).join("\n")
}

/// The first line of `lines` whose label, once the pane's border and the cursor are trimmed, is
/// `label` followed by blanks and then `then`.
fn row<'a>(lines: &'a str, label: &str, then: impl Fn(char) -> bool) -> Option<&'a str> {
    lines.lines().find(|line| {
        line.trim_start_matches(|c: char| matches!(c, '│' | '▶') || c.is_whitespace())
            .strip_prefix(label)
            .is_some_and(|rest| {
                rest.starts_with(' ') && rest.trim_start().chars().next().is_some_and(&then)
            })
    })
}

/// The row of a toggle: its label, then its `[x]` or `[ ]`.
fn toggle<'a>(lines: &'a str, label: &str) -> Option<&'a str> {
    row(lines, label, |c| c == '[')
}

/// The cell columns of the pane that holds column `at` of `line`: from the border to its left
/// to the border to its right, or the line's ends where there is none.
fn pane_span(line: &str, at: usize) -> std::ops::Range<usize> {
    let cells: Vec<char> = line.chars().collect();
    let left = cells[..at].iter().rposition(|&c| c == '│').unwrap_or(0);
    let right = cells[at..]
        .iter()
        .position(|&c| c == '│')
        .map_or(cells.len(), |n| at + n + 1);
    left..right
}

/// The text of `line` inside the pane that holds `word`.
fn in_pane_of(line: &str, word: &str) -> String {
    let at = line.find(word).expect("the word is on the line");
    let at = line[..at].chars().count();
    line.chars()
        .skip(pane_span(line, at).start)
        .take(pane_span(line, at).len())
        .collect()
}

#[test]
fn a_pane_is_the_cells_between_its_borders() {
    let line = "│ Power [x] │ Other [ ] │";
    assert_eq!(in_pane_of(line, "Power"), "│ Power [x] │");
    assert_eq!(in_pane_of(line, "Other"), "│ Other [ ] │");
    assert!(!in_pane_of(line, "Power").contains("[ ]"));
}

#[test]
fn a_toggle_row_matches_its_exact_label_only() {
    let rows = "│    Power meters [x] on\n│  ▶ Power        [ ] off\n";
    assert_eq!(toggle(rows, "Power"), Some("│  ▶ Power        [ ] off"));
    assert_eq!(toggle("│    Power meters [x] on", "Power"), None);
}

#[test]
fn each_still_carries_its_landmarks() {
    let dac = says(Still::Dac);
    for word in ["Gungnir", "So What", "Optical", "Link", "Level from: host"] {
        assert!(dac.contains(word), "dac lacks {word}");
    }
    let tone = says(Still::Tone);
    assert!(
        tone.contains("Sennheiser HD 800 S") && tone.contains("oratory1990"),
        "{tone}"
    );
    assert!(says(Still::Telemetry).contains(" Telemetry "));
}

#[test]
fn the_amp_still_shows_the_aegir_awake_with_both_meters_and_the_mirrored_graph() {
    let amp = says(Still::Amp);
    for word in ["Aegir 2", "Meter L", "Meter R", "L/R"] {
        assert!(amp.contains(word), "amp lacks {word}\n{amp}");
    }
    let graph = row(&amp, "L/R", |c| c != '[').expect("an L/R row");
    let drawn = graph
        .chars()
        .filter(|c| ('\u{2801}'..='\u{28ff}').contains(c))
        .count();
    assert!(
        drawn > 80,
        "the L/R graph holds {drawn} cells of history\n{amp}"
    );
    let power = toggle(&amp, "Power").unwrap_or_else(|| panic!("no Power row\n{amp}"));
    assert!(
        in_pane_of(power, "Power").contains("[x]"),
        "the Aegir is in standby: {power}\n{amp}"
    );
    assert!(
        !amp.lines()
            .any(|l| l.contains("Standby") || l.contains("standby")),
        "{amp}"
    );
    assert!(amp.contains("Level from: meters"), "{amp}");
    assert!(
        row(&amp, "host", |c| c != '[').is_none(),
        "a host graph row on the amp\n{amp}"
    );
}

#[test]
fn the_telemetry_still_reads_both_meters_fast() {
    let telemetry = says(Still::Telemetry);
    let fast = telemetry.lines().filter(|l| l.contains("10/s")).count();
    assert!(fast >= 2, "two meters at 10/s, found {fast}\n{telemetry}");
    for line in telemetry.lines().filter(|l| l.contains("10/s")) {
        let drawn = line
            .chars()
            .filter(|c| ('\u{2801}'..='\u{28ff}').contains(c))
            .count();
        assert!(drawn > 30, "a meter graph of {drawn} cells\n{line}");
    }
}

#[test]
fn the_devices_still_lists_both_devices_connected() {
    let devices = says(Still::Devices);
    let listed = |name: &str| {
        devices
            .lines()
            .skip(crate::tui::screens::device::header_height(120).into())
            .filter(|l| l.contains(name) && l.split_whitespace().any(|w| w == "connected"))
            .count()
    };
    assert_eq!(listed("Gungnir"), 1, "{devices}");
    assert_eq!(listed("Aegir 2"), 1, "{devices}");
}

#[test]
fn the_dac_still_draws_the_clipping_risk_zone() {
    let shot = still(&stand_in(), Still::Dac);
    assert!(
        shot.clip_step.is_some(),
        "the stand-in's +5 dB shelf boosts"
    );
    let theme = theme_for("amber", svolder_ipc::ColorDepth::TrueColor);
    let ground = theme.alert_ground().fg.expect("the zone has a colour");
    let lines = text(&shot.buffer);
    let y = lines
        .iter()
        .position(|l| l.contains("Volume"))
        .expect("a Volume row");
    let y = u16::try_from(y).expect("a row");
    let cells = |x: u16| shot.buffer.cell((x, y)).expect("a cell");
    let line = &lines[usize::from(y)];
    let at = line.find("Volume").expect("the label");
    let pane = pane_span(line, line[..at].chars().count());
    assert!(
        pane.clone()
            .any(|x| cells(u16::try_from(x).expect("a column")).fg == ground),
        "no cell in the zone's colour inside the Volume pane\n{line}"
    );
    assert!(lines[usize::from(y)].contains('┃'), "the threshold tick");
}

#[test]
fn no_still_shows_the_mascot_a_toast_or_a_passphrase_note() {
    for s in Still::ALL {
        let shot = still(&stand_in(), s);
        let without = still_with(&stand_in(), s, |app| app.mascot.capable = false);
        assert_eq!(
            shot.buffer,
            without.buffer,
            "{}: the squirrel drew",
            s.name()
        );
        assert!(shot.areas.toast.is_none(), "{}: a toast", s.name());
        assert!(
            !text(&shot.buffer).iter().any(|l| l.contains("Passphrase:")),
            "{}",
            s.name()
        );
    }
}

#[test]
fn every_still_passes_the_guard_on_this_machine() {
    // A fixed identity: the suite must pass whoever runs it and wherever.
    let host = HostDetails::new(
        Some("/home/someone".into()),
        Some("someone".into()),
        Some("box7".into()),
    );
    for s in Still::ALL {
        guard(&still(&stand_in(), s).buffer, &host, s.name()).unwrap_or_else(|why| panic!("{why}"));
    }
}

#[test]
fn a_still_is_taken_with_the_animation_layer_at_rest() {
    for s in Still::ALL {
        assert!(still(&stand_in(), s).at_rest, "{}", s.name());
    }
}

#[test]
fn a_still_built_twice_is_the_same_frame() {
    let one = still(&stand_in(), Still::Dac).buffer;
    std::thread::sleep(std::time::Duration::from_millis(50));
    let two = still(&stand_in(), Still::Dac).buffer;
    assert_eq!(one, two);
}
