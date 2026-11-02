// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon end to end, over the in-process fake device: a real `svolderd` process, a
//! real Unix socket, and a real `svolder_ipc::Client`.

mod common;

use std::process::Command;
use std::time::Duration;

use futures::StreamExt;
use svolder_ipc::{Client, Event, Request, Response};

use common::{
    CONFIG, POLL, POLLS, STEP_BUDGET, TEST_TIMEOUT, connect, daemon, spawn_daemon,
    spawn_daemon_with_env, spawn_daemon_with_piped_stderr, wait_until_connected,
};

#[tokio::test]
async fn state_and_set_work_over_the_daemon() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client, _socket) = daemon().await;
        let resp = client
            .request(Request::State { device: None })
            .await
            .unwrap();
        assert!(
            matches!(&resp, Response::Values { values, .. } if values.iter().any(|o| o.key.as_deref() == Some("volume"))),
            "got {resp:?}"
        );
        let set = client
            .request(Request::Set {
                device: None,
                key: "input".into(),
                value: "4".into(),
                step: 1,
                no_wait: false,
            })
            .await
            .unwrap();
        assert!(
            matches!(&set, Response::Value(o) if o.raw == serde_json::json!(4)),
            "got {set:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A scan over the daemon carries the same row a direct one does, signal strength and
/// product id included: a front end reads the same fields whichever backend answered it.
#[tokio::test]
async fn a_scan_over_the_daemon_carries_the_signal_and_the_product_id() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client, _socket) = daemon().await;
        let resp = client
            .request(Request::Scan { window_ms: 10 })
            .await
            .expect("a scan reply");
        let Response::Scan { entries } = resp else {
            panic!("got {resp:?}");
        };
        let entry = entries.first().expect("the fake device is in the scan");
        assert_eq!(entry.unit, "0102030405060708");
        assert_eq!(entry.product_id, 2);
        assert_eq!(entry.rssi, Some(-50));
    })
    .await
    .expect("test timed out");
}

/// The firmware-update family is refused at the daemon, before any session
/// sees it.
#[tokio::test]
async fn restricted_raw_is_refused_by_the_daemon() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client, _socket) = daemon().await;
        let refused = client
            .request(Request::Raw {
                device: None,
                id: "210.1".into(),
                value_type: "u8".into(),
                value: "0".into(),
                no_wait: false,
            })
            .await
            .unwrap();
        assert!(
            matches!(&refused, Response::Error { code: 1, .. }),
            "got {refused:?}"
        );
        // A following state read still answers: the refusal touched nothing.
        assert!(matches!(
            client
                .request(Request::State { device: None })
                .await
                .unwrap(),
            Response::Values { .. }
        ));
    })
    .await
    .expect("test timed out");
}

/// The passphrase function is refused at the daemon too, in the passphrase verb's words, before
/// any session sees it: `raw` is no way to change the code.
#[tokio::test]
async fn raw_set_of_the_passphrase_function_is_refused_by_the_daemon() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client, _socket) = daemon().await;
        let refused = client
            .request(Request::Raw {
                device: None,
                id: "250".into(),
                value_type: "u32".into(),
                value: "1".into(),
                no_wait: false,
            })
            .await
            .unwrap();
        assert!(
            matches!(&refused, Response::Error { code: 1, message }
                if message.contains("function 250 changes the passphrase")),
            "got {refused:?}"
        );
        // A following state read still answers: the refusal touched nothing.
        assert!(matches!(
            client
                .request(Request::State { device: None })
                .await
                .unwrap(),
            Response::Values { .. }
        ));
    })
    .await
    .expect("test timed out");
}

/// `set mute up` is a usage error over the daemon, exactly as it is on the CLI's direct
/// path: a control with no range to move through is refused before a value to step from is
/// looked for, so the answer names the control rather than a missing reading.
#[tokio::test]
async fn set_up_on_a_control_that_cannot_be_stepped_is_a_usage_error() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client, _socket) = daemon().await;
        let refused = client
            .request(Request::Set {
                device: None,
                key: "mute".into(),
                value: "up".into(),
                step: 1,
                no_wait: false,
            })
            .await
            .unwrap();
        assert!(
            matches!(&refused, Response::Error { code: 1, message } if message == "control mute has no range to step through"),
            "got {refused:?}"
        );
        let unknown = client
            .request(Request::Set {
                device: None,
                key: "nope".into(),
                value: "up".into(),
                step: 1,
                no_wait: false,
            })
            .await
            .unwrap();
        assert!(
            matches!(&unknown, Response::Error { code: 1, message } if message.starts_with("no control named nope; this device has: ")),
            "got {unknown:?}"
        );
        let stepped = client
            .request(Request::Set {
                device: None,
                key: "volume".into(),
                value: "down".into(),
                step: 1,
                no_wait: false,
            })
            .await
            .unwrap();
        assert!(matches!(stepped, Response::Value(_)), "got {stepped:?}");
    })
    .await
    .expect("test timed out");
}

