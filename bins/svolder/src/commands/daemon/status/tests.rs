// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daemon status` against a fake `systemctl` and daemons that answer on a socket, or never do.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use svolder_ipc::socket::daemon_version::{Skew, mismatch_line, skew};
use svolder_ipc::{Event, Handler};

use super::*;
use crate::commands::daemon::POLL;
use crate::commands::daemon::test_support::{
    FAKE_LOG, FakeDaemon, Tools, VERSION, a_daemon_on, installed_units, no_units,
};
use crate::test_support::temp_dir;

/// A device's line names the product either way, and says how long the daemon is leaving
/// a device that is away alone for when the daemon reports a wait. A daemon too old to
/// report one is printed as it always was.
#[test]
fn a_device_line_says_how_long_the_daemon_is_waiting_before_it_tries_again() {
    let mut device = DeviceInfo {
        signal: None,
        name: "bf3".into(),
        unit: "0102030405060708".into(),
        product_id: 3,
        product: Some("Gungnir 2".into()),
        firmware: 1,
        profile: "gungnir-2".into(),
        connected: true,
        stale: false,
        retry_in_secs: None,
        passphrase_note: None,
        refused: false,
        recalled: None,
    };
    assert_eq!(device_line(&device), "device bf3: connected (Gungnir 2)");

    device.connected = false;
    assert_eq!(
        device_line(&device),
        "device bf3: not connected (Gungnir 2)"
    );

    device.retry_in_secs = Some(15);
    assert_eq!(
        device_line(&device),
        "device bf3: not connected, next attempt in 15 s (Gungnir 2)"
    );

    // A device that never advertised a product name is named by its unit id, as ever.
    device.product = None;
    assert_eq!(
        device_line(&device),
        "device bf3: not connected, next attempt in 15 s (0102030405060708)"
    );
}

/// The bridge's line in each state it reports.
#[test]
fn the_mqtt_line_says_where_the_bridge_stands() {
    let host = || "192.168.1.10".to_string();
    for (status, line) in [
        (MqttStatus::Off, "mqtt: off"),
        (
            MqttStatus::Connecting {
                host: host(),
                port: 1883,
                retry_in_secs: None,
            },
            "mqtt: connecting to 192.168.1.10:1883",
        ),
        (
            MqttStatus::Connecting {
                host: host(),
                port: 1883,
                retry_in_secs: Some(8),
            },
            "mqtt: connecting to 192.168.1.10:1883 (retry in 8 s)",
        ),
        (
            MqttStatus::Connected {
                host: host(),
                port: 1883,
                commands: false,
            },
            "mqtt: connected to 192.168.1.10:1883 (read-only)",
        ),
        (
            MqttStatus::Connected {
                host: host(),
                port: 8883,
                commands: true,
            },
            "mqtt: connected to 192.168.1.10:8883 (commands on)",
        ),
        (MqttStatus::Unknown, "mqtt: unknown"),
    ] {
        assert_eq!(mqtt_line(&status), line);
    }
}

/// The MCP server's line in every state, in the words the daemon's log uses too.
#[test]
fn the_mcp_line_says_where_the_server_stands() {
    let listen: std::net::SocketAddr = "127.0.0.1:8737".parse().unwrap();
    let token = svolder_ipc::config::McpAuth::Token;
    let none = svolder_ipc::config::McpAuth::None;
    for (status, line) in [
        (McpStatus::Off, "mcp: off"),
        (
            McpStatus::Listening {
                listen,
                auth: token,
                writes: false,
            },
            "mcp: listening on 127.0.0.1:8737 (token, read-only)",
        ),
        (
            McpStatus::Listening {
                listen,
                auth: token,
                writes: true,
            },
            "mcp: listening on 127.0.0.1:8737 (token, writes)",
        ),
        (
            McpStatus::Listening {
                listen,
                auth: none,
                writes: false,
            },
            "mcp: listening on 127.0.0.1:8737 (no auth, read-only)",
        ),
        (
            McpStatus::Failed {
                listen,
                error: "Address already in use (os error 98)".into(),
            },
            "mcp: cannot listen on 127.0.0.1:8737: Address already in use (os error 98)",
        ),
        (McpStatus::Unknown, "mcp: unknown"),
    ] {
        assert_eq!(status.line(), line);
    }
}

