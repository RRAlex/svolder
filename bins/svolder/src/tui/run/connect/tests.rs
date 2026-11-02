// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The loop's side of the Connect category, over a stub daemon and token files in a scratch
//! directory of each case's own. Tokens are fake and compared with `assert!`, never printed.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex};

use svolder_ipc::McpStatus;
use svolder_ipc::token::{self, Store, TokenSource};

use crate::tui::app::ConnChange;
use crate::tui::boxes::connections::{self, TokenView};

use super::super::test_support::*;
use super::super::*;
use super::{ask_daemon, read};

const FAKE_ENV_TOKEN: &str = "fake-env-token-not-a-real-secret-0000";

/// The token under `home`, made if there is none.
fn token_under(home: &Path) -> String {
    token::peek_with(Store::MCP, None, None, home)
        .expect("the token reads")
        .map_or_else(
            || {
                token::rotate_with(Store::MCP, None, None, home)
                    .expect("a token is made")
                    .value()
                    .to_string()
            },
            |token| token.value().to_string(),
        )
}

/// A change writes only its own key into the file, keeps the file's comments, tells the daemon
/// to reload and asks it how it stands now; and what the daemon answers lands on the rows.
#[tokio::test]
async fn a_change_writes_only_its_key_then_reloads_and_asks_again() {
    let (daemon, path, watching) = on_a_daemon().await;
    let before = std::fs::read_to_string(&path).expect("the file reads");
    let opener = opener_on_nothing(path.clone());
    let (mut look, _) = look_over(&state_home());

    let mut watching = taken(
        Intent::Connection(ConnChange::McpWrites(true)),
        &opener,
        watching,
        &mut look,
    )
    .await;

    let text = std::fs::read_to_string(&path).expect("the file reads");
    let config = Config::parse(&text).expect("the file parses");
    assert!(config.mcp.writes && !config.mcp.enabled);
    assert_eq!(
        text.lines().count(),
        before.lines().count(),
        "a key was added where the template shows one"
    );
    assert!(text.contains("# svolderd's MCP server for AI assistants"));
    assert_eq!(daemon.reloads(), 1, "the daemon was not told to reload");
    assert!(
        watching.app.overlays.conn.mcp.writes,
        "the row does not say what is on disk"
    );
    assert!(
        watching.daemon_version.is_some(),
        "the daemon was not asked how it stands"
    );

    daemon.says(
        Some(McpStatus::Listening {
            listen: "127.0.0.1:8737".parse().unwrap(),
            auth: svolder_ipc::config::McpAuth::Token,
            writes: true,
        }),
        None,
    );
    let info = recv_version(&mut watching.daemon_version).await;
    daemon_answered(&mut watching, info, &path);
    assert!(
        matches!(
            watching.app.overlays.conn.mcp_status,
            Some(McpStatus::Listening { writes: true, .. })
        ),
        "the status row did not take the daemon's answer"
    );
    watching.close().await;
}

/// A value the file would refuse is reported, writes nothing and does not reach the daemon.
#[tokio::test]
async fn a_refused_change_leaves_the_file_and_the_daemon_alone() {
    let (daemon, path, watching) = on_a_daemon().await;
    let before = std::fs::read_to_string(&path).expect("the file reads");
    let opener = opener_on_nothing(path.clone());
    let (mut look, _) = look_over(&state_home());

    let watching = taken(
        Intent::Connection(ConnChange::MqttEnabled(true)),
        &opener,
        watching,
        &mut look,
    )
    .await;

    assert_eq!(
        std::fs::read_to_string(&path).expect("the file reads"),
        before
    );
    assert_eq!(daemon.reloads(), 0);
    assert!(watching.app.toast().is_some(), "the refusal was not said");
    watching.close().await;
}