/// A device that never answers is reported at the default log level, so a journal says why
/// the daemon is not connected instead of staying silent about it.
#[tokio::test]
async fn a_device_that_is_never_found_is_reported_at_the_default_level() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        // A unit id the fake device does not advertise, so every attempt fails.
        let (mut guard, _socket) = spawn_daemon_with_piped_stderr(
            "schema = 1\n[[device]]\nname = \"absent\"\nudid = \"0908070605040302\"\n",
        );
        let stderr = guard.child_stderr().expect("the child's stderr was piped");
        // Reading the child's lines blocks, so it happens off the runtime; the guard stays
        // here, and dropping it on a failure kills the child and ends the read. Every line
        // read is kept, so a run that never sees the retry says what the daemon said instead.
        let (found, said) = tokio::task::spawn_blocking(move || {
            use std::io::{BufRead, BufReader};
            let mut said = String::new();
            let mut found = false;
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                found = line.contains("absent: connect failed:") && line.contains("retrying");
                said.push_str(&line);
                said.push('\n');
                if found {
                    break;
                }
            }
            (found, said)
        })
        .await
        .expect("the reader finished");
        assert!(
            found,
            "no connect failure was logged at the default level; the daemon said: {said:?}"
        );
        drop(guard);
    })
    .await
    .expect("test timed out");
}

/// SIGHUP reloads the profile registry without a restart: a user file dropped into the
/// config directory's `profiles/` relabels a control, and the very next `state` reads the
/// device through the reloaded profile.
///
/// A user file whose `name` matches a built-in merges over it, control by key, so this one
/// changes `input`'s label and nothing else.
#[tokio::test]
async fn sighup_reloads_the_profiles_a_running_daemon_reads() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut child, mut client, _socket) = daemon().await;
        assert_eq!(
            input_label(&mut client)
                .await
                .expect("a state reply")
                .as_deref(),
            Some("Input")
        );

        let profiles = child.profiles_dir();
        std::fs::create_dir_all(&profiles).expect("create the profiles directory");
        std::fs::write(
            profiles.join("gungnir2-v2.toml"),
            "schema = 1\nname = \"gungnir2-v2\"\n\
             [[control]]\nid = [2]\nkey = \"input\"\nlabel = \"Source\"\nkind = \"select\"\n\
             values = { 1 = \"TV\", 2 = \"Streamer\" }\n",
        )
        .expect("write the profile override");

        let signalled = Command::new("kill")
            .arg("-HUP")
            .arg(child.pid().to_string())
            .status()
            .expect("run kill");
        assert!(signalled.success(), "kill -HUP failed: {signalled:?}");

        // The reload is asynchronous: poll the label the daemon reads the device through.
        for _ in 0..POLLS {
            match input_label(&mut client).await {
                Ok(label) if label.as_deref() == Some("Source") => return,
                Ok(_) => {}
                Err(e) => panic!(
                    "the daemon stopped answering after the hangup ({e}); it exited with {:?} \
                     and said: {:?}",
                    child.exited(),
                    child.said()
                ),
            }
            tokio::time::sleep(POLL).await;
        }
        panic!("the daemon never reported the reloaded label within {STEP_BUDGET:?}");
    })
    .await
    .expect("test timed out");
}

/// The label the daemon currently reads the `input` control through, from a `state` reply, or
/// why the reply never came.
async fn input_label(client: &mut Client) -> Result<Option<String>, String> {
    let response = client
        .request(Request::State { device: None })
        .await
        .map_err(|e| e.to_string())?;
    let Response::Values { values, .. } = response else {
        panic!("expected readings, got {response:?}");
    };
    Ok(values
        .iter()
        .find(|o| o.key.as_deref() == Some("input"))
        .map(|o| o.label.clone()))
}