/// A daemon too old to report a bridge gets no `mqtt:` line at all.
#[tokio::test]
async fn status_prints_no_mqtt_line_for_a_daemon_that_reports_none() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-status-no-bridge");
    let socket = dir.0.join("svolder.sock");
    let served = a_daemon_on(&socket).await;

    let lines = status_lines(&tools.ctl(), &socket, &dir.0.join("svolderd.log"), &[]).await;
    assert!(
        lines.iter().any(|line| line.starts_with("daemon:")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("mqtt:")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("mcp:")),
        "{lines:?}"
    );

    let mut client = Client::connect(&socket).await.expect("the daemon answers");
    let _ = client.request(Request::Stop).await;
    let _ = served.await;
}

/// `status` on a machine systemd runs the daemon on prints both units' states, and the
/// daemon's own version and devices when it answers.
#[tokio::test]
async fn status_prints_the_unit_states_and_what_the_daemon_answers() {
    let tools = Tools::new(&installed_units("/run/user/1000/svolder.sock"));
    let dir = temp_dir("daemon-status");
    let socket = dir.0.join("svolder.sock");
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let served = tokio::spawn({
        let socket = socket.clone();
        async move {
            let _ = svolder_ipc::serve(&socket, Arc::new(FakeDaemon), async {
                let _ = stopped.await;
            })
            .await;
        }
    });
    for _ in 0..100 {
        if svolder_ipc::is_running(&socket).await {
            break;
        }
        tokio::time::sleep(POLL).await;
    }

    let lines = status_lines(&tools.ctl(), &socket, &dir.0.join("svolderd.log"), &[]).await;
    assert!(
        lines
            .iter()
            .any(|line| line == &format!("{SOCKET_UNIT}: installed, enabled, active")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with(&format!("{SERVICE_UNIT}: installed"))),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains(VERSION)),
        "the daemon's version: {lines:?}"
    );
    // The mismatch line follows the daemon's own, and the bridge's line that.
    let daemon = lines
        .iter()
        .position(|line| line.starts_with("daemon:"))
        .expect("the daemon's line");
    assert_eq!(
        lines.get(daemon + 1).map(String::as_str),
        Some(
            format!(
                "mismatch: daemon v{VERSION} is older than this svolder v{}: svolder daemon restart",
                env!("CARGO_PKG_VERSION")
            )
            .as_str()
        ),
        "{lines:?}"
    );
    assert_eq!(
        lines.get(daemon + 2).map(String::as_str),
        Some("mqtt: connected to 192.168.1.10:1883 (read-only)"),
        "{lines:?}"
    );
    assert_eq!(
        lines.get(daemon + 3).map(String::as_str),
        Some("mcp: listening on 127.0.0.1:8737 (token, read-only)"),
        "{lines:?}"
    );
    assert_eq!(
        lines.get(daemon + 4).map(String::as_str),
        Some("web: listening on 127.0.0.1:8738 (token, writes)"),
        "{lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("zQ9") || line.contains("mcp-token")),
        "the token description reached the status text: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("device fake: connected")),
        "the devices it holds: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line == "device away: not connected, next attempt in 15 s (Bifrost 3)"),
        "the wait a device that is away is being left alone for: {lines:?}"
    );
    assert!(
        lines.contains(&format!("log: {FAKE_LOG}")),
        "where the daemon says it logs: {lines:?}"
    );

    let _ = stop.send(());
    let _ = served.await;
}

