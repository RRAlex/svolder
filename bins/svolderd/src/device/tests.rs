// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;

use super::*;

/// A handle with nothing behind it: `matches` is the only thing exercised against it, and
/// nothing it does reaches the channels this never spawned an actor to answer on.
fn bare_handle(name: &str, unit: &str, product: Option<&str>) -> DeviceHandle {
    let (commands, _rx) = mpsc::channel(1);
    let (events, _) = broadcast::channel(1);
    let (_tx, feed) = watch::channel(Feed::default());
    DeviceHandle {
        name: name.into(),
        udid: unit.into(),
        commands,
        writer: crate::writer::Writer::unattended(),
        events,
        info: Arc::new(Mutex::new(LiveInfo {
            signal: None,
            unit: unit.into(),
            product_id: 0,
            product: product.map(str::to_string),
            firmware: 0,
            profile: String::new(),
            connected: false,
            stale: false,
            next_attempt: None,
            passphrase_note: None,
            refused: false,
            history: Arc::default(),
            ring: Arc::default(),
        })),
        feed,
    }
}

/// An empty selector names no device: it is a prefix of every unit id, so without a guard it
/// would silently match whichever device happened to be checked first.
#[test]
fn an_empty_selector_matches_nothing() {
    let handle = bare_handle("fake", "0102030405060708", Some("Gungnir 2"));
    assert!(!handle.matches(""));
    assert!(handle.matches("fake"));
    assert!(handle.matches("Gungnir 2"));
    assert!(handle.matches("0102"));
}

/// A transport whose scan parks until a test releases it, so an actor can be observed
/// with a discover-and-connect attempt genuinely in flight. It stands in for a real
/// adapter, where the scan window alone is five seconds.
struct ParkedScan {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
    /// How many times a caller has asked this transport to settle, which is how a test
    /// tells whether the work an abandoned attempt left in flight was waited for.
    settles: Arc<std::sync::atomic::AtomicUsize>,
    /// The devices a caller asked this transport to settle, one at a time.
    settled: Arc<std::sync::Mutex<Vec<UnitId>>>,
}

impl ParkedScan {
    /// A transport parked on the notifications a test holds, with nothing settled yet.
    fn new(started: &Arc<tokio::sync::Notify>, release: &Arc<tokio::sync::Notify>) -> Self {
        Self {
            started: Arc::clone(started),
            release: Arc::clone(release),
            settles: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            settled: Arc::default(),
        }
    }

    /// How many times a caller has asked this transport to settle.
    fn settles(&self) -> usize {
        self.settles.load(Ordering::SeqCst)
    }

    /// The devices a caller asked this transport to settle.
    fn settled(&self) -> Vec<UnitId> {
        self.settled.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Transport for ParkedScan {
    async fn scan(&self, _window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        self.started.notify_one();
        self.release.notified().await;
        Ok(futures::stream::empty().boxed())
    }

    async fn connect(&self, _unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        Err(BleError::NotFound)
    }

    async fn settle(&self, _bound: Duration) {
        self.settles.fetch_add(1, Ordering::SeqCst);
    }

    async fn settle_for(&self, unit: UnitId, bound: Duration) {
        self.settled.lock().unwrap().push(unit);
        self.settle(bound).await;
    }
}

/// A transport that reports the moment of every discover-and-connect attempt made through
/// it, and that only lets one through while the device is put on the air.
///
/// The scan is what every attempt starts with, so the instants it sends are the attempts
/// themselves, and the gaps between them are the waits the retry schedule paced. Off the
/// air nothing is found and nothing connects, which is the device a schedule exists for:
/// one that is switched off.
struct CountedScan {
    inner: svolder_ble::FakeTransport,
    /// Every attempt's moment, in the order the attempts were made.
    attempts: mpsc::UnboundedSender<tokio::time::Instant>,
    /// Whether the device is on the air for the attempt running now.
    reachable: Arc<AtomicBool>,
}

impl CountedScan {
    /// A transport over the fake device, whose passphrase is `1234`, with the device off
    /// the air, and the receiver every attempt's moment arrives on.
    fn new() -> (
        Self,
        mpsc::UnboundedReceiver<tokio::time::Instant>,
        Arc<AtomicBool>,
    ) {
        let registry = Registry::builtin().expect("the built-in profiles load");
        let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
        let (attempts, seen) = mpsc::unbounded_channel();
        let reachable = Arc::new(AtomicBool::new(false));
        let transport = Self {
            inner: svolder_ble::FakeTransport::from_profile(
                &profile,
                "0102030405060708".parse().expect("a valid unit id"),
                3,
                Passphrase::from_text("1234").expect("four digits"),
            ),
            attempts,
            reachable: Arc::clone(&reachable),
        };
        (transport, seen, reachable)
    }

    /// Whether the device is on the air.
    fn reachable(&self) -> bool {
        self.reachable.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl Transport for CountedScan {
    async fn scan(&self, window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        let _ = self.attempts.send(tokio::time::Instant::now());
        if self.reachable() {
            self.inner.scan(window).await
        } else {
            Ok(futures::stream::empty().boxed())
        }
    }

    async fn held(&self) -> Result<svolder_ble::Advertisements, BleError> {
        if self.reachable() {
            self.inner.held().await
        } else {
            Ok(futures::stream::empty().boxed())
        }
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        if self.reachable() {
            self.inner.connect(unit).await
        } else {
            Err(BleError::NotFound)
        }
    }
}

/// The next attempt's moment, waited for on the channel the transport reports them on.
///
/// The clock is paused in these tests, so the wait itself costs nothing and time moves
/// only to the deadline the actor is sleeping on: the instant that arrives here is the
/// one the schedule asked for.
async fn next_attempt(
    seen: &mut mpsc::UnboundedReceiver<tokio::time::Instant>,
) -> tokio::time::Instant {
    tokio::time::timeout(Duration::from_secs(600), seen.recv())
        .await
        .expect("the actor made another attempt")
        .expect("the transport is still reporting attempts")
}

/// Let every other task run up to the point where it is waiting on the clock, so the
/// actor is inside the wait between two attempts rather than still finishing one.
///
/// Paused time only moves when nothing is runnable, so this returns once the actor has
/// settled, and it costs one millisecond of the wait it settled into.
async fn settle_into_the_wait() {
    tokio::time::sleep(Duration::from_millis(1)).await;
}

/// The retry schedule paces the attempts a device that is not there earns: the waits are
/// 2, 5, 15, 30 and 60 seconds, and the last one repeats for as long as the device stays
/// away.
///
/// Every attempt scans the adapter, so a fixed short retry is what keeps a radio
/// discovering for as long as a switched-off device is configured. The seconds are
/// spelled out here rather than read from [`RETRY_SCHEDULE`], since they are what
/// `docs/usage/daemon.md` promises and what the whole change is for.
#[tokio::test(start_paused = true)]
async fn the_retry_schedule_paces_the_attempts_a_missing_device_earns() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let _handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    let mut previous = next_attempt(&mut seen).await;
    let mut gaps = Vec::new();
    for _ in 0..6 {
        let at = next_attempt(&mut seen).await;
        gaps.push(at - previous);
        previous = at;
    }
    assert_eq!(
        gaps,
        [2, 5, 15, 30, 60, 60].map(Duration::from_secs),
        "the attempts were not paced by the schedule"
    );
}

/// A command that needs the link ends the wait at once and starts the schedule again, so
/// a user acting on a device that has just been switched back on never waits out a step.
#[tokio::test(start_paused = true)]
async fn a_command_that_needs_the_link_ends_the_wait_and_restarts_the_schedule() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    // Five failures in, the actor is waiting out the schedule's last and longest step.
    let mut last = next_attempt(&mut seen).await;
    for _ in 0..4 {
        last = next_attempt(&mut seen).await;
    }
    settle_into_the_wait().await;
    assert_eq!(
        handle.describe().retry_in_secs,
        Some(60),
        "the wait the device reports is not the schedule's last step"
    );

    // The command is refused, as it is whenever the link is down, and the attempt it
    // asks for runs there and then rather than a minute later.
    let refused = ask(
        &handle,
        Command::Set {
            key: "mute".into(),
            value: "on".into(),
            step: 1,
            no_wait: false,
        },
    )
    .await;
    assert!(
        matches!(&refused, Response::Error { code: 3, message } if message == "device not connected"),
        "got {refused:?}"
    );
    let woken = next_attempt(&mut seen).await;
    assert!(
        woken - last < Duration::from_secs(1),
        "the command left the actor waiting for {:?}",
        woken - last
    );
    assert_eq!(
        next_attempt(&mut seen).await - woken,
        RETRY_SCHEDULE[0],
        "the schedule did not start again from its first step"
    );
}

/// Try now ends the wait at once and starts the schedule again, and is answered `Ack`.
#[tokio::test(start_paused = true)]
async fn a_wake_ends_the_wait_and_restarts_the_schedule() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");
    let mut last = next_attempt(&mut seen).await;
    for _ in 0..4 {
        last = next_attempt(&mut seen).await;
    }
    settle_into_the_wait().await;
    assert_eq!(ask(&handle, Command::Wake).await, Response::Ack);
    let woken = next_attempt(&mut seen).await;
    assert!(
        woken - last < Duration::from_secs(1),
        "the wake left the actor waiting"
    );
    assert_eq!(next_attempt(&mut seen).await - woken, RETRY_SCHEDULE[0]);
}

/// A read during a wait is answered from the last known state and leaves the schedule
/// alone: nothing about it says the device is back.
#[tokio::test(start_paused = true)]
async fn a_read_during_the_wait_leaves_the_schedule_alone() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    let mut last = next_attempt(&mut seen).await;
    for _ in 0..4 {
        last = next_attempt(&mut seen).await;
    }
    settle_into_the_wait().await;

    let state = ask(&handle, Command::State).await;
    assert!(
        matches!(&state, Response::Error { code: 3, .. }),
        "got {state:?}"
    );
    assert_eq!(
        next_attempt(&mut seen).await - last,
        RETRY_SCHEDULE[RETRY_SCHEDULE.len() - 1],
        "the read moved the next attempt"
    );
}

/// A changed entry ends the wait and starts the schedule again: the next attempt is the
/// one that uses it, so a passphrase the user has just corrected is tried there and then
/// rather than at the end of a step that may be a minute long.
#[tokio::test(start_paused = true)]
async fn a_changed_entry_during_the_wait_ends_it_and_restarts_the_schedule() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    // Five failures in, the actor is waiting out the schedule's last and longest step.
    let mut last = next_attempt(&mut seen).await;
    for _ in 0..4 {
        last = next_attempt(&mut seen).await;
    }
    settle_into_the_wait().await;

    let acknowledged = ask(&handle, Command::Configure(fake_config("1234"))).await;
    assert!(
        matches!(acknowledged, Response::Ack),
        "got {acknowledged:?}"
    );
    let woken = next_attempt(&mut seen).await;
    assert!(
        woken - last < Duration::from_secs(1),
        "the changed entry left the actor waiting for {:?}",
        woken - last
    );
    assert_eq!(
        next_attempt(&mut seen).await - woken,
        RETRY_SCHEDULE[0],
        "the schedule did not start again from its first step"
    );
}

/// An entry that says what the actor is already doing, and a reload of the profiles,
/// leave the wait alone: neither says anything about the device being back.
///
/// This is what keeps the schedule meaningful at all, since the handler hands every device
/// it keeps a `Configure` on every reload — a reload of the profiles alone would otherwise
/// put every waiting device back to the schedule's first step.
#[tokio::test(start_paused = true)]
async fn an_unchanged_entry_and_a_profiles_reload_leave_the_schedule_alone() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    let mut last = next_attempt(&mut seen).await;
    for _ in 0..4 {
        last = next_attempt(&mut seen).await;
    }
    settle_into_the_wait().await;

    let same = ask(&handle, Command::Configure(fake_config("auto"))).await;
    assert!(matches!(same, Response::Ack), "got {same:?}");
    let reloaded = ask(
        &handle,
        Command::Reload(Arc::new(Registry::builtin().unwrap())),
    )
    .await;
    assert!(matches!(reloaded, Response::Ack), "got {reloaded:?}");
    assert_eq!(
        next_attempt(&mut seen).await - last,
        RETRY_SCHEDULE[RETRY_SCHEDULE.len() - 1],
        "an entry that changed nothing moved the next attempt"
    );
}