/// A change to `[mqtt]` writes only its keys and never the password the file holds.
#[tokio::test]
async fn an_mqtt_change_keeps_the_password_out_of_every_write() {
    let (daemon, path, watching) = on_a_daemon().await;
    std::fs::write(
        &path,
        "schema = 1\n[mqtt]\nhost = \"broker.example.com\"\npassword = \"fake-secret-pass\"\n",
    )
    .expect("the file is written");
    let opener = opener_on_nothing(path.clone());
    let (mut look, _) = look_over(&state_home());

    let watching = taken(
        Intent::Connection(ConnChange::MqttCommands(true)),
        &opener,
        watching,
        &mut look,
    )
    .await;

    let text = std::fs::read_to_string(&path).expect("the file reads");
    assert!(text.contains("commands = true") && text.contains("password = \"fake-secret-pass\""));
    assert_eq!(daemon.reloads(), 1);
    assert!(watching.app.overlays.conn.mqtt.password_set);
    let shown = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(!shown.contains("fake-secret"));
    watching.close().await;
}

/// `v` reads the token from the store into the category for ten seconds.
#[tokio::test]
async fn revealing_reads_the_token_for_ten_seconds() {
    let (_daemon, path, watching) = on_a_daemon().await;
    let home = state_home();
    let token = token_under(&home);
    let (mut look, _) = look_over(&home);
    let opener = opener_on_nothing(path);

    let watching = taken(Intent::RevealToken, &opener, watching, &mut look).await;

    let now = Instant::now();
    assert!(watching.app.overlays.conn.shown(now) == Some(token.as_str()));
    assert!(
        watching
            .app
            .overlays
            .conn
            .shown(now + connections::REVEAL_FOR + Duration::from_secs(1))
            .is_none()
    );
    watching.close().await;
}

/// `c` sends the OSC 52 sequence of the token through the terminal writer and says only that it
/// was sent: the terminal gives no answer, and the note holds no token.
#[tokio::test]
async fn copying_emits_the_osc_52_sequence_through_the_writer() {
    let (_daemon, path, watching) = on_a_daemon().await;
    let home = state_home();
    let token = token_under(&home);
    let (mut look, sent) = look_over(&home);
    let opener = opener_on_nothing(path);

    let mut watching = taken(Intent::CopyToken, &opener, watching, &mut look).await;
    web_box::settle_copy(&mut watching).await;

    let sent = sent.lock().expect("the record").clone();
    assert_eq!(sent.len(), 1, "the writer took {} sequences", sent.len());
    assert!(sent[0] == connections::osc52(&token));
    assert!(sent[0].starts_with("\x1b]52;c;") && sent[0].ends_with('\x07'));
    let toast = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert_eq!(
        toast,
        "token sent to the terminal's clipboard (OSC 52) — if your terminal ignores it, \
         svolder mcp token prints it"
    );
    assert!(!toast.contains(&token) && !toast.contains("copied"));
    watching.close().await;
}

/// The token is sent plain and wrapped for tmux, handed to the display's tool on its input alone, and a tool
/// that ran is named; no note holds the token, a failing tool is said as a note.
#[tokio::test]
async fn the_token_goes_to_the_terminal_wrapped_and_to_the_tool() {
    use crate::tui::clipboard::Tried;
    let (_daemon, path, watching) = on_a_daemon().await;
    let home = state_home();
    let token = token_under(&home);
    let (mut look, sent) = look_over(&home);
    look.env = Arc::new(|name| (name == "TMUX").then(|| "1".into()));
    let tools = Arc::new(Mutex::new(Vec::<String>::new()));
    let record = Arc::clone(&tools);
    look.copier = Arc::new(move |text| {
        record.lock().unwrap().push(text.to_string());
        Tried::Copied("wl-copy")
    });
    let opener = opener_on_nothing(path);

    let mut watching = taken(Intent::CopyToken, &opener, watching, &mut look).await;
    web_box::settle_copy(&mut watching).await;

    let sent = sent.lock().expect("the record").clone();
    assert!(
        sent[0].starts_with("\x1b]52;") && sent[0].contains("\x07\x1bPtmux;\x1b\x1b]52;"),
        "{sent:?}"
    );
    assert!(sent[0].ends_with("\x1b\\"), "{sent:?}");
    assert_eq!(
        tools.lock().unwrap().as_slice(),
        std::slice::from_ref(&token)
    );
    let toast = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert_eq!(toast, "token copied with wl-copy");

    look.copier = Arc::new(|_| Tried::Failed("xclip did not run: boom".into()));
    let mut watching = taken(Intent::CopyToken, &opener, watching, &mut look).await;
    web_box::settle_copy(&mut watching).await;
    let toast = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert_eq!(
        toast,
        "token sent to the terminal's clipboard (OSC 52); xclip did not run: boom"
    );
    assert!(!toast.contains(&token));
    watching.close().await;
}

