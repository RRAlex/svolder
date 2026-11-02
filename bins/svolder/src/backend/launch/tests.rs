// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::ffi::OsStr;
use std::sync::Mutex;

use svolder_ipc::checks::UnitFacts;
use svolder_ipc::{Client, Request, Response};

use super::*;
use crate::backend::Globals;
use crate::test_support::{daemons_on, install_script, temp_dir};

/// The environment entry `name=value` as a child takes it.
fn entry(name: &str, value: impl AsRef<OsStr>) -> (OsString, OsString) {
    (name.into(), value.as_ref().to_os_string())
}

/// A host with no systemd, whose daemon is the `svolderd` the workspace built, logging and
/// running inside one temporary directory.
struct TestHost {
    /// The directory that stands in for the runtime, state and configuration homes.
    root: PathBuf,
    /// What systemd says, as the case under test wants it said.
    units: SystemdFacts,
    /// The daemon to start, `None` for a case where none may be started.
    binary: Option<PathBuf>,
    /// Every time the socket unit was asked to start, and the listener that answered it.
    started: Mutex<Vec<String>>,
    /// The listener a started socket unit leaves behind, as systemd would.
    listening: Mutex<Option<std::os::unix::net::UnixListener>>,
    /// Where the socket unit listens once it has been started.
    listen_at: Option<PathBuf>,
}

impl TestHost {
    /// A host with no systemd at all, which is every machine the daemon is autospawned on.
    fn without_systemd(root: &Path, binary: Option<PathBuf>) -> TestHost {
        TestHost {
            root: root.to_path_buf(),
            units: svolder_ipc::checks::live::absent(),
            binary,
            started: Mutex::new(Vec::new()),
            listening: Mutex::new(None),
            listen_at: None,
        }
    }

    /// A host whose socket unit is installed, in the state `unit` describes, listening on
    /// `listen_at` once it has been started.
    fn with_socket_unit(root: &Path, unit: UnitFacts, listen_at: Option<PathBuf>) -> TestHost {
        TestHost {
            root: root.to_path_buf(),
            units: SystemdFacts {
                present: true,
                socket_unit: Some(unit),
                service_unit: Some(UnitFacts {
                    installed: true,
                    enabled: true,
                    active: false,
                    running: false,
                }),
                listen_stream: listen_at.clone(),
                bluetoothd_exec_start: None,
                unreadable: None,
            },
            binary: None,
            started: Mutex::new(Vec::new()),
            listening: Mutex::new(None),
            listen_at,
        }
    }

    /// The units the socket unit was asked to start.
    fn starts(&self) -> Vec<String> {
        self.started.lock().expect("the record").clone()
    }
}

impl Host for TestHost {
    fn units(&self) -> SystemdFacts {
        self.units.clone()
    }

    fn start_socket(&self) -> Result<(), String> {
        self.started
            .lock()
            .expect("the record")
            .push(SOCKET_UNIT.to_string());
        // Starting a socket unit binds its listener, which is what makes the path answer.
        if let Some(at) = self.listen_at.as_deref() {
            let listener = std::os::unix::net::UnixListener::bind(at).map_err(|e| e.to_string())?;
            *self.listening.lock().expect("the listener") = Some(listener);
        }
        Ok(())
    }

    fn daemon_binary(&self) -> Option<PathBuf> {
        self.binary.clone()
    }

    fn log(&self) -> PathBuf {
        crate::state::log_path_in(&self.root)
    }

    fn daemon_env(&self) -> Vec<(OsString, OsString)> {
        vec![
            entry("SVOLDER_TRANSPORT", "fake"),
            entry("SVOLDER_HOST", "off"),
            entry("HOME", &self.root),
            entry("XDG_CONFIG_HOME", self.root.join("config")),
            entry("XDG_STATE_HOME", &self.root),
            entry("XDG_RUNTIME_DIR", &self.root),
        ]
    }
}

/// Everything a device verb needs, for a case that only cares about the transport and the
/// socket.
fn globals<'a>(transport: &'a str, socket: &'a str) -> Globals<'a> {
    Globals {
        transport,
        adapter: None,
        socket: Some(socket),
        device: None,
        verbose: false,
    }
}

