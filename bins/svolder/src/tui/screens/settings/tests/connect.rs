// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Connect category: the token rows, the typed rows, read-only rows under a foreign daemon and
//! the AutoEq refresh row.

use super::*;
/// The Connect category at 80×24 on its first row, with the token masked.
const CONNECT_MOCKUP: [&str; 22] = [
    "┌ Settings ─ tab→ ─ ¹Look ─ ²Effects ─ ³Behaviour ─ ⁴Host audio ─┤ ⁵Connect ├──┐",
    "│ ▶ Web UI             …                    │                                  │",
    "│   MCP server       ‹ off | on ›           │                                  │",
    "│   MCP listen         loopback:8737        │  The web UI serves this          │",
    "│   MCP writes       ‹ read-only | writes › │  screen's controls to a browser  │",
    "│   MCP token          ••••••3fQ            │  on this computer or your        │",
    "│   MCP status         listening            │  network. Enter opens its box:   │",
    "│   MQTT client      ‹ off | on ›           │  on and off, how far it          │",
    "│   MQTT broker host   not set              │  reaches, a link or a pairing    │",
    "│   MQTT broker port   1883                 │  code to sign a browser in, and  │",
    "│   MQTT commands    ‹ read-only | allowed ›│  its token.                      │",
    "│   MQTT username      not set              │                                  │",
    "│   MQTT password      not set              │  [web] enabled = false, listen   │",
    "│   MQTT status        off                  │  = \"127.0.0.1:8738\"              │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "│                                           │                                  │",
    "└ ~/.config/svolder/config.toml ──────────────────────────────────── esc close ┘",
];

#[test]
fn connect_at_80x24_draws_the_mockup() {
    assert_eq!(box_rows(&connect_app()), CONNECT_MOCKUP);
}

/// The token row is masked in the row and in the right column, and the right column says where
/// the token comes from and which keys act on it.
#[test]
fn the_token_row_is_masked_and_names_its_source_and_keys() {
    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    let all = shown(&app);
    assert!(all.contains("••••••3fQ"), "the masked token is not drawn");
    assert!(
        !all.contains("fake-token"),
        "the box draws the token while it is hidden"
    );
    assert!(all.contains("from svolder's own file"), "{all}");
    assert!(all.contains("v or Enter opens it"), "{all}");
}

/// A token of fewer than 16 characters is masked without a tail.
#[test]
fn a_short_token_shows_no_tail() {
    assert_eq!(connections::mask("shorttoken"), "••••••");
    assert_eq!(connections::mask(FAKE_TOKEN), "••••••3fQ");
}

/// The token is drawn in its box while it is revealed and not a frame past its minute, whatever
/// the loop has done since: the draw reads `app.now`, and a tick drops the text and the box.
#[test]
fn a_revealed_token_is_drawn_for_a_minute_on_app_now() {
    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    let start = app.now;
    press(&mut app, KeyCode::Char('v'));
    app.overlays
        .conn
        .reveal(FAKE_TOKEN.to_string(), start + connections::REVEAL_FOR);
    assert!(shown(&app).contains(FAKE_TOKEN), "the token is not drawn");
    assert_eq!(Setting::McpToken.value(&app), "(revealed)");

    app.now = start + Duration::from_secs(59);
    assert!(shown(&app).contains(FAKE_TOKEN), "gone before its minute");

    app.now = start + Duration::from_secs(60);
    assert!(
        !shown(&app).contains("fake-token"),
        "the token is still drawn at a minute"
    );

    app.now = start;
    app.tick(start + Duration::from_secs(61));
    assert!(
        app.overlays.conn.revealed.is_none(),
        "a tick keeps the text past its time"
    );
    app.now = start;
    assert!(
        !shown(&app).contains("fake-token") && app.overlays.conn.modal.is_none(),
        "the token is drawn after the tick dropped it"
    );
}

/// A `Debug` dump of the category never carries the token.
#[test]
fn the_category_never_prints_the_token() {
    let mut app = connect_app();
    app.overlays
        .conn
        .reveal(FAKE_TOKEN.to_string(), app.now + connections::REVEAL_FOR);
    let dump = format!("{:?}", app.overlays.conn);
    assert!(
        !dump.contains("fake-token"),
        "a Debug dump carries the token"
    );
}

