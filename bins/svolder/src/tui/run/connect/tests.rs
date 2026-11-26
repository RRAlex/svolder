// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The loop's side of the Connect category, over a stub daemon and token files in a scratch
//! directory of each case's own. Tokens are fake and compared with `assert!`, never printed.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex};

use svolder_ipc::McpStatus;
use svolder_ipc::config::ListenDefaults;
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
    let config = Config::parse_with_listen_defaults(&text, &ListenDefaults::default())
        .expect("the file parses");
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

/// With a daemon that has `mqtt_set`, every `[mqtt]` change is the daemon's one request: this side
/// writes nothing, and the password goes in the request and in no toast, row or `Debug`.
#[tokio::test]
async fn with_a_daemon_the_mqtt_rows_write_through_mqtt_set() {
    let (daemon, path, watching) = on_a_daemon().await;
    daemon.lists_features(&[svolder_ipc::FEATURE_MQTT_SET]);
    std::fs::write(&path, "schema = 1\n[mqtt]\nusername = \"filed\"\n").unwrap();
    let before = std::fs::read_to_string(&path).expect("the file reads");
    let opener = opener_on_nothing(path.clone());
    let (mut look, _) = look_over(&state_home());

    let mut watching = watching;
    for change in [
        ConnChange::MqttHost("broker.example.com".into()),
        ConnChange::MqttPort(1884),
        ConnChange::MqttUsername(Some("svolder".into())),
        ConnChange::MqttPassword(Some("fake-secret-pass".to_string().into())),
        ConnChange::MqttCommands(true),
        ConnChange::MqttEnabled(true),
    ] {
        watching = taken(Intent::Connection(change), &opener, watching, &mut look).await;
    }

    use svolder_ipc::MqttOp;
    assert_eq!(
        daemon.mqtt_ops(),
        [
            MqttOp::Host {
                value: "broker.example.com".into()
            },
            MqttOp::Port { value: 1884 },
            MqttOp::Username {
                value: Some("svolder".into())
            },
            MqttOp::Password {
                value: Some("fake-secret-pass".into())
            },
            MqttOp::Commands { commands: true },
            MqttOp::Enabled { enabled: true },
        ]
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("the file reads"),
        before,
        "the daemon is the one writer"
    );
    assert_eq!(daemon.reloads(), 0, "the daemon reloads itself");
    let shown = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(!shown.contains("fake-secret"), "{shown}");
    assert!(!format!("{:?}", watching.app.overlays.conn).contains("fake-secret"));
    watching.close().await;
}

/// With no daemon the rows write the file themselves, keep every other key, and the password is
/// in the file and in nothing the app says; an empty change clears it.
#[tokio::test]
async fn with_no_daemon_the_mqtt_rows_write_the_file() {
    let (backend, _gate, app) = fake(std::time::Duration::from_secs(2)).await;
    let watching = watching(backend, app, None);
    let path = scratch_config();
    Config::update_mcp(&path, |_| {}).expect("the scratch config is made");
    std::fs::write(&path, "schema = 1\n[mqtt]\nhost = \"broker.example.com\"\n")
        .expect("the file is written");
    let opener = opener_on_nothing(path.clone());
    let (mut look, _) = look_over(&state_home());

    let watching = taken(
        Intent::Connection(ConnChange::MqttUsername(Some("svolder".into()))),
        &opener,
        watching,
        &mut look,
    )
    .await;
    let watching = taken(
        Intent::Connection(ConnChange::MqttPassword(Some(
            "fake-secret-pass".to_string().into(),
        ))),
        &opener,
        watching,
        &mut look,
    )
    .await;
    let text = std::fs::read_to_string(&path).expect("the file reads");
    assert!(text.contains("username = \"svolder\""), "{text}");
    assert!(text.contains("password = \"fake-secret-pass\""), "{text}");
    assert!(text.contains("host = \"broker.example.com\""));
    let conn = &watching.app.overlays.conn;
    assert_eq!(conn.mqtt.username.as_deref(), Some("svolder"));
    assert!(conn.mqtt.password_in_file);
    assert!(!format!("{conn:?}").contains("fake-secret"));
    let shown = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(!shown.contains("fake-secret"), "{shown}");

    let watching = taken(
        Intent::Connection(ConnChange::MqttPassword(None)),
        &opener,
        watching,
        &mut look,
    )
    .await;
    assert!(
        !std::fs::read_to_string(&path)
            .unwrap()
            .contains("fake-secret")
    );
    assert!(!watching.app.overlays.conn.mqtt.password_in_file);
    watching.close().await;
}

