// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::time::Duration;

use super::*;
use crate::backend::Backend;
use crate::test_support::stub_daemon::{SocketFile, StubServer, fresh_socket, stub_daemon};
use svolder_ipc::Config;

/// A configuration naming `names`, written into a directory of the case's own.
fn config_naming(names: &[&str]) -> (crate::test_support::TempDir, PathBuf, Config) {
    let dir = crate::test_support::temp_dir("stale");
    let path = dir.0.join("config.toml");
    let text: String = names
        .iter()
        .map(|name| format!("[[device]]\nname = \"{name}\"\nudid = \"0102030405060708\"\n"))
        .collect();
    std::fs::write(&path, text).expect("the configuration is written");
    let config = Config::load_from(&path).expect("a valid configuration");
    (dir, path, config)
}

/// A stub daemon reading `path`, which holds the devices `fake` and `other`.
async fn daemon_reading(path: &Path) -> StubServer {
    let daemon = stub_daemon().await;
    daemon.says(None, Some(&path.display().to_string()));
    daemon
}

fn socket_of(daemon: &StubServer) -> String {
    daemon.socket.display().to_string()
}

fn globals(socket: &str) -> Globals<'_> {
    Globals {
        transport: "auto",
        adapter: None,
        socket: Some(socket),
        device: None,
        verbose: false,
    }
}

/// A daemon reading this file and holding other names than the file's is told once, and the
/// note says which file it re-read.
#[tokio::test]
async fn a_daemon_on_this_file_with_other_names_is_told_to_reload_once() {
    let (_dir, path, config) = config_naming(&["fake"]);
    let daemon = daemon_reading(&path).await;
    let socket = socket_of(&daemon);
    let note = reload_if_stale(&globals(&socket), &config, &path).await;
    assert_eq!(note, Some(format!("svolderd re-read {}", path.display())));
    assert_eq!(daemon.reloads(), 1);
}

/// The same file with the same names gives no note at all.
#[tokio::test]
async fn the_same_path_gives_no_warning() {
    let (_dir, path, config) = config_naming(&["OTHER", "Fake"]);
    let daemon = daemon_reading(&path).await;
    let socket = socket_of(&daemon);
    let note = reload_if_stale(&globals(&socket), &config, &path).await;
    assert_eq!(note, None);
}

/// The same names, in another case and order, are no reason to reload.
#[tokio::test]
async fn the_same_names_get_no_reload() {
    let (_dir, path, config) = config_naming(&["OTHER", "Fake"]);
    let daemon = daemon_reading(&path).await;
    let socket = socket_of(&daemon);
    assert_eq!(
        reload_if_stale(&globals(&socket), &config, &path).await,
        None
    );
    assert_eq!(daemon.reloads(), 0);
}

/// A daemon reading another file that exists here is never reloaded, and the note names both
/// files; one that does not say which file it reads gets no note.
#[tokio::test]
async fn another_path_gets_a_warning_and_no_reload() {
    let (dir, path, config) = config_naming(&["fake"]);
    let other = dir.0.join("other.toml");
    std::fs::write(&other, "").expect("the other file is written");
    let daemon = daemon_reading(&other).await;
    let socket = socket_of(&daemon);
    assert_eq!(
        reload_if_stale(&globals(&socket), &config, &path).await,
        Some(format!(
            "svolderd reads {}, not {}",
            other.display(),
            path.display()
        ))
    );
    daemon.says(None, None);
    assert_eq!(
        reload_if_stale(&globals(&socket), &config, &path).await,
        None
    );
    assert_eq!(daemon.reloads(), 0);
}

/// A daemon whose path is not there at all, as a container's `/config/...` is not, gets no note.
#[tokio::test]
async fn a_path_that_does_not_exist_here_gives_no_warning() {
    let (_dir, path, config) = config_naming(&["fake"]);
    let daemon = daemon_reading(Path::new("/config/svolder/config.toml")).await;
    let socket = socket_of(&daemon);
    assert_eq!(
        reload_if_stale(&globals(&socket), &config, &path).await,
        None
    );
    assert_eq!(daemon.reloads(), 0);
}

/// Paths under the home directory are written with `~`.
#[test]
fn the_note_writes_the_home_directory_as_a_tilde() {
    let home = Path::new("/home/user");
    assert_eq!(
        tilde(
            Path::new("/home/user/.config/svolder/config.toml"),
            Some(home)
        ),
        "~/.config/svolder/config.toml"
    );
    assert_eq!(
        tilde(Path::new("/tmp/x/config.toml"), Some(home)),
        "/tmp/x/config.toml"
    );
    assert_eq!(
        tilde(Path::new("/home/userx/c.toml"), Some(home)),
        "/home/userx/c.toml"
    );
    assert_eq!(
        tilde(Path::new("/home/user/c.toml"), None),
        "/home/user/c.toml"
    );
}

