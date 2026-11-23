// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The stream on tokio's paused clock, against a scripted feed: no sleeps, no sockets.

use std::cell::RefCell;
use std::pin::pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use svolder_ipc::Signal;
use svolder_ipc::SignalSource;
use svolder_profiles::Registry;
use tokio::sync::mpsc;

use super::*;

const BASE_MS: u64 = 1_000_000_000;

/// A feed that tells what the test sends and answers the device list from what it holds.
struct Script {
    inputs: mpsc::UnboundedReceiver<Result<Input, Failure>>,
    devices: Arc<Mutex<Vec<DeviceInfo>>>,
    readings: Arc<Mutex<Option<Vec<ValueObject>>>>,
}

impl Feed for Script {
    async fn next(&mut self) -> Result<Input, Failure> {
        match self.inputs.recv().await {
            Some(input) => input,
            None => std::future::pending().await,
        }
    }

    fn devices(&self) -> BoxFuture<'static, Option<Vec<DeviceInfo>>> {
        let held = self.devices.lock().unwrap().clone();
        Box::pin(async move { Some(held) })
    }

    fn readings(&self) -> BoxFuture<'static, Option<Vec<ValueObject>>> {
        let held = self.readings.lock().unwrap().clone();
        Box::pin(async move { held })
    }
}

fn reading(id: &str, raw: u8) -> ValueObject {
    ValueObject {
        id: id.into(),
        key: None,
        label: String::new(),
        value_type: "u8".into(),
        raw: raw.into(),
        display: String::new(),
        value: None,
        unit: None,
        heard_at_ms: None,
    }
}

fn model(signal: Option<Signal>) -> Model {
    let profile = Registry::builtin()
        .unwrap()
        .get("bifrost3")
        .unwrap()
        .clone();
    Model::new(
        Some(PanelLink {
            name: "Bifrost 3".into(),
            connected: true,
            signal,
        }),
        Some(profile),
        vec![reading("10", 127)],
        Vec::new(),
    )
}

fn advert(dbm: i16, heard_at_ms: u64) -> Signal {
    Signal {
        dbm,
        source: SignalSource::Advert,
        heard_at_ms,
    }
}

fn host() -> Input {
    Input::Host(Box::new(HostView {
        available: true,
        ..HostView::default()
    }))
}

fn scenes() -> Input {
    Input::Scenes(Box::new(ScenesView {
        scenes: Vec::new(),
        on: None,
        recalled: None,
        known: true,
    }))
}

/// A volume reading of raw `raw` (127 is 0 dB).
fn volume(raw: u8) -> Input {
    Input::Status(reading("10", raw))
}

/// What a run printed, with the time each line came after the start.
type Lines = Rc<RefCell<Vec<(Duration, String)>>>;

/// What one run of the stream is given besides the script that drives it.
struct Rig {
    model: Model,
    devices: Vec<DeviceInfo>,
    readings: Option<Vec<ValueObject>>,
    /// Wall time the clock runs ahead of tokio's, as a suspend leaves it.
    skew: Rc<std::cell::Cell<u64>>,
    /// Whether the reader of the lines has gone away.
    gone: Rc<std::cell::Cell<bool>>,
}

impl Rig {
    fn new(signal: Option<Signal>) -> Rig {
        Rig {
            model: model(signal),
            devices: Vec::new(),
            readings: None,
            skew: Rc::default(),
            gone: Rc::default(),
        }
    }
}

/// Run the stream over a feed while `script` drives it, and say how the stream ended if it did.
async fn drive<S, Fut>(
    signal: Option<Signal>,
    devices: Vec<DeviceInfo>,
    script: S,
) -> (Lines, Option<Ended>)
where
    S: FnOnce(mpsc::UnboundedSender<Result<Input, Failure>>) -> Fut,
    Fut: Future<Output = ()>,
{
    let mut rig = Rig::new(signal);
    rig.devices = devices;
    drive_rig(rig, script).await
}

