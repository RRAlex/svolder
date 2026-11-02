// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The half of a backend that owns a session: this process opens the transport, picks the
//! device the configuration names, and talks to it itself.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use svolder_ble::{
    Advertisement, BleError, DeviceState, Episode, FakeTransport, Session, Transport,
};
use svolder_ipc::{
    Config, DeviceConfig, PassphraseNote, ScanEntry, Signal, SignalSource, ValueObject,
    passphrase_verdict,
};
use svolder_profiles::roles;
use svolder_profiles::{
    EqMode, EqParam, Profile, ProfileError, Registry, apply_input_labels, describe, intent,
};
use svolder_proto::function::PASSPHRASE;
use svolder_proto::{FunctionId, Message, Passphrase, ProtoError, UnitId, Value, ValueType};
use tokio::task::JoinHandle;

use super::{
    DeviceRow, Failure, Globals, Identified, Update, WriteError, bad_request, config_error,
    no_link, refused, usage,
};
use crate::commands::device::{Selected, collect_unique, profile_for, select_device};
use crate::recovery::Kind;

/// The two direct transports a verb can open.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Wire {
    /// A real adapter through BlueZ.
    Ble,
    /// The in-process fake device.
    Fake,
}

impl Wire {
    /// The name `--transport` accepts and `-v` reports.
    pub(super) fn name(self) -> &'static str {
        match self {
            Wire::Ble => "ble",
            Wire::Fake => "fake",
        }
    }
}

/// How long a step sent is held against a relative step while no reading shows it: as long as a
/// writer holds the step it asked of a function.
const SENT_LIFETIME: Duration = Duration::from_secs(5);

/// A session this process opened, and the profile it reads that device through.
pub struct DirectBackend {
    /// Kept past `open` so a scan has something to listen on.
    transport: Arc<dyn Transport>,
    /// The profiles this session was opened with, which also name what a scan hears.
    registry: Registry,
    /// Absent for a verb that asked not to connect, and while a reconnect attempt owns it.
    pub(super) session: Option<Session>,
    pub(super) profile: Profile,
    /// The name the configuration file gives the device this session is on, for the front ends
    /// that ask which configured device they are watching. `None` for a device the file says
    /// nothing about, which a `-d` unit id or an advertisement alone can select. A unit id of a
    /// configured device is not one of those: it names that device.
    pub(super) configured: Option<String>,
    /// What the configuration says the device's passphrase is: `auto`, `none` or four digits,
    /// and `auto` for a device the file says nothing about.
    passphrase: String,
    /// How that passphrase disagrees with the advertisement the session holds, as of the last
    /// time it was judged. See [`DirectBackend::judge_passphrase`].
    note: Option<PassphraseNote>,
    /// Whether an entry was saved while a reconnect attempt held the session, which is applied
    /// to it when it is back.
    entry_waits: bool,
    /// Where the refusal episode of a device on `auto` stands: whether its one drop of the link
    /// is spent. See [`Episode`].
    episode: Episode,
    /// The reconnect attempt that carried the session off, until it hands it back.
    reconnecting: Option<Reconnect>,
    /// The readings as of the moment a reconnect attempt carried the session off, so a read
    /// still answers from the last known values while it runs.
    last_state: DeviceState,
    /// The step last sent to each function, when, and whether a relative step worked it out. A
    /// relative step goes on from it by the rule the daemon's writer uses, [`roles::next_step`]:
    /// a lowering whose echo is still on its way is never undone, and quick steps chain. Each
    /// send of a step replaces it, and a `raw` write forgets it. It is forgotten when a reading
    /// shows it, whether the session reads it in [`DirectBackend::next_status`] or a relative
    /// step finds it, when [`SENT_LIFETIME`] passes, when a write that waited is answered, and
    /// when the write that made it fails, other than by a timeout, which brings back the note
    /// before it. A waited write that timed out keeps it, since the device may have applied it.
    sent: HashMap<FunctionId, (i64, Instant, bool)>,
    /// What opening had to say that was not worth refusing to open over — a daemon this
    /// process tried to start and could not — for whoever opened this to say once, where its
    /// own messages go.
    pub(super) warnings: Vec<String>,
    /// When the advertisement the session holds was heard, in unix milliseconds: the open, or
    /// the listen of a later reconnect that took a fresh one. It is what the signal strength is
    /// stamped with. `None` while the advertisement is one the adapter held from before: its
    /// strength is of unknown age, so none is listed.
    heard_ms: Option<u64>,
}

/// What a reconnect attempt hands back: the session it took, how the connect went, and when it
/// took a fresh advertisement for the session, if it did (unix milliseconds).
///
/// The session travels into the attempt and out of it either way, because its readings are
/// what answers a read once the backend has it again.
type Attempt = (Session, Result<(), BleError>, Option<u64>);

