// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The twins and the guard.

use ratatui::buffer::Buffer;

use crate::tui::screenshots::text::*;

fn lines(rows: &[&str]) -> Buffer {
    Buffer::with_lines(rows.iter().copied())
}

fn someone() -> HostDetails {
    HostDetails::new(
        Some("/home/someone".into()),
        Some("someone".into()),
        Some("box7\n".into()),
    )
}

#[test]
fn a_still_twin_is_its_rows_with_trailing_blanks_trimmed() {
    let twin = still_twin(&lines(&["ab  ", "    ", " c  "]));
    assert_eq!(twin, "ab\n\n c\n");
}

#[test]
fn the_animation_twin_keeps_each_line_once_in_first_drawn_order_with_braille_blank() {
    let mut twin = AnimationTwin::default();
    twin.add(&lines(&["x⠁ ", "y  "]));
    twin.add(&lines(&["x⠂ ", "z  "]));
    assert_eq!(twin.text(), "x\ny\nz\n");
}

#[test]
fn the_guard_refuses_the_home_the_user_the_host_and_absolute_paths() {
    for line in [
        "SOMEONE was here",
        "on box7 now",
        "svolder tui --join /tmp/sd-abc/sv.sock",
        "(/etc/passwd)",
        "x=/opt/a",
        "/usr/bin",
    ] {
        assert!(refused(line, &someone()).is_some(), "{line}");
    }
}

#[test]
fn the_home_rule_alone_refuses_a_home_inside_a_word() {
    assert_eq!(
        refused("abc/home/someone", &someone()),
        Some("the home directory")
    );
    assert_eq!(
        refused("cfg /home/someone/x", &someone()),
        Some("the home directory")
    );
}

#[test]
fn the_guard_passes_the_pinned_path_and_slashes_inside_words() {
    for line in [
        " ~/.config/svolder/config.toml",
        "L/R",
        "10/s",
        "Volume ██109/127",
        "fw 210.2.1 3",
        "a / b",
    ] {
        assert_eq!(refused(line, &someone()), None, "{line}");
    }
}

#[test]
fn a_refused_frame_names_its_place_and_line() {
    let why = guard(&lines(&["fine", "at /tmp/x"]), &someone(), "dac, frame 3")
        .expect_err("a path is refused");
    assert!(
        why.starts_with("dac, frame 3, line 2: an absolute path"),
        "{why}"
    );
}

/// The home of the process running the test, planted in a frame. A process
/// with no home (or one under two characters, which the guard never looks for) has nothing to
/// plant, so the test returns: the fixed-identity cases above cover the guard itself.
#[test]
fn this_process_own_home_is_refused() {
    let Ok(home) = std::env::var("HOME") else {
        return;
    };
    if home.trim().chars().count() < 2 {
        return;
    }
    let host = HostDetails::new(Some(home.clone()), None, None);
    let why = guard(&lines(&[&format!("x{home}")]), &host, "t").expect_err("the home");
    assert!(why.contains("the home directory"), "{why}");
    let machine = HostDetails::of_this_machine();
    assert!(guard(&lines(&[&format!("x{home}")]), &machine, "t").is_err());
}

#[test]
fn the_user_and_host_names_are_matched_as_whole_words() {
    for user in ["sam", "dev"] {
        let host = HostDetails::new(None, Some(user.into()), Some("eq".into()));
        for line in ["Sample rate 48 kHz", "Devices", "Level", "Value  connected"] {
            assert_eq!(refused(line, &host), None, "{user}: {line}");
        }
    }
    // A name at the very end of a longer word: only the left boundary spares it.
    let ted = HostDetails::new(None, Some("ted".into()), None);
    assert_eq!(refused("connected", &ted), None);
    assert!(refused("by ted", &ted).is_some());
    let host = HostDetails::new(None, Some("sam".into()), Some("box7".into()));
    for line in ["sam@box7", "by SAM.", "on (box7)", "x sam"] {
        assert!(refused(line, &host).is_some(), "{line}");
    }
}

#[test]
fn details_too_short_to_mean_anything_are_not_looked_for() {
    let host = HostDetails::new(Some("/".into()), Some("ab".into()), None);
    assert_eq!(refused("ab a/b", &host), None);
}
