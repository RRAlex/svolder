// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The verb over the fake device, a fake `systemctl` and daemons of the cases' own: the
//! orchestration's every branch, with a doctor that looks at nothing and a Ctrl-C the case
//! presses itself.

use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use svolder_ipc::checks::{self, State, SystemdFacts};

use super::*;
use crate::commands::daemon::test_support::{Tools, a_daemon_on, no_units, units_in};
use crate::test_support::temp_dir;

/// The host a stopped daemon would be started on: one with no daemon to start, which counts how
/// often it was asked for one.
struct CountingHost {
    asked: Cell<usize>,
    dir: PathBuf,
}

impl CountingHost {
    fn new(dir: &Path) -> CountingHost {
        CountingHost {
            asked: Cell::new(0),
            dir: dir.to_path_buf(),
        }
    }
}

impl Host for CountingHost {
    fn units(&self) -> SystemdFacts {
        checks::live::absent()
    }

    fn start_socket(&self) -> Result<(), String> {
        Err("no systemd".to_string())
    }

    fn daemon_binary(&self) -> Option<PathBuf> {
        self.asked.set(self.asked.get() + 1);
        None
    }

    fn log(&self) -> PathBuf {
        self.dir.join("svolderd.log")
    }

    fn daemon_env(&self) -> Vec<(OsString, OsString)> {
        Vec::new()
    }
}

/// A doctor that finds one thing, naming the fake's unit and an adapter address, and says
/// whether it was ever started.
async fn doctor(started: &Cell<bool>) -> Vec<Outcome> {
    started.set(true);
    vec![Outcome {
        check: "device",
        state: State::Pass,
        reason: "0102030405060708 is on the air through AA:BB:CC:DD:EE:01".to_string(),
        fix: None,
    }]
}

/// Everything one case holds.
struct Rig {
    dir: crate::test_support::TempDir,
    tools: Tools,
    host: CountingHost,
    asked: Cell<usize>,
    started: Cell<bool>,
    /// What the doctor was told about connecting to the socket.
    connect: Cell<Option<bool>>,
    /// The order in which the question was asked and Ctrl-C listened for.
    order: RefCell<Vec<&'static str>>,
}

impl Rig {
    fn new(what: &str) -> Rig {
        Rig::with_units(what, |_| no_units())
    }

    /// A rig whose `systemctl` shows the units `show` makes of the socket path.
    fn with_units(what: &str, show: impl FnOnce(&str) -> String) -> Rig {
        let dir = temp_dir(what);
        let host = CountingHost::new(&dir.0);
        let socket = dir.0.join("sv.sock").display().to_string();
        Rig {
            dir,
            tools: Tools::new(&show(&socket)),
            host,
            asked: Cell::new(0),
            started: Cell::new(false),
            connect: Cell::new(None),
            order: RefCell::new(Vec::new()),
        }
    }

    fn out(&self, name: &str) -> PathBuf {
        self.dir.0.join(name)
    }

    fn socket(&self) -> String {
        self.dir.0.join("sv.sock").display().to_string()
    }

    /// Run the verb as `g` says, answering `answer` to the question and pressing Ctrl-C when
    /// `press_after` says to: at once, before the run has done anything, for zero.
    async fn capture(
        &self,
        g: &Globals<'_>,
        options: Options,
        answer: bool,
        press_after: Option<Duration>,
    ) -> Report {
        self.capture_over(g, options, answer, press_after, None)
            .await
    }

    /// [`Rig::capture`] talking through `transport`, when there is one, instead of opening one.
    async fn capture_over(
        &self,
        g: &Globals<'_>,
        options: Options,
        answer: bool,
        press_after: Option<Duration>,
        transport: Option<Arc<dyn Transport>>,
    ) -> Report {
        let presses = Arc::new(Presses::new());
        if press_after == Some(Duration::ZERO) {
            presses.press();
        } else if let Some(after) = press_after {
            let presses = Arc::clone(&presses);
            tokio::spawn(async move {
                tokio::time::sleep(after).await;
                presses.press();
            });
        }
        let hands = Hands {
            ctl: &self.tools.ctl(),
            host: &self.host,
            ask: Box::new(|_| {
                self.asked.set(self.asked.get() + 1);
                self.order.borrow_mut().push("ask");
                Ok(answer)
            }),
            listen: Box::new(|| {
                self.order.borrow_mut().push("listen");
                presses
            }),
            transport,
        };
        capture(g, options, hands, |connect| {
            self.connect.set(Some(connect));
            doctor(&self.started)
        })
        .await
    }
}