/// Without a daemon answering, `status` names the log a daemon nobody installed writes.
#[tokio::test]
async fn status_without_a_daemon_names_the_socket_and_the_fallback_log() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-status-bare");
    let socket = dir.0.join("svolder.sock");
    let log = dir.0.join("svolderd.log");

    let lines = status_lines(&tools.ctl(), &socket, &log, &[]).await;
    assert!(
        lines.iter().any(|line| line.contains("nothing answers")),
        "{lines:?}"
    );
    assert!(
        lines.contains(&format!("log: {}", log.display())),
        "{lines:?}"
    );
}

/// An answering daemon's own log wins: its path, or the journal; an older one that names none
/// gets the fallback path where systemd does not manage the daemon.
#[tokio::test]
async fn status_prints_the_log_a_daemon_reports_else_the_fallback() {
    let fallback = "/home/user/fallback/svolderd.log";
    for (reported, expected) in [
        (Some(FAKE_LOG), format!("log: {FAKE_LOG}")),
        (Some("journal"), "log: the journal".to_string()),
        (None, format!("log: {fallback}")),
    ] {
        let tools = Tools::new(&no_units());
        let dir = temp_dir("daemon-status-log");
        let socket = dir.0.join("svolder.sock");
        let (stop, served) = a_logging_daemon(&socket, reported).await;

        let lines = status_lines(&tools.ctl(), &socket, Path::new(fallback), &[]).await;
        let found: Vec<&String> = lines.iter().filter(|l| l.starts_with("log:")).collect();
        assert_eq!(found, [&expected], "{lines:?}");

        let _ = stop.send(());
        let _ = served.await;
    }
}

/// A daemon of a given version that answers a `version` naming `log` as where it writes.
struct LogDaemon(&'static str, Option<&'static str>);

#[async_trait::async_trait]
impl Handler for LogDaemon {
    async fn handle(&self, _request: Request) -> Response {
        Response::Version {
            version: self.0.to_string(),
            mqtt: None,
            mcp: None,
            config: None,
            log: self.1.map(str::to_string),
            mcp_token: None,
            web: None,
            web_token: None,
        }
    }

    async fn subscribe(
        &self,
        _device: Option<String>,
    ) -> futures::stream::BoxStream<'static, Event> {
        futures::stream::pending().boxed()
    }
}

/// Serve a [`LogDaemon`] of the test version on `socket` until the returned sender fires.
async fn a_logging_daemon(
    socket: &Path,
    log: Option<&'static str>,
) -> (
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    a_daemon_of(socket, VERSION, log).await
}

/// Serve a [`LogDaemon`] of `version` on `socket` until the returned sender fires.
async fn a_daemon_of(
    socket: &Path,
    version: &'static str,
    log: Option<&'static str>,
) -> (
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<()>,
) {
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let served = tokio::spawn({
        let socket = socket.to_path_buf();
        async move {
            let _ = svolder_ipc::serve(&socket, Arc::new(LogDaemon(version, log)), async {
                let _ = stopped.await;
            })
            .await;
        }
    });
    for _ in 0..100 {
        if svolder_ipc::is_running(socket).await {
            break;
        }
        tokio::time::sleep(POLL).await;
    }
    (stop, served)
}

