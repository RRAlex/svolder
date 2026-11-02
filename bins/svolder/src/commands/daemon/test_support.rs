// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the test modules of the four `daemon` verbs share: fake `systemctl`, `journalctl` and
//! `tail` programs that record how they were called, the daemons a verb talks to over a socket,
//! and the text `systemctl show` prints for the unit states a case needs.

use std::path::Path;
use std::sync::Arc;

use futures::StreamExt;
use svolder_ipc::{DeviceInfo, Event, Handler, McpStatus, MqttStatus, Request, Response};

use super::*;
use crate::test_support::{TempDir, install_script, temp_dir};

/// The version the daemon under test answers with.
pub(super) const VERSION: &str = "0.0.0-test";

/// The programs a case under test has: a `systemctl`, a `journalctl` and a `tail` that
/// each record what they were called with and print what the case wants them to print.
pub(crate) struct Tools {
    /// The directory holding the programs and their record.
    pub(super) dir: TempDir,
}

impl Tools {
    /// Programs of this case's own, with `show` as what `systemctl show` prints.
    pub(crate) fn new(show: &str) -> Tools {
        let dir = temp_dir("daemon-tools");
        let show_file = dir.0.join("show.txt");
        std::fs::write(&show_file, show).expect("the canned properties are written");
        for program in ["systemctl", "journalctl", "tail"] {
            let path = dir.0.join(program);
            install_script(
                &path,
                &format!(
                    "#!/bin/sh\n\
                         PATH=/usr/bin:/bin\n\
                         printf '%s\\n' \"{program} $*\" >> '{record}'\n\
                         case \" $* \" in\n\
                         *' stop '*) sed -i 's/ActiveState=active/ActiveState=inactive/g' '{show}' ;;\n\
                         *' show '*) cat '{show}' ;;\n\
                         esac\n\
                         exit 0\n",
                    record = dir.0.join("record.txt").display(),
                    show = show_file.display(),
                ),
            );
        }
        Tools { dir }
    }

    /// Makes `systemctl enable` fail after it is recorded.
    pub(super) fn fail_enable(&self) {
        let path = self.dir.0.join("systemctl");
        let script = std::fs::read_to_string(&path).expect("the fake is there");
        let script = script.replace("esac\n", "*' enable '*) exit 1 ;;\nesac\n");
        install_script(&path, &script);
    }

    /// Makes `systemctl stop` fail after it is recorded.
    pub(crate) fn fail_stop(&self) {
        let path = self.dir.0.join("systemctl");
        let script = std::fs::read_to_string(&path).expect("the fake is there");
        let script = script.replacen(" in\n", " in\n*' stop '*) exit 1 ;;\n", 1);
        install_script(&path, &script);
    }

    /// The programs, as a verb runs them.
    pub(crate) fn ctl(&self) -> Ctl {
        Ctl::in_dir(&self.dir.0)
    }