/// A link that comes up starts the schedule again, so the device that dropped once and
/// came back is tried at the first step rather than at the step its last absence reached.
#[tokio::test(start_paused = true)]
async fn a_connect_restarts_the_schedule() {
    let (transport, mut seen, reachable) = CountedScan::new();
    let transport = Arc::new(transport);
    let shared = Arc::clone(&transport) as Arc<dyn Transport>;
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    // Three failures in, the actor is three steps down the schedule; the device is put
    // on the air for the attempt that follows.
    for _ in 0..3 {
        next_attempt(&mut seen).await;
    }
    reachable.store(true, Ordering::SeqCst);
    next_attempt(&mut seen).await;
    for _ in 0..500 {
        if handle.describe().connected {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let live = handle.describe();
    assert!(
        live.connected,
        "the actor never connected to the fake device"
    );
    assert_eq!(
        live.retry_in_secs, None,
        "a connected device is not waiting for anything"
    );

    // The device goes away again: the link ends, and the attempt that follows it is the
    // schedule's first step away from the one that found nothing.
    reachable.store(false, Ordering::SeqCst);
    transport.inner.sever();
    let dropped = next_attempt(&mut seen).await;
    assert_eq!(
        next_attempt(&mut seen).await - dropped,
        RETRY_SCHEDULE[0],
        "the schedule did not start again from its first step"
    );
}

/// A transport whose scan is always quiet while it holds one device, which is what an
/// adapter does when its window falls between the reports a device is making.
struct QuietScan {
    inner: svolder_ble::FakeTransport,
}

#[async_trait::async_trait]
impl Transport for QuietScan {
    async fn scan(&self, _window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        Ok(futures::stream::empty().boxed())
    }

    async fn held(&self) -> Result<svolder_ble::Advertisements, BleError> {
        self.inner.held().await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        self.inner.connect(unit).await
    }
}

/// A transport whose first scan parks until a test releases it, and whose device is the
/// fake's, so an actor can be handed a changed entry with its first discover-and-connect
/// attempt genuinely in flight and the attempt then let through.
struct ParkedFirstScan {
    inner: svolder_ble::FakeTransport,
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
    /// Raised by the scan that parks, so only the first one does.
    parked: AtomicBool,
}

impl ParkedFirstScan {
    /// A transport over the fake device, whose passphrase is `1234`, parked on the
    /// notifications a test holds.
    fn new(started: &Arc<tokio::sync::Notify>, release: &Arc<tokio::sync::Notify>) -> Self {
        let registry = Registry::builtin().expect("the built-in profiles load");
        let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
        Self {
            inner: svolder_ble::FakeTransport::from_profile(
                &profile,
                "0102030405060708".parse().expect("a valid unit id"),
                3,
                Passphrase::from_text("1234").expect("four digits"),
            ),
            started: Arc::clone(started),
            release: Arc::clone(release),
            parked: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl Transport for ParkedFirstScan {
    async fn scan(&self, window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        if !self.parked.swap(true, Ordering::SeqCst) {
            self.started.notify_one();
            self.release.notified().await;
        }
        self.inner.scan(window).await
    }

    async fn held(&self) -> Result<svolder_ble::Advertisements, BleError> {
        self.inner.held().await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        self.inner.connect(unit).await
    }
}

/// An entry corrected while a connect attempt is in flight reaches the session that
/// attempt hands back, rather than only the entry stored for the next one.
///
/// The attempt carries a copy of the entry it was spawned with, so the session it opens
/// is keyed with the passphrase that copy names — the wrong one here, which the device
/// turns down during the refresh a connect ends with. Keying that session from the entry
/// the actor holds now is what lets the very next write go through; without it the link
/// would go on refusing every frame, under an entry the daemon has already answered a
/// reload for, until it dropped.
#[tokio::test]
async fn an_entry_changed_mid_attempt_keys_the_session_the_attempt_hands_back() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let transport = Arc::new(ParkedFirstScan::new(&started, &release)) as Arc<dyn Transport>;
        // The entry the attempt is built from names the wrong passphrase for the device.
        let shared: Arc<dyn Transport> = transport;
        let handle = spawn(
            Arc::clone(&shared),
            Arc::new(Registry::builtin().unwrap()),
            fake_config("9999"),
            scan_for(&shared),
        )
        .expect("the actor starts");

        // The actor is inside its attempt, parked in the scan: the entry it is handed
        // now is one only the end of that attempt can apply to the session it opens.
        started.notified().await;
        let acknowledged = ask(&handle, Command::Configure(fake_config("1234"))).await;
        assert!(
            matches!(acknowledged, Response::Ack),
            "got {acknowledged:?}"
        );
        release.notify_one();

        for _ in 0..500 {
            if handle.describe().connected {
                let written = ask(
                    &handle,
                    Command::Set {
                        key: "input".into(),
                        value: "4".into(),
                        step: 1,
                        no_wait: false,
                    },
                )
                .await;
                assert!(
                    matches!(&written, Response::Value(_)),
                    "the session was left keyed from the entry the attempt started with: \
                         {written:?}"
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("the actor never connected to the fake device");
    })
    .await
    .expect("test timed out");
}

/// A device the adapter holds is discovered even though the scan window was quiet: the
/// advertisement it holds names the profile and passphrase, and the connect that follows
/// is what settles whether the device is still there.
#[tokio::test]
async fn discover_finds_a_device_the_adapter_holds() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let transport = Arc::new(QuietScan {
        inner: svolder_ble::FakeTransport::from_profile(
            &profile,
            unit,
            3,
            Passphrase::from_text("1234").expect("four digits"),
        ),
    }) as Arc<dyn Transport>;

    let (advert, heard_at_ms) = discover(&transport, &scan_for(&transport), unit, None)
        .await
        .expect("the held device is discovered");
    assert_eq!(advert.unit_id(), unit);
    assert_eq!(heard_at_ms, None, "a held advertisement was not heard");
}

/// Send one command to an actor and wait a second for its answer.
async fn ask(handle: &DeviceHandle, command: Command) -> Response {
    let (tx, rx) = oneshot::channel();
    handle
        .commands
        .send((command, tx))
        .await
        .map_err(|_| ())
        .expect("the actor is running");
    tokio::time::timeout(Duration::from_secs(1), rx)
        .await
        .expect("the actor answered the command within a second")
        .expect("the actor did not drop the reply")
}

/// The actor serves commands throughout a discover-and-connect attempt: on a real adapter
/// one attempt is a five-second scan window plus a twenty-second connect deadline, and no
/// read needs the link.
#[tokio::test]
async fn commands_are_answered_while_a_connect_attempt_runs() {
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let transport = Arc::new(ParkedScan::new(&started, &release)) as Arc<dyn Transport>;
    let config = DeviceConfig {
        name: "parked".into(),
        udid: "0102030405060708".into(),
        profile: None,
        passphrase: "auto".into(),
        inputs: BTreeMap::new(),
    };
    let shared: Arc<dyn Transport> = transport;
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        config,
        scan_for(&shared),
    )
    .expect("the actor starts");

    // The actor is now inside its attempt, parked in the scan.
    started.notified().await;

    // Before a first sighting a read of the whole state says the device has not been
    // reached, rather than answering with an empty list of readings.
    let state = ask(&handle, Command::State).await;
    assert!(
        matches!(&state, Response::Error { code: 3, message } if *message == svolder_ipc::not_found_yet("parked")),
        "got {state:?}"
    );
    // A read of one key says the device has not been found, not that the key is unknown.
    let got = ask(
        &handle,
        Command::Get {
            key: "volume".into(),
        },
    )
    .await;
    assert!(
        matches!(&got, Response::Error { code: 3, message } if *message == svolder_ipc::not_found_yet("parked")),
        "got {got:?}"
    );
    let eq = ask(&handle, Command::EqShow).await;
    assert!(
        matches!(&eq, Response::Error { code: 3, message } if *message == svolder_ipc::not_found_yet("parked")),
        "got {eq:?}"
    );
    // A command that needs the link is refused at once rather than queued behind the attempt.
    let set = ask(
        &handle,
        Command::Set {
            key: "mute".into(),
            value: "on".into(),
            step: 1,
            no_wait: false,
        },
    )
    .await;
    assert!(
        matches!(&set, Response::Error { code: 3, message } if message == "device not connected"),
        "got {set:?}"
    );
    // The refusal gate still stands with no link: a firmware-update id is turned away.
    let refused = ask(
        &handle,
        Command::Raw {
            id: "210.1".into(),
            value_type: "u8".into(),
            value: "0".into(),
            no_wait: false,
        },
    )
    .await;
    assert!(
        matches!(&refused, Response::Error { code: 1, message } if message.contains("firmware")),
        "got {refused:?}"
    );
    // The device is reported as not connected throughout.
    assert!(!handle.describe().connected);

    release.notify_waiters();
}

/// A shutdown that arrives while an attempt is parked stops that attempt once its grace
/// has run out, and then waits on the transport.
///
/// Stopping the attempt is what hands the connect it was inside of back to the transport
/// with nobody to give a link to, and the transport takes that connection down on a task
/// of its own. The daemon's runtime is about to stop, so the wait here is the only thing
/// that lets the disconnect reach the adapter: without it a stop while the device is off
/// leaves BlueZ holding a connection nobody cancels.
#[tokio::test(start_paused = true)]
async fn a_shutdown_that_outlasts_its_grace_stops_the_attempt_and_settles() {
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let parked = Arc::new(ParkedScan::new(&started, &release));
    let shared = Arc::clone(&parked) as Arc<dyn Transport>;
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    // The actor is inside its attempt, parked in the scan, and nothing releases it: the
    // grace is what ends the wait.
    started.notified().await;
    assert_eq!(parked.settles(), 0, "nothing has been given up on yet");

    let (tx, rx) = oneshot::channel();
    handle
        .commands
        .send((Command::Shutdown, tx))
        .await
        .map_err(|_| ())
        .expect("the actor is running");
    let farewell = tokio::time::timeout(Duration::from_secs(60), rx)
        .await
        .expect("the actor answered the shutdown")
        .expect("the actor did not drop the reply");
    assert!(matches!(farewell, Response::Ack), "got {farewell:?}");
    assert_eq!(
        parked.settles(),
        1,
        "the stopped attempt's release was not waited for exactly once"
    );
    assert_eq!(
        parked.settled(),
        ["0102030405060708".parse::<UnitId>().unwrap()],
        "the actor settled other devices' work, or none"
    );
}

/// A transport whose links take a write forever, so a connect attempt holds a live link it can
/// never finish refreshing, and whose `settle_for` notes whether that link had been dropped by
/// then.
struct HungLinks {
    inner: svolder_ble::FakeTransport,
    dropped: Arc<AtomicBool>,
    /// What `dropped` said when the last `settle_for` began.
    dropped_at_settle: Arc<std::sync::Mutex<Option<bool>>>,
}

struct HungLink(Arc<AtomicBool>);

impl Drop for HungLink {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl svolder_ble::Link for HungLink {
    async fn write(&self, _frame: &[u8]) -> Result<(), BleError> {
        std::future::pending().await
    }

    fn notifications(&self) -> svolder_ble::Notifications {
        futures::stream::pending().boxed()
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl Transport for HungLinks {
    async fn scan(&self, window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        self.inner.scan(window).await
    }

    async fn connect(&self, _unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        Ok(Box::new(HungLink(Arc::clone(&self.dropped))))
    }

    async fn settle_for(&self, _unit: UnitId, _bound: Duration) {
        *self.dropped_at_settle.lock().unwrap() = Some(self.dropped.load(Ordering::SeqCst));
    }
}

/// A shutdown that stops an attempt holding a live link waits for the link's drop before it
/// settles, so a release the drop records is one the settle sees.
#[tokio::test(start_paused = true)]
async fn a_stopped_attempt_is_dropped_before_the_actor_settles() {
    let profile = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    let unit: UnitId = "0102030405060708".parse().unwrap();
    let passphrase = svolder_proto::Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let dropped = Arc::new(AtomicBool::new(false));
    let dropped_at_settle = Arc::new(std::sync::Mutex::new(None));
    let transport = Arc::new(HungLinks {
        inner: svolder_ble::FakeTransport::from_profile(&profile, unit, 3, passphrase),
        dropped: Arc::clone(&dropped),
        dropped_at_settle: Arc::clone(&dropped_at_settle),
    });
    let shared: Arc<dyn Transport> = transport;
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    // The attempt opens its link and hangs in the refresh's first write.
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(!dropped.load(Ordering::SeqCst), "the link went early");

    let (tx, rx) = oneshot::channel();
    handle
        .commands
        .send((Command::Shutdown, tx))
        .await
        .map_err(|_| ())
        .expect("the actor is running");
    let farewell = tokio::time::timeout(Duration::from_secs(60), rx)
        .await
        .expect("the actor answered the shutdown")
        .expect("the actor did not drop the reply");
    assert!(matches!(farewell, Response::Ack), "got {farewell:?}");
    assert_eq!(
        *dropped_at_settle.lock().unwrap(),
        Some(true),
        "the actor settled before the aborted attempt dropped its link"
    );
}

/// One device configuration for the fake device's own unit id.
fn fake_config(passphrase: &str) -> DeviceConfig {
    DeviceConfig {
        name: "fake".into(),
        udid: "0102030405060708".into(),
        profile: None,
        passphrase: passphrase.into(),
        inputs: BTreeMap::new(),
    }
}

/// A transport over the fake that raises `cancel` as a connect begins: a shutdown that arrives
/// while the attempt is inside its connect.
struct StopsOnConnect {
    inner: svolder_ble::FakeTransport,
    cancel: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl Transport for StopsOnConnect {
    async fn scan(&self, window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        self.inner.scan(window).await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        self.cancel.store(true, Ordering::SeqCst);
        self.inner.connect(unit).await
    }
}

/// Run one discover-and-connect attempt over a fake device whose passphrase is `1234`,
/// handing it a session opened with the wrong one, and hand back the session it returns.
///
/// `cancel` is the flag a shutting-down actor sets, raised as the connect begins: the attempt
/// is asked to close the session it connected rather than hand back a live link.
async fn run_one_attempt(configured: &str, cancel: bool) -> Session {
    let registry = Arc::new(Registry::builtin().unwrap());
    let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let flag = Arc::new(AtomicBool::new(false));
    let fake = svolder_ble::FakeTransport::from_profile(
        &profile,
        unit,
        3,
        Passphrase::from_text("1234").expect("four digits"),
    );
    let transport = match cancel {
        true => Arc::new(StopsOnConnect {
            inner: fake,
            cancel: Arc::clone(&flag),
        }) as Arc<dyn Transport>,
        false => Arc::new(fake) as Arc<dyn Transport>,
    };
    let advert = transport
        .scan(Duration::from_millis(1))
        .await
        .expect("the fake always scans")
        .next()
        .await
        .expect("the fake advertises itself");
    let session = Session::new(
        Arc::clone(&transport),
        advert,
        profile,
        Passphrase::from_text("9999").expect("four digits"),
    );
    let scanner = scan_for(&transport);
    let attempt = connect_attempt(
        transport,
        scanner,
        unit,
        registry,
        fake_config(configured),
        Carried {
            session: Some(session),
            episode: Episode::default(),
        },
        flag,
    )
    .await;
    assert!(attempt.outcome.is_ok(), "got {:?}", attempt.outcome);
    attempt.session.expect("the session comes back")
}

/// An attempt asked to stand down closes the session it has just connected, so a
/// shutting-down actor is handed a released radio rather than a live link to close after
/// it has stopped waiting.
#[tokio::test]
async fn a_cancelled_attempt_closes_the_session_it_connected() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let closed = run_one_attempt("auto", true).await;
        assert!(!closed.is_connected(), "the link was handed back open");
        assert!(
            closed.state().is_stale(),
            "the readings are not marked stale"
        );

        // Without the flag the same attempt hands the link back live, which is what the
        // actor goes on serving commands through.
        let live = run_one_attempt("auto", false).await;
        assert!(live.is_connected(), "the link was closed unasked");
    })
    .await
    .expect("test timed out");
}

/// A session the actor already holds is re-keyed from the advertisement of the attempt
/// that reconnects it, when the device is configured `auto`: a passphrase changed from
/// the official app while the device was away is picked up without a restart.
///
/// The fake device rejects a frame encoded with the wrong passphrase, so a write is what
/// tells the two cases apart. A device configured with literal digits keeps them, as it
/// must: the file is the user's to update.
#[tokio::test]
async fn a_reused_session_takes_the_advertised_passphrase_under_auto() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut rekeyed = run_one_attempt("auto", false).await;
        let confirmed = rekeyed
            .set(Message::command(FunctionId::one(2), Value::U8(3)))
            .await;
        assert!(confirmed.is_ok(), "got {confirmed:?}");

        let mut kept = run_one_attempt("9999", false).await;
        let refused = kept
            .set(Message::command(FunctionId::one(2), Value::U8(3)))
            .await;
        assert!(
            matches!(refused, Err(BleError::PassphraseRejected)),
            "got {refused:?}"
        );
    })
    .await
    .expect("test timed out");
}

#[test]
fn the_firmware_family_and_the_passphrase_function_are_refused() {
    let refused = refuse(FunctionId::two(210, 1)).expect("210.1 is restricted");
    assert!(
        matches!(&refused, Response::Error { code: 1, message } if message.contains("firmware")),
        "got {refused:?}"
    );
    let sensitive = refuse(FunctionId::one(250)).expect("250 is sensitive");
    assert!(
        matches!(&sensitive, Response::Error { code: 1, message }
                if message == "function 250 changes the passphrase; use `svolder passphrase set`"),
        "got {sensitive:?}"
    );
    assert!(refuse(FunctionId::one(10)).is_none(), "a normal id passes");
    assert!(
        refuse(FunctionId::three(210, 2, 1)).is_none(),
        "the version query is not restricted"
    );
}

#[test]
fn raw_requests_parse_the_way_the_cli_parses_them() {
    let m = build_raw("2", "u8", "4").expect("a valid raw request");
    assert_eq!(m.id, FunctionId::one(2));
    assert_eq!(m.value, Value::U8(4));
    for (id, ty, value) in [("nope", "u8", "1"), ("2", "u9", "1"), ("2", "u8", "300")] {
        let err = build_raw(id, ty, value).unwrap_err();
        assert_eq!(err.code, 1, "{id} {ty} {value}: {}", err.message);
    }
}

#[test]
fn link_failures_map_to_the_codes_the_cli_exits_with() {
    assert_eq!(code_for(&BleError::NotFound), 2);
    assert_eq!(code_for(&BleError::Connection("x".into())), 3);
    assert_eq!(code_for(&BleError::Bootloader), 3);
    assert_eq!(code_for(&BleError::PassphraseRejected), 3);
    assert_eq!(code_for(&BleError::Adapter("x".into())), 3);
    assert_eq!(code_for(&BleError::Timeout(Duration::from_secs(1))), 4);
    assert_eq!(code_for(&BleError::Proto(ProtoError::BadUnitId)), 1);
    assert_eq!(code_for(&BleError::Profile(ProfileError::NoEq)), 1);
}

/// The daemon refuses a device with no tone control in the profile crate's own words, the
/// ones the CLI's direct path shows too.
#[test]
fn no_tone_control_is_refused_in_the_profile_crates_words() {
    let said = ProfileError::NoEq.to_string();
    assert!(
        matches!(no_eq(), Response::Error { code: 1, ref message } if *message == said),
        "got {:?}",
        no_eq()
    );
}

/// An actor built by hand over `transport`, holding a connected session over the fake gungnir2
/// device (read through `gungnir2-v2`) and the advertisement that session came from, the way
/// the actor is after a connect, with nothing running.
async fn connected_actor(transport: Arc<dyn Transport>) -> Actor {
    connected_actor_on("gungnir2-v2", transport).await
}

/// [`connected_actor`] for the built-in profile `profile`.
async fn connected_actor_on(profile: &str, transport: Arc<dyn Transport>) -> Actor {
    let registry = Registry::builtin().unwrap();
    let fake = svolder_ble::FakeTransport::from_profile(
        registry.get(profile).expect("a built-in"),
        "0102030405060708".parse().expect("a valid unit id"),
        3,
        Passphrase::from_text("1234").expect("four digits"),
    );
    connected_actor_over(profile, Arc::new(fake), transport).await
}

/// [`connected_actor_on`] over the fake device `fake`, which the test keeps to look at.
async fn connected_actor_over(
    profile: &str,
    fake: Arc<svolder_ble::FakeTransport>,
    transport: Arc<dyn Transport>,
) -> Actor {
    let registry = Arc::new(Registry::builtin().unwrap());
    let profile = registry.get(profile).expect("a built-in").clone();
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let fake = fake as Arc<dyn Transport>;
    let advert = fake
        .scan(Duration::from_millis(1))
        .await
        .expect("the fake always scans")
        .next()
        .await
        .expect("the fake advertises itself");
    let mut session = Session::new(
        fake,
        advert.clone(),
        profile.clone(),
        Passphrase::from_text("1234").expect("four digits"),
    );
    session.connect().await.expect("the fake connects");
    let (events, _) = broadcast::channel(EVENT_QUEUE);
    let (feed, _) = watch::channel(Feed::default());
    let scanner = scan_for(&transport);
    Actor {
        name: "fake".into(),
        unit,
        profile,
        advert: Some(advert),
        config: fake_config("1234"),
        transport,
        registry,
        scanner,
        events,
        info: Arc::new(Mutex::new(LiveInfo {
            signal: None,
            unit: unit.to_string(),
            product_id: 0,
            product: None,
            firmware: 0,
            profile: String::new(),
            connected: true,
            stale: false,
            next_attempt: None,
            passphrase_note: None,
            refused: false,
            history: Arc::default(),
            ring: Arc::default(),
        })),
        feed,
        session: Some(session),
        last_state: DeviceState::default(),
        last_failure: None,
        note: None,
        failures: 0,
        farewell: None,
        episode: Episode::default(),
    }
}

/// The profile a reload resolves differs from the one the live session was opened with, and
/// the session follows it: the next refresh burst waits on the new profile's functions.
#[tokio::test]
async fn a_reload_hands_a_live_session_the_profile_it_now_resolves() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut actor = connected_actor(Arc::new(svolder_ble::FakeTransport::from_profile(
            Registry::builtin().unwrap().get("gungnir2-v2").unwrap(),
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::NONE,
        )))
        .await;
        let session = actor.session.as_ref().expect("a live session");
        assert_eq!(session.profile().name, "gungnir2-v2");

        // The entry names another profile and the profiles are reloaded: the actor's
        // re-resolution lands on it.
        actor.config.profile = Some("gungnir2-v1".into());
        actor.reload(Arc::new(Registry::builtin().unwrap()));
        assert_eq!(actor.profile.name, "gungnir2-v1");
        let session = actor.session.as_ref().expect("a live session");
        assert_eq!(session.profile().name, "gungnir2-v1");
    })
    .await
    .expect("test timed out");
}