/// A running reconnect attempt, as the backend that started it holds it.
struct Reconnect {
    /// The task running the connect, which hands the session back when it ends.
    handle: JoinHandle<Attempt>,
    /// Set to ask the attempt to close the session it connected instead of handing back a
    /// live link. The attempt reads it once its connect is done, so an attempt that has
    /// already opened a link still sees it and releases the device.
    cancel: Arc<AtomicBool>,
    /// How long a close waits for this attempt: the session's own grace, kept here because
    /// the attempt carried the session off with it.
    grace: Duration,
    /// The device the attempt is for, whose work a close waits for.
    unit: UnitId,
    /// Whether the link dropped on its own, so the attempt read the device's advertisement
    /// afresh and the code it keyed the session with says whether the code has changed.
    natural: bool,
}

impl DirectBackend {
    /// Open the transport and, when `connect`, select a device and connect to it.
    ///
    /// The configuration decides which device that is and how to talk to it: a `-d` selector
    /// naming a configured device, by its name or by its unit id, is matched by that device's
    /// unit id, and its `profile`, `passphrase` and input names override what the advertisement
    /// implies.
    pub(super) async fn open(
        g: &Globals<'_>,
        config: &Config,
        wire: Wire,
        connect: bool,
    ) -> Result<DirectBackend, Failure> {
        let (transport, adapter) = transport_for(g, config, wire).await?;
        open_over(g, config, transport, connect, wire, adapter.as_deref()).await
    }

    /// Judge the configured passphrase against the advertisement the session holds, and keep
    /// the verdict for [`DirectBackend::passphrase_note`].
    ///
    /// Best effort, and nothing the session does depends on it: the advertisement can be older
    /// than the device's code. The note stands until the device shows the code works — it
    /// answered the connect's refresh in full, or the echo of a waited `set` — or until the
    /// session takes a newer advertisement, which only a device on `auto` has it do (see
    /// [`Session::reread_code`]) and `auto` has no note. A device that shows its code works also
    /// ends the refusal episode. With no session there is no advertisement to judge against and
    /// no note.
    fn judge_passphrase(&mut self) {
        if let Some(session) = self.session.as_ref() {
            self.episode.observe(session);
        }
        self.note = self
            .session
            .as_ref()
            .and_then(|session| match session.code_accepted() {
                true => None,
                false => {
                    passphrase_verdict(&self.passphrase, &session.advertisement().manufacturer)
                }
            });
    }

    /// Take `entry`, the configuration file's new entry for the device this session is on, as
    /// the passphrase to run on, as a daemon does when it takes an entry in: the code it means
    /// against the advertisement the session holds (the one `auto` means, none, or the digits)
    /// is handed to the session, which lifts a refusal a changed code answers and owes the device
    /// a refresh, and the passphrase is judged again. An entry that means no code leaves the
    /// session as it was. While a reconnect attempt holds the session the entry is kept and
    /// applied when the session is back.
    pub(super) fn adopt_entry(&mut self, entry: &str) {
        if self.passphrase != entry {
            self.episode.end();
        }
        self.passphrase = entry.to_string();
        self.entry_waits = self.session.is_none();
        self.apply_entry();
    }

