// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Connect category's state, with a fixed fake token compared with `assert!` so a failing
//! case never prints it.

use super::*;
use crate::tui::boxes::connections::TokenState;

const FAKE_TOKEN: &str = "fake-token-0123456789-abcdefghijklmnop-x3fQ";

#[test]
fn base64_pads_as_the_standard_alphabet_does() {
    for (plain, coded) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
        ("hello", "aGVsbG8="),
    ] {
        assert_eq!(base64(plain.as_bytes()), coded, "{plain:?}");
    }
    assert_eq!(base64(&[0xfb, 0xff, 0xfe]), "+//+");
}

#[test]
fn the_clipboard_sequence_is_osc_52_to_the_clipboard_selection() {
    assert_eq!(osc52("hello"), "\x1b]52;c;aGVsbG8=\x07");
    let sequence = osc52(FAKE_TOKEN);
    assert!(sequence.starts_with("\x1b]52;c;") && sequence.ends_with('\x07'));
    assert!(!sequence.contains("fake-token"), "the token is not encoded");
}

#[test]
fn the_mask_hides_all_but_a_long_tokens_last_three() {
    assert_eq!(mask(FAKE_TOKEN), "••••••3fQ");
    assert_eq!(mask("short"), "••••••");
    assert_eq!(mask(&"a".repeat(15)), "••••••");
    assert_eq!(mask(&"a".repeat(16)), "••••••aaa");
}

#[test]
fn what_is_kept_of_a_token_is_its_mask_and_its_source() {
    let mut conn = Connections::default();
    let token = svolder_ipc::token::peek_with(
        svolder_ipc::token::Store::MCP,
        Some(FAKE_TOKEN.to_string()),
        None,
        std::path::Path::new("/nowhere"),
    )
    .expect("an environment token reads");
    conn.take_token(Ok(token));
    assert!(
        conn.token
            == TokenView::Present {
                masked: "••••••3fQ".into(),
                source: TokenSource::Env("SVOLDER_MCP_TOKEN"),
            }
    );
    assert!(!format!("{conn:?}").contains("fake-token"));
    conn.take_token(Ok(None));
    assert!(conn.token == TokenView::Missing);
    conn.take_token(Err(TokenError("token file /x is empty".into())));
    assert!(conn.token == TokenView::Unreadable("token file /x is empty".into()));
}

#[test]
fn a_revealed_token_is_shown_until_its_time_and_dropped_after() {
    let mut conn = Connections::default();
    let at = Instant::now();
    conn.reveal(FAKE_TOKEN.to_string(), at + REVEAL_FOR);
    assert!(conn.shown(at) == Some(FAKE_TOKEN));
    assert!(conn.shown(at + Duration::from_secs(9)) == Some(FAKE_TOKEN));
    assert!(conn.shown(at + REVEAL_FOR).is_none());
    conn.expire(at + Duration::from_secs(9));
    assert!(conn.revealed.is_some());
    conn.expire(at + REVEAL_FOR);
    assert!(conn.revealed.is_none());
    let revealed = Revealed {
        text: FAKE_TOKEN.into(),
        until: at,
    };
    assert!(!format!("{revealed:?}").contains("fake-token"));
}

#[test]
fn a_new_token_read_drops_the_one_on_show() {
    let mut conn = Connections::default();
    conn.reveal(FAKE_TOKEN.to_string(), Instant::now() + REVEAL_FOR);
    conn.take_token(Ok(None));
    assert!(conn.revealed.is_none());
}

