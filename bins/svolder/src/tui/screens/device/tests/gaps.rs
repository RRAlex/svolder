// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The empty lines that air out the Host group: one before the Path row, the Spectrum row, the
//! computer's Volume row and the host graph, each drawn only from rows the Controls pane has to
//! spare.

use super::host_group::{daemon_hosting, gungnir_route};
use super::*;

/// One frame of `app` at `width` by `height`: its rows.
fn drawn(app: &App, width: u16, height: u16) -> Vec<String> {
    text(&frame(app, width, height))
}

/// The Controls pane's text on a row of a frame: what follows its border, trimmed.
fn left(row: &str) -> String {
    row.split('│').nth(1).unwrap_or("").trim().to_string()
}

/// Which of the `labels` start a row of the Controls pane that has an empty line right above it
/// under a row of the pane's own (never the top of the pane).
fn aired(rows: &[String], labels: &[&'static str]) -> Vec<&'static str> {
    let mut found = Vec::new();
    for label in labels {
        let Some(at) = rows.iter().position(|row| {
            let words = left(row);
            words.trim_start_matches('▶').trim().starts_with(label)
        }) else {
            continue;
        };
        if at >= 2 && left(&rows[at - 1]).is_empty() && !left(&rows[at - 2]).is_empty() {
            found.push(*label);
        }
    }
    found
}

/// The device the gaps are told on: a DAC with a Path row and a Spectrum row, the daemon
/// speaking for the host.
fn dac() -> App {
    let mut app = daemon_hosting(gungnir_route(|_| {}));
    app.host.capturing = true;
    app
}

/// A device with no volume of its own, so the Host group carries the computer's Volume row.
fn no_volume() -> App {
    let mut app = app_on(profile_named("kraken"));
    app.host.linked = true;
    app.host.fold_read(
        Some(svolder_host::SinkState {
            volume: 0.54,
            muted: false,
            name: "USB DAC".into(),
            target: None,
        }),
        true,
    );
    app.host.capturing = true;
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq(
        false,
        &[("PK", 1000.0, 0.0, 1.0)],
    ));
    app.flip_side();
    app
}

/// The gaps of the Host group at the four terminal sizes the screen is pinned at: one before
/// the Path row wherever a row is spare (the 80x24 frame is packed, and its rows go to the Path
/// row's reserve of its most lines), and in the tall and wide frames every gap,
/// before the Spectrum row too, the host graph aired as it always was. No gap comes between the
/// Host heading and the computer's Volume row (the switch line that opens it).
#[test]
fn the_gaps_appear_at_these_sizes() {
    let sizes = [(80, 24), (100, 45), (120, 40), (160, 50)];
    // At 80x24 the pane is packed and the rows it has go to the Path row's reserve: no gap.
    let expected_dac: [&[&str]; 4] = [
        &[],
        &["Path", "Spectrum", "host"],
        &["Path", "Spectrum", "host"],
        &["Path", "Spectrum", "host"],
    ];
    let expected_bare: [&[&str]; 4] = [
        &[],
        &["Spectrum", "host"],
        &["Spectrum", "host"],
        &["Spectrum", "host"],
    ];
    for (at, (width, height)) in sizes.into_iter().enumerate() {
        let rows = drawn(&dac(), width, height);
        assert_eq!(
            aired(&rows, &["Path", "Spectrum", "host"]),
            expected_dac[at],
            "dac {width}x{height}"
        );
        let rows = drawn(&no_volume(), width, height);
        assert_eq!(
            aired(&rows, &["Target", "Spectrum", "host"]),
            expected_bare[at],
            "no volume {width}x{height}"
        );
    }
}

/// The gaps are rows the pane has to spare: at 80x24 the Status pane starts where it did without
/// them, and the tone pane keeps its rows.
#[test]
fn the_gaps_cost_the_other_panes_no_rows_at_80x24() {
    let rows = drawn(&dac(), 80, 24);
    let status = rows
        .iter()
        .position(|row| row.starts_with("┌ Status"))
        .expect("the Status pane");
    assert_eq!(status, 18);
    let tone = rows.iter().filter(|row| row.contains("││")).count();
    let plain = drawn(&host_mockup_app(), 80, 24);
    assert_eq!(
        plain.iter().filter(|row| row.contains("││")).count(),
        tone,
        "the panes are as tall"
    );
}

/// The pane's lines at `height` rows in the wide tier, as text.
fn pane_lines(app: &App, height: usize) -> Vec<String> {
    let tier = crate::tui::layout_tier::Tier {
        wide: true,
        tall: false,
    };
    super::super::controls::controls_pane(app, &Theme::new(false), 60, tier, height)
        .lines
        .iter()
        .map(|line| line.to_string())
        .collect()
}

/// Whether the line of `lines` that starts with `label` has an empty line above it.
fn gap_before(lines: &[String], label: &str) -> bool {
    let at = lines
        .iter()
        .position(|line| line.trim_start_matches(['▶', ' ']).starts_with(label))
        .unwrap_or_else(|| panic!("no {label}: {lines:#?}"));
    at > 0 && lines[at - 1].trim().is_empty()
}

/// With rows to spare for only some of the gaps they fill in the order Path, host, Spectrum; no
/// row is lost for one, and where nothing is spare nothing is added.
#[test]
fn spare_rows_fill_the_gaps_in_order() {
    let app = dac();
    let all = pane_lines(&app, 100);
    assert!(gap_before(&all, "Path") && gap_before(&all, "Spectrum"));
    // The pane counts the Path row at its most lines: the slack is what the fixture's takes less.
    let drawn = super::super::controls::controls_pane(
        &app,
        &Theme::new(false),
        60,
        crate::tui::layout_tier::Tier {
            wide: true,
            tall: false,
        },
        100,
    )
    .path
    .expect("a Path row");
    let slack = super::super::path_line::MAX_LINES - usize::from(drawn.height);
    let full = all.len() + slack;
    let one = pane_lines(&app, full - 1);
    assert!(
        gap_before(&one, "Path") && !gap_before(&one, "Spectrum"),
        "{one:#?}"
    );
    assert_eq!(one.len() + slack, full - 1);
    let none = pane_lines(&app, full - 2);
    assert!(!gap_before(&none, "Path") && !gap_before(&none, "Spectrum"));
    assert_eq!(none.len() + slack, full - 2);
    // Without a row to spare there are no gaps, and every row stays.
    let packed = pane_lines(&app, 1);
    let tight = pane_lines(&app, packed.len());
    assert_eq!(tight.len(), packed.len());
    assert!(!gap_before(&tight, "Path") && !gap_before(&tight, "Spectrum"));
    assert!(tight.iter().any(|line| line.contains("Spectrum")));

    // Short of the blanks between the groups the layout is packed, and its spare rows go to no
    // gap: the group spacing the user also asked for outranks them.
    let spaced = full - 2;
    let packed = pane_lines(&app, 1).len();
    assert!(packed + 1 < spaced, "the fixture has blanks to give up");
    let between = pane_lines(&app, spaced - 1);
    assert!(!gap_before(&between, "Path") && !gap_before(&between, "Spectrum"));
    assert!(
        between.len() < spaced - 1,
        "the packed layout has rows to spare"
    );

    // A device with no volume: the Host heading stays on its first row, no gap comes between.
    let app = no_volume();
    let all = pane_lines(&app, 100);
    assert!(gap_before(&all, "Spectrum") && !gap_before(&all, "Target"));
    assert!(
        gap_before(&all, "Host"),
        "the group's own blank is above the heading"
    );
    // Never at an edge of the pane.
    assert!(all.first().is_some_and(|line| !line.trim().is_empty()));
    assert!(all.last().is_some_and(|line| !line.trim().is_empty()));
}
