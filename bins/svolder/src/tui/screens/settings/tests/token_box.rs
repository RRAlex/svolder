// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The MCP token box of the Connect category: its keys, its one-line token, its clock and its
//! click.

use super::*;

/// The Connect category on the token row with the box opened by `v` and the loop's answer, the
/// token put on show, as the loop does after `RevealToken`.
fn token_box_app() -> App {
    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    assert_eq!(
        press(&mut app, KeyCode::Char('v')),
        vec![Intent::RevealToken]
    );
    assert_eq!(app.overlays.conn.modal, Some(Modal::Token));
    app.overlays
        .conn
        .reveal(FAKE_TOKEN.to_string(), app.now + connections::REVEAL_FOR);
    app
}

/// `v` and Enter on the token row open the box and ask for the token to be revealed.
#[test]
fn v_and_enter_open_the_box_and_ask_for_the_token() {
    for code in [KeyCode::Char('v'), KeyCode::Enter] {
        let mut app = connect_app();
        select(&mut app, Setting::McpToken);
        assert_eq!(press(&mut app, code), vec![Intent::RevealToken], "{code:?}");
        assert_eq!(app.overlays.conn.modal, Some(Modal::Token), "{code:?}");
    }
}

/// The box draws the token whole on a line of its own, one cell in from the border.
#[test]
fn the_box_draws_the_token_on_one_line_a_cell_in() {
    let app = token_box_app();
    let rows = box_rows(&app);
    let hit: Vec<&String> = rows.iter().filter(|r| r.contains(FAKE_TOKEN)).collect();
    assert_eq!(hit.len(), 1, "{rows:#?}");
    let line = hit[0];
    let at = line.find(FAKE_TOKEN).unwrap();
    assert!(line[..at].ends_with("│ "), "not a cell in: {line}");
    assert!(
        line[at + FAKE_TOKEN.len()..].starts_with(' '),
        "text after the token: {line}"
    );
    assert!(shown(&app).contains("MCP token"));
    assert!(shown(&app).contains("copies"), "{}", shown(&app));
}

/// A token longer than the line wraps onto lines whose joined text is the token, each starting
/// one cell in.
#[test]
fn a_long_token_wraps_into_lines_that_join_to_it() {
    let long = format!("fake-{}", "abcdefghij".repeat(10));
    let mut app = token_box_app();
    app.overlays
        .conn
        .reveal(long.clone(), app.now + connections::REVEAL_FOR);
    let rows = box_rows(&app);
    let border = rows
        .iter()
        .find_map(|r| r.find("┌ MCP token").map(|b| r[..b].chars().count()))
        .unwrap();
    let mut joined = String::new();
    for row in &rows {
        let cells: Vec<char> = row.chars().collect();
        if cells.get(border) != Some(&'│') || cells.get(border + 1) != Some(&' ') {
            continue;
        }
        let part: String = cells[border + 2..]
            .iter()
            .take_while(|c| **c != ' ' && **c != '│')
            .collect();
        // a token line holds what is left of the token
        if !part.is_empty() && long[joined.len()..].starts_with(&part) {
            joined.push_str(&part);
        }
    }
    assert_eq!(joined, long, "{rows:#?}");
}

/// `c` in the box copies and leaves it open.
#[test]
fn c_in_the_box_copies_the_token() {
    let mut app = token_box_app();
    assert_eq!(press(&mut app, KeyCode::Char('c')), vec![Intent::CopyToken]);
    assert_eq!(app.overlays.conn.modal, Some(Modal::Token));
}

/// Esc, q, v and Enter close the box and drop the token's text.
#[test]
fn the_closing_keys_drop_the_token() {
    for code in [
        KeyCode::Esc,
        KeyCode::Char('q'),
        KeyCode::Char('v'),
        KeyCode::Enter,
    ] {
        let mut app = token_box_app();
        assert!(press(&mut app, code).is_empty(), "{code:?}");
        assert!(app.overlays.conn.modal.is_none(), "{code:?}");
        assert!(app.overlays.conn.revealed.is_none(), "{code:?}");
    }
}

/// Another key is swallowed: the box stays and no row under it acts.
#[test]
fn other_keys_are_swallowed() {
    let mut app = token_box_app();
    for code in [KeyCode::Char('R'), KeyCode::Down, KeyCode::Char('x')] {
        assert!(press(&mut app, code).is_empty(), "{code:?}");
        assert_eq!(app.overlays.conn.modal, Some(Modal::Token), "{code:?}");
    }
}

/// The box closes by itself when its sixty seconds are up, and not before.
#[test]
fn the_box_closes_at_sixty_seconds_on_the_clock() {
    assert_eq!(connections::REVEAL_FOR, Duration::from_secs(60));
    let mut app = token_box_app();
    let start = app.now;
    app.tick(start + Duration::from_secs(59));
    assert_eq!(app.overlays.conn.modal, Some(Modal::Token));
    assert!(app.overlays.conn.revealed.is_some());
    app.tick(start + Duration::from_secs(60));
    assert!(app.overlays.conn.modal.is_none());
    assert!(app.overlays.conn.revealed.is_none());
}