/// The `svolderd` this test drives: the one the workspace build leaves in the target
/// directory, beside the directory this test binary itself runs from.
///
/// `make test` builds every binary before it runs a test and says so with
/// `SVOLDER_TEST_PREBUILT`, so a run that set it and still has no daemon is a broken build
/// rather than a case to skip.
fn prebuilt_daemon() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let binary = exe.parent()?.parent()?.join("svolderd");
    if binary.is_file() {
        return Some(binary);
    }
    assert!(
        std::env::var_os("SVOLDER_TEST_PREBUILT").is_none(),
        "SVOLDER_TEST_PREBUILT is set but {} is not there",
        binary.display()
    );
    eprintln!(
        "skipping: no svolderd at {}; build the workspace (`make test`) to run this case",
        binary.display()
    );
    None
}

/// Stops every daemon a case started on one socket, however the case ends.
///
/// A case that fails an assertion never reaches its last line, and a daemon it started is
/// in a session of its own: nothing else would ever end it. So the stopping is a drop, and
/// it is synchronous — the request goes over a plain socket rather than a client of a
/// runtime that may be unwinding.
struct Daemons(PathBuf);

impl Drop for Daemons {
    fn drop(&mut self) {
        use std::io::Write;
        if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(&self.0)
            && let Ok(line) = svolder_ipc::to_line(&svolder_ipc::RequestLine {
                id: 1,
                request: Request::Stop,
            })
        {
            let _ = stream.write_all(line.as_bytes());
        }
        for _ in 0..50 {
            let running = daemons_on(&self.0);
            if running.is_empty() {
                return;
            }
            std::thread::sleep(POLL);
            for pid in running {
                // SAFETY: `kill` takes a pid and a signal and has no preconditions; the pid
                // is one this case started and the signal is a termination request.
                unsafe { libc::kill(pid as i32, libc::SIGTERM) };
            }
        }
    }
}

/// Wait for the daemons on `socket` to settle at `want`, and say how many there are.
async fn daemons_settle_at(socket: &Path, want: usize) -> Vec<u32> {
    let mut running = daemons_on(socket);
    for _ in 0..30 {
        if running.len() == want {
            break;
        }
        tokio::time::sleep(POLL).await;
        running = daemons_on(socket);
    }
    running
}

/// Two front ends starting at the same moment on a machine with no systemd end up talking
/// to one daemon between them, and it logs where this machine keeps its state.
///
/// Nothing here touches a radio: the daemon is started over the fake transport, in a
/// directory of this case's own.
#[tokio::test]
async fn nothing_listening_and_no_systemd_autospawns_one_daemon_for_two_clients() {
    let Some(binary) = prebuilt_daemon() else {
        return;
    };
    let root = temp_dir("launch-autospawn");
    let socket = root.0.join("svolder.sock");
    let _daemons = Daemons(socket.clone());
    let host = TestHost::without_systemd(&root.0, Some(binary));
    let at = socket.to_string_lossy().into_owned();
    let config = Config::default();

    let (one, two) = (globals("auto", &at), globals("auto", &at));
    let (first, second) = tokio::join!(
        reach_on(&host, &one, &config),
        reach_on(&host, &two, &config),
    );
    let reached = Reached::Daemon(socket.clone());
    assert_eq!(
        first.as_ref().ok(),
        Some(&reached),
        "the first front end reached no daemon: {first:?}"
    );
    assert_eq!(
        second.as_ref().ok(),
        Some(&reached),
        "the second front end reached no daemon: {second:?}"
    );
    assert_eq!(
        daemons_settle_at(&socket, 1).await.len(),
        1,
        "two front ends left more than one daemon running"
    );
    assert!(
        crate::state::log_path_in(&root.0).is_file(),
        "the daemon's output goes to the log this machine keeps for it"
    );
}