    /// Every command the programs were called with, in order.
    pub(crate) fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.0.join("record.txt"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

/// The `systemctl` call a verb makes to read the two units' states.
pub(super) fn shown_units() -> String {
    format!(
        "systemctl --user show {SOCKET_UNIT} {SERVICE_UNIT} -p {}",
        checks::live::UNIT_PROPERTIES
    )
}

/// A daemon that lets its socket go when it is asked to, which is what one a front end
/// autospawned does.
struct StoppingDaemon(tokio::sync::Notify);

#[async_trait::async_trait]
impl Handler for StoppingDaemon {
    async fn handle(&self, request: Request) -> Response {
        match request {
            Request::Stop => {
                self.0.notify_one();
                Response::Stopping
            }
            Request::Version => Response::Version {
                version: VERSION.to_string(),
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

/// Serve a daemon that stops when asked on `socket`, and answer once it is listening.
pub(crate) async fn a_daemon_on(socket: &Path) -> tokio::task::JoinHandle<()> {
    let handler = Arc::new(StoppingDaemon(tokio::sync::Notify::new()));
    let served = tokio::spawn({
        let socket = socket.to_path_buf();
        let stops = Arc::clone(&handler);
        async move {
            let _ = svolder_ipc::serve(&socket, handler, async move {
                stops.0.notified().await;
            })
            .await;
        }
    });
    for _ in 0..500 {
        if svolder_ipc::is_running(socket).await {
            break;
        }
        tokio::time::sleep(POLL).await;
    }
    served
}

/// What `systemctl --user show` prints for two units it has never heard of.
pub(crate) fn no_units() -> String {
    format!("Id={SOCKET_UNIT}\nLoadState=not-found\n\nId={SERVICE_UNIT}\nLoadState=not-found\n")
}

/// What it prints for two installed units, the socket listening on `listen`.
pub(super) fn installed_units(listen: &str) -> String {
    format!(
        "Id={SOCKET_UNIT}\nLoadState=loaded\nUnitFileState=enabled\nActiveState=active\n\
             Listen={listen} (Stream)\n\n\
             Id={SERVICE_UNIT}\nLoadState=loaded\nUnitFileState=enabled\nActiveState=active\n"
    )
}

/// A daemon that answers a version and two devices, one held and one it is waiting to try
/// again for, for the verbs that ask it something.
///
/// It answers a stop request the way a daemon does and then keeps serving, which is the one
/// thing a real daemon never does: that is what gives the wait for the socket to close
/// something to give up on.
pub(super) struct FakeDaemon;

/// The log file [`FakeDaemon`] says it writes.
pub(super) const FAKE_LOG: &str = "/home/user/.local/state/svolder/svolderd.log";

#[async_trait::async_trait]
impl Handler for FakeDaemon {
    async fn handle(&self, request: Request) -> Response {
        match request {
            Request::Stop => Response::Stopping,
            Request::Version => Response::Version {
                version: VERSION.to_string(),
                mqtt: Some(MqttStatus::Connected {
                    host: "192.168.1.10".into(),
                    port: 1883,
                    commands: false,
                }),
                mcp: Some(McpStatus::Listening {
                    listen: "127.0.0.1:8737".parse().unwrap(),
                    auth: svolder_ipc::config::McpAuth::Token,
                    writes: false,
                }),
                config: None,
                log: Some(FAKE_LOG.to_string()),
                mcp_token: Some(svolder_ipc::TokenInfo {
                    kind: svolder_ipc::TokenKind::State,
                    path: Some("/home/user/.local/state/svolder/mcp-token".into()),
                    tail: "zQ9".into(),
                }),
                web: Some(svolder_ipc::WebStatus::Listening {
                    listen: "127.0.0.1:8738".parse().unwrap(),
                    auth: svolder_ipc::config::WebAuth::Token,
                    writes: true,
                    guest: svolder_ipc::config::WebGuest::None,
                }),
                web_token: None,
            },
            Request::Devices => Response::Devices {
                devices: vec![
                    DeviceInfo {
                        signal: None,
                        name: "fake".into(),
                        unit: "0102030405060708".into(),
                        product_id: 2,
                        product: Some("Gungnir 2".into()),
                        firmware: 1,
                        profile: "gungnir-2".into(),
                        connected: true,
                        stale: false,
                        retry_in_secs: None,
                        passphrase_note: None,
                        refused: false,
                        recalled: None,
                    },
                    DeviceInfo {
                        signal: None,
                        name: "away".into(),
                        unit: "0807060504030201".into(),
                        product_id: 3,
                        product: Some("Bifrost 3".into()),
                        firmware: 1,
                        profile: "bifrost-3".into(),
                        connected: false,
                        stale: true,
                        retry_in_secs: Some(15),
                        passphrase_note: None,
                        refused: false,
                        recalled: None,
                    },
                ],
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

/// What it prints for two installed units, both stopped, the socket unit on `listen`.
pub(super) fn stopped_units(listen: &str) -> String {
    format!(
        "Id={SOCKET_UNIT}\nLoadState=loaded\nUnitFileState=enabled\nActiveState=inactive\n\
             Listen={listen} (Stream)\n\n\
             Id={SERVICE_UNIT}\nLoadState=loaded\nUnitFileState=enabled\nActiveState=inactive\n"
    )
}

/// What `systemctl --user show` prints for both units installed, in the given states, the socket
/// unit listening on `listen`.
pub(crate) fn units_in(listen: &str, socket: &str, service: &str) -> String {
    format!(
        "Id={SOCKET_UNIT}\nLoadState=loaded\nUnitFileState=enabled\nActiveState={socket}\n\
             Listen={listen} (Stream)\n\n\
             Id={SERVICE_UNIT}\nLoadState=loaded\nUnitFileState=enabled\nActiveState={service}\n"
    )
}
