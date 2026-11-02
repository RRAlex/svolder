// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The control methods' own guards: each acts on its own kind of control only.

use super::*;
use crate::backend::DeviceRow;
use crate::tui::test_support::profile;
use svolder_ipc::TuiConfig;

/// `flip` turns a toggle and nothing else; `step_select` steps a select and nothing else;
/// `step_level` steps a level and nothing else.
#[test]
fn each_control_method_acts_on_its_own_kind_only() {
    let app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![DeviceRow {
            name: "gungnir".into(),
            ..DeviceRow::default()
        }],
    );
    let of = |kind| {
        app.profile
            .controls
            .iter()
            .find(|c| c.kind == kind)
            .expect("a control")
    };
    let (toggle, select, level) = (
        of(ControlKind::Toggle),
        of(ControlKind::Select),
        of(ControlKind::Level),
    );
    assert!(!app.flip(toggle).is_empty());
    assert!(app.flip(select).is_empty() && app.flip(level).is_empty());
    assert!(app.step_select(toggle, 1).is_empty() && app.step_select(level, 1).is_empty());
    assert!(!app.step_level(level, 1).is_empty());
    assert!(app.step_level(select, 1).is_empty() && app.step_level(toggle, 1).is_empty());
}
