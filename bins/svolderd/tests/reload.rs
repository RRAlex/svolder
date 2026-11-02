// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Reloading a running daemon: the version it reports, the device set it reconciles from its
//! configuration file on request and on SIGHUP, the socket it adopts from a service manager,
//! and the stop request that ends it.

mod common;

use std::process::Command;
use std::time::Duration;

use futures::StreamExt;
use svolder_ipc::{Client, Event, Request, Response};

use common::{
    CONFIG, POLL, POLLS, STEP_BUDGET, TEST_TIMEOUT, connect, daemon, device_names, spawn_daemon,
    spawn_daemon_with_handed_over_socket, wait_until_connected,
};

/// A second device the fake transport never advertises, so it is configured and running but
/// never connected — which is all the device set needs to show it.
const SECOND: &str = "[[device]]\nname = \"second\"\nudid = \"0908070605040302\"\n";

#[tokio::test]
async fn the_daemon_reports_the_version_it_was_built_at() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client, _socket) = daemon().await;
        let answer = client
            .request(Request::Version)
            .await
            .expect("a version reply");
        assert!(
            matches!(&answer, Response::Version { version, .. } if version == env!("CARGO_PKG_VERSION")),
            "got {answer:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A reload re-reads the configuration file and reconciles the running actors with it: a
/// device added to the file is started, and one taken out of it is stopped.
#[tokio::test]
async fn a_reload_adds_and_removes_devices() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (child, mut client, _socket) = daemon().await;
        assert_eq!(device_names(&mut client).await, vec!["fake".to_string()]);

        std::fs::write(child.config_path(), format!("{CONFIG}{SECOND}"))
            .expect("add the second device");
        let reloaded = client
            .request(Request::Reload)
            .await
            .expect("a reload reply");
        assert!(
            matches!(&reloaded, Response::Reloaded { devices } if devices == &["fake", "second"]),
            "got {reloaded:?}"
        );
        assert_eq!(device_names(&mut client).await, vec!["fake", "second"]);

        std::fs::write(child.config_path(), format!("schema = 1\n{SECOND}"))
            .expect("remove the first device");
        let reloaded = client
            .request(Request::Reload)
            .await
            .expect("a reload reply");
        assert!(
            matches!(&reloaded, Response::Reloaded { devices } if devices == &["second"]),
            "got {reloaded:?}"
        );
        assert_eq!(device_names(&mut client).await, vec!["second".to_string()]);
    })
    .await
    .expect("test timed out");
}

/// A profile named in the file is picked up without stopping the device's actor: the link
/// stays up and the very next device list reports the new profile.
#[tokio::test]
async fn a_reload_gives_a_running_device_the_profile_the_file_names() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (child, mut client, _socket) = daemon().await;
        assert_eq!(profile_of(&mut client).await, "gungnir2-v2");

        std::fs::write(
            child.config_path(),
            format!("{CONFIG}profile = \"gungnir2-v1\"\n"),
        )
        .expect("name a profile for the device");
        let reloaded = client
            .request(Request::Reload)
            .await
            .expect("a reload reply");
        assert!(
            matches!(&reloaded, Response::Reloaded { devices } if devices == &["fake"]),
            "got {reloaded:?}"
        );
        assert_eq!(profile_of(&mut client).await, "gungnir2-v1");
        assert!(
            connected(&mut client).await,
            "the device was restarted rather than reconfigured in place"
        );
    })
    .await
    .expect("test timed out");
}

