// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Session timing and queue bounds, on paused time against the fake link.

use super::*;
use crate::fake::FakeTransport;
use crate::tap::{FrameDirection, FrameTap, TappedTransport};
use std::sync::Mutex;
use svolder_profiles::Registry;
use svolder_proto::UnitId;

/// A link that stamps every write with the time it went out.
type Stamps = Arc<Mutex<Vec<Instant>>>;

struct StampedLink {
    inner: Box<dyn Link>,
    writes: Arc<Mutex<Vec<Instant>>>,
    disconnects: Arc<Mutex<Vec<Instant>>>,
}

struct StampedTransport {
    inner: FakeTransport,
    writes: Arc<Mutex<Vec<Instant>>>,
    disconnects: Arc<Mutex<Vec<Instant>>>,
}

#[async_trait::async_trait]
impl Transport for StampedTransport {
    async fn scan(&self, window: Duration) -> Result<crate::Advertisements, BleError> {
        self.inner.scan(window).await
    }

    async fn connect(&self, unit: svolder_proto::UnitId) -> Result<Box<dyn Link>, BleError> {
        let inner = self.inner.connect(unit).await?;
        Ok(Box::new(StampedLink {
            inner,
            writes: Arc::clone(&self.writes),
            disconnects: Arc::clone(&self.disconnects),
        }))
    }
}

#[async_trait::async_trait]
impl Link for StampedLink {
    async fn write(&self, frame: &[u8]) -> Result<(), BleError> {
        self.writes.lock().unwrap().push(Instant::now());
        self.inner.write(frame).await
    }

    fn notifications(&self) -> Notifications {
        self.inner.notifications()
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        self.disconnects.lock().unwrap().push(Instant::now());
        self.inner.disconnect().await
    }
}

