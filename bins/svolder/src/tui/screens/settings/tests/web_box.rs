// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Web UI window of the Connect category: its keys, QR codes, links and tokens.

use super::*;

/// The Connect category opened on the Web UI row of a daemon whose web UI is off, with the box
/// opened by Enter.
fn web_box_app() -> App {
    let mut app = connect_app();
    app.overlays.conn.daemon_up = Some(true);
    app.overlays.conn.web.listen = "127.0.0.1:8738".parse().unwrap();
    app.overlays.conn.web_status = Some(svolder_ipc::WebStatus::Off);
    select(&mut app, Setting::WebUi);
    let asked = press(&mut app, KeyCode::Enter);
    assert!(asked.is_empty(), "a link was asked of a web UI that is off");
    assert!(matches!(app.overlays.conn.modal, Some(Modal::Web(_))));
    app
}

/// The box's title is its name alone: the web UI is not called experimental there.
#[test]
fn the_web_boxs_title_does_not_say_experimental() {
    let mut app = web_box_app();
    let rows = shown(&app);
    assert!(rows.contains("Web UI"), "{rows}");
    assert!(!rows.contains("experimental"), "{rows}");
    let _ = press(&mut app, KeyCode::Esc);
}

/// Enter on Web UI writes `enabled`, ←→ on Reach writes the listen address with its port, `l`
/// and `p` ask for codes, `c` copies a link on show, `r` then `y` replaces the token, and Esc
/// closes the box and forgets its codes.
#[test]
fn the_web_boxs_keys_ask_for_what_their_rows_change() {
    let mut app = web_box_app();
    assert!(shown(&app).contains("Web UI"), "{}", shown(&app));
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Connection(ConnChange::WebEnabled(true))]
    );
    let _ = press(&mut app, KeyCode::Down);
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Connection(ConnChange::WebListen(
            "0.0.0.0:8738".parse().unwrap()
        ))]
    );
    assert_eq!(press(&mut app, KeyCode::Char('l')), vec![Intent::WebLink]);
    assert_eq!(press(&mut app, KeyCode::Char('p')), vec![Intent::WebPair]);
    assert!(
        press(&mut app, KeyCode::Char('c')).is_empty(),
        "a copy with no link on show"
    );
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link = Some(crate::tui::boxes::web_box::Code {
            text: "http://192.168.1.20:8738/#login=abc".into(),
            until: std::time::Instant::now() + std::time::Duration::from_secs(60),
        });
    }
    assert_eq!(
        press(&mut app, KeyCode::Char('c')),
        vec![Intent::CopyWebLink]
    );
    assert!(press(&mut app, KeyCode::Char('r')).is_empty());
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        vec![Intent::RotateWebToken]
    );
    let _ = press(&mut app, KeyCode::Esc);
    assert!(app.overlays.conn.modal.is_none());
}

/// The Guest view row reads `off` or `read-only` from `[web] guest`; `g` anywhere in the box, and
/// Enter or an arrow on the row, ask to turn it the other way, refused as every row's write is
/// when the daemon reads another file.
#[test]
fn the_guest_row_and_g_turn_the_guest_view() {
    let mut app = web_box_app();
    assert!(shown(&app).contains("Guest view    off"), "{}", shown(&app));
    let on = vec![Intent::Connection(ConnChange::WebGuest(true))];
    assert_eq!(press(&mut app, KeyCode::Char('g')), on);
    // Down twice from Web UI is Guest view: Reach, then the row.
    let _ = press(&mut app, KeyCode::Down);
    let _ = press(&mut app, KeyCode::Down);
    for key in [KeyCode::Enter, KeyCode::Left, KeyCode::Right] {
        assert_eq!(press(&mut app, key), on, "{key:?}");
    }
    app.overlays.conn.web.guest = svolder_ipc::config::WebGuest::Read;
    assert!(
        shown(&app).contains("Guest view    read-only"),
        "{}",
        shown(&app)
    );
    let off = vec![Intent::Connection(ConnChange::WebGuest(false))];
    assert_eq!(press(&mut app, KeyCode::Char('g')), off);
    assert_eq!(press(&mut app, KeyCode::Enter), off);
    app.overlays.conn.elsewhere = Some("/elsewhere/config.toml".into());
    assert!(press(&mut app, KeyCode::Char('g')).is_empty());
    assert!(app.toast().is_some(), "no word about the other file");
}

