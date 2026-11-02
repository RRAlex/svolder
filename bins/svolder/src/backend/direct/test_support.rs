// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The fake BLE transport a test severs and reopens a device's link through, and the direct
//! backends built over it. It sits beside `direct` so the backends it builds can fill in that
//! module's private fields.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize};

use futures::StreamExt;
use svolder_ble::{Advertisements, Link, Notifications, SessionConfig};

use super::*;

/// A transport over one fake device whose `connect` can be closed, so a test can sever
/// the link and leave a reconnect no way to succeed until it says otherwise.
pub(crate) struct FakeGate {
    inner: FakeTransport,
    connects: AtomicBool,
    /// How long each `connect` takes before it answers, in milliseconds. A real adapter
    /// spends seconds here; the fake spends none unless a case asks it to.
    connect_ms: AtomicU64,
    /// Whether a scan answers with the device. Turned off, the scan comes back empty
    /// while the transport still holds the device, which is what an adapter does when
    /// its window falls between the reports it makes.
    scans: AtomicBool,
    /// Whether a link this transport handed out has been disconnected, which is what
    /// closing a backend does and what releases the device for the next central.
    closed: Arc<AtomicBool>,
    /// How many times a caller has asked this transport to settle, which is what a
    /// backend does when it stops an attempt whose connect it cannot wait out.
    settles: AtomicUsize,
    /// The devices a caller asked this transport to settle for, in order.
    settled: Mutex<Vec<UnitId>>,
    /// How many `connect` futures are alive: one ends when it answers or when its task is
    /// aborted and dropped.
    live_connects: Arc<AtomicUsize>,
    /// What `live_connects` said when the last `settle_for` began.
    live_at_settle: Mutex<Option<usize>>,
    /// How many times a caller has listened on this transport, a scan or a sighting.
    listens: AtomicUsize,
    /// Whether a scan takes the whole window it is given before it answers, as an
    /// adapter's does, while a sighting ends the moment the device is heard. Off, both
    /// answer at once, as the fake does.
    windowed: AtomicBool,
    /// Whether a `connect` fails once a link has been released.
    fails_after_release: AtomicBool,
    /// Whether the links this transport hands out hear nothing from the device: it applies a
    /// write, and the echo never comes.
    deaf: Arc<AtomicBool>,
}

/// One live connect, counted from its entry until it is dropped.
struct Live(Arc<AtomicUsize>);

