// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::{Reply, to_line};

const OWN: &str = "0.16.0";

/// Versions compare in semver order, with numbers as numbers and a pre-release before its release.
#[test]
fn skew_compares_versions_in_semver_order() {
    assert_eq!(skew(OWN, OWN), None);
    assert_eq!(skew("no version (x)", OWN), None);
    assert_eq!(skew("0.0.1", OWN), Some(Skew::Older));
    assert_eq!(skew("0.0.1-rc.1", OWN), Some(Skew::Older));
    assert_eq!(skew("9.9.9", OWN), Some(Skew::Newer));
    assert_eq!(skew("0.100.0", OWN), Some(Skew::Newer));
    assert_eq!(skew("0.16.0-rc.1", OWN), Some(Skew::Older));
    assert_eq!(skew("0.16.0.1", OWN), Some(Skew::Newer));
    assert_eq!(skew("0.16.1-rc.1", OWN), Some(Skew::Newer));
    assert_eq!(skew("0.16.1", "0.16.1-rc.1"), Some(Skew::Newer));
}

/// Build metadata is ignored, so two builds of one version do not differ, and a version that
/// cannot be read is the older.
#[test]
fn skew_ignores_build_metadata_and_takes_an_unreadable_version_as_older() {
    assert_eq!(skew("0.16.0+build.5", OWN), None);
    assert_eq!(skew(OWN, "0.16.0+build.5"), None);
    assert_eq!(skew("0.16.0+build.5", "0.16.0+build.6"), None);
    assert_eq!(skew("0.16.0-rc.1+build.5", "0.16.0-rc.1"), None);
    assert_eq!(skew("0.16.1+build.5", OWN), Some(Skew::Newer));
    assert_eq!(skew("99.0.0-rc.1+abc", OWN), Some(Skew::Newer));
    assert_eq!(skew("0.x", OWN), Some(Skew::Older));
    assert_eq!(skew("99.x.0", OWN), Some(Skew::Older));
    assert_eq!(skew("", OWN), None);
    assert_eq!(skew("an unexpected answer (y)", OWN), None);
}

/// A version number is told from the words that stand in for one that was not given.
#[test]
fn is_version_tells_a_version_from_the_words_that_stand_in_for_one() {
    for (text, is) in [
        ("0.1.0", true),
        ("no version (x)", false),
        ("an unexpected answer (y)", false),
        ("", false),
        ("1 2", false),
        ("0.16.0-rc.1+abc", true),
    ] {
        assert_eq!(is_version(text), is, "{text:?}");
    }
}

/// The words and the way out, for both skews, whole.
#[test]
fn advice_names_the_way_out_of_each_skew() {
    assert_eq!(
        Skew::Older.words("0.0.1", OWN),
        "daemon v0.0.1 is older than this svolder v0.16.0"
    );
    assert_eq!(
        Skew::Older.advice("0.0.1", OWN),
        "daemon v0.0.1 is older than this svolder v0.16.0: svolder daemon restart"
    );
    assert_eq!(
        Skew::Newer.advice("9.9.9", OWN),
        "daemon v9.9.9 is newer than this svolder v0.16.0: restart svolder"
    );
    assert_eq!(RESTART_COMMAND, "svolder daemon restart");
}

/// The line `status` adds names the advice, and a matching or wordless daemon gets none.
#[test]
fn mismatch_line_is_the_advice_for_a_daemon_of_another_version() {
    assert_eq!(
        mismatch_line("0.0.1", OWN).as_deref(),
        Some("mismatch: daemon v0.0.1 is older than this svolder v0.16.0: svolder daemon restart")
    );
    assert_eq!(
        mismatch_line("9.9.9", OWN).as_deref(),
        Some("mismatch: daemon v9.9.9 is newer than this svolder v0.16.0: restart svolder")
    );
    assert_eq!(mismatch_line(OWN, OWN), None);
    assert_eq!(mismatch_line("no version (x)", OWN), None);
}