/// The two key lines hold `g` and still fit the box inside 80 columns.
#[test]
fn the_web_boxs_key_lines_name_g_and_fit() {
    let app = web_box_app();
    let rows = text(&frame(&app, 80, 24));
    let keys: Vec<&String> = rows
        .iter()
        .filter(|r| r.contains("rows ·") || r.contains("closes"))
        .collect();
    assert_eq!(keys.len(), 2, "{rows:#?}");
    assert!(keys.iter().any(|r| r.contains("g guest")), "{keys:?}");
    for row in keys {
        assert_eq!(row.chars().count(), 80, "{row}");
        assert!(row.trim_end().ends_with('│'), "{row}");
    }
}

/// Enter on the Link row asks for a link as `l` does, and on the Pairing code row for a code as
/// `p` does.
#[test]
fn enter_on_the_link_and_pairing_rows_asks_as_l_and_p_do() {
    let mut app = web_box_app();
    let _ = press(&mut app, KeyCode::Down);
    let _ = press(&mut app, KeyCode::Down);
    let _ = press(&mut app, KeyCode::Down);
    assert_eq!(press(&mut app, KeyCode::Enter), vec![Intent::WebLink]);
    let _ = press(&mut app, KeyCode::Down);
    assert_eq!(press(&mut app, KeyCode::Enter), vec![Intent::WebPair]);
    let _ = press(&mut app, KeyCode::Down);
    assert!(
        press(&mut app, KeyCode::Enter).is_empty(),
        "Enter on the Token row asked for something"
    );
    // With no daemon, Enter says what `l` says and asks for nothing.
    app.overlays.conn.daemon_up = Some(false);
    let _ = press(&mut app, KeyCode::Up);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
}

/// The question before the web token is replaced shows its answers, `y` among them, in the
/// style of the quit question; Enter and `y` answer Yes, `n` and Esc answer No.
#[test]
fn the_web_token_question_shows_y_and_enter_answers_yes() {
    let mut app = web_box_app();
    let _ = press(&mut app, KeyCode::Char('r'));
    let words = shown(&app);
    assert!(words.contains("[Y/n]"), "{words}");
    assert!(words.contains("Enter/y replaces"), "{words}");
    assert!(words.contains("Esc/n/q cancels"), "{words}");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::RotateWebToken]
    );
    for answer in [KeyCode::Char('n'), KeyCode::Esc] {
        let _ = press(&mut app, KeyCode::Char('r'));
        assert!(press(&mut app, answer).is_empty());
        assert!(matches!(
            &app.overlays.conn.modal,
            Some(Modal::Web(web)) if !web.rotating
        ));
    }
    let _ = press(&mut app, KeyCode::Char('r'));
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        vec![Intent::RotateWebToken]
    );
}

/// Opening the box on a web UI that listens asks for a link at once, for its QR code.
#[test]
fn opening_the_box_on_a_listening_web_ui_asks_for_a_link() {
    let mut app = connect_app();
    app.overlays.conn.daemon_up = Some(true);
    app.overlays.conn.web_status = Some(svolder_ipc::WebStatus::Listening {
        listen: "0.0.0.0:8738".parse().unwrap(),
        auth: svolder_ipc::config::WebAuth::Token,
        writes: true,
        guest: svolder_ipc::config::WebGuest::None,
    });
    select(&mut app, Setting::WebUi);
    assert_eq!(press(&mut app, KeyCode::Enter), vec![Intent::WebLink]);
}

