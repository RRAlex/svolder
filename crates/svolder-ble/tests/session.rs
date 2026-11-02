// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Session behaviour against the in-process fake device.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use svolder_ble::{
    Advertisement, Advertisements, BleError, FakeTransport, Link, Notifications, Session,
    SessionConfig, Transport,
};
use svolder_fake::FakeDevice;
use svolder_profiles::Registry;
use svolder_proto::{FunctionId, Message, Passphrase, ProtoError, UnitId, Value};

fn gungnir_session() -> Session {
    gungnir_session_with_device().0
}

fn gungnir_session_with_device() -> (Session, Arc<Mutex<FakeDevice>>) {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let transport = FakeTransport::from_profile(&profile, unit, 3, pass);
    let device = transport.device();
    let advert = svolder_ble::Advertisement {
        manufacturer: device.lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    (
        Session::new(Arc::new(transport), advert, profile, pass),
        device,
    )
}

/// Read the firmware versions a connect asked for, which wait unread on the stream behind the
/// refresh burst: one `210.2.<subsystem>` status per subsystem the profile lists.
async fn hear_firmware(s: &mut Session) {
    for sub in s.profile().firmware_subsystems.clone() {
        let heard = tokio::time::timeout(Duration::from_secs(5), s.recv())
            .await
            .expect("recv() hung")
            .expect("a firmware status");
        assert_eq!(heard.id, FunctionId::three(210, 2, sub));
    }
}

#[tokio::test(start_paused = true)]
async fn connect_refreshes_state() {
    let mut s = gungnir_session();
    s.connect().await.unwrap();
    assert!(!s.state().is_stale());
    // Volume (10) and input (2) were seeded and reported on refresh.
    assert!(
        s.state().get(FunctionId::one(10)).is_some(),
        "volume in state"
    );
    assert!(
        s.state().get(FunctionId::one(2)).is_some(),
        "input in state"
    );
}

/// A device that talks on its own never leaves the quiet gap that ends a refresh burst: the
/// fake amplifier's meters follow the music ten times a second, and a session connecting to
/// it hands over its snapshot as soon as every function the profile names has reported,
/// rather than waiting out the bound for a silence that will not come while the music plays.
///
/// Time is paused, so what the connect takes is what the refresh logic decided and nothing
/// the machine was busy with.
#[tokio::test(start_paused = true)]
async fn a_device_that_never_goes_quiet_still_finishes_its_refresh() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("fake-amp").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let transport = FakeTransport::from_profile(&profile, unit, 3, Passphrase::NONE);
    let advert = Advertisement {
        manufacturer: transport.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    svolder_ble::drive_signal(&transport, svolder_ble::SIGNAL_PERIOD);
    let mut s = Session::new(Arc::new(transport), advert, profile, Passphrase::NONE);
    let bound = SessionConfig::default().drain_bound;
    let started = tokio::time::Instant::now();
    // Bounded so a refresh that never ends fails here with something to read, rather than
    // spinning the runtime through virtual time for as long as the test is given.
    tokio::time::timeout(Duration::from_secs(60), s.connect())
        .await
        .expect("the refresh burst ends")
        .expect("the device connects");
    let took = started.elapsed();
    assert!(
        took < bound / 4,
        "the refresh ended on the readings, not on the {bound:?} bound: it took {took:?}"
    );
    assert!(!s.state().is_stale());
    assert!(
        s.state().get(FunctionId::two(20, 1)).is_some(),
        "the left meter's reading in state"
    );
}

#[tokio::test(start_paused = true)]
async fn bootloader_device_is_refused() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    // firmware_build 0 => bootloader.
    let transport = FakeTransport::from_profile(&profile, unit, 0, Passphrase::NONE);
    let advert = svolder_ble::Advertisement {
        manufacturer: transport.device().lock().unwrap().manufacturer_data(),
        rssi: None,
        record: None,
    };
    let mut s = Session::new(Arc::new(transport), advert, profile, Passphrase::NONE);
    assert!(matches!(s.connect().await, Err(BleError::Bootloader)));
}

/// `close` releases the link a one-shot CLI verb opened: state goes stale and there is no
/// link left to send through, so a later `set` reports a connection error instead of
/// talking to a connection the session no longer holds.
#[tokio::test(start_paused = true)]
async fn close_releases_the_link() {
    let mut s = gungnir_session();
    s.connect().await.unwrap();
    assert!(!s.state().is_stale());

    s.close().await;
    assert!(s.state().is_stale());

    let err = s
        .set(Message::command(FunctionId::one(2), Value::U8(4)))
        .await
        .unwrap_err();
    assert!(matches!(err, BleError::Connection(_)), "got {err:?}");
    assert!(err.to_string().contains("not connected"), "got {err}");
}

/// Wraps a `FakeTransport` and counts every `connect` attempt, so the backoff-and-deadline
/// path in `Session::connect` can be exercised without a real device. Holds the transport
/// behind an `Arc` so a test can also keep its own handle (e.g. to call `sever`).
struct CountingTransport {
    inner: Arc<FakeTransport>,
    attempts: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Transport for CountingTransport {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        self.inner.scan(window).await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        self.inner.connect(unit).await
    }
}

#[tokio::test(start_paused = true)]
async fn connect_gives_up_at_the_deadline() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let fake = FakeTransport::from_profile(&profile, unit, 3, pass);
    // A unit id that never matches the fake device, so every connect attempt fails.
    let mut manufacturer = fake.device().lock().unwrap().manufacturer_data();
    manufacturer.unit_id = "AABBCCDDEEFF0011".parse::<UnitId>().unwrap();
    let advert = Advertisement {
        manufacturer,
        rssi: Some(-50),
        record: None,
    };
    let attempts = Arc::new(AtomicUsize::new(0));
    let transport = CountingTransport {
        inner: Arc::new(fake),
        attempts: Arc::clone(&attempts),
    };
    let config = SessionConfig {
        backoff_min: Duration::from_millis(5),
        backoff_max: Duration::from_millis(20),
        connect_deadline: Duration::from_millis(50),
        ..SessionConfig::default()
    };
    let mut s = Session::with_config(Arc::new(transport), advert, profile, pass, config);
    let err = s.connect().await.unwrap_err();
    assert!(matches!(err, BleError::Connection(_)), "got {err:?}");
    // The give-up message names the transport's own last failure, not just the deadline.
    assert!(
        err.to_string().contains("no matching device found"),
        "got {err}"
    );
    assert!(
        attempts.load(Ordering::SeqCst) >= 3,
        "expected at least three connect attempts, got {}",
        attempts.load(Ordering::SeqCst)
    );
}

#[tokio::test(start_paused = true)]
async fn set_confirms_the_echo() {
    let mut s = gungnir_session();
    s.connect().await.unwrap();
    let confirmed = s
        .set(Message::command(FunctionId::one(2), Value::U8(4)))
        .await
        .unwrap();
    assert_eq!(confirmed.id, FunctionId::one(2));
    assert_eq!(confirmed.value, Value::U8(4));
    assert_eq!(
        s.state().get(FunctionId::one(2)).unwrap().value,
        Value::U8(4)
    );
}

/// A restricted id fails before any frame exists, so the fake sees nothing.
#[tokio::test(start_paused = true)]
async fn set_of_a_restricted_id_never_reaches_the_device() {
    let (mut s, device) = gungnir_session_with_device();
    s.connect().await.unwrap();
    let seen = device.lock().unwrap().commands_seen();
    let err = s
        .set(Message::command(FunctionId::two(210, 1), Value::Bool(true)))
        .await
        .unwrap_err();
    assert!(
        matches!(err, BleError::Proto(ProtoError::Restricted(_))),
        "got {err:?}"
    );
    assert_eq!(
        device.lock().unwrap().commands_seen(),
        seen,
        "the fake saw a frame"
    );
}

/// No echo ever arrives (the fake answers `200`, identify, with nothing),
/// so `set` must give up once `set_timeout` elapses rather than waiting forever.
#[tokio::test(start_paused = true)]
async fn set_times_out_without_an_echo() {
    let mut s = gungnir_session();
    s.connect().await.unwrap();
    let err = s
        .set(Message::command(FunctionId::one(200), Value::Data(vec![])))
        .await
        .unwrap_err();
    assert!(matches!(err, BleError::Timeout(_)), "got {err:?}");
}

