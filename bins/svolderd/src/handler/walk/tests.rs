// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The walk through the daemon, on the fake transport: a device on the air that no block
//! configures is walked to, beside a configured one.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::BoxStream;
use svolder_ble::fake::Advertised;
use svolder_ble::{Advertisements, BleError, FakeFleet, FakeTransport, Link, Listening, Transport};
use svolder_ipc::{
    CODE_NOT_FOUND, CODE_USAGE, Config, ConfiguredDevice, Connect, Event, Handler, JOB_STOP_BOUND,
    JobId, JobState, PassphraseKind, Reason, Request, Response, WALK_IDLE_BOUND, WalkOp,
};
use svolder_profiles::Registry;
use svolder_proto::{Passphrase, UnitId};
use svolder_walk::{Phase, RefusalKind, Step, Takes, WalkState, Working};
use tokio::time::Instant;

use super::super::{Daemon, SHUTDOWN_DEADLINE};
use crate::locks::read;
use crate::writer::Origin;

/// The device on the air that no block configures; it advertises a code it does not hold, so
/// `auto` is refused and the walk asks for the passphrase.
const NEW: &str = "0102030405060708";
/// The configured device.
const AMP: &str = "1112131415161718";

type Events = BoxStream<'static, Event>;

fn registry() -> Arc<Registry> {
    Arc::new(Registry::builtin().expect("the built-in profiles load"))
}

/// The unconfigured device: a gungnir 2 with the code 1234 that advertises 9999.
fn new_device() -> FakeTransport {
    let registry = registry();
    let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
    let device = FakeTransport::from_profile(
        &profile,
        NEW.parse().expect("a unit id"),
        3,
        Passphrase::from_text("1234").expect("four digits"),
    );
    device.advertise(Advertised::Always(
        Passphrase::from_text("9999").expect("four digits"),
    ));
    device
}