/// On a direct session the code rows say the daemon serves the web UI, and `l` and `p` ask for
/// nothing.
#[test]
fn a_direct_session_asks_the_web_box_for_no_codes() {
    let mut app = web_box_app();
    app.overlays.conn.daemon_up = Some(false);
    assert!(press(&mut app, KeyCode::Char('l')).is_empty());
    assert!(press(&mut app, KeyCode::Char('p')).is_empty());
    assert!(
        shown(&app).contains("the daemon serves the web UI"),
        "{}",
        shown(&app)
    );
}

/// `q` in the Web UI box closes it as `Esc` does: no quit question, the Settings box where it was.
#[test]
fn q_closes_the_web_ui_window_as_esc_does() {
    let mut app = web_box_app();
    let at = app.overlays.settings;
    assert!(press(&mut app, KeyCode::Char('q')).is_empty());
    assert!(app.overlays.conn.modal.is_none(), "the box stayed open");
    assert!(!app.overlays.quitting, "q asked to quit");
    assert_eq!(app.overlays.settings, at, "the Settings box moved");
}

/// The same while a link is on show.
#[test]
fn q_in_the_web_ui_window_with_a_link_showing_closes_it() {
    let mut app = web_box_with_link("0.0.0.0:8738");
    let at = app.overlays.settings;
    assert!(press(&mut app, KeyCode::Char('q')).is_empty());
    assert!(app.overlays.conn.modal.is_none(), "the box stayed open");
    assert!(!app.overlays.quitting, "q asked to quit");
    assert_eq!(app.overlays.settings, at, "the Settings box moved");
}

/// The same while a pairing code is on show.
#[test]
fn q_in_the_web_ui_window_with_a_pairing_code_showing_closes_it() {
    let mut app = web_box_app();
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.pair = Some(crate::tui::boxes::web_box::Code {
            text: "482913".into(),
            until: app.now + std::time::Duration::from_secs(90),
        });
    }
    let at = app.overlays.settings;
    assert!(press(&mut app, KeyCode::Char('q')).is_empty());
    assert!(app.overlays.conn.modal.is_none(), "the box stayed open");
    assert!(!app.overlays.quitting, "q asked to quit");
    assert_eq!(app.overlays.settings, at, "the Settings box moved");
}

/// `q` declines the question before the web token is replaced, as `Esc` does: the question
/// goes, the Web UI box stays, no quit question.
#[test]
fn q_declines_the_web_token_question_as_esc_does() {
    let mut app = web_box_app();
    for no in [KeyCode::Char('q'), KeyCode::Esc] {
        let _ = press(&mut app, KeyCode::Char('r'));
        assert!(
            shown(&app).contains("Replace the web token?"),
            "{}",
            shown(&app)
        );
        assert!(press(&mut app, no).is_empty());
        assert!(
            !shown(&app).contains("Replace the web token?"),
            "{no:?} did not decline"
        );
        assert!(matches!(app.overlays.conn.modal, Some(Modal::Web(_))));
        assert!(!app.overlays.quitting, "{no:?} asked to quit");
    }
}

/// `q` declines the question before the MCP token is replaced, as `Esc` does, and never asks to
/// quit.
#[test]
fn q_declines_the_mcp_token_question_as_esc_does() {
    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    assert!(press(&mut app, KeyCode::Char('R')).is_empty());
    assert_eq!(app.overlays.conn.modal, Some(Modal::Rotating));
    assert!(press(&mut app, KeyCode::Char('q')).is_empty());
    assert!(app.overlays.conn.modal.is_none(), "q did not decline");
    assert!(!app.overlays.quitting, "q asked to quit");
    assert!(app.overlays.settings.is_some(), "the Settings box closed");
}

