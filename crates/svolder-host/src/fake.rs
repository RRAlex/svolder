// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fakes of the sources, for tests of what uses them: each answers from what the test
//! gave it and records what it was asked. A clone shares its original's state, so a test
//! keeps one and hands the other to the code under test.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream::StreamExt;
use tokio::sync::{Notify, watch};

use crate::analysis::Frame;
use crate::capture::{Block, Capture, FrameCapture};
use crate::players::{Button, Changes, NowPlaying, Players, Status};
use crate::sink::{Sink, SinkState};

/// A write a [`FakeSink`] was asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SinkWrite {
    /// [`Sink::set_volume`], with the volume asked for.
    Volume(f32),
    /// [`Sink::toggle_mute`].
    ToggleMute,
}

/// A sink that reads what the test set and takes the writes it is asked for.
#[derive(Debug, Clone, Default)]
pub struct FakeSink {
    inner: Arc<Mutex<SinkInner>>,
}

#[derive(Debug, Default)]
struct SinkInner {
    state: Option<SinkState>,
    reads: usize,
    writes: Vec<SinkWrite>,
}

impl FakeSink {
    /// A sink that reads `state`: `None` is a host with no sink to read (no `wpctl`).
    pub fn new(state: Option<SinkState>) -> Self {
        let sink = Self::default();
        sink.set(state);
        sink
    }

    /// What every read from now on gives.
    pub fn set(&self, state: Option<SinkState>) {
        self.lock().state = state;
    }

    /// What a read would give now, without counting as one.
    pub fn state(&self) -> Option<SinkState> {
        self.lock().state.clone()
    }

    /// How many times the sink was read.
    pub fn reads(&self) -> usize {
        self.lock().reads
    }

    /// Every write asked for, in order, whether there was a sink to take it or not.
    pub fn writes(&self) -> Vec<SinkWrite> {
        self.lock().writes.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SinkInner> {
        crate::locked(&self.inner)
    }
}

#[async_trait]
impl Sink for FakeSink {
    async fn read(&self) -> Option<SinkState> {
        let mut inner = self.lock();
        inner.reads += 1;
        inner.state.clone()
    }

    /// Takes the volume capped to `0.0..=1.0` and rounded to the hundredth `wpctl` is sent.
    async fn set_volume(&self, volume: f32) -> bool {
        let mut inner = self.lock();
        inner.writes.push(SinkWrite::Volume(volume));
        match inner.state.as_mut() {
            Some(state) if volume.is_finite() => {
                state.volume = (volume.clamp(0.0, 1.0) * 100.0).round() / 100.0;
                true
            }
            _ => false,
        }
    }

    async fn toggle_mute(&self) -> bool {
        let mut inner = self.lock();
        inner.writes.push(SinkWrite::ToggleMute);
        match inner.state.as_mut() {
            Some(state) => {
                state.muted = !state.muted;
                true
            }
            None => false,
        }
    }
}

/// Players that follow what the test set and record the presses they are asked for.
#[derive(Debug, Clone)]
pub struct FakePlayers {
    /// Where the changes are sent from, until the players are lost.
    sender: Arc<Mutex<Option<watch::Sender<Option<NowPlaying>>>>>,
    /// What the followed player plays, which outlives the sender.
    now: watch::Receiver<Option<NowPlaying>>,
    presses: Arc<Mutex<Vec<Button>>>,
    /// Whether a press turns the status over as a real player's would.
    answers: bool,
}

impl FakePlayers {
    /// Players following `now`: `None` is a bus with no player on it.
    pub fn new(now: Option<NowPlaying>) -> Self {
        let (sender, now) = watch::channel(now);
        Self {
            sender: Arc::new(Mutex::new(Some(sender))),
            now,
            presses: Arc::default(),
            answers: false,
        }
    }

    /// The same players, answering presses as a real player does: [`Button::Play`] makes the
    /// followed player `Playing` and [`Button::Pause`] makes it `Paused`, the change sent on
    /// [`Players::changes`], its position held where it stood. Previous and next change
    /// nothing.
    pub fn answering(mut self) -> Self {
        self.answers = true;
        self
    }