    /// Hand the session the code the entry this backend holds means, and judge again.
    fn apply_entry(&mut self) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let code = match self.passphrase.as_str() {
            "auto" => Ok(session.advertisement().manufacturer.passphrase()),
            text => Passphrase::from_text(text),
        };
        if let Ok(code) = code {
            session.set_passphrase(code);
        }
        self.judge_passphrase();
    }

    /// How the configured passphrase disagrees with the device's advertisement, as of the last
    /// time it was judged: at open, after a waited `set`, and after a reconnect.
    pub(super) fn passphrase_note(&self) -> Option<PassphraseNote> {
        self.note
    }

    /// The device as a list row, from what the session holds: the product, the profile it is
    /// read through, the signal strength of the advertisement it opened on (an advert reading,
    /// stamped when the session opened) and whether the passphrase was refused. `None` while
    /// no session is open. Whether the link is up and when it will be tried again are not
    /// the row's to say here.
    pub(super) fn own_row(&self) -> Option<DeviceRow> {
        let session = self.session.as_ref()?;
        let advert = session.advertisement();
        Some(DeviceRow {
            name: self
                .configured
                .clone()
                .unwrap_or_else(|| advert.unit_id().to_string()),
            product: advert.product_name().map(str::to_string),
            profile: Some(self.profile.name.clone()),
            product_id: Some(advert.manufacturer.product_id),
            signal: advert
                .rssi
                .zip(self.heard_ms)
                .map(|(dbm, heard_at_ms)| Signal {
                    dbm,
                    source: SignalSource::Advert,
                    heard_at_ms,
                }),
            refused: session.refused(),
            ..DeviceRow::default()
        })
    }

    /// Whether the device has refused the passphrase on the link the session holds now.
    pub(super) fn refused(&self) -> bool {
        self.session.as_ref().is_some_and(Session::refused)
    }

    /// Listen for `window` and keep the first advertisement seen for each unit id.
    pub(super) async fn scan(&self, window: Duration) -> Result<Vec<ScanEntry>, Failure> {
        let seen = collect_unique(self.transport.as_ref(), window)
            .await
            .map_err(|e| Failure::from_ble(&e))?;
        Ok(seen
            .iter()
            .map(|advert| crate::scanning::entry(advert, &self.registry))
            .collect())
    }

    /// Every reading the session holds, described through this device's profile.
    ///
    /// While a reconnect attempt owns the session these are the readings it took away, so a
    /// read during one answers with the last known values rather than with nothing.
    pub(super) fn state(&self) -> Vec<ValueObject> {
        let state = match self.session.as_ref() {
            Some(session) => session.state(),
            None if self.reconnecting.is_some() => &self.last_state,
            None => return Vec::new(),
        };
        state
            .messages()
            .map(|m| ValueObject::from_reading(&describe(&self.profile, m), m))
            .collect()
    }

    /// Every reading the device holds, or a refusal while a reconnect attempt owns the
    /// session.
    ///
    /// A snapshot is what a caller asks for when it wants the values the device holds *now*,
    /// so during a reconnect it is turned away rather than answered from
    /// [`DirectBackend::last_state`]: last-known values handed over as a fresh read would let
    /// a refresh report success for a read that never reached the device.
    pub(super) fn snapshot(&self) -> Result<Vec<ValueObject>, Failure> {
        match self.reconnecting.is_some() {
            true => Err(Failure::of(
                Kind::Dropped,
                "the link is reconnecting; nothing to read",
            )),
            false => Ok(self.state()),
        }
    }

    /// Set one control from a literal value, or step a level from what it last reported.
    pub(super) async fn set(
        &mut self,
        key: &str,
        value: &str,
        step: i64,
        no_wait: bool,
    ) -> Result<Option<ValueObject>, WriteError> {
        let way = value.to_ascii_lowercase();
        let relative = matches!(way.as_str(), "up" | "down");
        let message = match way.as_str() {
            "up" => self.relative_command(key, step)?,
            "down" => self.relative_command(key, step.saturating_neg())?,
            _ => intent(&self.profile, key, value).map_err(|e| bad_request(e.to_string()))?,
        };
        // The session reports a band under its own id, so the step sent is kept under that id.
        let (id, raw) = (self.profile.read_as(message.id), message.value.as_i64());
        // The step sent replaces the one before it, and is where the next relative step starts.
        let prior = self.sent.remove(&id);
        if let Some(raw) = raw {
            self.sent.insert(id, (raw, Instant::now(), relative));
        }
        let sent = self.send(message, no_wait).await;
        match &sent {
            // An answered write shows in the reading; one sent without waiting waits for its echo.
            Ok(Some(_)) => {
                self.sent.remove(&id);
            }
            Ok(None) => {}
            // A write that timed out may have landed, so its step stays.
            Err(WriteError::Ble(BleError::Timeout(_))) => {}
            // A write that was refused never changed the device, so the step before it stands.
            Err(_) => {
                self.sent.remove(&id);
                if let Some(prior) = prior {
                    self.sent.insert(id, prior);
                }
            }
        }
        sent
    }

    /// The command that nudges a level control from what the device last reported, and from the
    /// step last sent to it while no reading shows that, by [`roles::next_step`].
    ///
    /// A control that cannot be stepped is turned away before the current value is looked
    /// up, so `set mute up` complains about the control rather than about a reading a
    /// boolean was never going to supply.
    fn relative_command(&mut self, key: &str, delta: i64) -> Result<Message, WriteError> {
        // The checks every front end makes come first, before the reading is looked at.
        let control =
            roles::steppable(&self.profile, key).map_err(|e| bad_request(e.to_string()))?;
        let (id, range) = (control.id, control.range);
        let reading = self
            .session
            .as_ref()
            .and_then(|session| session.state().get(id))
            .and_then(|m| m.value.as_i64());
        let (Some(reading), Some(range)) = (reading, range) else {
            return Err(match self.session {
                None => self.no_session(),
                Some(_) => refused(
                    4,
                    crate::recovery::INSPECT,
                    format!("no current value for {key}"),
                ),
            });
        };
        let last = match self.sent.get(&id) {
            Some(&(raw, at, chained)) if reading != raw && at.elapsed() < SENT_LIFETIME => {
                Some((raw, chained))
            }
            _ => {
                self.sent.remove(&id);
                None
            }
        };
        let to = roles::next_step(last, reading, delta, range);
        intent(&self.profile, key, &to.to_string()).map_err(|e| bad_request(e.to_string()))
    }

    /// Send one command straight to a function id.
    pub(super) async fn raw(
        &mut self,
        id: &str,
        value_type: &str,
        value: &str,
        no_wait: bool,
    ) -> Result<Option<ValueObject>, WriteError> {
        let message = raw_message(id, value_type, value)?;
        self.sent.remove(&self.profile.read_as(message.id));
        self.send(message, no_wait).await
    }

    /// Set one band parameter; `band` is 1-based, as it is typed.
    pub(super) async fn eq_set(
        &mut self,
        band: usize,
        param: &str,
        value: f64,
        no_wait: bool,
    ) -> Result<Option<ValueObject>, WriteError> {
        let param: EqParam = param
            .parse()
            .map_err(|e: ProfileError| bad_request(e.to_string()))?;
        let band0 = band
            .checked_sub(1)
            .ok_or_else(|| bad_request("band numbers start at 1"))?;
        let message = svolder_profiles::eq_set(&self.profile, band0, param, value)
            .map_err(|e| bad_request(e.to_string()))?;
        self.send(message, no_wait).await
    }

    /// Select the tone-control mode.
    pub(super) async fn eq_mode(
        &mut self,
        mode: &str,
        no_wait: bool,
    ) -> Result<Option<ValueObject>, WriteError> {
        let mode: EqMode = mode
            .parse()
            .map_err(|e: ProfileError| bad_request(e.to_string()))?;
        let message = svolder_profiles::eq_mode(&self.profile, mode)
            .map_err(|e| bad_request(e.to_string()))?;
        self.send(message, no_wait).await
    }

    /// Send one command through the session: nothing to report for a no-wait, and otherwise
    /// the reading the device confirmed.
    ///
    /// Every frame this backend produces passes [`FunctionId::refusal`] here, whichever verb
    /// built it: `raw set` refuses a protected id before a transport is even opened, but a
    /// profile key mapped onto one would otherwise reach the device through `set`.
    async fn send(
        &mut self,
        message: Message,
        no_wait: bool,
    ) -> Result<Option<ValueObject>, WriteError> {
        if let Some(refusal) = message.id.refusal() {
            return Err(bad_request(refusal.to_string()));
        }
        let Some(session) = self.session.as_mut() else {
            return Err(self.no_session());
        };
        if no_wait {
            session
                .set_no_wait(message)
                .await
                .map_err(WriteError::Ble)?;
            return Ok(None);
        }
        // A recall or a save has no echo: the session sends it without waiting, and the answer
        // is the daemon's `Ack`, not a reading.
        let action = self.profile.action_of(message.id).is_some();
        let confirmed = match session.set(message).await {
            Ok(confirmed) => confirmed,
            Err(e) => {
                // A refusal ends what an earlier answer showed.
                self.judge_passphrase();
                return Err(WriteError::Ble(e));
            }
        };
        // An echo the device answered with no refusal shows the code works.
        self.judge_passphrase();
        if action {
            return Ok(None);
        }
        Ok(Some(ValueObject::from_reading(
            &describe(&self.profile, &confirmed),
            &confirmed,
        )))
    }

    /// Why there is no session to write through: a reconnect attempt has it, or this verb
    /// never asked for one. Both earn the exit code a lost link does.
    fn no_session(&self) -> WriteError {
        match self.reconnecting.is_some() {
            true => no_link("the link is reconnecting"),
            false => no_link("no session was opened for this verb"),
        }
    }

    /// Ask the device to show itself: one frame per [`svolder_ble::IDENTIFY_GAP`], and it goes
    /// out on a refused link too, so it does not pass through [`DirectBackend::send`]. A profile
    /// that offers no identify refuses it, link or not.
    pub(super) async fn identify(&mut self) -> Result<Identified, WriteError> {
        if !self.profile.identify {
            let device = self.configured.as_deref().unwrap_or(&self.profile.display);
            return Err(bad_request(format!("{device} offers no identify")));
        }
        let Some(session) = self.session.as_mut() else {
            return Err(self.no_session());
        };
        let sent = session.identify().await.map_err(WriteError::Ble)?;
        let device = self
            .configured
            .clone()
            .unwrap_or_else(|| session.advertisement().unit_id().to_string());
        Ok(Identified { device, sent })
    }

    /// Send function 250 with `code`'s encoded value, then use it for every frame after: the
    /// only sanctioned path to that function, and the one write that does not go through
    /// [`DirectBackend::send`], whose refusal gate turns every other route to it away.
    pub(super) async fn passphrase(&mut self, code: &str) -> Result<(), WriteError> {
        let p = Passphrase::from_text(code).map_err(|e| bad_request(e.to_string()))?;
        let Some(session) = self.session.as_mut() else {
            return Err(self.no_session());
        };
        session
            .set_no_wait(Message::command(PASSPHRASE, Value::U32(p.encoded())))
            .await
            .map_err(WriteError::Ble)?;
        session.set_passphrase(p);
        // The device advertises the new code from now on: the session's copy says so too.
        session.advertise_passphrase(p);
        Ok(())
    }

    /// The next thing the device has to say: a reading, or the link going down or coming back.
    ///
    /// Readings are read through [`Session::recv`], which never reconnects behind the caller's
    /// back, so a lost link is reported the moment it is noticed: `Connection { connected: false
    /// }` comes straight back, and the next call is the one that reconnects (`Connection {
    /// connected: true }` once the device answers, `Ok(None)` if not within the session's
    /// connect deadline). That is the same sequence the daemon publishes, so a watching verb
    /// reads the same updates either way, with the reconnect between the two.
    pub(super) async fn next_status(&mut self) -> Result<Option<Update>, BleError> {
        if self.reconnecting.is_some() {
            return self.await_reconnect().await;
        }
        let Some(session) = self.session.as_mut() else {
            return Ok(None);
        };
        // A device on `auto` that refused its code on this link, on the connect's refresh or
        // on a write or a status since, has the link released once and the code read afresh,
        // in the same sequence a lost link has: down, then back.
        let auto = self.passphrase == "auto";
        if self.episode.due(auto, session) {
            self.episode.dropped(session.passphrase());
            self.episode.listened();
            self.start_reconnect(true);
            return Ok(Some(Update::Connection { connected: false }));
        }
        let heard = session.recv().await;
        // A refresh a wake owed may have shown the code works, and a refusal ends what an
        // earlier answer showed.
        self.judge_passphrase();
        if let Some(m) = &heard
            && self
                .sent
                .get(&m.id)
                .is_some_and(|&(raw, ..)| m.value.as_i64() == Some(raw))
        {
            self.sent.remove(&m.id);
        }
        match heard {
            Some(m) => Ok(Some(Update::Status(ValueObject::from_reading(
                &describe(&self.profile, &m),
                &m,
            )))),
            None => {
                self.start_reconnect(false);
                Ok(Some(Update::Connection { connected: false }))
            }
        }
    }

    /// Hand the session to a task that reconnects it, keeping the readings it held.
    ///
    /// With `reread`, set when the device just refused its code, the task first releases the
    /// link, listens for the device's next advertisement for one scan window
    /// ([`Session::reread_code`]) and keys the session from it, and only then connects: the
    /// reconnect of a device on `auto` that has to read its code afresh. Whatever the listen
    /// hears or does not, the connect follows.
    ///
    /// Without it, a device on `auto` whose link dropped on its own is listened for the same way
    /// before the connect, as the daemon's reconnect does, and the session is keyed from what
    /// is heard: a device power-cycled to advertise another code is read, and
    /// [`DirectBackend::await_reconnect`] says whether the code changed.
    ///
    /// The task closes the session itself when the cancel flag is set by the time its
    /// connect is done, so a backend that stopped waiting never leaves a link open behind
    /// it: the only code that can release a link the attempt opened is the attempt.
    fn start_reconnect(&mut self, reread: bool) {
        let Some(mut session) = self.session.take() else {
            return;
        };
        self.last_state = session.state().clone();
        let grace = session.config().close_grace;
        let unit = session.advertisement().unit_id();
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let auto = self.passphrase == "auto";
        let transport = Arc::clone(&self.transport);
        let handle = tokio::spawn(async move {
            let mut heard = None;
            if reread {
                if session.reread_code(crate::scanning::WINDOW).await {
                    heard = Some(unix_ms());
                }
            } else if auto
                && let Ok(Some(advert)) = transport.sight(unit, crate::scanning::WINDOW).await
            {
                session.set_passphrase(advert.manufacturer.passphrase());
                session.set_advertisement(advert);
                heard = Some(unix_ms());
            }
            let outcome = session.connect().await;
            if flag.load(Ordering::SeqCst) {
                session.close().await;
            }
            (session, outcome, heard)
        });
        self.reconnecting = Some(Reconnect {
            handle,
            cancel,
            grace,
            unit,
            natural: !reread,
        });
    }

    /// Wait for the running reconnect attempt and take its session back.
    ///
    /// The wait is on `&mut JoinHandle`, which the task outlives: a caller that drops this
    /// future — a front end whose `select!` woke on something else — leaves the attempt
    /// running and finds it again on its next call, rather than aborting a connect part way
    /// through and starting another.
    async fn await_reconnect(&mut self) -> Result<Option<Update>, BleError> {
        let (finished, natural) = {
            let attempt = self.reconnecting.as_mut().expect("a running attempt");
            ((&mut attempt.handle).await, attempt.natural)
        };
        self.reconnecting = None;
        match finished {
            Ok((session, outcome, heard)) => {
                if let Some(ms) = heard {
                    self.heard_ms = Some(ms);
                }
                // A reconnect after a link that dropped on its own read the device's code afresh:
                // another code than the refused one is another episode.
                if natural && self.passphrase == "auto" {
                    self.episode.code_seen(session.passphrase());
                }
                // A link that comes back refused after the drop is the same refusal.
                if outcome.is_ok() && session.refused() {
                    self.episode.refused_again(session.passphrase());
                }
                self.session = Some(session);
                match std::mem::take(&mut self.entry_waits) {
                    true => self.apply_entry(),
                    false => self.judge_passphrase(),
                }
                match outcome {
                    Ok(()) => Ok(Some(Update::Connection { connected: true })),
                    Err(_) => Ok(None),
                }
            }
            Err(e) => Err(BleError::Connection(format!(
                "the reconnect attempt failed: {e}"
            ))),
        }
    }

    /// Wait out a running reconnect attempt, then close whatever session it hands back, so a
    /// Forkbeard device is free for the next central.
    ///
    /// The attempt is asked to stand down before the wait starts, so one that finishes inside
    /// the grace closes the link it opened itself, and the session comes back with no link left
    /// to release. The wait is the session's `close_grace`, not the whole connect deadline: a
    /// user who quits during a reconnect is leaving, so an attempt still deep in a connect is
    /// stopped rather than sat out. A stopped attempt never reads the flag, so the link its
    /// connect may still be opening belongs to the transport, given the same grace to take it
    /// back down, once the aborted attempt has dropped, which is waited for within the grace.
    pub(super) async fn close(&mut self) {
        if let Some(mut attempt) = self.reconnecting.take() {
            // Set before the wait, so an attempt whose connect finishes during the grace
            // reads it and releases the device rather than handing back a live link.
            attempt.cancel.store(true, Ordering::SeqCst);
            match tokio::time::timeout(attempt.grace, &mut attempt.handle).await {
                Ok(Ok((session, _, _))) => self.session = Some(session),
                // The attempt panicked: there is no session to take back or stop.
                Ok(Err(_)) => {}
                Err(_) => {
                    attempt.handle.abort();
                    // The abort takes effect once the attempt's future is dropped, which can
                    // be after this task yields, and dropping it may record a release: wait
                    // for the drop before looking at what the transport has in flight.
                    let _ = tokio::time::timeout(attempt.grace, &mut attempt.handle).await;
                    // Stopping the attempt leaves whatever its connect had opened to the
                    // transport, and nothing drives that once this process leaves.
                    self.transport.settle_for(attempt.unit, attempt.grace).await;
                }
            }
        }
        if let Some(session) = self.session.as_mut() {
            session.close().await;
        }
    }
}