/// A path reached through a symlink is the same file.
#[tokio::test]
async fn a_symlinked_path_is_the_same_file() {
    let (dir, path, config) = config_naming(&["fake"]);
    let link = dir.0.join("link.toml");
    std::os::unix::fs::symlink(&path, &link).expect("the link is made");
    let daemon = daemon_reading(&link).await;
    let socket = socket_of(&daemon);
    assert!(
        reload_if_stale(&globals(&socket), &config, &path)
            .await
            .is_some()
    );
    assert_eq!(daemon.reloads(), 1);
}

/// With no daemon answering nothing is reloaded and nothing is started: the socket stays
/// unbound.
#[tokio::test]
async fn no_daemon_means_no_reload_and_no_start() {
    let (dir, path, config) = config_naming(&["fake"]);
    let socket = dir.0.join("none.sock");
    let socket_arg = socket.display().to_string();
    assert_eq!(
        reload_if_stale(&globals(&socket_arg), &config, &path).await,
        None
    );
    assert!(!socket.exists());
}

/// A transport that never asks a daemon never reloads one.
#[tokio::test]
async fn a_direct_transport_asks_no_daemon() {
    let (_dir, path, config) = config_naming(&["fake"]);
    let daemon = daemon_reading(&path).await;
    let socket = socket_of(&daemon);
    let g = Globals {
        transport: "fake",
        ..globals(&socket)
    };
    assert_eq!(reload_if_stale(&g, &config, &path).await, None);
    assert_eq!(daemon.reloads(), 0);
}

/// With nothing configured a daemon that still holds devices is told too, before the front
/// door chooses the wizard.
#[tokio::test]
async fn a_daemon_holding_devices_the_empty_file_does_not_name_is_told() {
    let (_dir, path, config) = config_naming(&[]);
    let daemon = daemon_reading(&path).await;
    let socket = socket_of(&daemon);
    assert!(
        reload_if_stale(&globals(&socket), &config, &path)
            .await
            .is_some()
    );
    assert_eq!(daemon.reloads(), 1);
}

/// A daemon that answers `version` and then never answers anything else is given up on
/// quietly, for the devices ask and for the reload alike.
struct Stalls {
    config: String,
    devices_answered: bool,
}

#[async_trait::async_trait]
impl svolder_ipc::Handler for Stalls {
    async fn handle(&self, request: Request) -> Response {
        match request {
            Request::Version => Response::Version {
                version: "9.9.9".into(),
                mqtt: None,
                mcp: None,
                config: Some(self.config.clone()),
                log: None,
                mcp_token: None,
                web: None,
                web_token: None,
            },
            Request::Devices if self.devices_answered => Response::Devices {
                devices: Vec::new(),
            },
            _ => std::future::pending().await,
        }
    }

    async fn subscribe(
        &self,
        _device: Option<String>,
    ) -> futures::stream::BoxStream<'static, svolder_ipc::Event> {
        Box::pin(futures::stream::pending())
    }
}

