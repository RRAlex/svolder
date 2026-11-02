// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::widgets::Block;

use super::*;

/// A scene of a device with a mute, a balance and a tone control.
fn full_scene() -> Scene {
    Scene {
        has_eq: true,
        has_mute: true,
        has_balance: true,
        bands: 5,
        ..Scene::default()
    }
}

const MUTE: Action = Action {
    code: KeyCode::Char('m'),
    word: "mute",
};

const CENTRE: Action = Action {
    code: KeyCode::Char('0'),
    word: "0 centre",
};

const REFRESH: Action = Action {
    code: KeyCode::Char('r'),
    word: "refresh",
};

/// The text of `line`.
fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// A word's key is its first letter in bold in the key role and the rest is in the text
/// role; `0 centre` draws its key before the name the same way. Words are joined by ` ─ `
/// with a blank each side, and each answers its offset, width and key.
#[test]
fn the_words_draw_their_key_bold_and_the_rest_as_text() {
    let theme = Theme::new(false);
    let (line, hits) = actions_title(
        &[MUTE, CENTRE, REFRESH],
        None,
        40,
        Screen::Device,
        &full_scene(),
        &theme,
    );
    assert_eq!(line_text(&line), " mute ─ 0 centre ─ refresh ");
    assert_eq!(
        hits,
        vec![
            (1, 4, KeyCode::Char('m')),
            (8, 8, KeyCode::Char('0')),
            (19, 7, KeyCode::Char('r')),
        ]
    );
    let key = theme.style(Role::KeyHint).add_modifier(Modifier::BOLD);
    let text = theme.style(Role::Text);
    let styled = |content: &str| {
        line.spans
            .iter()
            .find(|span| span.content == content)
            .unwrap_or_else(|| panic!("no span {content:?}"))
            .style
    };
    assert_eq!(styled("m"), key);
    assert_eq!(styled("ute"), text);
    assert_eq!(styled("0"), key);
    assert_eq!(styled(" centre"), text);
    assert_eq!(styled("r"), key);
    assert_eq!(styled("efresh"), text);
}

/// An action unavailable on the device is faint whole, and still answers its hit.
#[test]
fn an_unavailable_word_is_faint_whole() {
    let theme = Theme::new(false);
    let bare = Scene {
        has_mute: false,
        has_balance: false,
        ..full_scene()
    };
    let (line, hits) = actions_title(
        &[MUTE, CENTRE, REFRESH],
        None,
        40,
        Screen::Device,
        &bare,
        &theme,
    );
    let faint = theme.style(Role::Faint);
    for word in ["mute", "0 centre"] {
        let span = line
            .spans
            .iter()
            .find(|span| span.content == word)
            .unwrap_or_else(|| panic!("{word:?} is not one span"));
        assert_eq!(span.style, faint, "{word:?}");
    }
    assert!(
        line.spans.iter().any(|span| span.content == "r"),
        "an available word beside them lost its key"
    );
    assert_eq!(hits.len(), 3, "a faint word still has its hit");
}

/// The mark stays at the far right with the words left of it, one ` ─ ` apart.
#[test]
fn the_mark_sits_right_of_the_words() {
    let theme = Theme::new(false);
    let (line, _) = actions_title(
        &[REFRESH],
        Some("6 functions"),
        40,
        Screen::Telemetry,
        &full_scene(),
        &theme,
    );
    assert_eq!(line_text(&line), " refresh ─ 6 functions ");
    let (alone, hits) = actions_title(
        &[],
        Some("more ↑"),
        40,
        Screen::Devices,
        &full_scene(),
        &theme,
    );
    assert_eq!(line_text(&alone), " more ↑ ");
    assert!(hits.is_empty());
}

/// A border too short drops words from the last one back; the mark stays.
#[test]
fn a_short_border_drops_words_from_the_last_back() {
    let theme = Theme::new(false);
    let shown = |room: u16, mark: Option<&str>| {
        let (line, hits) = actions_title(
            &[MUTE, CENTRE, REFRESH],
            mark,
            room,
            Screen::Device,
            &full_scene(),
            &theme,
        );
        (line_text(&line), hits.len())
    };
    assert_eq!(
        shown(27, None),
        (" mute ─ 0 centre ─ refresh ".to_string(), 3)
    );
    assert_eq!(shown(26, None), (" mute ─ 0 centre ".to_string(), 2));
    assert_eq!(shown(16, None), (" mute ".to_string(), 1));
    assert_eq!(shown(5, None), (String::new(), 0));
    assert_eq!(shown(5, Some("3")), (" 3 ".to_string(), 0));
    assert_eq!(shown(10, Some("3")), (" mute ─ 3 ".to_string(), 1));
}

/// A Controls pane 20 cells wide keeps `mute` only, with the two dashes the title needs
/// before it.
#[test]
fn a_narrow_controls_border_keeps_mute_only() {
    let theme = Theme::new(false);
    let (line, _) = actions_title(
        &[MUTE, CENTRE, REFRESH],
        None,
        action_room(20, " Controls ".chars().count()),
        Screen::Device,
        &full_scene(),
        &theme,
    );
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(20, 3))
        .expect("a test terminal opens");
    terminal
        .draw(|frame| {
            frame.render_widget(
                Block::bordered()
                    .title(" Controls ")
                    .title(line.right_aligned()),
                frame.area(),
            );
        })
        .expect("the frame draws");
    assert_eq!(
        crate::tui::test_support::text(terminal.backend().buffer())[0],
        "┌ Controls ── mute ┐"
    );
}