/// The status rows say what the daemon answers, in a word on the row and its own line in the
/// right column.
#[test]
fn the_status_rows_say_what_the_daemon_answers() {
    let mut app = connect_app();
    select(&mut app, Setting::McpStatus);
    assert!(right_words(&app).contains("mcp: listening on 127.0.0.1:8737 (token, read-only)"));
    app.overlays.conn.mcp_status = Some(McpStatus::Failed {
        listen: "127.0.0.1:8737".parse().unwrap(),
        error: "address in use".into(),
    });
    assert_eq!(Setting::McpStatus.value(&app), "failed");
    assert!(right_words(&app).contains("cannot listen on 127.0.0.1:8737: address in use"));

    app.overlays.conn.mqtt_status = Some(MqttStatus::Connected {
        host: "broker.example.com".into(),
        port: 1883,
        commands: false,
    });
    select(&mut app, Setting::MqttStatus);
    assert_eq!(Setting::MqttStatus.value(&app), "connected");
    assert!(right_words(&app).contains("mqtt: connected to broker.example.com:1883 (read-only)"));

    app.session.daemon = false;
    assert_eq!(Setting::McpStatus.value(&app), "no daemon");
    assert_eq!(Setting::MqttStatus.value(&app), "no daemon");
}

/// Each switch turns the other way from what the file holds, on `Space` and on either arrow.
#[test]
fn the_connect_switches_ask_for_the_other_way() {
    let mut app = connect_app();
    app.overlays.conn.mqtt.host = Some("broker.example.com".into());
    for (setting, want) in [
        (Setting::McpEnabled, ConnChange::McpEnabled(true)),
        (Setting::McpWrites, ConnChange::McpWrites(true)),
        (Setting::MqttEnabled, ConnChange::MqttEnabled(true)),
        (Setting::MqttCommands, ConnChange::MqttCommands(true)),
    ] {
        select(&mut app, setting);
        for key in [
            KeyCode::Char(' '),
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Enter,
        ] {
            assert_eq!(
                press(&mut app, key),
                vec![Intent::Connection(want.clone())],
                "{setting:?} on {key:?}"
            );
        }
    }
}

/// A typed row opens an edit line on what the file holds; `Enter` saves a valid value as one
/// change, an invalid one is refused with the line still up, and `Esc` cancels.
#[test]
fn the_typed_rows_open_an_edit_line_and_save_one_change() {
    let mut app = connect_app();
    select(&mut app, Setting::McpListen);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        app.overlays.conn.modal,
        Some(Modal::Editing(Line {
            field: Field::Listen,
            text: "127.0.0.1:8737".into(),
            caret: 14
        }))
    );
    let drawn = shown(&app);
    assert!(
        drawn.contains("MCP listen") && drawn.contains("127.0.0.1:8737"),
        "{drawn}"
    );

    for _ in 0..4 {
        press(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "x");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(
        app.overlays.conn.modal.is_some(),
        "a bad address closed the line"
    );
    assert!(app.toast().is_some(), "a bad address was not refused");

    press(&mut app, KeyCode::Backspace);
    type_text(&mut app, "9001");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Connection(ConnChange::McpListen(
            "127.0.0.1:9001".parse().unwrap()
        ))]
    );
    assert!(app.overlays.conn.modal.is_none());

    select(&mut app, Setting::MqttHost);
    press(&mut app, KeyCode::Enter);
    type_text(&mut app, "broker.example.com");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Connection(ConnChange::MqttHost(
            "broker.example.com".into()
        ))]
    );

    select(&mut app, Setting::MqttPort);
    press(&mut app, KeyCode::Enter);
    for _ in 0..4 {
        press(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "0");
    press(&mut app, KeyCode::Enter);
    assert!(app.overlays.conn.modal.is_some(), "port 0 was taken");
    press(&mut app, KeyCode::Backspace);
    type_text(&mut app, "8883");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::Connection(ConnChange::MqttPort(8883))]
    );

    select(&mut app, Setting::MqttPort);
    press(&mut app, KeyCode::Enter);
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert!(
        app.overlays.conn.modal.is_none() && app.overlays.settings.is_some(),
        "Esc closed the box too"
    );
}

/// An unchanged value closes the line and asks for nothing.
#[test]
fn saving_an_unchanged_value_asks_for_nothing() {
    let mut app = connect_app();
    select(&mut app, Setting::MqttPort);
    press(&mut app, KeyCode::Enter);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.overlays.conn.modal.is_none());
}