async fn stalling_daemon(
    path: &Path,
    devices_answered: bool,
) -> (SocketFile, tokio::sync::oneshot::Sender<()>) {
    let socket = SocketFile(fresh_socket());
    let (stop, rx) = tokio::sync::oneshot::channel::<()>();
    let served = std::sync::Arc::new(Stalls {
        config: path.display().to_string(),
        devices_answered,
    });
    let at = socket.to_path_buf();
    tokio::spawn(async move {
        let _ = svolder_ipc::serve(&at, served, async {
            let _ = rx.await;
        })
        .await;
    });
    for _ in 0..500 {
        if Client::connect(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    (socket, stop)
}

#[tokio::test]
async fn a_daemon_that_stalls_after_version_is_given_up_on_without_a_note() {
    for devices_answered in [false, true] {
        let (_dir, path, config) = config_naming(&["fake"]);
        let (socket, _stop) = stalling_daemon(&path, devices_answered).await;
        let socket = socket.display().to_string();
        let noted = tokio::time::timeout(
            Duration::from_secs(10),
            reload_if_stale(&globals(&socket), &config, &path),
        )
        .await
        .expect("the check ended on its own timeouts");
        assert_eq!(noted, None, "devices answered: {devices_answered}");
    }
}

/// A daemon that answers every request, each after `pause`.
struct Slow {
    config: String,
    pause: Duration,
}

#[async_trait::async_trait]
impl svolder_ipc::Handler for Slow {
    async fn handle(&self, request: Request) -> Response {
        tokio::time::sleep(self.pause).await;
        match request {
            Request::Version => Response::Version {
                version: "9.9.9".into(),
                mqtt: None,
                mcp: None,
                config: Some(self.config.clone()),
                log: None,
                mcp_token: None,
                web: None,
                web_token: None,
            },
            Request::Devices => Response::Devices {
                devices: Vec::new(),
            },
            _ => Response::Reloaded {
                devices: Vec::new(),
            },
        }
    }

    async fn subscribe(
        &self,
        _device: Option<String>,
    ) -> futures::stream::BoxStream<'static, svolder_ipc::Event> {
        Box::pin(futures::stream::pending())
    }
}

/// The three asks share one deadline: a daemon that answers each in under [`ASK_WAIT`], but
/// not all three, is given up on at [`ASK_WAIT`] with no note.
#[tokio::test]
async fn the_whole_check_shares_one_deadline() {
    let (_dir, path, config) = config_naming(&["fake"]);
    let socket = SocketFile(fresh_socket());
    let (stop, rx) = tokio::sync::oneshot::channel::<()>();
    let served = std::sync::Arc::new(Slow {
        config: path.display().to_string(),
        pause: ASK_WAIT * 2 / 5,
    });
    let at = socket.to_path_buf();
    tokio::spawn(async move {
        let _ = svolder_ipc::serve(&at, served, async {
            let _ = rx.await;
        })
        .await;
    });
    for _ in 0..500 {
        if Client::connect(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let shown = socket.display().to_string();
    let start = std::time::Instant::now();
    let noted = reload_if_stale(&globals(&shown), &config, &path).await;
    assert_eq!(noted, None, "three asks of {:?} each", ASK_WAIT * 2 / 5);
    assert!(
        start.elapsed() < ASK_WAIT * 3 / 2,
        "the check took {:?}",
        start.elapsed()
    );
    let _ = stop.send(());
}

/// Holding a daemon for a landing gives up on a device list that never comes, within
/// [`ASK_WAIT`], rather than freezing the screen.
#[tokio::test]
async fn holding_a_daemon_that_stalls_on_the_device_list_gives_up() {
    let (_dir, path, _config) = config_naming(&["fake"]);
    let (socket, _stop) = stalling_daemon(&path, false).await;
    let g = Globals {
        transport: "auto",
        adapter: None,
        socket: None,
        device: None,
        verbose: false,
    };
    let held = tokio::time::timeout(
        Duration::from_secs(5),
        Backend::hold(&g, Some(&socket), Some("fake"), &Config::path()),
    )
    .await
    .expect("the hold ended on its own bound");
    assert!(held.is_none());
}

/// A preset writer's reload goes only to a daemon whose `version` names the file just written:
/// one reading this file (spelled through a symlink too) is told once, and one reading another
/// file, naming none, or reading a path that is not there is not.
#[tokio::test]
async fn a_preset_reload_reaches_only_a_daemon_reading_this_file() {
    let (dir, path, _config) = config_naming(&["fake"]);
    let daemon = daemon_reading(&path).await;
    reload_if_reads(daemon.socket.clone(), &path).await;
    assert_eq!(daemon.reloads(), 1, "a daemon reading this file");

    let link = dir.0.join("link.toml");
    std::os::unix::fs::symlink(&path, &link).expect("the link is made");
    reload_if_reads(daemon.socket.clone(), &link).await;
    assert_eq!(daemon.reloads(), 2, "the same file under another name");

    let other = dir.0.join("other.toml");
    std::fs::write(&other, "").expect("the other file is written");
    reload_if_reads(daemon.socket.clone(), &other).await;
    assert_eq!(daemon.reloads(), 2, "another file");

    daemon.says(None, None);
    reload_if_reads(daemon.socket.clone(), &path).await;
    assert_eq!(daemon.reloads(), 2, "a daemon that names no file");

    daemon.says(None, Some("/config/svolder/config.toml"));
    reload_if_reads(daemon.socket.clone(), &path).await;
    assert_eq!(daemon.reloads(), 2, "a path that is not here");
}

/// A reload the daemon refuses, and a daemon that is not there, are passed over: nothing is
/// returned for either.
#[tokio::test]
async fn a_preset_reload_that_fails_or_has_no_daemon_is_quiet() {
    let (dir, path, _config) = config_naming(&["fake"]);
    let daemon = daemon_reading(&path).await;
    daemon.refuses_reloads("no");
    reload_if_reads(daemon.socket.clone(), &path).await;
    assert_eq!(daemon.reloads(), 1, "it was asked");
    reload_if_reads(dir.0.join("none.sock"), &path).await;
}