/// A scratch folder, removed when it is dropped.
struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A scratch folder holding a configuration file with `text`.
fn folder(tag: &str, text: &str) -> (Folder, Config) {
    let dir = std::env::temp_dir().join(format!("svolderd-walk-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).expect("the folder is made");
    let path = dir.join("config.toml");
    std::fs::write(&path, text).expect("the file is written");
    let config = Config::load_from(&path).expect("the file parses");
    (Folder(dir), config)
}

/// A shared daemon over `transport`, reading the configuration in `dir`.
fn daemon_in(dir: &std::path::Path, config: Config, transport: Arc<dyn Transport>) -> Arc<Daemon> {
    Daemon::new(
        transport,
        registry(),
        dir.join("config.toml"),
        dir.join("profiles"),
        PathBuf::from("/nonexistent/svolder.sock"),
        config,
    )
    .shared()
}

/// The configured amp's block.
const AMP_BLOCK: &str =
    "[[device]]\nname = \"amp\"\nudid = \"1112131415161718\"\npassphrase = \"1234\"\n";

/// The fleet of the unconfigured device and the amp.
fn fleet() -> Arc<dyn Transport> {
    Arc::new(
        FakeFleet::from_more(
            &registry(),
            new_device(),
            &format!("gungnir2-v2@{AMP}"),
            false,
        )
        .expect("the fleet is made"),
    )
}

async fn walk(daemon: &Daemon, origin: Origin, op: WalkOp) -> Response {
    daemon.handle_from(origin, Request::Walk { op }).await
}

async fn follow(daemon: &Daemon) -> Events {
    daemon
        .stream(Request::Walk { op: WalkOp::Follow })
        .await
        .map_err(|e| format!("{e:?}"))
        .expect("the walk stream")
}

async fn started(daemon: &Daemon) -> JobId {
    match walk(daemon, Origin::Web, WalkOp::Start).await {
        Response::Job { job } => job,
        other => panic!("the walk did not start: {other:?}"),
    }
}

/// The next state of `job` on `events`.
async fn next(events: &mut Events, job: JobId) -> JobState {
    next_within(events, job, Duration::from_secs(60)).await
}

/// The next state of `job` on `events`, within `bound`.
async fn next_within(events: &mut Events, job: JobId, bound: Duration) -> JobState {
    loop {
        let event = tokio::time::timeout(bound, events.next())
            .await
            .expect("an event in time")
            .expect("the stream is open");
        if let Event::Walk(event) = event
            && event.job == job
        {
            return event.state;
        }
    }
}

/// The walk's next state that waits for an answer; panics on an end.
async fn waiting(events: &mut Events, job: JobId) -> WalkState {
    loop {
        match next(events, job).await {
            JobState::Progress { payload } => {
                let state: WalkState = serde_json::from_value(payload).expect("a walk state");
                if state.phase == Phase::Waiting {
                    return state;
                }
            }
            JobState::Started => {}
            other => panic!("the walk ended: {other:?}"),
        }
    }
}

/// The walk's last state: how its job ended.
async fn ended(events: &mut Events, job: JobId) -> JobState {
    loop {
        let state = next(events, job).await;
        if state.is_final() {
            return state;
        }
    }
}

async fn answer(daemon: &Daemon, job: JobId, choice: usize, text: Option<&str>) {
    let op = WalkOp::Answer {
        job,
        choice,
        text: text.map(str::to_string),
    };
    assert_eq!(walk(daemon, Origin::Web, op).await, Response::Ack);
}

/// Wait for `device` to be connected, by its events.
async fn connected(daemon: &Daemon, device: &str) {
    let mut events = daemon.device_events(Some(device.to_string()));
    tokio::time::timeout(Duration::from_secs(60), async {
        while let Some(event) = events.next().await {
            if let Event::Connection {
                connected: true, ..
            } = event
            {
                return;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{device} never connected"));
}

/// The whole walk: the warning names the linked amp, the scan lists both devices with the amp
/// marked, picking the amp is refused naming it, the new device is named, `auto` and a wrong
/// code are refused (never quoted), the right one is written, the walk ends with the device as
/// `device_config` answers it, and the reload connects it.
#[tokio::test]
async fn a_walk_finds_names_tests_and_writes_a_device_and_the_reload_connects_it() {
    let (dir, config) = folder("whole", AMP_BLOCK);
    let daemon = daemon_in(&dir.0, config.clone(), fleet());
    assert_eq!(daemon.reconcile(&config).await, ["amp"]);
    connected(&daemon, "amp").await;
    // A hand edit stands before the walk; the walk's own write takes the stamp, so the change
    // is gone once the device is written.
    std::fs::File::options()
        .write(true)
        .open(dir.0.join("config.toml"))
        .expect("the file opens")
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        .expect("the time is set");
    assert!(daemon.config_changed().await, "the hand edit shows");
    let mut events = follow(&daemon).await;
    let job = started(&daemon).await;

    let warned = waiting(&mut events, job).await;
    assert_eq!(warned.origin, "web");
    assert_eq!(warned.prompt.step, Step::Linked);
    assert!(
        warned.prompt.body[0].starts_with("amp has a link"),
        "{warned:?}"
    );
    answer(&daemon, job, 0, None).await;

    let scan = waiting(&mut events, job).await;
    assert_eq!(
        (scan.prompt.step, scan.prompt.takes),
        (Step::Scan, Takes::Choice)
    );
    answer(&daemon, job, 0, None).await;
    let pick = waiting(&mut events, job).await;
    assert_eq!(pick.prompt.step, Step::Pick);
    let units: Vec<&str> = pick
        .rows
        .iter()
        .map(|row| row.entry.unit.as_str())
        .collect();
    assert_eq!(units.len(), 2, "{pick:?}");
    let amp_at = units
        .iter()
        .position(|unit| *unit == AMP)
        .expect("the amp is heard");
    let new_at = units
        .iter()
        .position(|unit| *unit == NEW)
        .expect("the new one is heard");
    assert_eq!(pick.rows[amp_at].configured.as_deref(), Some("amp"));
    assert_eq!(pick.rows[new_at].configured, None);
    assert!(pick.prompt.choices[amp_at].ends_with("configured as amp"));

    answer(&daemon, job, amp_at, None).await;
    let again = waiting(&mut events, job).await;
    let refusal = again.refusal.expect("picking the amp is refused");
    assert_eq!(refusal.kind, RefusalKind::Configured);
    assert!(refusal.message.contains("amp"), "{}", refusal.message);
    assert_eq!(again.prompt.step, Step::Pick);

    answer(&daemon, job, new_at, None).await;
    let name = waiting(&mut events, job).await;
    assert_eq!(
        (name.prompt.step, name.prompt.takes),
        (Step::Name, Takes::Text)
    );
    answer(&daemon, job, 0, Some("desk")).await;
    let code = waiting(&mut events, job).await;
    assert_eq!(
        (code.prompt.step, code.prompt.takes),
        (Step::Passphrase, Takes::Secret)
    );
    assert_eq!(code.connect, Some(Connect::Refused), "auto was refused");

    answer(&daemon, job, 0, Some("4321")).await;
    let wrong = waiting(&mut events, job).await;
    assert_eq!(wrong.prompt.step, Step::Passphrase);
    let refusal = wrong.refusal.clone().expect("a wrong code is refused");
    assert_eq!(refusal.kind, RefusalKind::Passphrase);
    let said = serde_json::to_string(&wrong).expect("a state is JSON");
    assert!(!said.contains("4321"), "{said}");

    answer(&daemon, job, 0, Some("1234")).await;
    let JobState::Done { payload } = ended(&mut events, job).await else {
        panic!("the walk did not end done");
    };
    let device: ConfiguredDevice = serde_json::from_value(payload).expect("the device written");
    assert_eq!(
        (
            device.name.as_str(),
            device.unit.as_str(),
            device.passphrase
        ),
        ("desk", NEW, PassphraseKind::Set)
    );
    let written = std::fs::read_to_string(dir.0.join("config.toml")).expect("the file reads");
    assert!(written.contains("name = \"desk\""), "{written}");
    assert!(
        !daemon.config_changed().await,
        "the walk's write is not a change"
    );
    connected(&daemon, "desk").await;
    assert!(read(&daemon.devices).probe.is_none());
    daemon.shutdown().await;
}

/// One walk at a time: a second start is refused busy, naming whose walk runs; its states say
/// the same, and a follower that comes late reads the walk's whole state, rows included, from
/// the replay.
#[tokio::test]
async fn a_second_start_is_busy_and_a_late_follower_reads_the_whole_state() {
    let (dir, config) = folder("busy", "");
    let daemon = daemon_in(&dir.0, config, Arc::new(new_device()));
    let mut events = follow(&daemon).await;
    let job = started(&daemon).await;
    waiting(&mut events, job).await;
    answer(&daemon, job, 0, None).await;
    let pick = waiting(&mut events, job).await;
    assert_eq!(pick.rows.len(), 1);

    match walk(&daemon, Origin::Socket, WalkOp::Start).await {
        Response::Error {
            code: CODE_USAGE,
            message,
            reason: Some(Reason::Busy),
        } => assert!(
            message.contains("started by the web page") && !message.contains("by web "),
            "{message}"
        ),
        other => panic!("a second walk started: {other:?}"),
    }

    let mut late = follow(&daemon).await;
    let replayed = waiting(&mut late, job).await;
    assert_eq!(replayed, pick, "the replay is the walk's last state");
    assert_eq!(replayed.origin, "web");

    assert_eq!(
        walk(&daemon, Origin::Web, WalkOp::Cancel { job }).await,
        Response::Ack
    );
    assert_eq!(ended(&mut events, job).await, JobState::Cancelled);
    daemon.shutdown().await;
}

/// A cancelled walk ends cancelled; an answer to it then, or to a walk that never was, is
/// refused with a reason of its own.
#[tokio::test]
async fn answers_to_a_walk_that_does_not_wait_are_refused() {
    let (dir, config) = folder("refused", "");
    let daemon = daemon_in(&dir.0, config, Arc::new(new_device()));
    let mut events = follow(&daemon).await;
    let job = started(&daemon).await;
    waiting(&mut events, job).await;
    assert_eq!(
        walk(&daemon, Origin::Web, WalkOp::Cancel { job }).await,
        Response::Ack
    );
    assert_eq!(ended(&mut events, job).await, JobState::Cancelled);
    let late = WalkOp::Answer {
        job,
        choice: 0,
        text: None,
    };
    match walk(&daemon, Origin::Web, late).await {
        Response::Error {
            reason: Some(Reason::NotWaiting),
            ..
        } => {}
        other => panic!("an answer to an ended walk: {other:?}"),
    }
    match walk(&daemon, Origin::Web, WalkOp::Back { job: 999 }).await {
        Response::Error {
            code: CODE_NOT_FOUND,
            reason: Some(Reason::NoSuchJob),
            ..
        } => {}
        other => panic!("a back to no walk: {other:?}"),
    }
    daemon.shutdown().await;
}

/// Back returns to the step before; with nothing behind, the state says so.
#[tokio::test]
async fn back_returns_to_the_step_before() {
    let (dir, config) = folder("back", "");
    let daemon = daemon_in(&dir.0, config, Arc::new(new_device()));
    let mut events = follow(&daemon).await;
    let job = started(&daemon).await;
    let scan = waiting(&mut events, job).await;
    assert!(!scan.can_go_back);
    assert_eq!(
        walk(&daemon, Origin::Web, WalkOp::Back { job }).await,
        Response::Ack
    );
    let stayed = waiting(&mut events, job).await;
    assert_eq!(stayed.prompt.step, Step::Scan);
    assert_eq!(stayed.refusal.map(|r| r.kind), Some(RefusalKind::Answer));
    answer(&daemon, job, 0, None).await;
    let pick = waiting(&mut events, job).await;
    assert!(pick.can_go_back);
    assert_eq!(
        walk(&daemon, Origin::Web, WalkOp::Back { job }).await,
        Response::Ack
    );
    assert_eq!(waiting(&mut events, job).await.prompt.step, Step::Scan);
    walk(&daemon, Origin::Web, WalkOp::Cancel { job }).await;
    daemon.shutdown().await;
}

/// A walk nobody answers ends cancelled after the idle bound, and frees the one slot.
#[tokio::test(start_paused = true)]
async fn a_walk_nobody_answers_ends_after_the_idle_bound() {
    let (dir, config) = folder("idle", "");
    let daemon = daemon_in(&dir.0, config, Arc::new(new_device()));
    let mut events = follow(&daemon).await;
    let job = started(&daemon).await;
    waiting(&mut events, job).await;
    let since = Instant::now();
    let bound = WALK_IDLE_BOUND + Duration::from_secs(60);
    assert_eq!(
        next_within(&mut events, job, bound).await,
        JobState::Cancelled
    );
    assert!(since.elapsed() >= WALK_IDLE_BOUND, "{:?}", since.elapsed());
    assert!(since.elapsed() < WALK_IDLE_BOUND + Duration::from_secs(1));
    assert!(matches!(
        walk(&daemon, Origin::Web, WalkOp::Start).await,
        Response::Job { .. }
    ));
    daemon.shutdown().await;
}

/// A transport over the new device whose connects never finish: a passphrase test on it is
/// still running when the daemon stops.
struct Stuck(FakeTransport);

#[async_trait::async_trait]
impl Transport for Stuck {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        self.0.scan(window).await
    }

    async fn held(&self) -> Result<Advertisements, BleError> {
        self.0.held().await
    }

    async fn listen(&self, window: Duration, listening: &dyn Listening) -> Result<(), BleError> {
        self.0.listen(window, listening).await
    }

    async fn connect(&self, _unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        std::future::pending().await
    }
}

/// The daemon's stop in the middle of a passphrase test ends the walk within the jobs' bound and
/// closes the probe with the other actors, within theirs.
#[tokio::test(start_paused = true)]
async fn a_stop_in_the_middle_of_a_probe_closes_it_in_bound() {
    let (dir, config) = folder("stop", "");
    let daemon = daemon_in(&dir.0, config, Arc::new(Stuck(new_device())));
    let mut events = follow(&daemon).await;
    let job = started(&daemon).await;
    waiting(&mut events, job).await;
    answer(&daemon, job, 0, None).await;
    waiting(&mut events, job).await;
    answer(&daemon, job, 0, None).await;
    waiting(&mut events, job).await;
    answer(&daemon, job, 0, Some("desk")).await;
    let testing = loop {
        if let JobState::Progress { payload } = next(&mut events, job).await {
            let state: WalkState = serde_json::from_value(payload).expect("a walk state");
            if state.working == Some(Working::Testing) {
                break state;
            }
        }
    };
    assert_eq!(testing.phase, Phase::Working);
    tokio::time::sleep(Duration::from_millis(10)).await;
    let probe = read(&daemon.devices).probe.clone().expect("the probe runs");
    let since = Instant::now();
    daemon.shutdown().await;
    assert!(
        since.elapsed() <= JOB_STOP_BOUND + SHUTDOWN_DEADLINE + svolder_ipc::WRITE_SETTLE_BOUND,
        "{:?}",
        since.elapsed()
    );
    assert_eq!(ended(&mut events, job).await, JobState::Cancelled);
    tokio::time::sleep(Duration::from_millis(1)).await;
    assert!(probe.commands.is_closed(), "the probe's actor still runs");
}

/// A terminal's walk is the terminal's to a person: the refusal says "the terminal", never the
/// log's word `socket`, and says it for the AutoEq family's article too.
#[tokio::test]
async fn a_busy_refusal_names_a_terminals_walk_as_the_terminal() {
    let (dir, config) = folder("busy-terminal", "");
    let daemon = daemon_in(&dir.0, config, Arc::new(new_device()));
    let job = match walk(&daemon, Origin::Socket, WalkOp::Start).await {
        Response::Job { job } => job,
        other => panic!("the walk did not start: {other:?}"),
    };
    match walk(&daemon, Origin::Web, WalkOp::Start).await {
        Response::Error {
            message,
            reason: Some(Reason::Busy),
            ..
        } => {
            assert!(
                message.contains(&format!("(job {job}, started by the terminal)")),
                "{message}"
            );
            assert!(!message.contains("socket"), "{message}");
        }
        other => panic!("a second walk started: {other:?}"),
    }
    daemon.shutdown().await;
}