/// Every other svolderd of this user's is named after the daemon this session reaches,
/// with the command that stops that one — and neither the daemon this session reaches nor
/// one whose socket cannot be told is among them.
#[tokio::test]
async fn status_names_every_other_svolderd_of_this_user_and_what_stops_each() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-status-others");
    let socket = dir.0.join("svolder.sock");
    let elsewhere = PathBuf::from("/tmp/svolder-firstrun/svolder.sock");
    let running = vec![
        RunningDaemon {
            pid: 11,
            socket: Some(socket.clone()),
        },
        RunningDaemon {
            pid: 12,
            socket: None,
        },
        RunningDaemon {
            pid: 4711,
            socket: Some(elsewhere.clone()),
        },
    ];

    let (stop, served) = a_logging_daemon(&socket, Some(FAKE_LOG)).await;
    let lines = status_lines(&tools.ctl(), &socket, &dir.0.join("svolderd.log"), &running).await;
    let _ = stop.send(());
    let _ = served.await;
    let also: Vec<&String> = lines
        .iter()
        .filter(|line| line.starts_with("also running:"))
        .collect();
    assert_eq!(
        also,
        [&format!(
            "also running: svolderd pid 4711 on {path} — stop it with: \
                 svolder --socket {path} daemon stop",
            path = elsewhere.display()
        )],
        "{lines:?}"
    );
    // It reads after the daemon this session asked about, and before the log.
    let at = |what: &str| lines.iter().position(|line| line.starts_with(what));
    assert!(at("daemon:") < at("also running:"), "{lines:?}");
    assert!(at("also running:") < at("log:"), "{lines:?}");
}

/// Versions compare in semver order, and the line `status` adds names which side is older.
#[test]
fn skew_compares_versions_in_semver_order() {
    let own = env!("CARGO_PKG_VERSION");
    assert_eq!(skew(own, own), None);
    assert_eq!(skew("no version (x)", own), None);
    assert_eq!(skew("0.0.1", own), Some(Skew::Older));
    assert_eq!(skew("0.0.1-rc.1", own), Some(Skew::Older));
    assert_eq!(skew("9.9.9", own), Some(Skew::Newer));
    // Numbers compare as numbers, not as text.
    assert_eq!(skew("0.100.0", own), Some(Skew::Newer));
    assert_eq!(skew("0.9.0", own).is_some(), own != "0.9.0");
    // A pre-release sorts before its release.
    let pre = format!("{own}-rc.1");
    assert_eq!(skew(&pre, own), Some(Skew::Older));
    let (core, _) = own.split_once('-').unwrap_or((own, ""));
    if !own.contains('-') {
        let next = format!("{core}.1");
        assert_eq!(skew(&next, own), Some(Skew::Newer));
    }

    assert_eq!(
        mismatch_line("0.0.1", own).as_deref(),
        Some(
            format!(
                "mismatch: daemon v0.0.1 is older than this svolder v{own}: svolder daemon restart"
            )
            .as_str()
        )
    );
    assert_eq!(
        mismatch_line("9.9.9", own).as_deref(),
        Some(
            format!("mismatch: daemon v9.9.9 is newer than this svolder v{own}: restart svolder")
                .as_str()
        )
    );
    assert_eq!(mismatch_line(own, own), None);
}

/// A daemon of this svolder's own version earns no mismatch line.
#[tokio::test]
async fn status_prints_no_mismatch_for_a_daemon_of_its_own_version() {
    struct Same;
    #[async_trait::async_trait]
    impl Handler for Same {
        async fn handle(&self, request: Request) -> Response {
            match request {
                Request::Version => Response::Version {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    mqtt: None,
                    mcp: None,
                    config: None,
                    log: None,
                    mcp_token: None,
                    web: None,
                    web_token: None,
                },
                _ => Response::Ack,
            }
        }

        async fn subscribe(
            &self,
            _device: Option<String>,
        ) -> futures::stream::BoxStream<'static, Event> {
            futures::stream::pending().boxed()
        }
    }
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-status-same");
    let socket = dir.0.join("svolder.sock");
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let served = tokio::spawn({
        let socket = socket.clone();
        async move {
            let _ = svolder_ipc::serve(&socket, Arc::new(Same), async {
                let _ = stopped.await;
            })
            .await;
        }
    });
    for _ in 0..500 {
        if svolder_ipc::is_running(&socket).await {
            break;
        }
        tokio::time::sleep(POLL).await;
    }

    let lines = status_lines(&tools.ctl(), &socket, &dir.0.join("svolderd.log"), &[]).await;
    assert!(
        lines.iter().any(|line| line.starts_with("daemon: ")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("mismatch:")),
        "{lines:?}"
    );
    let _ = stop.send(());
    let _ = served.await;
}