/// A wrong passphrase surfaces as a `250.255` status folded during
/// `connect`'s refresh. From then on `set` and `set_no_wait` must both refuse without
/// writing anything.
#[tokio::test(start_paused = true)]
async fn rejected_passphrase_stops_all_sending() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let device_pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let session_pass = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let transport = FakeTransport::from_profile(&profile, unit, 3, device_pass);
    let device = transport.device();
    let advert = svolder_ble::Advertisement {
        manufacturer: device.lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut s = Session::new(Arc::new(transport), advert, profile, session_pass);
    let connected = s.connect().await;
    assert!(connected.is_ok(), "connect returned {connected:?}");
    assert!(s.state().passphrase_rejected());

    let seen = device.lock().unwrap().commands_seen();
    let err = s
        .set(Message::command(FunctionId::one(2), Value::U8(4)))
        .await
        .unwrap_err();
    assert!(matches!(err, BleError::PassphraseRejected), "got {err:?}");
    let err = s
        .set_no_wait(Message::command(FunctionId::one(2), Value::U8(4)))
        .await
        .unwrap_err();
    assert!(matches!(err, BleError::PassphraseRejected), "got {err:?}");
    assert_eq!(
        device.lock().unwrap().commands_seen(),
        seen,
        "set/set_no_wait wrote after the passphrase was rejected"
    );
}

/// Wraps a `FakeTransport` and fails the first `fails` connect attempts, so `Session::connect`'s
/// backoff-and-retry path can be exercised end to end (as opposed to `connect_gives_up_at_the_deadline`,
/// which never succeeds at all).
struct Flaky {
    inner: FakeTransport,
    fails: usize,
    seen: AtomicUsize,
}

#[async_trait::async_trait]
impl Transport for Flaky {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        self.inner.scan(window).await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        let n = self.seen.fetch_add(1, Ordering::SeqCst);
        if n < self.fails {
            return Err(BleError::Connection("flaky".into()));
        }
        self.inner.connect(unit).await
    }
}

#[tokio::test(start_paused = true)]
async fn connect_retries_until_it_succeeds() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let inner = FakeTransport::from_profile(&profile, unit, 3, pass);
    let advert = Advertisement {
        manufacturer: inner.device().lock().unwrap().manufacturer_data(),
        rssi: None,
        record: None,
    };
    let flaky = Flaky {
        inner,
        fails: 2,
        seen: AtomicUsize::new(0),
    };
    let config = SessionConfig {
        backoff_min: Duration::from_millis(1),
        backoff_max: Duration::from_millis(4),
        connect_deadline: Duration::from_secs(2),
        ..Default::default()
    };
    let mut s = Session::with_config(Arc::new(flaky), advert, profile, pass, config);
    // A regression in the backoff/retry loop could hang forever instead of giving up or
    // succeeding; bound it so that shows up as a failure instead of blocking the runtime.
    tokio::time::timeout(Duration::from_secs(5), s.connect())
        .await
        .expect("connect() hung")
        .unwrap();
    assert!(s.state().get(FunctionId::one(10)).is_some());
}

#[tokio::test(start_paused = true)]
async fn watch_yields_a_status_then_survives_a_set() {
    let mut s = gungnir_session();
    s.connect().await.unwrap();
    // A set produces an echo; next_status should surface a status message.
    s.set_no_wait(Message::command(FunctionId::one(2), Value::U8(5)))
        .await
        .unwrap();
    // A regression here could hang forever instead of yielding a status; bound it so that
    // shows up as a failure instead of blocking the runtime.
    let status = tokio::time::timeout(Duration::from_secs(5), s.next_status())
        .await
        .expect("next_status() hung")
        .unwrap();
    assert!(status.is_some());
}

/// When the link drops mid-watch, `next_status` marks state stale,
/// reconnects with backoff (which refreshes), and keeps going rather than returning an
/// error. Whether the reconnect's refresh burst surfaces a message before the outer
/// timeout below elapses is not guaranteed, so both outcomes are accepted; either way the
/// reconnect must have happened (a second connect attempt) and state must no longer be
/// stale.
#[tokio::test(start_paused = true)]
async fn next_status_reconnects_after_the_link_is_severed() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let fake = Arc::new(FakeTransport::from_profile(&profile, unit, 3, pass));
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let attempts = Arc::new(AtomicUsize::new(0));
    let transport = CountingTransport {
        inner: Arc::clone(&fake),
        attempts: Arc::clone(&attempts),
    };
    let mut s = Session::new(Arc::new(transport), advert, profile, pass);
    s.connect().await.unwrap();
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    hear_firmware(&mut s).await;

    fake.sever();
    let result = tokio::time::timeout(Duration::from_secs(1), s.next_status()).await;
    match result {
        Err(_) => {
            // Reconnected and refreshed, then blocked on the fresh stream with nothing
            // more to read: the outer timeout elapses, as expected.
        }
        Ok(Ok(Some(_))) => {
            // Also acceptable: the reconnect's refresh burst surfaced a message before
            // the outer timeout elapsed.
        }
        Ok(other) => panic!("unexpected next_status result: {other:?}"),
    }
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert!(!s.state().is_stale());
}

/// A write attempted after the link is severed (and before any reconnect) reports a
/// connection error instead of silently succeeding.
#[tokio::test(start_paused = true)]
async fn write_after_sever_reports_a_connection_error() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let fake = Arc::new(FakeTransport::from_profile(&profile, unit, 3, pass));
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut s = Session::new(
        Arc::clone(&fake) as Arc<dyn Transport>,
        advert,
        profile,
        pass,
    );
    s.connect().await.unwrap();
    fake.sever();
    let err = s
        .set_no_wait(Message::command(FunctionId::one(2), Value::U8(3)))
        .await
        .unwrap_err();
    assert!(matches!(err, BleError::Connection(_)), "got {err:?}");
    // Pin it to the severed-link seam specifically, not just any `Connection` error (e.g.
    // "not connected", which would also match the variant above).
    assert!(err.to_string().contains("link severed"), "got {err}");
    // The failed write tore the link down, so the session no longer claims fresh state.
    assert!(s.state().is_stale());
}

/// `recv` is the cancel-safe read the daemon's device actor selects on: it hands back the
/// next status on the link it already holds and folds it into state, without reconnecting.
#[tokio::test(start_paused = true)]
async fn recv_yields_a_status_after_a_set() {
    let mut s = gungnir_session();
    assert!(!s.is_connected(), "no link before connect");
    s.connect().await.unwrap();
    assert!(s.is_connected());
    hear_firmware(&mut s).await;
    s.set_no_wait(Message::command(FunctionId::one(2), Value::U8(5)))
        .await
        .unwrap();
    let m = tokio::time::timeout(Duration::from_secs(5), s.recv())
        .await
        .expect("recv() hung")
        .expect("a status");
    assert_eq!(m.id, FunctionId::one(2));
    assert_eq!(m.value, Value::U8(5));
    assert_eq!(
        s.state().get(FunctionId::one(2)).unwrap().value,
        Value::U8(5),
        "recv folds the status into state"
    );
    assert!(s.is_connected());
}

/// One `recv`, bounded, so a regression that never yields anything fails the test instead of
/// hanging the runtime.
async fn recv_soon(s: &mut Session) -> Option<Message> {
    tokio::time::timeout(Duration::from_secs(5), s.recv())
        .await
        .expect("recv() hung")
}

/// A frame carrying several statuses yields every one of them through `recv`: nothing the
/// device said in one notification is lost.
#[tokio::test(start_paused = true)]
async fn recv_yields_every_message_of_a_frame() {
    let mut s = gungnir_session();
    s.connect().await.unwrap();
    // The fake answers a refresh with one frame holding every seeded status.
    s.set_no_wait(Message::command(FunctionId::one(1), Value::Data(vec![])))
        .await
        .unwrap();
    let first = recv_soon(&mut s).await.expect("the frame's first status");
    let second = recv_soon(&mut s).await.expect("the frame's second status");
    // The fake's state is a BTreeMap, so the frame it encodes lists statuses in ascending
    // function id order: `recv` must hand them out in that same order, not just distinctly.
    assert!(
        first.id < second.id,
        "expected ascending ids, got {:?} then {:?}",
        first.id,
        second.id
    );
}

/// A message the device sent while `set` was waiting for its echo comes out of the next
/// `recv` rather than being folded away where no caller can see it.
///
/// The fake answers each command with a frame of its own, so the mute echo below arrives
/// while the following `set` is still waiting for the input echo it asked for.
#[tokio::test(start_paused = true)]
async fn recv_hands_out_what_set_folded_but_did_not_return() {
    let mut s = gungnir_session();
    s.connect().await.unwrap();
    hear_firmware(&mut s).await;
    s.set_no_wait(Message::command(FunctionId::one(5), Value::Bool(true)))
        .await
        .unwrap();
    let confirmed = s
        .set(Message::command(FunctionId::one(2), Value::U8(4)))
        .await
        .unwrap();
    assert_eq!(confirmed.id, FunctionId::one(2));
    let leftover = recv_soon(&mut s).await.expect("the mute echo");
    assert_eq!(leftover.id, FunctionId::one(5));
    assert_eq!(leftover.value, Value::Bool(true));
}