async fn drive_rig<S, Fut>(rig: Rig, script: S) -> (Lines, Option<Ended>)
where
    S: FnOnce(mpsc::UnboundedSender<Result<Input, Failure>>) -> Fut,
    Fut: Future<Output = ()>,
{
    let (tx, rx) = mpsc::unbounded_channel();
    let mut feed = Script {
        inputs: rx,
        devices: Arc::new(Mutex::new(rig.devices)),
        readings: Arc::new(Mutex::new(rig.readings)),
    };
    let mut model = rig.model;
    let clock = Clock::scripted(BASE_MS, rig.skew);
    let start = Instant::now();
    let lines: Lines = Rc::default();
    let seen = Rc::clone(&lines);
    let gone = Rc::clone(&rig.gone);
    let mut out = move |line: Option<&str>| {
        let Some(line) = line else {
            return !gone.get();
        };
        seen.borrow_mut().push((start.elapsed(), line.to_string()));
        true
    };
    let mut run = pin!(stream(&mut feed, &mut model, &clock, &mut out));
    let ended = tokio::select! {
        ended = &mut run => Some(ended),
        () = script(tx) => None,
    };
    (lines, ended)
}

fn volumes(lines: &Lines) -> Vec<String> {
    lines
        .borrow()
        .iter()
        .map(|(_, line)| {
            let value: serde_json::Value = serde_json::from_str(line).unwrap();
            value["device"]["volume"]["text"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

fn info(signal: Option<Signal>) -> DeviceInfo {
    DeviceInfo {
        name: "Bifrost 3".into(),
        unit: "0102030405060708".into(),
        product_id: 10,
        product: None,
        firmware: 1,
        profile: "bifrost3".into(),
        connected: true,
        stale: false,
        retry_in_secs: None,
        passphrase_note: None,
        refused: false,
        recalled: None,
        signal,
        key_readings: Vec::new(),
    }
}

/// The snapshot is the first line, as soon as the host view and the scenes are in: before the
/// grace runs out.
#[tokio::test(start_paused = true)]
async fn the_snapshot_is_the_first_line_as_soon_as_it_is_whole() {
    let (lines, _) = drive(None, Vec::new(), |tx| async move {
        tx.send(Ok(host())).unwrap();
        tx.send(Ok(scenes())).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    })
    .await;
    let lines = lines.borrow();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].0, Duration::ZERO);
    assert!(
        lines[0].1.contains("\"name\":\"Bifrost 3\""),
        "{}",
        lines[0].1
    );
}

/// A daemon that sends no host view or scenes still gets its first line, once the grace is out.
#[tokio::test(start_paused = true)]
async fn the_first_line_waits_for_the_grace_when_nothing_else_comes() {
    let (lines, _) = drive(None, Vec::new(), |_tx| async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
    })
    .await;
    let lines = lines.borrow();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].0, SNAPSHOT_GRACE);
}

/// Changes inside a quarter of a second are one line, with the last of them: at most four lines
/// a second.
#[tokio::test(start_paused = true)]
async fn changes_are_coalesced_to_four_lines_a_second() {
    let (lines, _) = drive(None, Vec::new(), |tx| async move {
        tx.send(Ok(host())).unwrap();
        tx.send(Ok(scenes())).unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        for raw in [120, 110, 100, 90, 80] {
            tx.send(Ok(volume(raw))).unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    })
    .await;
    let times: Vec<Duration> = lines.borrow().iter().map(|(at, _)| *at).collect();
    assert_eq!(times, [Duration::ZERO, MIN_GAP]);
    assert_eq!(volumes(&lines).last().unwrap(), "\u{2212}17.6");
}

/// A change that leaves the line as it was prints nothing.
#[tokio::test(start_paused = true)]
async fn the_same_line_is_never_printed_twice_in_a_row() {
    let (lines, _) = drive(None, Vec::new(), |tx| async move {
        tx.send(Ok(host())).unwrap();
        tx.send(Ok(scenes())).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        tx.send(Ok(volume(127))).unwrap();
        tx.send(Ok(host())).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    })
    .await;
    assert_eq!(lines.borrow().len(), 1);
}

/// A signal reading turns old two minutes after the daemon heard it, and a line says so then.
#[tokio::test(start_paused = true)]
async fn a_line_is_printed_when_the_signal_turns_old() {
    let heard = BASE_MS - 100_000;
    let (lines, _) = drive(Some(advert(-58, heard)), Vec::new(), |tx| async move {
        tx.send(Ok(host())).unwrap();
        tx.send(Ok(scenes())).unwrap();
        tokio::time::sleep(Duration::from_secs(30)).await;
    })
    .await;
    let lines = lines.borrow();
    assert_eq!(lines.len(), 2, "{lines:?}");
    // Heard 100 s before the start: old after 20 s and a millisecond more.
    assert_eq!(lines[1].0, Duration::from_millis(20_001));
    assert!(lines[0].1.contains("\"old\":false"));
    assert!(lines[1].1.contains("\"old\":true"));
}

/// The daemon's device list is read again every 15 seconds, and a new signal shows in a line.
#[tokio::test(start_paused = true)]
async fn the_signal_is_read_again_every_fifteen_seconds() {
    let devices = vec![info(Some(advert(-85, BASE_MS)))];
    let (lines, _) = drive(Some(advert(-58, BASE_MS)), devices, |tx| async move {
        tx.send(Ok(host())).unwrap();
        tx.send(Ok(scenes())).unwrap();
        tokio::time::sleep(Duration::from_secs(14)).await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    })
    .await;
    let lines = lines.borrow();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[1].0, DEVICES_EVERY);
    assert!(lines[1].1.contains("\"dbm\":-85"));
}

