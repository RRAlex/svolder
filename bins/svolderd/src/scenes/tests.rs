// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scenes service over stand-in devices and a fake computer: each device is a stand-in behind
//! a real writer that records the commands it is sent and applies them to the readings its feed
//! publishes, so a recall is judged by what reaches each actor.

mod events;
mod races;
mod recalls;
mod save;
mod upkeep;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use futures::StreamExt;
use futures::stream::BoxStream;
use svolder_host::eq::{ComputerPart, EqView};
use svolder_host::fake::{FakeChain, FakeParams};
use svolder_ipc::config::HostConfig;
use svolder_ipc::scenes::Scene;
use svolder_ipc::socket::protocol::{SceneOp, ScenesView};
use svolder_ipc::{Config, Event, Request, Response};
use svolder_profiles::scene::{self, DevicePart};
use svolder_profiles::tone::apo::{FilterKind, SourceFilter};
use svolder_profiles::tone::preset::{self, Preset};
use svolder_profiles::{ControlKind, EqMode, EqParam, Profile, Registry};
use svolder_proto::{FunctionId, Message, Value};
use tokio::sync::{mpsc, oneshot, watch};

use crate::device::{Command, DeviceHandle, Feed, LiveInfo};
use crate::handler::Daemon;
use crate::host::HostMode;
use crate::test_support::shown;
use crate::writer::{Origin, Writer};

/// The unit ids of the stand-in devices.
pub(super) const BIFROST: &str = "0102030405060708";
pub(super) const LYR: &str = "0A0B0C0D0E0F1011";
pub(super) const LOKI: &str = "1112131415161718";

/// One stand-in device behind a real writer: it records what it was sent and applies it.
pub(super) struct Stand {
    pub(super) handle: Arc<DeviceHandle>,
    pub(super) log: Arc<std::sync::Mutex<Vec<String>>>,
    pub(super) gate: Arc<tokio::sync::Semaphore>,
    pub(super) feed: watch::Sender<Feed>,
    pub(super) profile: Arc<Profile>,
    /// The 1-based command the stand answers with an error and does not apply: a device lost.
    pub(super) fail_at: Arc<std::sync::atomic::AtomicUsize>,
    /// While set, a write sent without waiting is answered `Ack` and its echo is held (the TUI
    /// sends every write so; the device's report comes a moment later).
    pub(super) late_echo: Arc<std::sync::atomic::AtomicBool>,
    /// The echoes held by `late_echo`, published by [`Stand::flush_late`].
    late: Arc<std::sync::Mutex<Vec<Message>>>,
}

impl Stand {
    /// The commands the stand was sent, in order, not counting the reloads' (`Configure` and
    /// `Reload`, which every config write sends to every device).
    pub(super) fn log(&self) -> Vec<String> {
        let log = self.log.lock().expect("the log").clone();
        log.into_iter()
            .filter(|line| line != "another command")
            .collect()
    }

    /// The first raw value of the select `key`: what the stand reads as it is seeded.
    pub(super) fn first(&self, key: &str) -> i64 {
        let control = self.profile.control_by_key(key).expect("a control");
        control
            .values
            .first()
            .map(|(raw, _)| i64::from(*raw))
            .expect("a value")
    }

    /// The second raw value of the select `key`: one the stand does not read as it is seeded,
    /// which reads its first.
    pub(super) fn other(&self, key: &str) -> i64 {
        let control = self.profile.control_by_key(key).expect("a control");
        control
            .values
            .get(1)
            .map(|(raw, _)| i64::from(*raw))
            .expect("a second value")
    }

    /// The raw step the stand reads for `key`.
    pub(super) fn raw(&self, key: &str) -> Option<i64> {
        let id = self.profile.control_by_key(key)?.id;
        let feed = self.feed.borrow();
        feed.readings
            .iter()
            .find(|m| m.id == id)
            .and_then(|m| m.value.as_i64())
    }