/// A fake daemon on a fresh socket that reads one request line and answers it with `reply`, or
/// holds the connection without answering when there is none.
fn fake_daemon(reply: Option<Reply>) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let socket = std::env::temp_dir().join(format!(
        "svolder-version-fake-{}-{seq}.sock",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&socket);
    let listener = tokio::net::UnixListener::bind(&socket).expect("the socket binds");
    tokio::spawn(async move {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept().await {
            let (read, mut write) = stream.into_split();
            let mut read = tokio::io::BufReader::new(read);
            let mut line = String::new();
            let _ = read.read_line(&mut line).await;
            if let Some(reply) = &reply {
                let text = to_line(reply).expect("a reply line");
                let _ = write.write_all(text.as_bytes()).await;
            }
            held.push((read, write));
        }
    });
    socket
}

/// A version reply is the daemon's version and status, whole; any other reply is nothing.
#[tokio::test]
async fn a_version_reply_is_a_daemon_info_and_another_reply_is_nothing() {
    let socket = fake_daemon(Some(Reply::Response {
        id: 1,
        response: Response::Version {
            version: "9.9.9".into(),
            mqtt: None,
            mcp: Some(McpStatus::Off),
            log: Some("journal".into()),
            config: Some("/etc/svolder.toml".into()),
            mcp_token: None,
            web: None,
            web_token: None,
        },
    }));
    assert_eq!(
        version_within(&socket, Duration::from_secs(3)).await,
        Some(DaemonInfo {
            version: "9.9.9".into(),
            mqtt: None,
            mcp: Some(McpStatus::Off),
            config: Some("/etc/svolder.toml".into()),
            mcp_token: None,
            web: None,
            web_token: None,
        })
    );
    let _ = std::fs::remove_file(&socket);

    let socket = fake_daemon(Some(Reply::Response {
        id: 1,
        response: Response::Ack,
    }));
    assert_eq!(version_within(&socket, Duration::from_secs(3)).await, None);
    let _ = std::fs::remove_file(&socket);

    let gone = std::env::temp_dir().join("svolder-version-fake-nothing.sock");
    assert_eq!(version_within(&gone, Duration::from_secs(3)).await, None);
}

/// A daemon that takes the connection and never answers is given up on after the wait.
#[tokio::test(start_paused = true)]
async fn a_silent_daemon_is_given_up_on_after_the_wait() {
    let socket = fake_daemon(None);
    assert_eq!(version_within(&socket, Duration::from_secs(3)).await, None);
    let _ = std::fs::remove_file(&socket);
}

/// `version_on` hands back the whole reply: the info and the log the daemon names.
#[tokio::test]
async fn version_on_carries_the_log_with_the_info() {
    let socket = fake_daemon(Some(Reply::Response {
        id: 1,
        response: Response::Version {
            version: "9.9.9".into(),
            mqtt: None,
            mcp: Some(McpStatus::Off),
            log: Some("journal".into()),
            config: None,
            mcp_token: None,
            web: None,
            web_token: None,
        },
    }));
    let mut client = Client::connect(&socket).await.expect("connects");
    let reply = version_on(&mut client).await.expect("a version");
    assert_eq!(
        reply,
        VersionReply {
            info: DaemonInfo {
                version: "9.9.9".into(),
                mqtt: None,
                mcp: Some(McpStatus::Off),
                config: None,
                mcp_token: None,
                web: None,
                web_token: None,
            },
            log: Some("journal".into()),
        }
    );
    let _ = std::fs::remove_file(&socket);
}

/// Any other reply is named as unexpected.
#[tokio::test]
async fn version_on_names_another_reply_as_unexpected() {
    let socket = fake_daemon(Some(Reply::Response {
        id: 1,
        response: Response::Ack,
    }));
    let mut client = Client::connect(&socket).await.expect("connects");
    let why = version_on(&mut client).await.expect_err("no version");
    assert!(matches!(why, Unanswered::Other(_)));
    assert_eq!(why.to_string(), "an unexpected answer (Ack)");
    let _ = std::fs::remove_file(&socket);
}

/// A connection that is lost before the answer is named as no version.
#[tokio::test]
async fn version_on_names_a_lost_connection() {
    let socket =
        std::env::temp_dir().join(format!("svolder-version-drop-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let listener = tokio::net::UnixListener::bind(&socket).expect("the socket binds");
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            drop(stream);
        }
    });
    let mut client = Client::connect(&socket).await.expect("connects");
    let why = version_on(&mut client).await.expect_err("no version");
    assert!(matches!(why, Unanswered::Failed(_)));
    assert!(why.to_string().starts_with("no version ("), "{why}");
    let _ = std::fs::remove_file(&socket);
}
