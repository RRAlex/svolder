// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The dump against a fake `pw-dump`: a script in the case's own directory that prints a
//! fixture, fails or hangs. Nothing here runs the real `pw-dump`.

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::{TempDir, install_script};
use svolder_host::route::{PlayerMatch, PwDump, route};

/// A fixture of real `pw-dump` output.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pw-dump")
        .join(name)
}

#[tokio::test]
async fn the_dump_a_tool_prints_is_read_and_gives_a_route() {
    let dir = TempDir::new("pw-dump-reads");
    let script = format!(
        "#!/bin/sh\ncat '{}'\n",
        fixture("one-stream.json").display()
    );
    install_script(&dir.join("pw-dump"), &script);
    let dump = PwDump::new(dir.join("pw-dump"))
        .read()
        .await
        .expect("the fake answers");
    let found = route(&dump, Some(1_790_719), "").expect("it is a pw-dump");
    assert_eq!(found.player_match, PlayerMatch::Pid);
    assert_eq!(
        found.sink.expect("a sink").name,
        "alsa_output.pci-0000_11_00.1.hdmi-stereo"
    );
}

#[tokio::test]
async fn a_tool_that_fails_gives_no_dump() {
    let dir = TempDir::new("pw-dump-fails");
    install_script(&dir.join("pw-dump"), "#!/bin/sh\necho '[]'\nexit 1\n");
    assert_eq!(PwDump::new(dir.join("pw-dump")).read().await, None);
}

#[tokio::test]
async fn a_missing_tool_gives_no_dump() {
    let dir = TempDir::new("pw-dump-missing");
    assert_eq!(PwDump::new(dir.join("no-such-tool")).read().await, None);
}

#[tokio::test]
async fn a_tool_that_does_not_answer_is_given_up_on() {
    let dir = TempDir::new("pw-dump-hangs");
    install_script(&dir.join("pw-dump"), "#!/bin/sh\nexec sleep 30\n");
    let dump = PwDump::new(dir.join("pw-dump")).with_timeout(Duration::from_millis(300));
    let started = Instant::now();
    assert_eq!(dump.read().await, None);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the call waited {:?}",
        started.elapsed()
    );
}