/// A subscription to a daemon with no devices configured stays open with nothing to say,
/// rather than ending at once as if the daemon had stopped: a front end watching it shows an
/// empty device list instead of reporting that the event stream failed.
#[tokio::test]
async fn a_subscription_with_no_devices_configured_stays_open() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut guard, socket) = spawn_daemon("schema = 1\n");
        let client = connect(&mut guard, &socket).await;
        let mut events = client.subscribe(None).await.expect("a subscription");
        // Not [`STEP_BUDGET`]: this is how long the stream must stay silent, so every extra
        // millisecond is time a passing run spends waiting, where a step's budget is time only
        // a failing run spends. A stream that ends does so as the daemon registers it, well
        // inside this.
        let ended = tokio::time::timeout(Duration::from_millis(300), events.next()).await;
        assert!(
            ended.is_err(),
            "the stream should have stayed open and silent, got {ended:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A no-wait `eq_mode` is acknowledged instead of answered with a reading: the daemon sends
/// the frame and does not wait for the echo, exactly as a no-wait `set` does. The echo still
/// reaches subscribers, since the actor reads it like any other status.
#[tokio::test]
async fn a_no_wait_eq_request_is_acknowledged() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut child, mut client, socket) = daemon().await;
        let sub = Client::connect(&socket).await.unwrap();
        let mut events = sub.subscribe(None).await.unwrap();
        let first = events.next().await.expect("a connection snapshot");
        assert!(matches!(first, Event::Connection { .. }), "got {first:?}");

        let sent = client
            .request(Request::EqMode {
                device: None,
                mode: "eq".into(),
                no_wait: true,
            })
            .await;
        let ack = sent.unwrap_or_else(|e| {
            panic!(
                "no answer to a no-wait eq_mode ({e}); the daemon exited with {:?} and said: {:?}",
                child.exited(),
                child.said()
            )
        });
        assert!(matches!(ack, Response::Ack), "got {ack:?}");
        let sent = client
            .request(Request::EqSet {
                device: None,
                band: 1,
                param: "f".into(),
                value: 50.0,
                no_wait: true,
            })
            .await;
        let ack = sent.unwrap_or_else(|e| {
            panic!(
                "no answer to a no-wait eq_set ({e}); the daemon exited with {:?} and said: {:?}",
                child.exited(),
                child.said()
            )
        });
        assert!(matches!(ack, Response::Ack), "got {ack:?}");

        let heard = tokio::time::timeout(STEP_BUDGET, async {
            let mut saw_mode = false;
            let mut saw_band = false;
            while !(saw_mode && saw_band)
                && let Some(ev) = events.next().await
            {
                if let Event::Status { reading, .. } = &ev {
                    saw_mode |= reading.label == "Tone control mode" && reading.display == "eq";
                    saw_band |= reading.label == "EQ band 1 frequency";
                }
            }
            (saw_mode, saw_band)
        })
        .await
        .unwrap_or_else(|_| panic!("subscriber heard no eq echo within {STEP_BUDGET:?}"));
        assert_eq!(heard, (true, true), "subscriber missed an eq echo");
    })
    .await
    .expect("test timed out");
}

/// Three front ends on one daemon each hear what a fourth one set: a set's status, carrying the
/// value that was set, is published to every subscriber, not to the first or the last one to
/// subscribe.
#[tokio::test]
async fn three_subscribers_each_hear_a_set_a_fourth_client_makes() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client, socket) = daemon().await;
        let mut subscribers = Vec::new();
        for _ in 0..3 {
            let sub = Client::connect(&socket).await.unwrap();
            let mut events = sub.subscribe(None).await.unwrap();
            // The connection snapshot every subscriber opens with proves the daemon has this
            // subscription registered before the set below is sent.
            let first = events.next().await.expect("a connection snapshot");
            assert!(matches!(first, Event::Connection { .. }), "got {first:?}");
            subscribers.push(events);
        }

        client
            .request(Request::Set {
                device: None,
                key: "mute".into(),
                value: "on".into(),
                step: 1,
                no_wait: false,
            })
            .await
            .unwrap();
        // One budget for all three, counted from the set's answer.
        let deadline = tokio::time::Instant::now() + STEP_BUDGET;
        for (nth, events) in subscribers.iter_mut().enumerate() {
            let heard = tokio::time::timeout_at(deadline, async {
                while let Some(ev) = events.next().await {
                    if matches!(&ev, Event::Status { reading, .. }
                        if reading.key.as_deref() == Some("mute") && reading.display == "on")
                    {
                        return Some(ev);
                    }
                }
                None
            })
            .await
            .unwrap_or_else(|_| panic!("subscriber {nth} heard no mute on within {STEP_BUDGET:?}"));
            assert!(heard.is_some(), "subscriber {nth}'s stream ended first");
        }
    })
    .await
    .expect("test timed out");
}