/// The fake device as the environment's knobs make it.
fn fake_transport() -> Arc<dyn Transport> {
    let registry = crate::registry().expect("the profiles load");
    Arc::new(svolder_ble::FakeTransport::from_env(&registry).expect("the fake builds"))
}

fn fake<'a>(socket: &'a str, device: Option<&'a str>) -> Globals<'a> {
    Globals {
        transport: "fake",
        adapter: None,
        socket: Some(socket),
        device,
        verbose: false,
    }
}

fn options(out: &Path, seconds: u32, keep_ids: bool) -> Options {
    Options {
        seconds,
        keep_ids,
        out: Some(out.to_path_buf()),
    }
}

fn text_of(report: &Report) -> String {
    let file = report.file.as_ref().expect("a file was written");
    std::fs::read_to_string(file).expect("the file reads")
}

/// The hex of every frame sent, from the frames section.
fn sent_hex(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.starts_with('+') && line.contains("  sent  "))
        .filter_map(|line| line.rsplit("  ").next().map(str::to_string))
        .collect()
}

#[tokio::test(start_paused = true)]
async fn a_fake_capture_writes_every_section() {
    let rig = Rig::new("capture-sections");
    let socket = rig.socket();
    let out = rig.out("all.txt");

    let report = rig
        .capture(&fake(&socket, None), options(&out, 2, false), false, None)
        .await;

    assert_eq!(report.code, 0);
    assert_eq!(report.file.as_deref(), Some(out.as_path()));
    let text = text_of(&report);
    assert!(text.starts_with("svolder capture\n"), "{text}");
    assert!(
        text.contains(&format!("svolder {}\n", env!("CARGO_PKG_VERSION"))),
        "{text}"
    );
    for header in [
        "== doctor ==",
        "== adverts ==",
        "== inspect ==",
        "== frames ==",
    ] {
        assert!(text.contains(header), "{header} missing: {text}");
    }
    assert!(text.contains("  sent  "), "{text}");
    assert!(text.contains("  received  "), "{text}");
    assert!(
        text.contains("\n    command frame, passphrase field set (scrubbed)\n    Command "),
        "a scrubbed sent frame still reads as a command: {text}"
    );
    assert!(!text.contains("capture ended:"), "{text}");
    assert!(rig.started.get(), "the doctor ran");
    assert_eq!(rig.asked.get(), 0, "no daemon, no question");
}

#[tokio::test(start_paused = true)]
async fn a_fake_capture_leaves_no_unit_or_passphrase() {
    let rig = Rig::new("capture-scrub");
    let socket = rig.socket();

    let kept = rig
        .capture(
            &fake(&socket, None),
            options(&rig.out("kept.txt"), 2, true),
            false,
            None,
        )
        .await;
    let kept = text_of(&kept);
    let record = kept
        .lines()
        .find_map(|line| line.strip_prefix("company 0x"))
        .and_then(|line| line.split("  ").nth(1))
        .expect("an advert record")
        .to_string();
    let code = &record[28..36];
    assert_ne!(code, "00000000", "the fake advertises a code: {record}");

    let scrubbed = rig
        .capture(
            &fake(&socket, None),
            options(&rig.out("scrubbed.txt"), 2, false),
            false,
            None,
        )
        .await;
    let text = text_of(&scrubbed);
    let lower = text.to_lowercase();
    assert!(!lower.contains("0102030405060708"), "{text}");
    assert!(!lower.contains("aa:bb:cc:dd:ee:01"), "{text}");
    assert!(!lower.contains(code), "the encrypted code is gone: {text}");
    assert!(text.contains("0000000000000001"), "the placeholder: {text}");
    let sent = sent_hex(&text);
    assert!(!sent.is_empty(), "{text}");
    assert!(
        sent.iter().all(|hex| hex.starts_with("00000000")),
        "{sent:?}"
    );
    let scrubbed_record = text
        .lines()
        .find_map(|line| line.strip_prefix("company 0x"))
        .and_then(|line| line.split("  ").nth(1))
        .expect("an advert record");
    assert_eq!(&scrubbed_record[28..36], "00000000", "{scrubbed_record}");
}

