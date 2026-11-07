// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use futures::{FutureExt, StreamExt};
use svolder_host::capture::FrameCapture;
use svolder_host::fake::{CaptureCall, FakeFrames, FakeSink};
use svolder_host::{Frame, PwFrames, SinkState};

use super::super::tests::{pass, settle};
use super::*;
use crate::host::{
    CaptureState, FakeKnobs, Frames, FramesEvent, FramesHold, HostMode, HostService, Parts,
};

const TARGET: &str = "alsa_output.usb-Schiit_Audio_Gungnir-00.analog-stereo";

/// What a sink reads when it is the DAC.
fn dac(name: &str, target: &str) -> SinkState {
    SinkState {
        volume: 0.5,
        muted: false,
        name: name.to_string(),
        target: Some(target.to_string()),
    }
}

/// A frame stamped `t`, which is how a test tells frames apart.
fn frame(t: u64) -> Frame {
    Frame {
        capture_ms: t,
        left: [0; 240],
        right: [0; 240],
        levels: [0; 4],
        beat: false,
        bpm: None,
        phase: 0,
    }
}

/// A host whose capture is the test's: a fake frames capture over a fake sink, and no bus.
struct Rig {
    service: HostService,
    frames: FakeFrames,
    sink: FakeSink,
}

fn rig() -> Rig {
    let frames = FakeFrames::new();
    let sink = FakeSink::new(Some(dac("Schiit Gungnir 2", TARGET)));
    let service = HostService::idle();
    service.run_with(parts(Ok(Frames {
        capture: Box::new(frames.clone()),
        sink: Arc::new(sink.clone()),
    })));
    Rig {
        service,
        frames,
        sink,
    }
}

/// Parts with no bus and the frames source `frames`.
fn parts(frames: Result<Frames, String>) -> Parts {
    Parts {
        connect: Arc::new(|| Box::pin(async { Err(svolder_host::NoBus::Absent) })),
        routes: None,
        covers: Arc::new(|_| None),
        helpers: Vec::new(),
        frames,
        computer: None,
    }
}

fn running(name: &str) -> CaptureState {
    CaptureState::Running {
        sink: name.to_string(),
        target: TARGET.to_string(),
    }
}

/// The next event of `events`, if one is ready; the clock does not move meanwhile.
fn ready(events: &mut futures::stream::BoxStream<'static, FramesEvent>) -> Option<FramesEvent> {
    events.next().now_or_never().flatten()
}

fn start() -> CaptureCall {
    CaptureCall::Start(TARGET.to_string())
}

/// The paused clock on by `secs` seconds, one at a time, so every timer in between fires on its own
/// second.
async fn pass_secs(secs: u64) {
    for _ in 0..secs {
        pass(Duration::from_secs(1)).await;
    }
}

#[tokio::test(start_paused = true)]
async fn capture_starts_with_the_first_hold_and_stops_2_s_after_the_last() {
    let rig = rig();
    settle().await;
    assert_eq!(rig.frames.calls(), []);
    assert_eq!(rig.sink.reads(), 0, "nobody looks, nothing is read");

    let hold = rig.service.frames().expect("a host that runs");
    let mut events = hold.events();
    assert!(matches!(
        ready(&mut events),
        Some(FramesEvent::Capture(CaptureState::Starting))
    ));
    settle().await;
    assert_eq!(rig.frames.calls(), [start()]);
    let Some(FramesEvent::Capture(state)) = ready(&mut events) else {
        panic!("the capture's state changed");
    };
    assert_eq!(state, running("Schiit Gungnir 2"));

    assert!(rig.frames.push(frame(7)));
    settle().await;
    let Some(FramesEvent::Frame(heard)) = ready(&mut events) else {
        panic!("a frame");
    };
    assert_eq!(heard.capture_ms, 7);

    drop(events);
    settle().await;
    pass(Duration::from_millis(1900)).await;
    assert_eq!(rig.frames.calls(), [start()], "not yet");
    pass(Duration::from_millis(200)).await;
    assert_eq!(rig.frames.calls(), [start(), CaptureCall::Stop]);
    let reads = rig.sink.reads();
    pass(Duration::from_secs(10)).await;
    assert_eq!(rig.sink.reads(), reads, "no reads once it stopped");
    assert_eq!(
        rig.service.frames().expect("a host").state(),
        CaptureState::Starting
    );
    assert_eq!(rig.sink.writes(), [], "the daemon only reads the sink");
}