/// Two reloads at once are one reload after the other: neither starts a second actor for a
/// device the other has already started, and the set the daemon serves afterwards is the
/// file's, with an actor still behind every handle in it.
#[tokio::test]
async fn two_reloads_at_once_are_taken_one_at_a_time() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (child, mut first, socket) = daemon().await;
        let mut second = Client::connect(&socket)
            .await
            .expect("a second client connects");
        std::fs::write(child.config_path(), format!("{CONFIG}{SECOND}"))
            .expect("add the second device");

        let (a, b) = tokio::join!(
            first.request(Request::Reload),
            second.request(Request::Reload)
        );
        for reloaded in [a.expect("a reload reply"), b.expect("a reload reply")] {
            assert!(
                matches!(&reloaded, Response::Reloaded { devices } if devices == &["fake", "second"]),
                "got {reloaded:?}"
            );
        }
        assert_eq!(device_names(&mut first).await, vec!["fake", "second"]);

        // An actor is still behind each handle: the connected device answers with readings,
        // and the absent one with its own actor's refusal rather than the "is not running"
        // that a handle whose actor went away earns.
        let readings = first
            .request(Request::State {
                device: Some("fake".into()),
            })
            .await
            .expect("a state reply");
        assert!(matches!(&readings, Response::Values { .. }), "got {readings:?}");
        let absent = first
            .request(Request::State {
                device: Some("second".into()),
            })
            .await
            .expect("a state reply");
        assert!(
            matches!(&absent, Response::Error { message, .. } if *message == svolder_ipc::not_found_yet("second")),
            "got {absent:?}"
        );

        // Nothing was asked to stop, so an actor that stopped is one a reload started and the
        // other's set then dropped. An actor over the fake stops as soon as its handle goes.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let said = child.said();
        assert!(
            !said.contains("actor stopped"),
            "a reload started an actor the other reload's device set dropped: {said:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A name that keeps its entry but changes its unit id names another device: the actor that
/// was talking to the old one is stopped and a new one started for the new one.
#[tokio::test]
async fn a_reload_restarts_a_device_whose_unit_id_changed() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (child, mut client, _socket) = daemon().await;
        let device = one_device(&mut client).await;
        assert_eq!(device.unit, "0102030405060708");
        assert!(device.connected, "got {device:?}");

        // The same name, pointed at a unit id the fake transport never advertises.
        std::fs::write(
            child.config_path(),
            "schema = 1\n[[device]]\nname = \"fake\"\nudid = \"0908070605040302\"\n",
        )
        .expect("point the device at another unit id");
        let reloaded = client
            .request(Request::Reload)
            .await
            .expect("a reload reply");
        assert!(
            matches!(&reloaded, Response::Reloaded { devices } if devices == &["fake"]),
            "got {reloaded:?}"
        );

        let device = one_device(&mut client).await;
        assert_eq!(device.unit, "0908070605040302");
        assert!(
            !device.connected,
            "the link to the old unit id was kept: {device:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A name that changed only in spelling is the same device: the actor keeps its link and is
/// reconfigured where it stands, and the daemon reports it by the name the file now carries.
#[tokio::test]
async fn a_reload_that_only_respells_a_name_keeps_the_link() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (child, mut client, socket) = daemon().await;
        assert!(one_device(&mut client).await.connected);

        std::fs::write(
            child.config_path(),
            "schema = 1\n[[device]]\nname = \"FAKE\"\nudid = \"0102030405060708\"\n",
        )
        .expect("the device is renamed in spelling alone");
        let reloaded = client
            .request(Request::Reload)
            .await
            .expect("a reload reply");
        assert!(
            matches!(&reloaded, Response::Reloaded { devices } if devices == &["FAKE"]),
            "got {reloaded:?}"
        );

        let device = one_device(&mut client).await;
        assert_eq!(device.name, "FAKE", "the daemon kept the old spelling");
        assert!(
            device.connected,
            "the link was dropped over a change of case: {device:?}"
        );

        // What the daemon answers for the device is named the same way its list names it: a
        // reply or an event under the old spelling is one a front end matching the two would
        // find no profile for, and would draw as a device with no controls.
        let state = client
            .request(Request::State { device: None })
            .await
            .expect("a state reply");
        assert!(
            matches!(&state, Response::Values { device, .. } if device == "FAKE"),
            "got {state:?}"
        );
        let sub = Client::connect(&socket).await.expect("a second connection");
        let mut events = sub.subscribe(None).await.expect("a subscription");
        let first = tokio::time::timeout(STEP_BUDGET, events.next())
            .await
            .unwrap_or_else(|_| panic!("no event within {STEP_BUDGET:?}"))
            .expect("the stream is open");
        assert!(
            matches!(&first, Event::Connection { device, .. } if device == "FAKE"),
            "got {first:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A passphrase corrected in the file is tried on the very next write: the daemon keys the
/// running session with it, rather than leaving the device refusing every frame until the
/// link drops.
#[tokio::test]
async fn a_reload_that_corrects_the_passphrase_frees_the_next_write() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        // The fake device's passphrase is 1234, so the daemon starts with the wrong one: the
        // link comes up and the refresh that ends it is refused.
        let (mut child, socket) = spawn_daemon(&with_passphrase("9999"));
        let mut client = connect(&mut child, &socket).await;
        wait_until_connected(&mut child, &mut client).await;
        let refused = set_input(&mut client).await;
        assert!(
            matches!(&refused, Response::Error { code: 3, message } if message.contains("passphrase")),
            "got {refused:?}"
        );

        std::fs::write(child.config_path(), with_passphrase("1234"))
            .expect("correct the passphrase");
        let reloaded = client
            .request(Request::Reload)
            .await
            .expect("a reload reply");
        assert!(
            matches!(&reloaded, Response::Reloaded { devices } if devices == &["fake"]),
            "got {reloaded:?}"
        );

        let written = set_input(&mut client).await;
        assert!(
            matches!(&written, Response::Value(_)),
            "the corrected passphrase never reached the running session: {written:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A file that does not parse is refused, named line and all, and the devices the daemon has
/// are the ones it keeps.
#[tokio::test]
async fn a_reload_of_an_unparsable_file_keeps_the_devices_it_has() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (child, mut client, _socket) = daemon().await;
        std::fs::write(child.config_path(), "schema = 1\nthis is not toml\n")
            .expect("write an unparsable config");

        let refused = client
            .request(Request::Reload)
            .await
            .expect("a reload reply");
        let Response::Error { code, message } = &refused else {
            panic!("got {refused:?}");
        };
        assert_eq!(*code, 1, "got {refused:?}");
        assert!(
            message.contains("line 2"),
            "the refusal does not name the line: {message:?}"
        );
        assert_eq!(device_names(&mut client).await, vec!["fake".to_string()]);
    })
    .await
    .expect("test timed out");
}

/// SIGHUP does what a reload request does: the file is re-read and the device set follows it.
#[tokio::test]
async fn sighup_reconciles_the_device_set_with_the_file() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut child, mut client, _socket) = daemon().await;
        std::fs::write(child.config_path(), format!("{CONFIG}{SECOND}"))
            .expect("add the second device");

        let signalled = Command::new("kill")
            .arg("-HUP")
            .arg(child.pid().to_string())
            .status()
            .expect("run kill");
        assert!(signalled.success(), "kill -HUP failed: {signalled:?}");

        // The reload is asynchronous: poll the device set the daemon answers with.
        for _ in 0..POLLS {
            if device_names(&mut client).await == vec!["fake", "second"] {
                return;
            }
            tokio::time::sleep(POLL).await;
        }
        panic!(
            "the daemon never reported the reloaded device set within {STEP_BUDGET:?}; it \
             exited with {:?} and said: {:?}",
            child.exited(),
            child.said()
        );
    })
    .await
    .expect("test timed out");
}