/// The fake amp advertises a product id the codec's table does not know, so the daemon names
/// it after the profile made for it, both in a scan's answer and in the device it holds.
#[tokio::test]
async fn the_daemon_names_the_fake_amp_after_its_profile() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut guard, socket) =
            spawn_daemon_with_env(CONFIG, &[("SVOLDER_FAKE_PROFILE", "fake-amp")]);
        let mut client = connect(&mut guard, &socket).await;
        wait_until_connected(&mut guard, &mut client).await;

        let scanned = client
            .request(Request::Scan { window_ms: 100 })
            .await
            .unwrap();
        assert!(
            matches!(&scanned, Response::Scan { entries } if entries.iter().any(|e| e.product.as_deref() == Some("Fake amp"))),
            "got {scanned:?}"
        );
        let listed = client.request(Request::Devices).await.unwrap();
        assert!(
            matches!(&listed, Response::Devices { devices } if devices.iter().any(|d| d.product.as_deref() == Some("Fake amp"))),
            "got {listed:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// Readings nobody asked for reach subscribers too: the fake amplifier's meters play a
/// signal of their own, and the daemon forwards each reading as the status it is, so a front
/// end on the far side of the socket sees the meters move.
#[tokio::test]
async fn a_subscriber_hears_the_signal_on_the_meters() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut guard, socket) = spawn_daemon_with_env(
            CONFIG,
            &[
                ("SVOLDER_FAKE_PROFILE", "fake-amp"),
                ("SVOLDER_FAKE_SIGNAL", "1"),
            ],
        );
        let mut client = connect(&mut guard, &socket).await;
        wait_until_connected(&mut guard, &mut client).await;
        let sub = Client::connect(&socket).await.unwrap();
        let mut events = sub.subscribe(None).await.unwrap();
        let mut readings = 0;
        while readings < 3 {
            let ev = tokio::time::timeout(STEP_BUDGET, events.next())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "only {readings} meter reading(s) in {STEP_BUDGET:?}; the daemon said: \
                         {:?}",
                        guard.said()
                    )
                })
                .expect("stream open");
            if matches!(&ev, Event::Status { reading, .. } if reading.key.as_deref() == Some("meter_1"))
            {
                readings += 1;
            }
        }
    })
    .await
    .expect("test timed out");
}

#[tokio::test]
async fn subscribe_yields_a_status_after_a_set() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client, socket) = daemon().await;
        let sub = Client::connect(&socket).await.unwrap();
        let mut events = sub.subscribe(None).await.unwrap();
        // Every subscriber opens with a connection snapshot, which also proves the daemon
        // has this subscription registered before the set below is sent.
        let first = events.next().await.expect("a connection snapshot");
        assert!(matches!(first, Event::Connection { .. }), "got {first:?}");

        client
            .request(Request::Set {
                device: None,
                key: "mute".into(),
                value: "on".into(),
                step: 1,
                no_wait: false,
            })
            .await
            .unwrap();
        let ev = tokio::time::timeout(STEP_BUDGET, events.next())
            .await
            .unwrap_or_else(|_| panic!("no event within {STEP_BUDGET:?}"))
            .expect("stream open");
        assert!(
            matches!(&ev, Event::Status { reading, .. } if reading.key.as_deref() == Some("mute")),
            "got {ev:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// Take `flock(LOCK_EX | LOCK_NB)` on `path` the way a daemon does, returning the file that
/// holds it, or the error.
fn flock(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::fd::AsRawFd;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    // SAFETY: the descriptor belongs to `file`, which outlives the call.
    match unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } {
        0 => Ok(file),
        _ => Err(std::io::Error::last_os_error()),
    }
}

fn lock_of(socket: &std::path::Path) -> std::path::PathBuf {
    let mut name = socket.as_os_str().to_owned();
    name.push(".lock");
    name.into()
}

/// The lock file alone keeps a second daemon out: this test holds the lock with no daemon
/// running and no socket file, so the refusal checks see nothing to refuse.
#[tokio::test]
async fn a_daemon_refuses_to_start_while_the_socket_lock_is_held() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut child, socket) = common::spawn_daemon_on(CONFIG, |dir| {
            // Hold the lock before the daemon starts, at the path it will derive.
            let socket = dir.join("svolder.sock");
            std::mem::forget(flock(&lock_of(&socket)).expect("the test takes the lock"));
            socket
        });
        let status = child.wait_for_exit().await;
        assert_eq!(
            status.code(),
            Some(3),
            "the daemon said: {:?}",
            child.said()
        );
        assert!(
            child.said().contains("another svolderd is already running"),
            "the daemon said: {:?}",
            child.said()
        );
        assert!(!socket.exists(), "a refused daemon binds nothing");
    })
    .await
    .expect("test timed out");
}

