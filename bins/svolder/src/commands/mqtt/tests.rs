// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `mqtt on|off`, `mqtt set …` and `mqtt password` over a stub daemon and a configuration file of
//! each case's own: who writes the file, what the verb says, and that no line carries the
//! password.

use svolder_ipc::{
    CODE_USAGE, Client, Config, FEATURE_MQTT_SET, MqttOp, MqttProblem, MqttStatus, Response,
};

use super::*;
use crate::test_support::stub_daemon::{StubServer, stub_daemon_at};
use crate::test_support::temp_dir;

fn short_socket() -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .subsec_nanos();
    std::path::PathBuf::from(format!(
        "/tmp/svm9-{}-{seq}-{nanos:x}.sock",
        std::process::id()
    ))
}

async fn stub_daemon() -> StubServer {
    let daemon = stub_daemon_at(short_socket()).await;
    Client::connect(&daemon.socket)
        .await
        .expect("the stub daemon answers on its socket");
    daemon
}

/// With a daemon that has the request, each verb is one request, this side writes nothing, and
/// the line is where the bridge stands.
#[tokio::test]
async fn with_a_daemon_each_verb_is_one_request_and_this_side_writes_nothing() {
    let dir = temp_dir("mqtt-daemon");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[FEATURE_MQTT_SET]);
    let ops = [
        MqttOp::Host {
            value: "ha.lan".into(),
        },
        MqttOp::Port { value: 1884 },
        MqttOp::Username {
            value: Some("svolder".into()),
        },
        MqttOp::Password {
            value: Some("fake-secret".into()),
        },
        MqttOp::Enabled { enabled: true },
        MqttOp::Enabled { enabled: false },
    ];
    let mut lines = Vec::new();
    for op in ops.clone() {
        let done = apply(&daemon.socket, &own, op).await.expect("applied");
        assert!(!done.line.contains("fake-secret"), "{}", done.line);
        lines.push(done.line);
    }
    assert_eq!(daemon.mqtt_ops(), ops);
    assert_eq!(lines[0], "mqtt: off", "off until it is turned on");
    assert_eq!(lines[4], "mqtt: connected to stub:1883 (read-only)");
    assert_eq!(lines[5], "mqtt: off");
    assert_eq!(daemon.reloads(), 0, "the daemon reloads itself");
    assert!(!own.exists(), "the daemon is the writer");
}

/// A broker that refuses the login is said in words, and the verb still succeeds: the key is
/// written and the bridge runs, it is the broker that says no.
#[tokio::test]
async fn a_refused_login_is_a_note_not_a_failure() {
    let dir = temp_dir("mqtt-refused");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[FEATURE_MQTT_SET]);
    daemon.says_mqtt(MqttStatus::Trouble {
        host: "ha.lan".into(),
        port: 1883,
        problem: MqttProblem::Refused,
        retry_in_secs: Some(2),
    });
    let done = apply(&daemon.socket, &own, MqttOp::Port { value: 1883 })
        .await
        .expect("applied");
    assert_eq!(
        done.line,
        "mqtt: not connected to ha.lan:1883: the broker refused the login (check username and \
         password) (retry in 2 s)"
    );
}

/// A daemon too old for the request is told to restart, and nothing is written behind it.
#[tokio::test]
async fn a_daemon_without_the_request_is_not_written_around() {
    let dir = temp_dir("mqtt-old");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[]);
    let failed = apply(&daemon.socket, &own, MqttOp::Enabled { enabled: true })
        .await
        .expect_err("refused");
    assert!(matches!(failed, Failed::TooOld(_)), "{failed}");
    assert!(daemon.mqtt_ops().is_empty());
    assert!(!own.exists());
}

/// A daemon's refusal reaches the caller in the daemon's words.
#[tokio::test]
async fn a_refusal_is_the_daemons_words() {
    let dir = temp_dir("mqtt-refusal");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[FEATURE_MQTT_SET]);
    daemon.answers_switch(Response::error(CODE_USAGE, "mqtt.host must not be empty"));
    let failed = apply(&daemon.socket, &own, MqttOp::Enabled { enabled: true })
        .await
        .expect_err("refused");
    assert_eq!(failed.to_string(), "mqtt.host must not be empty");
}