/// With the Web UI box closed, `q` on Connect still asks to quit.
#[test]
fn q_on_settings_with_no_window_open_still_asks_to_quit() {
    let mut app = connect_app();
    assert!(app.overlays.conn.modal.is_none());
    assert!(press(&mut app, KeyCode::Char('q')).is_empty());
    assert!(app.overlays.quitting && app.overlays.settings.is_some());
}

/// The Web UI box with a link on show, on a listen address `listen`.
fn web_box_with_link(listen: &str) -> App {
    let mut app = web_box_app();
    app.overlays.conn.web.listen = listen.parse().unwrap();
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link = Some(crate::tui::boxes::web_box::Code {
            text: "http://192.168.1.20:8738/#login=abc".into(),
            until: app.now + std::time::Duration::from_secs(90),
        });
    }
    app
}

/// How many cells of `buffer` are painted white, which only the QR code is.
fn white_cells(buffer: &Buffer) -> usize {
    buffer
        .content()
        .iter()
        .filter(|cell| cell.style().bg == Some(ratatui::style::Color::White))
        .count()
}

/// With room, the box draws the link and its QR code black on white under its rows; without,
/// it says to widen the terminal; on a loopback listen it says a phone cannot reach it.
#[test]
fn the_web_box_draws_the_links_qr_code_where_it_has_room() {
    let app = web_box_with_link("0.0.0.0:8738");
    let big = frame(&app, 120, 60);
    let rows = text(&big).join("\n");
    assert!(rows.contains("Web UI"), "{rows}");
    assert!(
        rows.contains("http://192.168.1.20:8738/#login=abc"),
        "{rows}"
    );
    assert!(rows.contains('█'), "{rows}");
    assert!(white_cells(&big) > 100, "the QR code is not areas white");
    let small = text(&frame(&app, 80, 24)).join("\n");
    assert!(
        small.contains("widen the terminal to see the QR code"),
        "{small}"
    );
    assert_eq!(white_cells(&frame(&app, 80, 24)), 0);

    let loopback = web_box_with_link("127.0.0.1:8738");
    let shown = text(&frame(&loopback, 120, 60)).join("\n");
    let said = "a phone cannot reach this computer's loopback: choose your";
    assert!(shown.contains(said), "{shown}");
    assert_eq!(white_cells(&frame(&loopback, 120, 60)), 0);
}

/// The Web UI box on the network at `192.168.1.20`, with the guest view `guest`, a sign-in link on
/// show when `link`; the guest page is known when the guest view is on.
fn web_box_guest(guest: bool, link: bool) -> App {
    let mut app = web_box_app();
    app.overlays.conn.web.listen = "0.0.0.0:8738".parse().unwrap();
    app.overlays.conn.web.enabled = true;
    app.overlays.conn.web.guest = match guest {
        true => svolder_ipc::config::WebGuest::Read,
        false => svolder_ipc::config::WebGuest::None,
    };
    app.overlays.conn.web_status = Some(svolder_ipc::WebStatus::Listening {
        listen: "0.0.0.0:8738".parse().unwrap(),
        auth: svolder_ipc::config::WebAuth::Token,
        writes: true,
        guest: app.overlays.conn.web.guest,
    });
    let now = app.now;
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        if link {
            web.link = Some(crate::tui::boxes::web_box::Code {
                text: "http://192.168.1.20:8738/#login=abcdefghijklmnop".into(),
                until: now + std::time::Duration::from_secs(90),
            });
        }
        let listen = "0.0.0.0:8738".parse().unwrap();
        web.page = Some(Box::new(crate::tui::boxes::web_box::Page {
            url: "http://192.168.1.20:8738/".into(),
            local: false,
            made_for: (listen, listen),
        }));
    }
    app
}