/// The lock file is `<socket>.lock` at mode 0600 and outlives the daemon that made it; a
/// daemon started after the first exits takes the lock again and serves.
#[tokio::test]
async fn the_lock_file_is_private_stays_behind_and_is_taken_again() {
    use std::os::unix::fs::PermissionsExt;
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut first, socket) = spawn_daemon(CONFIG);
        let mut client = connect(&mut first, &socket).await;
        let lock = lock_of(&socket);
        let mode = std::fs::metadata(&lock)
            .expect("the lock file exists")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        // A running daemon holds the lock.
        assert!(
            flock(&lock).is_err(),
            "the lock is held while the daemon runs"
        );
        let answer = client.request(Request::Stop).await.expect("a stop reply");
        assert!(matches!(answer, Response::Stopping), "got {answer:?}");
        let status = first.wait_for_exit().await;
        assert!(status.success(), "the daemon exited with {status:?}");
        assert!(lock.exists(), "the lock file stays after the daemon exits");
        drop(flock(&lock).expect("the lock is free once the daemon has exited"));

        // The next daemon on the same socket path gets the lock and serves.
        let (mut third, _) = common::spawn_daemon_on(CONFIG, |_| socket.clone());
        let _client = connect(&mut third, &socket).await;
        assert!(
            third.exited().is_none(),
            "the daemon said: {:?}",
            third.said()
        );
    })
    .await
    .expect("test timed out");
}

/// A socket that cannot be bound ends the daemon before a device is touched. A socket path
/// longer than `sun_path` fails at the bind; the device actor logs `actor stopped` when
/// `Daemon::shutdown` winds it down, so a log with no line naming the device shows that no
/// actor was ever started.
#[tokio::test]
async fn a_bind_that_fails_ends_the_daemon_before_any_device_is_touched() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut child, _socket) =
            common::spawn_daemon_on(CONFIG, |dir| dir.join(format!("{}.sock", "s".repeat(150))));
        let status = child.wait_for_exit().await;
        assert_eq!(
            status.code(),
            Some(1),
            "the daemon said: {:?}",
            child.said()
        );
        let said = child.said();
        assert!(said.contains(".sock"), "the error names the path: {said:?}");
        assert!(
            !said.contains("fake"),
            "a device actor started before the bind failed: {said:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// Over the real socket, a connected fake device answers `history` with its one connection
/// transition, an unknown device with the usual refusal, and a span over a day as a day.
#[tokio::test]
async fn history_over_the_daemon_reports_the_connection() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut guard, socket) = spawn_daemon(CONFIG);
        let mut client = connect(&mut guard, &socket).await;
        wait_until_connected(&mut guard, &mut client).await;
        let reply = client
            .request(Request::History {
                device: None,
                span_secs: 10 * 24 * 3600,
            })
            .await
            .unwrap();
        let Response::History {
            device,
            now_ms,
            transitions,
        } = reply
        else {
            panic!("got {reply:?}");
        };
        assert_eq!(device, "fake");
        assert_eq!(transitions.len(), 1, "{transitions:?}");
        assert!(transitions[0].connected);
        assert!(transitions[0].at_ms <= now_ms);
        let refused = client
            .request(Request::History {
                device: Some("nope".into()),
                span_secs: 60,
            })
            .await
            .unwrap();
        assert!(
            matches!(refused, Response::Error { code: 2, .. }),
            "got {refused:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// A link the device drops is noticed and made again: the history holds the link going down and
/// coming back, `devices` reports it connected and fresh, a `set` is answered on the new link,
/// and the journal says the link was lost.
#[tokio::test]
async fn a_link_the_device_drops_is_made_again() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut guard, socket) =
            spawn_daemon_with_env(CONFIG, &[("SVOLDER_FAKE_SEVER_AFTER_MS", "500")]);
        let mut client = connect(&mut guard, &socket).await;
        let mut changes: Vec<bool> = Vec::new();
        for _ in 0..POLLS {
            let reply = client
                .request(Request::History {
                    device: None,
                    span_secs: 3600,
                })
                .await
                .expect("a history reply");
            let Response::History { transitions, .. } = reply else {
                panic!("got {reply:?}");
            };
            changes = transitions.iter().map(|t| t.connected).collect();
            if changes == [true, false, true] {
                break;
            }
            tokio::time::sleep(POLL).await;
        }
        assert_eq!(
            changes,
            [true, false, true],
            "the link was not lost and made again within {STEP_BUDGET:?}; the daemon said: {:?}",
            guard.said()
        );
        // The history shows the link up again before the device's first refresh has landed, so
        // `devices` is polled for fresh rather than read once.
        let mut listed = client.request(Request::Devices).await.unwrap();
        for _ in 0..POLLS {
            if matches!(&listed, Response::Devices { devices } if devices.iter().all(|d| d.connected && !d.stale))
            {
                break;
            }
            tokio::time::sleep(POLL).await;
            listed = client.request(Request::Devices).await.unwrap();
        }
        assert!(
            matches!(&listed, Response::Devices { devices } if devices.iter().all(|d| d.connected && !d.stale)),
            "the device was not connected and fresh within {STEP_BUDGET:?}: {listed:?}"
        );
        let set = client
            .request(Request::Set {
                device: None,
                key: "input".into(),
                value: "4".into(),
                step: 1,
                no_wait: false,
            })
            .await
            .unwrap();
        assert!(
            matches!(&set, Response::Value(o) if o.raw == serde_json::json!(4)),
            "got {set:?}"
        );
        assert!(
            guard.said().contains("fake: link lost"),
            "the daemon said: {:?}",
            guard.said()
        );
    })
    .await
    .expect("test timed out");
}