#[tokio::test(start_paused = true)]
async fn keep_ids_keeps_them() {
    let rig = Rig::new("capture-keep");
    let socket = rig.socket();

    let report = rig
        .capture(
            &fake(&socket, None),
            options(&rig.out("kept.txt"), 2, true),
            false,
            None,
        )
        .await;

    let text = text_of(&report);
    assert!(text.contains("0102030405060708"), "{text}");
    assert!(text.contains("AA:BB:CC:DD:EE:01"), "{text}");
    assert!(text.contains("--keep-ids"), "{text}");
    assert!(
        sent_hex(&text)
            .iter()
            .any(|hex| !hex.starts_with("00000000")),
        "a sent frame keeps its passphrase field: {text}"
    );
}

#[tokio::test(start_paused = true)]
async fn an_existing_out_file_is_refused_before_anything_runs() {
    let rig = Rig::new("capture-refused");
    let socket = rig.socket();
    let out = rig.out("there.txt");
    std::fs::write(&out, "keep me").expect("the file is written");

    let report = rig
        .capture(&fake(&socket, None), options(&out, 2, false), true, None)
        .await;

    assert_eq!(report.code, 1);
    assert_eq!(report.file, None);
    assert_eq!(std::fs::read_to_string(&out).expect("it reads"), "keep me");
    assert!(!rig.started.get(), "the doctor was not started");
    assert_eq!(rig.asked.get(), 0);

    let missing = rig.out("no-such-dir").join("capture.txt");
    let report = rig
        .capture(
            &fake(&socket, None),
            options(&missing, 2, false),
            true,
            None,
        )
        .await;
    assert_eq!(report.code, 1);
    assert!(!rig.started.get(), "a missing directory fails at the start");
}

#[tokio::test(start_paused = true)]
async fn the_daemon_transport_is_refused() {
    let rig = Rig::new("capture-daemon");
    let socket = rig.socket();
    let out = rig.out("never.txt");
    let g = Globals {
        transport: "daemon",
        ..fake(&socket, None)
    };

    let report = rig.capture(&g, options(&out, 2, false), true, None).await;

    assert_eq!(report.code, 1);
    assert_eq!(report.file, None);
    assert!(!out.exists());
    assert!(!rig.started.get());
}

#[tokio::test(start_paused = true)]
async fn a_declined_question_captures_nothing() {
    let rig = Rig::new("capture-declined");
    let socket = rig.dir.0.join("sv.sock");
    let served = a_daemon_on(&socket).await;
    let socket_text = socket.display().to_string();
    let out = rig.out("never.txt");
    let g = Globals {
        transport: "auto",
        ..fake(&socket_text, None)
    };

    let report = rig.capture(&g, options(&out, 2, false), false, None).await;

    assert_eq!(report.code, 0);
    assert_eq!(report.file, None);
    assert!(!out.exists());
    assert_eq!(rig.asked.get(), 1, "the owner was asked once");
    assert!(!rig.started.get(), "nothing ran after the no");
    assert!(
        svolder_ipc::is_running(&socket).await,
        "the daemon was left alone"
    );
    assert!(
        rig.tools
            .calls()
            .iter()
            .all(|call| !call.contains(" stop "))
    );
    served.abort();
}