#[test]
fn the_daemon_says_where_it_reads_and_the_rows_follow() {
    let own = Path::new("/home/user/.config/svolder/config.toml");
    let said = |config: Option<&str>| DaemonInfo {
        version: "0.0.0".into(),
        mqtt: Some(MqttStatus::Off),
        mcp: Some(McpStatus::Off),
        config: config.map(str::to_string),
        mcp_token: None,
        web: None,
        web_token: None,
    };
    let mut conn = Connections::default();
    conn.daemon_said(&said(Some("/home/user/.config/svolder/config.toml")), own);
    assert!(conn.elsewhere.is_none());
    assert!(conn.mcp_status == Some(McpStatus::Off) && conn.mqtt_status == Some(MqttStatus::Off));
    conn.daemon_said(&said(Some("/config/config.toml")), own);
    assert_eq!(conn.elsewhere.as_deref(), Some("/config/config.toml"));
    conn.daemon_said(&said(None), own);
    assert!(
        conn.elsewhere.is_none(),
        "a daemon too old to say is not elsewhere"
    );
}

#[test]
fn the_sections_load_without_the_password() {
    let config = Config::parse(
        "schema = 1\n[mcp]\nenabled = true\nlisten = \"127.0.0.1:9000\"\nwrites = true\n\
         [mqtt]\nenabled = true\nhost = \"broker.example.com\"\npassword = \"fake-secret-pass\"\n",
    )
    .expect("the config parses");
    let mut conn = Connections::default();
    conn.load(&config);
    assert!(conn.mcp.enabled && conn.mcp.writes);
    assert_eq!(conn.mcp.listen.port(), 9000);
    assert_eq!(conn.mqtt.host.as_deref(), Some("broker.example.com"));
    assert!(conn.mqtt.password_set);
    assert!(!format!("{conn:?}").contains("fake-secret"));
}

/// A token as this session reads it: masked, from `source`.
fn read_as(conn: &mut Connections, source: TokenSource, tail: &str) {
    conn.token = TokenView::Present {
        masked: format!("••••••{tail}"),
        source,
    };
}

fn described(kind: TokenKind, path: Option<&str>, tail: &str) -> DaemonInfo {
    DaemonInfo {
        version: "0.0.0".into(),
        mqtt: None,
        mcp: None,
        config: None,
        mcp_token: Some(TokenInfo {
            kind,
            path: path.map(str::to_string),
            tail: tail.into(),
        }),
        web: None,
        web_token: None,
    }
}

const STATE: &str = "/home/user/.local/state/svolder/mcp-token";
const OWN_CONFIG: &str = "/home/user/.config/svolder/config.toml";

#[test]
fn a_daemon_with_an_environment_token_differs_from_a_session_reading_its_state_file() {
    let mut conn = Connections::default();
    read_as(&mut conn, TokenSource::Generated(STATE.into()), "3fQ");
    conn.daemon_said(
        &described(TokenKind::Environment, None, "9xZ"),
        Path::new(OWN_CONFIG),
    );
    assert_eq!(
        conn.token_state(),
        TokenState::Differs {
            masked: "••••••9xZ".into(),
            reason: "in the daemon's environment (SVOLDER_MCP_TOKEN)".into(),
        }
    );
}

#[test]
fn another_path_with_another_tail_is_another_token() {
    let mut conn = Connections::default();
    read_as(&mut conn, TokenSource::Generated(STATE.into()), "3fQ");
    conn.daemon_said(
        &described(TokenKind::State, Some(STATE), "3fQ"),
        Path::new(OWN_CONFIG),
    );
    assert_eq!(conn.token_state(), TokenState::Own);
    conn.daemon_said(
        &described(TokenKind::State, Some("/var/lib/svolder/mcp-token"), "aaa"),
        Path::new(OWN_CONFIG),
    );
    assert_eq!(
        conn.token_state(),
        TokenState::Differs {
            masked: "••••••aaa".into(),
            reason: "at /var/lib/svolder/mcp-token, not the file this session reads".into(),
        }
    );
}