/// A writer the terminal refuses is said plainly, without the token.
#[tokio::test]
async fn a_copy_the_writer_refuses_is_reported_without_the_token() {
    let (_daemon, path, watching) = on_a_daemon().await;
    let home = state_home();
    let token = token_under(&home);
    let (mut look, _) = look_over(&home);
    look.clipboard = Box::new(|_| Err(io::Error::other("the terminal is gone")));
    let opener = opener_on_nothing(path);

    let mut watching = taken(Intent::CopyToken, &opener, watching, &mut look).await;
    web_box::settle_copy(&mut watching).await;

    let toast = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(
        toast.contains("could not be sent to the terminal"),
        "{toast}"
    );
    assert!(toast.contains("the terminal is gone"), "{toast}");
    assert!(!toast.contains(&token));
    watching.close().await;
}

/// Rotating replaces the generated token, tells the daemon to reload, and takes the new masked
/// tail in; the new token differs from the old one and is mode 0600.
#[tokio::test]
async fn rotating_replaces_the_token_and_has_the_daemon_reload() {
    let (daemon, path, watching) = on_a_daemon().await;
    let home = state_home();
    let old = token_under(&home);
    let (mut look, _) = look_over(&home);
    let opener = opener_on_nothing(path);

    let mut watching = taken(Intent::RotateToken, &opener, watching, &mut look).await;

    let new = token_under(&home);
    assert!(new != old, "the token was not replaced");
    assert_eq!(daemon.reloads(), 1, "the daemon was not told to reload");
    assert!(
        watching.app.overlays.conn.token
            == TokenView::Present {
                masked: connections::mask(&new),
                source: TokenSource::Generated(token::generated_path_in(Store::MCP, &home)),
            }
    );
    let mode = std::fs::metadata(token::generated_path_in(Store::MCP, &home))
        .expect("the token file exists")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    let toast = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(toast.contains("token replaced"), "{toast}");
    assert!(!toast.contains(&new) && !toast.contains(&old));
    assert!(watching.daemon_version.is_some());
    watching.daemon_version = None;
    watching.close().await;
}

/// A token from the environment is not rotated: the file is untouched, the daemon is not told
/// and the toast names the variable and not its value.
#[tokio::test]
async fn rotating_an_environment_token_is_refused_naming_the_variable() {
    let (daemon, path, watching) = on_a_daemon().await;
    let home = state_home();
    let kept = token_under(&home);
    let (mut look, _) = look_over(&home);
    look.token_site.env = Some(FAKE_ENV_TOKEN.to_string());
    let opener = opener_on_nothing(path);

    let watching = taken(Intent::RotateToken, &opener, watching, &mut look).await;

    assert!(token_under(&home) == kept, "the generated token changed");
    assert_eq!(daemon.reloads(), 0);
    let toast = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(toast.contains("SVOLDER_MCP_TOKEN"), "{toast}");
    assert!(!toast.contains(FAKE_ENV_TOKEN));
    watching.close().await;
}