/// The Web UI box as it draws at `width`×`height`: its rows from the top border to the bottom
/// one, trailing blanks off, the box's own columns only.
fn web_box_text(app: &App, width: u16, height: u16) -> String {
    let rows = text(&frame(app, width, height));
    let top = rows
        .iter()
        .position(|row| row.contains("┌ Web UI "))
        .expect("the box is drawn");
    let left = rows[top].chars().position(|c| c == '┌').unwrap();
    let right = rows[top].chars().position(|c| c == '┐').unwrap();
    let mut out = Vec::new();
    for row in &rows[top..] {
        let cells: String = row.chars().skip(left).take(right + 1 - left).collect();
        out.push(cells.trim_end().to_string());
        if cells.starts_with('└') {
            break;
        }
    }
    out.join("\n")
}

/// The box at 80, 120 and 200 columns with the guest view off, on alone and on beside a sign-in
/// link, and on a loopback listen, as `web_box.golden` holds them. A change to the box shows in
/// the golden as a diff.
fn web_box_golden_text() -> String {
    let mut out = String::new();
    for (width, height) in [(80, 70), (120, 46), (200, 46)] {
        for (name, app) in [
            ("guest off, sign-in link", web_box_guest(false, true)),
            ("guest on alone", web_box_guest(true, false)),
            ("guest on, sign-in link", web_box_guest(true, true)),
        ] {
            out.push_str(&format!("=== {width} columns: {name}\n"));
            out.push_str(&web_box_text(&app, width, height));
            out.push('\n');
        }
    }
    let mut loopback = web_box_guest(true, true);
    loopback.overlays.conn.web.listen = "127.0.0.1:8738".parse().unwrap();
    out.push_str("=== 120 columns: loopback\n");
    out.push_str(&web_box_text(&loopback, 120, 46));
    out.push('\n');
    out
}

#[test]
fn the_web_box_is_the_golden_box() {
    assert_eq!(web_box_golden_text(), include_str!("../web_box.golden"));
}

/// Both codes side by side where the box can be that wide, stacked where it cannot, the words
/// that say to widen where neither fits; each has its caption; the guest code shows with no
/// sign-in link, and the sign-in code is not drawn without one; a guest view that is off draws
/// no guest code.
#[test]
fn the_guest_qr_sits_beside_or_under_the_sign_in_qr() {
    let both = web_box_guest(true, true);
    let wide = text(&frame(&both, 200, 46)).join("\n");
    assert!(wide.contains("sign in"), "{wide}");
    assert!(
        wide.contains("guest: read-only, anyone on your network"),
        "{wide}"
    );
    let caption_row =
        |rows: &str, needle: &str| rows.lines().position(|row| row.contains(needle)).unwrap();
    assert_eq!(
        caption_row(&wide, "sign in"),
        caption_row(&wide, "guest: read-only"),
        "side by side: one caption row"
    );
    let stacked = text(&frame(&both, 80, 70)).join("\n");
    assert!(
        caption_row(&stacked, "sign in") < caption_row(&stacked, "guest: read-only"),
        "{stacked}"
    );
    let words = text(&frame(&both, 80, 30)).join("\n");
    assert!(
        words.contains("widen the terminal to see the QR code"),
        "{words}"
    );
    assert_eq!(white_cells(&frame(&both, 80, 30)), 0);

    let alone = text(&frame(&web_box_guest(true, false), 120, 46)).join("\n");
    assert!(alone.contains("guest: read-only"), "{alone}");
    assert!(!alone.contains("sign in"), "{alone}");
    let off = text(&frame(&web_box_guest(false, true), 120, 46)).join("\n");
    assert!(
        off.contains("sign in") && !off.contains("guest: read-only"),
        "{off}"
    );
    // The web server off: no page to open.
    let mut down = web_box_guest(true, false);
    down.overlays.conn.web_status = Some(svolder_ipc::WebStatus::Off);
    assert_eq!(white_cells(&frame(&down, 120, 46)), 0);
}