/// `v`, `c` and `R` on the token row ask for what they say; `R` asks first, and only a yes
/// replaces the token.
#[test]
fn the_token_keys_reveal_copy_and_rotate_after_a_question() {
    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    assert_eq!(
        press(&mut app, KeyCode::Char('v')),
        vec![Intent::RevealToken]
    );
    assert_eq!(press(&mut app, KeyCode::Char('c')), vec![Intent::CopyToken]);
    // `c` on the row copies without the box; in the box it copies and the box stays.
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert_eq!(press(&mut app, KeyCode::Char('c')), vec![Intent::CopyToken]);
    assert!(app.overlays.conn.modal.is_none());

    assert!(press(&mut app, KeyCode::Char('R')).is_empty());
    assert_eq!(app.overlays.conn.modal, Some(Modal::Rotating));
    assert!(shown(&app).contains("Replace the MCP token? [y/N]"));
    for no in [KeyCode::Char('n'), KeyCode::Esc, KeyCode::Enter] {
        assert!(press(&mut app, no).is_empty());
        assert!(app.overlays.conn.modal.is_none(), "{no:?} did not cancel");
        press(&mut app, KeyCode::Char('R'));
    }
    assert_eq!(
        press(&mut app, KeyCode::Char('y')),
        vec![Intent::RotateToken]
    );
    assert!(app.overlays.conn.modal.is_none());

    // `v` opens the box and `v` again closes it, dropping the token.
    assert_eq!(
        press(&mut app, KeyCode::Char('v')),
        vec![Intent::RevealToken]
    );
    app.overlays
        .conn
        .reveal(FAKE_TOKEN.to_string(), app.now + connections::REVEAL_FOR);
    assert!(press(&mut app, KeyCode::Char('v')).is_empty());
    assert!(app.overlays.conn.revealed.is_none());
    assert!(app.overlays.conn.modal.is_none());
}

/// A token from the environment or a file is not rotated: the key says so, naming the source,
/// and no question is asked.
#[test]
fn rotating_an_outside_token_is_refused_naming_the_source() {
    for (source, named) in [
        (TokenSource::Env("SVOLDER_MCP_TOKEN"), "SVOLDER_MCP_TOKEN"),
        (
            TokenSource::File("/run/secrets/mcp".into()),
            "/run/secrets/mcp",
        ),
    ] {
        let mut app = connect_app();
        app.overlays.conn.token = TokenView::Present {
            masked: connections::mask(FAKE_TOKEN),
            source,
        };
        select(&mut app, Setting::McpToken);
        assert!(press(&mut app, KeyCode::Char('R')).is_empty());
        assert!(
            app.overlays.conn.modal.is_none(),
            "{named}: a question was asked"
        );
        let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
        assert!(toast.contains(named), "{named}: {toast}");
    }
}

/// Where the daemon reads another configuration, every Connect row is read-only and the right
/// column says where that file is.
#[test]
fn a_daemon_reading_another_config_makes_every_row_read_only() {
    let mut app = connect_app();
    let own = app.config_path.clone();
    app.overlays.conn.daemon_said(
        &daemon_with_config(Some("/config/svolder/config.toml")),
        &own,
    );
    assert_eq!(
        app.overlays.conn.elsewhere.as_deref(),
        Some("/config/svolder/config.toml")
    );
    for setting in CATEGORIES[CONNECT].1 {
        select(&mut app, *setting);
        let mut keys = vec![KeyCode::Char(' '), KeyCode::Enter, KeyCode::Left];
        if *setting == Setting::McpToken {
            keys.extend([KeyCode::Char('v'), KeyCode::Char('c'), KeyCode::Char('R')]);
        }
        for key in keys {
            // The Web UI row opens its box, whose own writes are refused as these rows' are.
            if *setting == Setting::WebUi {
                assert!(
                    press(&mut app, key).is_empty(),
                    "the Web UI box asked for a link"
                );
                assert!(
                    press(&mut app, KeyCode::Enter).is_empty(),
                    "the Web UI box wrote on a daemon that reads another file"
                );
                let _ = press(&mut app, KeyCode::Esc);
                continue;
            }
            assert!(
                press(&mut app, key).is_empty(),
                "{setting:?} on {key:?} asked for something"
            );
            assert!(
                app.overlays.conn.modal.is_none(),
                "{setting:?} on {key:?} opened a box"
            );
        }
        assert!(
            right_words(&app).contains("the daemon reads /config/svolder/config.toml"),
            "{setting:?}: the right column does not say where the daemon's file is"
        );
    }
    let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(toast.contains("/config/svolder/config.toml"), "{toast}");
}

/// A daemon reading this very file, spelled the same or resolving to it, leaves the rows
/// editable; so does one too old to say.
#[test]
fn a_daemon_reading_the_same_config_leaves_the_rows_editable() {
    let mut app = connect_app();
    let own = app.config_path.clone();
    let spelled = own.display().to_string();
    app.overlays
        .conn
        .daemon_said(&daemon_with_config(Some(&spelled)), &own);
    assert!(app.overlays.conn.elsewhere.is_none());
    app.overlays
        .conn
        .daemon_said(&daemon_with_config(None), &own);
    assert!(app.overlays.conn.elsewhere.is_none());
    select(&mut app, Setting::McpEnabled);
    assert!(!press(&mut app, KeyCode::Char(' ')).is_empty());
}