impl Live {
    fn enter(count: &Arc<AtomicUsize>) -> Live {
        count.fetch_add(1, Ordering::SeqCst);
        Live(Arc::clone(count))
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// One link over the gate's own transport, which records the disconnect that closes it.
struct GatedLink {
    inner: Box<dyn Link>,
    closed: Arc<AtomicBool>,
    deaf: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl Link for GatedLink {
    async fn write(&self, frame: &[u8]) -> Result<(), BleError> {
        self.inner.write(frame).await
    }

    fn notifications(&self) -> Notifications {
        let deaf = Arc::clone(&self.deaf);
        self.inner
            .notifications()
            .filter(move |_| futures::future::ready(!deaf.load(Ordering::SeqCst)))
            .boxed()
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        self.closed.store(true, Ordering::SeqCst);
        self.inner.disconnect().await
    }
}

impl FakeGate {
    /// A gate over a freshly seeded fake device, open.
    pub(super) fn new(profile: &Profile, unit: UnitId, passphrase: Passphrase) -> FakeGate {
        FakeGate {
            inner: FakeTransport::from_profile(profile, unit, 3, passphrase),
            connects: AtomicBool::new(true),
            connect_ms: AtomicU64::new(0),
            scans: AtomicBool::new(true),
            closed: Arc::new(AtomicBool::new(false)),
            settles: AtomicUsize::new(0),
            settled: Mutex::new(Vec::new()),
            live_connects: Arc::new(AtomicUsize::new(0)),
            live_at_settle: Mutex::new(None),
            listens: AtomicUsize::new(0),
            windowed: AtomicBool::new(false),
            fails_after_release: AtomicBool::new(false),
            deaf: Arc::new(AtomicBool::new(false)),
        }
    }

    /// From now on the device applies a write and the echo never reaches this transport's links,
    /// or reaches them again.
    pub(super) fn hear_nothing(&self, deaf: bool) {
        self.deaf.store(deaf, Ordering::SeqCst);
    }

    /// The device reports volume `raw` on its own, as a knob turned.
    pub(super) fn turn_knob(&self, raw: u8) {
        self.inner
            .push(&[Message::status(FunctionId::one(10), Value::U8(raw))]);
    }

    /// The advertisement the fake device broadcasts.
    pub(super) fn advertisement(&self) -> Advertisement {
        self.inner.advertisement()
    }

    /// Say what the advertisement carries as the device's code from now on.
    pub(crate) fn advertise(&self, how: svolder_ble::Advertised) {
        self.inner.advertise(how);
    }

    /// Have the device advertise nothing once a link to it has been released.
    pub(crate) fn go_quiet_after_release(&self) {
        self.inner.go_quiet_after_release();
    }

    /// Have the device advertise again after it went quiet.
    pub(crate) fn speak_again(&self) {
        self.inner.speak_again();
    }

    /// How many links have been opened on this transport.
    pub(crate) fn opened(&self) -> usize {
        self.inner.connects()
    }

    /// How many links a caller has released.
    pub(crate) fn releases(&self) -> usize {
        self.inner.releases()
    }

    /// Change the device's code to `to` the way the official app does, with a `250` command
    /// encoded under `from`, the code it holds now.
    pub(crate) fn change_code(&self, from: Passphrase, to: Passphrase) {
        let change = Message::command(PASSPHRASE, Value::U32(to.encoded()));
        let frame = svolder_proto::encode_command(&change, from).expect("a frame");
        self.inner
            .device()
            .lock()
            .expect("device mutex")
            .handle(&frame);
    }

    /// Cut the current link, the way a device dropping the connection would.
    pub(crate) fn sever(&self) {
        self.inner.sever();
    }

    /// Answer every scan from now on with nothing, the way an adapter whose window
    /// falls between a device's reports does, while still holding the device.
    pub(crate) fn quiet_scans(&self) {
        self.scans.store(false, Ordering::SeqCst);
    }

    /// Make every scan from now on take its whole window, as an adapter's does, and every
    /// sighting end the moment the device advertises — at once, since it always is.
    pub(crate) fn takes_windows(&self) {
        self.windowed.store(true, Ordering::SeqCst);
    }

    /// How many times a caller has listened on this transport, a scan or a sighting.
    pub(crate) fn listens(&self) -> usize {
        self.listens.load(Ordering::SeqCst)
    }

    /// Refuse every `connect` from now on: the device is out of reach.
    pub(crate) fn refuse_connects(&self) {
        self.connects.store(false, Ordering::SeqCst);
    }

    /// Refuse every `connect` once a link has been released: the device is not reachable
    /// again after a caller lets go of it.
    pub(crate) fn fail_connects_after_release(&self) {
        self.fails_after_release.store(true, Ordering::SeqCst);
    }

    /// Take `connect` calls again: the device is back in range.
    pub(crate) fn allow_connects(&self) {
        self.connects.store(true, Ordering::SeqCst);
    }

    /// Set one function on the device behind the transport, the way anything other than
    /// this process changing it would.
    pub(crate) fn set_raw(&self, id: FunctionId, value: Value) {
        self.inner
            .device()
            .lock()
            .expect("device mutex")
            .set_raw(id, value);
    }

    /// Report these statuses on the link, as the device would on its own.
    pub(crate) fn push(&self, messages: &[svolder_proto::Message]) {
        self.inner.push(messages);
    }

    /// Play a signal on the device's meters, one notification every `period` while a link is up.
    pub(crate) fn play_signal(&self, period: Duration) {
        svolder_ble::drive_signal(&self.inner, period);
    }

    /// Whether a link this transport handed out has been disconnected: a backend closed
    /// is a device released.
    pub(crate) fn closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// How many times a caller has asked this transport to settle.
    pub(crate) fn settles(&self) -> usize {
        self.settles.load(Ordering::SeqCst)
    }

    /// The devices a caller asked this transport to settle for, in order.
    pub(crate) fn settled(&self) -> Vec<UnitId> {
        self.settled.lock().unwrap().clone()
    }

    /// How many connect futures were still alive when the last settle began.
    pub(crate) fn live_at_settle(&self) -> Option<usize> {
        *self.live_at_settle.lock().unwrap()
    }

    /// Make every `connect` from now on take `how_long` to answer, the way a real
    /// adapter's discovery and pairing do.
    pub(crate) fn slow_connects(&self, how_long: Duration) {
        let ms = u64::try_from(how_long.as_millis()).unwrap_or(u64::MAX);
        self.connect_ms.store(ms, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl Transport for FakeGate {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        self.listens.fetch_add(1, Ordering::SeqCst);
        if self.windowed.load(Ordering::SeqCst) {
            tokio::time::sleep(window).await;
        }
        match self.scans.load(Ordering::SeqCst) {
            true => self.inner.scan(window).await,
            false => Ok(futures::stream::empty().boxed()),
        }
    }

    /// The device is heard the moment a sighting starts, when scans can see it at all;
    /// a quiet sighting on a windowed gate waits its window out for nothing.
    async fn sight(
        &self,
        unit: UnitId,
        window: Duration,
    ) -> Result<Option<Advertisement>, BleError> {
        self.listens.fetch_add(1, Ordering::SeqCst);
        match self.scans.load(Ordering::SeqCst) {
            true => self.inner.sight(unit, window).await,
            false => {
                if self.windowed.load(Ordering::SeqCst) {
                    tokio::time::sleep(window).await;
                }
                Ok(None)
            }
        }
    }

    /// The device is held whether or not a scan can see it.
    async fn held(&self) -> Result<Advertisements, BleError> {
        self.inner.held().await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        let _live = Live::enter(&self.live_connects);
        let taking = Duration::from_millis(self.connect_ms.load(Ordering::SeqCst));
        if !taking.is_zero() {
            tokio::time::sleep(taking).await;
        }
        if !self.connects.load(Ordering::SeqCst)
            || (self.fails_after_release.load(Ordering::SeqCst) && self.inner.releases() > 0)
        {
            return Err(BleError::NotFound);
        }
        Ok(Box::new(GatedLink {
            inner: self.inner.connect(unit).await?,
            closed: Arc::clone(&self.closed),
            deaf: Arc::clone(&self.deaf),
        }))
    }

    /// Nothing here outlives the call that asked for it, so there is nothing to wait
    /// for. The ask is counted, which is what [`FakeGate::settles`] reports.
    async fn settle(&self, _bound: Duration) {
        self.settles.fetch_add(1, Ordering::SeqCst);
    }

    /// Records the device, then counts as a settle of everything.
    async fn settle_for(&self, unit: UnitId, bound: Duration) {
        self.settled.lock().unwrap().push(unit);
        *self.live_at_settle.lock().unwrap() = Some(self.live_connects.load(Ordering::SeqCst));
        self.settle(bound).await;
    }
}

/// A direct backend over a gated fake device, already connected and refreshed, beside
/// the gate that decides whether a reconnect can succeed.
///
/// The fake answers instantly, so the write spacing, the idle gap that ends a refresh
/// burst and the connect deadline are all cut right down: these cases are about what the
/// updates say, not about how long a real radio takes to say it.
pub(super) async fn fake_backend() -> (DirectBackend, Arc<FakeGate>) {
    fake_backend_with(Duration::from_millis(50)).await
}

/// The same, with `connect_deadline` as long as the case needs a reconnect to keep
/// trying for.
pub(in crate::backend) async fn fake_backend_with(
    connect_deadline: Duration,
) -> (DirectBackend, Arc<FakeGate>) {
    fake_backend_on("gungnir2-v2", connect_deadline).await
}

/// The same, on the built-in profile called `profile`, for a case that needs a device whose
/// meters have a scale to play a signal on.
pub(in crate::backend) async fn fake_backend_on(
    profile: &str,
    connect_deadline: Duration,
) -> (DirectBackend, Arc<FakeGate>) {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get(profile).expect("a built-in profile").clone();
    fake_backend_over(profile, connect_deadline).await
}

/// The same, on a profile the case built itself. The firmware versions the connect asked for
/// are read first, so the first update a case sees is its own.
pub(in crate::backend) async fn fake_backend_over(
    profile: Profile,
    connect_deadline: Duration,
) -> (DirectBackend, Arc<FakeGate>) {
    let asked = profile.firmware_subsystems.len();
    let (mut backend, gate) = fake_backend_unread_over(profile, connect_deadline).await;
    for _ in 0..asked {
        let heard = backend.next_status().await.expect("an update");
        assert!(
            matches!(&heard, Some(Update::Status(vo)) if vo.id.starts_with("210.2.")),
            "the connect's firmware answers come first: {heard:?}"
        );
    }
    (backend, gate)
}

/// [`fake_backend_over`] with the connect's firmware answers still unread.
pub(in crate::backend) async fn fake_backend_unread_over(
    profile: Profile,
    connect_deadline: Duration,
) -> (DirectBackend, Arc<FakeGate>) {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let unit: UnitId = "0102030405060708".parse().expect("a valid unit id");
    let passphrase = Passphrase::from_digits([1, 2, 3, 4]).expect("four digits");
    let gate = Arc::new(FakeGate::new(&profile, unit, passphrase));
    let config = SessionConfig {
        write_spacing: Duration::from_millis(1),
        firmware_delay: Duration::from_millis(1),
        drain_idle: Duration::from_millis(10),
        backoff_min: Duration::from_millis(1),
        backoff_max: Duration::from_millis(5),
        connect_deadline,
        ..SessionConfig::default()
    };
    let mut session = Session::with_config(
        Arc::clone(&gate) as Arc<dyn Transport>,
        gate.advertisement(),
        profile.clone(),
        passphrase,
        config,
    );
    session.connect().await.expect("the fake device connects");
    let backend = DirectBackend {
        transport: Arc::clone(&gate) as Arc<dyn Transport>,
        registry: registry.clone(),
        session: Some(session),
        profile,
        configured: None,
        passphrase: "auto".into(),
        note: None,
        entry_waits: false,
        episode: Episode::default(),
        reconnecting: None,
        last_state: DeviceState::default(),
        sent: HashMap::new(),
        warnings: Vec::new(),
        heard_ms: Some(0),
    };
    (backend, gate)
}
