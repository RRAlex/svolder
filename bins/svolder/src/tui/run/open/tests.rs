// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::test_support::stub_daemon::{StubServer, stub_daemon};

/// The command line of a start that reaches the stub daemon on its socket and reads no host
/// file of its own.
fn globals_for(daemon: &StubServer) -> OwnedGlobals {
    OwnedGlobals {
        transport: "auto".into(),
        socket: Some(daemon.socket.to_string_lossy().into_owned()),
        adapter: None,
        device: None,
        verbose: false,
    }
}

/// Opening for the TUI over a daemon reads the daemon's device list once, and the list the
/// front end gets is that read.
#[tokio::test]
async fn open_device_with_asks_the_daemon_for_the_device_list_once() {
    let daemon = stub_daemon().await;
    let (mut backend, rows) = open_device_with(globals_for(&daemon), &Config::default())
        .await
        .expect("the stub daemon answers");
    assert_eq!(rows.len(), 2);
    assert!(backend.unit().await.is_some());
    assert_eq!(daemon.device_asks(), 1);
    backend.close().await;
}

/// An opener that opens as the TUI does, over the stub daemon, and counts its opens.
struct Counting {
    opens: AtomicUsize,
}

impl Open for Counting {
    async fn open(&self, globals: OwnedGlobals, _: &Path) -> Result<Opened, Failure> {
        self.opens.fetch_add(1, Ordering::SeqCst);
        open_device_with(globals, &Config::default()).await
    }

    async fn open_on(&self, _: PathBuf, globals: OwnedGlobals) -> Result<Opened, Failure> {
        self.open(globals, Path::new("")).await
    }

    async fn hold(
        &self,
        _: Option<PathBuf>,
        _: OwnedGlobals,
        _: Option<String>,
        _: &Path,
    ) -> Option<Held> {
        None
    }
}

/// A reopen over a daemon asks for the device list once, whatever it then reads (unit id,
/// passphrase note) from what the open answered.
#[tokio::test]
async fn reopen_asks_the_daemon_for_the_device_list_once() {
    let daemon = stub_daemon().await;
    let opener = Opener::new(
        globals_for(&daemon),
        TuiConfig::default(),
        std::env::temp_dir().join("svolder-reopen-asks-once.toml"),
        Counting {
            opens: AtomicUsize::new(0),
        },
    );
    let watching = opener.reopen(None, None).await.expect("the stub opens");
    assert_eq!(opener.open.opens.load(Ordering::SeqCst), 1);
    assert!(watching.app.session.unit.is_some());
    assert_eq!(daemon.device_asks(), 1);
}

/// The TUI opens with the configuration file it shows, the one its opener holds, rather than
/// whatever `Config::path` resolves to: a `default_device` that file names and does not
/// configure fails the open in that file's own words.
#[tokio::test]
async fn the_tui_opens_with_the_file_it_shows() {
    let dir = crate::test_support::temp_dir("tui-opens-its-file");
    let path = dir.join("config.toml");
    std::fs::write(&path, "default_device = \"Bench DAC\"\n").expect("the file is written");
    let globals = OwnedGlobals {
        transport: "fake".into(),
        socket: None,
        adapter: None,
        device: None,
        verbose: false,
    };
    let failure = Live { only: None }
        .open(globals, &path)
        .await
        .err()
        .expect("the default names no device");
    assert!(failure.what.contains("Bench DAC"), "{failure}");
}
