// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

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
        matches!(&state, Response::Error { code: CODE_LINK, message, reason: Some(svolder_ipc::Reason::NotFoundYet) } if *message == svolder_ipc::not_found_yet("parked")),
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
        matches!(&got, Response::Error { code: CODE_LINK, message, .. } if *message == svolder_ipc::not_found_yet("parked")),
        "got {got:?}"
    );
    let eq = ask(&handle, Command::EqShow).await;
    assert!(
        matches!(&eq, Response::Error { code: CODE_LINK, message, .. } if *message == svolder_ipc::not_found_yet("parked")),
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
        matches!(&set, Response::Error { code: CODE_LINK, message, .. } if message == "device not connected"),
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
        matches!(&refused, Response::Error { code: CODE_USAGE, message, .. } if message.contains("firmware")),
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