    /// The stand reads nothing of `key` any more, as a device that has just reconnected.
    pub(super) fn forget(&self, key: &str) {
        let id = self.profile.control_by_key(key).expect("a control").id;
        self.feed
            .send_modify(|feed| feed.readings.retain(|m| m.id != id));
    }

    /// The stand reads no reading at all, as a device that has just reconnected.
    pub(super) fn forget_all(&self) {
        self.feed.send_modify(|feed| feed.readings.clear());
    }

    /// The held echoes come in: the device reports what it was sent.
    pub(super) fn flush_late(&self) {
        for message in self.late.lock().expect("late").drain(..) {
            self.feed.send_modify(|feed| heard(feed, &message));
        }
    }

    /// The oldest held echo comes in, and no other.
    pub(super) fn flush_next(&self) {
        let message = {
            let mut late = self.late.lock().expect("late");
            (!late.is_empty()).then(|| late.remove(0))
        };
        if let Some(message) = message {
            self.feed.send_modify(|feed| heard(feed, &message));
        }
    }

    /// The stand reads `readings` again.
    pub(super) fn read_again(&self, readings: Vec<Message>) {
        self.feed.send_modify(|feed| feed.readings = readings);
    }

    /// The stand's readings.
    pub(super) fn readings(&self) -> Vec<Message> {
        self.feed.borrow().readings.clone()
    }
}

/// The feed hears `message` as an actor's feed does: the reading is kept and the control is
/// stamped as heard now (a count no two hearings share).
fn heard(feed: &mut Feed, message: &Message) {
    static HEARD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    feed.readings.retain(|m| m.id != message.id);
    feed.readings.push(message.clone());
    feed.heard_at
        .insert(message.id, HEARD.fetch_add(1, Ordering::SeqCst));
}

/// The message a command writes, as the device would report it, for the stand to apply.
fn applied(profile: &Profile, command: &Command) -> Option<Message> {
    match command {
        Command::EqSet {
            band, param, value, ..
        } => svolder_profiles::eq_set(profile, band - 1, param.parse().ok()?, *value).ok(),
        Command::EqMode { mode, .. } => svolder_profiles::eq_mode(profile, mode.parse().ok()?).ok(),
        Command::Raw { id, value, .. } => Some(Message::status(
            profile.read_as(id.parse().ok()?),
            Value::U8(value.parse().ok()?),
        )),
        Command::Set { key, value, .. } => {
            let sent = svolder_profiles::intent(profile, key, value).ok()?;
            Some(Message::status(sent.id, sent.value))
        }
        _ => None,
    }
}

/// What a device that has read everything holds: the tone control flat, every held control on its
/// first value, the built-in bands on 15, and then `extra` (key and raw step).
fn seeded(profile: &Profile, extra: &[(&str, i64)]) -> Vec<Message> {
    let mut readings: Vec<Message> = Vec::new();
    let mut put = |message: Message| {
        readings.retain(|read| read.id != message.id);
        readings.push(message);
    };
    if let Some(eq) = profile.eq.as_ref() {
        for (band, spec) in eq.bands.iter().enumerate() {
            for (param, value) in [
                (EqParam::Frequency, spec.center),
                (EqParam::Gain, 0.0),
                (EqParam::Q, eq.q_center),
            ] {
                put(svolder_profiles::eq_set(profile, band, param, value).expect("a centre"));
            }
        }
        put(svolder_profiles::eq_mode(profile, EqMode::Eq).expect("a mode"));
        put(Message::status(eq.loudness_id(), Value::U8(0)));
    }
    for control in scene::held_controls(profile) {
        let first = match control.kind {
            ControlKind::Select => control.values.first().map_or(0, |(raw, _)| i64::from(*raw)),
            ControlKind::Toggle => 0,
            _ => control.range.map_or(0, |(low, _)| low),
        };
        let sent = svolder_profiles::intent(profile, &control.key, &first.to_string())
            .expect("a first value");
        put(Message::status(sent.id, sent.value));
    }
    if let Some(table) = &profile.builtin_settings {
        for key in &table.keys {
            let sent = svolder_profiles::intent(profile, key, "15").expect("a band");
            put(Message::status(sent.id, sent.value));
        }
    }
    for (key, raw) in extra {
        let sent = svolder_profiles::intent(profile, key, &raw.to_string()).expect("a reading");
        put(Message::status(sent.id, sent.value));
    }
    readings
}