/// A profile change that lands while a connect attempt is in flight, the session being inside
/// the attempt, reaches that session when the attempt hands it back, even when the attempt
/// found nothing and brings no advertisement.
#[tokio::test]
async fn a_reload_during_an_attempt_reaches_the_session_it_hands_back() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let transport = Arc::new(ParkedScan::new(&started, &release)) as Arc<dyn Transport>;
        let mut actor = connected_actor(transport).await;
        let (commands, mut rx) = mpsc::channel(COMMAND_QUEUE);

        let mut changed = fake_config("1234");
        changed.profile = Some("gungnir2-v1".into());
        let driver = async {
            // The attempt is parked in its scan, with the session inside it.
            started.notified().await;
            let (reply, answered) = oneshot::channel();
            commands
                .send((Command::Configure(changed), reply))
                .await
                .map_err(|_| ())
                .expect("the actor is running");
            let acknowledged = answered.await.expect("the actor answered");
            assert!(
                matches!(acknowledged, Response::Ack),
                "got {acknowledged:?}"
            );
            // The scan finds nothing: the attempt fails and hands the session back as it was.
            release.notify_one();
        };
        let (end, ()) = tokio::join!(actor.attempt(&mut rx), driver);
        assert!(
            matches!(end, AttemptEnd::Failed(_)),
            "the attempt found nothing"
        );
        assert_eq!(actor.profile.name, "gungnir2-v1");
        let session = actor.session.as_ref().expect("the session came back");
        assert_eq!(session.profile().name, "gungnir2-v1");
    })
    .await
    .expect("test timed out");
}