/// A stop request is answered and then acted on: the daemon ends as a SIGTERM would, and the
/// socket file it bound itself goes with it.
#[tokio::test]
async fn a_stop_request_ends_a_daemon_that_bound_its_own_socket() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut child, mut client, socket) = daemon().await;
        let answer = client.request(Request::Stop).await.expect("a stop reply");
        assert!(matches!(answer, Response::Stopping), "got {answer:?}");

        let status = child.wait_for_exit().await;
        assert!(status.success(), "the daemon exited with {status:?}");
        assert!(
            !socket.exists(),
            "a daemon that bound its own socket removes the file on the way out"
        );
    })
    .await
    .expect("test timed out");
}

/// A daemon whose socket file is taken out from under it stops rather than serving a path
/// nothing can reach while it holds every device.
///
/// That is the shape a socket unit starting over a daemon a front end had spawned leaves:
/// systemd unlinks the path to bind its own listener, and the daemon that had it would go on
/// holding the device against the service that replaced it.
#[tokio::test]
async fn a_daemon_whose_socket_is_taken_away_stops() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut child, socket) = spawn_daemon(CONFIG);
        let mut client = connect(&mut child, &socket).await;
        wait_until_connected(&mut child, &mut client).await;

        // Another listener on the same path, which is what a socket unit starting over a
        // running daemon leaves: the file is a different inode from the one this one bound.
        std::fs::remove_file(&socket).expect("the socket file is taken away");
        let taken =
            std::os::unix::net::UnixListener::bind(&socket).expect("another listener binds it");

        let status = child.wait_for_exit().await;
        assert!(status.success(), "the daemon exited with {status:?}");
        assert!(
            child
                .said()
                .contains("no longer the socket this daemon bound"),
            "the daemon did not say why it stopped: {:?}",
            child.said()
        );
        assert!(
            socket.exists(),
            "the daemon removed a socket file another listener had bound"
        );
        drop(taken);
    })
    .await
    .expect("test timed out");
}