/// The guest code follows the running daemon's status, not the file: a file edited to `read`
/// with no reload draws none, a daemon serving guests while the file says `none` draws one, and
/// under `auth = "none"` there is no code and one line says why.
#[test]
fn the_guest_qr_follows_the_daemons_status_and_not_the_file() {
    let guest_status = |app: &mut App, guest, auth| {
        app.overlays.conn.web_status = Some(svolder_ipc::WebStatus::Listening {
            listen: "0.0.0.0:8738".parse().unwrap(),
            auth,
            writes: true,
            guest,
        });
    };
    use svolder_ipc::config::{WebAuth, WebGuest};
    let mut edited = web_box_guest(true, false);
    guest_status(&mut edited, WebGuest::None, WebAuth::Token);
    let rows = text(&frame(&edited, 120, 46)).join("\n");
    assert!(!rows.contains("guest: read-only"), "{rows}");
    assert_eq!(white_cells(&frame(&edited, 120, 46)), 0);
    let mut serving = web_box_guest(false, false);
    guest_status(&mut serving, WebGuest::Read, WebAuth::Token);
    let rows = text(&frame(&serving, 120, 46)).join("\n");
    assert!(rows.contains("guest: read-only"), "{rows}");
    let mut open = web_box_guest(true, true);
    guest_status(&mut open, WebGuest::Read, WebAuth::None);
    let rows = text(&frame(&open, 120, 46)).join("\n");
    assert!(!rows.contains("guest: read-only"), "{rows}");
    assert!(rows.contains("sign in"), "the sign-in code stays: {rows}");
    assert!(rows.contains("no guest view: auth is none"), "{rows}");
}

/// The words for a loopback listen or a base no phone reaches come once, not once for each
/// code.
#[test]
fn the_reach_words_come_once_for_both_codes() {
    let mut loopback = web_box_guest(true, true);
    loopback.overlays.conn.web.listen = "127.0.0.1:8738".parse().unwrap();
    let rows = text(&frame(&loopback, 120, 46)).join("\n");
    assert_eq!(rows.matches("a phone cannot reach").count(), 1, "{rows}");
    assert_eq!(white_cells(&frame(&loopback, 120, 46)), 0);
    // One line of words does not widen the box for two codes it does not draw.
    assert!(
        web_box_text(&loopback, 200, 46)
            .lines()
            .next()
            .unwrap()
            .chars()
            .count()
            <= 64
    );
    let mut local = web_box_guest(true, true);
    if let Some(Modal::Web(web)) = local.overlays.conn.modal.as_mut() {
        web.link_local = true;
        web.page.as_mut().unwrap().local = true;
    }
    let rows = text(&frame(&local, 120, 46)).join("\n");
    assert_eq!(
        rows.matches("a phone cannot open this link").count(),
        1,
        "{rows}"
    );
    assert_eq!(white_cells(&frame(&local, 120, 46)), 0);
}

/// A pairing code shows in two threes with its countdown, and with it the way round a code the
/// network used up.
#[test]
fn the_web_box_shows_a_pairing_code_and_its_fallback() {
    let mut app = web_box_app();
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.pair = Some(crate::tui::boxes::web_box::Code {
            text: "482913".into(),
            until: app.now + std::time::Duration::from_secs(118),
        });
    }
    let rows = shown(&app);
    assert!(rows.contains("482 913 · 1:58 left"), "{rows}");
    assert!(rows.contains("the link is the fallback"), "{rows}");
}

