// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The animation, built once from a hand-written correction.

use std::sync::OnceLock;

use crate::tui::screenshots::raster::{Palette, Side, resolve};
use crate::tui::screenshots::scenes::{Animation, animation};
use crate::tui::screenshots::text::{HostDetails, guard};
use crate::tui::screenshots::world::stand_in;
use crate::tui::test_support::text;

fn built() -> &'static Animation {
    static BUILT: OnceLock<Animation> = OnceLock::new();
    BUILT.get_or_init(|| animation(&stand_in()))
}

fn says(n: usize) -> String {
    text(&built().frames[n]).join("\n")
}

fn ground() -> Palette {
    Palette::of(&crate::tui::test_support::theme_for(
        "amber",
        svolder_ipc::ColorDepth::TrueColor,
    ))
}

#[test]
fn it_lasts_ten_seconds_at_thirty_frames_a_second() {
    let n = built().frames.len();
    assert!((285..=315).contains(&n), "{n} frames");
}

#[test]
fn it_opens_on_bare_ground_and_closes_on_it() {
    let palette = ground();
    let first = &built().frames[0];
    assert!(
        first.content().iter().all(|c| c.symbol().trim().is_empty()),
        "Particles at 0 s is bare ground"
    );
    assert!(
        first
            .content()
            .iter()
            .all(|c| resolve(c.bg, Side::Bg, &palette) == Ok(palette.ground))
    );
    let last = built().frames.last().expect("a frame");
    for cell in last.content() {
        assert_eq!(resolve(cell.fg, Side::Fg, &palette), Ok(palette.ground));
        assert_eq!(resolve(cell.bg, Side::Bg, &palette), Ok(palette.ground));
    }
}

#[test]
fn the_first_loop_frame_hides_the_header_logo_and_the_exit_flies_into_it() {
    let a = built();
    let first_loop = says(a.loop_start);
    let later = says(a.loop_start + 45);
    let header = |s: &str| s.lines().take(3).collect::<Vec<_>>().join("\n");
    assert_ne!(
        header(&first_loop),
        header(&later),
        "the logo appears as the sparks land"
    );
}

/// The cells of frame `n` whose foreground is exactly `colour`.
fn drawn_in(n: usize, colour: ratatui::style::Color) -> usize {
    built().frames[n]
        .content()
        .iter()
        .filter(|c| c.fg == colour)
        .count()
}

#[test]
fn the_exit_hands_over_with_the_panes_held_in_the_themes_pane_grey() {
    use crate::tui::decoration::effects::SWEPT_FROM;
    use crate::tui::theme::Surface;
    let a = built();
    let theme = crate::tui::test_support::theme_for("amber", svolder_ipc::ColorDepth::TrueColor);
    let pane = theme.surface(Surface::Pane).expect("a pane grey");
    assert_ne!(
        pane, SWEPT_FROM,
        "the two greys differ, or this proves nothing"
    );
    // The frame the intro's last turn leaves, then the loop's first: the held panes are the
    // theme's grey at once, never the layer's starting grey.
    assert_eq!(
        drawn_in(a.loop_start - 1, pane),
        0,
        "no pane before the hand-over"
    );
    assert_eq!(
        drawn_in(a.loop_start, SWEPT_FROM),
        0,
        "the starting grey on the first frame"
    );
    assert!(
        drawn_in(a.loop_start, pane) > 500,
        "{} cells in the pane grey on the first loop frame",
        drawn_in(a.loop_start, pane)
    );
}

#[test]
fn the_volume_steps_from_minus_30_1_to_minus_24_1_db() {
    let a = built();
    assert!(
        says(a.volume_frame - 1).contains("-30.1 dB"),
        "{}",
        says(a.volume_frame - 1)
    );
    assert!(
        says(a.volume_frame + 30).contains("-24.1 dB"),
        "{}",
        says(a.volume_frame + 30)
    );
}

#[test]
fn the_key_takes_it_to_the_tone_screen_and_the_rise_marks_the_preset_changed() {
    let a = built();
    assert!(!says(a.key_frame - 1).contains(" Response "));
    assert!(says(a.key_frame + 20).contains(" Response "));
    assert!(!says(a.gain_frame - 1).contains("Sennheiser HD 800 S*"));
    assert!(says(a.gain_frame + 10).contains("Sennheiser HD 800 S*"));
}

#[test]
fn the_timeline_is_in_order_and_the_fade_starts_on_an_unfaded_frame() {
    let a = built();
    assert!(a.loop_start < a.volume_frame);
    assert!(a.volume_frame < a.key_frame);
    assert!(a.key_frame < a.gain_frame);
    assert!(a.gain_frame < a.fade_start);
    assert!(a.fade_start < a.frames.len());
    // The frame before the fade is the TUI's own: it carries colours the fade would have
    // rewritten as plain RGB, such as the terminal's default.
    let palette = ground();
    let untouched = a.frames[a.fade_start - 1]
        .content()
        .iter()
        .any(|c| resolve(c.fg, Side::Fg, &palette) != Ok(palette.ground));
    assert!(untouched);
    assert_ne!(a.frames[a.fade_start - 1], a.frames[a.fade_start]);
}

#[test]
fn no_frame_carries_a_toast_or_a_host_detail() {
    let a = built();
    assert_eq!(a.toasts, 0);
    // A fixed identity: the suite must pass whoever runs it and wherever.
    let host = HostDetails::new(
        Some("/home/someone".into()),
        Some("someone".into()),
        Some("box7".into()),
    );
    for (n, frame) in a.frames.iter().enumerate() {
        guard(frame, &host, &format!("svolder, frame {n}")).unwrap_or_else(|why| panic!("{why}"));
        assert!(
            !text(frame).iter().any(|l| l.contains("Passphrase:")),
            "frame {n}: a passphrase note"
        );
    }
}

#[test]
fn the_animation_built_twice_is_the_same() {
    let again = animation(&stand_in());
    assert_eq!(again.frames, built().frames);
}