/// Build the transport the configuration and the command line choose for `wire`: on the
/// adapter [`preferred_adapter`] picks, which is the one that last reached the configured
/// device when none is named. The adapter it was opened on comes back with it, for
/// [`open_over`].
pub(crate) async fn transport_for(
    g: &Globals<'_>,
    config: &Config,
    wire: Wire,
) -> Result<(Arc<dyn Transport>, Option<String>), Failure> {
    let chosen = choose_configured(config, g.device);
    let remembered = last_adapter(wire, chosen).await;
    let adapter = preferred_adapter(g.adapter, config.adapter.as_deref(), remembered);
    let transport = open_transport(wire, adapter.as_deref()).await?;
    Ok((transport, adapter))
}

/// Open a direct backend over `transport` and, when `connect`, select a device and connect to
/// it, exactly as [`DirectBackend::open`] does over the transport it builds.
///
/// A transport that wraps another one, to watch the frames that pass, goes in here, with the
/// `wire` and the `adapter` [`transport_for`] opened it on: the adapter is remembered for the
/// device, as it is when [`DirectBackend::open`] builds the transport itself.
pub(crate) async fn open_over(
    g: &Globals<'_>,
    config: &Config,
    transport: Arc<dyn Transport>,
    connect: bool,
    wire: Wire,
    adapter: Option<&str>,
) -> Result<DirectBackend, Failure> {
    let registry = super::registry()?;
    let chosen = choose_configured(config, g.device);
    if !connect {
        return Ok(DirectBackend {
            transport,
            registry,
            session: None,
            profile: Registry::generic(0),
            configured: chosen.map(|device| device.name.clone()),
            passphrase: configured_text(chosen),
            note: None,
            entry_waits: false,
            episode: Episode::default(),
            reconnecting: None,
            last_state: DeviceState::default(),
            sent: HashMap::new(),
            warnings: Vec::new(),
            heard_ms: None,
        });
    }
    let selector = match chosen {
        Some(device) => Some(device.udid.as_str()),
        None => g.device,
    };
    let (session, profile, episode, heard) = reach(&transport, &registry, chosen, selector).await?;
    let unit = session.advertisement().unit_id();
    // The adapter that connected is the one that was named, and where none was, the one
    // the transport took for itself: without that second answer the state file could only
    // ever differ from the config on a run that typed `--adapter`, which is the one run
    // that needs it least.
    let took = adapter_taken(adapter, transport.adapter());
    // Only a profile the registry can find again is worth remembering: a generic one is
    // named for its product id and would be an unknown name on the next read.
    let known = registry.get(&profile.name).map(|_| profile.name.as_str());
    remember_adapter(wire, took.as_deref(), &unit.to_string(), known).await;
    let mut backend = DirectBackend {
        transport,
        registry,
        session: Some(session),
        profile,
        configured: chosen.map(|device| device.name.clone()),
        passphrase: configured_text(chosen),
        note: None,
        entry_waits: false,
        episode,
        reconnecting: None,
        last_state: DeviceState::default(),
        sent: HashMap::new(),
        warnings: Vec::new(),
        heard_ms: heard.then(unix_ms),
    };
    backend.judge_passphrase();
    Ok(backend)
}

