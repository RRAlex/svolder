// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;

use super::*;
use svolder_ble::adapters::Better;
use svolder_ipc::CODE_NOT_FOUND;
use svolder_ipc::{CODE_LINK, CODE_TIMEOUT, CODE_USAGE};

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
            adapter_hint: None,
            hint_epoch: 0,
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
            adapter_hint: None,
            hint_epoch: 0,
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
        drop_due: false,
        farewell: None,
        episode: Episode::default(),
        first_attempt: None,
    }
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
            adapter_hint: None,
            hint_epoch: 0,
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
        drop_due: false,
        farewell: None,
        episode: Episode::default(),
        first_attempt: None,
    }
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

mod attempts;
mod changes;
mod events;
mod fleet;
mod identify;
mod notes;
mod readings;
mod recall;
mod refusal;
mod reload;
mod requests;
mod schedule;
mod signal;

/// What `describe` says of the hint after the scan task has run, waiting under a deadline.
async fn hint_of(actor: &Actor, wanted: bool) -> Option<svolder_ipc::AdapterHint> {
    let handle_info = Arc::clone(&actor.info);
    tokio::time::timeout(Duration::from_secs(5), async move {
        loop {
            let seen = locked(&handle_info).adapter_hint.clone();
            if seen.is_some() == wanted {
                return seen;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the hint settles")
}

fn louder_elsewhere() -> Better {
    Better {
        take: "AA:BB:CC:DD:EE:01".into(),
        name: "hci0".into(),
        rssi: Some(-74),
        own: Some(Some(-80)),
    }
}

/// A device that keeps failing to connect on one adapter while another hears it carries the
/// hint in its row, with the failure and the count; it goes when the streak does.
#[tokio::test]
async fn a_failing_streak_with_a_louder_adapter_sets_the_hint_in_the_row() {
    let mut actor = unproven_actor().await;
    actor.failures = 4;
    actor.hint_when("hci2".into(), "no connection within 20 s", async {
        Some(louder_elsewhere())
    });
    let hint = hint_of(&actor, true).await.expect("a hint");
    assert_eq!(
        (
            hint.failing.as_str(),
            hint.better.as_str(),
            hint.take.as_str(),
            hint.tries
        ),
        ("hci2", "hci0", "AA:BB:CC:DD:EE:01", 4)
    );
    assert_eq!((hint.own_heard, hint.own_dbm), (true, Some(-80)));
    assert_eq!(hint.better_dbm, Some(-74));
    // The same streak going on only moves the count and the words.
    actor.failures = 6;
    actor.refresh_hint("no connection within 20 s (again)");
    let again = hint_of(&actor, true).await.expect("still there");
    assert_eq!(
        (again.tries, again.failure.as_str()),
        (6, "no connection within 20 s (again)")
    );
    // The row the `devices` verb answers with carries it.
    let handle = bare_handle("fake", "0102030405060708", None);
    locked(&handle.info).adapter_hint = Some(again.clone());
    assert_eq!(handle.describe().adapter_hint, Some(again));
    actor.clear_hint();
    assert!(hint_of(&actor, false).await.is_none());
}

/// A scan that finishes after the streak ended sets nothing.
#[tokio::test]
async fn a_scan_that_outlives_its_streak_sets_no_hint() {
    let mut actor = unproven_actor().await;
    actor.failures = 3;
    let (release, parked) = tokio::sync::oneshot::channel::<()>();
    actor.hint_when("hci2".into(), "no connection within 20 s", async move {
        let _ = parked.await;
        Some(louder_elsewhere())
    });
    actor.clear_hint();
    release.send(()).expect("the scan task waits");
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    assert!(locked(&actor.info).adapter_hint.is_none());
}

/// The log's pointer and the row's hint come from the same scan: no louder adapter, no hint.
#[tokio::test]
async fn no_other_adapter_hearing_the_device_sets_no_hint() {
    let mut actor = unproven_actor().await;
    actor.failures = 3;
    actor.hint_when("hci2".into(), "no connection within 20 s", async { None });
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    assert!(locked(&actor.info).adapter_hint.is_none());
}