/// An attempt that finds nothing hands a session back as it was while the actor's profile is
/// still the placeholder it names before any advertisement has been seen.
#[tokio::test]
async fn an_attempt_that_finds_nothing_leaves_the_session_the_placeholder_profile_unsent() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let transport = Arc::new(ParkedScan::new(&started, &release)) as Arc<dyn Transport>;
        let mut actor = connected_actor(transport).await;
        actor.advert = None;
        actor.profile = Registry::generic(0);
        let (_commands, mut rx) = mpsc::channel(COMMAND_QUEUE);
        let driver = async {
            started.notified().await;
            release.notify_one();
        };
        let (end, ()) = tokio::join!(actor.attempt(&mut rx), driver);
        assert!(
            matches!(end, AttemptEnd::Failed(_)),
            "the attempt found nothing"
        );
        let session = actor.session.as_ref().expect("the session came back");
        assert_eq!(session.profile().name, "gungnir2-v2");
    })
    .await
    .expect("test timed out");
}

/// An entry whose passphrase is no passphrase leaves the actor without a link: the attempt
/// fails after discovery, and every command that needs the link answers code 3, "not
/// connected", the code of a connection that failed.
#[tokio::test]
async fn an_unusable_passphrase_leaves_the_device_not_connected_with_code_3() {
    let registry = Arc::new(Registry::builtin().unwrap());
    let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let transport = Arc::new(svolder_ble::FakeTransport::from_profile(
        &profile,
        unit,
        3,
        Passphrase::from_text("1234").expect("four digits"),
    )) as Arc<dyn Transport>;
    let shared: Arc<dyn Transport> = transport;
    let handle = spawn(
        Arc::clone(&shared),
        registry,
        fake_config("12"),
        scan_for(&shared),
    )
    .expect("the actor starts");

    // The attempt has found the device once the actor names its profile.
    tokio::time::timeout(Duration::from_secs(5), async {
        while handle.describe().profile.is_empty() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the attempt never discovered the device");

    let set = ask(
        &handle,
        Command::Set {
            key: "mute".into(),
            value: "on".into(),
            step: 1,
            no_wait: false,
        },
    )
    .await;
    assert!(
        matches!(&set, Response::Error { code: 3, message } if message == "device not connected"),
        "got {set:?}"
    );
    assert!(!handle.describe().connected);
}

/// Hold the wall clock the actor reads at `ms` unix milliseconds.
fn clock_at(ms: u64) {
    TEST_CLOCK.with(|c| c.set(Some(ms)));
}

/// The transitions of a handle's history over `span_secs`, as (at_ms, connected).
fn marks(handle: &DeviceHandle, span_secs: u64) -> Vec<(u64, bool)> {
    match handle.history(span_secs) {
        Response::History { transitions, .. } => {
            transitions.iter().map(|t| (t.at_ms, t.connected)).collect()
        }
        other => panic!("got {other:?}"),
    }
}

/// Connecting, losing the link and regaining it record three transitions with their
/// wall-clock times, in order; saying the same state again records nothing.
#[tokio::test(start_paused = true)]
async fn the_actor_records_each_change_of_the_link() {
    let actor = connected_actor(Arc::new(svolder_ble::FakeTransport::from_profile(
        Registry::builtin().unwrap().get("gungnir2-v2").unwrap(),
        "0102030405060708".parse().unwrap(),
        3,
        Passphrase::from_text("1234").unwrap(),
    )))
    .await;
    actor.info.lock().unwrap().connected = false;
    let handle = DeviceHandle {
        name: "fake".into(),
        udid: "0102030405060708".into(),
        commands: mpsc::channel(1).0,
        writer: crate::writer::Writer::unattended(),
        events: actor.events.clone(),
        info: Arc::clone(&actor.info),
        feed: watch::channel(Feed::default()).1,
    };
    assert!(marks(&handle, 3600).is_empty(), "never connected: nothing");

    clock_at(1_000_000);
    actor.set_connected(true);
    actor.set_connected(true);
    clock_at(1_060_000);
    actor.set_connected(false);
    clock_at(1_120_000);
    actor.set_connected(true);
    actor.set_connected(true);

    assert_eq!(
        marks(&handle, 3600),
        vec![(1_000_000, true), (1_060_000, false), (1_120_000, true)]
    );
}

/// A panic while the device's info was locked poisons the lock, and every later change is still
/// taken: what `devices` reports never freezes at the moment of the panic, and the device still
/// answers to its unit id.
#[tokio::test(start_paused = true)]
async fn a_poisoned_info_lock_still_takes_every_change() {
    let actor = connected_actor(Arc::new(svolder_ble::FakeTransport::from_profile(
        Registry::builtin().unwrap().get("gungnir2-v2").unwrap(),
        "0102030405060708".parse().unwrap(),
        3,
        Passphrase::from_text("1234").unwrap(),
    )))
    .await;
    let info = Arc::clone(&actor.info);
    let panicked = std::thread::spawn(move || {
        let _held = info.lock().unwrap();
        panic!("a panic while the device's info is locked");
    })
    .join();
    assert!(panicked.is_err() && actor.info.is_poisoned());
    let handle = DeviceHandle {
        name: "fake".into(),
        udid: "0102030405060708".into(),
        commands: mpsc::channel(1).0,
        writer: crate::writer::Writer::unattended(),
        events: actor.events.clone(),
        info: Arc::clone(&actor.info),
        feed: watch::channel(Feed::default()).1,
    };
    actor.set_connected(false);
    assert!(!handle.describe().connected, "the lost link was dropped");
    actor.set_next_attempt(Some(tokio::time::Instant::now() + Duration::from_secs(5)));
    assert_eq!(handle.describe().retry_in_secs, Some(5));
    assert!(
        handle.matches("0102"),
        "a poisoned lock made the device unmatchable"
    );
}

/// The ring holds at most 4096 transitions, and nothing older than a day survives an insert.
#[test]
fn the_history_ring_is_capped_in_count_and_age() {
    let mut history = History::default();
    for i in 0..5000u64 {
        history.record(1_000 + i, i % 2 == 0);
    }
    assert_eq!(history.len(), HISTORY_CAP);
    assert_eq!(history.oldest_ms(), Some(1_000 + 5000 - HISTORY_CAP as u64));

    let day = 24 * 3600 * 1000;
    let mut history = History::default();
    history.record(10, true);
    history.record(20, false);
    history.record(day + 15, true);
    assert_eq!(history.len(), 3, "the entry at 10 ms stays as the anchor");
    assert_eq!(history.oldest_ms(), Some(10));
    history.record(day + 25, false);
    assert_eq!(history.len(), 3, "the one at 20 ms is the anchor now");
    assert_eq!(history.oldest_ms(), Some(20));
}

/// A span returns the transitions inside it plus the last one before it; a span over a day
/// is a day; a clock stepped back leaves a later-stamped transition in the reply as it is.
#[test]
fn a_history_span_carries_the_state_at_its_start() {
    let mut history = History::default();
    history.record(1_000_000, true);
    history.record(2_000_000, false);
    history.record(3_000_000, true);
    let now = 3_500_000;
    let at = |span_secs| -> Vec<u64> {
        history
            .within(now, span_secs)
            .iter()
            .map(|t| t.at_ms)
            .collect()
    };
    assert_eq!(at(400), vec![3_000_000,]);
    assert_eq!(at(1000), vec![2_000_000, 3_000_000]);
    assert_eq!(at(2000), vec![1_000_000, 2_000_000, 3_000_000]);
    assert_eq!(at(0), vec![3_000_000], "the state now, at least");
    // Far beyond a day clamps to a day: the ring is empty before 1_000_000 anyway, so the
    // clamp shows against a now a day and a bit later.
    let late = 1_000_000 + 25 * 3600 * 1000;
    let clamped: Vec<u64> = history
        .within(late, 10 * 24 * 3600)
        .iter()
        .map(|t| t.at_ms)
        .collect();
    assert_eq!(
        clamped,
        vec![3_000_000],
        "only the last one sits before the day's start"
    );

    // A clock stepped back: now is earlier than the newest transition.
    let back: Vec<u64> = history
        .within(1_500_000, 600)
        .iter()
        .map(|t| t.at_ms)
        .collect();
    assert_eq!(back, vec![1_000_000, 2_000_000, 3_000_000]);
    assert!(history.within(0, 600).iter().all(|t| t.at_ms > 0));
}

/// A link that came up and stayed up for more than a day keeps its anchor: a day-long span
/// still starts with the state the link was in.
#[test]
fn the_history_keeps_the_change_before_the_day() {
    let hour = 3600 * 1000;
    let t0 = 1_000 * hour;
    let mut history = History::default();
    history.record(t0, true);
    history.record(t0 + 25 * hour, false);
    let reply: Vec<(u64, bool)> = history
        .within(t0 + 26 * hour, 24 * 3600)
        .iter()
        .map(|t| (t.at_ms, t.connected))
        .collect();
    assert_eq!(reply, vec![(t0, true), (t0 + 25 * hour, false)]);
}

/// A transport over the fake device, on the air as a gungnir2 whose code is `1234`, whose
/// advertisements say what a test makes them say: the device's own code until told otherwise,
/// then a code, or none. It stands in for an adapter that hands back an advertisement from its
/// cache, older than the device's code.
struct Shows {
    inner: svolder_ble::FakeTransport,
    /// What the passphrase field of every advertisement carries: `None` leaves the device's
    /// own, and `Some("none")` or four digits replace it.
    shown: Mutex<Option<&'static str>>,
}

impl Shows {
    /// A transport over a fake device whose code is `1234`, advertising it truthfully.
    fn new() -> Arc<Self> {
        let registry = Registry::builtin().expect("the built-in profiles load");
        let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
        Arc::new(Self {
            inner: svolder_ble::FakeTransport::from_profile(
                &profile,
                "0102030405060708".parse().expect("a valid unit id"),
                3,
                Passphrase::from_text("1234").expect("four digits"),
            ),
            shown: Mutex::new(None),
        })
    }

    /// Have every advertisement from now on carry `code`.
    fn show(&self, code: &'static str) {
        *self.shown.lock().unwrap() = Some(code);
    }
}

/// `advert` with the passphrase field a [`Shows`] transport says, when it says one.
fn said(mut advert: Advertisement, shown: Option<&str>) -> Advertisement {
    let Some(code) = shown else {
        return advert;
    };
    advert.manufacturer.encrypted_passphrase = match Passphrase::from_text(code) {
        Ok(p) if !p.is_none() => Passphrase::encrypt(
            p.encoded(),
            Passphrase::advert_key(advert.manufacturer.unit_id),
        ),
        _ => 0,
    };
    advert
}

#[async_trait::async_trait]
impl Transport for Shows {
    async fn scan(&self, window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        let heard = self.inner.scan(window).await?;
        let shown = *self.shown.lock().unwrap();
        Ok(heard.map(move |advert| said(advert, shown)).boxed())
    }

    async fn held(&self) -> Result<svolder_ble::Advertisements, BleError> {
        self.scan(Duration::ZERO).await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        self.inner.connect(unit).await
    }
}

/// Spawn an actor on `transport` with the entry's passphrase `configured`, and wait for the
/// attempt to finish: connected, or failed after finding the device.
async fn spawned_on(transport: &Arc<Shows>, configured: &str) -> DeviceHandle {
    let shared = Arc::clone(transport) as Arc<dyn Transport>;
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config(configured),
        scan_for(&shared),
    )
    .expect("the actor starts");
    attempted(&handle).await;
    handle
}

/// Wait until the actor has taken up an advertisement and its attempt has ended.
async fn attempted(handle: &DeviceHandle) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let info = handle.describe();
            if info.connected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the actor never connected to the fake device");
}

/// A code that differs from the advertised one earns a note, the link is made and kept all the
/// same, and the note carries no digit of either code.
#[tokio::test]
async fn a_code_that_differs_from_the_advertised_one_earns_a_note() {
    let transport = Shows::new();
    let handle = spawned_on(&transport, "9999").await;
    let info = handle.describe();
    assert_eq!(info.passphrase_note, Some(PassphraseNote::Differs));
    assert!(info.connected, "the note changes nothing about the link");
    let wire = svolder_ipc::socket::protocol::to_line(&Response::Devices {
        devices: vec![info],
    })
    .unwrap();
    // Looked at field by field, not as text: a clock reading in the line can hold any digits.
    fn leaks(value: &serde_json::Value, digits: &str) -> bool {
        match value {
            serde_json::Value::String(text) => text.contains(digits),
            serde_json::Value::Number(number) => number.to_string() == digits,
            serde_json::Value::Array(items) => items.iter().any(|item| leaks(item, digits)),
            serde_json::Value::Object(fields) => fields
                .iter()
                .any(|(key, item)| key.contains(digits) || leaks(item, digits)),
            _ => false,
        }
    }
    let sent: serde_json::Value = serde_json::from_str(&wire).unwrap();
    for digits in ["1234", "9999", "4321"] {
        assert!(!leaks(&sent, digits), "{digits} crossed the socket: {wire}");
    }
}