/// A socket handed over on descriptor 3 is served without being bound again, and is left
/// behind when the daemon stops: the file belongs to whoever bound it.
#[tokio::test]
async fn a_handed_over_socket_is_served_and_left_behind() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut child, socket) = spawn_daemon_with_handed_over_socket(CONFIG);
        let mut client = connect(&mut child, &socket).await;
        wait_until_connected(&mut child, &mut client).await;

        let answer = client
            .request(Request::Version)
            .await
            .expect("a version reply");
        assert!(
            matches!(&answer, Response::Version { version, .. } if version == env!("CARGO_PKG_VERSION")),
            "got {answer:?}"
        );

        let answer = client.request(Request::Stop).await.expect("a stop reply");
        assert!(matches!(answer, Response::Stopping), "got {answer:?}");
        let status = child.wait_for_exit().await;
        assert!(status.success(), "the daemon exited with {status:?}");
        assert!(
            socket.exists(),
            "a handed-over socket file belongs to whoever bound it and must be left behind"
        );
    })
    .await
    .expect("test timed out");
}

/// The one-device config with `digits` as its passphrase entry.
fn with_passphrase(digits: &str) -> String {
    format!("{CONFIG}passphrase = \"{digits}\"\n")
}

/// Write the device's input control, which is the shortest round trip that has to reach the
/// device and so the shortest thing a refused passphrase shows up in.
async fn set_input(client: &mut Client) -> Response {
    client
        .request(Request::Set {
            device: None,
            key: "input".into(),
            value: "4".into(),
            step: 1,
            no_wait: false,
        })
        .await
        .expect("a set reply")
}

/// The profile the daemon reports for its one device.
async fn profile_of(client: &mut Client) -> String {
    one_device(client).await.profile
}

/// Whether the daemon reports its one device as connected.
async fn connected(client: &mut Client) -> bool {
    one_device(client).await.connected
}

/// The one device the daemon reports, or a failure naming what it reported instead.
async fn one_device(client: &mut Client) -> svolder_ipc::DeviceInfo {
    let response = client
        .request(Request::Devices)
        .await
        .expect("a devices reply");
    let Response::Devices { mut devices } = response else {
        panic!("expected a device list, got {response:?}");
    };
    assert_eq!(devices.len(), 1, "got {devices:?}");
    devices.remove(0)
}
