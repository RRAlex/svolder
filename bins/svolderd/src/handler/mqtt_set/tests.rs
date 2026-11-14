// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::sync::Arc;

use svolder_ipc::{CODE_USAGE, Config, Handler, MqttOp, MqttStatus, Reason, Request, Response};

use crate::handler::Daemon;
use crate::handler::tests::daemon_over;

/// A scratch folder with a configuration file holding `text`, and a shared daemon reading it.
fn daemon_on(tag: &str, text: &str) -> (PathBuf, PathBuf, Arc<Daemon>) {
    let dir = std::env::temp_dir().join(format!("svolderd-ms-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, text).unwrap();
    let daemon = daemon_over(path.clone(), dir.join("profiles")).shared();
    (dir, path, daemon)
}

async fn set(daemon: &Daemon, op: MqttOp) -> Response {
    daemon.handle(Request::MqttSet { op }).await
}

/// The status of an answer that is one.
fn status(answer: Response) -> MqttStatus {
    match answer {
        Response::MqttSet { status, .. } => status,
        other => panic!("not the bridge's status: {other:?}"),
    }
}

/// Each key is written where the file keeps it, the bridge follows the reload, and nothing the
/// daemon answers or prints carries the password.
#[tokio::test]
async fn every_key_is_written_and_the_answer_never_carries_the_password() {
    let (dir, path, daemon) = daemon_on("keys", "schema = 1\n");
    // A port nothing listens on, on this machine: the bridge tries it and fails locally.
    for op in [
        MqttOp::Host {
            value: "127.0.0.1".into(),
        },
        MqttOp::Port { value: 1 },
        MqttOp::Username {
            value: Some("svolder".into()),
        },
        MqttOp::Password {
            value: Some("fake-secret".into()),
        },
    ] {
        let answer = set(&daemon, op).await;
        assert_eq!(status(answer.clone()), MqttStatus::Off, "{answer:?}");
        assert!(!format!("{answer:?}").contains("fake-secret"));
    }
    let on = set(&daemon, MqttOp::Enabled { enabled: true }).await;
    assert!(
        matches!(
            status(on),
            MqttStatus::Connecting { .. } | MqttStatus::Trouble { .. }
        ),
        "the reload starts the bridge"
    );
    let config = Config::load_from(&path).unwrap();
    assert!(config.mqtt.enabled);
    assert_eq!(config.mqtt.host.as_deref(), Some("127.0.0.1"));
    assert_eq!(config.mqtt.port, 1);
    assert_eq!(config.mqtt.username.as_deref(), Some("svolder"));
    assert_eq!(config.mqtt.password.as_deref(), Some("fake-secret"));
    assert_eq!(
        daemon.config_snapshot().mqtt.username.as_deref(),
        Some("svolder"),
        "the snapshot follows"
    );
    let version = daemon.handle(Request::Version).await;
    assert!(!format!("{version:?}").contains("fake-secret"));
    assert!(
        !serde_json::to_string(&version)
            .unwrap()
            .contains("fake-secret")
    );

    let off = set(&daemon, MqttOp::Enabled { enabled: false }).await;
    assert_eq!(status(off), MqttStatus::Off);
    set(&daemon, MqttOp::Password { value: None }).await;
    assert_eq!(Config::load_from(&path).unwrap().mqtt.password, None);
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A bridge turned on with no host is refused as invalid and the file is left as it was.
#[tokio::test]
async fn on_with_no_host_is_refused_and_writes_nothing() {
    let (dir, path, daemon) = daemon_on("nohost", "schema = 1\n");
    match set(&daemon, MqttOp::Enabled { enabled: true }).await {
        Response::Error {
            code: CODE_USAGE,
            reason: Some(Reason::Invalid),
            message,
            ..
        } => assert!(message.contains("mqtt.host"), "{message}"),
        other => panic!("not an invalid refusal: {other:?}"),
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "schema = 1\n");
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A daemon whose file is gone refuses and does not make the file again.
#[tokio::test]
async fn a_write_while_the_file_is_gone_is_refused() {
    let (dir, path, daemon) = daemon_on("gone", "schema = 1\n");
    std::fs::remove_file(&path).unwrap();
    let answer = set(&daemon, MqttOp::Host { value: "h".into() }).await;
    assert!(matches!(answer, Response::Error { .. }), "{answer:?}");
    assert!(!path.exists());
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}

/// A password written into a readable file narrows it to 0600, and the answer says so once.
#[tokio::test]
async fn the_answer_says_when_a_password_narrowed_the_file() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, path, daemon) = daemon_on("mode", "schema = 1\n");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let said = |answer: Response| match answer {
        Response::MqttSet { tightened, .. } => tightened,
        other => panic!("not the bridge's status: {other:?}"),
    };
    assert!(!said(set(&daemon, MqttOp::Port { value: 1884 }).await));
    let first = set(
        &daemon,
        MqttOp::Password {
            value: Some("fake-secret".into()),
        },
    )
    .await;
    assert!(said(first));
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let second = set(
        &daemon,
        MqttOp::Password {
            value: Some("other".into()),
        },
    )
    .await;
    assert!(!said(second));
    daemon.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
}