#[tokio::test(start_paused = true)]
async fn two_pages_share_one_capture() {
    let rig = rig();
    let first = rig.service.frames().expect("a host");
    let second = rig.service.frames().expect("a host");
    settle().await;
    assert_eq!(rig.frames.calls(), [start()], "one capture for two holds");
    assert_eq!(rig.service.frame_holds(), 2);
    let (mut a, mut b) = (first.events(), second.events());
    ready(&mut a);
    ready(&mut b);
    ready(&mut a);
    ready(&mut b);

    rig.frames.push(frame(1));
    settle().await;
    let (Some(FramesEvent::Frame(x)), Some(FramesEvent::Frame(y))) = (ready(&mut a), ready(&mut b))
    else {
        panic!("both heard the frame");
    };
    assert!(Arc::ptr_eq(&x, &y), "one frame, shared");

    drop(a);
    pass(Duration::from_secs(5)).await;
    assert_eq!(rig.frames.calls(), [start()], "the other still watches");
    drop(b);
    settle().await;
    pass(Duration::from_millis(2100)).await;
    assert_eq!(rig.frames.calls(), [start(), CaptureCall::Stop]);
}

/// A hold taken while the last one's 2 s are running out keeps the capture going.
#[tokio::test(start_paused = true)]
async fn a_hold_taken_during_the_linger_keeps_the_capture() {
    let rig = rig();
    let hold = rig.service.frames().expect("a host");
    settle().await;
    drop(hold);
    settle().await;
    pass(Duration::from_secs(1)).await;
    let again = rig.service.frames().expect("a host");
    pass(Duration::from_secs(10)).await;
    assert_eq!(
        rig.frames.calls(),
        [start()],
        "never stopped, never restarted"
    );
    drop(again);
    settle().await;
    pass(Duration::from_millis(2100)).await;
    assert_eq!(rig.frames.calls(), [start(), CaptureCall::Stop]);
}

/// A capture that stopped starts again with the next hold.
#[tokio::test(start_paused = true)]
async fn a_hold_after_the_stop_starts_the_capture_again() {
    let rig = rig();
    let hold = rig.service.frames().expect("a host");
    settle().await;
    drop(hold);
    settle().await;
    pass(Duration::from_secs(3)).await;
    assert_eq!(rig.frames.calls(), [start(), CaptureCall::Stop]);
    let _hold = rig.service.frames().expect("a host");
    settle().await;
    assert_eq!(rig.frames.calls(), [start(), CaptureCall::Stop, start()]);
}

/// With no `pw-record` to run, the state says the capture is unavailable and why.
#[tokio::test(start_paused = true)]
async fn no_pw_record_reports_capture_unavailable() {
    let service = HostService::idle();
    let sink = FakeSink::new(Some(dac("Schiit Gungnir 2", TARGET)));
    service.run_with(parts(Ok(Frames {
        capture: Box::new(PwFrames::new("/nonexistent/svolderd-test-pw-record")),
        sink: Arc::new(sink),
    })));
    let hold = service.frames().expect("a host");
    settle().await;
    let CaptureState::Unavailable { why } = hold.state() else {
        panic!("no capture is possible, got {:?}", hold.state());
    };
    assert!(why.contains("pw-record"), "{why}");
}