/// A severed link ends `recv` rather than reconnecting behind the caller's back: the actor
/// owning the session decides when to reconnect, outside its `select!`.
#[tokio::test(start_paused = true)]
async fn recv_ends_when_the_link_is_severed() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let fake = Arc::new(FakeTransport::from_profile(&profile, unit, 3, pass));
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut s = Session::new(
        Arc::clone(&fake) as Arc<dyn Transport>,
        advert,
        profile,
        pass,
    );
    s.connect().await.unwrap();
    hear_firmware(&mut s).await;
    fake.sever();
    let ended = tokio::time::timeout(Duration::from_secs(5), s.recv())
        .await
        .expect("recv() hung");
    assert!(ended.is_none(), "got {ended:?}");
    assert!(!s.is_connected());
    assert!(s.state().is_stale());
}

/// After the device is told a new passphrase, `set_passphrase` makes the session encode
/// with it; the fake refuses frames carrying the old code, so an accepted `set` proves the
/// new one is in use.
#[tokio::test(start_paused = true)]
async fn set_passphrase_switches_the_code_later_frames_use() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let old = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let new = Passphrase::from_digits([5, 6, 7, 8]).unwrap();
    let fake = Arc::new(FakeTransport::from_profile(&profile, unit, 3, old));
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut s = Session::new(
        Arc::clone(&fake) as Arc<dyn Transport>,
        advert.clone(),
        profile.clone(),
        old,
    );
    s.connect().await.unwrap();
    // The fake answers a 250 command by changing its own passphrase, with no status back.
    s.set_no_wait(Message::command(
        svolder_proto::function::PASSPHRASE,
        Value::U32(new.encoded()),
    ))
    .await
    .unwrap();
    s.set_passphrase(new);
    let confirmed = s
        .set(Message::command(FunctionId::one(2), Value::U8(4)))
        .await
        .unwrap();
    assert_eq!(confirmed.value, Value::U8(4));

    // A session still encoding with the old code is refused by the same device.
    let mut stale = Session::new(
        Arc::clone(&fake) as Arc<dyn Transport>,
        advert,
        profile,
        old,
    );
    stale.connect().await.unwrap();
    assert!(stale.state().passphrase_rejected());
}

/// A code the device turned down leaves the session refusing every write before it builds
/// one; the corrected code is tried on the very next write rather than after the link drops.
#[tokio::test(start_paused = true)]
async fn a_corrected_passphrase_is_tried_on_the_next_write() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let right = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let wrong = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let fake = Arc::new(FakeTransport::from_profile(&profile, unit, 3, right));
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut s = Session::new(
        Arc::clone(&fake) as Arc<dyn Transport>,
        advert,
        profile,
        wrong,
    );

    // The refresh a connect ends with already carries the wrong code, so the device has
    // turned this session down before a caller writes anything.
    s.connect().await.unwrap();
    assert!(s.state().passphrase_rejected());
    let refused = s
        .set(Message::command(FunctionId::one(2), Value::U8(4)))
        .await;
    assert!(
        matches!(refused, Err(BleError::PassphraseRejected)),
        "got {refused:?}"
    );

    s.set_passphrase(right);
    assert!(
        !s.state().passphrase_rejected(),
        "a corrected code left the session still refusing itself"
    );
    let confirmed = s
        .set(Message::command(FunctionId::one(2), Value::U8(4)))
        .await
        .expect("the device takes the corrected code");
    assert_eq!(confirmed.value, Value::U8(4));
}

/// Being handed the same code again is no remedy: the session goes on refusing every write on
/// the link that was refused, without a frame reaching the device.
///
/// This is the path a device whose configured code is wrong and stays wrong takes over and
/// over: every reload hands each device its entry whether or not that entry changed, so "stop
/// sending until the remedy" has to survive all of them. It covers `set_passphrase` on a live
/// link only: a new link starts unrefused and its refresh asks the device again (see
/// `refused_is_about_the_link_held_now` in `episode.rs`).
#[tokio::test(start_paused = true)]
async fn the_same_passphrase_again_is_no_remedy_for_a_refusal() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let right = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let wrong = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let fake = Arc::new(FakeTransport::from_profile(&profile, unit, 3, right));
    let device = fake.device();
    let advert = Advertisement {
        manufacturer: device.lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut s = Session::new(
        Arc::clone(&fake) as Arc<dyn Transport>,
        advert,
        profile,
        wrong,
    );
    s.connect().await.unwrap();
    assert!(s.state().passphrase_rejected());
    let seen = device.lock().unwrap().commands_seen();

    // The code the session is already using, handed to it again.
    s.set_passphrase(wrong);
    assert!(
        s.state().passphrase_rejected(),
        "the refusal was lifted by a code that had not changed"
    );
    for write in ["waiting for the echo", "not waiting for it"] {
        let message = Message::command(FunctionId::one(2), Value::U8(4));
        let refused = match write {
            "waiting for the echo" => s.set(message).await.map(|_| ()),
            _ => s.set_no_wait(message).await,
        };
        assert!(
            matches!(refused, Err(BleError::PassphraseRejected)),
            "{write}: got {refused:?}"
        );
    }
    assert_eq!(
        device.lock().unwrap().commands_seen(),
        seen,
        "a frame under the refused code reached the device"
    );
}

/// Wraps a `FakeTransport`; once `fail_next` is set, every `connect` call fails
/// immediately, so a test can force a well-connected session's *next* reconnect attempt to
/// fail without otherwise touching the underlying fake device.
struct FailAfterFirst {
    inner: FakeTransport,
    fail_next: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl Transport for FailAfterFirst {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        self.inner.scan(window).await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        if self.fail_next.load(Ordering::SeqCst) {
            return Err(BleError::Connection("forced failure".into()));
        }
        self.inner.connect(unit).await
    }
}

/// A reconnect that fails to find a new link within `connect_deadline`
/// must not leave a torn-down link recorded as live with fresh-looking state — `connect`
/// tears the old link down, clears it, and marks state stale before it even tries the new
/// connection, so a failure here leaves `is_stale()` true and no link to write through.
#[tokio::test(start_paused = true)]
async fn failed_reconnect_leaves_state_stale() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let inner = FakeTransport::from_profile(&profile, unit, 3, pass);
    let advert = Advertisement {
        manufacturer: inner.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let fail_next = Arc::new(AtomicBool::new(false));
    let transport = FailAfterFirst {
        inner,
        fail_next: Arc::clone(&fail_next),
    };
    let config = SessionConfig {
        backoff_min: Duration::from_millis(1),
        backoff_max: Duration::from_millis(4),
        connect_deadline: Duration::from_millis(20),
        ..SessionConfig::default()
    };
    let mut s = Session::with_config(Arc::new(transport), advert, profile, pass, config);
    s.connect().await.unwrap();
    assert!(!s.state().is_stale());

    fail_next.store(true, Ordering::SeqCst);
    let err = s.connect().await.unwrap_err();
    assert!(matches!(err, BleError::Connection(_)), "got {err:?}");
    assert!(s.state().is_stale());

    // No link to write through: refuses with a connection error rather than silently
    // succeeding or panicking.
    let err = s
        .set_no_wait(Message::command(FunctionId::one(2), Value::U8(1)))
        .await
        .unwrap_err();
    assert!(matches!(err, BleError::Connection(_)), "got {err:?}");
}

/// Wraps a `FakeTransport` with a `connect` that never answers, so a `Session::connect` can
/// only end when its deadline elapses and the transport's connect future is dropped
/// half-done. Everything else, the settles included, goes to the transport it wraps.
struct NeverAnswers(Arc<FakeTransport>);

#[async_trait::async_trait]
impl Transport for NeverAnswers {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        self.0.scan(window).await
    }

    async fn connect(&self, _unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        std::future::pending().await
    }

    async fn settle(&self, bound: Duration) {
        self.0.settle(bound).await
    }

    async fn settle_for(&self, unit: UnitId, bound: Duration) {
        self.0.settle_for(unit, bound).await
    }
}

/// A connect that runs out its deadline leaves the transport holding a connection it is
/// taking back down, so the session waits for that before it hands control back to a caller
/// who may stop driving the runtime.
#[tokio::test(start_paused = true)]
async fn a_connect_that_runs_out_its_deadline_settles_the_transport() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let fake = Arc::new(FakeTransport::from_profile(&profile, unit, 3, pass));
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let config = SessionConfig {
        connect_deadline: Duration::from_millis(50),
        close_grace: Duration::from_millis(10),
        ..SessionConfig::default()
    };
    let transport = NeverAnswers(Arc::clone(&fake));
    let mut s = Session::with_config(Arc::new(transport), advert, profile, pass, config);

    let err = s.connect().await.unwrap_err();
    assert!(matches!(err, BleError::Connection(_)), "got {err:?}");
    assert_eq!(
        fake.settles(),
        1,
        "the give-up asked the transport to settle"
    );
    assert_eq!(
        fake.settled(),
        [unit],
        "the give-up settled another device or none"
    );
}