/// Replacing the web token asks first; a token svolder does not own is refused, naming where it
/// comes from, and one it does own is replaced on `y`.
#[test]
fn the_web_token_is_replaced_after_a_question_and_only_when_svolder_owns_it() {
    let mut app = web_box_app();
    app.overlays.conn.web_token = TokenView::Present {
        masked: connections::mask(FAKE_TOKEN),
        source: TokenSource::Env("SVOLDER_WEB_TOKEN"),
    };
    let _ = press(&mut app, KeyCode::Char('r'));
    assert!(
        shown(&app).contains("Replace the web token?"),
        "{}",
        shown(&app)
    );
    assert!(press(&mut app, KeyCode::Char('y')).is_empty());
    let said = app
        .toast()
        .map(|toast| toast.text.clone())
        .unwrap_or_default();
    assert!(said.contains("SVOLDER_WEB_TOKEN"), "{said}");
    app.overlays.conn.web_token = TokenView::Present {
        masked: connections::mask(FAKE_TOKEN),
        source: TokenSource::Generated("/home/user/.local/state/svolder/web-token".into()),
    };
    let _ = press(&mut app, KeyCode::Char('r'));
    assert!(press(&mut app, KeyCode::Char('n')).is_empty());
    assert!(!shown(&app).contains("Replace the web token?"));
    let _ = press(&mut app, KeyCode::Char('r'));
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        vec![Intent::RotateWebToken]
    );
}

/// A link on this computer's own base, because none a phone reaches was found, draws the words
/// that say so instead of a QR code a phone cannot open.
#[test]
fn a_link_on_the_fallback_base_draws_words_not_a_qr_code() {
    let mut app = web_box_with_link("0.0.0.0:8738");
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link_local = true;
    }
    let big = frame(&app, 120, 60);
    let rows = text(&big).join("\n");
    assert!(rows.contains("a phone cannot open this link"), "{rows}");
    assert_eq!(white_cells(&big), 0, "a QR code was drawn");
}

/// A link that replaced one a browser used says so under its hint; a link that did not says
/// nothing of the kind.
#[test]
fn a_replaced_link_says_the_last_one_was_used() {
    let mut app = web_box_with_link("0.0.0.0:8738");
    let rows = text(&frame(&app, 120, 60)).join("\n");
    assert!(!rows.contains("was used"), "{rows}");
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link_replaced = true;
    }
    let rows = text(&frame(&app, 120, 60)).join("\n");
    assert!(
        rows.contains("the last link no longer works; a fresh one is shown"),
        "{rows}"
    );
}

/// The cell `(column, row)` where `needle` starts in the frame `app` draws at `width`×`height`.
fn find_at(app: &App, width: u16, height: u16, needle: &str) -> (u16, u16) {
    for (row, line) in text(&frame(app, width, height)).iter().enumerate() {
        if let Some(byte) = line.find(needle) {
            let column = line[..byte].chars().count();
            return (u16::try_from(column).unwrap(), u16::try_from(row).unwrap());
        }
    }
    panic!("{needle:?} is not drawn");
}

/// A left click on the link's text, on every line of it where it wraps, asks to open it; on its
/// label, on another row, or with no link on show, it asks for nothing.
#[test]
fn a_click_on_the_shown_link_asks_to_open_it() {
    let mut app = web_box_with_link("0.0.0.0:8738");
    let (column, row) = find_at(&app, 80, 24, "http://192.168.1.20");
    let open = Some(MouseAction::Intent(Intent::OpenWebLink));
    assert_eq!(clicked(&app, column, row), open);
    assert_eq!(clicked(&app, column + 10, row), open);
    let (label, _) = find_at(&app, 80, 24, "State");
    assert_eq!(
        clicked(&app, label, row - 3),
        None,
        "a click on another row"
    );
    // A link that wraps: both lines are the link.
    let long = format!("http://192.168.1.20:8738/#login={}", "a".repeat(70));
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link.as_mut().unwrap().text = long;
    }
    let (column, first) = find_at(&app, 80, 24, "http://192.168.1.20");
    assert_eq!(clicked(&app, column, first), open);
    assert_eq!(clicked(&app, column, first + 1), open, "the wrapped line");
    // No link: nothing on the row to click.
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link = None;
    }
    assert_eq!(clicked(&app, column, first), None);
}