    /// What the followed player plays from now on, sent on [`Players::changes`] when it differs
    /// from what it played before. Once the players are lost, nothing.
    pub fn set(&self, now: Option<NowPlaying>) {
        if let Some(sender) = crate::locked(&self.sender).as_ref() {
            send_changed(sender, now);
        }
    }

    /// Lose the players, as `Mpris` does when the bus goes away: what is followed becomes
    /// `None` (sent as a last change when it was not already), every change stream ends, and
    /// nothing is followed again.
    pub fn lose(&self) {
        if let Some(sender) = crate::locked(&self.sender).take() {
            send_changed(&sender, None);
        }
    }

    /// Every press asked for, in order, whether a player was there to take it or not.
    pub fn presses(&self) -> Vec<Button> {
        crate::locked(&self.presses).clone()
    }
}

#[async_trait]
impl Players for FakePlayers {
    fn now_playing(&self) -> Option<NowPlaying> {
        self.now.borrow().clone()
    }

    fn changes(&self) -> Changes {
        let mut now = self.now.clone();
        now.mark_unchanged();
        futures::stream::unfold(now, |mut now| async move {
            now.changed().await.ok()?;
            let playing = now.borrow_and_update().clone();
            Some((playing, now))
        })
        .boxed()
    }

    async fn press(&self, button: Button) -> bool {
        crate::locked(&self.presses).push(button);
        if self.answers
            && let Some(sender) = crate::locked(&self.sender).as_ref()
        {
            sender.send_if_modified(|playing| answer(playing.as_mut(), button));
        }
        self.now.borrow().is_some()
    }
}

/// What a player does on `button`: play and pause turn its status over, holding the position
/// where it stood (a pause counts the time played since the position was read). Whether
/// anything changed.
fn answer(playing: Option<&mut NowPlaying>, button: Button) -> bool {
    let Some(playing) = playing else {
        return false;
    };
    let status = match button {
        Button::Play => Status::Playing,
        Button::Pause => Status::Paused,
        Button::Previous | Button::Next => return false,
    };
    if playing.status == status {
        return false;
    }
    let now_ms = crate::players::unix_ms();
    if let Some(progress) = playing.progress.as_mut() {
        if playing.status == Status::Playing {
            let played_ms = now_ms.saturating_sub(progress.read_at_ms);
            progress.position_us += played_ms * 1_000 * u64::from(progress.rate_milli) / 1_000;
        }
        progress.read_at_ms = now_ms;
    }
    playing.status = status;
    true
}

/// Send `now` on `sender` when it differs from what it holds.
fn send_changed(sender: &watch::Sender<Option<NowPlaying>>, now: Option<NowPlaying>) {
    sender.send_if_modified(|playing| {
        if *playing == now {
            return false;
        }
        *playing = now;
        true
    });
}

/// A start or a stop a [`FakeCapture`] or a [`FakeFrames`] was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureCall {
    /// [`Capture::start`], with the sink named.
    Start(String),
    /// [`Capture::stop`].
    Stop,
}

/// A capture that runs when it is started and gives the blocks the test pushes while it runs.
#[derive(Debug, Clone, Default)]
pub struct FakeCapture {
    source: Source<Block>,
}

/// A frames capture that runs when it is started and gives the frames the test pushes while
/// it runs, with the rules of a [`FakeCapture`].
#[derive(Debug, Clone, Default)]
pub struct FakeFrames {
    source: Source<Frame>,
}

/// What a fake capture shares between the clones and the code under test: the items pushed
/// while a capture runs, and the calls it was asked for.
#[derive(Debug)]
struct Source<T> {
    inner: Arc<Mutex<CaptureInner<T>>>,
    /// Woken when an item is pushed or the capture is ended.
    arrived: Arc<Notify>,
}

impl<T> Clone for Source<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            arrived: Arc::clone(&self.arrived),
        }
    }
}