/// A host with no capture tool at all (nothing to run, nothing to read the sink with) says so
/// to a hold, rather than refusing it.
#[tokio::test(start_paused = true)]
async fn a_host_with_no_capture_source_reports_why() {
    let service = HostService::idle();
    service.run_with(parts(Err("no capture tool here".to_string())));
    let hold = service.frames().expect("a host that runs");
    assert_eq!(
        hold.state(),
        CaptureState::Unavailable {
            why: "no capture tool here".to_string()
        }
    );
}

/// A start that fails is tried again every 2 s while someone watches, and a sink that cannot
/// be read is no capture until it can be.
#[tokio::test(start_paused = true)]
async fn an_unavailable_capture_is_tried_again_every_2_s() {
    let rig = rig();
    rig.frames.refuse(true);
    let hold = rig.service.frames().expect("a host");
    settle().await;
    assert!(matches!(hold.state(), CaptureState::Unavailable { .. }));
    assert_eq!(rig.frames.calls(), [start()]);

    rig.frames.refuse(false);
    pass(Duration::from_millis(2100)).await;
    assert_eq!(hold.state(), running("Schiit Gungnir 2"));

    rig.sink.set(None);
    pass(Duration::from_millis(2100)).await;
    assert_eq!(
        hold.state(),
        running("Schiit Gungnir 2"),
        "one failed read keeps a capture that works"
    );
    pass(Duration::from_millis(2100)).await;
    let CaptureState::Unavailable { why } = hold.state() else {
        panic!("no sink, no capture, got {:?}", hold.state());
    };
    assert!(why.contains("sink"), "{why}");
    rig.sink.set(Some(dac("Schiit Gungnir 2", TARGET)));
    pass(Duration::from_millis(2100)).await;
    assert_eq!(hold.state(), running("Schiit Gungnir 2"));
}

/// A capture that ends on its own (the sink went away) is Unavailable, and started again.
#[tokio::test(start_paused = true)]
async fn a_capture_that_ends_on_its_own_starts_again() {
    let rig = rig();
    let hold = rig.service.frames().expect("a host");
    settle().await;
    rig.frames.end();
    settle().await;
    assert!(matches!(hold.state(), CaptureState::Unavailable { .. }));
    pass(Duration::from_millis(2100)).await;
    assert_eq!(hold.state(), running("Schiit Gungnir 2"));
    assert_eq!(rig.frames.calls(), [start(), start()]);
}

/// The capture follows the default sink, looked at every 2 s, and the label is its name.
#[tokio::test(start_paused = true)]
async fn the_capture_follows_the_default_sink() {
    let rig = rig();
    let hold = rig.service.frames().expect("a host");
    settle().await;
    rig.sink.set(Some(dac(
        "Built-in Audio",
        "alsa_output.pci-0000_00_1f.3.analog-stereo",
    )));
    pass(Duration::from_millis(2100)).await;
    assert_eq!(
        hold.state(),
        CaptureState::Running {
            sink: "Built-in Audio".to_string(),
            target: "alsa_output.pci-0000_00_1f.3.analog-stereo".to_string(),
        }
    );
    assert_eq!(
        rig.frames.calls(),
        [
            start(),
            CaptureCall::Start("alsa_output.pci-0000_00_1f.3.analog-stereo".to_string())
        ]
    );
    assert_eq!(rig.sink.writes(), [], "the daemon only reads the sink");
}

/// A read of the sink that gave nothing (no `wpctl`) is told apart from a sink with no node.
#[tokio::test(start_paused = true)]
async fn an_unread_sink_and_a_sink_with_no_node_give_their_own_reasons() {
    let rig = rig();
    rig.sink.set(None);
    let hold = rig.service.frames().expect("a host");
    settle().await;
    assert_eq!(
        hold.state(),
        CaptureState::Unavailable {
            why: "the default sink could not be read".to_string()
        }
    );
    let mut bare = dac("Bare", TARGET);
    bare.target = None;
    rig.sink.set(Some(bare));
    pass(Duration::from_millis(2100)).await;
    assert_eq!(
        hold.state(),
        CaptureState::Unavailable {
            why: "there is no sink to capture".to_string()
        }
    );
}