/// Build the transport a direct backend talks through, on the controller `adapter` names for
/// `ble` — [`preferred_adapter`] is what chooses that name, and `None` takes the machine's
/// first.
pub async fn open_transport(
    wire: Wire,
    adapter: Option<&str>,
) -> Result<Arc<dyn Transport>, Failure> {
    match wire {
        Wire::Fake => crate::registry()
            .and_then(|registry| FakeTransport::from_env(&registry))
            .map(|t| Arc::new(t) as Arc<dyn Transport>)
            .map_err(usage),
        Wire::Ble => svolder_ble::BluezTransport::new(adapter)
            .await
            .map(|t| Arc::new(t) as Arc<dyn Transport>)
            .map_err(|e| adapter_failure(&e, adapter)),
    }
}

/// Why a BlueZ transport would not open, in the words the error itself gives it.
///
/// An adapter failure takes its kind from whether one was named: a machine with no adapter at
/// all and a named adapter that is not there are different things to have happened, even
/// though the step and the exit code they earn are the same.
fn adapter_failure(err: &BleError, adapter: Option<&str>) -> Failure {
    match (err, adapter) {
        (BleError::Adapter(_), None) => Failure::of(Kind::NoAdapter, err.to_string()),
        _ => Failure::from_ble(err),
    }
}