/// `auto` follows the advertisement, so it never earns a note, whatever the advertisement
/// says.
#[tokio::test]
async fn auto_never_earns_a_note() {
    let transport = Shows::new();
    transport.show("4321");
    let handle = spawned_on(&transport, "auto").await;
    assert_eq!(handle.describe().passphrase_note, None);
}

/// `none` while the advertisement carries a code is a note of its own.
#[tokio::test]
async fn none_while_a_code_is_advertised_earns_a_note() {
    let transport = Shows::new();
    let handle = spawned_on(&transport, "none").await;
    assert_eq!(
        handle.describe().passphrase_note,
        Some(PassphraseNote::NoneConfigured)
    );
}

/// Four digits while the advertisement carries no code are a note of their own words.
#[tokio::test]
async fn digits_while_no_code_is_advertised_earn_their_own_note() {
    let transport = Shows::new();
    transport.show("none");
    let handle = spawned_on(&transport, "9999").await;
    assert_eq!(
        handle.describe().passphrase_note,
        Some(PassphraseNote::NoCodeAdvertised)
    );
}

/// A device that answered the connect's refresh in full has shown the code works, whatever an
/// advertisement older than its code says: the verdict stands for an advertisement that
/// differs, and the note does not.
#[tokio::test]
async fn a_device_that_answers_the_connect_burst_clears_a_stale_verdict() {
    let transport = Shows::new();
    transport.show("4321");
    let handle = spawned_on(&transport, "1234").await;
    assert_eq!(handle.describe().passphrase_note, None);
}

/// The firmware versions a connect asks for come back as readings the feed carries, stamped as
/// heard from the device, so the web's facts no longer fall back to the advertised build.
#[tokio::test]
async fn the_answer_becomes_a_reading() {
    let transport = Shows::new();
    let handle = spawned_on(&transport, "1234").await;
    let mut feed = handle.feed.clone();
    let firmware = FunctionId::three(210, 2, 1);
    let dsp = FunctionId::three(210, 2, 2);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            {
                let feed = feed.borrow_and_update();
                let held = |id| {
                    feed.readings
                        .iter()
                        .find(|m| m.id == id)
                        .map(|m| m.value.clone())
                };
                if held(firmware).is_some() && held(dsp).is_some() {
                    assert_eq!(held(firmware), Some(Value::U16(3)));
                    assert_eq!(held(dsp), Some(Value::U16(3)));
                    assert!(feed.heard_at.contains_key(&firmware));
                    assert!(feed.heard_at.contains_key(&dsp));
                    return;
                }
            }
            feed.changed().await.expect("the actor is running");
        }
    })
    .await
    .expect("the firmware answers never reached the feed");
}

/// Correcting the entry takes the note away, and spoiling it again brings it back.
#[tokio::test]
async fn correcting_the_entry_clears_the_note_and_spoiling_it_raises_it_again() {
    let transport = Shows::new();
    let handle = spawned_on(&transport, "9999").await;
    assert_eq!(
        handle.describe().passphrase_note,
        Some(PassphraseNote::Differs)
    );
    for (entry, note) in [
        ("1234", None),
        ("9999", Some(PassphraseNote::Differs)),
        ("auto", None),
    ] {
        let answered = ask(&handle, Command::Configure(fake_config(entry))).await;
        assert!(matches!(answered, Response::Ack), "got {answered:?}");
        assert_eq!(handle.describe().passphrase_note, note, "entry {entry}");
    }
}

/// An advertisement that later matches the entry takes the note away at the attempt that
/// reads it.
#[tokio::test]
async fn a_later_advertisement_that_matches_clears_the_note() {
    let transport = Shows::new();
    let handle = spawned_on(&transport, "9999").await;
    assert_eq!(
        handle.describe().passphrase_note,
        Some(PassphraseNote::Differs)
    );
    transport.show("9999");
    transport.inner.sever();
    tokio::time::timeout(Duration::from_secs(5), async {
        while handle.describe().passphrase_note.is_some() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the note outlived an advertisement that matches");
}

/// An actor over a session whose refresh proves nothing — the profile names no function, so
/// nothing is awaited — and an advertisement that differs from the entry's `1234`, which the
/// device itself accepts.
async fn unproven_actor() -> Actor {
    let registry = Arc::new(Registry::builtin().unwrap());
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let generic = Registry::generic(0);
    let fake = Arc::new(svolder_ble::FakeTransport::from_profile(
        &generic,
        unit,
        3,
        Passphrase::from_text("1234").expect("four digits"),
    )) as Arc<dyn Transport>;
    let mut advert = fake
        .scan(Duration::from_millis(1))
        .await
        .expect("the fake always scans")
        .next()
        .await
        .expect("the fake advertises itself");
    let mut session = Session::new(
        Arc::clone(&fake),
        advert.clone(),
        generic.clone(),
        Passphrase::from_text("1234").expect("four digits"),
    );
    session.connect().await.expect("the fake connects");
    assert!(
        !session.code_accepted(),
        "a refresh that awaits nothing proves nothing"
    );
    advert.manufacturer.encrypted_passphrase = Passphrase::encrypt(
        Passphrase::from_text("4321").unwrap().encoded(),
        Passphrase::advert_key(unit),
    );
    let (events, _) = broadcast::channel(EVENT_QUEUE);
    let scanner = scan_for(&fake);
    Actor {
        name: "fake".into(),
        unit,
        profile: generic,
        advert: Some(advert),
        config: fake_config("1234"),
        transport: fake,
        registry,
        scanner,
        events,
        info: Arc::new(Mutex::new(LiveInfo {
            signal: None,
            unit: unit.to_string(),
            product_id: 0,
            product: None,
            firmware: 0,
            profile: String::new(),
            connected: true,
            stale: false,
            next_attempt: None,
            passphrase_note: None,
            refused: false,
            history: Arc::default(),
            ring: Arc::default(),
        })),
        feed: watch::channel(Feed::default()).0,
        session: Some(session),
        last_state: DeviceState::default(),
        last_failure: None,
        note: None,
        failures: 0,
        farewell: None,
        episode: Episode::default(),
    }
}

/// The echo of a `set` the device answered shows the code works and takes the note away; a
/// status the device volunteers does not, and a verdict that has not changed is not said again.
#[tokio::test]
async fn the_echo_of_a_waited_set_clears_the_note_and_a_volunteered_status_does_not() {
    let mut actor = unproven_actor().await;
    assert!(actor.judge_passphrase(), "the first verdict is news");
    assert_eq!(actor.note, Some(PassphraseNote::Differs));
    assert!(
        !actor.judge_passphrase(),
        "the same verdict is not said twice"
    );

    actor.publish_status(&Message::status(FunctionId::one(10), Value::U8(100)));
    actor.judge_passphrase();
    assert_eq!(actor.note, Some(PassphraseNote::Differs));

    let answered = actor
        .run_command(Command::Raw {
            id: "10".into(),
            value_type: "u8".into(),
            value: "90".into(),
            no_wait: false,
        })
        .await;
    assert!(matches!(answered, Response::Value(_)), "got {answered:?}");
    assert_eq!(actor.note, None);
    assert_eq!(actor.info.lock().unwrap().passphrase_note, None);
}

/// After a `passphrase` change the held advertisement says the new code, which the device will
/// advertise next: an entry that names the new digits gets no note, and `auto` keeps the new code
/// rather than re-keying the live session to the old one.
#[tokio::test]
async fn a_passphrase_change_updates_the_held_advertisement() {
    let mut actor = connected_actor(Arc::new(svolder_ble::FakeTransport::from_profile(
        Registry::builtin().unwrap().get("gungnir2-v2").unwrap(),
        "0102030405060708".parse().unwrap(),
        3,
        Passphrase::from_text("1234").unwrap(),
    )))
    .await;
    let changed = actor.passphrase("5678").await;
    assert!(matches!(changed, Response::Ack), "got {changed:?}");

    let answered = actor.configure(fake_config("5678"));
    assert!(matches!(answered, Response::Ack), "got {answered:?}");
    assert_eq!(
        actor.note, None,
        "the entry names the code the device now holds"
    );

    let answered = actor.configure(fake_config("auto"));
    assert!(matches!(answered, Response::Ack), "got {answered:?}");
    assert_eq!(actor.note, None);
    let set = actor
        .run_command(Command::Set {
            key: "input".into(),
            value: "4".into(),
            step: 1,
            no_wait: false,
        })
        .await;
    assert!(
        matches!(set, Response::Value(_)),
        "auto re-keyed the session to the old code: {set:?}"
    );
}

/// A refusal that arrives on a link the connect's burst had shown to work ends that proof: the
/// actor judges after each status it reads, so the note comes back with the stale verdict.
#[tokio::test]
async fn a_refusal_after_a_proof_brings_the_note_back() {
    let transport = Shows::new();
    transport.show("4321");
    let handle = spawned_on(&transport, "1234").await;
    assert_eq!(
        handle.describe().passphrase_note,
        None,
        "proven by the burst"
    );
    transport.inner.push(&[Message::status(
        svolder_proto::function::PASSPHRASE_REJECTED,
        Value::Bool(true),
    )]);
    tokio::time::timeout(Duration::from_secs(5), async {
        while handle.describe().passphrase_note.is_none() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the refusal did not bring the note back");
    assert_eq!(
        handle.describe().passphrase_note,
        Some(PassphraseNote::Differs)
    );
}

/// A waited `set` the device refuses, on a link a connect's burst had shown to work, judges at
/// once: the refusal is consumed by the `set`, so `serve_connected` never reads it as a status.
#[tokio::test]
async fn a_refused_waited_set_brings_the_note_back() {
    let registry = Arc::new(Registry::builtin().unwrap());
    let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let fake = Arc::new(svolder_ble::FakeTransport::from_profile(
        &profile,
        unit,
        3,
        Passphrase::from_text("1234").expect("four digits"),
    ));
    let transport = Arc::clone(&fake) as Arc<dyn Transport>;
    let mut actor = connected_actor(Arc::clone(&transport)).await;
    // Over the fake this test can push to, with the session proven on 1234.
    let advert = fake
        .scan(Duration::from_millis(1))
        .await
        .expect("the fake always scans")
        .next()
        .await
        .expect("the fake advertises itself");
    let mut session = Session::new(
        transport,
        advert.clone(),
        profile,
        Passphrase::from_text("1234").expect("four digits"),
    );
    session.connect().await.expect("the fake connects");
    actor.session = Some(session);
    let mut held = advert;
    held.manufacturer.encrypted_passphrase = Passphrase::encrypt(
        Passphrase::from_text("4321").unwrap().encoded(),
        Passphrase::advert_key(unit),
    );
    actor.advert = Some(held);
    actor.judge_passphrase();
    assert_eq!(actor.note, None, "proven by the burst");

    fake.push(&[Message::status(
        svolder_proto::function::PASSPHRASE_REJECTED,
        Value::Bool(true),
    )]);
    let answered = actor
        .run_command(Command::Raw {
            id: "10".into(),
            value_type: "u8".into(),
            value: "90".into(),
            no_wait: false,
        })
        .await;
    assert!(
        matches!(answered, Response::Error { code: 3, .. }),
        "got {answered:?}"
    );
    assert_eq!(actor.note, Some(PassphraseNote::Differs));
}

/// What the actors share to scan with, over `transport`.
fn scan_for(transport: &Arc<dyn Transport>) -> Arc<Scanner> {
    Arc::new(Scanner::new(Arc::clone(transport)))
}

/// Two devices that are both away, on an adapter whose scans take their whole window.
struct Away;

#[async_trait::async_trait]
impl Transport for Away {
    async fn scan(&self, window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        tokio::time::sleep(window).await;
        Ok(futures::stream::empty().boxed())
    }

    async fn connect(&self, _unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        Err(BleError::NotFound)
    }
}

/// An entry named `name` for the device `udid`, on `auto`.
fn entry(name: &str, udid: &str) -> DeviceConfig {
    DeviceConfig {
        name: name.into(),
        udid: udid.into(),
        profile: None,
        passphrase: "auto".into(),
        inputs: BTreeMap::new(),
    }
}

/// Two devices that are away are looked for in one listen: one scan window after they start,
/// both actors are waiting out their first retry, not one of them still scanning for its own.
#[tokio::test(start_paused = true)]
async fn two_missing_devices_are_looked_for_in_one_window() {
    let transport = Arc::new(Away) as Arc<dyn Transport>;
    let scan = scan_for(&transport);
    let handles = [("amp", "0102030405060708"), ("dac", "0807060504030201")].map(|(name, udid)| {
        spawn(
            Arc::clone(&transport),
            Arc::new(Registry::builtin().unwrap()),
            entry(name, udid),
            Arc::clone(&scan),
        )
        .expect("the actor starts")
    });
    // One scan window, five seconds, and a millisecond.
    tokio::time::sleep(Duration::from_millis(5_001)).await;
    for handle in &handles {
        assert!(
            handle.describe().retry_in_secs.is_some(),
            "{} was still being looked for one window in",
            handle.name
        );
    }
}

/// A device on the air, beside one that is away, on an adapter that hears each device the
/// moment it advertises.
struct Fleet {
    here: svolder_ble::FakeTransport,
    listens: std::sync::atomic::AtomicUsize,
}

impl Fleet {
    fn new() -> Fleet {
        let registry = Registry::builtin().expect("the built-in profiles load");
        let profile = registry.get("gungnir2-v2").expect("a built-in").clone();
        Fleet {
            here: svolder_ble::FakeTransport::from_profile(
                &profile,
                "0102030405060708".parse().expect("a valid unit id"),
                3,
                Passphrase::from_text("1234").expect("four digits"),
            ),
            listens: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl Transport for Fleet {
    async fn scan(&self, window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        self.here.scan(window).await
    }

    async fn listen(
        &self,
        window: Duration,
        listening: &dyn svolder_ble::Listening,
    ) -> Result<(), BleError> {
        self.listens.fetch_add(1, Ordering::SeqCst);
        let deadline = tokio::time::Instant::now() + window;
        let advert = self.here.advertisement();
        loop {
            if listening.wants(advert.unit_id()) {
                listening.heard(&advert);
            }
            if listening.done() || tokio::time::Instant::now() >= deadline {
                return Ok(());
            }
            let next = tokio::time::Instant::now() + Duration::from_millis(100);
            tokio::time::sleep_until(next.min(deadline)).await;
        }
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        self.here.connect(unit).await
    }
}

/// The device on the air is reached at once, while the listen goes on for the one that is
/// away: it is not held behind the other's window.
#[tokio::test(start_paused = true)]
async fn a_device_on_the_air_is_reached_while_another_is_looked_for() {
    let fleet = Arc::new(Fleet::new());
    let transport = Arc::clone(&fleet) as Arc<dyn Transport>;
    let scan = scan_for(&transport);
    let away = spawn(
        Arc::clone(&transport),
        Arc::new(Registry::builtin().unwrap()),
        entry("away", "0807060504030201"),
        Arc::clone(&scan),
    )
    .expect("the actor starts");
    let here = spawn(
        Arc::clone(&transport),
        Arc::new(Registry::builtin().unwrap()),
        entry("here", "0102030405060708"),
        Arc::clone(&scan),
    )
    .expect("the actor starts");
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(
        here.describe().connected,
        "the device on the air waited for the one that is away"
    );
    let looking = away.describe();
    assert!(
        !looking.connected && looking.retry_in_secs.is_none(),
        "the listen for the device that is away ended early: {looking:?}"
    );
    assert_eq!(fleet.listens.load(Ordering::SeqCst), 1);
}

/// An actor stopped while its attempt is still looking for the device does not connect when the
/// sighting comes: the attempt hands the session back without opening a link.
#[tokio::test(start_paused = true)]
async fn an_actor_stopped_while_its_attempt_listens_connects_nothing() {
    let started = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let parked = Arc::new(ParkedFirstScan::new(&started, &release));
    let shared: Arc<dyn Transport> = Arc::clone(&parked) as Arc<dyn Transport>;
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");
    started.notified().await;

    let (tx, rx) = oneshot::channel();
    handle
        .commands
        .send((Command::Shutdown, tx))
        .await
        .map_err(|_| ())
        .expect("the actor is running");
    // Let the actor raise the stop, then let the listen end within the close grace.
    tokio::time::sleep(Duration::from_millis(100)).await;
    release.notify_one();
    let farewell = tokio::time::timeout(Duration::from_secs(60), rx)
        .await
        .expect("the actor answered the shutdown")
        .expect("the actor did not drop the reply");
    assert!(matches!(farewell, Response::Ack), "got {farewell:?}");
    assert_eq!(
        parked.inner.connects(),
        0,
        "a stopped attempt opened a link"
    );
}

mod refusal;

/// A step from a reading the device never reported is refused with code 4, naming the key: the
/// control can be stepped, there is only nothing to step from.
#[tokio::test]
async fn a_step_with_no_reading_to_step_from_is_refused_with_code_4() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut actor = connected_actor(Arc::new(svolder_ble::FakeTransport::from_profile(
            Registry::builtin().unwrap().get("gungnir2-v2").unwrap(),
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::from_text("1234").unwrap(),
        )))
        .await;
        // The profile names a level control at a function the device never reports.
        let source = "schema = 1\nname = \"trimmed\"\n\
            [[control]]\nid = [99]\nkey = \"trim\"\nlabel = \"Trim\"\nkind = \"level\"\n\
            range = [0, 10]\n";
        actor.profile = Registry::from_sources(vec![("trimmed".into(), source.into())])
            .expect("the test profile loads")
            .get("trimmed")
            .expect("the test profile")
            .clone();
        for value in ["up", "down"] {
            let refused = actor
                .run_command(Command::Set {
                    key: "trim".into(),
                    value: value.into(),
                    step: 1,
                    no_wait: false,
                })
                .await;
            assert!(
                matches!(&refused, Response::Error { code: 4, message } if message == "no current value for trim"),
                "{value}: got {refused:?}"
            );
        }
    })
    .await
    .expect("test timed out");
}

/// The daemon answers a readout an `only_while` gates as the CLI shows it: false while its gate
/// reads false, true once the gate reads true, the raw value as the device sent it.
#[tokio::test]
async fn a_gated_readout_is_answered_false_while_its_gate_reads_false() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let transport = Arc::new(svolder_ble::FakeTransport::from_profile(
            Registry::builtin().unwrap().get("tyr").unwrap(),
            "0102030405060708".parse().unwrap(),
            3,
            Passphrase::NONE,
        ));
        let mut actor = connected_actor_on("tyr", transport).await;
        let warming = actor
            .profile
            .control_by_key("warming_up")
            .expect("a gated readout")
            .clone();
        let warm = Message::status(warming.id, Value::Bool(true));
        assert_eq!(actor.value_object(&warm).display, "false", "the amp is off");
        let answer = actor
            .run_command(Command::Set {
                key: "power".into(),
                value: "on".into(),
                step: 1,
                no_wait: false,
            })
            .await;
        assert!(matches!(answer, Response::Value(_)), "{answer:?}");
        let shown = actor.value_object(&warm);
        assert_eq!(shown.display, "true");
        assert_eq!(shown.raw, serde_json::json!(true));
    })
    .await
    .expect("test timed out");
}

/// The next `climb` event on `events`, as its key and target.
async fn next_climb(events: &mut broadcast::Receiver<Event>) -> (String, Option<i64>) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Event::Climb { key, target, .. }) = events.recv().await {
                return (key, target);
            }
        }
    })
    .await
    .expect("a climb event")
}