/// Moving to the Connect category with Tab or its digit works like any other, and a box open on
/// it swallows the keys that would leave.
#[test]
fn the_connect_box_swallows_every_key_while_open() {
    let mut app = connect_app();
    select(&mut app, Setting::MqttPort);
    press(&mut app, KeyCode::Enter);
    for key in [KeyCode::Tab, KeyCode::Char('1'), KeyCode::Down] {
        press(&mut app, key);
    }
    assert_eq!(
        app.overlays.settings.map(|at| at.category),
        Some(CONNECT),
        "a key left the category with a line open"
    );
    // The rows under a box take no click: a click outside the edit line cancels it, and
    // one inside it asks for nothing.
    let (_, _, hits) = drawn(&app, 80, 24);
    assert_eq!(
        mouse::resolve(&hits, click(5, 3), &app),
        Some(MouseAction::Key(KeyCode::Esc)),
        "a row took a click under the box"
    );
    let top = hits.top_box().expect("the edit line is a box");
    assert!(top.width < 80, "the edit line is a box of its own: {top:?}");
    assert_eq!(
        mouse::resolve(&hits, click(top.x + 1, top.y + 1), &app),
        None
    );
}

/// A click outside the Connect edit line cancels the edit and leaves the Settings box open.
#[test]
fn a_click_outside_the_connect_edit_line_cancels_it_and_keeps_the_box() {
    let mut app = connect_app();
    select(&mut app, Setting::MqttPort);
    press(&mut app, KeyCode::Enter);
    assert!(
        app.overlays.conn.modal.is_some(),
        "Enter opened no edit line"
    );
    let (_, _, hits) = drawn(&app, 80, 24);
    let Some(MouseAction::Key(code)) = mouse::resolve(&hits, click(0, 0), &app) else {
        panic!("a click outside did not press a key");
    };
    assert_eq!(code, KeyCode::Esc);
    press(&mut app, code);
    assert!(app.overlays.conn.modal.is_none(), "the edit stayed open");
    assert!(
        app.overlays.settings.is_some(),
        "the box closed with the edit"
    );
}

/// Where the daemon's token is not the one this session reads, the row shows the daemon's tail,
/// the right column says where its token lives, and `v`, `c` and `R` refuse with that reason.
#[test]
fn a_token_the_daemon_does_not_use_is_labelled_and_its_keys_refuse() {
    use svolder_ipc::{TokenInfo, TokenKind};

    let mut app = connect_app();
    let mut info = daemon_with_config(None);
    info.mcp_token = Some(TokenInfo {
        kind: TokenKind::Environment,
        path: None,
        tail: "9xZ".into(),
    });
    let own = app.config_path.clone();
    app.overlays.conn.daemon_said(&info, &own);
    select(&mut app, Setting::McpToken);
    assert_eq!(Setting::McpToken.value(&app), "••••••9xZ");
    assert!(
        right_words(&app).contains("in the daemon's environment (SVOLDER_MCP_TOKEN)"),
        "{}",
        right_words(&app)
    );
    assert!(!shown(&app).contains("3fQ"), "this session's tail is drawn");
    for key in ['v', 'c', 'R'] {
        assert!(press(&mut app, KeyCode::Char(key)).is_empty(), "{key}");
        assert!(app.overlays.conn.modal.is_none());
        let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
        assert!(toast.contains("SVOLDER_MCP_TOKEN"), "{key}: {toast}");
    }

    info.mcp_token = Some(TokenInfo {
        kind: TokenKind::State,
        path: Some("/var/lib/svolder/mcp-token".into()),
        tail: "3fQ".into(),
    });
    app.overlays.conn.daemon_said(&info, &own);
    assert!(
        right_words(&app)
            .contains("at /var/lib/svolder/mcp-token, not the file this session reads"),
        "{}",
        right_words(&app)
    );
}

/// With no daemon answering the row is this session's own reading, marked as such, and the
/// token keys work on it.
#[test]
fn with_no_daemon_the_row_is_the_sessions_own_and_marked() {
    let mut app = connect_app();
    app.overlays.conn.daemon_missing();
    select(&mut app, Setting::McpToken);
    assert_eq!(Setting::McpToken.value(&app), "••••••3fQ");
    assert!(
        right_words(&app).contains("(daemon not running)"),
        "{}",
        right_words(&app)
    );
    assert_eq!(
        press(&mut app, KeyCode::Char('v')),
        vec![Intent::RevealToken]
    );
    assert_eq!(press(&mut app, KeyCode::Char('c')), vec![Intent::CopyToken]);
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('R'));
    assert_eq!(app.overlays.conn.modal, Some(Modal::Rotating));
}

/// The token keys are in the key table, so the help overlay lists them.
#[test]
fn the_token_keys_are_in_the_key_table() {
    let hints = crate::tui::keys::hints_for(Screen::Settings);
    for label in ["v", "c", "R"] {
        assert!(
            hints
                .iter()
                .any(|hint| hint.label == label && hint.does.contains("token")),
            "{label} is not listed for the Settings box"
        );
    }
}