/// The volume's raw step, as `get` answers it.
async fn volume_raw(client: &mut Client) -> i64 {
    match client
        .request(Request::Get {
            device: None,
            key: "volume".into(),
        })
        .await
        .expect("a get reply")
    {
        Response::Value(vo) => vo.raw.as_i64().expect("a raw step"),
        other => panic!("{other:?}"),
    }
}

/// Two relative steps sent without waiting for their echoes move the volume two steps: the
/// second is not worked out from a reading the first has not changed yet.
#[tokio::test]
async fn two_relative_steps_sent_without_waiting_move_two_steps() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client, _socket) = daemon().await;
        let before = volume_raw(&mut client).await;
        for _ in 0..2 {
            let ack = client
                .request(Request::Set {
                    device: None,
                    key: "volume".into(),
                    value: "up".into(),
                    step: 1,
                    no_wait: true,
                })
                .await
                .expect("a set reply");
            assert!(matches!(ack, Response::Ack), "{ack:?}");
        }
        let mut now = before;
        for _ in 0..POLLS {
            now = volume_raw(&mut client).await;
            if now == before + 2 {
                break;
            }
            tokio::time::sleep(POLL).await;
        }
        assert_eq!(now, before + 2, "two steps up from {before}");
    })
    .await
    .expect("test timed out");
}

/// Over the real socket, a `readings` request answers the volume a set moved, at a wall-clock
/// time at or before the daemon's own, binned by the second, with the link's change.
#[tokio::test]
async fn readings_over_the_daemon_carry_the_volume_set() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client, _socket) = daemon().await;
        // 90: below three quarters, so it lands at once, and above the fake's 63, so the 1 s
        // bin's maximum is the set whichever bin the connect's reading fell in.
        let set = client
            .request(Request::Set {
                device: None,
                key: "volume".into(),
                value: "90".into(),
                step: 1,
                no_wait: false,
            })
            .await
            .unwrap();
        assert!(matches!(set, Response::Value(_)), "{set:?}");
        let reply = client
            .request(Request::Readings {
                device: None,
                span_secs: 99_999,
            })
            .await
            .unwrap();
        let Response::Readings {
            device,
            now_ms,
            bin_ms,
            series,
            link,
            ..
        } = reply
        else {
            panic!("got {reply:?}");
        };
        assert_eq!((device.as_str(), bin_ms), ("fake", 1000));
        let volume = series
            .iter()
            .find(|s| s.key.as_deref() == Some("volume"))
            .expect("the volume's series");
        let &(at_ms, last) = volume.points.last().expect("a point");
        assert_eq!(last, 90.0);
        assert!(at_ms <= now_ms);
        assert!(link.iter().any(|change| change.connected), "{link:?}");
    })
    .await
    .expect("test timed out");
}