/// With no daemon the verb writes the file itself, says so, and never prints the password.
#[tokio::test]
async fn with_no_daemon_the_verb_writes_the_file_itself() {
    let dir = temp_dir("mqtt-alone");
    let own = dir.0.join("config.toml");
    let socket = short_socket();
    for op in [
        MqttOp::Host {
            value: "ha.lan".into(),
        },
        MqttOp::Username {
            value: Some("svolder".into()),
        },
        MqttOp::Password {
            value: Some("fake-secret".into()),
        },
        MqttOp::Enabled { enabled: true },
    ] {
        let done = apply(&socket, &own, op).await.expect("written");
        assert!(!done.line.contains("fake-secret"), "{}", done.line);
        assert!(done.notes.iter().all(|n| !n.contains("fake-secret")));
        assert!(
            done.notes.iter().any(|n| n.contains("nothing answers")),
            "{:?}",
            done.notes
        );
    }
    let config = Config::load_from(&own).unwrap();
    assert!(config.mqtt.enabled);
    // A readable file that gets a password is tightened, once, and the note says so.
    {
        use std::os::unix::fs::PermissionsExt;
        let open = dir.0.join("open.toml");
        std::fs::write(&open, "schema = 1\n").unwrap();
        std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o644)).unwrap();
        let op = |v: &str| MqttOp::Password {
            value: Some(v.into()),
        };
        let first = apply(&socket, &open, op("a")).await.unwrap();
        assert!(
            first.notes.iter().any(|n| n.contains("now 0600")),
            "{:?}",
            first.notes
        );
        let second = apply(&socket, &open, op("b")).await.unwrap();
        assert!(second.notes.iter().all(|n| !n.contains("now 0600")));
    }
    assert_eq!(config.mqtt.host.as_deref(), Some("ha.lan"));
    assert_eq!(config.mqtt.username.as_deref(), Some("svolder"));
    assert_eq!(config.mqtt.password.as_deref(), Some("fake-secret"));
    // Turning it on with no host is refused by the parse, and nothing is changed.
    let bare = dir.0.join("bare.toml");
    let failed = apply(&socket, &bare, MqttOp::Enabled { enabled: true })
        .await
        .expect_err("refused");
    assert!(failed.to_string().contains("mqtt.host"), "{failed}");
}

/// A password is the first line of what is piped in, without its line end; no line, or an empty
/// one, is no password.
#[test]
fn a_piped_password_is_its_first_line() {
    let read = |text: &str| password_from(&mut text.as_bytes());
    assert_eq!(read("hunter2\n").unwrap(), "hunter2");
    assert_eq!(read("hunter2\r\n").unwrap(), "hunter2");
    assert_eq!(read("hunter2").unwrap(), "hunter2");
    assert_eq!(read("two words\nsecond\n").unwrap(), "two words");
    assert_eq!(
        read(" padded \n").unwrap(),
        " padded ",
        "spaces are the user's"
    );
    assert!(read("").is_err());
    assert!(read("\n").is_err());
}

/// A password with no username in the file is said to go unsent, and the shell's own
/// `SVOLDER_MQTT_PASSWORD` is said to outrank the file.
#[test]
fn the_password_notes_name_what_would_keep_it_from_being_used() {
    let mut config = Config::parse("").unwrap();
    assert_eq!(
        password_notes(&config, false),
        ["no username is set, so the password is not sent: svolder mqtt set username NAME"]
    );
    config.mqtt.username = Some("u".into());
    assert!(password_notes(&config, false).is_empty());
    assert_eq!(
        password_notes(&config, true),
        [
            "SVOLDER_MQTT_PASSWORD is set in this shell, and a daemon with it set uses it over the file"
        ]
    );
}

/// The echo a prompt turned off is put back by `restore_held`, which the Ctrl-C path calls while
/// the reading thread still stands in its read.
#[test]
fn the_held_echo_is_put_back_on_a_terminal() {
    let (mut master, mut slave) = (0, 0);
    // SAFETY: openpty fills two descriptors it owns; both are closed below.
    let opened = unsafe {
        libc::openpty(
            &mut master,
            &mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(opened, 0, "a pty opens");
    let echo = |fd| {
        // SAFETY: `t` is filled by tcgetattr before it is read.
        unsafe {
            let mut t = std::mem::zeroed::<libc::termios>();
            assert_eq!(libc::tcgetattr(fd, &mut t), 0);
            t.c_lflag & libc::ECHO != 0
        }
    };
    assert!(echo(slave));
    let hidden = crate::wizard::cli::Echo::off(slave).expect("a terminal");
    assert!(!echo(slave), "echo is off while the prompt reads");
    crate::wizard::cli::Echo::restore_held();
    assert!(echo(slave), "echo is back after the restore");
    drop(hidden);
    assert!(echo(slave));
    // SAFETY: both descriptors came from openpty above.
    unsafe {
        libc::close(master);
        libc::close(slave);
    }
}

/// Clearing a username the environment also gives says why it comes back.
#[test]
fn clearing_a_username_the_environment_gives_says_so() {
    let clear = MqttOp::Username { value: None };
    let set = MqttOp::Username {
        value: Some("u".into()),
    };
    assert!(username_note(&clear, true).is_some());
    assert!(username_note(&set, true).is_none());
    assert!(username_note(&clear, false).is_none());
}

/// A daemon that narrowed the file's mode says so once, in a note.
#[tokio::test]
async fn a_narrowed_mode_the_daemon_reports_is_a_note() {
    let dir = temp_dir("mqtt-narrowed");
    let own = dir.0.join("config.toml");
    let daemon = stub_daemon().await;
    daemon.lists_features(&[FEATURE_MQTT_SET]);
    daemon.tightens_on_password();
    let done = apply(
        &daemon.socket,
        &own,
        MqttOp::Password {
            value: Some("fake-secret".into()),
        },
    )
    .await
    .expect("applied");
    assert!(
        done.notes.iter().any(|n| n.contains("made it 0600")),
        "{:?}",
        done.notes
    );
    let other = apply(&daemon.socket, &own, MqttOp::Port { value: 1884 })
        .await
        .expect("applied");
    assert!(other.notes.iter().all(|n| !n.contains("0600")));
}