/// An edit line opened before the daemon says it reads another file saves nothing: Enter on it
/// is refused with the same words as any key on a read-only row.
#[test]
fn an_edit_line_opened_before_an_elsewhere_answer_does_not_save() {
    let mut app = connect_app();
    select(&mut app, Setting::McpListen);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Backspace);
    assert!(matches!(app.overlays.conn.modal, Some(Modal::Editing(_))));
    let own = app.config_path.clone();
    app.overlays.conn.daemon_said(
        &daemon_with_config(Some("/config/svolder/config.toml")),
        &own,
    );
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.overlays.conn.modal.is_none());
    let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(toast.contains("read-only here"), "{toast}");
}

/// The question before the token is replaced is answered against what is known when it is
/// answered: a daemon that has said it uses another token since the question went up turns a
/// yes into a refusal.
#[test]
fn a_rotate_question_answered_after_a_differs_answer_does_not_rotate() {
    use svolder_ipc::{TokenInfo, TokenKind};

    let mut app = connect_app();
    select(&mut app, Setting::McpToken);
    press(&mut app, KeyCode::Char('R'));
    assert_eq!(app.overlays.conn.modal, Some(Modal::Rotating));
    let mut info = daemon_with_config(None);
    info.mcp_token = Some(TokenInfo {
        kind: TokenKind::Environment,
        path: None,
        tail: "9xZ".into(),
    });
    let own = app.config_path.clone();
    app.overlays.conn.daemon_said(&info, &own);
    assert!(press(&mut app, KeyCode::Char('y')).is_empty());
    assert!(app.overlays.conn.modal.is_none());
    let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
    assert!(toast.contains("not this session's token"), "{toast}");
}

/// The AutoEq refresh row steps `never`, `1 week`, `2 weeks` … `99 weeks` one at a time with the
/// arrows and stops at both ends instead of wrapping; Space turns nothing.
#[test]
fn the_autoeq_refresh_row_steps_and_stops_at_the_ends() {
    use svolder_ipc::config::RefreshWeeks::{Never, Weeks};
    let step = |app: &mut App, key| match press(app, key).as_slice() {
        [Intent::AutoEqRefresh(weeks)] => Some(*weeks),
        [] => None,
        other => panic!("unexpected {other:?}"),
    };
    let mut app = app();
    select(&mut app, Setting::AutoEqRefresh);
    assert_eq!(
        Setting::AutoEqRefresh.value(&app),
        "13 weeks",
        "the default"
    );

    app.presets.autoeq_weeks = Weeks(13);
    assert_eq!(step(&mut app, KeyCode::Right), Some(Weeks(14)));
    assert_eq!(step(&mut app, KeyCode::Left), Some(Weeks(12)));

    app.presets.autoeq_weeks = Weeks(99);
    assert_eq!(step(&mut app, KeyCode::Right), None, "99 weeks is the top");
    assert_eq!(step(&mut app, KeyCode::Left), Some(Weeks(98)));

    app.presets.autoeq_weeks = Weeks(1);
    assert_eq!(step(&mut app, KeyCode::Left), Some(Never));
    assert_eq!(step(&mut app, KeyCode::Right), Some(Weeks(2)));

    app.presets.autoeq_weeks = Never;
    assert_eq!(step(&mut app, KeyCode::Left), None, "never is the bottom");
    assert_eq!(step(&mut app, KeyCode::Right), Some(Weeks(1)));
    assert_eq!(press(&mut app, KeyCode::Char(' ')), Vec::new());
}

/// The row reads the refresh the way the file holds it, and its right column says where.
#[test]
fn the_autoeq_refresh_row_reads_the_file_s_value() {
    use svolder_ipc::config::RefreshWeeks::{Never, Weeks};
    let mut app = app();
    for (weeks, value, line) in [
        (Never, "never", "[autoeq] refresh_weeks = \"never\""),
        (Weeks(1), "1 week", "[autoeq] refresh_weeks = 1"),
        (Weeks(2), "2 weeks", "[autoeq] refresh_weeks = 2"),
        (Weeks(99), "99 weeks", "[autoeq] refresh_weeks = 99"),
    ] {
        app.presets.autoeq_weeks = weeks;
        assert_eq!(Setting::AutoEqRefresh.value(&app), value);
        assert_eq!(super::autoeq_line(weeks), line);
    }
    select(&mut app, Setting::AutoEqRefresh);
    let rows = box_rows(&app).join("\n");
    assert!(rows.contains("AutoEq refresh"), "{rows}");
    assert!(rows.contains("[autoeq] refresh_weeks = 99"), "{rows}");
}