#[tokio::test(start_paused = true)]
async fn a_failure_part_way_writes_what_it_has() {
    let rig = Rig::new("capture-failure");
    let socket = rig.socket();
    let out = rig.out("partial.txt");

    let report = rig
        .capture(
            &fake(&socket, Some("1112131415161718")),
            options(&out, 2, false),
            false,
            None,
        )
        .await;

    assert_eq!(report.code, 2, "nothing in range");
    let text = text_of(&report);
    assert!(text.contains("== adverts =="), "{text}");
    assert!(!text.contains("== inspect =="), "{text}");
    assert!(
        text.trim_end()
            .lines()
            .last()
            .is_some_and(|line| line.starts_with("[capture ended: ")),
        "{text}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_press_in_the_window_ends_it_early_and_is_no_failure() {
    let rig = Rig::new("capture-press-window");
    let socket = rig.socket();
    let out = rig.out("early.txt");

    let report = rig
        .capture(
            &fake(&socket, None),
            options(&out, 3_000, false),
            false,
            Some(Duration::from_secs(600)),
        )
        .await;

    assert_eq!(report.code, 0);
    let text = text_of(&report);
    assert!(
        text.contains("frame window 3000 s, ended early after "),
        "{text}"
    );
    assert!(!text.contains("capture ended:"), "{text}");
    assert!(text.contains("== frames =="), "{text}");
}

#[tokio::test(start_paused = true)]
async fn a_press_before_the_window_writes_what_is_done_and_exits_130() {
    let rig = Rig::new("capture-press-early");
    let socket = rig.socket();
    let out = rig.out("pressed.txt");

    let report = rig
        .capture(
            &fake(&socket, None),
            options(&out, 60, false),
            false,
            Some(Duration::ZERO),
        )
        .await;

    assert_eq!(report.code, 130);
    let text = text_of(&report);
    assert!(!text.contains("== doctor =="), "{text}");
    assert!(!text.contains("== adverts =="), "{text}");
    assert!(text.contains("[capture ended: "), "{text}");
    assert!(!rig.started.get(), "the doctor was not started");
}

#[tokio::test(start_paused = true)]
async fn a_failure_after_the_stop_still_brings_the_daemon_back() {
    let rig = Rig::new("capture-restore");
    let socket = rig.dir.0.join("sv.sock");
    let served = a_daemon_on(&socket).await;
    let socket_text = socket.display().to_string();
    let out = rig.out("failed.txt");
    let g = Globals {
        transport: "ble",
        ..fake(&socket_text, Some("1112131415161718"))
    };

    let report = rig
        .capture_over(
            &g,
            options(&out, 2, false),
            true,
            None,
            Some(fake_transport()),
        )
        .await;

    assert_eq!(rig.asked.get(), 1);
    assert_eq!(rig.host.asked.get(), 1, "a daemon was started again");
    assert_eq!(report.code, 3, "the daemon that did not come back");
    assert!(text_of(&report).contains("[capture ended: "));
    let _ = served.await;
}

/// Where systemd's socket unit is listening with its service idle, the doctor is told not to
/// connect, and only the socket unit is put back.
#[tokio::test(start_paused = true)]
async fn an_idle_service_behind_a_listening_socket_is_never_connected_to() {
    let rig = Rig::with_units("capture-idle", |socket| {
        units_in(socket, "active", "inactive")
    });
    let socket = rig.dir.0.join("sv.sock");
    let listener = std::os::unix::net::UnixListener::bind(&socket).expect("the socket binds");
    listener.set_nonblocking(true).expect("non-blocking");
    let socket_text = socket.display().to_string();
    let g = Globals {
        transport: "ble",
        ..fake(&socket_text, None)
    };

    let report = rig
        .capture_over(
            &g,
            options(&rig.out("idle.txt"), 1, false),
            true,
            None,
            Some(fake_transport()),
        )
        .await;

    assert_eq!(report.code, 0);
    assert_eq!(rig.connect.get(), Some(false), "the doctor may not connect");
    assert!(listener.accept().is_err(), "nothing connected");
    let calls = rig.tools.calls();
    assert!(calls.iter().any(|c| c.contains(" stop ")), "{calls:?}");
    assert!(
        calls.iter().any(|c| c.ends_with(" start svolderd.socket")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .all(|c| !c.ends_with(" start svolderd.service")),
        "{calls:?}"
    );
}

/// With nobody holding the device the doctor may connect, as it always has.
#[tokio::test(start_paused = true)]
async fn with_no_holder_the_doctor_may_connect() {
    let rig = Rig::new("capture-connects");
    let socket = rig.socket();
    rig.capture(
        &fake(&socket, None),
        options(&rig.out("a.txt"), 1, false),
        false,
        None,
    )
    .await;
    assert_eq!(rig.connect.get(), Some(true));
}

/// A unit given with `-d` that no scan ever showed the scrub is hidden in the line that says why
/// the capture ended.
#[tokio::test(start_paused = true)]
async fn a_unit_named_with_d_is_scrubbed_from_the_ended_line() {
    let rig = Rig::new("capture-ended-unit");
    let socket = rig.socket();

    let report = rig
        .capture(
            &fake(&socket, Some("1111222233334444")),
            options(&rig.out("ended.txt"), 1, false),
            false,
            None,
        )
        .await;

    assert_eq!(report.code, 2);
    let text = text_of(&report);
    assert!(!text.contains("1111222233334444"), "{text}");
    let ended = text
        .lines()
        .find(|line| line.starts_with("[capture ended: "))
        .expect("an ended line");
    assert!(ended.contains("0000000000000001"), "{ended}");
}

/// The file is the owner's alone to read.
#[tokio::test(start_paused = true)]
async fn the_file_is_not_readable_by_others() {
    use std::os::unix::fs::PermissionsExt;
    let rig = Rig::new("capture-mode");
    let socket = rig.socket();
    let report = rig
        .capture(
            &fake(&socket, None),
            options(&rig.out("mode.txt"), 1, true),
            false,
            None,
        )
        .await;
    let file = report.file.expect("a file");
    let mode = std::fs::metadata(file)
        .expect("it exists")
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0, "{mode:o}");
}

/// Ctrl-C is listened for after the question, so one pressed at the prompt does what it does
/// anywhere else while nothing is stopped.
#[tokio::test(start_paused = true)]
async fn ctrl_c_is_listened_for_only_after_the_question() {
    let rig = Rig::new("capture-order");
    let socket = rig.dir.0.join("sv.sock");
    let served = a_daemon_on(&socket).await;
    let socket_text = socket.display().to_string();
    let g = Globals {
        transport: "auto",
        ..fake(&socket_text, None)
    };

    rig.capture(&g, options(&rig.out("never.txt"), 1, false), false, None)
        .await;
    assert_eq!(
        *rig.order.borrow(),
        ["ask"],
        "a declined question listens for nothing"
    );
    served.abort();

    let rig = Rig::new("capture-order-yes");
    let socket = rig.dir.0.join("sv.sock");
    let served = a_daemon_on(&socket).await;
    let socket_text = socket.display().to_string();
    let g = Globals {
        transport: "ble",
        ..fake(&socket_text, None)
    };
    rig.capture_over(
        &g,
        options(&rig.out("yes.txt"), 1, false),
        true,
        None,
        Some(fake_transport()),
    )
    .await;
    assert_eq!(*rig.order.borrow(), ["ask", "listen"]);
    let _ = served.await;
}

/// A press that is there when the readings start ends the capture as pressed, not as a window
/// that ended early.
#[tokio::test(start_paused = true)]
async fn a_press_before_the_readings_ends_the_capture_as_pressed() {
    let rig = Rig::new("capture-readings");
    let socket = rig.socket();
    let g = fake(&socket, None);
    let config = Config::default();
    let tap = FrameTap::new();
    let tapped: Arc<dyn Transport> = Arc::new(TappedTransport::new(fake_transport(), tap.clone()));
    let mut backend = Backend::direct_over(&g, &config, tapped, Wire::Fake, None)
        .await
        .expect("the fake connects");
    let presses = Presses::new();
    presses.press();
    let mut draft = Draft::new("2026-10-23 12:00:00", 60);

    let end = read_and_listen(&mut backend, 60, &presses, &tap, &mut draft).await;

    assert!(matches!(end, End::Pressed));
    assert!(draft.data.ended_early.is_none());
    assert!(draft.data.readings.is_none());
    backend.close().await;
}