/// A `recalled` request naming more characters than a preset's name can have is refused, and the
/// device's memory stays as it was.
#[tokio::test]
async fn a_recalled_name_longer_than_a_preset_name_is_refused() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client, _socket) = daemon().await;
        let reply = client
            .request(Request::Recalled {
                device: None,
                name: "x".repeat(41),
            })
            .await
            .unwrap();
        assert!(matches!(reply, Response::Error { .. }), "{reply:?}");
        let Response::Devices { devices } = client.request(Request::Devices).await.unwrap() else {
            panic!("no device list");
        };
        assert_eq!(devices[0].recalled, None);
    })
    .await
    .expect("test timed out");
}

/// A `recalled` request is acknowledged and shows in the device's row.
#[tokio::test]
async fn a_recalled_request_shows_in_the_device_row() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (_child, mut client, _socket) = daemon().await;
        let ack = client
            .request(Request::Recalled {
                device: None,
                name: "Warm".into(),
            })
            .await
            .unwrap();
        assert_eq!(ack, Response::Ack);
        let mut recalled = None;
        for _ in 0..POLLS {
            let Response::Devices { devices } = client.request(Request::Devices).await.unwrap()
            else {
                panic!("no device list");
            };
            recalled = devices[0].recalled.clone();
            if recalled.is_some() {
                break;
            }
            tokio::time::sleep(POLL).await;
        }
        assert_eq!(recalled.as_deref(), Some("Warm"));
    })
    .await
    .expect("test timed out");
}

/// A request to set `key` to `value` on the daemon's only device, answered or failed.
async fn set(client: &mut Client, key: &str, value: &str) -> Response {
    client
        .request(Request::Set {
            device: None,
            key: key.into(),
            value: value.into(),
            step: 1,
            no_wait: false,
        })
        .await
        .expect("a set reply")
}

/// The Loki Max's first band, raw step, as `get` answers it; `None` until it has a reading.
async fn band1_raw(client: &mut Client) -> Option<i64> {
    match client
        .request(Request::Get {
            device: None,
            key: "band1".into(),
        })
        .await
        .expect("a get reply")
    {
        Response::Value(vo) => vo.raw.as_i64(),
        _ => None,
    }
}

/// Poll until band 1 reads `raw`, for as long as a step is given; what it read last otherwise.
async fn band1_reaches(client: &mut Client, raw: i64) -> Option<i64> {
    let mut now = None;
    for _ in 0..POLLS {
        now = band1_raw(client).await;
        if now == Some(raw) {
            break;
        }
        tokio::time::sleep(POLL).await;
    }
    now
}

/// Save the bands to slot 1 over the socket, move band 1 on, and recall the slot: the device
/// reports the saved band and the daemon's reading follows it.
#[tokio::test]
async fn set_recall_over_the_socket_brings_the_saved_bands_back() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut guard, socket) =
            spawn_daemon_with_env(CONFIG, &[("SVOLDER_FAKE_PROFILE", "lokimax")]);
        let mut client = connect(&mut guard, &socket).await;
        wait_until_connected(&mut guard, &mut client).await;

        let band = set(&mut client, "band1", "20").await;
        assert!(
            matches!(&band, Response::Value(o) if o.raw == serde_json::json!(20)),
            "{band:?}"
        );
        let saved = set(&mut client, "save", "1").await;
        assert!(matches!(saved, Response::Ack), "{saved:?}");
        let moved = set(&mut client, "band1", "10").await;
        assert!(
            matches!(&moved, Response::Value(o) if o.raw == serde_json::json!(10)),
            "{moved:?}"
        );

        let recalled = set(&mut client, "recall", "Preset 1").await;
        assert!(matches!(recalled, Response::Ack), "{recalled:?}");
        let now = band1_reaches(&mut client, 20).await;
        assert_eq!(
            now,
            Some(20),
            "the saved band is back; the daemon said: {:?}",
            guard.said()
        );
    })
    .await
    .expect("test timed out");
}

/// A unit that reports nothing after a recall is asked once, a second after, for everything it
/// holds, so the daemon's readings catch up with the bands the recall set.
#[tokio::test]
async fn a_silent_recall_over_the_socket_is_followed_by_one_refresh() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let (mut guard, socket) = spawn_daemon_with_env(
            CONFIG,
            &[
                ("SVOLDER_FAKE_PROFILE", "lokimax"),
                ("SVOLDER_FAKE_SILENT_RECALL", "1"),
            ],
        );
        let mut client = connect(&mut guard, &socket).await;
        wait_until_connected(&mut guard, &mut client).await;

        set(&mut client, "band1", "20").await;
        let saved = set(&mut client, "save", "1").await;
        assert!(matches!(saved, Response::Ack), "{saved:?}");
        set(&mut client, "band1", "10").await;

        let recalled = set(&mut client, "recall", "1").await;
        assert!(matches!(recalled, Response::Ack), "{recalled:?}");
        // The device set band 1 back to 20 and said nothing: the reading is still the old one.
        assert_eq!(band1_raw(&mut client).await, Some(10));
        // The refresh that follows is what brings it up to date.
        let now = band1_reaches(&mut client, 20).await;
        assert_eq!(
            now,
            Some(20),
            "the refresh never came; the daemon said: {:?}",
            guard.said()
        );
    })
    .await
    .expect("test timed out");
}