/// With autospawn off, a machine with no daemon runs the verb in a session of its own —
/// and `--transport daemon`, which has no such step, says so in words that name the verb
/// that installs one.
#[tokio::test]
async fn autospawn_off_falls_back_to_a_direct_session_and_transport_daemon_fails_cleanly() {
    let root = temp_dir("launch-no-autospawn");
    let socket = root.0.join("svolder.sock");
    let at = socket.to_string_lossy().into_owned();
    let host = TestHost::without_systemd(&root.0, None);
    let mut config = Config::default();
    config.daemon.autospawn = false;

    let reached = reach_on(&host, &globals("auto", &at), &config)
        .await
        .expect("a direct session");
    assert_eq!(
        reached,
        Reached::Direct {
            autospawn_failed: None
        },
        "with autospawn off nothing was tried, so there is nothing to say about it"
    );

    let failure = reach_on(&host, &globals("daemon", &at), &config)
        .await
        .expect_err("--transport daemon insists on a daemon");
    assert_eq!(failure.code, Kind::NoDaemon.code());
    assert!(failure.what.contains("no daemon"), "{}", failure.what);
    assert!(
        failure.next.contains("svolder daemon install"),
        "{}",
        failure.next
    );
    assert!(
        !socket.exists(),
        "nothing was started, so nothing bound the socket"
    );
}

/// A socket unit that is installed but not listening is started, and the daemon behind it
/// is the one the verb runs on: nothing of this process's own is spawned.
#[tokio::test]
async fn an_installed_socket_unit_is_started_rather_than_a_daemon_of_this_process_s_own() {
    let root = temp_dir("launch-socket-unit");
    let socket = root.0.join("svolder.sock");
    let at = socket.to_string_lossy().into_owned();
    let host = TestHost::with_socket_unit(
        &root.0,
        UnitFacts {
            installed: true,
            enabled: true,
            active: false,
            running: false,
        },
        Some(socket.clone()),
    );

    let reached = reach_on(&host, &globals("auto", &at), &Config::default())
        .await
        .expect("the socket unit answers once it is started");
    assert_eq!(reached, Reached::Daemon(socket.clone()));
    assert_eq!(host.starts(), vec![SOCKET_UNIT.to_string()]);
    assert!(
        daemons_on(&socket).is_empty(),
        "the socket unit's own service is systemd's to start"
    );
}

/// A socket unit that is installed and active but answers somewhere else is still
/// systemd's: nothing of this process's own takes the path it owns, and the verb runs in a
/// session of its own instead.
#[tokio::test]
async fn a_socket_unit_that_holds_the_path_is_never_raced_by_an_autospawn() {
    let root = temp_dir("launch-socket-unit-active");
    let socket = root.0.join("svolder.sock");
    let at = socket.to_string_lossy().into_owned();
    let host = TestHost::with_socket_unit(
        &root.0,
        UnitFacts {
            installed: true,
            enabled: true,
            active: true,
            running: true,
        },
        None,
    );

    let reached = reach_on(&host, &globals("auto", &at), &Config::default())
        .await
        .expect("a direct session");
    assert_eq!(
        reached,
        Reached::Direct {
            autospawn_failed: None
        }
    );
    assert!(host.starts().is_empty(), "an active unit is not started");
    // `daemon_binary` answers `None` on this host, so an autospawn would have failed
    // loudly rather than quietly: what this asserts is that it was never reached for.
    assert!(!socket.exists(), "nothing bound the socket");
}

/// A daemon is started with a transport it opens, or with none at all: `auto` and `daemon`
/// are how a front end asks for a daemon, and a daemon handed either exits without ever
/// answering.
#[test]
fn a_daemon_is_only_handed_a_transport_it_opens() {
    let env = |transport: &'static str| {
        daemon_env_from(move |name| match name {
            "SVOLDER_TRANSPORT" => Some(transport.into()),
            "SVOLDER_ADAPTER" => Some("hci1".into()),
            _ => None,
        })
    };
    for passed in ["ble", "fake"] {
        assert_eq!(
            env(passed),
            vec![
                entry("SVOLDER_TRANSPORT", passed),
                entry("SVOLDER_ADAPTER", "hci1")
            ],
            "{passed}"
        );
    }
    for dropped in ["auto", "daemon", "", "carrier-pigeon"] {
        assert_eq!(
            env(dropped),
            vec![entry("SVOLDER_ADAPTER", "hci1")],
            "{dropped} is not a transport the daemon opens"
        );
    }
    assert_eq!(daemon_env_from(|_| None), Vec::new());
}