/// Taking the configuration in reads the refresh from `[autoeq]`, the default 13 weeks while
/// the table is absent.
#[test]
fn the_app_takes_the_refresh_from_the_config() {
    use svolder_ipc::config::RefreshWeeks::{Never, Weeks};
    let mut app = plain();
    assert_eq!(app.presets.autoeq_weeks, Weeks(13));
    let config = svolder_ipc::Config::parse("schema = 1\n[autoeq]\nrefresh_weeks = \"never\"\n")
        .expect("a config");
    app.take_config(&config);
    assert_eq!(app.presets.autoeq_weeks, Never);
}

/// The switch rows draw as a two-way choice, the side in force lit and the label of the row
/// kept, whether or not the cursor is on them; the rows that only report or open something draw
/// as plain text, with no arrows.
#[test]
fn the_switch_rows_draw_as_choices_and_the_status_rows_as_plain_text() {
    let mut app = connect_app();
    app.overlays.conn.mqtt.host = Some("broker.example.com".into());
    let row_of = |app: &App, label: &str| {
        box_rows(app)
            .into_iter()
            .find(|row| row.contains(label))
            .unwrap_or_else(|| panic!("no row {label}"))
    };
    for (label, choice) in [
        ("MCP server", "‹ off | on ›"),
        ("MCP writes", "‹ read-only | writes ›"),
        ("MQTT client", "‹ off | on ›"),
        ("MQTT commands", "‹ read-only | allowed ›"),
    ] {
        // the cursor elsewhere: the choice is still drawn whole
        select(&mut app, Setting::McpToken);
        assert!(
            row_of(&app, label).contains(choice),
            "{label}: {}",
            row_of(&app, label)
        );
    }
    app.overlays.conn.mcp.enabled = true;
    app.overlays.conn.mcp.writes = true;
    app.overlays.conn.mqtt.enabled = true;
    app.overlays.conn.mqtt.commands = true;
    for (setting, label, choice) in [
        (Setting::McpEnabled, "MCP server", "‹ off | on ›"),
        (Setting::McpWrites, "MCP writes", "‹ read-only | writes ›"),
        (Setting::MqttEnabled, "MQTT client", "‹ off | on ›"),
        (
            Setting::MqttCommands,
            "MQTT commands",
            "‹ read-only | allowed ›",
        ),
    ] {
        for focused in [false, true] {
            select(&mut app, if focused { setting } else { Setting::McpToken });
            assert!(
                row_of(&app, label).contains(choice),
                "{label}: {}",
                row_of(&app, label)
            );
        }
    }
    // a status row, focused: no arrows
    for setting in [
        Setting::McpStatus,
        Setting::MqttStatus,
        Setting::McpToken,
        Setting::WebUi,
    ] {
        select(&mut app, setting);
        let row = box_rows(&app)
            .into_iter()
            .find(|row| row.contains('▶'))
            .unwrap();
        let left: String = row.chars().take(44).collect();
        assert!(
            !left.contains('‹') && !left.contains('›'),
            "{setting:?}: {row}"
        );
    }
}

/// The choice's lit side is the value in force: the style of `on` differs from `off`'s.
#[test]
fn the_choice_lights_the_side_in_force() {
    let mut app = connect_app();
    let lit = |app: &App, side: &str| {
        let buffer = frame(app, 80, 24);
        let rows = text(&buffer);
        let y = rows.iter().position(|r| r.contains("MCP server")).unwrap();
        let x = rows[y].find("‹ off | on ›").unwrap();
        let offset = if side == "on" { 8 } else { 2 };
        let at = rows[y][..x].chars().count() + offset;
        buffer[(u16::try_from(at).unwrap(), u16::try_from(y).unwrap())].style()
    };
    select(&mut app, Setting::McpToken);
    let (off_style, on_style) = (lit(&app, "off"), lit(&app, "on"));
    assert_ne!(off_style, on_style, "both sides look alike while off");
    app.overlays.conn.mcp.enabled = true;
    assert_eq!(lit(&app, "on"), off_style, "the lit side did not move");
    assert_eq!(lit(&app, "off"), on_style);
}

/// Turning the MQTT client on with no broker host says why at once and puts the cursor on the
/// host row; with a host it asks for the change.
#[test]
fn the_mqtt_client_cannot_go_on_without_a_host_and_says_so() {
    let mut app = connect_app();
    select(&mut app, Setting::MqttEnabled);
    for key in [KeyCode::Char(' '), KeyCode::Right, KeyCode::Enter] {
        assert!(press(&mut app, key).is_empty());
        let toast = app.toast().map(|t| t.text.clone()).unwrap_or_default();
        assert!(toast.contains("set MQTT broker host first"), "{toast}");
        assert_eq!(app.overlays.settings.map(current), Some(Setting::MqttHost));
        select(&mut app, Setting::MqttEnabled);
    }
    // turning it off needs no host
    app.overlays.conn.mqtt.enabled = true;
    assert_eq!(
        press(&mut app, KeyCode::Char(' ')),
        vec![Intent::Connection(ConnChange::MqttEnabled(false))]
    );
}