/// A sink with no description is labelled by its node name.
#[tokio::test(start_paused = true)]
async fn a_sink_with_no_name_is_labelled_by_its_node() {
    let rig = rig();
    rig.sink.set(Some(dac("", TARGET)));
    let hold = rig.service.frames().expect("a host");
    settle().await;
    assert_eq!(hold.state(), running(TARGET));
}

/// A page that reads slowly skips to the newest frames; the capture never waits for it.
#[tokio::test(start_paused = true)]
async fn a_slow_hold_skips_to_the_newest_frames() {
    let rig = rig();
    let hold = rig.service.frames().expect("a host");
    let mut events = hold.events();
    settle().await;
    for t in 0..100 {
        rig.frames.push(frame(t));
    }
    settle().await;
    let mut heard = Vec::new();
    while let Some(event) = ready(&mut events) {
        if let FramesEvent::Frame(frame) = event {
            heard.push(frame.capture_ms);
        }
    }
    assert!(heard.len() < 100, "the old ones were skipped: {heard:?}");
    assert_eq!(heard.last(), Some(&99), "the newest came: {heard:?}");
}

/// A host that is off gives no hold; one that goes off lets a hold see the capture end.
#[tokio::test(start_paused = true)]
async fn an_idle_or_off_host_gives_no_hold() {
    assert!(HostService::idle().frames().is_none());
    let off = HostService::start(&svolder_ipc::config::HostConfig::default(), HostMode::Off);
    settle().await;
    assert!(off.frames().is_none());
}