impl<T> Default for Source<T> {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(CaptureInner::default())),
            arrived: Arc::default(),
        }
    }
}

#[derive(Debug)]
struct CaptureInner<T> {
    target: Option<String>,
    items: VecDeque<T>,
    /// Whether the running capture ends once its items are taken.
    ending: bool,
    /// Whether a start fails, as one does without `pw-record`.
    refusing: bool,
    calls: Vec<CaptureCall>,
}

impl<T> Default for CaptureInner<T> {
    fn default() -> Self {
        Self {
            target: None,
            items: VecDeque::new(),
            ending: false,
            refusing: false,
            calls: Vec::new(),
        }
    }
}

impl<T> Source<T> {
    fn push(&self, item: T) -> bool {
        let taken = {
            let mut inner = self.lock();
            if inner.target.is_some() {
                inner.items.push_back(item);
                true
            } else {
                false
            }
        };
        self.arrived.notify_one();
        taken
    }

    fn end(&self) {
        {
            let mut inner = self.lock();
            if inner.target.is_some() {
                inner.ending = true;
            }
        }
        self.arrived.notify_one();
    }

    fn refuse(&self, refusing: bool) {
        self.lock().refusing = refusing;
    }

    fn calls(&self) -> Vec<CaptureCall> {
        self.lock().calls.clone()
    }

    fn start(&self, sink: &str) -> bool {
        let mut inner = self.lock();
        inner.calls.push(CaptureCall::Start(sink.to_string()));
        if sink.trim().is_empty() || inner.refusing {
            inner.halt();
            return false;
        }
        if inner.target.as_deref() != Some(sink) {
            inner.halt();
            inner.target = Some(sink.to_string());
        }
        true
    }

    fn stop(&self) {
        let mut inner = self.lock();
        inner.calls.push(CaptureCall::Stop);
        inner.halt();
    }

    fn target(&self) -> Option<String> {
        self.lock().target.clone()
    }