/// A daemon that is already answering is the answer: systemd is not asked anything.
#[tokio::test]
async fn a_daemon_that_answers_ends_the_chain_at_once() {
    let root = temp_dir("launch-answering");
    let socket = root.0.join("svolder.sock");
    let at = socket.to_string_lossy().into_owned();
    let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind the test socket");
    let host = TestHost::with_socket_unit(
        &root.0,
        UnitFacts {
            installed: true,
            enabled: true,
            active: false,
            running: false,
        },
        None,
    );

    for transport in ["auto", "daemon"] {
        let reached = reach_on(&host, &globals(transport, &at), &Config::default())
            .await
            .expect("the daemon that is answering");
        assert_eq!(reached, Reached::Daemon(socket.clone()), "{transport}");
    }
    assert!(
        host.starts().is_empty(),
        "nothing needed starting: the socket answers"
    );
}

/// A probe is step 1 and nothing else: it answers with the socket a daemon is already on,
/// and with nothing at all rather than starting one.
#[tokio::test]
async fn a_probe_finds_the_daemon_that_is_there_and_starts_none() {
    let root = temp_dir("launch-probe");
    let socket = root.0.join("svolder.sock");
    let at = socket.to_string_lossy().into_owned();
    // Autospawn is on and there is a daemon to start, so a chain would have started one.
    let config = Config::default();
    assert!(config.daemon.autospawn);

    assert_eq!(probe(&globals("auto", &at), &config).await, None);
    assert!(
        daemons_on(&socket).is_empty() && !socket.exists(),
        "a probe started something"
    );

    let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("bind the test socket");
    assert_eq!(
        probe(&globals("auto", &at), &config).await,
        Some(socket.clone())
    );
}

/// With no daemon to start, the verb still runs: `--transport auto` ends in a session of
/// its own, and `--transport daemon` says both what it wanted and what stopped it.
#[tokio::test]
async fn with_no_daemon_to_start_the_verb_still_runs_and_daemon_says_why_it_cannot() {
    let root = temp_dir("launch-no-binary");
    let socket = root.0.join("svolder.sock");
    let at = socket.to_string_lossy().into_owned();
    let host = TestHost::without_systemd(&root.0, None);

    // The chain says nothing itself: it hands the fact back, with the log to read, for the
    // front end that asked to say in its own way.
    let reached = reach_on(&host, &globals("auto", &at), &Config::default())
        .await
        .expect("a direct session");
    let Reached::Direct {
        autospawn_failed: Some(failed),
    } = reached
    else {
        panic!("a daemon that could not be started was not reported: {reached:?}");
    };
    assert!(failed.what.contains("svolderd"), "{}", failed.what);
    assert_eq!(failed.log, crate::state::log_path_in(&root.0));
    assert_eq!(
        failed.line(),
        format!(
            "{}; using a direct session — see {}",
            failed.what,
            failed.log.display()
        )
    );

    let failure = reach_on(&host, &globals("daemon", &at), &Config::default())
        .await
        .expect_err("--transport daemon insists on a daemon");
    assert!(failure.what.contains("no daemon"), "{}", failure.what);
    assert!(failure.what.contains("svolderd"), "{}", failure.what);
    assert_eq!(failure.next, crate::recovery::next_step(Kind::NoDaemon));
}

/// A daemon that starts and stops again is reported with the log to read, and what it said
/// on its way out is in that log, under whatever was there before.
#[tokio::test]
async fn a_daemon_that_stops_at_once_says_so_in_the_log_the_message_names() {
    let root = temp_dir("launch-daemon-stops");
    let socket = root.0.join("svolder.sock");
    let refuses = root.0.join("svolderd");
    install_script(
        &refuses,
        "#!/bin/sh\necho 'it will not start' >&2\nexit 3\n",
    );
    let host = TestHost::without_systemd(&root.0, Some(refuses));
    let log = crate::state::log_path_in(&root.0);
    std::fs::create_dir_all(log.parent().expect("a directory")).expect("the log's directory");
    std::fs::write(&log, "what an earlier daemon said\n").expect("the log is written");

    let failure = autospawn(&host, &socket)
        .await
        .expect_err("the daemon stopped without answering");
    assert_eq!(failure.code, Kind::AutospawnFailed.code());
    assert!(failure.what.contains("stopped"), "{}", failure.what);
    assert_eq!(
        failure.next,
        crate::recovery::next_step(Kind::AutospawnFailed)
    );

    let said = std::fs::read_to_string(&log).expect("the log is there");
    assert!(
        said.contains("what an earlier daemon said") && said.contains("it will not start"),
        "the log keeps what was there and adds what this one said: {said:?}"
    );

    // The log is what a reader is sent to, so the message that insists on a daemon names it.
    let at = socket.to_string_lossy().into_owned();
    let insisted = reach_on(&host, &globals("daemon", &at), &Config::default())
        .await
        .expect_err("--transport daemon insists on a daemon");
    assert!(
        insisted.what.contains(&log.display().to_string()),
        "the message names the log to read: {}",
        insisted.what
    );
}