/// A capture that drops when it is ended, so a test sees it was.
struct Probe {
    inner: FakeFrames,
    ended: Arc<AtomicBool>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.ended.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl FrameCapture for Probe {
    fn start(&mut self, sink: &str) -> bool {
        self.inner.start(sink)
    }

    fn stop(&mut self) {
        self.inner.stop();
    }

    fn target(&self) -> Option<String> {
        self.inner.target()
    }

    async fn next_frame(&mut self) -> Option<Frame> {
        self.inner.next_frame().await
    }
}

#[tokio::test(start_paused = true)]
async fn reconfigure_off_ends_the_capture() {
    let ended = Arc::new(AtomicBool::new(false));
    let frames = FakeFrames::new();
    let service = HostService::idle();
    service.run_with(parts(Ok(Frames {
        capture: Box::new(Probe {
            inner: frames.clone(),
            ended: Arc::clone(&ended),
        }),
        sink: Arc::new(FakeSink::new(Some(dac("Schiit Gungnir 2", TARGET)))),
    })));
    let hold = service.frames().expect("a host");
    settle().await;
    assert_eq!(hold.state(), running("Schiit Gungnir 2"));
    assert!(!ended.load(Ordering::SeqCst));

    service.reconfigure(
        &svolder_ipc::config::HostConfig { enabled: false },
        HostMode::Fake,
    );
    settle().await;
    assert!(
        ended.load(Ordering::SeqCst),
        "the capture was ended with the run"
    );
    assert!(matches!(hold.state(), CaptureState::Unavailable { .. }));
    assert!(
        service.frames().is_none(),
        "no new holds on a host that is off"
    );
    drop(hold);
}

/// A host that is brought back runs its capture for a hold that outlived the stop.
#[tokio::test(start_paused = true)]
async fn a_hold_that_outlives_a_reconfigure_gets_the_new_capture() {
    let rig = rig();
    let hold = rig.service.frames().expect("a host");
    settle().await;
    rig.service.reconfigure(
        &svolder_ipc::config::HostConfig { enabled: false },
        HostMode::Fake,
    );
    settle().await;
    assert!(matches!(hold.state(), CaptureState::Unavailable { .. }));
    rig.service.reconfigure(
        &svolder_ipc::config::HostConfig { enabled: true },
        HostMode::Fake,
    );
    settle().await;
    pass(Duration::from_millis(100)).await;
    assert!(
        matches!(hold.state(), CaptureState::Running { sink, .. } if sink == "Schiit Gungnir 2"),
        "the fake host's own capture, got {:?}",
        hold.state()
    );
}

#[test]
fn the_fake_capture_knob_reads_off() {
    let knob = |value: Option<&str>| {
        FakeKnobs::from_knobs(|name| {
            (name == "SVOLDER_FAKE_CAPTURE")
                .then(|| value.map(str::to_string))
                .flatten()
        })
        .capture_off
    };
    assert!(knob(Some("off")));
    assert!(knob(Some(" OFF ")));
    assert!(!knob(Some("on")));
    assert!(!knob(None));
}

#[tokio::test(start_paused = true)]
async fn the_fake_capture_off_knob_reports_unavailable() {
    let service = HostService::idle();
    let knobs =
        FakeKnobs::from_knobs(|name| (name == "SVOLDER_FAKE_CAPTURE").then(|| "off".into()));
    service.run_with(Parts::fake(knobs));
    let hold = service.frames().expect("the fake host runs");
    settle().await;
    let CaptureState::Unavailable { why } = hold.state() else {
        panic!(
            "a capture that is off is unavailable, got {:?}",
            hold.state()
        );
    };
    assert!(why.contains("off"), "{why}");
}

/// The fake host's capture is music through the real analysis, at real time, while held.
#[tokio::test(start_paused = true)]
async fn the_fake_host_captures_synthetic_music_while_held() {
    let service = HostService::idle();
    service.run_with(Parts::fake(FakeKnobs::default()));
    let hold: FramesHold = service.frames().expect("the fake host runs");
    let mut events = hold.events();
    let mut frames = 0;
    let mut state = None;
    let mut last = None;
    // Five seconds of the clock: 150 frames and a tempo.
    for _ in 0..200 {
        match tokio::time::timeout(Duration::from_secs(1), events.next())
            .await
            .expect("an event within a second")
            .expect("the stream goes on")
        {
            FramesEvent::Capture(now) => state = Some(now),
            FramesEvent::Frame(frame) => {
                frames += 1;
                last = Some(frame);
            }
        }
    }
    assert!(
        matches!(&state, Some(CaptureState::Running { sink, .. }) if sink == "Schiit Gungnir 2"),
        "{state:?}"
    );
    assert!(frames > 150, "{frames} frames");
    let last = last.expect("a frame");
    assert!(last.left.iter().any(|&b| b > 0), "the music has a spectrum");
    assert!(
        last.bpm.is_some_and(|bpm| (118..=122).contains(&bpm)),
        "{:?}",
        last.bpm
    );
}

/// Without a hold the fake makes nothing: no frames are analysed while nobody looks.
#[tokio::test(start_paused = true)]
async fn the_fake_host_makes_no_frames_without_a_hold() {
    let service = HostService::idle();
    service.run_with(Parts::fake(FakeKnobs::default()));
    pass(Duration::from_secs(5)).await;
    let hold = service.frames().expect("the fake host runs");
    assert_eq!(hold.state(), CaptureState::Starting);
}

/// Without a hold nothing is started and no sink is read, however long the host runs.
#[tokio::test(start_paused = true)]
async fn no_hold_means_no_start_and_no_read() {
    let rig = rig();
    pass(Duration::from_secs(30)).await;
    assert_eq!(rig.frames.calls(), []);
    assert_eq!(rig.sink.reads(), 0);
    assert_eq!(
        rig.service.frames().expect("a host").state(),
        CaptureState::Starting
    );
}

/// A hold taken a moment before the linger runs out keeps the capture: it is never stopped or
/// started again. The helper also counts the holds once more before it stops, for one that lands
/// in the very wake-up the linger ends in; a paused clock cannot put the two in one poll, so
/// this case does not pin that line.
#[tokio::test(start_paused = true)]
async fn a_hold_just_before_the_linger_ends_keeps_the_capture() {
    for _ in 0..30 {
        let rig = rig();
        let hold = rig.service.frames().expect("a host");
        settle().await;
        drop(hold);
        settle().await;
        pass(Duration::from_millis(1990)).await;
        let again = rig.service.frames().expect("a host");
        pass(Duration::from_millis(20)).await;
        assert_eq!(rig.frames.calls(), [start()], "stopped inside the wake-up");
        assert_eq!(again.state(), running("Schiit Gungnir 2"));
    }
}

/// A read of the sink that fails once keeps the capture that works; a second in a row ends it.
/// (The sink coming back is in `an_unavailable_capture_is_tried_again_every_2_s`.)
#[tokio::test(start_paused = true)]
async fn one_failed_sink_read_keeps_the_capture() {
    let rig = rig();
    let hold = rig.service.frames().expect("a host");
    settle().await;
    rig.sink.set(None);
    pass(Duration::from_millis(2100)).await;
    rig.sink.set(Some(dac("Schiit Gungnir 2", TARGET)));
    pass(Duration::from_millis(2100)).await;
    assert_eq!(hold.state(), running("Schiit Gungnir 2"));
    assert_eq!(
        rig.frames.calls(),
        [start()],
        "never stopped, never restarted"
    );
}

/// A capture that dies as soon as it starts is not started every 2 s: the tries back off, and
/// each change of state is sent once.
#[tokio::test(start_paused = true)]
async fn a_capture_that_dies_at_once_backs_off() {
    let rig = rig();
    let hold = rig.service.frames().expect("a host");
    let mut events = hold.events();
    let mut changes = 0;
    for _ in 0..60 {
        // The capture ends whenever it runs; one second of the clock at a time.
        rig.frames.end();
        pass(Duration::from_secs(1)).await;
        while let Some(event) = ready(&mut events) {
            if matches!(event, FramesEvent::Capture(_)) {
                changes += 1;
            }
        }
    }
    let starts = rig
        .frames
        .calls()
        .iter()
        .filter(|call| matches!(call, CaptureCall::Start(_)))
        .count();
    // 2 s, 4 s, 8 s, 16 s, then 30 s apart: about 7 in a minute, not 30.
    assert!((4..=9).contains(&starts), "{starts} starts in a minute");
    // The first state, then Running and Unavailable once for each start.
    assert!(
        changes <= 1 + 2 * starts,
        "{changes} changes for {starts} starts"
    );
}

/// A page that comes back after the linger is not made to wait out the back-off of a capture
/// that failed before.
#[tokio::test(start_paused = true)]
async fn a_page_that_returns_after_the_linger_does_not_wait_out_an_old_back_off() {
    let rig = rig();
    rig.frames.refuse(true);
    let hold = rig.service.frames().expect("a host");
    // Tries at 0, 2 and 6 s fail; the next would be at 14 s.
    pass_secs(7).await;
    assert_eq!(
        hold.state(),
        CaptureState::Unavailable {
            why: "pw-record could not be started".to_string()
        }
    );
    drop(hold);
    settle().await;
    pass(Duration::from_millis(2100)).await;
    rig.frames.refuse(false);
    let hold = rig.service.frames().expect("a host");
    pass(Duration::from_millis(100)).await;
    assert_eq!(
        hold.state(),
        running("Schiit Gungnir 2"),
        "at once, not at 14 s"
    );
}

/// A capture that has run for a good while and is then moved to a sink that fails starts its
/// back-off from the first failure, not from the count before the good run.
#[tokio::test(start_paused = true)]
async fn a_long_good_run_clears_the_failure_count() {
    let rig = rig();
    rig.frames.refuse(true);
    let hold = rig.service.frames().expect("a host");
    // Three failures (0, 2 and 6 s); the try at 14 s works.
    pass_secs(10).await;
    rig.frames.refuse(false);
    pass_secs(20).await;
    assert_eq!(hold.state(), running("Schiit Gungnir 2"));
    // Moved to a sink the capture cannot start on.
    rig.frames.refuse(true);
    rig.sink.set(Some(dac("Other", "other-node")));
    pass(Duration::from_millis(2500)).await;
    rig.frames.refuse(false);
    pass(Duration::from_millis(2500)).await;
    assert!(
        matches!(hold.state(), CaptureState::Running { ref target, .. } if target == "other-node"),
        "tried again after 2 s: {:?}",
        hold.state()
    );
}

/// A step of a run that was halted publishes nothing, though it was not ended: the generation
/// gate holds the state and the frames back.
#[tokio::test(start_paused = true)]
async fn a_helper_step_racing_a_halt_publishes_nothing() {
    let service = HostService::idle();
    let shared = Arc::clone(&service.shared);
    let generation = *crate::locks::locked(&shared.generation);
    let frames = FakeFrames::new();
    let sink = FakeSink::new(Some(dac("Schiit Gungnir 2", TARGET)));
    let stray = tokio::spawn(run(
        Arc::clone(&shared),
        generation,
        Frames {
            capture: Box::new(frames.clone()),
            sink: Arc::new(sink.clone()),
        },
    ));
    shared.holds.send_modify(|count| *count += 1);
    settle().await;
    assert_eq!(
        *shared.capture.borrow(),
        running("Schiit Gungnir 2"),
        "it ran"
    );

    service.stop();
    let mut heard = shared.frames.subscribe();
    frames.push(frame(1));
    sink.set(Some(dac(
        "Built-in Audio",
        "alsa_output.pci-0000_00_1f.3.analog-stereo",
    )));
    pass(Duration::from_secs(5)).await;
    let published = shared.capture.borrow().clone();
    assert!(
        matches!(&published, CaptureState::Unavailable { why } if why.contains("off")),
        "a halted run published {published:?}"
    );
    assert!(heard.try_recv().is_err(), "a halted run sent a frame");
    stray.abort();
}

/// A `pw-record` that writes a little and exits on its own: the capture says it ended, and
/// the next look starts it again (the real recorder reports its end, `target()` going `None`,
/// which `svolder-host`'s own tests pin; this is the daemon reading it).
#[tokio::test(start_paused = true)]
async fn a_pw_record_that_exits_is_noticed_and_started_again() {
    let dir = std::env::temp_dir().join(format!("svolderd-stub-pw-record-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("pw-record");
    // Two frames' worth of silence (1600 samples a channel each, f32), then the end.
    crate::test_support::install_script(
        &script,
        &format!(
            "#!/bin/sh\necho run >> '{}/runs'\nhead -c 25600 /dev/zero\nexit 0\n",
            dir.display()
        ),
    );
    let service = HostService::idle();
    service.run_with(parts(Ok(Frames {
        capture: Box::new(PwFrames::new(&script)),
        sink: Arc::new(FakeSink::new(Some(dac("Schiit Gungnir 2", TARGET)))),
    })));
    let hold = service.frames().expect("a host");
    let runs = || std::fs::read_to_string(dir.join("runs")).map_or(0, |runs| runs.lines().count());
    // The process and its thread are real: look until the daemon has seen it end, without
    // moving the clock.
    let mut ended = false;
    for _ in 0..500 {
        settle().await;
        if matches!(hold.state(), CaptureState::Unavailable { why } if why.contains("ended")) {
            ended = true;
            break;
        }
    }
    assert!(ended, "the end was not noticed: {:?}", hold.state());
    assert_eq!(runs(), 1);
    let mut again = false;
    for _ in 0..30 {
        pass(Duration::from_secs(1)).await;
        if runs() >= 2 {
            again = true;
            break;
        }
    }
    assert!(again, "it was not started again: {} runs", runs());
    drop(hold);
    service.stop();
    let _ = std::fs::remove_dir_all(&dir);
}