/// What the state file says last connected this device, read off the async path.
///
/// It is only asked for when the configuration says which device this session is for — a `-d`
/// naming a device that is not configured, or no device at all, leaves the run with no udid to
/// look up — and only for a real radio, since the fake transport has no adapters to choose
/// between. Reading it is a file read, so it happens on a blocking thread rather than on the
/// one a front end draws with.
async fn last_adapter(wire: Wire, chosen: Option<&DeviceConfig>) -> Option<String> {
    let (Wire::Ble, Some(device)) = (wire, chosen) else {
        return None;
    };
    let udid = device.udid.clone();
    let state = tokio::task::spawn_blocking(crate::state::DeviceState::load)
        .await
        .ok()?;
    state.last_adapter(&udid)
}

/// Which adapter a direct session opens on: `--adapter`, else the one that last connected this
/// device, else the configured `adapter`, else whichever the transport takes first.
///
/// The remembered adapter sits between the first two because it is the more specific answer: a
/// machine with a dongle and a deaf built-in controller has one adapter that reaches a given
/// device, and the last one that did is the best guess for the next time. `None` is what leaves
/// the transport to take the first adapter the machine has.
fn preferred_adapter(
    named: Option<&str>,
    configured: Option<&str>,
    remembered: Option<String>,
) -> Option<String> {
    named
        .map(str::to_string)
        .or(remembered)
        .or_else(|| configured.map(str::to_string))
}