/// A stand-in device of the built-in profile `profile` named `name` on `unit`, reading
/// everything, with `extra` readings (key and raw step) over the seeded ones.
pub(super) fn stand(profile: &str, name: &str, unit: &str, extra: &[(&str, i64)]) -> Stand {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = Arc::new(registry.get(profile).expect("a built-in").clone());
    let (feed, feed_rx) = watch::channel(Feed {
        name: name.into(),
        unit: unit.into(),
        profile: Some(Arc::clone(&profile)),
        connected: true,
        readings: seeded(&profile, extra),
        ..Feed::default()
    });
    let (commands, mut inbox) = mpsc::channel::<(Command, oneshot::Sender<Response>)>(8);
    let log: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let gate = Arc::new(tokio::sync::Semaphore::new(10_000));
    let fail_at = Arc::new(std::sync::atomic::AtomicUsize::new(usize::MAX));
    let late_echo = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let late: Arc<std::sync::Mutex<Vec<Message>>> = Arc::default();
    {
        let (late_echo, late) = (Arc::clone(&late_echo), Arc::clone(&late));
        let (log, gate, publish, profile, fail_at) = (
            Arc::clone(&log),
            Arc::clone(&gate),
            feed.clone(),
            Arc::clone(&profile),
            Arc::clone(&fail_at),
        );
        tokio::spawn(async move {
            let mut seen = 0;
            while let Some((command, reply)) = inbox.recv().await {
                gate.acquire().await.expect("the gate is open").forget();
                if applied(&profile, &command).is_some() {
                    seen += 1;
                    if seen == fail_at.load(Ordering::SeqCst) {
                        log.lock().expect("the log").push(shown(&command));
                        let _ = reply.send(Response::error(
                            svolder_ipc::CODE_LINK,
                            "the device went away",
                        ));
                        continue;
                    }
                }
                log.lock().expect("the log").push(shown(&command));
                if late_echo.load(Ordering::SeqCst)
                    && matches!(command, Command::Set { no_wait: true, .. })
                    && let Some(message) = applied(&profile, &command)
                {
                    late.lock().expect("late").push(message);
                    let _ = reply.send(Response::Ack);
                    continue;
                }
                let Some(message) = applied(&profile, &command) else {
                    let _ = reply.send(Response::Ack);
                    continue;
                };
                publish.send_modify(|feed| heard(feed, &message));
                let status = Message::status(message.id, message.value);
                let reading = svolder_profiles::describe(&profile, &status);
                let _ = reply.send(Response::Value(svolder_ipc::ValueObject::from_reading(
                    &reading, &status,
                )));
            }
        });
    }
    let (events, _) = tokio::sync::broadcast::channel(1);
    let handle = Arc::new(DeviceHandle {
        name: name.into(),
        udid: unit.into(),
        writer: Writer::spawn(commands.clone(), feed_rx.clone()),
        commands,
        events,
        info: Arc::new(std::sync::Mutex::new(LiveInfo {
            signal: None,
            unit: unit.into(),
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
        feed: feed_rx,
    });
    Stand {
        handle,
        log,
        gate,
        feed,
        profile,
        fail_at,
        late_echo,
        late,
    }
}

/// A daemon over stand-in devices, a fake computer and a config file of its own.
pub(super) struct Rig {
    pub(super) daemon: Arc<Daemon>,
    pub(super) dir: PathBuf,
    pub(super) config: PathBuf,
    /// The file the recalled scene is kept in.
    pub(super) state: PathBuf,
    pub(super) chain: FakeChain,
    pub(super) params: FakeParams,
    pub(super) stands: Vec<Stand>,
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Rig {
    /// A rig over `stands` (each named as its config block is) with a fake computer when
    /// `computer`; `more` are the lines appended to the config (devices that never connect,
    /// scenes).
    pub(super) async fn new(stands: Vec<Stand>, computer: bool, more: &str) -> Rig {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "svolderd-scenes-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("profiles")).expect("a scratch directory");
        let mut text = "schema = 1\n".to_string();
        for stand in &stands {
            text.push_str(&format!(
                "[[device]]\nname = \"{}\"\nudid = \"{}\"\npassphrase = \"1234\"\n",
                stand.handle.name, stand.handle.udid
            ));
        }
        text.push_str(more);
        let config = dir.join("config.toml");
        std::fs::write(&config, text).expect("a config");
        let state = dir.join("state").join("scenes.toml");
        Rig::over(stands, computer, dir, config, state).await
    }

    /// A rig over a config and a state file that are there already.
    pub(super) async fn over(
        stands: Vec<Stand>,
        computer: bool,
        dir: PathBuf,
        config: PathBuf,
        state: PathBuf,
    ) -> Rig {
        let registry = Arc::new(Registry::builtin().expect("the built-in profiles load"));
        let transport = svolder_ble::FakeTransport::from_profile(
            registry.get("gungnir2-v2").expect("a built-in"),
            "0F0F0F0F0F0F0F0F".parse().expect("a unit id"),
            3,
            svolder_proto::Passphrase::from_text("1234").expect("four digits"),
        );
        let loaded = Config::load_from(&config).expect("the config loads");
        let daemon = Daemon::new(
            Arc::new(transport),
            registry,
            config.clone(),
            dir.join("profiles"),
            dir.join("s.sock"),
            loaded,
        )
        .with_host_mode(HostMode::Fake)
        .with_scenes_file(state.clone())
        .shared();
        // The host is started as the config's section says, so a reload leaves it as it is, and
        // then the computer is the test's own fake EQ.
        daemon.host_to(&HostConfig::default());
        let (chain, params) = (FakeChain::new(), FakeParams::new());
        if computer {
            daemon.host().run_with_eq(chain.clone(), params.clone());
        } else {
            daemon.host().stop();
        }
        daemon.with_devices(stands.iter().map(|s| Arc::clone(&s.handle)).collect());
        Rig {
            daemon,
            dir,
            config,
            state,
            chain,
            params,
            stands,
        }
    }

    /// One scene request, as a socket client sends it.
    pub(super) async fn ask(&self, op: SceneOp) -> Response {
        self.daemon
            .handle_from(Origin::Socket, Request::Scene { op })
            .await
    }

    pub(super) async fn recall(&self, name: &str) -> Response {
        self.ask(SceneOp::Recall {
            name: name.into(),
            confirm: false,
        })
        .await
    }

    pub(super) async fn save(&self, name: &str) -> Response {
        self.ask(SceneOp::Save {
            name: name.into(),
            force: false,
        })
        .await
    }

    /// Write `scenes` to the config as the file spells them and have the daemon read it.
    pub(super) async fn add(&self, scenes: &[Scene]) {
        for scene in scenes {
            Config::add_scene(&self.config, scene).expect("a scene is added");
        }
        self.daemon.reload_all().await.expect("the reload");
    }

    /// The scenes' view as it is now.
    pub(super) fn view(&self) -> ScenesView {
        self.daemon.scenes_watch().borrow().clone()
    }

    /// The view once `done` holds of it, waiting no longer than five seconds for the changes.
    pub(super) async fn view_when(
        &self,
        what: &str,
        done: impl Fn(&ScenesView) -> bool,
    ) -> ScenesView {
        let mut watching = self.daemon.scenes_watch();
        let waited = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                {
                    let view = watching.borrow_and_update();
                    if done(&view) {
                        return view.clone();
                    }
                }
                if watching.changed().await.is_err() {
                    panic!("the view's sender is gone while waiting for {what}");
                }
            }
        })
        .await;
        match waited {
            Ok(view) => view,
            Err(_) => panic!("no view with {what}: {:?}", self.view()),
        }
    }

    /// The EQ of the fake computer as it is now.
    pub(super) fn eq(&self) -> EqView {
        self.daemon
            .host()
            .scene_eq()
            .expect("the computer is there")
    }
}

/// How many times the fake chain was applied.
pub(super) fn applies(chain: &FakeChain) -> usize {
    chain
        .calls()
        .iter()
        .filter(|call| matches!(call, svolder_host::fake::ChainCall::Apply(_)))
        .count()
}

/// A peaking filter.
pub(super) fn pk(f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter {
        kind: FilterKind::Peaking,
        f,
        g,
        q,
    }
}

/// A computer part that plays `filters` under `name` at `preamp_db` with every stage off.
pub(super) fn computer_part(
    name: &str,
    filters: Vec<SourceFilter>,
    preamp_db: f64,
) -> ComputerPart {
    ComputerPart {
        eq_on: true,
        name: name.to_string(),
        origin: None,
        preamp_db,
        filters,
        loudness: None,
        crossfeed_on: false,
        crossfeed: None,
        speakers_on: false,
        speakers: None,
    }
}

/// The built-in preset `name` of `profile`'s tone control, as a scene's tone: no source.
pub(super) fn tone_of(profile: &Profile, name: &str) -> Preset {
    let eq = profile.eq.as_ref().expect("a tone control");
    preset::find(&preset::builtins(eq), name)
        .expect("a built-in")
        .clone()
}

/// A device's part for the stand `on`, with `tone` and `controls` (key and raw step).
pub(super) fn part(on: &Stand, tone: Option<Preset>, controls: &[(&str, i64)]) -> DevicePart {
    DevicePart {
        udid: on.handle.udid.clone(),
        name: on.handle.name.clone(),
        tone,
        raw_bands: None,
        controls: controls
            .iter()
            .map(|(key, step)| ((*key).to_string(), *step))
            .collect(),
    }
}

/// A scene of `name` holding `computer` and `devices`.
pub(super) fn scene(name: &str, computer: Option<ComputerPart>, devices: Vec<DevicePart>) -> Scene {
    Scene {
        name: name.to_string(),
        computer,
        devices,
    }
}

/// The error answer's words and reason.
pub(super) fn refused(answer: Response) -> (String, Option<svolder_ipc::Reason>) {
    match answer {
        Response::Error {
            message, reason, ..
        } => (message, reason),
        other => panic!("not a refusal: {other:?}"),
    }
}

/// The outcome of a recall's answer, as (who, status, notes).
pub(super) fn outcomes(
    answer: Response,
) -> Vec<(
    String,
    svolder_ipc::socket::protocol::PartStatus,
    Vec<String>,
)> {
    match answer {
        Response::Scene { outcome, .. } => outcome
            .into_iter()
            .map(|part| (part.who, part.status, part.notes))
            .collect(),
        other => panic!("not a scene answer: {other:?}"),
    }
}

/// The raw step of function `id` among `readings`.
pub(super) fn reading(readings: &[Message], id: FunctionId) -> Option<i64> {
    readings
        .iter()
        .find(|m| m.id == id)
        .and_then(|m| m.value.as_i64())
}

/// The next scenes' view on `events`, whatever else comes first.
pub(super) async fn next_scenes(events: &mut BoxStream<'static, Event>) -> ScenesView {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(5), events.next())
            .await
            .expect("an event came")
            .expect("the stream is open");
        if let Event::Scenes(view) = event {
            return view;
        }
    }
}