#[test]
fn a_matching_file_source_is_this_sessions_own() {
    let mut conn = Connections::default();
    read_as(
        &mut conn,
        TokenSource::File("/run/secrets/mcp".into()),
        "3fQ",
    );
    conn.daemon_said(
        &described(TokenKind::File, Some("/run/secrets/mcp"), "3fQ"),
        Path::new(OWN_CONFIG),
    );
    assert_eq!(conn.token_state(), TokenState::Own);
    conn.daemon_said(
        &described(TokenKind::File, Some("/run/secrets/other"), "3fQ"),
        Path::new(OWN_CONFIG),
    );
    assert!(matches!(conn.token_state(), TokenState::Differs { .. }));
    conn.daemon_said(
        &described(TokenKind::State, Some(STATE), "3fQ"),
        Path::new(OWN_CONFIG),
    );
    assert!(
        matches!(conn.token_state(), TokenState::Differs { .. }),
        "a kind apart"
    );
}

#[test]
fn a_session_with_no_token_differs_from_a_daemon_that_has_one() {
    let mut conn = Connections::default();
    conn.take_token(Ok(None));
    conn.daemon_said(
        &described(TokenKind::Environment, None, "9xZ"),
        Path::new(OWN_CONFIG),
    );
    assert!(matches!(conn.token_state(), TokenState::Differs { .. }));
}

#[test]
fn no_daemon_or_an_older_reply_leaves_the_sessions_own_reading() {
    let mut conn = Connections::default();
    read_as(&mut conn, TokenSource::Generated(STATE.into()), "3fQ");
    assert_eq!(conn.token_state(), TokenState::Own);
    conn.daemon_said(
        &described(TokenKind::Environment, None, "9xZ"),
        Path::new(OWN_CONFIG),
    );
    conn.daemon_missing();
    assert_eq!(conn.daemon_up, Some(false));
    assert_eq!(conn.token_state(), TokenState::Own);
    let older = DaemonInfo {
        version: "0.12.0".into(),
        mqtt: None,
        mcp: None,
        config: None,
        mcp_token: None,
        web: None,
        web_token: None,
    };
    conn.daemon_said(&older, Path::new(OWN_CONFIG));
    assert_eq!(conn.token_state(), TokenState::Own);
}

/// A token on show is this session's own: it goes when the daemon says it uses another one,
/// and stays while it says it uses this one.
#[test]
fn a_revealed_token_goes_when_the_state_turns_away_from_own() {
    let mut conn = Connections::default();
    read_as(&mut conn, TokenSource::Generated(STATE.into()), "3fQ");
    let until = Instant::now() + Duration::from_secs(10);
    conn.reveal("fake-revealed-token-0000".into(), until);
    conn.daemon_said(
        &described(TokenKind::State, Some(STATE), "3fQ"),
        Path::new(OWN_CONFIG),
    );
    assert!(conn.shown(Instant::now()).is_some());
    conn.daemon_said(
        &described(TokenKind::Environment, None, "9xZ"),
        Path::new(OWN_CONFIG),
    );
    assert!(conn.shown(Instant::now()).is_none());
    assert!(conn.revealed.is_none());
}

/// The same file with another tail is a daemon that has not read it again, not another token;
/// an environment token with another tail is another token.
#[test]
fn the_same_file_with_another_tail_is_not_read_again_yet() {
    let mut conn = Connections::default();
    read_as(&mut conn, TokenSource::Generated(STATE.into()), "3fQ");
    conn.daemon_said(
        &described(TokenKind::State, Some(STATE), "aaa"),
        Path::new(OWN_CONFIG),
    );
    assert_eq!(
        conn.token_state(),
        TokenState::NotReadYet { path: STATE.into() }
    );
    assert!(conn.token_refusal().is_none());

    read_as(&mut conn, TokenSource::Env("SVOLDER_MCP_TOKEN"), "3fQ");
    conn.daemon_said(
        &described(TokenKind::Environment, None, "aaa"),
        Path::new(OWN_CONFIG),
    );
    assert!(matches!(conn.token_state(), TokenState::Differs { .. }));
}