/// A recall through the writer pushes `recalled` on the device's events, notes it in the ring
/// and shows in the device's row; a climb pushes `climb` as it starts, as it is steered and as
/// it ends.
#[tokio::test]
async fn the_memory_and_the_climb_are_pushed_as_events() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let transport = Shows::new();
        let handle = spawned_on(&transport, "1234").await;
        let mut events = handle.events.subscribe();
        let eq = handle
            .feed
            .borrow()
            .profile
            .as_ref()
            .and_then(|p| p.eq.clone())
            .expect("a tone control");
        let fitting: Vec<svolder_profiles::tone::preset::Preset> =
            svolder_profiles::tone::preset::fitting(&eq, &[])
                .into_iter()
                .map(|(p, _)| p)
                .collect();
        let crate::writer::Recalling { done, .. } =
            handle
                .writer
                .recall(crate::writer::Origin::Mcp, "Warm".into(), fitting);
        done.await.expect("the recall ended");
        let said = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(Event::Recalled { device, name }) = events.recv().await {
                    return (device, name);
                }
            }
        })
        .await
        .expect("a recalled event");
        assert_eq!(said.1, "Warm");
        assert_eq!(handle.describe().recalled.as_deref(), Some("Warm"));
        let Response::Readings { recalls, .. } = handle.readings(60) else {
            panic!("not a readings answer");
        };
        assert_eq!(recalls.last().map(|r| r.name.as_str()), Some("Warm"));
        let _ = handle
            .writer
            .glide(
                crate::writer::Origin::Mqtt,
                "volume".into(),
                127,
                crate::writer::Mode::Latest,
            )
            .await;
        assert_eq!(
            next_climb(&mut events).await,
            ("volume".to_string(), Some(127))
        );
        let _ = handle
            .writer
            .glide(
                crate::writer::Origin::Mqtt,
                "volume".into(),
                126,
                crate::writer::Mode::Latest,
            )
            .await;
        assert_eq!(
            next_climb(&mut events).await,
            ("volume".to_string(), Some(126))
        );
        let _ = handle
            .writer
            .submit(
                crate::writer::Origin::Socket,
                Command::Set {
                    key: "volume".into(),
                    value: "60".into(),
                    step: 1,
                    no_wait: false,
                },
                crate::writer::Mode::Ordered,
            )
            .await;
        assert_eq!(next_climb(&mut events).await, ("volume".to_string(), None));
    })
    .await
    .expect("test timed out");
}

/// A span past the ring's half hour is cut to it: the link changes the answer carries are those
/// inside 1,800 s and the one before them, never the older ones.
#[tokio::test]
async fn a_readings_span_past_the_ring_is_cut_to_its_half_hour() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let transport = Shows::new();
        let handle = spawned_on(&transport, "1234").await;
        let now = history::wall_ms();
        history::TEST_CLOCK.with(|clock| clock.set(Some(now)));
        let link = Arc::clone(&locked(&handle.info).history);
        *locked(&link) = history::History::default();
        for (ago_s, up) in [(3000, true), (1900, false), (100, true)] {
            locked(&link).record(now - ago_s * 1000, up);
        }
        let Response::Readings { link, bin_ms, .. } = handle.readings(u64::MAX) else {
            panic!("not a readings answer");
        };
        assert_eq!(bin_ms, 1000);
        let ago: Vec<u64> = link.iter().map(|c| (now - c.at_ms) / 1000).collect();
        assert_eq!(ago, [1900, 100]);
        history::TEST_CLOCK.with(|clock| clock.set(None));
    })
    .await
    .expect("test timed out");
}

/// A readings span with nothing reported in it still says what held at its start: the series
/// carry their newest sample from before the span, and the link its last change.
#[test]
fn a_readings_span_carries_what_held_at_its_start() {
    let handle = bare_handle("dac", "UNIT", None);
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("gungnir2-v2")
        .expect("a built-in")
        .clone();
    let volume = profile.control_by_key("volume").expect("a volume").id;
    let now = std::time::Instant::now();
    let ring = Arc::clone(&locked(&handle.info).ring);
    locked(&ring).record(
        &profile,
        &svolder_proto::Message::status(volume, svolder_proto::Value::U8(50)),
        now - Duration::from_secs(10),
    );
    let link = Arc::clone(&locked(&handle.info).history);
    locked(&link).record(history::wall_ms() - 10_000, false);
    let Response::Readings { series, link, .. } = handle.readings(1) else {
        panic!("not a readings answer");
    };
    let [volume] = series.as_slice() else {
        panic!("expected the volume alone: {series:?}");
    };
    assert_eq!(volume.points.len(), 1, "{volume:?}");
    assert_eq!(volume.points[0].1, 50.0);
    let [change] = link.as_slice() else {
        panic!("expected the change before the span: {link:?}");
    };
    assert!(!change.connected);
}

