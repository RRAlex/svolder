// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The PipeWire config box's own read, where no daemon gives the route: a fake `pw-dump` that
//! prints a capture of a private instance, and one that fails.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

use svolder_host::{Asound, PwDump};

use super::super::test_support::*;
use super::super::*;
use super::{Reader, first_due, polling, read, settle};

/// The capture the fake `pw-dump` prints: real output of a private PipeWire instance.
const CAPTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../crates/svolder-host/tests/fixtures/pw-dump/setup-pipe.json"
);

/// A `pw-dump` in this case's scratch directory that runs `body`, and an empty ALSA tree beside it.
fn reader(body: &str) -> Reader {
    let dir: PathBuf = scratch_config().parent().unwrap().to_path_buf();
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("pw-dump");
    std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    Reader {
        dump: PwDump::new(script).with_timeout(Duration::from_secs(10)),
        asound: Asound::new(dir.join("asound")),
    }
}

/// A loop on the fake device with the box open and a reader of the case's own.
async fn with_box(reader: Reader) -> Watching {
    let (backend, _gate, app) = fake(Duration::from_secs(1)).await;
    let mut watching = watching(backend, app, None);
    watching.app.host.linked = true;
    watching.app.open_setup();
    watching.setup.reader = Some(reader);
    watching
}

/// With no daemon the box reads `pw-dump` when it opens: the route lands in the box, and the
/// judge's lines for it are the ones the box draws.
#[tokio::test]
async fn the_box_reads_the_graph_itself_when_it_opens_without_a_daemon() {
    let mut watching = with_box(reader(&format!("cat '{CAPTURE}'"))).await;
    assert!(first_due(&watching), "a freshly opened box is owed a read");
    read(&mut watching);
    assert!(
        !first_due(&watching) && !polling(&watching),
        "one read at a time"
    );
    settle(&mut watching).await;
    let setup = watching.app.setup_box().unwrap();
    assert!(!setup.failed && setup.asked);
    let route = setup.own.as_deref().expect("the route was read");
    assert!(route.sink.is_some() && route.setup.is_some());
    let check = watching.app.setup_check();
    assert_eq!(check.lines.len(), 7);
    assert!(!check.sink.is_empty());
    assert!(
        polling(&watching),
        "the clock asks again while the box is open"
    );
    assert!(!first_due(&watching));
}

/// A graph that cannot be read leaves the box with a note and every line not read (the last
/// route is kept to fall back on, never judged), and the next read tries again.
#[tokio::test]
async fn a_failed_read_is_noted_and_the_box_judges_nothing_after_it() {
    let mut watching = with_box(reader("exit 1")).await;
    read(&mut watching);
    settle(&mut watching).await;
    let setup = watching.app.setup_box().unwrap();
    assert!(setup.failed && setup.own.is_none());
    watching.setup.reader = Some(reader(&format!("cat '{CAPTURE}'")));
    read(&mut watching);
    settle(&mut watching).await;
    let setup = watching.app.setup_box().unwrap();
    assert!(!setup.failed && setup.own.is_some());
    watching.setup.reader = Some(reader("exit 1"));
    read(&mut watching);
    settle(&mut watching).await;
    let setup = watching.app.setup_box().unwrap();
    assert!(setup.failed && setup.own.is_some(), "the last route stays");
    assert!(watching.app.setup_route().is_none(), "but it is not judged");
}

/// With a daemon speaking for the host, or with the box shut, nothing is read here.
#[tokio::test]
async fn nothing_is_read_with_a_daemon_or_with_the_box_shut() {
    let mut watching = with_box(reader("exit 1")).await;
    watching.app.host.daemon = Some(crate::tui::host::FromDaemon { route: None });
    assert!(!first_due(&watching) && !polling(&watching));
    watching.app.host.daemon = None;
    assert!(first_due(&watching));
    watching.app.overlays.setup = None;
    assert!(!first_due(&watching) && !polling(&watching));
}
