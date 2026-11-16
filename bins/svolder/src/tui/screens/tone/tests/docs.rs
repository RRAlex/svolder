// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The documents that show what this screen draws.

use super::*;

/// The mockups docs/usage/tui.md shows of the tone-control screen are the frames this build draws:
/// the flat curve, the stored curve in Flat mode, and band 2's gain being typed.
#[test]
fn the_docs_show_the_tone_screens_this_build_draws() {
    use crate::tui::test_support::assert_usage_md_shows;

    assert_usage_md_shows(&text(&screen(&flat_app())), "tone-control screen");
    assert_usage_md_shows(
        &text(&screen(&in_mode(lifted_app(), 1, 127))),
        "tone-control screen in Flat mode",
    );
    let mut app = flat_app();
    keyed(
        &mut app,
        &[KeyCode::Char('2'), KeyCode::Enter, KeyCode::Tab],
    );
    type_text(&mut app, "1.5");
    let (buffer, _, _) = framed(&app, (80, 24), &Theme::new(false));
    assert_usage_md_shows(&text(&buffer), "tone-control screen's edit line");
}

/// docs/usage/tui.md shows the tone screen with an imported preset loaded with changes at 80×24,
/// and the side pane the wide tier adds, byte for byte what this build draws.
#[test]
fn usage_md_shows_the_target_and_the_side_pane_this_build_draws() {
    let app = imported_app();
    let narrow = text(&frame_with_theme(&app, 80, 24, &Theme::new(false)));
    assert_usage_md_shows(&narrow, "tone-control screen with an imported preset");
    let wide = text(&frame_with_theme(&app, 126, 30, &Theme::new(false)));
    let pane: Vec<String> = wide[3..12]
        .iter()
        .map(|row| {
            row.chars()
                .skip(126 - usize::from(tone::fit::SIDE_WIDTH))
                .collect()
        })
        .collect();
    assert_usage_md_shows(&pane, "tone-control screen's side pane");
}