/// Every point of the series for `key` in the daemon's readings answer, oldest first.
fn points_of(handle: &DeviceHandle, key: &str) -> Vec<f64> {
    let profile = handle.feed.borrow().profile.clone().expect("a profile");
    let id = profile
        .control_by_key(key)
        .expect("a control")
        .id
        .to_string();
    let Response::Readings { series, .. } = handle.readings(600) else {
        panic!("not a readings answer");
    };
    series
        .into_iter()
        .find(|s| s.id == id)
        .map(|s| s.points.into_iter().map(|(_, v)| v).collect())
        .unwrap_or_default()
}

/// Every point of the volume series in the daemon's readings answer.
fn volume_points(handle: &DeviceHandle) -> Vec<f64> {
    points_of(handle, "volume")
}

/// The number of the feed's readings that are numbers (a bool counts as one).
fn numeric_readings(handle: &DeviceHandle) -> usize {
    handle
        .feed
        .borrow()
        .readings
        .iter()
        .filter(|m| matches!(m.value, svolder_proto::Value::Bool(_)) || m.value.as_i64().is_some())
        .count()
}

/// A connect's own refresh reaches the readings: a device that connects and reports nothing
/// more has its volume, and every other number the session holds, in the answer.
#[tokio::test]
async fn a_connect_records_what_the_refresh_read() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let transport = Shows::new();
        let handle = spawned_on(&transport, "1234").await;
        let Response::Readings { series, .. } = handle.readings(600) else {
            panic!("not a readings answer");
        };
        let held = numeric_readings(&handle);
        assert!(held > 1, "the fake holds {held} numbers");
        assert!(!volume_points(&handle).is_empty(), "no volume: {series:?}");
        assert_eq!(series.len(), held, "a series for each number held");
    })
    .await
    .expect("test timed out");
}

/// Sever the link and wait for the actor to connect again.
async fn reconnected(transport: &Shows, handle: &DeviceHandle) {
    let before = transport.inner.connects();
    transport.inner.sever();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if transport.inner.connects() > before && handle.describe().connected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the actor never reconnected");
    // The actor records the connect's readings before it marks the link up, so the link
    // being up means they are in the ring.
}

/// A reconnect with the values unchanged records them again at the volume held, and one that
/// found the volume changed while the link was down records the new value after the old.
#[tokio::test]
async fn a_reconnect_records_what_the_refresh_read() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let transport = Shows::new();
        let handle = spawned_on(&transport, "1234").await;
        let held = *volume_points(&handle).last().expect("the connect's volume");
        reconnected(&transport, &handle).await;
        let after = volume_points(&handle);
        assert_eq!(after.last(), Some(&held), "{after:?}");
        assert!(after.iter().all(|v| *v == held), "{after:?}");

        let profile = handle.feed.borrow().profile.clone().expect("a profile");
        let id = profile.control_by_key("volume").expect("a volume").id;
        locked(&transport.inner.device()).set_raw(id, svolder_proto::Value::U8(held as u8 + 3));
        reconnected(&transport, &handle).await;
        let changed = volume_points(&handle);
        assert_eq!(changed.last(), Some(&(held + 3.0)), "{changed:?}");
        assert_eq!(changed.iter().filter(|v| **v == held + 3.0).count(), 1);
    })
    .await
    .expect("test timed out");
}

/// The strip's marks for the readings the daemon answers now.
fn marks_now(handle: &DeviceHandle) -> Vec<(u64, String)> {
    let profile = handle.feed.borrow().profile.clone().expect("a profile");
    let Response::Readings {
        now_ms,
        series,
        recalls,
        link,
        ..
    } = handle.readings(600)
    else {
        panic!("not a readings answer");
    };
    let past = crate::web::view::Past {
        now_ms,
        series: &series,
        recalls: &recalls,
        link: &link,
    };
    crate::web::view::strip(&profile, &past, 600)
        .marks
        .into_iter()
        .map(|m| (m.t, m.text))
        .collect()
}

/// A reconnect that finds the sample rate changed while the link was down draws exactly one
/// mark, in the second of the reconnect; an input that is as it was draws none.
#[tokio::test]
async fn a_change_while_away_marks_the_reconnect_once() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let transport = Shows::new();
        let handle = spawned_on(&transport, "1234").await;
        assert!(marks_now(&handle).is_empty(), "{:?}", marks_now(&handle));
        let held = *points_of(&handle, "sample_rate").last().expect("a rate");
        let profile = handle.feed.borrow().profile.clone().expect("a profile");
        let id = profile.control_by_key("sample_rate").expect("a rate").id;
        locked(&transport.inner.device()).set_raw(id, svolder_proto::Value::U8(held as u8 + 1));
        // The connect's second is over, so the reconnect's reading is a bin of its own.
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let before = history::wall_ms() / 1000;
        reconnected(&transport, &handle).await;
        let after = history::wall_ms() / 1000;
        // Let the reconnect's second end, so the strip covers it.
        tokio::time::sleep(Duration::from_millis(1100)).await;
        let marks = marks_now(&handle);
        let [(t, _)] = marks.as_slice() else {
            panic!("expected one mark: {marks:?}");
        };
        assert!(
            (before..=after).contains(t),
            "{t} outside {before}..={after}: {marks:?}"
        );
    })
    .await
    .expect("test timed out");
}

/// Identify during a wait is answered not connected and leaves the schedule alone: it is not a
/// command that needs the link, so it does not end the wait.
#[tokio::test(start_paused = true)]
async fn identify_during_the_wait_leaves_the_schedule_alone() {
    let (transport, mut seen, _reachable) = CountedScan::new();
    let shared: Arc<dyn Transport> = Arc::new(transport);
    let handle = spawn(
        Arc::clone(&shared),
        Arc::new(Registry::builtin().unwrap()),
        fake_config("auto"),
        scan_for(&shared),
    )
    .expect("the actor starts");
    let mut last = next_attempt(&mut seen).await;
    for _ in 0..4 {
        last = next_attempt(&mut seen).await;
    }
    settle_into_the_wait().await;

    let answer = ask(&handle, Command::Identify).await;
    assert!(
        matches!(&answer, Response::Error { code: 3, message } if message == "device not connected"),
        "got {answer:?}"
    );
    assert_eq!(
        next_attempt(&mut seen).await - last,
        RETRY_SCHEDULE[RETRY_SCHEDULE.len() - 1],
        "identify moved the next attempt"
    );
}

/// Identify on a linked actor sends one frame and answers sent; a second inside the gap answers
/// just sent.
#[tokio::test(start_paused = true)]
async fn identify_on_a_linked_actor_answers_sent_then_just_sent() {
    let transport = Arc::new(Away) as Arc<dyn Transport>;
    let mut actor = connected_actor(transport).await;
    for sent in [true, false] {
        assert_eq!(
            actor.run_command(Command::Identify).await,
            Response::Identify {
                device: "fake".into(),
                sent
            }
        );
    }
}

/// A device whose profile offers no identify answers a usage refusal that says so, on a linked
/// actor and on one with no link alike, and sends nothing.
#[tokio::test(start_paused = true)]
async fn identify_on_a_device_whose_profile_offers_none_is_refused_in_words() {
    let transport = Arc::new(Away) as Arc<dyn Transport>;
    let mut actor = connected_actor(transport).await;
    actor.profile.identify = false;
    for linked in [true, false] {
        if !linked {
            actor.session = None;
        }
        let answer = actor.run_command(Command::Identify).await;
        assert!(
            matches!(&answer, Response::Error { code: 1, message } if message == "fake offers no identify"),
            "linked {linked}: {answer:?}"
        );
    }
}

/// An actor over a fake gungnir2 link, and a handle that reads what the actor publishes.
async fn actor_and_handle() -> (Actor, DeviceHandle) {
    let transport = Arc::new(Away) as Arc<dyn Transport>;
    let actor = connected_actor(transport).await;
    let handle = DeviceHandle {
        name: "fake".into(),
        udid: "0102030405060708".into(),
        commands: mpsc::channel(1).0,
        writer: crate::writer::Writer::unattended(),
        events: actor.events.clone(),
        info: Arc::clone(&actor.info),
        feed: actor.feed.subscribe(),
    };
    (actor, handle)
}

/// The strength an advertisement carries becomes the device's signal, as an advert's, stamped with
/// the wall clock of the moment the daemon heard it.
#[tokio::test]
async fn advert_rssi_becomes_an_advert_signal() {
    tokio::time::timeout(Duration::from_secs(30), async {
        clock_at(1_700_000_000_000);
        let handle = spawned_on(&Shows::new(), "1234").await;
        assert_eq!(
            handle.describe().signal,
            Some(svolder_ipc::Signal {
                dbm: -50,
                source: svolder_ipc::SignalSource::Advert,
                heard_at_ms: 1_700_000_000_000,
            })
        );
    })
    .await
    .expect("test timed out");
}

/// A device never heard with a strength has no signal, an advert that carries none leaves the one
/// held alone, and a fresh one replaces it.
#[tokio::test(start_paused = true)]
async fn an_advert_without_a_strength_leaves_the_held_signal_alone() {
    let (actor, handle) = actor_and_handle().await;
    let mut advert = actor.advert.clone().expect("the actor holds an advert");
    advert.rssi = None;
    clock_at(1_000);
    actor.update_identity(&advert, Some(history::wall_ms()));
    assert_eq!(handle.describe().signal, None);

    advert.rssi = Some(-70);
    actor.update_identity(&advert, Some(history::wall_ms()));
    clock_at(2_000);
    advert.rssi = None;
    actor.update_identity(&advert, Some(history::wall_ms()));
    assert_eq!(
        handle.describe().signal.map(|s| (s.dbm, s.heard_at_ms)),
        Some((-70, 1_000))
    );

    advert.rssi = Some(-45);
    actor.update_identity(&advert, Some(history::wall_ms()));
    assert_eq!(
        handle.describe().signal.map(|s| (s.dbm, s.heard_at_ms)),
        Some((-45, 2_000))
    );
}

/// The bridge reads the signal through the feed: it carries what `devices` says, and keeps it
/// when the link goes.
#[tokio::test(start_paused = true)]
async fn the_feed_carries_the_signal_the_row_does() {
    let (actor, handle) = actor_and_handle().await;
    assert_eq!(handle.feed.borrow().signal, None);
    let mut advert = actor.advert.clone().expect("the actor holds an advert");
    advert.rssi = Some(-62);
    clock_at(5_000_000);
    actor.update_identity(&advert, Some(history::wall_ms()));
    let heard = handle.feed.borrow().signal;
    assert_eq!(
        heard.map(|s| (s.dbm, s.source, s.heard_at_ms)),
        Some((-62, svolder_ipc::SignalSource::Advert, 5_000_000))
    );
    assert_eq!(heard, handle.describe().signal);
    actor.set_connected(true);
    actor.set_connected(false);
    assert_eq!(handle.feed.borrow().signal, heard);
}

/// What the actor last heard about the signal is what it keeps while the link is gone: the same
/// strength, source and stamp, however long the device stays away.
#[tokio::test(start_paused = true)]
async fn signal_keeps_source_and_stamp_after_disconnect() {
    let (actor, handle) = actor_and_handle().await;
    let mut advert = actor.advert.clone().expect("the actor holds an advert");
    advert.rssi = Some(-62);
    clock_at(5_000_000);
    actor.update_identity(&advert, Some(history::wall_ms()));
    actor.set_connected(true);
    let heard = handle.describe().signal;
    assert!(heard.is_some());

    clock_at(5_060_000);
    actor.set_connected(false);
    clock_at(9_000_000);
    let kept = handle.describe();
    assert!(!kept.connected);
    assert_eq!(kept.signal, heard);
    assert_eq!(
        kept.signal.map(|s| (s.dbm, s.source, s.heard_at_ms)),
        Some((-62, svolder_ipc::SignalSource::Advert, 5_000_000))
    );
}