/// No row, tooltip or help text of the category calls the MQTT row a bridge.
#[test]
fn the_row_is_the_mqtt_client() {
    assert_eq!(Setting::MqttEnabled.label(), "MQTT client");
    let mut app = connect_app();
    select(&mut app, Setting::MqttEnabled);
    let all = shown(&app);
    assert!(!all.contains("bridge"), "{all}");
    assert!(!Setting::MqttCommands.label().contains("bridge"));
}

/// The themes a focused row is checked at: monochrome, then each colour depth.
fn depth_themes() -> Vec<(String, Theme)> {
    let mut themes = vec![(
        "monochrome".to_string(),
        Theme::from_config(&svolder_ipc::TuiConfig::default(), true, ColorDepth::Ansi16),
    )];
    for depth in [
        ColorDepth::Ansi16,
        ColorDepth::Ansi256,
        ColorDepth::TrueColor,
    ] {
        themes.push((format!("{depth:?}"), theme_for("amber", depth)));
    }
    themes
}

/// The focused switch row is no band: its label and the side in force take the selected style,
/// the other side does not, and the cells after the choice are plain, at every depth.
#[test]
fn a_focused_switch_row_marks_its_label_and_side_in_force_not_the_row() {
    for (name, theme) in depth_themes() {
        let selected = theme.style(Role::Selected).add_modifier;
        for (setting, label, off, on) in [
            (Setting::McpEnabled, "MCP server", "off", "on"),
            (Setting::McpWrites, "MCP writes", "read-only", "writes"),
            (Setting::MqttEnabled, "MQTT client", "off", "on"),
            (
                Setting::MqttCommands,
                "MQTT commands",
                "read-only",
                "allowed",
            ),
        ] {
            for second in [false, true] {
                let mut app = connect_app();
                app.overlays.conn.mqtt.host = Some("broker.example.com".into());
                match setting {
                    Setting::McpEnabled => app.overlays.conn.mcp.enabled = second,
                    Setting::McpWrites => app.overlays.conn.mcp.writes = second,
                    Setting::MqttEnabled => app.overlays.conn.mqtt.enabled = second,
                    _ => app.overlays.conn.mqtt.commands = second,
                }
                select(&mut app, setting);
                let buffer = frame_with_theme(&app, 80, 24, &theme);
                let rows = text(&buffer);
                let y = rows.iter().position(|r| r.contains(label)).unwrap();
                let chars: Vec<char> = rows[y].chars().collect();
                let at = |word: &str, from: usize| {
                    let w: Vec<char> = word.chars().collect();
                    (from..chars.len())
                        .find(|&i| chars[i..].starts_with(&w))
                        .unwrap()
                };
                let styled = |x: usize| {
                    buffer
                        .cell((u16::try_from(x).unwrap(), u16::try_from(y).unwrap()))
                        .unwrap()
                        .modifier
                        .contains(selected)
                };
                let lead = at(label, 0);
                let arrows = at("‹", lead);
                let first = arrows + 2;
                let second_at = at(" | ", first) + 3;
                let (lit, dim) = match second {
                    true => (second_at, first),
                    false => (first, second_at),
                };
                let _ = (off, on);
                let ctx = format!("{name} {label} second={second}");
                assert!(styled(lead), "{ctx}: the label is not selected");
                assert!(styled(lit), "{ctx}: the side in force is not selected");
                assert!(!styled(dim), "{ctx}: the other side looks selected");
                let end = at("›", second_at) + 1;
                assert!(!styled(end), "{ctx}: the row is still a band");
            }
        }
    }
}

/// Each switch row's `‹` and `›` are its Prev and Next hits, by cell, and the rule beside the
/// row is neither.
#[test]
fn the_switch_rows_arrows_are_their_hits_and_the_rule_is_not() {
    for setting in [
        Setting::McpEnabled,
        Setting::McpWrites,
        Setting::MqttEnabled,
        Setting::MqttCommands,
    ] {
        let mut app = connect_app();
        select(&mut app, setting);
        let (buffer, _, hits) = drawn(&app, 80, 24);
        let rows = text(&buffer);
        let y = rows
            .iter()
            .position(|r| r.contains(setting.label()))
            .unwrap();
        let chars: Vec<char> = rows[y].chars().collect();
        let open = chars.iter().position(|c| *c == '‹').unwrap();
        let close = chars.iter().position(|c| *c == '›').unwrap();
        let y = u16::try_from(y).unwrap();
        let hit = |x: usize| hits.hit_at(u16::try_from(x).unwrap(), y).cloned();
        assert_eq!(hit(open), Some(Hit::SettingPrev), "{setting:?} ‹");
        assert_eq!(hit(close), Some(Hit::SettingNext), "{setting:?} ›");
        let rule = chars.iter().rposition(|c| *c == '│').unwrap();
        let rule = chars[..rule].iter().rposition(|c| *c == '│').unwrap();
        assert!(close < rule, "{setting:?}: the choice reaches the rule");
        for x in [rule, rule + 1] {
            assert!(
                !matches!(hit(x), Some(Hit::SettingPrev | Hit::SettingNext)),
                "{setting:?}: cell {x} past the choice is an arrow hit"
            );
        }
    }
}

