// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

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