/// A daemon that stops with the code for "another daemon holds the socket" is not a failure
/// while that other daemon is still starting: the socket answers a moment later.
#[tokio::test]
async fn a_daemon_that_lost_the_lock_finds_the_winner_answering() {
    let root = temp_dir("launch-daemon-loses");
    let socket = root.0.join("svolder.sock");
    let loses = root.0.join("svolderd");
    install_script(&loses, "#!/bin/sh\nexit 3\n");
    let host = TestHost::without_systemd(&root.0, Some(loses));

    let winner = {
        let socket = socket.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            std::os::unix::net::UnixListener::bind(&socket).expect("the winner binds")
        })
    };
    autospawn(&host, &socket)
        .await
        .expect("the winner answers after the loser has gone");
    drop(winner.await.expect("the winner task ran"));
}

/// A daemon that is still starting when its two seconds are up is ended rather than left
/// running: nothing else would ever wait for it, and it would hold the devices of the
/// daemon that comes next.
#[tokio::test]
async fn a_daemon_that_never_answers_is_not_left_behind() {
    let root = temp_dir("launch-daemon-hangs");
    let socket = root.0.join("svolder.sock");
    let sleeps = root.0.join("svolderd");
    // `exec` so the sleeping process is the one that was started, and the signal that ends
    // it ends the whole of it.
    install_script(&sleeps, "#!/bin/sh\nexec sleep 30\n");
    let host = TestHost::without_systemd(&root.0, Some(sleeps));
    let _daemons = Daemons(socket.clone());

    let failure = autospawn(&host, &socket)
        .await
        .expect_err("nothing answered on the socket");
    assert_eq!(failure.code, Kind::AutospawnFailed.code());
    assert!(failure.what.contains("did not answer"), "{}", failure.what);
    assert!(
        daemons_on(&socket).is_empty(),
        "the daemon that never answered was left running"
    );
}

/// What a daemon nobody started by hand is started with: the socket to serve, the transport
/// and adapter this front end was given, and no handover of this process's own.
#[tokio::test]
async fn an_autospawned_daemon_is_told_the_socket_and_keeps_no_handover() {
    let Some(binary) = prebuilt_daemon() else {
        return;
    };
    let root = temp_dir("launch-environment");
    let socket = root.0.join("svolder.sock");
    let _daemons = Daemons(socket.clone());
    let host = TestHost::without_systemd(&root.0, Some(binary));

    autospawn(&host, &socket)
        .await
        .expect("the daemon comes up");
    let mut client = Client::connect(&socket).await.expect("the daemon answers");
    let version = client
        .request(Request::Version)
        .await
        .expect("a version reply");
    assert!(
        matches!(&version, Response::Version { version, .. } if !version.is_empty()),
        "got {version:?}"
    );
    let environ = std::fs::read(format!(
        "/proc/{}/environ",
        daemons_on(&socket).first().expect("the daemon's pid")
    ))
    .expect("the daemon's environment");
    let set: Vec<&[u8]> = environ.split(|byte| *byte == 0).collect();
    for wanted in [
        format!("SVOLDER_SOCKET={}", socket.display()),
        "SVOLDER_TRANSPORT=fake".to_string(),
    ] {
        assert!(
            set.contains(&wanted.as_bytes()),
            "the daemon was not given {wanted}"
        );
    }
    for unwanted in ["LISTEN_FDS", "LISTEN_PID"] {
        assert!(
            !set.iter()
                .any(|value| value.starts_with(format!("{unwanted}=").as_bytes())),
            "the daemon inherited {unwanted}"
        );
    }
}