/// A feed that fails ends the stream with its failure; a closed reader ends it quietly.
#[tokio::test(start_paused = true)]
async fn the_daemon_stopping_ends_the_stream_with_exit_3() {
    let (_, ended) = drive(None, Vec::new(), |tx| async move {
        tx.send(Err(Failure::of(Kind::DaemonStopped, DAEMON_STOPPED)))
            .unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    })
    .await;
    let Some(Ended::Failed(failure)) = ended else {
        panic!("the stream should end with a failure: {ended:?}")
    };
    assert_eq!(failure.code, 3);
    assert_eq!(failure.what, DAEMON_STOPPED);
}

/// The link dropping and coming back: the line says the DAC is away, then present again with the
/// readings read afresh.
#[tokio::test(start_paused = true)]
async fn a_reconnect_reads_the_readings_again() {
    let mut rig = Rig::new(None);
    rig.readings = Some(vec![reading("10", 103)]);
    let (lines, _) = drive_rig(rig, |tx| async move {
        tx.send(Ok(host())).unwrap();
        tx.send(Ok(scenes())).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        tx.send(Ok(Input::Connection(false))).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        tx.send(Ok(Input::Connection(true))).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    })
    .await;
    let lines = lines.borrow();
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines[0].1.contains("\"present\":true"));
    assert!(lines[1].1.contains("\"present\":false"));
    assert!(lines[2].1.contains("\"present\":true"));
    assert!(
        lines[2].1.contains("\"text\":\"\u{2212}8.0\""),
        "{}",
        lines[2].1
    );
    assert!(lines[0].1.contains("\"text\":\"0.0\""));
}

/// A DAC away at the start streams as away and turns present when its link comes up.
#[tokio::test(start_paused = true)]
async fn a_dac_away_at_the_start_streams_and_then_turns_present() {
    let mut rig = Rig::new(None);
    rig.model.link.as_mut().unwrap().connected = false;
    rig.readings = Some(vec![reading("10", 127)]);
    let (lines, _) = drive_rig(rig, |tx| async move {
        tx.send(Ok(host())).unwrap();
        tx.send(Ok(scenes())).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        tx.send(Ok(Input::Connection(true))).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    })
    .await;
    let lines = lines.borrow();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].1.contains("\"present\":false"));
    assert!(lines[1].1.contains("\"present\":true"));
}

/// With no device at all the stream is `device: null` and still goes on with the scenes.
#[tokio::test(start_paused = true)]
async fn no_device_streams_a_null_device() {
    let mut rig = Rig::new(None);
    rig.model.link = None;
    rig.model.profile = None;
    let (lines, _) = drive_rig(rig, |tx| async move {
        tx.send(Ok(host())).unwrap();
        tx.send(Ok(scenes())).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    })
    .await;
    let lines = lines.borrow();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].1.contains("\"device\":null"), "{}", lines[0].1);
}

/// The ages are read from the wall clock: a suspend that only the wall clock sees (the monotonic
/// one stands still) still turns a reading old at the next look, not hours late.
#[tokio::test(start_paused = true)]
async fn a_suspend_cannot_keep_a_stale_signal_bright() {
    let mut rig = Rig::new(Some(advert(-58, BASE_MS)));
    let skew = Rc::clone(&rig.skew);
    rig.devices = Vec::new();
    let (lines, _) = drive_rig(rig, |tx| async move {
        tx.send(Ok(host())).unwrap();
        tx.send(Ok(scenes())).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        skew.set(3_600_000);
        tokio::time::sleep(DEVICES_EVERY).await;
    })
    .await;
    let lines = lines.borrow();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[1].1.contains("\"old\":true"));
    assert!(lines[1].0 <= DEVICES_EVERY, "{:?}", lines[1].0);
}