/// The stamp of each reading is the moment it was accepted: a connect's burst all at the connect,
/// a later status or a set's echo at its own moment, and a reading kept after the link goes keeps
/// its stamp. Every way the daemon builds a reading carries it.
#[tokio::test(start_paused = true)]
async fn readings_carry_when_they_were_heard() {
    let (mut actor, _handle) = actor_and_handle().await;
    let stamps = |response: Response| -> BTreeMap<String, Option<u64>> {
        let Response::Values { values, .. } = response else {
            panic!("not a list of readings: {response:?}");
        };
        values.into_iter().map(|v| (v.id, v.heard_at_ms)).collect()
    };

    clock_at(1_000_000);
    actor.record_state();
    // The firmware versions the connect asked for come out of the session after its burst, and
    // are stamped as they are published.
    for _ in 0..actor.profile.firmware_subsystems.len() {
        let answer = actor.session.as_mut().expect("a session").recv().await;
        actor.publish_status(&answer.expect("a firmware answer"));
    }
    let burst = stamps(actor.state_values());
    assert!(burst.len() > 1, "{burst:?}");
    assert!(burst.values().all(|t| *t == Some(1_000_000)), "{burst:?}");

    clock_at(1_005_000);
    let volume = actor.profile.control_by_key("volume").expect("a volume").id;
    let answer = actor
        .run_command(Command::Set {
            key: "volume".into(),
            value: "up".into(),
            step: 1,
            no_wait: false,
        })
        .await;
    let Response::Value(echo) = answer else {
        panic!("not an echo: {answer:?}");
    };
    assert_eq!(echo.heard_at_ms, Some(1_005_000), "the set's echo");

    let after = stamps(actor.state_values());
    assert_eq!(after[&volume.to_string()], Some(1_005_000), "{after:?}");
    let others: Vec<_> = after
        .iter()
        .filter(|(id, _)| **id != volume.to_string())
        .collect();
    assert!(!others.is_empty());
    assert!(
        others.iter().all(|(_, t)| **t == Some(1_000_000)),
        "{after:?}"
    );

    // The link goes and a long time passes: nothing is re-stamped.
    clock_at(1_065_000);
    actor.set_connected(false);
    clock_at(2_000_000);
    assert_eq!(stamps(actor.state_values()), after);
    let Response::Value(got) = actor.get("volume") else {
        panic!("no volume");
    };
    assert_eq!(got.heard_at_ms, Some(1_005_000), "get");
}

/// A status the device volunteers is published stamped with the moment it arrived, and the
/// stamp is the one a later read answers with.
#[tokio::test(start_paused = true)]
async fn a_volunteered_status_is_stamped_when_it_arrives() {
    let (actor, _handle) = actor_and_handle().await;
    let mut events = actor.events.subscribe();
    let volume = actor.profile.control_by_key("volume").expect("a volume").id;
    clock_at(3_000_000);
    actor.publish_status(&Message::status(volume, Value::U8(100)));
    let Ok(Event::Status { reading, .. }) = events.try_recv() else {
        panic!("no status event");
    };
    assert_eq!(reading.heard_at_ms, Some(3_000_000));
    clock_at(3_010_000);
    assert_eq!(
        actor
            .value_object(&Message::status(volume, Value::U8(100)))
            .heard_at_ms,
        Some(3_000_000)
    );
}

/// A listen hands an advertisement over with the moment it was heard, and that moment, not the
/// later one the actor takes the advertisement in, is the signal's stamp.
#[tokio::test(start_paused = true)]
async fn a_signal_is_stamped_when_the_listen_heard_it() {
    let transport = Shows::new() as Arc<dyn Transport>;
    let unit: UnitId = "0102030405060708".parse().unwrap();
    clock_at(7_000);
    let (advert, heard_at_ms) = discover(&transport, &scan_for(&transport), unit, None)
        .await
        .expect("the fake is heard");
    assert_eq!(heard_at_ms, Some(7_000));

    let (actor, handle) = actor_and_handle().await;
    clock_at(9_000);
    actor.update_identity(&advert, heard_at_ms);
    assert_eq!(
        handle.describe().signal.map(|s| (s.dbm, s.heard_at_ms)),
        Some((-50, 7_000))
    );
}

/// An advertisement the adapter held, not one a listen heard, carries a cached strength that is
/// no hearing: it leaves the signal held as it was, or none.
#[tokio::test(start_paused = true)]
async fn a_held_advert_leaves_the_held_signal_untouched() {
    let (actor, handle) = actor_and_handle().await;
    let mut advert = actor.advert.clone().expect("the actor holds an advert");
    advert.rssi = Some(-40);
    clock_at(1_000);
    actor.update_identity(&advert, None);
    assert_eq!(handle.describe().signal, None);

    advert.rssi = Some(-70);
    actor.update_identity(&advert, Some(1_000));
    advert.rssi = Some(-40);
    clock_at(2_000);
    actor.update_identity(&advert, None);
    assert_eq!(
        handle.describe().signal.map(|s| (s.dbm, s.heard_at_ms)),
        Some((-70, 1_000))
    );
}

/// A transport whose listens hear nothing and whose adapter holds the device with a cached
/// strength of -40 dBm.
struct HeldOnly(Arc<Shows>);

#[async_trait::async_trait]
impl Transport for HeldOnly {
    async fn scan(&self, window: Duration) -> Result<svolder_ble::Advertisements, BleError> {
        tokio::time::sleep(window).await;
        Ok(futures::stream::empty().boxed())
    }

    async fn held(&self) -> Result<svolder_ble::Advertisements, BleError> {
        let heard = self.0.held().await?;
        Ok(heard
            .map(|mut advert| {
                advert.rssi = Some(-40);
                advert
            })
            .boxed())
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn svolder_ble::Link>, BleError> {
        self.0.connect(unit).await
    }
}

/// A device the daemon reaches through the adapter's held list, having heard nothing of it, has
/// no signal: the cached strength is not shown as heard now.
#[tokio::test(start_paused = true)]
async fn a_device_reached_through_the_held_list_has_no_signal() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let shared = Arc::new(HeldOnly(Shows::new())) as Arc<dyn Transport>;
        let handle = spawn(
            Arc::clone(&shared),
            Arc::new(Registry::builtin().unwrap()),
            fake_config("1234"),
            scan_for(&shared),
        )
        .expect("the actor starts");
        // The listen hears nothing for its whole window before the held list is tried.
        while !handle.describe().connected {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let info = handle.describe();
        assert!(info.connected);
        assert_eq!(info.signal, None);
    })
    .await
    .expect("test timed out");
}

/// A connect stamps what the device reported on that link and nothing else: a reading kept from
/// before a disconnect that the burst did not repeat keeps the stamp it had.
#[tokio::test(start_paused = true)]
async fn a_connect_does_not_restamp_a_reading_the_burst_did_not_repeat() {
    let (mut actor, _handle) = actor_and_handle().await;
    let (a, b) = (FunctionId::one(10), FunctionId::one(11));
    let mut state = DeviceState::default();
    state.apply(&Message::status(a, Value::U8(1)));
    state.apply(&Message::status(b, Value::U8(2)));
    actor.session = None;
    actor.last_state = state.clone();
    clock_at(1_000);
    actor.record_state();
    let stamps = |actor: &Actor| actor.feed.borrow().heard_at.clone();
    assert_eq!(stamps(&actor), BTreeMap::from([(a, 1_000), (b, 1_000)]));

    // The link goes; the next one's burst repeats only `a`.
    state.set_stale(true);
    state.apply(&Message::status(a, Value::U8(1)));
    actor.last_state = state;
    clock_at(5_000);
    actor.record_state();
    assert_eq!(stamps(&actor), BTreeMap::from([(a, 5_000), (b, 1_000)]));
}

/// A recall sent as a raw command, waited or not, goes out and answers at once: an action has no
/// echo, and nothing of it is published or kept as a reading.
#[tokio::test(start_paused = true)]
async fn a_raw_recall_is_sent_without_an_echo_and_is_no_reading() {
    let transport = Arc::new(svolder_ble::FakeTransport::from_profile(
        Registry::builtin().unwrap().get("lokimax").unwrap(),
        "0102030405060708".parse().unwrap(),
        3,
        Passphrase::NONE,
    ));
    let mut actor = connected_actor_on("lokimax", transport).await;
    let mut events = actor.events.subscribe();
    let recall = FunctionId::three(7, 2, 2);
    let began = tokio::time::Instant::now();
    for (command, what) in [
        (
            Command::Raw {
                id: "7.2.2".into(),
                value_type: "data".into(),
                value: String::new(),
                no_wait: false,
            },
            "a waited raw command",
        ),
        (
            Command::Set {
                key: "recall".into(),
                value: "2".into(),
                step: 1,
                no_wait: false,
            },
            "a waited set",
        ),
    ] {
        let answered = actor.run_command(command).await;
        assert!(matches!(answered, Response::Ack), "{what}: {answered:?}");
    }
    assert!(
        began.elapsed() < Duration::from_secs(1),
        "no echo was waited for: {:?}",
        began.elapsed()
    );
    assert!(actor.state_of().get(recall).is_none());
    assert!(!actor.feed.borrow().heard_at.contains_key(&recall));
    while let Ok(event) = events.try_recv() {
        if let Event::Status { reading, .. } = event {
            assert_ne!(reading.id, "7.2.2", "an action is no status");
        }
    }
}

/// A Loki Max actor on a fake device the test keeps, with the transport the actor itself scans.
async fn loki_actor() -> (Actor, Arc<svolder_ble::FakeTransport>) {
    let registry = Registry::builtin().unwrap();
    let fake = Arc::new(svolder_ble::FakeTransport::from_profile(
        registry.get("lokimax").expect("lokimax"),
        "0102030405060708".parse().expect("a valid unit id"),
        3,
        Passphrase::from_text("1234").expect("four digits"),
    ));
    let transport = Arc::new(svolder_ble::FakeTransport::from_profile(
        registry.get("lokimax").expect("lokimax"),
        "0102030405060708".parse().expect("a valid unit id"),
        3,
        Passphrase::NONE,
    ));
    let actor = connected_actor_over("lokimax", Arc::clone(&fake), transport).await;
    (actor, fake)
}

/// How many frames the fake device has taken.
fn frames_taken(fake: &svolder_ble::FakeTransport) -> usize {
    fake.device().lock().expect("the device").commands_seen()
}

/// A `set` of a device slot's recall goes to the device as one frame and is answered at once: an
/// action has no echo to wait for, however the command asked.
#[tokio::test(start_paused = true)]
async fn set_recall_is_sent_without_waiting_for_an_echo() {
    let (mut actor, fake) = loki_actor().await;
    let before = frames_taken(&fake);
    let began = tokio::time::Instant::now();
    let answered = actor
        .run_command(Command::Set {
            key: "recall".into(),
            value: "Preset 2".into(),
            step: 1,
            no_wait: false,
        })
        .await;
    assert!(matches!(answered, Response::Ack), "{answered:?}");
    assert!(
        began.elapsed() < Duration::from_secs(1),
        "{:?}",
        began.elapsed()
    );
    assert_eq!(frames_taken(&fake), before + 1);
}

/// A read of an action is a usage error in the profile crate's words, which say it is sent and
/// how.
#[tokio::test(start_paused = true)]
async fn get_of_an_action_is_a_usage_error_in_the_profiles_words() {
    let (actor, _fake) = loki_actor().await;
    for key in ["recall", "save"] {
        let answered = actor.get(key);
        let Response::Error { code, message } = &answered else {
            panic!("{key}: {answered:?}");
        };
        assert_eq!(*code, 1, "{key}");
        assert_eq!(
            message,
            &format!("{key} is an action: it is sent, never read; svolder set {key} 1-3 sends it")
        );
    }
}

/// A slot the action does not have is refused in words, and no frame is sent for it.
#[tokio::test(start_paused = true)]
async fn set_recall_with_a_slot_past_the_actions_slots_is_refused_before_any_frame() {
    let (mut actor, fake) = loki_actor().await;
    let before = frames_taken(&fake);
    for slot in ["4", "0", "Preset 4", "up"] {
        let answered = actor
            .run_command(Command::Set {
                key: "recall".into(),
                value: slot.into(),
                step: 1,
                no_wait: false,
            })
            .await;
        let Response::Error { code, message } = &answered else {
            panic!("{slot}: {answered:?}");
        };
        assert_eq!(*code, 1, "{slot}");
        assert!(message.contains("recall"), "{slot}: {message}");
    }
    assert_eq!(frames_taken(&fake), before, "nothing went to the device");
}