/// The adapter a session really ran on: the one that was named for it, else the one the
/// transport reports having taken.
fn adapter_taken(named: Option<&str>, transport: Option<&str>) -> Option<String> {
    named.or(transport).map(str::to_string)
}

/// Keep the adapter that connected `unit` and the profile it was read through, so the next run
/// tries that adapter first and `eq list` can name the device's tone control without connecting.
///
/// `adapter` is the controller the session really ran on: the one that was named, or the one the
/// transport reports taking when nothing named any. The file has the last word on whether there
/// is anything to write, so connecting the same device over the same adapter and profile, the
/// steady state, writes nothing. The rest happens on a blocking thread, and a failed write is
/// dropped: what this file holds is worth a faster connect and nothing more, and a session that
/// is up is no place to report a read-only state directory.
async fn remember_adapter(wire: Wire, adapter: Option<&str>, unit: &str, profile: Option<&str>) {
    if wire != Wire::Ble {
        return;
    }
    let Some(adapter) = adapter else {
        return;
    };
    let (adapter, unit) = (adapter.to_string(), unit.to_string());
    let profile = profile.map(str::to_string);
    let _ = tokio::task::spawn_blocking(move || {
        let mut state = crate::state::DeviceState::load();
        if state.remember(&unit, &adapter, profile.as_deref()) {
            let _ = state.save();
        }
    })
    .await;
}

/// The command frame `raw` sends: the function id, the value type and the value, each read as
/// text. A text that does not parse, or a value that does not fit its type, is a usage error,
/// found before a session is involved.
pub(super) fn raw_message(id: &str, value_type: &str, value: &str) -> Result<Message, WriteError> {
    let id: FunctionId = id
        .parse()
        .map_err(|e: ProtoError| bad_request(e.to_string()))?;
    let value_type: ValueType = value_type
        .parse()
        .map_err(|e: ProtoError| bad_request(e.to_string()))?;
    let value = Value::from_text(value_type, value).map_err(|e| bad_request(e.to_string()))?;
    Ok(Message::command(id, value))
}

