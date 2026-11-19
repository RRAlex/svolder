// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_host::RouteFacts;
use svolder_ipc::host::{SetupOutcome, setup_check};

use super::*;

#[test]
fn a_line_wears_the_mark_of_its_outcome() {
    assert_eq!(mark(SetupOutcome::Pass), "✓");
    assert_eq!(mark(SetupOutcome::Fail), "✗");
    assert_eq!(mark(SetupOutcome::Unknown), "?");
}

#[test]
fn a_check_not_read_passes_none_of_its_seven_lines() {
    let check = setup_check(&RouteFacts::default());
    assert_eq!(passes(&check), (0, 7));
}