/// The Web UI box with a link of `tail` letters after the usual base: 32 characters and `tail`.
fn web_box_with_long_link(tail: usize) -> (App, String) {
    let mut app = web_box_with_link("0.0.0.0:8738");
    let link = format!("http://192.168.1.20:8738/#login={}", "a".repeat(tail));
    if let Some(Modal::Web(web)) = app.overlays.conn.modal.as_mut() {
        web.link.as_mut().unwrap().text = link.clone();
    }
    (app, link)
}

/// The link sits on lines of its own, as wide as the box's text area less a cell of margin each
/// side, with no timer, label or border inside its text; the timer is on the label's line.
#[test]
fn the_link_is_on_lines_of_its_own_at_the_full_width_of_the_box() {
    let (app, link) = web_box_with_long_link(70);
    let rows = text(&frame(&app, 80, 24));
    let (column, first) = find_at(&app, 80, 24, "http://192.168.1.20");
    let cells = |row: u16| rows[usize::from(row)].chars().collect::<Vec<_>>();
    let head = cells(first);
    let column = usize::from(column);
    let width = 60;
    assert_eq!(head[column - 1], ' ', "a margin before the link");
    assert_eq!(
        head[column..column + width].iter().collect::<String>(),
        link[..width]
    );
    assert_eq!(head[column + width], ' ', "a margin after the first line");
    assert_eq!(
        head[column + width + 1],
        '│',
        "the border follows the margin"
    );
    let tail = cells(first + 1);
    let rest: String = tail[column..column + link.len() - width].iter().collect();
    assert_eq!(
        rest,
        link[width..],
        "the wrapped line is the rest of the link"
    );
    assert!(
        tail[column + link.len() - width..column + width + 1]
            .iter()
            .all(|c| *c == ' '),
        "nothing after the link's last piece"
    );
    let label = cells(first - 1).iter().collect::<String>();
    assert!(
        label.contains("Link") && label.contains("1:30 left"),
        "{label}"
    );
}

/// Under the pointer every line of a wrapped link underlines, and none does without it.
#[test]
fn every_line_of_a_wrapped_link_underlines_on_hover() {
    let (mut app, _) = web_box_with_long_link(70);
    let (column, first) = find_at(&app, 80, 24, "http://192.168.1.20");
    let underlined = |app: &App, row: u16, x: u16| {
        frame(app, 80, 24)[(x, row)]
            .modifier
            .contains(ratatui::style::Modifier::UNDERLINED)
    };
    for row in [first, first + 1] {
        assert!(
            !underlined(&app, row, column),
            "underlined with no pointer on it"
        );
    }
    app.pointer.over_link = true;
    for row in [first, first + 1] {
        assert!(underlined(&app, row, column), "row {row} did not underline");
        assert!(
            underlined(&app, row, column + 5),
            "row {row} did not underline"
        );
    }
    assert!(
        !underlined(&app, first - 1, column),
        "the label's line underlined with the link"
    );
}

/// A click on the last cell of each full-width line takes the link, and the cell after does not.
#[test]
fn a_click_on_any_cell_of_a_wrapped_links_lines_opens_it() {
    let (app, _) = web_box_with_long_link(70);
    let (column, first) = find_at(&app, 80, 24, "http://192.168.1.20");
    let open = Some(MouseAction::Intent(Intent::OpenWebLink));
    assert_eq!(clicked(&app, column + 59, first), open);
    assert_eq!(clicked(&app, column + 60, first), None);
    assert_eq!(clicked(&app, column + 41, first + 1), open);
    assert_eq!(clicked(&app, column + 42, first + 1), None);
}

/// Under a link the box says what the terminal's own link handling does with it.
#[test]
fn the_box_says_how_to_use_the_link_as_a_terminal_link() {
    let (app, _) = web_box_with_long_link(10);
    let rows = shown(&app);
    assert!(rows.contains("Ctrl+click"), "{rows}");
    assert!(rows.contains("Shift+right-click copies"), "{rows}");
}