/// A password written into a file others can read says the file is now 0600.
#[tokio::test]
async fn a_password_into_a_readable_file_is_said_to_tighten_it() {
    use std::os::unix::fs::PermissionsExt;
    let (backend, _gate, app) = fake(std::time::Duration::from_secs(2)).await;
    let watching = watching(backend, app, None);
    let path = scratch_config();
    Config::update_mcp(&path, |_| {}).expect("the scratch config is made");
    std::fs::write(&path, "schema = 1\n").expect("the file is written");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let opener = opener_on_nothing(path.clone());
    let (mut look, _) = look_over(&state_home());
    let watching = taken(
        Intent::Connection(ConnChange::MqttPassword(Some("p".to_string().into()))),
        &opener,
        watching,
        &mut look,
    )
    .await;
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let shown = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(shown.contains("0600"), "{shown}");
    watching.close().await;
}

/// `v` reads the token from the store into the category for a minute.
#[tokio::test]
async fn revealing_reads_the_token_for_a_minute() {
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

/// `v` with no token to read closes the box and says so: no empty box stays open.
#[tokio::test]
async fn revealing_with_no_token_closes_the_box_and_says_so() {
    let (_daemon, path, mut watching) = on_a_daemon().await;
    let home = state_home();
    let (mut look, _) = look_over(&home);
    let opener = opener_on_nothing(path);
    watching.app.overlays.conn.modal = Some(connections::Modal::Token);

    let watching = taken(Intent::RevealToken, &opener, watching, &mut look).await;

    assert!(watching.app.overlays.conn.modal.is_none());
    assert!(watching.app.overlays.conn.revealed.is_none());
    let toast = watching.app.toast().expect("a toast").text.clone();
    assert!(toast.contains("no token yet"), "{toast}");
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

/// A daemon that narrowed the file's mode says so in a toast, and the password is not in it.
#[tokio::test]
async fn a_narrowed_mode_the_daemon_reports_is_a_toast() {
    let (daemon, path, watching) = on_a_daemon().await;
    daemon.lists_features(&[svolder_ipc::FEATURE_MQTT_SET]);
    daemon.tightens_on_password();
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let watching = taken(
        Intent::Connection(ConnChange::MqttPassword(Some(
            "fake-secret".to_string().into(),
        ))),
        &opener,
        watching,
        &mut look,
    )
    .await;
    let shown = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(shown.contains("0600"), "{shown}");
    assert!(!shown.contains("fake-secret"));
    watching.close().await;
}

/// The Web UI and MCP server rows show what the daemon's switch wrote: after each answer the
/// row reads the new state, and a second press turns it off; a refused switch leaves the row and
/// says why.
#[tokio::test]
async fn a_server_switch_refreshes_its_row_from_the_file_it_wrote() {
    let (daemon, path, watching) = on_a_daemon().await;
    daemon.lists_features(&["server_switch"]);
    daemon.switches_write(&path);
    let opener = opener_on_nothing(path.clone());
    let (mut look, _) = look_over(&state_home());

    let watching = taken(
        Intent::Connection(ConnChange::McpEnabled(true)),
        &opener,
        watching,
        &mut look,
    )
    .await;
    assert!(
        watching.app.overlays.conn.mcp.enabled,
        "the MCP row stayed off after the switch"
    );
    let watching = taken(
        Intent::Connection(ConnChange::McpEnabled(false)),
        &opener,
        watching,
        &mut look,
    )
    .await;
    assert!(!watching.app.overlays.conn.mcp.enabled);

    let watching = taken(
        Intent::Connection(ConnChange::WebEnabled(true)),
        &opener,
        watching,
        &mut look,
    )
    .await;
    assert!(
        watching.app.overlays.conn.web.enabled,
        "the web row stayed off after the switch"
    );
    let mut watching = taken(
        Intent::Connection(ConnChange::WebEnabled(false)),
        &opener,
        watching,
        &mut look,
    )
    .await;
    assert!(!watching.app.overlays.conn.web.enabled);

    daemon.answers_switch(svolder_ipc::Response::error(
        svolder_ipc::CODE_USAGE,
        "the MCP server cannot start",
    ));
    watching.app.dismiss_toast();
    let watching = taken(
        Intent::Connection(ConnChange::McpEnabled(true)),
        &opener,
        watching,
        &mut look,
    )
    .await;
    assert!(!watching.app.overlays.conn.mcp.enabled);
    let said = watching
        .app
        .toast()
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(said.contains("cannot start"), "{said}");
    watching.close().await;
}

// The Bluetooth adapter list's loop side: the scan that fills it and the pick that saves.

use super::super::adapters;

/// The unit and name a case configures, so the pick has a device to remember an adapter for.
fn bf3() -> svolder_ipc::DeviceConfig {
    svolder_ipc::DeviceConfig {
        name: "BF3".into(),
        udid: "0A1B2C3D4E5F6071".into(),
        profile: None,
        passphrase: "auto".into(),
        inputs: Default::default(),
    }
}

/// Picking an adapter under a running daemon writes the address into the file as `add` writes a
/// device, tells the daemon to reload, makes the picked adapter the one the state file remembers
/// for the device, and says the daemon takes it up when it restarts.
#[tokio::test]
async fn picking_an_adapter_under_a_daemon_writes_its_address_and_tells_the_daemon() {
    let (daemon, path, watching) = on_a_daemon().await;
    daemon.lists_features(&[svolder_ipc::FEATURE_ADAPTER_SWITCH]);
    daemon.holds_adapter("hci2", false);
    let device = bf3();
    Config::add_device(&path, &device).expect("the device is added");
    let opener = opener_on_nothing(path.clone());
    let home = state_home();
    let (mut look, _) = look_over(&home);

    let watching = taken(
        Intent::PickAdapter {
            address: "AA:BB:CC:DD:EE:02".into(),
            name: "hci2".into(),
        },
        &opener,
        watching,
        &mut look,
    )
    .await;

    let config = Config::load_from(&path).expect("the file parses");
    assert_eq!(config.adapter.as_deref(), Some("AA:BB:CC:DD:EE:02"));
    assert_eq!(daemon.reloads(), 1, "the daemon was not told to reload");
    assert_eq!(
        watching.app.overlays.conn.adapter.as_deref(),
        Some("AA:BB:CC:DD:EE:02"),
        "the row does not say what is on disk"
    );
    assert_eq!(
        crate::state::DeviceState::load_from(&home)
            .last_adapter("0A1B2C3D4E5F6071", Some("AA:BB:CC:DD:EE:02")),
        Some("AA:BB:CC:DD:EE:02".to_string()),
        "the remembered adapter would outrank the pick, and a kernel number moves"
    );
    let words = watching
        .app
        .toast()
        .expect("the pick was said")
        .text
        .clone();
    assert!(words.contains("the daemon switched to it"), "{words}");
    watching.close().await;
}

/// What the toast says after the pick of `hci2` under a daemon that reports it holds `holds`.
async fn said_after_picking_under(holds: &str, pinned: bool) -> String {
    let (daemon, path, watching) = on_a_daemon().await;
    daemon.lists_features(&[svolder_ipc::FEATURE_ADAPTER_SWITCH]);
    daemon.holds_adapter(holds, pinned);
    Config::add_device(&path, &bf3()).expect("the device is added");
    let opener = opener_on_nothing(path);
    let (mut look, _) = look_over(&state_home());
    let watching = taken(
        Intent::PickAdapter {
            address: "AA:BB:CC:DD:EE:02".into(),
            name: "hci2".into(),
        },
        &opener,
        watching,
        &mut look,
    )
    .await;
    let words = watching
        .app
        .toast()
        .expect("the pick was said")
        .text
        .clone();
    watching.close().await;
    words
}

/// A daemon pinned by its command line or `SVOLDER_ADAPTER` ignores the file's adapter: the toast
/// says it stays where it is, not that it switched, and does not add the TUI's own `--adapter`
/// to that.
#[tokio::test]
async fn a_pick_under_a_pinned_daemon_says_the_daemon_is_pinned() {
    let words = said_after_picking_under("hci0", true).await;
    assert!(!words.contains("switched"), "{words}");
    assert!(
        words.contains("the daemon is pinned to hci0 by --adapter or SVOLDER_ADAPTER"),
        "{words}"
    );
}

/// A daemon that could not open the picked adapter stays on the one it has, and the toast says so.
#[tokio::test]
async fn a_pick_the_daemon_could_not_open_says_it_stayed() {
    let words = said_after_picking_under("hci0", false).await;
    assert!(!words.contains("switched"), "{words}");
    assert!(words.contains("the daemon stays on hci0"), "{words}");
}

/// Picking an adapter in a direct session writes the file alone and reconnects through it: the
/// device is opened again.
#[tokio::test]
async fn picking_an_adapter_in_a_direct_session_reconnects() {
    let (backend, gate, mut app) = fake(SHORT_DEADLINE).await;
    app.session.device_name = "BF3".into();
    let watching = watching(backend, app, Some("BF3"));
    let path = scratch_config();
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("the directory is made");
    std::fs::write(&path, Config::template()).expect("the template is written");
    let log = Log::default();
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        path.clone(),
        fake_open(Arc::clone(&log), gate, &[]),
    );
    let (mut look, _) = look_over(&state_home());

    let watching = taken(
        Intent::PickAdapter {
            address: "AA:BB:CC:DD:EE:01".into(),
            name: "hci0".into(),
        },
        &opener,
        watching,
        &mut look,
    )
    .await;

    let config = Config::load_from(&path).expect("the file parses");
    assert_eq!(config.adapter.as_deref(), Some("AA:BB:CC:DD:EE:01"));
    assert_eq!(
        opens(&log),
        vec![(Some("BF3".to_string()), true)],
        "the device was not opened again, the old session released first"
    );
    watching.close().await;
}

/// The list's scan runs in a task and comes back through the loop: over the fake transport,
/// one adapter that hears each configured device.
#[tokio::test]
async fn the_lists_scan_comes_back_as_rows() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut watching = watching(backend, app, None);
    let path = scratch_config();
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("the directory is made");
    std::fs::write(
        &path,
        "schema = 1\n[[device]]\nname = \"BF3\"\nudid = \"0A1B2C3D4E5F6071\"\n",
    )
    .expect("the file is written");
    let opener = opener_on_nothing(path);

    adapters::list(&opener, &mut watching);
    let found = adapters::recv(&mut watching.adapters_scan)
        .await
        .expect("the scan answered")
        .expect("the fake transport lists its adapters");

    assert_eq!(found.rows.len(), 1);
    assert_eq!(found.rows[0].heard[0].0, "BF3");
    assert!(found.rows[0].in_use, "the only adapter is the one in use");
    watching.close().await;
}

/// A daemon that cannot switch is told so in words that name the restart.
#[tokio::test]
async fn a_daemon_too_old_to_switch_is_told_to_restart() {
    let (daemon, path, watching) = on_a_daemon().await;
    daemon.lists_features(&[]);
    let opener = opener_on_nothing(path.clone());
    let (mut look, _) = look_over(&state_home());

    let watching = taken(
        Intent::PickAdapter {
            address: "AA:BB:CC:DD:EE:02".into(),
            name: "hci2".into(),
        },
        &opener,
        watching,
        &mut look,
    )
    .await;

    let words = watching.app.toast().expect("said").text.clone();
    assert!(words.contains("svolder daemon restart"), "{words}");
    watching.close().await;
}