fn code_is(code: std::process::ExitCode, expected: u8) {
    assert_eq!(
        format!("{code:?}"),
        format!("{:?}", std::process::ExitCode::from(expected))
    );
}

/// The globals of a call that names `socket` and asks for `transport`.
fn globals<'a>(socket: &'a str, transport: &'a str) -> Globals<'a> {
    Globals {
        transport,
        adapter: None,
        socket: Some(socket),
        device: None,
        verbose: false,
    }
}

/// With a daemon stopped under it the real stream ends with exit 3 and the daemon's words, and
/// whatever `--transport` says it is the daemon it talks to.
#[tokio::test]
async fn the_daemon_stopping_ends_the_real_stream_with_exit_3() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    let socket = daemon.socket.display().to_string();
    let g = globals(&socket, "ble");
    daemon.empty();
    let mut run = pin!(super::run(&g, true));
    tokio::select! {
        _ = &mut run => panic!("the stream ended before the daemon stopped"),
        () = daemon.wait_subscribed() => {}
    }
    daemon.stop().await;
    code_is(run.await, 3);
}

/// A device the daemon holds but has not found streams as away instead of failing: the stream
/// is on that device and ends only with the daemon.
#[tokio::test]
async fn a_device_the_daemon_has_not_found_is_streamed_as_away() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    daemon.unfound(true);
    let socket = daemon.socket.display().to_string();
    let g = Globals {
        device: Some("fake"),
        ..globals(&socket, "auto")
    };
    let mut run = pin!(super::run(&g, true));
    tokio::select! {
        _ = &mut run => panic!("the stream ended for an absent device"),
        () = daemon.wait_subscribed() => {}
    }
    assert_eq!(daemon.subscribed(), [Some("fake".to_string())]);
    daemon.stop().await;
    code_is(run.await, 3);
}

/// A daemon with no device at all streams too, on no device.
#[tokio::test]
async fn a_daemon_with_no_device_is_streamed_as_no_device() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    daemon.empty();
    let socket = daemon.socket.display().to_string();
    let g = globals(&socket, "auto");
    let mut run = pin!(super::run(&g, true));
    tokio::select! {
        _ = &mut run => panic!("the stream ended for a daemon with no device"),
        () = daemon.wait_subscribed() => {}
    }
    assert_eq!(daemon.subscribed(), [None]);
    daemon.stop().await;
    code_is(run.await, 3);
}

/// Several devices and none chosen is exit 2, as every verb's; `-d` chooses.
#[tokio::test]
async fn several_devices_and_none_chosen_is_exit_2() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    let socket = daemon.socket.display().to_string();
    code_is(super::run(&globals(&socket, "auto"), true).await, 2);
}

/// With no daemon on the socket the verb exits 3 and starts nothing, whatever `--transport` or
/// `--no-start` say; without `--json` it is a usage error.
#[tokio::test]
async fn no_daemon_is_exit_3_and_starts_nothing() {
    use crate::test_support::stub_daemon::{SocketFile, fresh_socket};

    let none = SocketFile(fresh_socket());
    let nowhere = none.0.display().to_string();
    for transport in ["auto", "daemon", "ble", crate::backend::RUNNING] {
        code_is(super::run(&globals(&nowhere, transport), true).await, 3);
    }
    assert!(!none.0.exists(), "no daemon was started");
    code_is(super::run(&globals(&nowhere, "auto"), false).await, 1);
}

/// A reader that goes away is noticed at the next look, with no change to write.
#[tokio::test(start_paused = true)]
async fn a_reader_that_went_away_ends_the_stream_without_a_change() {
    let rig = Rig::new(None);
    let gone = Rc::clone(&rig.gone);
    let (lines, ended) = drive_rig(rig, |tx| async move {
        tx.send(Ok(host())).unwrap();
        tx.send(Ok(scenes())).unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
        gone.set(true);
        tokio::time::sleep(DEVICES_EVERY * 2).await;
    })
    .await;
    assert!(matches!(ended, Some(Ended::ReaderGone)), "{ended:?}");
    assert_eq!(lines.borrow().len(), 1);
}