/// A daemon of this svolder's own version that names no log writes no file: `status` says so
/// instead of naming the autospawn file, and an older daemon keeps the fallback line.
#[tokio::test]
async fn status_says_a_current_daemon_that_names_no_log_writes_no_file() {
    let own = env!("CARGO_PKG_VERSION");
    let fallback = "/home/user/fallback/svolderd.log";
    for (version, expected) in [
        (own, format!("log: {NOT_A_FILE}")),
        ("999.0.0", format!("log: {NOT_A_FILE}")),
        ("0.0.1", format!("log: {fallback}")),
    ] {
        let tools = Tools::new(&no_units());
        let dir = temp_dir("daemon-status-nofile");
        let socket = dir.0.join("svolder.sock");
        let (stop, served) = a_daemon_of(&socket, version, None).await;
        let lines = status_lines(&tools.ctl(), &socket, Path::new(fallback), &[]).await;
        let found: Vec<&String> = lines.iter().filter(|l| l.starts_with("log:")).collect();
        assert_eq!(found, [&expected], "{version}: {lines:?}");
        let _ = stop.send(());
        let _ = served.await;
    }
}

/// A daemon on `socket` that takes every connection and reads its requests, answering the
/// first one with `first` (when there is one) and every other never.
fn silent_daemon(socket: &Path, first: Option<svolder_ipc::Response>) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let listener = tokio::net::UnixListener::bind(socket).expect("the socket binds");
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            let (read, mut write) = stream.into_split();
            let mut read = tokio::io::BufReader::new(read);
            let mut line = String::new();
            let _ = read.read_line(&mut line).await;
            if let Some(response) = first.clone() {
                let reply = svolder_ipc::Reply::Response { id: 1, response };
                let text = svolder_ipc::to_line(&reply).expect("a reply line");
                let _ = write.write_all(text.as_bytes()).await;
            }
            held.push((read, write));
        }
    });
}

/// A daemon that takes the connection and never answers is reported as not answering, and
/// `status` ends.
#[tokio::test]
async fn status_gives_up_on_a_daemon_that_never_answers_the_version() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-status-silent");
    let socket = dir.0.join("svolder.sock");
    silent_daemon(&socket, None);

    let lines = tokio::time::timeout(
        Duration::from_secs(10),
        status_lines(&tools.ctl(), &socket, &dir.0.join("svolderd.log"), &[]),
    )
    .await
    .expect("status ends");
    let said = format!(
        "daemon: no answer within {ASK_WAIT:?} on {}",
        socket.display()
    );
    assert!(lines.contains(&said), "{lines:?}");
    assert!(
        !lines.iter().any(|line| line.starts_with("device")),
        "{lines:?}"
    );
}

/// A daemon that gives its version and then never lists its devices is reported as not
/// answering, and `status` ends.
#[tokio::test]
async fn status_gives_up_on_a_daemon_that_never_lists_its_devices() {
    let tools = Tools::new(&no_units());
    let dir = temp_dir("daemon-status-silent-devices");
    let socket = dir.0.join("svolder.sock");
    // One connection answers its first request and holds, so the device ask over the same
    // connection never gets its answer.
    silent_daemon(
        &socket,
        Some(svolder_ipc::Response::Version {
            version: VERSION.into(),
            mqtt: None,
            mcp: None,
            config: None,
            log: None,
            mcp_token: None,
            web: None,
            web_token: None,
        }),
    );

    let lines = tokio::time::timeout(
        Duration::from_secs(10),
        status_lines(&tools.ctl(), &socket, &dir.0.join("svolderd.log"), &[]),
    )
    .await
    .expect("status ends");
    assert!(
        lines.contains(&format!("devices: no answer within {ASK_WAIT:?}")),
        "{lines:?}"
    );
}