/// A token from a file is not rotated either, and the toast names the file.
#[tokio::test]
async fn rotating_a_token_file_s_token_is_refused_naming_the_file() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    let home = state_home();
    let file = home.join("secret-token");
    std::fs::write(&file, "fake-file-token-not-a-real-secret\n").expect("the file is written");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).expect("mode");
    watching.app.overlays.conn.mcp.token_file = Some(file.display().to_string());
    let (mut look, _) = look_over(&home);
    let opener = opener_on_nothing(path);

    let watching = taken(Intent::RotateToken, &opener, watching, &mut look).await;

    assert_eq!(
        std::fs::read_to_string(&file).expect("the file reads"),
        "fake-file-token-not-a-real-secret\n"
    );
    assert_eq!(daemon.reloads(), 0);
    let toast = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(toast.contains(&file.display().to_string()), "{toast}");
    assert!(!toast.contains("fake-file-token"));
    watching.close().await;
}

/// Looking never makes a token: with none yet, the category says so and the store stays empty.
#[tokio::test]
async fn reading_the_category_never_makes_a_token() {
    let (_daemon, path, mut watching) = on_a_daemon().await;
    let home = state_home();
    let opener = opener_on_nothing(path);

    read(
        &opener,
        &mut watching,
        &TokenSite {
            env: None,
            env_error: None,
            web_env: None,
            web_env_error: None,
            state_home: home.clone(),
        },
    );

    assert!(watching.app.overlays.conn.token == TokenView::Missing);
    assert!(!token::generated_path_in(Store::MCP, &home).exists());
    watching.close().await;
}

/// A daemon of another version is warned of in a toast that names both and the way out: an
/// older one is restarted, and for a newer one it is the TUI that is.
#[tokio::test]
async fn a_daemon_of_another_version_is_warned_of_with_the_way_out() {
    let own = env!("CARGO_PKG_VERSION");
    let (daemon, path, mut watching) = on_a_daemon().await;
    for (version, said) in [
        (
            "0.0.1",
            format!("daemon v0.0.1 is older than this svolder v{own}: svolder daemon restart"),
        ),
        (
            "9.9.9",
            format!("daemon v9.9.9 is newer than this svolder v{own}: restart svolder"),
        ),
    ] {
        daemon.version(version);
        watching.daemon_version = ask_daemon(watching.backend.as_ref());
        let info = recv_version(&mut watching.daemon_version).await;
        daemon_answered(&mut watching, info, &path);
        assert_eq!(
            watching.app.toast().map(|t| t.text.as_str()),
            Some(said.as_str())
        );
    }
    watching.close().await;
}

/// A daemon that reads another configuration file makes the category read-only once it has
/// answered.
#[tokio::test]
async fn a_daemon_reading_another_file_is_taken_as_read_only() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    daemon.says(None, Some("/config/svolder/config.toml"));
    watching.daemon_version = ask_daemon(watching.backend.as_ref());
    let info = recv_version(&mut watching.daemon_version).await;
    daemon_answered(&mut watching, info, &path);
    assert_eq!(
        watching.app.overlays.conn.elsewhere.as_deref(),
        Some("/config/svolder/config.toml")
    );

    daemon.says(None, Some(&path.display().to_string()));
    watching.daemon_version = ask_daemon(watching.backend.as_ref());
    let info = recv_version(&mut watching.daemon_version).await;
    daemon_answered(&mut watching, info, &path);
    assert!(watching.app.overlays.conn.elsewhere.is_none());
    watching.close().await;
}