    async fn next(&self) -> Option<T> {
        loop {
            {
                let mut inner = self.lock();
                if inner.target.is_some() {
                    if let Some(item) = inner.items.pop_front() {
                        return Some(item);
                    }
                    if inner.ending {
                        inner.halt();
                        return None;
                    }
                }
            }
            // A push between the look above and this wait leaves its wake-up stored, so the
            // wait ends at once and the loop looks again.
            self.arrived.notified().await;
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CaptureInner<T>> {
        crate::locked(&self.inner)
    }
}

impl<T> CaptureInner<T> {
    fn halt(&mut self) {
        self.target = None;
        self.items.clear();
        self.ending = false;
    }
}

impl FakeCapture {
    /// A capture that runs nothing until it is started.
    pub fn new() -> Self {
        Self::default()
    }

    /// Hand `block` to the running capture, to be given by the next [`Capture::next_block`].
    /// Whether a capture was running to take it: with none, the block is dropped.
    pub fn push(&self, block: Block) -> bool {
        self.source.push(block)
    }

    /// End the running capture on its own, as `pw-record` stopping does: once the blocks
    /// pushed before are taken, [`Capture::next_block`] gives `None` and nothing runs.
    pub fn end(&self) {
        self.source.end();
    }

    /// Make every start from now on fail (`true`), as one does on a host without
    /// `pw-record`, or succeed again (`false`).
    pub fn refuse(&self, refusing: bool) {
        self.source.refuse(refusing);
    }

    /// Every start and stop asked for, in order.
    pub fn calls(&self) -> Vec<CaptureCall> {
        self.source.calls()
    }
}

#[async_trait]
impl Capture for FakeCapture {
    fn start(&mut self, sink: &str) -> bool {
        self.source.start(sink)
    }

    fn stop(&mut self) {
        self.source.stop();
    }

    fn target(&self) -> Option<String> {
        self.source.target()
    }

    async fn next_block(&mut self) -> Option<Block> {
        self.source.next().await
    }
}

impl FakeFrames {
    /// A capture that runs nothing until it is started.
    pub fn new() -> Self {
        Self::default()
    }

    /// Hand `frame` to the running capture, to be given by the next
    /// [`FrameCapture::next_frame`]. Whether a capture was running to take it: with none, the
    /// frame is dropped.
    pub fn push(&self, frame: Frame) -> bool {
        self.source.push(frame)
    }

    /// End the running capture on its own: once the frames pushed before are taken,
    /// [`FrameCapture::next_frame`] gives `None` and nothing runs.
    pub fn end(&self) {
        self.source.end();
    }

    /// Make every start from now on fail (`true`), or succeed again (`false`).
    pub fn refuse(&self, refusing: bool) {
        self.source.refuse(refusing);
    }

    /// Every start and stop asked for, in order.
    pub fn calls(&self) -> Vec<CaptureCall> {
        self.source.calls()
    }
}

#[async_trait]
impl FrameCapture for FakeFrames {
    fn start(&mut self, sink: &str) -> bool {
        self.source.start(sink)
    }

    fn stop(&mut self) {
        self.source.stop();
    }

    fn target(&self) -> Option<String> {
        self.source.target()
    }

    async fn next_frame(&mut self) -> Option<Frame> {
        self.source.next().await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::fft::BANDS;
    use crate::players::Progress;

    fn dac(volume: f32, muted: bool) -> SinkState {
        SinkState {
            volume,
            muted,
            name: "USB DAC".to_string(),
            target: Some("alsa_output.usb-dac".to_string()),
        }
    }

    fn track(title: &str, status: Status) -> NowPlaying {
        NowPlaying {
            genre: Vec::new(),
            title: title.to_string(),
            artist: "Miles Davis".to_string(),
            identity: "Fake Player".to_string(),
            status,
            ..Default::default()
        }
    }

    fn block(level: f32) -> Block {
        Block {
            bands: [level; BANDS],
            left: level,
            right: level,
        }
    }

    /// Whether `next_block` still has nothing after a moment.
    async fn nothing_comes(capture: &mut FakeCapture) -> bool {
        tokio::time::timeout(Duration::from_millis(50), capture.next_block())
            .await
            .is_err()
    }

    /// What `next_block` gives, which a case expects at once.
    async fn given(capture: &mut FakeCapture) -> Option<Block> {
        tokio::time::timeout(Duration::from_secs(1), capture.next_block())
            .await
            .expect("next_block gave nothing")
    }

    /// The next change, which a case expects at once.
    async fn change(changes: &mut Changes) -> Option<NowPlaying> {
        tokio::time::timeout(Duration::from_secs(1), changes.next())
            .await
            .expect("no change came")
            .expect("the changes ended")
    }

    /// Players that answer presses as a real one does: play and pause turn the status over and
    /// hold the position where it was, the other buttons change nothing.
    #[tokio::test]
    async fn answering_players_flip_the_status_on_presses() {
        let playing = NowPlaying {
            progress: Some(Progress {
                position_us: 5_000_000,
                rate_milli: 1000,
                read_at_ms: 1_000,
            }),
            ..track("So What", Status::Playing)
        };
        let players = FakePlayers::new(Some(playing)).answering();
        let mut changes = players.changes();

        assert!(players.press(Button::Pause).await);
        let paused = change(&mut changes).await.expect("a player");
        assert_eq!(paused.status, Status::Paused);
        let held = paused.progress.expect("a position");
        assert!(held.position_us > 5_000_000, "{held:?}");

        assert!(players.press(Button::Play).await);
        let resumed = change(&mut changes).await.expect("a player");
        assert_eq!(resumed.status, Status::Playing);
        assert_eq!(
            resumed.progress.expect("a position").position_us,
            held.position_us
        );

        assert!(players.press(Button::Next).await);
        assert_eq!(players.now_playing(), Some(resumed));
        assert_eq!(
            players.presses(),
            [Button::Pause, Button::Play, Button::Next]
        );
    }

    /// Players that were not told to answer leave the status alone, as before.
    #[tokio::test]
    async fn plain_fake_players_ignore_what_a_press_would_do() {
        let players = FakePlayers::new(Some(track("So What", Status::Playing)));
        assert!(players.press(Button::Pause).await);
        assert_eq!(
            players.now_playing().map(|now| now.status),
            Some(Status::Playing)
        );
    }

    #[tokio::test]
    async fn the_fake_sink_reads_what_was_set_and_takes_its_writes() {
        let sink = FakeSink::new(Some(dac(0.54, false)));
        let held = sink.clone();
        assert_eq!(sink.read().await, Some(dac(0.54, false)));
        assert!(sink.set_volume(0.56).await);
        assert!(sink.set_volume(1.2).await);
        assert!(sink.toggle_mute().await);
        assert_eq!(held.state(), Some(dac(1.0, true)));
        assert_eq!(held.reads(), 1);
        assert_eq!(
            held.writes(),
            [
                SinkWrite::Volume(0.56),
                SinkWrite::Volume(1.2),
                SinkWrite::ToggleMute
            ]
        );
    }

    #[tokio::test]
    async fn the_fake_sink_rounds_a_volume_to_the_hundredth_wpctl_is_sent() {
        let sink = FakeSink::new(Some(dac(0.54, false)));
        assert!(sink.set_volume(0.564).await);
        assert_eq!(sink.state().map(|s| s.volume), Some(0.56));
    }

    #[tokio::test]
    async fn without_a_sink_the_fake_records_the_writes_and_takes_none() {
        let sink = FakeSink::new(None);
        assert_eq!(sink.read().await, None);
        assert!(!sink.set_volume(0.5).await);
        assert!(!sink.toggle_mute().await);
        assert_eq!(sink.state(), None);
        assert_eq!(
            sink.writes(),
            [SinkWrite::Volume(0.5), SinkWrite::ToggleMute]
        );
    }

    #[tokio::test]
    async fn the_fake_players_send_each_change_and_record_each_press() {
        let players = FakePlayers::new(None);
        let held = players.clone();
        let mut changes = players.changes();
        assert!(!players.press(Button::Play).await);
        held.set(Some(track("So What", Status::Playing)));
        assert_eq!(
            change(&mut changes).await,
            Some(track("So What", Status::Playing))
        );
        assert_eq!(
            players.now_playing(),
            Some(track("So What", Status::Playing))
        );
        assert!(players.next().await);
        held.set(None);
        assert_eq!(change(&mut changes).await, None);
        assert_eq!(held.presses(), [Button::Play, Button::Next]);
    }

    #[tokio::test]
    async fn setting_the_same_track_again_sends_no_change() {
        let players = FakePlayers::new(Some(track("So What", Status::Playing)));
        let mut changes = players.changes();
        players.set(Some(track("So What", Status::Playing)));
        assert!(
            tokio::time::timeout(Duration::from_millis(50), changes.next())
                .await
                .is_err(),
            "the same track again was sent as a change"
        );
        players.set(Some(track("So What", Status::Paused)));
        assert_eq!(
            change(&mut changes).await,
            Some(track("So What", Status::Paused))
        );
    }

    #[tokio::test]
    async fn lost_players_send_none_end_every_change_stream_and_take_no_press() {
        let players = FakePlayers::new(Some(track("So What", Status::Playing)));
        let held = players.clone();
        let mut changes = players.changes();
        held.lose();
        assert_eq!(change(&mut changes).await, None);
        let ended = tokio::time::timeout(Duration::from_secs(1), changes.next()).await;
        assert_eq!(ended.expect("the changes ended in time"), None);
        assert_eq!(players.now_playing(), None);
        assert!(!players.press(Button::Play).await);
        assert_eq!(held.presses(), [Button::Play]);
        // A stream asked for afterwards ends at once, and nothing is followed again.
        let later = tokio::time::timeout(Duration::from_secs(1), players.changes().next()).await;
        assert_eq!(later.expect("a later stream ended in time"), None);
        held.set(Some(track("Blue in Green", Status::Playing)));
        assert_eq!(players.now_playing(), None);
    }

    #[tokio::test]
    async fn the_fake_capture_gives_the_blocks_pushed_while_it_runs() {
        let mut capture = FakeCapture::new();
        let held = capture.clone();
        assert!(!held.push(block(0.1)), "nothing runs to take it");
        assert!(capture.start("alsa_output.usb-dac"));
        assert_eq!(capture.target().as_deref(), Some("alsa_output.usb-dac"));
        assert!(held.push(block(0.5)));
        assert_eq!(given(&mut capture).await, Some(block(0.5)));
        assert!(nothing_comes(&mut capture).await);
        capture.stop();
        assert_eq!(capture.target(), None);
        assert!(nothing_comes(&mut capture).await);
        assert_eq!(
            held.calls(),
            [
                CaptureCall::Start("alsa_output.usb-dac".to_string()),
                CaptureCall::Stop
            ]
        );
    }

    #[tokio::test]
    async fn a_block_pushed_while_the_capture_waits_is_given_at_once() {
        let mut capture = FakeCapture::new();
        let held = capture.clone();
        assert!(capture.start("hdmi-out"));
        let waiting = tokio::spawn(async move { capture.next_block().await });
        tokio::task::yield_now().await;
        assert!(held.push(block(0.7)));
        let given = tokio::time::timeout(Duration::from_secs(1), waiting)
            .await
            .expect("the block was given")
            .unwrap();
        assert_eq!(given, Some(block(0.7)));
    }

    #[tokio::test]
    async fn an_ended_capture_gives_its_last_blocks_then_none_once() {
        let mut capture = FakeCapture::new();
        let held = capture.clone();
        assert!(capture.start("hdmi-out"));
        assert!(held.push(block(0.3)));
        held.end();
        assert_eq!(given(&mut capture).await, Some(block(0.3)));
        assert_eq!(given(&mut capture).await, None);
        assert_eq!(capture.target(), None);
        assert!(nothing_comes(&mut capture).await);
    }

    #[tokio::test]
    async fn the_fake_capture_refuses_no_sink_and_a_refused_start() {
        let mut capture = FakeCapture::new();
        assert!(!capture.start(""));
        capture.refuse(true);
        assert!(!capture.start("hdmi-out"));
        assert_eq!(capture.target(), None);
        capture.refuse(false);
        assert!(capture.start("hdmi-out"));
        assert_eq!(capture.target().as_deref(), Some("hdmi-out"));
    }

    #[tokio::test]
    async fn starting_the_same_sink_goes_on_and_another_replaces_it() {
        let mut capture = FakeCapture::new();
        let held = capture.clone();
        assert!(capture.start("hdmi-out"));
        assert!(held.push(block(0.2)));
        assert!(capture.start("hdmi-out"));
        assert_eq!(given(&mut capture).await, Some(block(0.2)));
        assert!(held.push(block(0.4)));
        assert!(capture.start("alsa_output.usb-dac"));
        assert!(
            nothing_comes(&mut capture).await,
            "a block of the other sink was kept"
        );
        assert_eq!(capture.target().as_deref(), Some("alsa_output.usb-dac"));
    }

    #[tokio::test]
    async fn the_fake_frames_capture_gives_the_frames_pushed_while_it_runs() {
        let frame = Frame {
            capture_ms: 7,
            left: [3; 240],
            right: [4; 240],
            levels: [1, 2, 3, 4],
            beat: true,
            bpm: Some(120),
            phase: 9,
        };
        let mut capture = FakeFrames::new();
        let held = capture.clone();
        assert!(!held.push(frame), "nothing runs yet");
        assert!(capture.start("alsa_output.usb-dac"));
        assert!(held.push(frame));
        assert_eq!(capture.next_frame().await, Some(frame));
        assert!(held.push(frame));
        held.end();
        assert_eq!(capture.next_frame().await, Some(frame));
        assert_eq!(capture.next_frame().await, None);
        assert_eq!(capture.target(), None);
        assert_eq!(
            held.calls(),
            [CaptureCall::Start("alsa_output.usb-dac".to_string())]
        );
    }
}