/// A write of the value a control holds, to a unit that reports a change only: the daemon waits
/// out the set timeout and answers with the value the device holds, not with an error.
async fn a_repeat_answers_with_what_the_device_holds(
    profile: &str,
    knobs: &[(&str, &str)],
    requests: Vec<Request>,
) {
    let mut envs = vec![("SVOLDER_FAKE_PROFILE", profile)];
    envs.extend_from_slice(knobs);
    let (mut guard, socket) = spawn_daemon_with_env(CONFIG, &envs);
    let mut client = connect(&mut guard, &socket).await;
    wait_until_connected(&mut guard, &mut client).await;
    for request in requests {
        let label = format!("{request:?}");
        let answer = client.request(request).await.expect("a reply");
        assert!(
            matches!(answer, Response::Value(_)),
            "{label} answered {answer:?}; the daemon said: {:?}",
            guard.said()
        );
    }
}

fn set_request(key: &str, value: &str) -> Request {
    Request::Set {
        device: None,
        key: key.into(),
        value: value.into(),
        step: 1,
        no_wait: false,
    }
}

/// The tone-control mode set to the mode the device is already in is not an error, and the
/// writes after it still go through.
#[tokio::test]
async fn a_mode_set_to_the_mode_already_on_is_answered_with_the_mode() {
    tokio::time::timeout(TEST_TIMEOUT, async {
        let mode = |mode: &str| Request::EqMode {
            device: None,
            mode: mode.into(),
            no_wait: false,
        };
        // flat is the resting mode; eq, then eq again, then flat.
        a_repeat_answers_with_what_the_device_holds(
            "bifrost3",
            &[],
            vec![mode("flat"), mode("eq"), mode("eq"), mode("flat")],
        )
        .await;
    })
    .await
    .expect("test timed out");
}

/// Under `quiet_same` a toggle, a selector, a volume step that lands on the held value and a
/// band set to its value are each answered with what the device holds, and the write queue
/// carries on.
#[tokio::test]
async fn repeats_to_a_unit_that_reports_changes_only_are_answered_with_the_value() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let band = |value: f64| Request::EqSet {
            device: None,
            band: 1,
            param: "g".into(),
            value,
            no_wait: false,
        };
        a_repeat_answers_with_what_the_device_holds(
            "gungnir2-v2",
            &[("SVOLDER_FAKE_QUIET_SAME", "1")],
            vec![
                set_request("mute", "on"),
                set_request("mute", "on"),
                set_request("input", "4"),
                set_request("input", "4"),
                set_request("volume", "100"),
                set_request("volume", "100"),
                band(3.0),
                band(3.0),
            ],
        )
        .await;
    })
    .await
    .expect("test timed out");
}

/// A band of a Loki Max set to the value it holds, with a recall of a slot behind it.
#[tokio::test]
async fn a_loki_max_band_repeat_does_not_wedge_the_line() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (mut guard, socket) = spawn_daemon_with_env(
            CONFIG,
            &[
                ("SVOLDER_FAKE_PROFILE", "lokimax"),
                ("SVOLDER_FAKE_QUIET_SAME", "1"),
            ],
        );
        let mut client = connect(&mut guard, &socket).await;
        wait_until_connected(&mut guard, &mut client).await;
        for _ in 0..2 {
            let answer = set(&mut client, "band1", "20").await;
            assert!(
                matches!(answer, Response::Value(_)),
                "{answer:?}; the daemon said: {:?}",
                guard.said()
            );
        }
        let saved = set(&mut client, "save", "1").await;
        assert!(matches!(saved, Response::Ack), "{saved:?}");
        let recalled = set(&mut client, "recall", "1").await;
        assert!(matches!(recalled, Response::Ack), "{recalled:?}");
        let again = set(&mut client, "band1", "20").await;
        assert!(matches!(again, Response::Value(_)), "{again:?}");
        assert_eq!(band1_raw(&mut client).await, Some(20));
    })
    .await
    .expect("test timed out");
}