/// A non-UTF-8 `SVOLDER_MCP_TOKEN` is the same error `resolve` gives, for every read of the
/// token, and never falls back to the generated file.
#[tokio::test]
async fn an_unreadable_environment_token_is_an_error_not_a_fallback() {
    let (daemon, path, watching) = on_a_daemon().await;
    let home = state_home();
    let kept = token_under(&home);
    let (mut look, sent) = look_over(&home);
    look.token_site.env_error = Some("SVOLDER_MCP_TOKEN is not valid UTF-8".into());
    let opener = opener_on_nothing(path);

    let mut watching = watching;
    for intent in [Intent::RevealToken, Intent::CopyToken, Intent::RotateToken] {
        watching = taken(intent, &opener, watching, &mut look).await;
        let toast = watching
            .app
            .toast()
            .map(|t| t.text.clone())
            .unwrap_or_default();
        assert_eq!(toast, "SVOLDER_MCP_TOKEN is not valid UTF-8");
    }
    assert!(watching.app.overlays.conn.shown(Instant::now()).is_none());
    assert!(sent.lock().expect("the record").is_empty());
    assert!(token_under(&home) == kept);
    assert_eq!(daemon.reloads(), 0);
    watching.close().await;
}

/// A rotation whose reload cannot reach the daemon says the daemon was not told, and names the
/// command that does.
#[tokio::test]
async fn a_rotation_the_daemon_was_not_told_of_says_so() {
    let (daemon, path, watching) = on_a_daemon().await;
    let home = state_home();
    token_under(&home);
    let (mut look, _) = look_over(&home);
    let opener = opener_on_nothing(path);
    drop(daemon);

    let watching = taken(Intent::RotateToken, &opener, watching, &mut look).await;

    let toast = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(toast.contains("daemon was not told"), "{toast}");
    assert!(toast.contains("svolder daemon restart"), "{toast}");
    watching.close().await;
}

/// On a direct session a rotation says no daemon uses the token, not that clients need it.
#[tokio::test]
async fn a_rotation_with_no_daemon_says_none_is_running() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let watching = watching(backend, app, None);
    let home = state_home();
    token_under(&home);
    let (mut look, _) = look_over(&home);
    let opener = opener_on_nothing(scratch_config());

    let watching = taken(Intent::RotateToken, &opener, watching, &mut look).await;

    let toast = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(toast.contains("no daemon is running"), "{toast}");
    assert!(!toast.contains("every client"), "{toast}");
    watching.close().await;
}

/// A write's own re-ask is kept when the category is read right after: the read asks nothing
/// more while one is in flight.
#[tokio::test]
async fn a_write_s_re_ask_is_not_replaced_by_a_read() {
    let (daemon, path, watching) = on_a_daemon().await;
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let mut watching = taken(
        Intent::Connection(ConnChange::McpWrites(true)),
        &opener,
        watching,
        &mut look,
    )
    .await;
    assert!(watching.daemon_version.is_some());

    read(&opener, &mut watching, &look.token_site);

    let info = recv_version(&mut watching.daemon_version).await;
    assert!(info.is_some(), "the write's ask was dropped");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(daemon.version_asks(), 1, "the read asked again");
    watching.close().await;
}

/// The daemon's token description arrives with its answer: a daemon using the environment's
/// token against a session reading its own state file makes the row the daemon's.
#[tokio::test]
async fn the_daemons_token_description_lands_with_its_answer() {
    use crate::tui::boxes::connections::TokenState;
    use svolder_ipc::{TokenInfo, TokenKind};

    let (daemon, path, mut watching) = on_a_daemon().await;
    let home = state_home();
    token_under(&home);
    let opener = opener_on_nothing(path.clone());
    read(
        &opener,
        &mut watching,
        &TokenSite {
            env: None,
            env_error: None,
            web_env: None,
            web_env_error: None,
            state_home: home,
        },
    );
    let info = recv_version(&mut watching.daemon_version).await;
    daemon_answered(&mut watching, info, &path);
    assert_eq!(watching.app.overlays.conn.token_state(), TokenState::Own);

    daemon.says_token(Some(TokenInfo {
        kind: TokenKind::Environment,
        path: None,
        tail: "9xZ".into(),
    }));
    watching.daemon_version = ask_daemon(watching.backend.as_ref());
    let info = recv_version(&mut watching.daemon_version).await;
    daemon_answered(&mut watching, info, &path);
    assert!(matches!(
        watching.app.overlays.conn.token_state(),
        TokenState::Differs { .. }
    ));
    watching.close().await;
}

