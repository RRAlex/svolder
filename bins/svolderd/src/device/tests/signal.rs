// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

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

/// Each strength a listen heard goes into the ring, at the moment it was heard, and `readings`
/// answers the series; a held advertisement adds nothing. The series is kept while the device
/// is not connected.
#[tokio::test(start_paused = true)]
async fn the_strengths_heard_are_a_series_in_the_readings_answer() {
    let (actor, handle) = actor_and_handle().await;
    let mut advert = actor.advert.clone().expect("the actor holds an advert");
    clock_at(100_000);
    advert.rssi = Some(-70);
    actor.update_identity(&advert, Some(95_000));
    advert.rssi = Some(-40);
    actor.update_identity(&advert, None);
    advert.rssi = Some(-61);
    actor.update_identity(&advert, Some(100_000));
    locked(&handle.info).connected = false;
    let Response::Readings { signal, .. } = handle.readings(1800) else {
        panic!("not a readings answer");
    };
    assert_eq!(signal, [(95_000, -70.0), (100_000, -61.0)]);
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