/// A click on the token copies it; a click on the box's label asks for nothing.
#[test]
fn a_click_on_the_token_copies_it() {
    let app = token_box_app();
    let (column, row) = find_at(&app, 80, 24, FAKE_TOKEN);
    let copy = Some(MouseAction::Intent(Intent::CopyToken));
    assert_eq!(clicked(&app, column, row), copy);
    assert_eq!(clicked(&app, column + 20, row), copy);
    assert_eq!(clicked(&app, column - 1, row), None, "the margin before it");
    let end = column + u16::try_from(FAKE_TOKEN.len()).unwrap();
    assert_eq!(clicked(&app, end, row), None, "just past its end");
    let (title, title_row) = find_at(&app, 80, 24, "MCP token");
    assert_eq!(clicked(&app, title, title_row), None);
}

/// A rotated token (another one read) closes the box; so does the daemon saying it uses
/// another token than this session's.
#[test]
fn a_token_change_closes_the_box() {
    let mut app = token_box_app();
    app.overlays.conn.take_token(Ok(None));
    assert!(app.overlays.conn.modal.is_none());
    assert!(app.overlays.conn.revealed.is_none());

    let mut app = token_box_app();
    let theirs = svolder_ipc::TokenInfo {
        kind: svolder_ipc::TokenKind::Environment,
        path: None,
        tail: "9xZ".into(),
    };
    let mut info = daemon_with_config(None);
    info.mcp_token = Some(theirs);
    app.overlays.conn.daemon_said(
        &info,
        std::path::Path::new("/home/user/.config/svolder/config.toml"),
    );
    assert!(app.overlays.conn.modal.is_none());
    assert!(app.overlays.conn.revealed.is_none());
}

/// A refused key warns as on the row and opens no box.
#[test]
fn refusals_open_no_box() {
    // a daemon reading another file
    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    app.overlays.conn.elsewhere = Some("/other/config.toml".into());
    for code in [KeyCode::Char('v'), KeyCode::Enter] {
        assert!(press(&mut app, code).is_empty());
        assert!(app.overlays.conn.modal.is_none());
    }
    // no token yet
    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    app.overlays.conn.token = TokenView::Missing;
    for code in [KeyCode::Char('v'), KeyCode::Enter] {
        assert!(press(&mut app, code).is_empty());
        assert!(app.overlays.conn.modal.is_none());
    }
}

/// The right column never prints the token: not while it is held, and not with the box shut.
#[test]
fn the_right_column_does_not_print_the_token() {
    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    app.overlays
        .conn
        .reveal(FAKE_TOKEN.to_string(), app.now + connections::REVEAL_FOR);
    assert!(
        !right_words(&app).replace(' ', "").contains(FAKE_TOKEN),
        "{}",
        right_words(&app)
    );
    assert!(!shown(&app).contains("fake-token"), "{}", shown(&app));
}

/// The box's text is the golden one.
#[test]
fn the_token_box_is_the_golden_box() {
    let app = token_box_app();
    let mut out = String::new();
    for (width, height) in [(80, 24), (120, 30)] {
        out.push_str(&format!("=== {width} columns\n"));
        out.push_str(&text(&frame(&app, width, height)).join("\n"));
        out.push('\n');
    }
    assert_eq!(out, include_str!("../token_box.golden"));
}

/// Only `v` and Enter open the box: Space and the arrows on the token row open nothing and ask
/// for no reveal.
#[test]
fn space_and_the_arrows_open_no_box() {
    for code in [KeyCode::Char(' '), KeyCode::Left, KeyCode::Right] {
        let mut app = connect_app();
        select(&mut app, Setting::McpToken);
        assert!(press(&mut app, code).is_empty(), "{code:?}");
        assert!(app.overlays.conn.modal.is_none(), "{code:?}");
    }
}

/// A token with wide characters is cut by cells, so no line is wider than the box's room and the
/// pieces join to the token.
#[test]
fn a_wide_token_is_cut_by_cells() {
    let wide = format!("fake-{}", "漢".repeat(40));
    let mut app = token_box_app();
    app.overlays
        .conn
        .reveal(wide.clone(), app.now + connections::REVEAL_FOR);
    let rows = box_rows(&app);
    let border = rows
        .iter()
        .find_map(|r| r.find("┌ MCP token").map(|b| r[..b].chars().count()))
        .unwrap();
    let mut joined = String::new();
    for row in &rows {
        let cells: Vec<char> = row.chars().collect();
        if cells.get(border) != Some(&'│') || cells.get(border + 1) != Some(&' ') {
            continue;
        }
        // a wide character fills two cells and the buffer's text shows the second as a space
        let part: String = cells[border + 2..]
            .iter()
            .take_while(|c| **c != '│')
            .filter(|c| **c != ' ')
            .collect();
        if !part.is_empty() && wide[joined.len()..].starts_with(&part) {
            joined.push_str(&part);
        }
    }
    assert_eq!(joined, wide, "{rows:#?}");
}