/// The MCP status row's right column and the Web UI row's carry the daemon's words for the
/// counts (`line()`), and a daemon that says none, or a server that is off, shows no count.
#[test]
fn the_connect_rows_say_the_clients_the_daemon_counts() {
    let mut app = connect_app();
    app.overlays.conn.web_status = Some(svolder_ipc::WebStatus::Listening {
        listen: "127.0.0.1:8738".parse().unwrap(),
        auth: svolder_ipc::config::WebAuth::Token,
        writes: true,
        guest: svolder_ipc::config::WebGuest::None,
    });
    let said = activity_said();
    app.overlays.conn.activity = Some(said);
    select(&mut app, Setting::McpStatus);
    let words = right_words(&app);
    assert!(words.contains(&said.mcp.unwrap().line()), "{words}");
    select(&mut app, Setting::WebUi);
    let words = right_words(&app);
    assert!(words.contains(&said.web.unwrap().line()), "{words}");
    // An older daemon says nothing: no count anywhere.
    app.overlays.conn.activity = None;
    for setting in [Setting::McpStatus, Setting::WebUi] {
        select(&mut app, setting);
        assert!(
            !right_words(&app).contains(&said_line(setting)),
            "{setting:?}"
        );
    }
    // A server that is off shows none, whatever the last answer held.
    app.overlays.conn.activity = Some(said);
    app.overlays.conn.mcp_status = Some(McpStatus::Off);
    app.overlays.conn.web_status = Some(svolder_ipc::WebStatus::Off);
    for setting in [Setting::McpStatus, Setting::WebUi] {
        select(&mut app, setting);
        assert!(
            !right_words(&app).contains(&said_line(setting)),
            "{setting:?}"
        );
    }
}

/// The daemon's `line()` for `setting`'s server in [`activity_said`].
fn said_line(setting: Setting) -> String {
    let said = activity_said();
    match setting {
        Setting::McpStatus => said.mcp.unwrap().line(),
        _ => said.web.unwrap().line(),
    }
}

/// A daemon that stops answering takes its counts with it: no count on the MCP row, frozen or
/// not.
#[test]
fn counts_go_when_the_daemon_stops_answering() {
    let mut app = connect_app();
    app.overlays.conn.activity = Some(activity_said());
    app.overlays.conn.mqtt_status = Some(MqttStatus::Off);
    select(&mut app, Setting::McpStatus);
    assert!(right_words(&app).contains(&said_line(Setting::McpStatus)));
    app.overlays.conn.daemon_missing();
    assert_eq!(app.overlays.conn.activity, None);
    assert_eq!(app.overlays.conn.mqtt_status, None);
    assert!(!right_words(&app).contains(&said_line(Setting::McpStatus)));
    // A session that knows no daemon is up shows none either, whatever it holds.
    app.overlays.conn.activity = Some(activity_said());
    app.overlays.conn.mcp_status = Some(McpStatus::Listening {
        listen: "127.0.0.1:8737".parse().unwrap(),
        auth: McpAuth::Token,
        writes: false,
    });
    app.session.daemon = false;
    assert!(!right_words(&app).contains(&said_line(Setting::McpStatus)));
}

/// The MQTT status column is the same with counts as without.
#[test]
fn the_mqtt_status_column_ignores_the_counts() {
    let mut app = connect_app();
    select(&mut app, Setting::MqttStatus);
    let before = box_rows(&app);
    app.overlays.conn.activity = Some(activity_said());
    assert_eq!(box_rows(&app), before);
}

/// `daemon_said` keeps the daemon's `activity` and drops it when a later answer has none.
#[test]
fn daemon_said_keeps_and_drops_the_activity() {
    let mut app = connect_app();
    let own = app.config_path.clone();
    let mut info = daemon_with_config(None);
    info.activity = Some(activity_said());
    app.overlays.conn.daemon_said(&info, &own);
    assert_eq!(app.overlays.conn.activity, Some(activity_said()));
    app.overlays
        .conn
        .daemon_said(&daemon_with_config(None), &own);
    assert_eq!(app.overlays.conn.activity, None);
}