/// A connected session whose writes are stamped, with the connect's own writes cleared.
async fn stamped_session(config: SessionConfig) -> (Session, Arc<Mutex<Vec<Instant>>>) {
    let profile = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let transport = FakeTransport::from_profile(&profile, unit, 3, Passphrase::NONE);
    let advert = Advertisement {
        manufacturer: transport.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let writes = Arc::new(Mutex::new(Vec::new()));
    let stamped = StampedTransport {
        inner: transport,
        writes: Arc::clone(&writes),
        disconnects: Arc::default(),
    };
    let mut s = Session::with_config(Arc::new(stamped), advert, profile, Passphrase::NONE, config);
    s.connect().await.unwrap();
    writes.lock().unwrap().clear();
    (s, writes)
}

fn volume(v: u8) -> Message {
    Message::command(FunctionId::one(10), Value::U8(v))
}

#[tokio::test(start_paused = true)]
async fn set_gives_up_set_timeout_after_it_began_not_after_its_write() {
    let config = SessionConfig {
        write_spacing: Duration::from_millis(500),
        set_timeout: Duration::from_millis(600),
        ..SessionConfig::default()
    };
    let (mut s, _) = stamped_session(config).await;
    s.set_no_wait(volume(1)).await.unwrap();
    let began = Instant::now();
    // No device answers function 200, so this runs to its deadline; its write first waits out
    // the spacing left from the write above.
    let err = s
        .set(Message::command(FunctionId::one(200), Value::Data(vec![])))
        .await
        .unwrap_err();
    assert!(matches!(err, BleError::Timeout(_)), "got {err:?}");
    assert_eq!(began.elapsed(), Duration::from_millis(600));
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_write_does_not_shorten_the_next_spacing() {
    let spacing = Duration::from_millis(100);
    let config = SessionConfig {
        write_spacing: spacing,
        ..SessionConfig::default()
    };
    let (mut s, writes) = stamped_session(config).await;
    tokio::time::sleep(spacing).await;
    // The first write goes out and its call returns at once.
    let _ = tokio::time::timeout(Duration::from_millis(50), s.set_no_wait(volume(1))).await;
    // A second is dropped in its spacing wait, and writes nothing.
    let _ = tokio::time::timeout(Duration::from_millis(10), s.set_no_wait(volume(2))).await;
    s.set_no_wait(volume(3)).await.unwrap();
    s.set_no_wait(volume(4)).await.unwrap();
    let times = writes.lock().unwrap().clone();
    assert_eq!(times.len(), 3, "the dropped second write sent nothing");
    for pair in times.windows(2) {
        assert!(
            pair[1] - pair[0] >= spacing,
            "two writes {:?} apart",
            pair[1] - pair[0]
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_set_that_timed_out_leaves_the_spacing_intact() {
    let config = SessionConfig {
        write_spacing: Duration::from_millis(100),
        set_timeout: Duration::from_millis(50),
        ..SessionConfig::default()
    };
    let (mut s, writes) = stamped_session(config).await;
    // The refresh's last write is out of the way, so the set writes at once; its timeout lands
    // inside the spacing after that write.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let err = s
        .set(Message::command(FunctionId::one(200), Value::Data(vec![])))
        .await
        .unwrap_err();
    assert!(matches!(err, BleError::Timeout(_)), "got {err:?}");
    s.set_no_wait(volume(1)).await.unwrap();
    let times = writes.lock().unwrap().clone();
    assert_eq!(times.len(), 2);
    assert!(times[1] - times[0] >= Duration::from_millis(100));
}

fn numbered(from: usize, count: usize) -> Vec<Message> {
    (from..from + count)
        .map(|i| {
            Message::command(
                FunctionId::one(1),
                Value::Data(vec![(i >> 8) as u8, i as u8]),
            )
        })
        .collect()
}

fn number(m: &Message) -> usize {
    match &m.value {
        Value::Data(d) => ((d[0] as usize) << 8) | d[1] as usize,
        other => panic!("not a numbered message: {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn pending_holds_its_cap_and_drops_the_oldest_with_one_warning_per_run() {
    let (mut s, _) = stamped_session(SessionConfig::default()).await;
    // Many small batches, as repeated `set` calls queue them.
    for batch in 0..(PENDING_CAP + 50) / 5 {
        s.queue_pending(numbered(batch * 5, 5));
    }
    let total = (PENDING_CAP + 50) / 5 * 5;
    assert_eq!(s.pending.len(), PENDING_CAP);
    assert_eq!(number(s.pending.front().unwrap()), total - PENDING_CAP);
    assert_eq!(number(s.pending.back().unwrap()), total - 1);
    assert_eq!(s.overflow_runs, 1, "one warning for the whole run");

    // Draining it empty through `recv` ends the run; the next overflow is a new one.
    while !s.pending.is_empty() {
        s.recv().await.unwrap();
    }
    s.queue_pending(numbered(0, PENDING_CAP + 1));
    assert_eq!(s.overflow_runs, 2);
}

#[tokio::test(start_paused = true)]
async fn recv_draining_the_queue_ends_the_overflow_run() {
    let (mut s, _) = stamped_session(SessionConfig::default()).await;
    s.queue_pending(numbered(0, PENDING_CAP + 3));
    assert_eq!(s.overflow_runs, 1);
    for _ in 0..PENDING_CAP {
        s.recv().await.unwrap();
    }
    assert!(!s.overflowing);
}

/// A link that hands the frame to the device at once and answers `latency` later, as a D-Bus
/// round trip does.
struct SlowLink {
    inner: Box<dyn Link>,
    latency: Duration,
}

struct SlowTransport(StampedTransport, Duration);

#[async_trait::async_trait]
impl Transport for SlowTransport {
    async fn scan(&self, window: Duration) -> Result<crate::Advertisements, BleError> {
        self.0.scan(window).await
    }

    async fn connect(&self, unit: svolder_proto::UnitId) -> Result<Box<dyn Link>, BleError> {
        let inner = self.0.connect(unit).await?;
        Ok(Box::new(SlowLink {
            inner,
            latency: self.1,
        }))
    }
}

#[async_trait::async_trait]
impl Link for SlowLink {
    async fn write(&self, frame: &[u8]) -> Result<(), BleError> {
        self.inner.write(frame).await?;
        tokio::time::sleep(self.latency).await;
        Ok(())
    }

    fn notifications(&self) -> Notifications {
        self.inner.notifications()
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        self.inner.disconnect().await
    }
}

/// A connected session over a `SlowLink`, with the connect's own writes cleared and its spacing
/// passed.
async fn slow_session(spacing: Duration, latency: Duration) -> (Session, Stamps, Stamps) {
    let profile = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let transport = FakeTransport::from_profile(&profile, unit, 3, Passphrase::NONE);
    let advert = Advertisement {
        manufacturer: transport.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let writes = Arc::new(Mutex::new(Vec::new()));
    let disconnects = Arc::new(Mutex::new(Vec::new()));
    let slow = SlowTransport(
        StampedTransport {
            inner: transport,
            writes: Arc::clone(&writes),
            disconnects: Arc::clone(&disconnects),
        },
        latency,
    );
    let config = SessionConfig {
        write_spacing: spacing,
        ..SessionConfig::default()
    };
    let mut s = Session::with_config(Arc::new(slow), advert, profile, Passphrase::NONE, config);
    s.connect().await.unwrap();
    tokio::time::sleep(spacing).await;
    writes.lock().unwrap().clear();
    (s, writes, disconnects)
}

#[tokio::test(start_paused = true)]
async fn a_write_dropped_inside_the_link_still_spaces_the_next() {
    let spacing = Duration::from_millis(100);
    let (mut s, writes, _) = slow_session(spacing, Duration::from_millis(10)).await;
    let dropped = tokio::time::timeout(Duration::from_millis(5), s.set_no_wait(volume(1))).await;
    assert!(
        dropped.is_err(),
        "the call was dropped inside the link's write"
    );
    s.set_no_wait(volume(2)).await.unwrap();
    let times = writes.lock().unwrap().clone();
    assert_eq!(times.len(), 2);
    assert!(
        times[1] - times[0] >= spacing,
        "two writes {:?} apart",
        times[1] - times[0]
    );
}

#[tokio::test(start_paused = true)]
async fn close_waits_out_the_spacing_before_it_disconnects() {
    let spacing = Duration::from_millis(100);
    let (mut s, writes, disconnects) = slow_session(spacing, Duration::ZERO).await;
    s.set_no_wait(volume(1)).await.unwrap();
    let wrote = writes.lock().unwrap()[0];
    s.close().await;
    let disconnected = disconnects.lock().unwrap()[0];
    assert!(
        disconnected - wrote >= spacing,
        "the link was disconnected {:?} after the last write",
        disconnected - wrote
    );
}

/// How long after its write a refresh burst ended.
async fn refresh_burst(s: &mut Session, writes: &Stamps) -> Duration {
    let before = writes.lock().unwrap().len();
    s.refresh().await.unwrap();
    let wrote = writes.lock().unwrap()[before];
    Instant::now() - wrote
}

#[tokio::test(start_paused = true)]
async fn a_new_profile_changes_what_the_next_refresh_burst_ends_on() {
    let config = SessionConfig {
        drain_idle: Duration::from_millis(300),
        ..SessionConfig::default()
    };
    let (mut s, writes) = stamped_session(config).await;
    let before = refresh_burst(&mut s, &writes).await;
    assert!(
        before < Duration::from_millis(300),
        "every function the profile names reports at once, so the burst ends complete: {before:?}"
    );

    // A function the device never reports: the burst can only end on the quiet gap.
    let mut next = s.profile().clone();
    let mut silent = next.controls[0].clone();
    silent.id = FunctionId::one(200);
    next.controls.push(silent);
    s.set_profile(next);
    let after = refresh_burst(&mut s, &writes).await;
    assert!(
        after >= Duration::from_millis(300),
        "the burst waited for the new profile's function: {after:?}"
    );
}

/// A connected session over a tapped fake link, the device holding `device_code` and the
/// session sending `session_code`. The tap has the connect's own frames in it.
async fn tapped_session(device_code: Passphrase, session_code: Passphrase) -> (Session, FrameTap) {
    let profile = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    tapped_session_of(profile, device_code, session_code).await
}

/// [`tapped_session`] for another profile.
async fn tapped_session_of(
    profile: Profile,
    device_code: Passphrase,
    session_code: Passphrase,
) -> (Session, FrameTap) {
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let transport = FakeTransport::from_profile(&profile, unit, 3, device_code);
    let advert = Advertisement {
        manufacturer: transport.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let tap = FrameTap::new();
    let tapped = TappedTransport::new(Arc::new(transport), tap.clone());
    let mut s = Session::new(Arc::new(tapped), advert, profile, session_code);
    s.connect().await.unwrap();
    (s, tap)
}

/// When a device answers a firmware ask.
#[derive(Clone, Copy)]
enum Report {
    /// As soon as it has the ask.
    Now,
    /// That long after it has the ask.
    After(Duration),
    /// Never: the ask is lost on its way.
    Never,
}

/// A link that holds back, or loses, what a firmware ask would bring.
struct ReportingLink {
    inner: Arc<dyn Link>,
    report: Arc<Mutex<Report>>,
}

struct ReportingTransport {
    inner: Arc<FakeTransport>,
    report: Arc<Mutex<Report>>,
}

#[async_trait::async_trait]
impl Transport for ReportingTransport {
    async fn scan(&self, window: Duration) -> Result<crate::Advertisements, BleError> {
        self.inner.scan(window).await
    }

    async fn connect(&self, unit: svolder_proto::UnitId) -> Result<Box<dyn Link>, BleError> {
        let inner: Arc<dyn Link> = Arc::from(self.inner.connect(unit).await?);
        Ok(Box::new(ReportingLink {
            inner,
            report: Arc::clone(&self.report),
        }))
    }
}

#[async_trait::async_trait]
impl Link for ReportingLink {
    async fn write(&self, frame: &[u8]) -> Result<(), BleError> {
        let asks = svolder_proto::decode_command(frame)
            .unwrap()
            .body
            .messages
            .iter()
            .any(|m| m.id.first() == 210);
        let report = *self.report.lock().unwrap();
        match (asks, report) {
            (true, Report::Never) => Ok(()),
            (true, Report::After(late)) => {
                let (inner, frame) = (Arc::clone(&self.inner), frame.to_vec());
                tokio::spawn(async move {
                    tokio::time::sleep(late).await;
                    let _ = inner.write(&frame).await;
                });
                Ok(())
            }
            _ => self.inner.write(frame).await,
        }
    }

    fn notifications(&self) -> Notifications {
        self.inner.notifications()
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        self.inner.disconnect().await
    }
}

/// A connected session over a tapped link whose device answers firmware asks as the returned
/// [`Report`] says (at once to begin with).
async fn reporting_session() -> (Session, FrameTap, Arc<Mutex<Report>>) {
    let (s, tap, report, _) = reporting_session_over_fake().await;
    (s, tap, report)
}

/// [`reporting_session`], with the fake transport too, to push statuses the device says on its
/// own.
async fn reporting_session_over_fake() -> (Session, FrameTap, Arc<Mutex<Report>>, Arc<FakeTransport>)
{
    let profile = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let fake = Arc::new(FakeTransport::from_profile(
        &profile,
        unit,
        3,
        Passphrase::NONE,
    ));
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let report = Arc::new(Mutex::new(Report::Now));
    let reporting = ReportingTransport {
        inner: Arc::clone(&fake),
        report: Arc::clone(&report),
    };
    let tap = FrameTap::new();
    let tapped = TappedTransport::new(Arc::new(reporting), tap.clone());
    let mut s = Session::new(Arc::new(tapped), advert, profile, Passphrase::NONE);
    s.connect().await.unwrap();
    (s, tap, report, fake)
}

/// The messages of every frame the session has sent so far, one entry per frame.
fn sent_messages(tap: &FrameTap) -> Vec<Vec<Message>> {
    tap.frames()
        .iter()
        .filter(|f| f.direction == FrameDirection::Sent)
        .map(|f| {
            svolder_proto::decode_command(&f.bytes)
                .unwrap()
                .body
                .messages
        })
        .collect()
}

fn identify_frames(tap: &FrameTap) -> usize {
    sent_messages(tap)
        .iter()
        .filter(|m| m.iter().any(|m| m.id == FunctionId::one(200)))
        .count()
}

#[tokio::test(start_paused = true)]
async fn identify_sends_one_frame_per_gap() {
    let (mut s, tap) = tapped_session(Passphrase::NONE, Passphrase::NONE).await;
    let before = sent_messages(&tap).len();
    assert!(s.identify().await.unwrap());
    let sent = Instant::now();
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert!(!s.identify().await.unwrap());
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert!(!s.identify().await.unwrap());
    let frames = sent_messages(&tap);
    assert_eq!(frames.len(), before + 1, "one frame went out in 1.8 s");
    let frame = tap
        .frames()
        .into_iter()
        .rfind(|f| f.direction == FrameDirection::Sent)
        .unwrap();
    // Header 0x01: Command, Data, one function id.
    let decoded = svolder_proto::decode_command(&frame.bytes).unwrap();
    assert_eq!(
        decoded.body.messages,
        vec![Message::command(FunctionId::one(200), Value::Data(vec![]))]
    );
    assert_eq!(decoded.body.messages[0].header().encode(), 0x01);

    tokio::time::sleep(IDENTIFY_GAP - sent.elapsed()).await;
    assert_eq!(sent.elapsed(), IDENTIFY_GAP);
    assert!(s.identify().await.unwrap());
    assert_eq!(identify_frames(&tap), 2);
    assert!(
        !sent_messages(&tap)[before..]
            .iter()
            .flatten()
            .any(|m| m.id == FunctionId::one(1)),
        "identify sends no refresh"
    );
}

#[tokio::test(start_paused = true)]
async fn identify_goes_out_on_a_refused_link() {
    let device = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let wrong = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let (mut s, tap) = tapped_session(device, wrong).await;
    assert!(s.refused());
    assert!(s.set_no_wait(volume(1)).await.is_err());
    assert!(s.identify().await.unwrap());
    assert_eq!(identify_frames(&tap), 1);
}

#[tokio::test(start_paused = true)]
async fn identify_without_a_link_is_an_error_and_a_new_link_starts_with_no_gap() {
    let (mut s, tap) = tapped_session(Passphrase::NONE, Passphrase::NONE).await;
    assert!(s.identify().await.unwrap());
    s.close().await;
    assert!(s.identify().await.is_err());
    s.connect().await.unwrap();
    assert!(s.identify().await.unwrap(), "a new link has no gap");
    assert_eq!(identify_frames(&tap), 2);
}

/// The ids of the messages in the frames the session sent, in order, one list per frame.
fn sent_ids(tap: &FrameTap) -> Vec<Vec<FunctionId>> {
    sent_messages(tap)
        .iter()
        .map(|frame| frame.iter().map(|m| m.id).collect())
        .collect()
}

/// Read until the session has been quiet for longer than any gap of the firmware ask.
async fn read_until_quiet(s: &mut Session) {
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(500), s.recv()).await {}
}

/// Each firmware ask the session sent, with the time it went out, by subsystem.
fn firmware_asks(tap: &FrameTap) -> Vec<(Duration, u8)> {
    tap.frames()
        .into_iter()
        .filter(|f| f.direction == FrameDirection::Sent)
        .flat_map(|f| {
            let at = f.at;
            svolder_proto::decode_command(&f.bytes)
                .unwrap()
                .body
                .messages
                .into_iter()
                .filter(|m| m.id.first() == 210)
                .map(move |m| (at, m.id.third().expect("a subsystem")))
        })
        .collect()
}

/// Each firmware report the session read, with the time it read it, by subsystem.
fn firmware_reports(tap: &FrameTap) -> Vec<(Duration, u8)> {
    tap.frames()
        .into_iter()
        .filter(|f| f.direction == FrameDirection::Received)
        .flat_map(|f| {
            let at = f.at;
            svolder_proto::decode_notification(&f.bytes)
                .unwrap()
                .messages
                .into_iter()
                .filter(|m| m.id.first() == 210)
                .map(move |m| (at, m.id.third().expect("a subsystem")))
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn a_connect_asks_each_listed_subsystem_once() {
    let (mut s, tap) = tapped_session(Passphrase::NONE, Passphrase::NONE).await;
    let ask = |sub| vec![FunctionId::three(210, 2, sub)];
    let refresh = vec![FunctionId::one(1)];
    assert_eq!(
        sent_ids(&tap),
        std::slice::from_ref(&refresh),
        "the connect sends the refresh"
    );
    read_until_quiet(&mut s).await;
    assert_eq!(
        sent_ids(&tap),
        [refresh.clone(), ask(1), ask(2)],
        "the refresh first, then one ask per subsystem, in order"
    );
    let first = sent_messages(&tap)[1][0].clone();
    assert_eq!(
        first,
        Message::command(FunctionId::three(210, 2, 1), Value::Data(vec![]))
    );
    // A reconnect is a new link and asks again, after its own refresh.
    s.connect().await.unwrap();
    read_until_quiet(&mut s).await;
    assert_eq!(
        sent_ids(&tap),
        [refresh.clone(), ask(1), ask(2), refresh, ask(1), ask(2)]
    );
}

/// How long a firmware ask waits, and for what: the first 300 ms after the connect's refresh,
/// and not a moment sooner.
#[tokio::test(start_paused = true)]
async fn the_first_ask_goes_out_300_ms_after_the_refresh() {
    let (mut s, tap) = tapped_session(Passphrase::NONE, Passphrase::NONE).await;
    let armed = tap.elapsed();
    let refresh = vec![vec![FunctionId::one(1)]];
    assert!(
        tokio::time::timeout(FIRMWARE_ASK_DELAY - Duration::from_millis(1), s.recv())
            .await
            .is_err(),
        "nothing is read inside the delay"
    );
    assert_eq!(sent_ids(&tap), refresh, "no ask inside the delay");
    let heard = tokio::time::timeout(Duration::from_secs(1), s.recv())
        .await
        .expect("recv() hung")
        .expect("the first report");
    assert_eq!(heard.id, FunctionId::three(210, 2, 1));
    assert_eq!(firmware_asks(&tap), [(armed + FIRMWARE_ASK_DELAY, 1)]);
}

/// A device that holds its report back for a second: the next ask waits for the report, then
/// for the delay after it.
#[tokio::test(start_paused = true)]
async fn the_next_ask_goes_out_300_ms_after_the_previous_report() {
    let (mut s, tap, report) = reporting_session().await;
    let armed = tap.elapsed();
    let late = Duration::from_secs(1);
    *report.lock().unwrap() = Report::After(late);
    for sub in [1, 2] {
        let heard = tokio::time::timeout(Duration::from_secs(5), s.recv())
            .await
            .expect("recv() hung")
            .expect("a firmware report");
        assert_eq!(heard.id, FunctionId::three(210, 2, sub));
    }
    let asks = firmware_asks(&tap);
    let reports = firmware_reports(&tap);
    assert_eq!(asks[0], (armed + FIRMWARE_ASK_DELAY, 1));
    assert_eq!(
        reports[0],
        (armed + FIRMWARE_ASK_DELAY + late, 1),
        "the report came a second after its ask"
    );
    assert_eq!(
        asks[1],
        (reports[0].0 + FIRMWARE_ASK_DELAY, 2),
        "the second ask waited for the first report, then the delay"
    );
    assert_eq!(asks.len(), 2);
}

/// Only the report of the subsystem asked last starts the next ask's delay: another status, or
/// the report of another subsystem, arriving while it is awaited does not.
#[tokio::test(start_paused = true)]
async fn only_the_awaited_report_starts_the_next_asks_delay() {
    let (mut s, tap, report, fake) = reporting_session_over_fake().await;
    let armed = tap.elapsed();
    let late = Duration::from_secs(1);
    *report.lock().unwrap() = Report::After(late);
    let first_ask = armed + FIRMWARE_ASK_DELAY;
    let report_at = first_ask + late;
    // While 210.2.1 is awaited, a volume and the report of 210.2.2 arrive.
    tokio::spawn(async move {
        tokio::time::sleep(FIRMWARE_ASK_DELAY + Duration::from_millis(100)).await;
        fake.push(&[Message::status(FunctionId::one(10), Value::U8(42))]);
        tokio::time::sleep(Duration::from_millis(100)).await;
        fake.push(&[Message::status(FunctionId::three(210, 2, 2), Value::U16(9))]);
    });
    let mut heard = Vec::new();
    while heard.len() < 4 {
        let m = tokio::time::timeout(Duration::from_secs(5), s.recv())
            .await
            .expect("recv() hung")
            .expect("a status");
        heard.push(m.id);
    }
    assert_eq!(
        heard,
        [
            FunctionId::one(10),
            FunctionId::three(210, 2, 2),
            FunctionId::three(210, 2, 1),
            FunctionId::three(210, 2, 2),
        ],
        "the volume, the stray report, the awaited report, then the report of the second ask"
    );
    let asks = firmware_asks(&tap);
    assert_eq!(asks[0], (first_ask, 1));
    assert_eq!(
        asks[1],
        (report_at + FIRMWARE_ASK_DELAY, 2),
        "the second ask followed the awaited report, not the volume or the stray report"
    );
}

/// The app neither retries nor times out: a report that never comes ends the chain.
#[tokio::test(start_paused = true)]
async fn a_report_that_never_comes_leaves_the_later_subsystems_unasked() {
    let (mut s, tap, report) = reporting_session().await;
    *report.lock().unwrap() = Report::Never;
    assert!(
        tokio::time::timeout(Duration::from_secs(600), s.recv())
            .await
            .is_err(),
        "nothing is heard"
    );
    let asked: Vec<u8> = firmware_asks(&tap)
        .into_iter()
        .map(|(_, sub)| sub)
        .collect();
    assert_eq!(asked, [1], "one ask, never repeated, and no second");
    // Still the one ask after any more time.
    let _ = tokio::time::timeout(Duration::from_secs(3600), s.recv()).await;
    assert_eq!(firmware_asks(&tap).len(), 1);
}

/// A link that goes in the middle of the chain takes it with it: the next link asks again from
/// the first subsystem.
#[tokio::test(start_paused = true)]
async fn a_link_lost_mid_chain_starts_again_from_the_first_subsystem() {
    let (mut s, tap) = tapped_session(Passphrase::NONE, Passphrase::NONE).await;
    let heard = tokio::time::timeout(Duration::from_secs(1), s.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(heard.id, FunctionId::three(210, 2, 1));
    // The second ask is a delay away when the link goes.
    s.close().await;
    assert!(s.recv().await.is_none());
    assert_eq!(firmware_asks(&tap).len(), 1);
    s.connect().await.unwrap();
    read_until_quiet(&mut s).await;
    let asked: Vec<u8> = firmware_asks(&tap)
        .into_iter()
        .map(|(_, sub)| sub)
        .collect();
    assert_eq!(asked, [1, 1, 2], "the new link began again with the first");
}

/// Reads dropped at any point of the chain neither lose an ask nor send one twice.
#[tokio::test(start_paused = true)]
async fn reads_dropped_during_the_chain_lose_and_double_no_ask() {
    let (mut s, tap) = tapped_session(Passphrase::NONE, Passphrase::NONE).await;
    // Every 100 ms the read is dropped; the waits for the delays are crossed in pieces.
    for _ in 0..30 {
        let _ = tokio::time::timeout(Duration::from_millis(100), s.recv()).await;
    }
    let asked: Vec<u8> = firmware_asks(&tap)
        .into_iter()
        .map(|(_, sub)| sub)
        .collect();
    assert_eq!(asked, [1, 2]);
}

#[tokio::test(start_paused = true)]
async fn a_profile_without_subsystems_asks_nothing() {
    let mut profile = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    profile.firmware_subsystems.clear();
    let (_s, tap) = tapped_session_of(profile, Passphrase::NONE, Passphrase::NONE).await;
    assert_eq!(sent_ids(&tap), [vec![FunctionId::one(1)]]);
}

#[tokio::test(start_paused = true)]
async fn the_answers_become_readings() {
    let (mut s, _tap) = tapped_session(Passphrase::NONE, Passphrase::NONE).await;
    let mut heard = Vec::new();
    while let Ok(Some(m)) = tokio::time::timeout(Duration::from_millis(500), s.recv()).await {
        heard.push(m);
    }
    for sub in [1, 2] {
        let id = FunctionId::three(210, 2, sub);
        assert!(
            heard.iter().any(|m| m.id == id && m.value == Value::U16(3)),
            "210.2.{sub} came out of recv with the fake's build: {heard:?}"
        );
        assert_eq!(s.state().get(id).map(|m| &m.value), Some(&Value::U16(3)));
    }
}

#[tokio::test(start_paused = true)]
async fn a_refused_link_asks_nothing_until_the_code_is_corrected() {
    let device = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let wrong = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let (mut s, tap) = tapped_session(device, wrong).await;
    assert!(s.refused());
    // Not even once the delay of the first ask is long past.
    let _ = tokio::time::timeout(Duration::from_secs(5), s.recv()).await;
    assert_eq!(
        sent_ids(&tap),
        [vec![FunctionId::one(1)]],
        "a refused link asks no firmware"
    );
    // The corrected code lifts the refusal and owes a refresh; the ask follows it, once.
    s.set_passphrase(device);
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(500), s.recv()).await {}
    let ask = |sub| vec![FunctionId::three(210, 2, sub)];
    assert_eq!(
        sent_ids(&tap),
        [
            vec![FunctionId::one(1)],
            vec![FunctionId::one(1)],
            ask(1),
            ask(2)
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_device_coming_back_on_refreshes_without_asking_firmware_again() {
    let (mut s, tap) = tapped_session(Passphrase::NONE, Passphrase::NONE).await;
    read_until_quiet(&mut s).await;
    let before = sent_ids(&tap).len();
    s.refresh_due = true;
    s.flush_refresh().await.unwrap();
    let after = sent_ids(&tap);
    assert_eq!(after.len(), before + 1);
    assert_eq!(after[before], [FunctionId::one(1)]);
}

#[tokio::test(start_paused = true)]
async fn an_ask_dropped_in_its_write_spacing_is_written_by_the_next_call() {
    let (mut s, tap) = tapped_session(Passphrase::NONE, Passphrase::NONE).await;
    let ask = |sub| vec![FunctionId::three(210, 2, sub)];
    let connected = sent_ids(&tap).len();
    // The ask falls due just after another write, so it waits out the write spacing.
    tokio::time::sleep(FIRMWARE_ASK_DELAY).await;
    s.set_no_wait(volume(1)).await.unwrap();
    let cut = tokio::time::timeout(Duration::from_millis(10), s.flush_refresh()).await;
    assert!(cut.is_err(), "the ask was still waiting out its spacing");
    assert_eq!(sent_ids(&tap)[connected..], [vec![FunctionId::one(10)]]);
    // The next call writes it, once; and sends nothing more while the report is awaited.
    s.flush_refresh().await.unwrap();
    s.flush_refresh().await.unwrap();
    assert_eq!(
        sent_ids(&tap)[connected..],
        [vec![FunctionId::one(10)], ask(1)]
    );
}

/// A set is not held up by a firmware ask that is due, nor by the wait for one, and the ask
/// then goes out at the next read.
#[tokio::test(start_paused = true)]
async fn a_set_is_not_held_up_by_the_chain() {
    let (mut s, tap) = tapped_session(Passphrase::NONE, Passphrase::NONE).await;
    let connected = sent_ids(&tap).len();
    // Inside the delay.
    let began = Instant::now();
    s.set(volume(5)).await.unwrap();
    assert!(
        began.elapsed() < FIRMWARE_ASK_DELAY,
        "took {:?}",
        began.elapsed()
    );
    // With the ask due.
    tokio::time::sleep(FIRMWARE_ASK_DELAY).await;
    let began = Instant::now();
    s.set(volume(6)).await.unwrap();
    assert!(
        began.elapsed() < s.config.write_spacing,
        "took {:?}",
        began.elapsed()
    );
    assert_eq!(
        sent_ids(&tap)[connected..],
        [vec![FunctionId::one(10)], vec![FunctionId::one(10)]],
        "no ask went out before or between the sets"
    );
    let heard = tokio::time::timeout(Duration::from_secs(1), s.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(heard.id, FunctionId::three(210, 2, 1));
    assert_eq!(firmware_asks(&tap).len(), 1);
}

/// A reading that arrives while the next ask waits for its time is handed over at once, and
/// the ask still goes out on time.
#[tokio::test(start_paused = true)]
async fn a_reading_is_not_held_up_by_the_wait_for_an_ask() {
    let profile = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let transport = Arc::new(FakeTransport::from_profile(
        &profile,
        unit,
        3,
        Passphrase::NONE,
    ));
    let advert = Advertisement {
        manufacturer: transport.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let tap = FrameTap::new();
    let tapped = TappedTransport::new(Arc::clone(&transport) as Arc<dyn Transport>, tap.clone());
    let mut s = Session::new(Arc::new(tapped), advert, profile, Passphrase::NONE);
    s.connect().await.unwrap();
    let armed = tap.elapsed();
    let sooner = Duration::from_millis(100);
    tokio::spawn(async move {
        tokio::time::sleep(sooner).await;
        transport.push(&[Message::status(FunctionId::one(10), Value::U8(42))]);
    });
    let heard = s.recv().await.unwrap();
    assert_eq!(heard.id, FunctionId::one(10));
    assert_eq!(tap.elapsed(), armed + sooner, "handed over when it arrived");
    let heard = s.recv().await.unwrap();
    assert_eq!(heard.id, FunctionId::three(210, 2, 1));
    assert_eq!(firmware_asks(&tap), [(armed + FIRMWARE_ASK_DELAY, 1)]);
}

#[tokio::test(start_paused = true)]
async fn the_answers_to_a_refresh_written_under_another_code_prove_nothing() {
    let device = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let wrong = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let (mut s, _tap) = tapped_session(device, wrong).await;
    assert!(s.refused());
    // The corrected code owes a refresh, which goes out under it...
    s.set_passphrase(device);
    s.flush_refresh().await.unwrap();
    // ...and the code changes again before its answers are read.
    s.set_passphrase(wrong);
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(500), s.recv()).await {}
    assert!(!s.code_accepted());
}

#[tokio::test(start_paused = true)]
async fn a_corrected_code_asks_once_it_is_proven_and_a_wrong_correction_spends_nothing() {
    let device = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let refused = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let wrong_again = Passphrase::from_digits([8, 8, 8, 8]).unwrap();
    let (mut s, tap) = tapped_session(device, refused).await;
    assert!(s.refused());
    let firmware_frames = |tap: &FrameTap| {
        sent_ids(tap)
            .into_iter()
            .flatten()
            .filter(|id| id.first() == 210)
            .collect::<Vec<_>>()
    };

    // A correction the device refuses again sends its refresh and nothing else.
    s.set_passphrase(wrong_again);
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(500), s.recv()).await {}
    assert!(s.refused());
    assert_eq!(
        sent_ids(&tap),
        [vec![FunctionId::one(1)], vec![FunctionId::one(1)]]
    );
    assert!(firmware_frames(&tap).is_empty());

    // The right code: its refresh, then, once its burst has proven it, one ask per subsystem.
    s.set_passphrase(device);
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(500), s.recv()).await {}
    assert!(s.code_accepted());
    assert_eq!(
        firmware_frames(&tap),
        [FunctionId::three(210, 2, 1), FunctionId::three(210, 2, 2)]
    );
    let frames = tap.frames();
    let ask_at = frames
        .iter()
        .position(|f| {
            f.direction == FrameDirection::Sent
                && svolder_proto::decode_command(&f.bytes)
                    .unwrap()
                    .body
                    .messages
                    .iter()
                    .any(|m| m.id.first() == 210)
        })
        .unwrap();
    let last_refresh = frames[..ask_at]
        .iter()
        .rposition(|f| {
            f.direction == FrameDirection::Sent
                && svolder_proto::decode_command(&f.bytes)
                    .unwrap()
                    .body
                    .messages
                    .iter()
                    .any(|m| m.id == FunctionId::one(1))
        })
        .unwrap();
    assert!(
        frames[last_refresh..ask_at]
            .iter()
            .any(|f| f.direction == FrameDirection::Received),
        "the burst that proved the code was read before the ask"
    );
}

/// A profile that names no function has nothing a refresh burst could prove a code with, so a
/// corrected code that lifted a refusal defers the firmware ask until a set's echo proves it:
/// then, and not before, each subsystem is asked, once.
#[tokio::test(start_paused = true)]
async fn a_corrected_code_proven_by_a_sets_echo_then_asks_for_the_firmware_once() {
    let device = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let refused = Passphrase::from_digits([9, 9, 9, 9]).unwrap();
    let full = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    // The device has every function; the session's profile names none of them.
    let mut bare = full.clone();
    bare.controls.clear();
    bare.eq = None;
    let unit = "0102030405060708".parse::<UnitId>().unwrap();
    let transport = FakeTransport::from_profile(&full, unit, 3, device);
    let advert = Advertisement {
        manufacturer: transport.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let tap = FrameTap::new();
    let tapped = TappedTransport::new(Arc::new(transport), tap.clone());
    let mut s = Session::new(Arc::new(tapped), advert, bare, refused);
    s.connect().await.unwrap();
    assert!(s.refused());
    let asks = |tap: &FrameTap| {
        sent_messages(tap)
            .into_iter()
            .flatten()
            .filter(|m| m.id.first() == 210)
            .map(|m| m.id)
            .collect::<Vec<_>>()
    };
    assert!(asks(&tap).is_empty(), "a refused link asks nothing");

    // The corrected code lifts the refusal; its refresh awaits nothing, so nothing proves it.
    s.set_passphrase(device);
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(500), s.recv()).await {}
    assert!(!s.refused());
    assert!(!s.code_accepted());
    assert!(asks(&tap).is_empty(), "an unproven code defers the ask");

    // A set's echo proves it; the next read then asks, once per subsystem and no more.
    s.set(Message::command(FunctionId::one(10), Value::U8(100)))
        .await
        .expect("the device echoes");
    assert!(s.code_accepted());
    for _ in 0..2 {
        while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(500), s.recv()).await {}
    }
    assert_eq!(
        asks(&tap),
        [FunctionId::three(210, 2, 1), FunctionId::three(210, 2, 2)]
    );
}

/// A connected session over a tapped fake Loki Max. Its bands are reported on the short
/// two-id form when `short`, and a recall reports nothing when `silent`; `firmware` gives the
/// profile two firmware subsystems to ask for. The tap has the connect's own frames in it.
async fn loki_session(
    short: bool,
    silent: bool,
    firmware: bool,
    config: SessionConfig,
) -> (Session, FrameTap, Arc<FakeTransport>) {
    let registry = Registry::builtin().unwrap();
    let fake = Arc::new(
        FakeTransport::from_knobs(&registry, |name| match name {
            "SVOLDER_FAKE_PROFILE" => Some("lokimax".into()),
            "SVOLDER_FAKE_PASSPHRASE" => Some("none".into()),
            "SVOLDER_FAKE_LOKI_REPORTS" => short.then(|| "short".into()),
            "SVOLDER_FAKE_SILENT_RECALL" => silent.then(|| "1".into()),
            _ => None,
        })
        .unwrap(),
    );
    let mut profile = registry.get("lokimax").unwrap().clone();
    if firmware {
        profile.firmware_subsystems = vec![1, 2];
    }
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let tap = FrameTap::new();
    let tapped = TappedTransport::new(Arc::clone(&fake) as Arc<dyn Transport>, tap.clone());
    let mut s = Session::with_config(Arc::new(tapped), advert, profile, Passphrase::NONE, config);
    s.connect().await.unwrap();
    (s, tap, fake)
}

/// Read for `span` of virtual time, handing nothing back.
async fn read_for(s: &mut Session, span: Duration) {
    let end = Instant::now() + span;
    while let Ok(Some(_)) = tokio::time::timeout_at(end, s.recv()).await {}
}

/// When each function 1 the session sent went out.
fn refresh_times(tap: &FrameTap) -> Vec<Duration> {
    tap.frames()
        .into_iter()
        .filter(|f| f.direction == FrameDirection::Sent)
        .filter(|f| {
            svolder_proto::decode_command(&f.bytes)
                .unwrap()
                .body
                .messages
                .iter()
                .any(|m| m.id == FunctionId::one(1))
        })
        .map(|f| f.at)
        .collect()
}

/// The recall of `slot`, as `set recall <slot>` builds it.
fn recall(s: &Session, slot: &str) -> Message {
    svolder_profiles::intent(s.profile(), "recall", slot).unwrap()
}

#[tokio::test(start_paused = true)]
async fn a_band_reported_on_its_two_id_form_is_read_as_the_band() {
    let (mut s, _, fake) = loki_session(true, false, false, SessionConfig::default()).await;
    let band = FunctionId::three(6, 1, 3);
    let mut heard = Vec::new();
    // A fresh report of band 1 on its two-id form, and one of band 2 on a third id under its
    // prefix.
    fake.push(&[Message::status(FunctionId::two(6, 1), Value::U8(20))]);
    fake.push(&[Message::status(FunctionId::three(6, 2, 9), Value::U8(7))]);
    while let Ok(Some(m)) = tokio::time::timeout(Duration::from_secs(1), s.recv()).await {
        heard.push(m);
    }
    assert!(
        heard
            .iter()
            .all(|m| m.id.first() != 6 || m.id.third() == Some(3)),
        "recv hands out the band's own id and never the alias: {heard:?}"
    );
    assert!(heard.contains(&Message::status(band, Value::U8(20))));
    assert!(heard.contains(&Message::status(FunctionId::three(6, 2, 3), Value::U8(7))));
    assert_eq!(s.state().get(band).map(|m| &m.value), Some(&Value::U8(20)));
    assert!(s.state().get(FunctionId::two(6, 1)).is_none());
    assert!(s.state().get(FunctionId::three(6, 2, 9)).is_none());
}

#[tokio::test(start_paused = true)]
async fn a_refresh_whose_bands_come_on_the_two_id_form_completes_its_burst() {
    let before = Instant::now();
    let (mut s, tap, _) = loki_session(true, true, false, SessionConfig::default()).await;
    assert!(
        before.elapsed() < SessionConfig::default().drain_idle,
        "the connect's burst ends complete, not on a quiet gap"
    );
    assert!(s.code_accepted());
    // The refresh the recall guard writes counts its burst the same way: owed on `6.1.3`, it is
    // paid by the report on `6.1`.
    s.set_no_wait(recall(&s, "1")).await.unwrap();
    read_for(&mut s, Duration::from_secs(2)).await;
    assert_eq!(refresh_times(&tap).len(), 2);
    assert!(s.burst.is_none(), "the guard's refresh burst is complete");
}

#[tokio::test(start_paused = true)]
async fn a_write_on_the_two_id_form_is_confirmed_by_the_bands_report() {
    for short in [false, true] {
        let (mut s, _, _) = loki_session(short, false, false, SessionConfig::default()).await;
        let echo = s
            .set(Message::command(FunctionId::two(6, 1), Value::U8(20)))
            .await
            .unwrap_or_else(|e| panic!("short={short}: {e}"));
        assert_eq!(
            echo,
            Message::status(FunctionId::three(6, 1, 3), Value::U8(20))
        );
        assert_eq!(
            s.state().get(FunctionId::three(6, 1, 3)).map(|m| &m.value),
            Some(&Value::U8(20))
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_recall_sends_one_empty_data_frame_on_7_2_and_the_slot() {
    let (mut s, tap, _) = loki_session(false, false, false, SessionConfig::default()).await;
    let sent = tap.frames().len();
    s.set_no_wait(recall(&s, "2")).await.unwrap();
    let frames: Vec<_> = tap.frames()[sent..]
        .iter()
        .filter(|f| f.direction == FrameDirection::Sent)
        .cloned()
        .collect();
    assert_eq!(frames.len(), 1);
    // Four bytes of passphrase field, then the length, header 0x03 and the three ids.
    assert_eq!(frames[0].bytes[4..10], [4, 0, 0x03, 7, 2, 2]);
    assert_eq!(
        frames[0].bytes.len(),
        12,
        "no data bytes, then the checksum"
    );
    let decoded = svolder_proto::decode_command(&frames[0].bytes).unwrap();
    assert_eq!(
        decoded.body.messages,
        [Message::command(
            FunctionId::three(7, 2, 2),
            Value::Data(vec![])
        )]
    );
}

#[tokio::test(start_paused = true)]
async fn a_recall_whose_bands_are_reported_asks_for_no_refresh() {
    let (mut s, tap, _) = loki_session(false, false, false, SessionConfig::default()).await;
    s.set_no_wait(recall(&s, "2")).await.unwrap();
    read_for(&mut s, Duration::from_secs(2)).await;
    assert_eq!(refresh_times(&tap).len(), 1, "the connect's refresh alone");
}

#[tokio::test(start_paused = true)]
async fn a_recall_with_no_band_report_within_a_second_asks_for_one_refresh() {
    let (mut s, tap, _) = loki_session(false, true, false, SessionConfig::default()).await;
    let recalled = tap.elapsed();
    s.set_no_wait(recall(&s, "2")).await.unwrap();
    read_for(
        &mut s,
        (recalled + Duration::from_millis(999)).saturating_sub(tap.elapsed()),
    )
    .await;
    assert_eq!(refresh_times(&tap).len(), 1, "nothing at 999 ms");
    read_for(&mut s, Duration::from_millis(100)).await;
    assert_eq!(
        refresh_times(&tap)[1..],
        [recalled + Duration::from_secs(1)],
        "one refresh, at one second"
    );
    read_for(&mut s, Duration::from_secs(10)).await;
    assert_eq!(refresh_times(&tap).len(), 2, "still one");
}

#[tokio::test(start_paused = true)]
async fn a_save_arms_no_refresh() {
    let (mut s, tap, _) = loki_session(false, true, false, SessionConfig::default()).await;
    let save = svolder_profiles::intent(s.profile(), "save", "1").unwrap();
    s.set_no_wait(save).await.unwrap();
    read_for(&mut s, Duration::from_secs(10)).await;
    assert_eq!(refresh_times(&tap).len(), 1, "the connect's refresh alone");
}

#[tokio::test(start_paused = true)]
async fn a_raw_recall_frame_arms_the_guard_as_set_recall_does() {
    let (mut s, tap, _) = loki_session(false, true, false, SessionConfig::default()).await;
    let raw = Message::command(FunctionId::three(7, 2, 3), Value::Data(vec![]));
    assert_eq!(raw, recall(&s, "3"));
    let recalled = tap.elapsed();
    // The waited set, as the daemon's raw command takes it: no echo is awaited.
    let sent = s.set(raw.clone()).await.unwrap();
    assert_eq!(sent, raw);
    read_for(&mut s, Duration::from_secs(5)).await;
    assert_eq!(
        refresh_times(&tap)[1..],
        [recalled + Duration::from_secs(1)]
    );
}

#[tokio::test(start_paused = true)]
async fn a_waited_set_of_an_action_returns_at_once_and_arms_the_guard() {
    let (mut s, tap, _) = loki_session(false, true, false, SessionConfig::default()).await;
    let recalled = tap.elapsed();
    let began = Instant::now();
    let sent = s.set(recall(&s, "2")).await.unwrap();
    assert_eq!(sent, recall(&s, "2"));
    assert!(
        began.elapsed() < s.config().set_timeout,
        "no echo is waited for: {:?}",
        began.elapsed()
    );
    read_for(&mut s, Duration::from_secs(10)).await;
    assert_eq!(
        refresh_times(&tap)[1..],
        [recalled + Duration::from_secs(1)],
        "exactly one refresh, at one second"
    );
}

/// A guard that falls due while a waited set awaits its echo writes its refresh then.
#[tokio::test(start_paused = true)]
async fn a_guard_falling_due_during_a_waited_set_is_written_at_once() {
    let (mut s, tap, _) = loki_session(false, true, false, SessionConfig::default()).await;
    let recalled = tap.elapsed();
    s.set_no_wait(recall(&s, "2")).await.unwrap();
    // No device answers function 200, so this waits for its whole timeout.
    let err = s
        .set(Message::command(FunctionId::one(200), Value::Data(vec![])))
        .await
        .unwrap_err();
    assert!(matches!(err, BleError::Timeout(_)), "got {err:?}");
    assert!(tap.elapsed() >= recalled + Duration::from_secs(2));
    assert_eq!(
        refresh_times(&tap)[1..],
        [recalled + Duration::from_secs(1)],
        "the refresh went out at one second, not when the set ended"
    );
}

#[tokio::test(start_paused = true)]
async fn a_link_that_drops_forgets_the_guard() {
    let (mut s, tap, fake) = loki_session(false, true, false, SessionConfig::default()).await;
    s.set_no_wait(recall(&s, "2")).await.unwrap();
    assert!(s.guard.is_some());
    fake.sever();
    assert!(s.recv().await.is_none());
    assert!(s.guard.is_none());
    s.connect().await.unwrap();
    read_for(&mut s, Duration::from_secs(10)).await;
    assert_eq!(
        refresh_times(&tap).len(),
        2,
        "the connect and the reconnect"
    );
}

/// Both timed asks wait on the same stream, whichever is due first: the guard's refresh at one
/// second after the recall and the firmware ask at its delay after the connect's refresh.
#[tokio::test(start_paused = true)]
async fn the_firmware_ask_and_the_recall_guard_each_go_out_on_time_when_both_wait() {
    // The firmware asks fall due before the guard.
    let (mut s, tap, _) = loki_session(false, true, true, SessionConfig::default()).await;
    let armed = tap.elapsed();
    let recalled = tap.elapsed();
    s.set_no_wait(recall(&s, "2")).await.unwrap();
    read_for(&mut s, Duration::from_millis(1500)).await;
    assert_eq!(
        firmware_asks(&tap),
        [
            (armed + FIRMWARE_ASK_DELAY, 1),
            (armed + FIRMWARE_ASK_DELAY * 2, 2)
        ]
    );
    assert_eq!(
        refresh_times(&tap)[1..],
        [recalled + Duration::from_secs(1)]
    );

    // The guard falls due before the first firmware ask.
    let config = SessionConfig {
        firmware_delay: Duration::from_secs(2),
        ..SessionConfig::default()
    };
    let (mut s, tap, _) = loki_session(false, true, true, config).await;
    let armed = tap.elapsed();
    let recalled = tap.elapsed();
    s.set_no_wait(recall(&s, "2")).await.unwrap();
    read_for(&mut s, Duration::from_millis(2500)).await;
    assert_eq!(
        refresh_times(&tap)[1..],
        [recalled + Duration::from_secs(1)]
    );
    assert_eq!(firmware_asks(&tap), [(armed + Duration::from_secs(2), 1)]);
}

/// A link that loses every command carrying one function, as a device that never answers it.
struct DeafLink {
    inner: Arc<dyn Link>,
    deaf_to: FunctionId,
}

struct DeafTransport {
    inner: Arc<FakeTransport>,
    deaf_to: FunctionId,
}

#[async_trait::async_trait]
impl Transport for DeafTransport {
    async fn scan(&self, window: Duration) -> Result<crate::Advertisements, BleError> {
        self.inner.scan(window).await
    }

    async fn connect(&self, unit: svolder_proto::UnitId) -> Result<Box<dyn Link>, BleError> {
        let inner: Arc<dyn Link> = Arc::from(self.inner.connect(unit).await?);
        Ok(Box::new(DeafLink {
            inner,
            deaf_to: self.deaf_to,
        }))
    }
}

#[async_trait::async_trait]
impl Link for DeafLink {
    async fn write(&self, frame: &[u8]) -> Result<(), BleError> {
        let deaf = svolder_proto::decode_command(frame)
            .unwrap()
            .body
            .messages
            .iter()
            .any(|m| m.id == self.deaf_to);
        if deaf {
            return Ok(());
        }
        self.inner.write(frame).await
    }

    fn notifications(&self) -> Notifications {
        self.inner.notifications()
    }

    async fn disconnect(&self) -> Result<(), BleError> {
        self.inner.disconnect().await
    }
}

/// The refresh a guard writes while a set awaits its echo reports the awaited band too, and
/// that report is no echo: a device that never answers the write leaves the set to time out.
#[tokio::test(start_paused = true)]
async fn a_report_in_the_burst_of_a_guards_refresh_is_not_taken_for_the_echo() {
    let band = FunctionId::three(6, 1, 3);
    let registry = Registry::builtin().unwrap();
    let profile = registry.get("lokimax").unwrap().clone();
    let fake = Arc::new(
        FakeTransport::from_knobs(&registry, |name| match name {
            "SVOLDER_FAKE_PROFILE" => Some("lokimax".into()),
            "SVOLDER_FAKE_PASSPHRASE" => Some("none".into()),
            "SVOLDER_FAKE_SILENT_RECALL" => Some("1".into()),
            _ => None,
        })
        .unwrap(),
    );
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let deaf = DeafTransport {
        inner: Arc::clone(&fake),
        deaf_to: band,
    };
    let tap = FrameTap::new();
    let tapped = TappedTransport::new(Arc::new(deaf), tap.clone());
    let mut s = Session::new(Arc::new(tapped), advert, profile, Passphrase::NONE);
    s.connect().await.unwrap();
    let recalled = tap.elapsed();
    s.set_no_wait(recall(&s, "2")).await.unwrap();
    let err = s
        .set(Message::command(band, Value::U8(20)))
        .await
        .expect_err("the band's report in the guard's burst is not the echo");
    assert!(matches!(err, BleError::Timeout(_)), "got {err:?}");
    assert_eq!(
        refresh_times(&tap)[1..],
        [recalled + Duration::from_secs(1)],
        "the guard's refresh went out while the echo was awaited"
    );
}

/// A recall sent while a refresh is owed writes that refresh and hears its burst first, so the
/// burst's reports cannot cancel the guard; a recall that is then not reported gets its refresh.
#[tokio::test(start_paused = true)]
async fn a_recall_sent_while_a_refresh_is_owed_settles_the_burst_first() {
    let (mut s, tap, _) = loki_session(false, true, false, SessionConfig::default()).await;
    s.refresh_due = true;
    s.set_no_wait(recall(&s, "2")).await.unwrap();
    assert!(
        s.burst.is_none(),
        "the owed refresh's burst was heard out first"
    );
    let ids = sent_ids(&tap);
    let recall_id = FunctionId::three(7, 2, 2);
    assert_eq!(
        ids[1..],
        [vec![FunctionId::one(1)], vec![recall_id]],
        "the owed refresh, then the recall"
    );
    let recalled_at = tap
        .frames()
        .iter()
        .rfind(|f| f.direction == FrameDirection::Sent)
        .unwrap()
        .at;
    read_for(&mut s, Duration::from_secs(5)).await;
    let refreshes = refresh_times(&tap);
    assert_eq!(
        refreshes.len(),
        3,
        "the connect, the owed one and the guard's"
    );
    let guarded = refreshes[2] - recalled_at;
    assert!(
        guarded <= Duration::from_secs(1) && guarded >= Duration::from_millis(980),
        "the guard's refresh came a second after the recall: {guarded:?}"
    );
}

/// A session against a fake that reports changes only (`SVOLDER_FAKE_QUIET_SAME=1`), no code.
async fn quiet_session() -> Session {
    let registry = Registry::builtin().unwrap();
    let fake = Arc::new(
        FakeTransport::from_knobs(&registry, |name| match name {
            "SVOLDER_FAKE_PASSPHRASE" => Some("none".into()),
            "SVOLDER_FAKE_QUIET_SAME" => Some("1".into()),
            _ => None,
        })
        .unwrap(),
    );
    let profile = registry.get("gungnir2-v2").unwrap().clone();
    let advert = Advertisement {
        manufacturer: fake.device().lock().unwrap().manufacturer_data(),
        rssi: Some(-50),
        record: None,
    };
    let mut s = Session::with_config(
        fake,
        advert,
        profile,
        Passphrase::NONE,
        SessionConfig::default(),
    );
    s.connect().await.unwrap();
    s
}

/// A write of the value the device holds gets no echo from a unit that reports a change only: the
/// answer after the timeout is the device's own report of it, and a write of another value that is
/// not echoed is still the timeout.
#[tokio::test(start_paused = true)]
async fn a_write_of_the_held_value_with_no_echo_answers_with_the_held_report() {
    let mut s = quiet_session().await;
    let held = s.state().get(FunctionId::one(10)).cloned().expect("volume");
    let began = Instant::now();
    let answer = s
        .set(Message::command(held.id, held.value.clone()))
        .await
        .expect("the report");
    assert_eq!(answer, held);
    assert_eq!(
        began.elapsed(),
        s.config.set_timeout,
        "it still waits the timeout"
    );
    // A change is echoed as usual, and its repeat is the held report again.
    let changed = s.set(volume(77)).await.unwrap();
    assert_eq!(changed.value, Value::U8(77));
    assert_eq!(s.set(volume(77)).await.unwrap().value, Value::U8(77));
    // A link that came back marks every reading stale, and a refresh that missed this one marks
    // the state fresh again: what the device held on the earlier link is no reason to call a
    // repeat written now answered.
    s.state.set_stale(true);
    s.state.set_stale(false);
    let err = s.set(volume(77)).await.unwrap_err();
    assert!(matches!(err, BleError::Timeout(_)), "got {err:?}");
    // Not held, not echoed: an id the fake never reports and a value that differs from none.
    let err = s
        .set(Message::command(FunctionId::one(200), Value::Data(vec![])))
        .await
        .unwrap_err();
    assert!(matches!(err, BleError::Timeout(_)), "got {err:?}");
}