/// Closing a session is the other point at which a caller stops driving the runtime, so it
/// waits for the transport the same way.
#[tokio::test(start_paused = true)]
async fn close_settles_the_transport() {
    let reg = Registry::builtin().unwrap();
    let profile = reg.get("gungnir2-v2").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let fake = Arc::new(FakeTransport::from_profile(&profile, unit, 3, pass));
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut s = Session::new(
        Arc::clone(&fake) as Arc<dyn Transport>,
        advert,
        profile,
        pass,
    );
    s.connect().await.unwrap();
    assert_eq!(fake.settles(), 0, "connecting settles nothing by itself");

    s.close().await;
    assert_eq!(fake.settles(), 1, "close asked the transport to settle");
    assert_eq!(
        fake.settled(),
        [unit],
        "close settled another device or none"
    );
}

const AUTO_OFF_PROFILE: &str = r#"
schema = 1
name = "auto-off"

[[control]]
id = [10]
key = "power"
label = "Power"
kind = "toggle"

[[control]]
id = [21, 1]
key = "temperature_reporting"
label = "Temperature reporting"
kind = "readout"
value_type = "bool"
auto_off = true
"#;

const TEMPERATURE: FunctionId = FunctionId::two(21, 1);

/// A session over a fake whose profile has an `auto_off` readout on `21.1`.
fn auto_off_session() -> (Session, Arc<Mutex<FakeDevice>>) {
    let reg = Registry::from_sources(vec![("auto-off".into(), AUTO_OFF_PROFILE.into())]).unwrap();
    let profile = reg.get("auto-off").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let transport = FakeTransport::from_profile(&profile, unit, 3, Passphrase::NONE);
    let device = transport.device();
    let advert = Advertisement {
        manufacturer: device.lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    (
        Session::new(Arc::new(transport), advert, profile, Passphrase::NONE),
        device,
    )
}

/// The next status from `recv`, or a failure if none arrives within a virtual second.
async fn next(s: &mut Session) -> Message {
    tokio::time::timeout(Duration::from_secs(1), s.recv())
        .await
        .expect("a status within a second")
        .expect("the link is up")
}

/// Whether `recv` has nothing more to hand out within a virtual second.
async fn quiet(s: &mut Session) -> bool {
    tokio::time::timeout(Duration::from_secs(1), s.recv())
        .await
        .is_err()
}

fn commands(device: &Arc<Mutex<FakeDevice>>) -> usize {
    device.lock().unwrap().commands_seen()
}

/// Each report of an `auto_off` readout turning on earns exactly one write of false, the
/// original status still reaches the caller, a report of false earns none, and the echo of the
/// reply does not start a loop.
#[tokio::test(start_paused = true)]
async fn an_auto_off_report_earns_one_false_write_each() {
    let (mut s, device) = auto_off_session();
    s.connect().await.unwrap();
    let base = commands(&device);

    s.set_no_wait(Message::command(TEMPERATURE, Value::Bool(true)))
        .await
        .unwrap();
    assert_eq!(commands(&device), base + 1, "the test's own command");
    assert_eq!(
        next(&mut s).await,
        Message::status(TEMPERATURE, Value::Bool(true)),
        "the original status reaches the caller"
    );
    assert_eq!(
        next(&mut s).await,
        Message::status(TEMPERATURE, Value::Bool(false)),
        "the reply's own echo follows"
    );
    assert_eq!(commands(&device), base + 2, "one write of false");
    assert!(quiet(&mut s).await);
    assert_eq!(
        commands(&device),
        base + 2,
        "the false echo earned no reply"
    );

    // A second true report gets one more write.
    s.set_no_wait(Message::command(TEMPERATURE, Value::Bool(true)))
        .await
        .unwrap();
    assert_eq!(next(&mut s).await.value, Value::Bool(true));
    assert_eq!(next(&mut s).await.value, Value::Bool(false));
    assert!(quiet(&mut s).await);
    assert_eq!(commands(&device), base + 4);

    // A false report gets none.
    s.set_no_wait(Message::command(TEMPERATURE, Value::Bool(false)))
        .await
        .unwrap();
    assert_eq!(next(&mut s).await.value, Value::Bool(false));
    assert!(quiet(&mut s).await);
    assert_eq!(commands(&device), base + 5, "only the test's own command");
}

/// A reply queued while `set` waits for its echo goes out when `set` returns, and the
/// confirmed echo is the one `set` hands back.
#[tokio::test(start_paused = true)]
async fn a_reply_queued_during_set_is_written_when_set_returns() {
    let (mut s, device) = auto_off_session();
    s.connect().await.unwrap();
    let base = commands(&device);

    let echo = s
        .set(Message::command(TEMPERATURE, Value::Bool(true)))
        .await
        .unwrap();
    assert_eq!(echo.value, Value::Bool(true));
    assert_eq!(commands(&device), base + 2, "the command and one reply");
    assert_eq!(next(&mut s).await.value, Value::Bool(false));
    assert!(quiet(&mut s).await);
    assert_eq!(commands(&device), base + 2);
}

/// A readout the device reports on in a refresh is turned off too, whether it says true or 1.
#[tokio::test(start_paused = true)]
async fn an_auto_off_readout_reported_in_a_refresh_is_written_false() {
    for on in [Value::Bool(true), Value::U8(1)] {
        let (mut s, device) = auto_off_session();
        device.lock().unwrap().set_raw(TEMPERATURE, on.clone());
        s.connect().await.unwrap();
        // The connect's own refresh frame, plus one reply.
        assert_eq!(commands(&device), 2, "{on:?}");
        assert_eq!(
            next(&mut s).await,
            Message::status(TEMPERATURE, Value::Bool(false)),
            "{on:?}: the reply's echo"
        );
        assert!(quiet(&mut s).await);
        assert_eq!(commands(&device), 2, "{on:?}: the echo earned no reply");
    }
}

/// Dropping `recv` while it waits, as a `select!` does, loses no status and does not repeat a
/// reply.
#[tokio::test(start_paused = true)]
async fn dropping_recv_loses_no_status_and_repeats_no_reply() {
    let (mut s, device) = auto_off_session();
    s.connect().await.unwrap();
    let base = commands(&device);
    assert!(
        quiet(&mut s).await,
        "an idle recv is dropped at its timeout"
    );
    s.set_no_wait(Message::command(TEMPERATURE, Value::Bool(true)))
        .await
        .unwrap();
    assert_eq!(next(&mut s).await.value, Value::Bool(true));
    // The reply is written on the next read, which a `select!` may drop at once.
    let _ = tokio::time::timeout(Duration::ZERO, s.recv()).await;
    assert_eq!(next(&mut s).await.value, Value::Bool(false));
    assert!(quiet(&mut s).await);
    assert_eq!(commands(&device), base + 2);
}

/// A reply is encoded when it is written, with the passphrase the session holds then: a
/// passphrase change between the report and the flush reaches the device in the reply.
#[tokio::test(start_paused = true)]
async fn a_reply_carries_the_passphrase_held_when_it_is_written() {
    let reg = Registry::from_sources(vec![("auto-off".into(), AUTO_OFF_PROFILE.into())]).unwrap();
    let profile = reg.get("auto-off").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let other = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let transport = FakeTransport::from_profile(&profile, unit, 3, pass);
    let device = transport.device();
    let advert = Advertisement {
        manufacturer: device.lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut s = Session::new(Arc::new(transport), advert, profile, pass);
    s.connect().await.unwrap();

    s.set_no_wait(Message::command(TEMPERATURE, Value::Bool(true)))
        .await
        .unwrap();
    assert_eq!(next(&mut s).await.value, Value::Bool(true));
    // The reply is queued, not yet written; the code changes before it goes out.
    s.set_passphrase(other);
    let _ = tokio::time::timeout(Duration::from_secs(1), s.recv()).await;
    assert!(
        s.state().passphrase_rejected(),
        "the reply went out with the old passphrase"
    );
}

/// Wraps a `FakeTransport` so that its `fail_at`th write on the link fails, counting from the
/// first write after the connect's refresh.
struct FailingWrite {
    inner: FakeTransport,
    writes: Arc<AtomicUsize>,
    fail_at: usize,
}

struct FailingLink {
    inner: Box<dyn Link>,
    writes: Arc<AtomicUsize>,
    fail_at: usize,
}

#[async_trait::async_trait]
impl Transport for FailingWrite {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        self.inner.scan(window).await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        let inner = self.inner.connect(unit).await?;
        Ok(Box::new(FailingLink {
            inner,
            writes: Arc::clone(&self.writes),
            fail_at: self.fail_at,
        }))
    }
}

#[async_trait::async_trait]
impl Link for FailingLink {
    async fn write(&self, frame: &[u8]) -> Result<(), BleError> {
        if self.writes.fetch_add(1, Ordering::SeqCst) + 1 == self.fail_at {
            return Err(BleError::Connection("write failed".into()));
        }
        self.inner.write(frame).await
    }

    fn notifications(&self) -> svolder_ble::Notifications {
        self.inner.notifications()
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        self.inner.disconnect().await
    }
}

/// The echo that confirms the user's change is what `set` returns, even when the `auto_off`
/// reply written after it fails; the failed write has torn the link down, and the next `recv`
/// reports that.
#[tokio::test(start_paused = true)]
async fn a_failed_reply_write_does_not_fail_a_confirmed_set() {
    let reg = Registry::from_sources(vec![("auto-off".into(), AUTO_OFF_PROFILE.into())]).unwrap();
    let profile = reg.get("auto-off").unwrap().clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let inner = FakeTransport::from_profile(&profile, unit, 3, Passphrase::NONE);
    let advert = Advertisement {
        manufacturer: inner.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    // Writes: 1 the refresh, 2 the user's command, 3 the reply.
    let transport = FailingWrite {
        inner,
        writes: Arc::new(AtomicUsize::new(0)),
        fail_at: 3,
    };
    let mut s = Session::new(Arc::new(transport), advert, profile, Passphrase::NONE);
    s.connect().await.unwrap();

    let echo = s
        .set(Message::command(TEMPERATURE, Value::Bool(true)))
        .await
        .expect("the confirmed echo is the outcome");
    assert_eq!(echo.value, Value::Bool(true));
    assert!(!s.is_connected(), "the failed write tore the link down");
    assert!(
        tokio::time::timeout(Duration::from_secs(1), s.recv())
            .await
            .expect("recv answers")
            .is_none(),
        "the next read reports the lost link"
    );
}

// ---- Refresh on power-on, refresh after a corrected passphrase ----

const INPUT: FunctionId = FunctionId::one(2);
const POWER: FunctionId = FunctionId::one(10);
const WARMING: FunctionId = FunctionId::one(4);
const REFUSAL: FunctionId = FunctionId::two(250, 255);

/// The models whose power state is the power function alone.
const POWER_ONLY: [&str; 4] = ["freya2", "stjarna", "hati", "saga2"];

/// The models whose power state is the power function and the warming-up readout together.
const POWER_AND_WARMING: [&str; 5] = ["aegir2", "vidar2", "kraken", "tyr", "wotan"];

/// What a test can change about the link a session is given.
#[derive(Default)]
struct Knobs {
    /// Drop every function 1 the session writes, as a device that never answers it.
    swallow_refresh: AtomicBool,
    /// Hand notifications out one message to a frame, as a device whose burst is long.
    split: AtomicBool,
    /// After the next function 1 has reached the device, never return from the write.
    hang_after_refresh: AtomicBool,
}

struct KnobTransport {
    inner: Arc<FakeTransport>,
    knobs: Arc<Knobs>,
}

struct KnobLink {
    inner: Box<dyn Link>,
    knobs: Arc<Knobs>,
}

#[async_trait::async_trait]
impl Transport for KnobTransport {
    async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        self.inner.scan(window).await
    }

    async fn connect(&self, unit: UnitId) -> Result<Box<dyn Link>, BleError> {
        Ok(Box::new(KnobLink {
            inner: self.inner.connect(unit).await?,
            knobs: Arc::clone(&self.knobs),
        }))
    }
}

#[async_trait::async_trait]
impl Link for KnobLink {
    async fn write(&self, frame: &[u8]) -> Result<(), BleError> {
        let refresh = svolder_proto::decode_command(frame)
            .map(|d| d.body.messages.iter().any(|m| m.id == FunctionId::one(1)))
            .unwrap_or(false);
        if refresh && self.knobs.swallow_refresh.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.inner.write(frame).await?;
        if refresh && self.knobs.hang_after_refresh.swap(false, Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        Ok(())
    }

    fn notifications(&self) -> Notifications {
        use futures::StreamExt;
        let notes = self.inner.notifications();
        if !self.knobs.split.load(Ordering::SeqCst) {
            return notes;
        }
        notes
            .flat_map(|bytes| {
                let frames: Vec<Vec<u8>> = svolder_proto::decode_notification(&bytes)
                    .map(|n| {
                        n.messages
                            .iter()
                            .map(|m| svolder_proto::encode_notification(std::slice::from_ref(m)))
                            .collect::<Result<_, _>>()
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                futures::stream::iter(frames)
            })
            .boxed()
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        self.inner.disconnect().await
    }
}

/// A session over the fake of a built-in profile, its link wrapped in the knobs.
struct Rig {
    session: Session,
    fake: Arc<FakeTransport>,
    device: Arc<Mutex<FakeDevice>>,
    knobs: Arc<Knobs>,
}

fn rig_with(name: &str, device_pass: Passphrase, session_pass: Passphrase) -> Rig {
    let reg = Registry::builtin().unwrap();
    rig_for(reg.get(name).unwrap().clone(), device_pass, session_pass)
}

fn rig_for(
    profile: svolder_profiles::Profile,
    device_pass: Passphrase,
    session_pass: Passphrase,
) -> Rig {
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let fake = Arc::new(FakeTransport::from_profile(&profile, unit, 3, device_pass));
    let device = fake.device();
    let knobs = Arc::new(Knobs::default());
    let advert = Advertisement {
        manufacturer: device.lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let transport = Arc::new(KnobTransport {
        inner: Arc::clone(&fake),
        knobs: Arc::clone(&knobs),
    });
    Rig {
        session: Session::new(transport, advert, profile, session_pass),
        fake,
        device,
        knobs,
    }
}

fn rig(name: &str) -> Rig {
    rig_with(name, Passphrase::NONE, Passphrase::NONE)
}

impl Rig {
    /// Have the device report `messages` on the link.
    fn push(&self, messages: &[Message]) {
        self.fake.push(messages);
    }

    fn commands(&self) -> usize {
        commands(&self.device)
    }

    /// Hand out everything `recv` has until a virtual second passes with nothing.
    async fn drain(&mut self) -> Vec<Message> {
        let mut out = Vec::new();
        while let Ok(Some(m)) =
            tokio::time::timeout(Duration::from_secs(1), self.session.recv()).await
        {
            out.push(m);
        }
        out
    }

    /// Make the device report `power`, and let `recv` take it in; returns what the session
    /// wrote in answer, in function 1 frames.
    async fn power(&mut self, power: bool) -> usize {
        let before = self.commands();
        self.push(&[Message::status(POWER, Value::Bool(power))]);
        self.drain().await;
        self.commands() - before
    }
}

fn status(id: FunctionId, value: Value) -> Message {
    Message::status(id, value)
}

/// A device whose power function alone says whether it is on is refreshed once each time it
/// goes from standby (off, on Saga 2) to on, and not for a report that leaves it where it was.
#[tokio::test(start_paused = true)]
async fn a_device_coming_back_on_is_refreshed_once() {
    for name in POWER_ONLY {
        let mut r = rig(name);
        r.device.lock().unwrap().set_raw(POWER, Value::Bool(true));
        r.session.connect().await.unwrap();
        assert_eq!(
            r.commands(),
            1,
            "{name}: the connect's refresh and nothing else"
        );

        assert_eq!(r.power(true).await, 0, "{name}: on again: nothing");
        assert_eq!(r.power(false).await, 0, "{name}: leaving on: nothing");
        assert_eq!(r.power(false).await, 0, "{name}: standby again: nothing");
        assert_eq!(r.power(true).await, 1, "{name}: back on: one refresh");
        assert_eq!(r.power(true).await, 0, "{name}: on again: nothing");
        assert_eq!(r.power(false).await, 0, "{name}");
        assert_eq!(
            r.power(true).await,
            1,
            "{name}: back on once more: one more"
        );
    }
}

/// The refresh's burst reaches `recv` like any other statuses.
#[tokio::test(start_paused = true)]
async fn the_burst_of_the_refresh_comes_out_of_recv() {
    let mut r = rig("freya2");
    r.session.connect().await.unwrap();
    r.power(false).await;
    r.device.lock().unwrap().set_raw(INPUT, Value::U8(3));
    r.push(&[status(POWER, Value::Bool(true))]);
    let got = r.drain().await;
    assert!(
        got.contains(&status(INPUT, Value::U8(3))),
        "the input the refresh read is handed out: {got:?}"
    );
}

/// A model whose power is the power function and the warming-up readout together is refreshed
/// on warming up to on, and on standby to on, and not on standby to warming up or on leaving.
#[tokio::test(start_paused = true)]
async fn an_amp_is_refreshed_when_it_finishes_warming_up_or_wakes() {
    for name in POWER_AND_WARMING {
        let mut r = rig(name);
        r.device.lock().unwrap().set_raw(POWER, Value::Bool(true));
        r.device
            .lock()
            .unwrap()
            .set_raw(WARMING, Value::Bool(false));
        r.session.connect().await.unwrap();
        assert_eq!(r.commands(), 1, "{name}");

        assert_eq!(r.power(false).await, 0, "{name}: on to standby");
        // Standby to warming up: power true while the readout still says true.
        r.push(&[status(WARMING, Value::Bool(true))]);
        r.drain().await;
        assert_eq!(r.power(true).await, 0, "{name}: standby to warming up");
        r.push(&[status(WARMING, Value::Bool(false))]);
        let before = r.commands();
        r.drain().await;
        assert_eq!(r.commands() - before, 1, "{name}: warming up to on");
        r.push(&[status(WARMING, Value::Bool(true))]);
        let before = r.commands();
        r.drain().await;
        assert_eq!(r.commands() - before, 0, "{name}: on to warming up");
        assert_eq!(r.power(false).await, 0, "{name}: warming up to standby");
        r.push(&[status(WARMING, Value::Bool(false))]);
        r.drain().await;
        assert_eq!(r.power(true).await, 1, "{name}: standby to on");
    }
}

/// Only a Boolean status moves the state, and only on the models that have one: another type,
/// a command from the device, and every other model change nothing.
#[tokio::test(start_paused = true)]
async fn only_a_boolean_status_on_a_model_with_a_power_state_counts() {
    let mut r = rig("freya2");
    r.session.connect().await.unwrap();
    assert_eq!(r.power(false).await, 0);
    for other in [Value::U8(1), Value::U16(1), Value::U32(1)] {
        r.push(&[status(POWER, other.clone())]);
        let before = r.commands();
        r.drain().await;
        assert_eq!(r.commands(), before, "{other:?}");
    }
    r.push(&[Message::command(POWER, Value::Bool(true))]);
    let before = r.commands();
    r.drain().await;
    assert_eq!(r.commands(), before, "a command from the device");

    let reg = Registry::builtin().unwrap();
    for name in reg.names() {
        if POWER_ONLY.contains(&name) || POWER_AND_WARMING.contains(&name) {
            continue;
        }
        let Some(profile) = reg.get(name) else {
            continue;
        };
        if profile.product_id.is_none() {
            continue;
        }
        let mut r = rig(name);
        r.session.connect().await.unwrap();
        // The firmware asks the connect owes go out first, so they are not counted below.
        r.drain().await;
        assert_eq!(r.power(false).await, 0, "{name}");
        assert_eq!(r.power(true).await, 0, "{name}");
    }
}

/// The state starts on, so a first report of on is no change; but nothing is judged on an amp
/// until the power function has reported, whatever the warming-up readout said first, so the
/// refresh burst of a connect (which lists function 4 before function 10) is not read as a
/// standby followed by a wake.
#[tokio::test(start_paused = true)]
async fn the_connect_burst_of_an_amp_is_not_a_wake() {
    for warming in [false, true] {
        let mut r = rig("aegir2");
        r.device.lock().unwrap().set_raw(POWER, Value::Bool(true));
        r.device
            .lock()
            .unwrap()
            .set_raw(WARMING, Value::Bool(warming));
        r.session.connect().await.unwrap();
        r.drain().await;
        assert_eq!(r.commands(), 1, "warming {warming}: no extra refresh");
    }
}

/// The state outlives a link: a device in standby when the link dropped and on when it is
/// back is refreshed by the connect and once more for coming back on.
#[tokio::test(start_paused = true)]
async fn a_device_that_woke_while_the_link_was_down_is_refreshed_again() {
    let mut r = rig("freya2");
    r.device.lock().unwrap().set_raw(POWER, Value::Bool(false));
    r.session.connect().await.unwrap();
    assert_eq!(r.commands(), 1, "standby from the start: nothing extra");
    r.fake.sever();
    assert!(r.session.recv().await.is_none());

    r.device.lock().unwrap().set_raw(POWER, Value::Bool(true));
    r.session.connect().await.unwrap();
    assert_eq!(r.commands(), 3, "the connect's refresh and the wake's");
    // The wake's burst is there to be read, and a set waits for it.
    let echo = r
        .session
        .set(Message::command(INPUT, Value::U8(4)))
        .await
        .unwrap();
    assert_eq!(echo.value, Value::U8(4));
}

/// A set made while the refresh's burst is arriving is confirmed by its own echo, not by the old
/// value of that function the burst carries.
#[tokio::test(start_paused = true)]
async fn a_set_during_the_wake_burst_is_confirmed_by_its_own_echo() {
    let mut r = rig("freya2");
    r.device.lock().unwrap().set_raw(INPUT, Value::U8(1));
    r.session.connect().await.unwrap();
    r.power(false).await;
    // Back on: the refresh is owed, and the first thing the session does is a set. The
    // refresh it writes first brings input 1.
    r.push(&[status(POWER, Value::Bool(true))]);
    assert_eq!(next(&mut r.session).await.id, POWER);
    let echo = r
        .session
        .set(Message::command(INPUT, Value::U8(4)))
        .await
        .unwrap();
    assert_eq!(
        echo.value,
        Value::U8(4),
        "the old input was taken for the echo"
    );
}

/// The same when the burst is long, its first message already handed out by `recv` and the
/// rest still on the way.
#[tokio::test(start_paused = true)]
async fn a_set_waits_for_the_rest_of_a_burst_recv_has_begun() {
    let mut r = rig("freya2");
    r.knobs.split.store(true, Ordering::SeqCst);
    r.device.lock().unwrap().set_raw(INPUT, Value::U8(1));
    r.session.connect().await.unwrap();
    r.power(false).await;
    r.push(&[status(POWER, Value::Bool(true))]);
    assert_eq!(next(&mut r.session).await.id, POWER);
    // The refresh goes out with this call and its first status comes back.
    let _first = next(&mut r.session).await;
    let echo = r
        .session
        .set(Message::command(INPUT, Value::U8(4)))
        .await
        .unwrap();
    assert_eq!(
        echo.value,
        Value::U8(4),
        "the old input was taken for the echo"
    );
}

/// No status older than a confirmed echo comes out of `recv` after it.
#[tokio::test(start_paused = true)]
async fn recv_hands_out_nothing_older_than_a_confirmed_echo() {
    let mut r = rig("freya2");
    r.device.lock().unwrap().set_raw(INPUT, Value::U8(1));
    r.session.connect().await.unwrap();
    r.power(false).await;
    r.push(&[status(POWER, Value::Bool(true))]);
    assert_eq!(next(&mut r.session).await.id, POWER);
    let echo = r
        .session
        .set(Message::command(INPUT, Value::U8(4)))
        .await
        .unwrap();
    assert_eq!(echo.value, Value::U8(4));
    let after = r.drain().await;
    assert!(
        !after.contains(&status(INPUT, Value::U8(1))),
        "the burst's old input came out after the echo: {after:?}"
    );
    assert_eq!(
        r.session.state().get(INPUT).unwrap().value,
        Value::U8(4),
        "state follows the echo"
    );
}

/// A power-on seen in the connect's own refresh burst is refreshed after the burst, and the two
/// bursts are told apart: the connect hands back with the second still arriving, and a set made
/// then is confirmed by its own echo.
#[tokio::test(start_paused = true)]
async fn a_wake_in_the_connect_burst_does_not_blur_the_two_bursts() {
    let mut r = rig("freya2");
    r.device.lock().unwrap().set_raw(POWER, Value::Bool(false));
    r.device.lock().unwrap().set_raw(INPUT, Value::U8(1));
    r.session.connect().await.unwrap();
    r.fake.sever();
    assert!(r.session.recv().await.is_none());
    r.device.lock().unwrap().set_raw(POWER, Value::Bool(true));

    r.session.connect().await.unwrap();
    assert_eq!(r.commands(), 3);
    let echo = r
        .session
        .set(Message::command(INPUT, Value::U8(4)))
        .await
        .unwrap();
    assert_eq!(echo.value, Value::U8(4));
    let after = r.drain().await;
    assert!(!after.contains(&status(INPUT, Value::U8(1))), "{after:?}");
}

/// Dropping `recv` inside the write of the refresh, after the frame has reached the device,
/// loses neither the refresh nor the track of its burst: a set still gets its own echo.
#[tokio::test(start_paused = true)]
async fn dropping_recv_inside_the_refresh_write_keeps_track_of_it() {
    let mut r = rig("freya2");
    r.device.lock().unwrap().set_raw(INPUT, Value::U8(1));
    r.session.connect().await.unwrap();
    r.power(false).await;
    let before = r.commands();
    r.push(&[status(POWER, Value::Bool(true))]);
    assert_eq!(next(&mut r.session).await.id, POWER);
    r.knobs.hang_after_refresh.store(true, Ordering::SeqCst);
    // The refresh reaches the device and the write never returns; the caller gives up on it.
    let hung = tokio::time::timeout(Duration::from_secs(1), r.session.recv()).await;
    assert!(hung.is_err(), "the write was meant to hang");
    assert_eq!(r.commands() - before, 1, "the refresh reached the device");

    let echo = r
        .session
        .set(Message::command(INPUT, Value::U8(4)))
        .await
        .unwrap();
    assert_eq!(
        echo.value,
        Value::U8(4),
        "the old input was taken for the echo"
    );
}

/// Nothing is written while the device has refused the passphrase, not for a power-on in the
/// frame that carries the refusal, and not for one after it.
#[tokio::test(start_paused = true)]
async fn nothing_is_written_while_the_passphrase_is_refused() {
    let right = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let wrong = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let mut r = rig_with("freya2", right, wrong);
    r.session.connect().await.unwrap();
    assert!(r.session.state().passphrase_rejected());
    let base = r.commands();
    assert_eq!(r.power(false).await, 0);
    assert_eq!(r.power(true).await, 0, "a wake while refused");
    r.push(&[
        status(POWER, Value::Bool(false)),
        status(POWER, Value::Bool(true)),
    ]);
    r.drain().await;
    assert_eq!(
        r.commands(),
        base,
        "something was written under a refused code"
    );
}

/// A refusal in the frame that carries the wake cancels the refresh the wake owes.
#[tokio::test(start_paused = true)]
async fn a_refusal_in_the_frame_of_the_wake_cancels_the_refresh() {
    let mut r = rig("freya2");
    r.session.connect().await.unwrap();
    r.power(false).await;
    let base = r.commands();
    r.push(&[
        status(POWER, Value::Bool(true)),
        status(REFUSAL, Value::Bool(true)),
    ]);
    r.drain().await;
    assert!(r.session.state().passphrase_rejected());
    assert_eq!(r.commands(), base);
}

/// The wait before a set ends on the quiet gap when the device never answers the refresh, and
/// the set still has its whole timeout after it.
#[tokio::test(start_paused = true)]
async fn the_wait_for_a_silent_burst_is_one_quiet_gap_and_the_set_keeps_its_timeout() {
    let mut r = rig("freya2");
    r.session.connect().await.unwrap();
    r.power(false).await;
    r.knobs.swallow_refresh.store(true, Ordering::SeqCst);
    r.push(&[status(POWER, Value::Bool(true))]);
    assert_eq!(next(&mut r.session).await.id, POWER);
    let idle = r.session.config().drain_idle;
    let started = tokio::time::Instant::now();
    let echo = r
        .session
        .set(Message::command(INPUT, Value::U8(4)))
        .await
        .unwrap();
    assert_eq!(echo.value, Value::U8(4));
    let took = started.elapsed();
    assert!(
        took >= idle && took < idle * 2,
        "the wait was one quiet gap, took {took:?}"
    );
}

/// A profile with a power function and meters that carry a scale, so the fake plays a signal.
const CHATTY_PROFILE: &str = r#"
schema = 1
name = "chatty"

[[control]]
id = [10]
key = "power"
label = "Power"
kind = "toggle"

[[control]]
id = [2]
key = "input"
label = "Input"
kind = "select"
values = { 1 = "One", 2 = "Two", 4 = "Four" }

[[control]]
id = [20, 1]
key = "meter_1"
label = "Meter L"
kind = "meter"
channel = "L"
range = [0, 1023]
floor = 8
tick = 700

[[control]]
id = [20, 2]
key = "meter_2"
label = "Meter R"
kind = "meter"
channel = "R"
range = [0, 1023]
floor = 8
tick = 700
"#;

/// A device that never stops talking and never finishes the burst holds a set back for the
/// burst's bound at most, and the set's own timeout starts after that.
#[tokio::test(start_paused = true)]
async fn the_wait_for_a_burst_that_never_ends_is_bounded_and_not_the_sets_timeout() {
    let reg = Registry::from_sources(vec![("chatty".into(), CHATTY_PROFILE.into())]).unwrap();
    let mut r = rig_for(
        reg.get("chatty").unwrap().clone(),
        Passphrase::NONE,
        Passphrase::NONE,
    );
    let config = SessionConfig {
        set_timeout: Duration::from_secs(1),
        ..SessionConfig::default()
    };
    let bound = config.drain_bound;
    let advert = r.session.advertisement().clone();
    let profile = r.session.profile().clone();
    let transport = Arc::new(KnobTransport {
        inner: Arc::clone(&r.fake),
        knobs: Arc::clone(&r.knobs),
    });
    r.session = Session::with_config(transport, advert, profile, Passphrase::NONE, config);
    r.session.connect().await.unwrap();
    svolder_ble::drive_signal(&r.fake, svolder_ble::SIGNAL_PERIOD);
    r.knobs.swallow_refresh.store(true, Ordering::SeqCst);
    // The signal never lets `drain` go quiet, so the reports are read one by one.
    for on in [false, true] {
        r.push(&[status(POWER, Value::Bool(on))]);
        while next(&mut r.session).await.id != POWER {}
    }
    let started = tokio::time::Instant::now();
    let echo = r
        .session
        .set(Message::command(INPUT, Value::U8(4)))
        .await
        .expect("the set keeps its own timeout");
    assert_eq!(echo.value, Value::U8(4));
    let took = started.elapsed();
    assert!(
        took >= bound / 2 && took <= bound + Duration::from_millis(500),
        "the wait ran to the bound, not to a quiet gap or past it: {took:?}"
    );
}

/// A passphrase corrected on a link that is up reads the device again, and what the refused
/// connect never read is read.
#[tokio::test(start_paused = true)]
async fn a_corrected_passphrase_refreshes_the_device() {
    let right = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let wrong = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let mut r = rig_with("freya2", right, wrong);
    r.session.connect().await.unwrap();
    assert!(r.session.state().passphrase_rejected());
    assert!(
        r.session.state().get(INPUT).is_none(),
        "the refused connect read nothing"
    );
    let base = r.commands();

    r.session.set_passphrase(right);
    let got = r.drain().await;
    assert_eq!(r.commands(), base + 1, "one refresh");
    assert!(got.iter().any(|m| m.id == INPUT), "{got:?}");
    assert!(r.session.state().get(INPUT).is_some());
    assert!(!r.session.state().passphrase_rejected());
}

/// The corrected passphrase's refresh is also written when a `set` is the next thing the
/// session does.
#[tokio::test(start_paused = true)]
async fn a_set_after_a_corrected_passphrase_writes_the_refresh_first() {
    let right = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let wrong = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let mut r = rig_with("freya2", right, wrong);
    r.session.connect().await.unwrap();
    let base = r.commands();
    r.session.set_passphrase(right);
    let echo = r
        .session
        .set(Message::command(INPUT, Value::U8(4)))
        .await
        .unwrap();
    assert_eq!(echo.value, Value::U8(4));
    assert_eq!(r.commands(), base + 2, "the refresh and the set");
}

/// A changed code that is still wrong is refused again after its one refresh; the same code
/// again, and a change on a session that was not refused, write nothing.
#[tokio::test(start_paused = true)]
async fn only_a_changed_code_that_lifts_a_refusal_earns_a_refresh() {
    let right = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let wrong = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let wronger = Passphrase::from_digits([8, 8, 8, 8]).unwrap();
    let mut r = rig_with("freya2", right, wrong);
    r.session.connect().await.unwrap();
    let base = r.commands();

    r.session.set_passphrase(wrong);
    r.drain().await;
    assert_eq!(r.commands(), base, "the same code again");

    r.session.set_passphrase(wronger);
    r.drain().await;
    assert_eq!(
        r.commands(),
        base + 1,
        "one refresh under the new wrong code"
    );
    assert!(r.session.state().passphrase_rejected(), "refused again");
    r.drain().await;
    assert_eq!(r.commands(), base + 1, "and nothing after");

    r.session.set_passphrase(right);
    r.drain().await;
    assert_eq!(r.commands(), base + 2);

    // Not refused: a change of code is the caller's business, no read is owed.
    r.session.set_passphrase(wrong);
    r.drain().await;
    assert_eq!(r.commands(), base + 2, "a change with no refusal to lift");
}

/// With no link up, a corrected code owes nothing: the connect that comes refreshes anyway.
#[tokio::test(start_paused = true)]
async fn a_corrected_code_with_no_link_owes_nothing() {
    let right = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let wrong = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let mut r = rig_with("freya2", right, wrong);
    r.session.connect().await.unwrap();
    r.session.close().await;
    r.session.set_passphrase(right);
    let base = r.commands();
    r.session.connect().await.unwrap();
    assert_eq!(r.commands(), base + 1, "the connect's refresh only");
    assert!(r.drain().await.is_empty());
    assert_eq!(r.commands(), base + 1);
}

/// The same as the connect-burst test, over a long burst and a `set` whose function comes after
/// the power function in the burst's order: a refresh written while the connect's own burst is
/// still being read would have that burst's later statuses counted against its own.
#[tokio::test(start_paused = true)]
async fn a_wake_in_a_long_connect_burst_does_not_blur_the_two_bursts() {
    const CAPACITANCE: FunctionId = FunctionId::two(11, 1);
    let mut r = rig("stjarna");
    r.knobs.split.store(true, Ordering::SeqCst);
    r.device.lock().unwrap().set_raw(POWER, Value::Bool(false));
    r.device.lock().unwrap().set_raw(CAPACITANCE, Value::U8(1));
    r.session.connect().await.unwrap();
    r.fake.sever();
    assert!(r.session.recv().await.is_none());
    r.device.lock().unwrap().set_raw(POWER, Value::Bool(true));

    r.session.connect().await.unwrap();
    assert_eq!(r.commands(), 3, "the connect's refresh and the wake's");
    let echo = r
        .session
        .set(Message::command(CAPACITANCE, Value::U8(3)))
        .await
        .unwrap();
    assert_eq!(
        echo.value,
        Value::U8(3),
        "the old value was taken for the echo"
    );
    let after = r.drain().await;
    assert!(
        !after.contains(&status(CAPACITANCE, Value::U8(1))),
        "{after:?}"
    );
}

/// A reloaded profile leaves the power state where it was.
#[tokio::test(start_paused = true)]
async fn a_reloaded_profile_keeps_the_power_state() {
    let mut r = rig("freya2");
    r.session.connect().await.unwrap();
    assert_eq!(r.power(false).await, 0);
    let profile = r.session.profile().clone();
    r.session.set_profile(profile);
    assert_eq!(r.power(true).await, 1, "standby survived the reload");
}

/// Two changes back to on in one frame owe one refresh, not two.
#[tokio::test(start_paused = true)]
async fn two_wakes_in_one_frame_send_a_single_refresh() {
    let mut r = rig("freya2");
    r.session.connect().await.unwrap();
    assert_eq!(r.power(false).await, 0);
    let before = r.commands();
    r.push(&[
        status(POWER, Value::Bool(true)),
        status(POWER, Value::Bool(false)),
        status(POWER, Value::Bool(true)),
    ]);
    r.drain().await;
    assert_eq!(r.commands() - before, 1);
}

/// A burst that never completed and ran past its bound is over: the next refresh starts a new
/// one rather than adding to it, so a set after it is held back by that burst alone.
#[tokio::test(start_paused = true)]
async fn a_burst_past_its_bound_is_not_added_to() {
    let mut r = rig("freya2");
    r.session.connect().await.unwrap();
    r.power(false).await;
    r.knobs.swallow_refresh.store(true, Ordering::SeqCst);
    r.push(&[status(POWER, Value::Bool(true))]);
    r.drain().await;
    tokio::time::sleep(r.session.config().drain_bound * 2).await;
    r.knobs.swallow_refresh.store(false, Ordering::SeqCst);
    r.power(false).await;
    r.push(&[status(POWER, Value::Bool(true))]);
    assert_eq!(next(&mut r.session).await.id, POWER);
    let started = tokio::time::Instant::now();
    r.session
        .set(Message::command(INPUT, Value::U8(4)))
        .await
        .unwrap();
    let took = started.elapsed();
    assert!(
        took < r.session.config().drain_idle,
        "the set waited for a burst that was never coming: {took:?}"
    );
}

/// A refresh the device answered in full shows the code works: every function the profile names
/// reported, with no refusal among them.
#[tokio::test(start_paused = true)]
async fn the_connect_burst_shows_the_code_works() {
    let mut s = gungnir_session();
    assert!(!s.code_accepted(), "nothing has been answered yet");
    s.connect().await.unwrap();
    assert!(s.code_accepted());
}

/// A refresh the device refused shows nothing: the code is not accepted, and stays so.
#[tokio::test(start_paused = true)]
async fn a_refused_burst_does_not_show_the_code_works() {
    let mut r = rig_with(
        "gungnir2-v2",
        Passphrase::from_digits([1, 2, 3, 4]).unwrap(),
        Passphrase::from_digits([9, 9, 9, 9]).unwrap(),
    );
    r.session.connect().await.unwrap();
    assert!(r.session.state().passphrase_rejected());
    assert!(!r.session.code_accepted());
}

/// The echo of a `set` shows the code works, and a different code starts the proof over: what
/// was shown about one code says nothing about another.
#[tokio::test(start_paused = true)]
async fn a_set_echo_shows_the_code_works_and_a_new_code_starts_over() {
    let mut s = gungnir_session();
    s.connect().await.unwrap();
    assert!(s.code_accepted());
    s.set_passphrase(Passphrase::from_digits([5, 5, 5, 5]).unwrap());
    assert!(!s.code_accepted(), "a different code is not yet answered");
    s.set_passphrase(Passphrase::from_digits([1, 2, 3, 4]).unwrap());
    assert!(!s.code_accepted(), "the proof for the old code is gone");
    s.set(Message::command(FunctionId::one(2), Value::U8(4)))
        .await
        .unwrap();
    assert!(s.code_accepted(), "the echo of a set answered the code");
}

/// A status the device sends on its own proves nothing: a profile that names no function never
/// completes a refresh, and what the device volunteers afterwards is not an answer.
#[tokio::test(start_paused = true)]
async fn a_status_the_device_volunteers_does_not_show_the_code_works() {
    let profile = Registry::generic(2);
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let fake = Arc::new(FakeTransport::from_profile(&profile, unit, 3, pass));
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut s = Session::new(
        Arc::clone(&fake) as Arc<dyn Transport>,
        advert,
        profile,
        pass,
    );
    s.connect().await.unwrap();
    assert!(
        !s.code_accepted(),
        "no function was awaited, so none answered"
    );
    fake.push(&[Message::status(FunctionId::one(10), Value::U8(100))]);
    assert!(s.recv().await.is_some());
    assert!(!s.code_accepted());
}

/// A new link starts the proof over: what the device answered on the link before says nothing
/// about the one that replaces it, so a session whose refresh proves nothing is not accepted
/// again until the device answers on the new link.
#[tokio::test(start_paused = true)]
async fn a_new_connect_starts_the_proof_over() {
    let profile = Registry::generic(2);
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let fake = Arc::new(FakeTransport::from_profile(&profile, unit, 3, pass));
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut s = Session::new(
        Arc::clone(&fake) as Arc<dyn Transport>,
        advert,
        profile,
        pass,
    );
    s.connect().await.unwrap();
    s.set(Message::command(FunctionId::one(10), Value::U8(90)))
        .await
        .unwrap();
    assert!(s.code_accepted());
    s.connect().await.unwrap();
    assert!(!s.code_accepted(), "the old link's answer carried over");
}

/// The refresh a wake owes the device is one of the session's own frames too: when every
/// function the profile names has answered it, the device has shown the code works.
#[tokio::test(start_paused = true)]
async fn the_burst_a_wake_brings_shows_the_code_works() {
    let mut r = rig("freya2");
    r.session.connect().await.unwrap();
    r.power(false).await;
    // A different code and back: the proof of the connect is gone.
    r.session
        .set_passphrase(Passphrase::from_digits([5, 5, 5, 5]).unwrap());
    r.session.set_passphrase(Passphrase::NONE);
    assert!(!r.session.code_accepted());
    r.power(true).await;
    assert!(r.session.code_accepted());
}

/// A refusal that arrives after the connect's burst showed the code works ends the proof: the
/// accessor does not claim a code the device has just turned down.
#[tokio::test(start_paused = true)]
async fn a_refusal_after_a_proof_ends_it() {
    let profile = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let pass = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let fake = Arc::new(FakeTransport::from_profile(&profile, unit, 3, pass));
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut s = Session::new(
        Arc::clone(&fake) as Arc<dyn Transport>,
        advert,
        profile,
        pass,
    );
    s.connect().await.unwrap();
    assert!(s.code_accepted());
    hear_firmware(&mut s).await;
    fake.push(&[Message::status(
        svolder_proto::function::PASSPHRASE_REJECTED,
        Value::Bool(true),
    )]);
    assert!(s.recv().await.is_some());
    assert!(!s.code_accepted());
}