/// A stream that began with no device ends, so that it can begin again, when a device is added.
#[tokio::test(start_paused = true)]
async fn a_device_added_to_a_stream_with_none_ends_it_to_begin_again() {
    let mut rig = Rig::new(None);
    rig.model.link = None;
    rig.model.profile = None;
    rig.devices = vec![info(None)];
    let (lines, ended) = drive_rig(rig, |tx| async move {
        tx.send(Ok(host())).unwrap();
        tx.send(Ok(scenes())).unwrap();
        tokio::time::sleep(DEVICES_EVERY * 2).await;
    })
    .await;
    assert!(matches!(ended, Some(Ended::Restart)), "{ended:?}");
    assert_eq!(lines.borrow().len(), 1);
}

/// `-d` wins; else the config's `default_device`; else the daemon's own single device.
#[test]
fn the_device_is_the_flags_else_the_configs_default() {
    let mut config = svolder_ipc::Config {
        default_device: Some("Den".into()),
        ..svolder_ipc::Config::default()
    };
    let g = |device| Globals {
        device,
        ..globals("/x", "auto")
    };
    assert_eq!(super::selector(&g(Some("Desk")), &config), Some("Desk"));
    assert_eq!(super::selector(&g(None), &config), Some("Den"));
    config.default_device = None;
    assert_eq!(super::selector(&g(None), &config), None);
}

/// The selector reaches the daemon's list: two devices and one chosen is that one; none chosen
/// is the refusal that lists them; a name that matches none says so and where the list is.
#[tokio::test]
async fn the_chosen_device_of_two_is_the_one_opened() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    let (backend, info) = Backend::open_watching(&daemon.socket, Some("other"))
        .await
        .expect("opened");
    assert_eq!(info.expect("a device").name, "other");
    backend.close().await;
    let refused = Backend::open_watching(&daemon.socket, None).await.err();
    assert_eq!(refused.expect("refused").code, 2);
    let typo = Backend::open_watching(&daemon.socket, Some("Desk"))
        .await
        .err()
        .expect("refused");
    assert_eq!(typo.code, 2);
    assert!(
        typo.what.contains("no device named \"Desk\""),
        "{}",
        typo.what
    );
    assert!(typo.next.contains("svolder devices"), "{}", typo.next);
}

/// Through the real feed, a device that was away at the start turns present, with its readings,
/// when the daemon finds it.
#[tokio::test]
async fn an_unconnected_open_turns_present_when_the_daemon_finds_the_device() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    daemon.unfound(true);
    daemon.hosts(HostView {
        available: true,
        ..HostView::default()
    });
    let socket = daemon.socket.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let mut out =
        move |line: Option<&str>| line.is_none_or(|line| tx.send(line.to_string()).is_ok());
    let mut run = pin!(super::watch(
        &socket,
        Some("fake"),
        Vec::new(),
        &Clock::Wall,
        &mut out
    ));
    let present = |line: &str| line.contains("\"present\":true");
    let ended = tokio::select! {
        ended = &mut run => Some(ended),
        () = async {
            daemon.wait_subscribed().await;
            daemon.publish(svolder_ipc::Event::Scenes(ScenesView {
                scenes: Vec::new(),
                on: None,
                recalled: None,
                known: true,
            }));
            let first = rx.recv().await.expect("a first line");
            assert!(first.contains("\"present\":false"), "{first}");
            daemon.finds_it();
            loop {
                let line = rx.recv().await.expect("a line");
                if present(&line) {
                    assert!(line.contains("\"volume\":{\"db\""), "{line}");
                    break;
                }
            }
        } => None,
    };
    assert!(ended.is_none(), "the stream ended: {ended:?}");
}

/// Whether the reader of a pipe is there is read off the pipe: open, yes; its read end closed, no.
#[test]
fn the_reader_of_a_pipe_is_seen_to_go() {
    let mut ends = [0 as libc::c_int; 2];
    // SAFETY: `ends` holds the two descriptors `pipe` fills in.
    assert_eq!(unsafe { libc::pipe(ends.as_mut_ptr()) }, 0);
    let [read, write] = ends;
    assert!(super::reader_is_there(write));
    // SAFETY: both descriptors are this test's own, each closed once.
    unsafe { libc::close(read) };
    assert!(!super::reader_is_there(write));
    unsafe { libc::close(write) };
}