/// The configured device a selector names: its name, else its unit id or a prefix of it that
/// no other configured device shares, else the configured default, else the only configured
/// device. `None` leaves the choice to the devices in range.
///
/// A `default_device` naming no configured device finds nothing here, and reads as no device:
/// `Backend::open` refuses it before it asks, but `eq list` asks without opening a backend.
pub(crate) fn choose_configured<'c>(
    config: &'c Config,
    selector: Option<&str>,
) -> Option<&'c DeviceConfig> {
    if let Some(sel) = selector {
        return by_name(config, sel).or_else(|| by_unit(config, sel));
    }
    if let Some(name) = config.default_device.as_deref() {
        return by_name(config, name);
    }
    match config.devices.as_slice() {
        [only] => Some(only),
        _ => None,
    }
}

/// The configured device with this name, case aside.
fn by_name<'c>(config: &'c Config, name: &str) -> Option<&'c DeviceConfig> {
    config
        .devices
        .iter()
        .find(|d| d.name.eq_ignore_ascii_case(name))
}

/// The one configured device whose unit id `selector` names, read by the rule every device
/// selector uses. A prefix that fits several devices names none of them.
fn by_unit<'c>(config: &'c Config, selector: &str) -> Option<&'c DeviceConfig> {
    if selector.is_empty() {
        return None;
    }
    let mut named = config
        .devices
        .iter()
        .filter(|d| crate::scanning::names_device(&d.udid, None, selector));
    let first = named.next()?;
    named.next().is_none().then_some(first)
}

/// Pick the device `selector` names on `transport` and connect a session to it: the radio half
/// of opening a direct backend.
///
/// That is one scan and one connect, and a second connect when a device on `auto` refuses the
/// first: its code is then read afresh, once. The selection listens for the device — up to the
/// end of its first advertisement when `selector` is a full unit id — and the connect opens its
/// link on what that listen heard, so nothing scans for the device twice. `chosen` is the
/// configured device `selector` came from, whose profile and passphrase override what the
/// advertisement implies. The last of what comes back says whether a window heard the
/// advertisement the session opened on, rather than the adapter holding it from before.
async fn reach(
    transport: &Arc<dyn Transport>,
    registry: &Registry,
    chosen: Option<&DeviceConfig>,
    selector: Option<&str>,
) -> Result<(Session, Profile, Episode, bool), Failure> {
    let Selected { advert, heard } =
        select_device(transport, registry, selector, crate::scanning::WINDOW).await?;
    let profile = configured_profile(registry, chosen, &advert);
    let passphrase = configured_passphrase(chosen, &advert).map_err(config_error)?;
    let mut session = Session::new(Arc::clone(transport), advert, profile.clone(), passphrase);
    session.connect().await.map_err(|e| Failure::from_ble(&e))?;
    // A device on `auto` that refuses the connect's refresh has the link released once and its
    // code read afresh, as a session that is watched does later, so a verb that opens a
    // session, which has no later, gets the same chance.
    let mut episode = Episode::default();
    if episode.due(configured_text(chosen) == "auto", &session) {
        episode.dropped(session.passphrase());
        episode.listened();
        session.reread_code(crate::scanning::WINDOW).await;
        // A device that cannot be reached again has still refused the code, and that is what
        // the verb learned: a lost link would send the user to the radio.
        if session.connect().await.is_err() {
            return Err(Failure::of(
                Kind::Refused,
                "the device refused the passphrase, and could not be reached again to read its code afresh",
            ));
        }
        if session.refused() {
            episode.refused_again(session.passphrase());
        }
    }
    Ok((session, profile, episode, heard))
}

/// The profile for a direct session: the configured one, else the registry's pick for the
/// advertised product and firmware, with the configured input names laid over it.
fn configured_profile(
    registry: &Registry,
    chosen: Option<&DeviceConfig>,
    advert: &Advertisement,
) -> Profile {
    let mut profile = chosen
        .and_then(|d| d.profile.as_deref())
        .and_then(|name| registry.get(name).cloned())
        .unwrap_or_else(|| profile_for(registry, advert));
    if let Some(device) = chosen {
        apply_input_labels(&mut profile, &device.inputs);
    }
    profile
}

/// What the configuration says the device's passphrase is, `auto` where it says nothing.
fn configured_text(chosen: Option<&DeviceConfig>) -> String {
    chosen.map_or_else(|| "auto".to_string(), |device| device.passphrase.clone())
}

/// The passphrase to encode commands with: the advertised one for `auto` and for an
/// unconfigured device, none for `none`, else the configured digits.
fn configured_passphrase(
    chosen: Option<&DeviceConfig>,
    advert: &Advertisement,
) -> Result<Passphrase, String> {
    match chosen.map(|d| d.passphrase.as_str()) {
        None | Some("auto") => Ok(advert.manufacturer.passphrase()),
        Some(text) => Passphrase::from_text(text).map_err(|e| e.to_string()),
    }
}

/// The unix wall clock in milliseconds.
fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(super) mod test_support;