/// A token made by the daemon after this session read the category (the session saw none) is
/// the one the session reads: the daemon's answer naming the generated file ends with the row
/// the session's own, and the copy key works.
#[tokio::test]
async fn a_token_the_daemon_made_in_the_file_this_session_reads_is_own() {
    use crate::tui::boxes::connections::TokenState;
    use svolder_ipc::{TokenInfo, TokenKind};

    let (daemon, path, mut watching) = on_a_daemon().await;
    let home = state_home();
    let opener = opener_on_nothing(path.clone());
    let (mut look, sent) = look_over(&home);
    read(&opener, &mut watching, &look.token_site);
    assert!(watching.app.overlays.conn.token == TokenView::Missing);
    let _ = recv_version(&mut watching.daemon_version).await;

    // The daemon turns on, makes the token where this session reads, and says so.
    let value = token_under(&home);
    daemon.says_token(Some(TokenInfo {
        kind: TokenKind::State,
        path: Some(
            token::generated_path_in(Store::MCP, &home)
                .display()
                .to_string(),
        ),
        tail: token::tail(&value),
    }));
    watching.daemon_version = ask_daemon(watching.backend.as_ref());
    let info = recv_version(&mut watching.daemon_version).await;
    super::answered(&mut watching, info, &path, &look.token_site);

    assert_eq!(watching.app.overlays.conn.token_state(), TokenState::Own);
    let watching = taken(Intent::CopyToken, &opener, watching, &mut look).await;
    assert_eq!(sent.lock().expect("the record").len(), 1);
    assert!(
        watching
            .app
            .toast()
            .is_some_and(|t| t.text == "copying the token…")
    );
    watching.close().await;
}

/// A change asked for before the daemon said it reads another file writes nothing and does not
/// reach the daemon.
#[tokio::test]
async fn a_change_carried_out_after_an_elsewhere_answer_writes_nothing() {
    let (daemon, path, mut watching) = on_a_daemon().await;
    let before = std::fs::read_to_string(&path).expect("the file reads");
    let opener = opener_on_nothing(path.clone());
    let (mut look, _) = look_over(&state_home());
    watching.app.overlays.conn.elsewhere = Some("/config/svolder/config.toml".into());

    let watching = taken(
        Intent::Connection(ConnChange::McpWrites(true)),
        &opener,
        watching,
        &mut look,
    )
    .await;

    assert_eq!(
        std::fs::read_to_string(&path).expect("the file reads"),
        before
    );
    assert_eq!(daemon.reloads(), 0);
    let toast = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(toast.contains("read-only here"), "{toast}");
    watching.close().await;
}

/// A rotation carried out while the daemon uses another token replaces nothing.
#[tokio::test]
async fn a_rotation_carried_out_after_a_differs_answer_replaces_nothing() {
    use svolder_ipc::{TokenInfo, TokenKind};

    let (daemon, path, mut watching) = on_a_daemon().await;
    let home = state_home();
    let kept = token_under(&home);
    let opener = opener_on_nothing(path.clone());
    let (mut look, _) = look_over(&home);
    read(&opener, &mut watching, &look.token_site);
    watching.app.overlays.conn.daemon_token = Some(TokenInfo {
        kind: TokenKind::Environment,
        path: None,
        tail: "9xZ".into(),
    });

    let watching = taken(Intent::RotateToken, &opener, watching, &mut look).await;

    assert!(token_under(&home) == kept, "the token was replaced");
    assert_eq!(daemon.reloads(), 0);
    let toast = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(toast.contains("not this session's token"), "{toast}");
    watching.close().await;
}
