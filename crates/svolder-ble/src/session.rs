// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One device's live session: connect, keep state fresh, set values, and reconnect.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use futures::StreamExt;
use svolder_profiles::{EqParam, Profile};
use svolder_proto::function::{FIRMWARE_FAMILY, FIRMWARE_VERSION_QUERY};
use svolder_proto::{
    FunctionId, Message, Notification, Passphrase, Value, decode_notification, encode_command,
};

use crate::advert::Advertisement;
use crate::episode::listen_afresh;
use crate::error::BleError;
use crate::power::{PowerRule, PowerWatch};
use crate::state::DeviceState;
use crate::transport::{Link, Notifications, Transport};

/// The least time between two identify frames on one link: a request inside it sends nothing.
pub const IDENTIFY_GAP: Duration = Duration::from_secs(2);

/// The default [`SessionConfig::firmware_delay`]: the official app's 300 ms.
const FIRMWARE_ASK_DELAY: Duration = Duration::from_millis(300);

/// Timing knobs for a session, all with field-tuned defaults.
#[derive(Debug, Clone, Copy)]
pub struct SessionConfig {
    /// How long `set` waits for the echo status before giving up.
    pub set_timeout: Duration,
    /// Least gap between two writes, standing in for waiting on each write's completion, which
    /// a write without response over BlueZ does not report (`docs/protocol.md`, "Writes").
    pub write_spacing: Duration,
    /// How long the firmware ask waits before each `210.2.<subsystem>`: after the refresh for
    /// the first, after the previous subsystem's report for each next one.
    pub firmware_delay: Duration,
    /// How long after a recall the session waits for the device to report the readings the
    /// recall changes, before it asks for a refresh instead. A unit that reports its bands after
    /// a recall cancels the wait, so the refresh is never written; one that stays quiet gets
    /// the refresh, once.
    pub recall_guard: Duration,
    /// Quiet gap that marks the end of a refresh burst.
    pub drain_idle: Duration,
    /// How long a refresh burst may run before it counts as over, whatever the device is
    /// still saying.
    pub drain_bound: Duration,
    /// First reconnect backoff.
    pub backoff_min: Duration,
    /// Longest reconnect backoff.
    pub backoff_max: Duration,
    /// How long `connect` keeps retrying before failing.
    pub connect_deadline: Duration,
    /// How long a caller closing down waits for a running connect attempt to hand the
    /// session back, before it stops waiting and leaves the attempt behind.
    pub close_grace: Duration,
}

/// The default [`SessionConfig::close_grace`]: long enough for a connect already under way to
/// finish and be closed, short enough that someone quitting is not made to sit out a whole
/// deadline. A caller that bounds the wait for a closing session derives its bound from this.
pub const CLOSE_GRACE: Duration = Duration::from_secs(8);

/// The longest an actor closing its session can take to answer a shutdown that finds a
/// connect attempt in flight, with the default grace. An attempt that stands down in time takes
/// the wait for it to stand down and the settle the close ends with; one that is stopped takes
/// that wait, the wait for the aborted attempt to drop, and the transport's settle. Each is one
/// [`CLOSE_GRACE`]. A caller that waits for such an actor derives its wait from
/// this, plus a margin for the disconnect and the write spacing.
pub const FAREWELL_BOUND: Duration = CLOSE_GRACE.saturating_mul(3);

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            set_timeout: Duration::from_secs(2),
            write_spacing: Duration::from_millis(20),
            firmware_delay: FIRMWARE_ASK_DELAY,
            recall_guard: Duration::from_secs(1),
            drain_idle: Duration::from_millis(300),
            // A device whose meters follow the music reports ten times a second and leaves no
            // quiet gap at all; what ends its refresh is the profile's own completed function
            // set, not this. This is the bound behind both endings: long enough for a real
            // refresh burst, which is over in well under a second, to reach one of them, and a
            // backstop for whatever reaches neither — an unprofiled device, or one that never
            // stops talking.
            drain_bound: Duration::from_secs(2),
            backoff_min: Duration::from_millis(100),
            backoff_max: Duration::from_secs(3),
            // The deadline covers the connect attempts and the backoffs between them, and
            // nothing else: a caller scans before it opens a session, not inside this. On a
            // real adapter one connect takes four to seven seconds, and one issued straight
            // after a disconnect is refused for several more, so twenty leaves room for a
            // refusal and the retry that gets past it.
            connect_deadline: Duration::from_secs(20),
            close_grace: CLOSE_GRACE,
        }
    }
}

/// The most messages the session holds for a caller that is not reading them. A caller that
/// keeps calling `set` without draining `recv` stays at this length; the oldest messages go
/// first. No ordinary burst of notifications comes near it.
pub const PENDING_CAP: usize = 1024;

/// A refresh burst the session has asked for and is still waiting to hear, the reply to a
/// function 1 it wrote on its own account.
struct Burst {
    /// How many more times each function has to report before the burst is over. A refresh
    /// written twice, because a drop left it unclear whether the first went out, owes each
    /// function twice.
    owed: BTreeMap<FunctionId, u32>,
    /// Whether the profile names any function at all: an empty set ends nothing, so the burst
    /// then ends on a quiet gap or the bound.
    ends_when_complete: bool,
    /// When the burst counts as over whatever the device is still saying.
    bound: Instant,
    /// The code the refresh that set the burst off was written under: the burst's answers
    /// prove that code, and no other.
    code: Passphrase,
}

/// A refresh the session will ask for if the device does not report what an action changed in
/// time.
struct Guard {
    /// When the refresh falls due.
    due: Instant,
    /// The functions whose report cancels it: the readings the action changes.
    watched: BTreeSet<FunctionId>,
}

/// A live session with one device.
pub struct Session {
    transport: Arc<dyn Transport>,
    advert: Advertisement,
    profile: Profile,
    passphrase: Passphrase,
    config: SessionConfig,
    link: Option<Box<dyn Link>>,
    notifications: Option<Notifications>,
    state: DeviceState,
    /// Messages already folded into state but not yet handed to a caller: the ones a frame
    /// carried beyond its first, and the ones `set` read while waiting for its echo.
    pending: VecDeque<Message>,
    /// The `auto_off` replies that `fold` queued and no write has sent yet. They are encoded
    /// when written, with the passphrase the session holds then.
    replies: VecDeque<Message>,
    /// When the last write the link took went out. The next write waits until `write_spacing`
    /// after it, whatever became of the call that made that write.
    last_write: Option<Instant>,
    /// Where the device's power state stands. It lives as long as the session, so a device
    /// that was in standby when the link dropped and is on when it comes back is noticed.
    power: PowerWatch,
    /// A refresh the session owes the device and has not written: the device came back on, or a
    /// corrected passphrase lifted a refusal. Written by the next `recv`, `set` or `refresh`.
    refresh_due: bool,
    /// The burst the last such refresh set off, until it is complete, or has gone quiet, or has
    /// run past its bound.
    burst: Option<Burst>,
    /// True from the first message `pending` dropped at its cap until `pending` empties, so
    /// one run of overflow is reported once.
    overflowing: bool,
    /// Whether the device has answered one of this session's own frames without refusing it,
    /// under the code the session holds now. See [`Session::code_accepted`].
    accepted: bool,
    /// When the last identify frame went out on the link held now, for [`IDENTIFY_GAP`].
    last_identify: Option<Instant>,
    /// How many of the profile's firmware subsystems have been asked for on the link held now,
    /// `None` until the ask begins: a refresh that follows (the device coming back on) does not
    /// ask again, and an ask cut short carries on from where it stopped.
    firmware_asked: Option<usize>,
    /// When the next subsystem's ask goes out: [`SessionConfig::firmware_delay`] after the
    /// refresh that began the ask, or after the report of the subsystem asked last. `None` while
    /// that report is awaited, and once every subsystem has been asked. [`Session::recv`] wakes
    /// for it.
    firmware_next: Option<Instant>,
    /// Whether the firmware ask waits for the code to be proven: set when a corrected code lifts
    /// a refusal, since the link then holds a code no answer has vouched for yet. The ask goes
    /// out once [`Session::code_accepted`] says so, and a correction that is refused again asks
    /// nothing. A link whose connect was not refused starts its ask at once.
    firmware_needs_proof: bool,
    /// The refresh a recall has armed and no report of the readings it changes has cancelled
    /// yet. [`Session::recv`] wakes for it.
    guard: Option<Guard>,
    /// Whether any control of the profile reads extra ids, so that `fold` has statuses to
    /// rewrite: a profile with none pays nothing per message.
    reads_aliases: bool,
    /// How many runs of overflow have been reported.
    #[cfg(test)]
    overflow_runs: usize,
}

impl Session {
    /// A session with default timing.
    pub fn new(
        transport: Arc<dyn Transport>,
        advert: Advertisement,
        profile: Profile,
        passphrase: Passphrase,
    ) -> Self {
        Self::with_config(
            transport,
            advert,
            profile,
            passphrase,
            SessionConfig::default(),
        )
    }

    /// A session with explicit timing.
    pub fn with_config(
        transport: Arc<dyn Transport>,
        advert: Advertisement,
        profile: Profile,
        passphrase: Passphrase,
        config: SessionConfig,
    ) -> Self {
        let reads_aliases = has_reads(&profile);
        Self {
            transport,
            advert,
            profile,
            passphrase,
            config,
            link: None,
            notifications: None,
            state: DeviceState::default(),
            pending: VecDeque::new(),
            replies: VecDeque::new(),
            power: PowerWatch::default(),
            refresh_due: false,
            burst: None,
            last_write: None,
            overflowing: false,
            accepted: false,
            last_identify: None,
            firmware_asked: None,
            firmware_next: None,
            firmware_needs_proof: false,
            guard: None,
            reads_aliases,
            #[cfg(test)]
            overflow_runs: 0,
        }
    }

    /// The device's advertisement.
    pub fn advertisement(&self) -> &Advertisement {
        &self.advert
    }

    /// Say that the device now advertises `passphrase`: a caller that has changed the device's
    /// code with a `250` command keeps the session's copy of the advertisement in step with what
    /// the device holds, so what is read from it (the code `auto` means) is the new one.
    pub fn advertise_passphrase(&mut self, passphrase: Passphrase) {
        self.advert.set_advertised_passphrase(passphrase);
    }

    /// Replace the advertisement the session holds with a newer one from the same device: what
    /// is read from it (the code `auto` means) is then the newer one. The link is untouched.
    pub fn set_advertisement(&mut self, advert: Advertisement) {
        self.advert = advert;
    }

    /// The code every frame is encoded with now.
    pub fn passphrase(&self) -> Passphrase {
        self.passphrase
    }

    /// Whether the device has refused the code on the link the session holds now: the link is
    /// up, a `250.255` came back on it, and no changed code has lifted that since. A refusal an
    /// earlier link earned does not count: [`Session::connect`] forgets it.
    pub fn refused(&self) -> bool {
        self.link.is_some() && self.state.passphrase_rejected()
    }

    /// The profile driving this session.
    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    /// Hand the session a reloaded profile. Its controls and tone control are what the next
    /// refresh burst waits to hear and what incoming statuses are looked up in; the link and
    /// the readings gathered so far are untouched.
    pub fn set_profile(&mut self, profile: Profile) {
        self.reads_aliases = has_reads(&profile);
        self.profile = profile;
    }

    /// The last confirmed state of every control.
    pub fn state(&self) -> &DeviceState {
        &self.state
    }

    /// The timing knobs this session runs with.
    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    /// True while the session holds a link to the device.
    pub fn is_connected(&self) -> bool {
        self.link.is_some()
    }

    /// Use `passphrase` for every frame from now on, clearing a passphrase-rejected refusal
    /// only when the code actually changes.
    ///
    /// The caller changes the device's passphrase with a `250` command and then tells the
    /// session, or corrects a misconfigured code; either way frames after this are encoded
    /// with what the device now expects. A refusal belongs to the link it came on. On that link
    /// it leaves the session refusing every write before it builds one, so a wrong code is not
    /// sent repeatedly, and only a *different* code lifts it and lets the next write reach the
    /// device again.
    ///
    /// Handing back the same code again does not lift the refusal on the link that earned it:
    /// a reload hands every device its entry whether or not it changed, and a code still wrong
    /// stays refused there. A new link starts unrefused instead ([`Session::connect`] forgets
    /// the refusal), and its refresh asks the device again.
    ///
    /// A changed code that lifts a refusal on a link that is up also owes the device a
    /// refresh: the connect's own burst was refused, so what it would have read is still
    /// unread. The next `recv`, `set` or `refresh` writes it. Under a code that is still wrong
    /// the device refuses it again, which is no worse than not having asked. The firmware
    /// versions are not asked for until the corrected code's burst has proven it.
    pub fn set_passphrase(&mut self, passphrase: Passphrase) {
        if self.passphrase != passphrase {
            self.accepted = false;
            if self.state.passphrase_rejected() && self.link.is_some() {
                self.refresh_due = true;
                self.firmware_needs_proof = true;
            }
            self.state.clear_passphrase_rejected();
        }
        self.passphrase = passphrase;
    }

    /// Whether the device has shown that the code the session holds works: it answered one of
    /// the session's own frames without refusing it. Two answers count. The refresh burst a
    /// connect takes counts when every function the profile names reported in reply to it, and
    /// a [`Session::set`] counts when its echo came back. A status the device sends on its own
    /// proves nothing, since it sends those whatever the code, so a profile that names no
    /// function never proves the code with a refresh.
    ///
    /// The proof is about the link and the code of now: a new connect, a different code and a
    /// refusal all end it, and a session that has not yet been answered says `false` rather than
    /// that the code is wrong.
    pub fn code_accepted(&self) -> bool {
        self.accepted && !self.state.passphrase_rejected()
    }

    /// The next status on the link the session already holds, folded into state.
    ///
    /// Every message the device sends comes out of here exactly once, one per call and in
    /// frame order, including messages [`Session::set`] read while waiting for its echo, with
    /// two exceptions: a status a confirmed echo of the same function has superseded is dropped
    /// when the echo is confirmed, and the rest of a notification that carries a refusal is
    /// dropped when [`Session::set`] is waiting for a refresh burst. A notification that
    /// decodes to nothing but dropped messages is skipped.
    ///
    /// Unlike [`Session::next_status`] this never reconnects: `None` means there is no link or
    /// the stream ended, with the link released and state marked stale first either way; the
    /// caller decides when to `connect` again.
    ///
    /// A status that turns an `auto_off` readout on is handed out like any other; the `false`
    /// that answers it is written first, before this waits on the stream again or hands out
    /// the next message.
    ///
    /// A status that brings a device with a power state back on, or a corrected passphrase that
    /// lifted a refusal, owes the device a refresh, and this writes it in the same place, after
    /// the replies. The statuses it brings come out of here like any other, and a [`Session::set`]
    /// made meanwhile waits for them first.
    ///
    /// Cancel-safe: no notification is lost when a caller drops this future in a `select!`. The
    /// notification stream's own `next` holds nothing once dropped. A reply leaves the queue as
    /// soon as the link has taken it, so a drop during the write spacing before it neither
    /// loses nor repeats it; a drop inside the link's own write can repeat that one write,
    /// which sets the readout false again, a write spacing after the first. A refresh's
    /// burst is armed before the write, and the refresh stays owed until the link has returned,
    /// so a drop inside the link's own write cannot make the session forget a refresh that may
    /// have gone out; it can repeat it, and the burst then owed is two. A firmware ask that falls
    /// due while this waits on the stream is written in the same place, and a drop costs it what
    /// it costs a reply: one dropped during the write spacing before it sends nothing and loses
    /// nothing, and one dropped inside the link's own write can repeat that one write.
    ///
    /// A status on an id some control reads as its own (`Control::reads`) is handed out, and
    /// kept in state, under that control's id. A recall whose readings the device has not
    /// reported within [`SessionConfig::recall_guard`] owes the device one refresh, written in
    /// the same place as a firmware ask that falls due. A drop leaves the guard as it was: it is
    /// cleared only by a report, a refresh written, or a link torn down, and a guard that fell
    /// due during the dropped wait is written by the next call.
    pub async fn recv(&mut self) -> Option<Message> {
        loop {
            if self.flush_replies().await.is_err() || self.flush_refresh().await.is_err() {
                return None;
            }
            if let Some(m) = self.pending.pop_front() {
                if self.pending.is_empty() {
                    self.overflowing = false;
                }
                return Some(m);
            }
            let wake = self.next_wake();
            let Some(notes) = self.notifications.as_mut() else {
                let _ = self.drop_link();
                return None;
            };
            // A firmware ask or a recall guard that falls due ends the wait, and the next turn of
            // the loop writes what it owes.
            let heard = match wake {
                Some(at) => match tokio::time::timeout_at(at, notes.next()).await {
                    Ok(heard) => heard,
                    Err(_) => continue,
                },
                None => notes.next().await,
            };
            match heard {
                Some(bytes) => {
                    let Some(n) = self.fold(&bytes) else {
                        continue;
                    };
                    let mut messages = n.messages.into_iter();
                    if let Some(m) = messages.next() {
                        self.queue_pending(messages);
                        return Some(m);
                    }
                    // A notification with only dropped messages: keep reading.
                }
                None => {
                    let _ = self.drop_link();
                    return None;
                }
            }
        }
    }

    /// Connect (retrying with backoff), subscribe, take a first full refresh, and ask the
    /// device for its firmware versions.
    ///
    /// An already-open link is torn down first (best-effort disconnect, then cleared with
    /// state marked stale) before the connect loop runs, so a reconnect that then fails within
    /// `connect_deadline` never leaves a torn-down link looking live: it returns `Err` with
    /// `link`/`notifications` both `None` and `state().is_stale()` true. Connecting — including
    /// a single hung `Transport::connect` call — is bounded by `connect_deadline`; each backoff
    /// sleep is clamped so it cannot run past it. Giving up then waits, in `gave_up`, for the
    /// transport to finish tearing down what the abandoned attempt left it holding, so failure
    /// takes the deadline plus up to `close_grace`.
    ///
    /// A refusal an earlier link earned is forgotten as the new one comes up: the refresh that
    /// ends this call is what says whether the device refuses the code now. The firmware
    /// versions are asked for after that refresh, one subsystem at a time as the official app
    /// does: the first `210.2.<subsystem>` the profile lists goes out 300 ms after the refresh,
    /// each next one 300 ms after the previous one's report, and none when the profile lists
    /// none. The asks are written by [`Session::recv`] (and by `refresh`, if one falls due in it) and
    /// the answers arrive as ordinary statuses out of `recv`. A report that never comes leaves the
    /// later subsystems unasked on that link. A device that has refused the code is asked
    /// nothing; the ask follows the refresh that a corrected code owes, once that code is
    /// proven.
    pub async fn connect(&mut self) -> Result<(), BleError> {
        if self.advert.manufacturer.in_bootloader() {
            return Err(BleError::Bootloader);
        }
        let had_link = self.link.is_some();
        self.accepted = false;
        self.firmware_asked = None;
        self.firmware_next = None;
        self.firmware_needs_proof = false;
        self.guard = None;
        // A refusal belongs to the link it came on, and the new link's refresh is what says
        // whether the device refuses the code now.
        self.state.clear_passphrase_rejected();
        if let Some(link) = self.link.as_ref() {
            // Best-effort teardown of the link this call is about to replace.
            let _ = link.disconnect().await;
        }
        if had_link {
            let _ = self.drop_link();
        }
        let unit = self.advert.unit_id();
        let deadline = tokio::time::Instant::now() + self.config.connect_deadline;
        let mut backoff = self.config.backoff_min;
        let mut last_err: Option<BleError> = None;

        let link = loop {
            // Bound before it is matched on, so the attempt's own borrow of the transport
            // is over by the time an arm asks the session for anything.
            let attempt = tokio::time::timeout_at(deadline, self.transport.connect(unit)).await;
            match attempt {
                Ok(Ok(link)) => break link,
                Ok(Err(e)) => {
                    last_err = Some(e);
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    if remaining.is_zero() {
                        return Err(self.gave_up(last_err).await);
                    }
                    tokio::time::sleep(backoff.min(remaining)).await;
                    backoff = backoff.saturating_mul(2).min(self.config.backoff_max);
                }
                Err(_) => return Err(self.gave_up(last_err).await),
            }
        };

        self.notifications = Some(link.notifications());
        self.link = Some(link);
        self.refresh().await?;
        self.ask_firmware().await
    }

    /// Send function 1 and fold every status the device sends in reply.
    ///
    /// If the notification stream ends before an idle gap (the device disconnected mid
    /// burst), the link is dropped, the state is marked stale, and this returns an error
    /// instead of reporting a fresh refresh for a device that is gone.
    ///
    /// A refresh the session already has under way for itself is heard out first, so the two
    /// bursts are never read as one. This refresh answers any refresh owed; one the burst itself
    /// owes, a device that reports coming back on inside it, is written once the burst is
    /// read, and its own burst comes out of [`Session::recv`].
    pub async fn refresh(&mut self) -> Result<(), BleError> {
        self.settle_burst().await?;
        let frame = encode_command(&refresh_message(), self.passphrase)?;
        self.refresh_due = false;
        self.guard = None;
        self.write(&frame).await?;
        let stream_ended = self.drain().await;
        if stream_ended {
            return Err(self.drop_link());
        }
        // A failed reply or refresh write has torn the link down; the state stays stale and the
        // next read reports the loss.
        if self.flush_replies().await.is_ok() && self.flush_refresh().await.is_ok() {
            self.state.set_stale(false);
        }
        Ok(())
    }

    /// Send a command and resolve when its echo status arrives or the timeout elapses.
    ///
    /// Refuses up front if the device already rejected the passphrase. A `Restricted`
    /// function id fails inside `encode_command`, before any frame reaches the link. A
    /// `250.255` status arriving while waiting marks the device passphrase-rejected and
    /// returns `BleError::PassphraseRejected`. If the notification stream ends instead of
    /// yielding a matching status, the link is dropped, state is marked stale, and this
    /// returns an error rather than waiting on a link that will never answer.
    ///
    /// Only the echo comes back from here; every other message read while waiting is queued
    /// for [`Session::recv`], in arrival order. A status of the echoed function that was queued
    /// before the echo is older than it and is dropped, so `recv` never hands out a value the
    /// echo has replaced.
    ///
    /// A refresh the session owes the device is written first, and the burst it brings, or any
    /// burst of its own still arriving, is read before the command goes out, so that no value in
    /// it can be taken for the echo. That wait ends with the burst, on a quiet gap, or at the
    /// burst's bound, and the timeout of the command itself starts after it.
    ///
    /// The echo of a command on an id some control reads as its own is the report of that
    /// control, whichever of its ids the device reports it on. A command on an action of the
    /// profile (a recall or a save) has no echo: it is written as [`Session::set_no_wait`] writes
    /// it, and this returns the message sent without waiting. While the echo of any other command
    /// is awaited, a recall guard that falls due writes its refresh then, not when this returns,
    /// and no report of the awaited id in that refresh's burst is taken for the echo.
    ///
    /// A command of the value the device already held may get no echo, as a unit that reports a
    /// change only says nothing: when the wait ends with the timeout and the device's last report
    /// of that function, on a link not known to be stale, is the value sent, the device is where
    /// the write asked it to be and the answer is that report, not the timeout.
    ///
    /// Cancel-safe in the way of [`Session::recv`]: a call dropped before its write has returned
    /// may still have sent the frame, and an action's guard is armed before that write, so it
    /// stays armed and a repeated command restarts it.
    pub async fn set(&mut self, message: Message) -> Result<Message, BleError> {
        let confirmed = self.set_and_wait(message).await;
        // A reply queued while the echo was awaited goes out now rather than at the next read,
        // and so does a refresh the echo's wait found owed. The outcome of `set` is the echo, or
        // why there is none: a write that fails has already torn the link down, and the next
        // read reports that.
        if self.link.is_some() {
            let _ = self.flush_replies().await;
            let _ = self.write_owed_refresh().await;
        }
        confirmed
    }

    /// The end of the wait for an echo that never came, once the frame is out: the held report
    /// when the device already held the value `message` writes, as heard on this link, and the
    /// timeout otherwise.
    fn unechoed(&self, message: &Message) -> Result<Message, BleError> {
        let id = self.read_as(message.id);
        match self.state.get(id) {
            Some(held)
                if !self.state.is_stale()
                    && self.state.heard(id)
                    && held.value == message.value =>
            {
                Ok(held.clone())
            }
            _ => Err(BleError::Timeout(self.config.set_timeout)),
        }
    }

    /// The write and the wait of [`Session::set`].
    async fn set_and_wait(&mut self, message: Message) -> Result<Message, BleError> {
        if self.state.passphrase_rejected() {
            return Err(BleError::PassphraseRejected);
        }
        if self.profile.action_of(message.id).is_some() {
            self.set_no_wait(message.clone()).await?;
            return Ok(message);
        }
        // A refresh the guard writes while the echo is awaited brings a burst of its own, which
        // may report the target too: until that burst is over no report of the target is the echo.
        let mut refreshed = false;
        let target = self.read_as(message.id);
        let frame = encode_command(&message, self.passphrase)?;
        self.write_owed_refresh().await?;
        self.settle_burst().await?;
        // The deadline starts here, so the wait for the write spacing counts against it, and
        // the wait for a burst before it does not.
        let deadline = Instant::now() + self.config.set_timeout;
        if tokio::time::timeout_at(deadline, self.wait_spacing())
            .await
            .is_err()
        {
            return Err(BleError::Timeout(self.config.set_timeout));
        }
        self.write(&frame).await?;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return self.unechoed(&message);
            }
            self.flush_replies().await?;
            // A recall guard falling due ends the wait early, to write its refresh.
            let wake = self
                .guard
                .as_ref()
                .map_or(deadline, |guard| guard.due.min(deadline));
            // The write above already failed if there was no link, and the arms below that tear
            // it down return rather than loop back: `notifications` is always set here.
            let notes = self
                .notifications
                .as_mut()
                .expect("a link that just wrote successfully has a notification stream");
            let burst_open = refreshed
                && self
                    .burst
                    .as_ref()
                    .is_some_and(|burst| burst.bound > Instant::now());
            match tokio::time::timeout_at(wake, notes.next()).await {
                Ok(Some(bytes)) => {
                    let Some(n) = self.fold(&bytes) else {
                        continue;
                    };
                    if self.state.passphrase_rejected() {
                        return Err(BleError::PassphraseRejected);
                    }
                    let mut messages = n.messages;
                    let echo = messages.iter().position(|m| m.id == target && !burst_open);
                    match echo {
                        Some(at) => {
                            let confirmed = messages.remove(at);
                            self.accepted = true;
                            // Whatever of this function is queued was read before the echo.
                            self.pending.retain(|m| m.id != target);
                            if self.pending.is_empty() {
                                self.overflowing = false;
                            }
                            self.queue_pending(messages);
                            return Ok(confirmed);
                        }
                        None => self.queue_pending(messages),
                    }
                }
                Ok(None) => return Err(self.drop_link()),
                Err(_) if Instant::now() >= deadline => {
                    return self.unechoed(&message);
                }
                Err(_) => {
                    self.write_owed_refresh().await?;
                    refreshed = true;
                }
            }
        }
    }

    /// Send a command without waiting for confirmation.
    ///
    /// Refuses up front if the device has already rejected the passphrase, same as `set`. A
    /// command on an action of the profile that changes readings (a recall) arms the recall
    /// guard, here and in `set`, before the write, as a refresh's burst is armed before its own:
    /// a write that fails tears the link down and the guard with it. Such a command first writes
    /// the refresh the session owes and reads the burst that brings, as `set` does for any
    /// command, so that no report of it is taken for the device's answer to the recall.
    pub async fn set_no_wait(&mut self, message: Message) -> Result<(), BleError> {
        if self.state.passphrase_rejected() {
            return Err(BleError::PassphraseRejected);
        }
        let frame = encode_command(&message, self.passphrase)?;
        if self
            .profile
            .action_of(message.id)
            .is_some_and(|(action, _)| !action.changes.is_empty())
        {
            // The readings the recall changes are what the guard waits for, so a refresh the
            // session owes, and the burst it brings, come first: a report in that burst is no
            // report of the recall.
            self.write_owed_refresh().await?;
            self.settle_burst().await?;
        }
        self.arm_guard(&message);
        self.write(&frame).await?;
        Ok(())
    }

    /// Ask the device to show itself (function 200, no value), without waiting for anything and
    /// without a refresh. Says whether a frame went out: `Ok(false)` inside [`IDENTIFY_GAP`] of
    /// the last one on this link, which sends nothing.
    ///
    /// Unlike `set_no_wait` this goes out on a link whose passphrase the device has refused, as
    /// the official app's identify does. Fails when there is no link.
    pub async fn identify(&mut self) -> Result<bool, BleError> {
        if self.link.is_none() {
            return Err(BleError::Connection("not connected".into()));
        }
        if self
            .last_identify
            .is_some_and(|at| at.elapsed() < IDENTIFY_GAP)
        {
            return Ok(false);
        }
        let message = Message::command(svolder_proto::function::IDENTIFY, Value::Data(vec![]));
        let frame = encode_command(&message, self.passphrase)?;
        self.write(&frame).await?;
        self.last_identify = Some(Instant::now());
        Ok(true)
    }

    /// The next status the device sends, reconnecting and re-refreshing if the link drops.
    ///
    /// [`Session::recv`] does the reading, so every message of every frame comes out of here
    /// too. If the stream ends, this reconnects with backoff — `connect` tears down the dead
    /// link, marks state stale, and refreshes on success, so state is never left stale while
    /// the link is up. Returns `Ok(None)` only when the reconnect itself fails within
    /// `connect_deadline`, so a caller looping on this can end cleanly instead of spinning
    /// forever.
    pub async fn next_status(&mut self) -> Result<Option<Message>, BleError> {
        loop {
            if self.notifications.is_none() {
                // No link yet: try to establish one. Nothing is queued behind a missing
                // stream, since tearing the link down clears the queue with it.
                self.reconnect().await?;
            }
            match self.recv().await {
                Some(m) => return Ok(Some(m)),
                None => {
                    // Link dropped: try to come back. `reconnect` tears down the dead
                    // link and marks state stale before it retries.
                    if self.reconnect().await.is_err() {
                        return Ok(None);
                    }
                }
            }
        }
    }

    /// Close the link and mark state stale.
    ///
    /// Best effort: the disconnect result is ignored because the point is to release the
    /// radio, and a link that refuses to close cleanly is dropped either way. After this,
    /// the session holds no link, `state().is_stale()` is true, and any write fails until
    /// `connect` opens a new one.
    ///
    /// Ends by giving the transport up to `close_grace` to finish what it still has in
    /// flight, so a caller that stops driving its runtime straight after this does not cut
    /// a connection the transport is taking back down.
    pub async fn close(&mut self) {
        if self.link.is_some() {
            // The last write is not cut off by the disconnect that follows it.
            self.wait_spacing().await;
        }
        if let Some(link) = self.link.take() {
            let _ = link.disconnect().await;
        }
        let _ = self.drop_link();
        self.transport
            .settle_for(self.advert.unit_id(), self.config.close_grace)
            .await;
    }

    /// Release the link, listen for the device's next advertisement for at most `window`, and
    /// key the session with the code that advertisement carries, for a caller that is about to
    /// `connect` again after the device refused the code it held.
    ///
    /// The listen is [`listen_afresh`], so an advertisement the adapter held from before the
    /// release is not taken for the device's word. Says whether a fresh advertisement was
    /// heard; when none was, the session keeps the code and the advertisement it had. A failed
    /// listen is the same as hearing nothing, since the connect that follows settles whether
    /// the device is there.
    pub async fn reread_code(&mut self, window: Duration) -> bool {
        let refused = self.passphrase;
        let unit = self.advert.unit_id();
        self.close().await;
        match listen_afresh(self.transport.as_ref(), unit, refused, window).await {
            Ok(Some(advert)) => {
                self.set_passphrase(advert.manufacturer.passphrase());
                self.advert = advert;
                true
            }
            _ => false,
        }
    }

    /// Reconnect with backoff and refresh, reusing `connect`.
    async fn reconnect(&mut self) -> Result<(), BleError> {
        self.connect().await
    }

    /// The error `connect` gives up with, once the transport has had up to `close_grace` to
    /// finish taking down the connection the abandoned attempt left it holding. A caller
    /// that quits on this error stops driving the runtime the transport's release runs on,
    /// so the wait happens here rather than after the error has been reported.
    async fn gave_up(&mut self, last: Option<BleError>) -> BleError {
        self.transport
            .settle_for(self.advert.unit_id(), self.config.close_grace)
            .await;
        no_connection(self.config.connect_deadline, last)
    }

    /// Sleep until `write_spacing` after the last write that went out. The time of that write
    /// is recorded when the link is handed the frame, so a call dropped during this sleep, or
    /// inside the link's own write, leaves the spacing for the next write as it was.
    ///
    /// Takes the session by `&mut` so the future holds no shared borrow of it across the sleep.
    fn wait_spacing(&mut self) -> tokio::time::Sleep {
        let until = self
            .last_write
            .map_or_else(Instant::now, |last| last + self.config.write_spacing);
        tokio::time::sleep_until(until)
    }

    /// Write one frame, once `write_spacing` has passed since the last one. Only one write is
    /// ever in flight because every caller reaches this through a `&mut self` method on
    /// `Session`. A write that fails tears the link down — the link and the notification
    /// stream are cleared and the state is marked stale — and reports the transport's own
    /// error, so a caller never keeps talking to a link the radio has already given up on.
    ///
    /// A write counts from the moment the link has the frame, and again from the moment the
    /// link returns: a call dropped inside the link's own write may still have sent the frame.
    async fn write(&mut self, frame: &[u8]) -> Result<(), BleError> {
        self.wait_spacing().await;
        let written = match self.link.as_ref() {
            Some(link) => {
                self.last_write = Some(Instant::now());
                link.write(frame).await
            }
            None => return Err(BleError::Connection("not connected".into())),
        };
        if let Err(e) = written {
            let _ = self.drop_link();
            return Err(e);
        }
        self.last_write = Some(Instant::now());
        Ok(())
    }

    /// Queue messages for a caller, dropping the oldest past `PENDING_CAP`. The first drop of
    /// a run says so on stderr; the run lasts until the queue empties.
    fn queue_pending(&mut self, messages: impl IntoIterator<Item = Message>) {
        self.pending.extend(messages);
        let excess = self.pending.len().saturating_sub(PENDING_CAP);
        if excess == 0 {
            return;
        }
        self.pending.drain(..excess);
        if !self.overflowing {
            self.overflowing = true;
            #[cfg(test)]
            {
                self.overflow_runs += 1;
            }
            crate::log::line(
                crate::log::LogLevel::Warn,
                &format!("{PENDING_CAP} unread messages are queued; dropping the oldest"),
            );
        }
    }

    /// Read statuses until every function the profile names has reported, or, failing that,
    /// until a `drain_idle` gap with nothing new, or, failing that, until `drain_bound` has
    /// passed with the device still talking — any of the three means the burst is over.
    /// Returns `true` instead when the notification stream ended: a burst over means the
    /// device has said its piece, a stream ended means the link is gone.
    ///
    /// The full set is the ordinary ending, and the only one a device that talks on its own
    /// ever reaches: meters following the music report several times a second with no quiet
    /// gap at all, so waiting for one would hold a snapshot back for as long as the music
    /// played. A device that does not report everything ends on the gap instead, and one that
    /// never stops talking on the bound; whichever ending arrives, everything read on the way
    /// is folded into state, so a burst cut short is a partial snapshot, not a lost one.
    async fn drain(&mut self) -> bool {
        let bound = tokio::time::Instant::now() + self.config.drain_bound;
        let mut awaited = self.awaited();
        // An empty set has nothing left to wait for from the very first notification, so
        // checking it unguarded would end the burst there rather than on anything the device
        // did; guarding it here means a profile that lists nothing instead waits for the gap.
        let complete_is_an_ending = !awaited.is_empty();
        loop {
            let left = bound.saturating_duration_since(tokio::time::Instant::now());
            if left.is_zero() {
                return false;
            }
            if self.flush_replies().await.is_err() {
                return true;
            }
            let Some(notes) = self.notifications.as_mut() else {
                return false;
            };
            // `notes.next()` is cancel-safe, so dropping it on a timeout loses nothing.
            match tokio::time::timeout(self.config.drain_idle.min(left), notes.next()).await {
                Ok(Some(bytes)) => {
                    let Some(notification) = self.fold(&bytes) else {
                        continue;
                    };
                    for message in &notification.messages {
                        awaited.remove(&message.id);
                    }
                    if complete_is_an_ending && awaited.is_empty() {
                        // Everything the profile names answered the refresh.
                        self.accepted = !self.state.passphrase_rejected();
                        return false;
                    }
                }
                Ok(None) => return true, // stream ended
                Err(_) => return false,  // the quiet gap, or the bound: burst over
            }
        }
    }

    /// Every function a refresh is waiting to hear: the profile's controls, and its tone
    /// control's mode, loudness and the three parameters of each of its bands.
    fn awaited(&self) -> BTreeSet<FunctionId> {
        let mut ids: BTreeSet<FunctionId> = self.profile.controls.iter().map(|c| c.id).collect();
        if let Some(eq) = &self.profile.eq {
            ids.insert(eq.mode_id());
            ids.insert(eq.loudness_id());
            for band in 0..eq.bands.len() {
                for param in [EqParam::Frequency, EqParam::Gain, EqParam::Q] {
                    if let Some(id) = eq.band_id(band, param) {
                        ids.insert(id);
                    }
                }
            }
        }
        ids
    }

    /// Decode one notification frame and fold it into `state`, handing back the decoded
    /// notification so a caller can also inspect its messages. Returns `None`, after
    /// counting the failure, when the frame could not be decoded at all.
    fn fold(&mut self, bytes: &[u8]) -> Option<Notification> {
        match decode_notification(bytes) {
            Ok(mut n) => {
                if self.reads_aliases {
                    for m in &mut n.messages {
                        m.id = self.profile.read_as(m.id);
                    }
                }
                self.cancel_guard(&n);
                self.state.apply_notification(&n);
                self.queue_auto_off_replies(&n);
                self.follow_power(&n);
                self.count_burst(&n);
                self.follow_firmware(&n);
                Some(n)
            }
            Err(_) => {
                self.state.note_decode_error();
                None
            }
        }
    }

    /// Queue the write that turns each `auto_off` readout this notification reports on back
    /// off. The app does the same with the amplifiers' temperature reporting: a device that
    /// sends it on expects to be told to stop. One report earns one reply, so a device that
    /// reports it on again is answered again and one that reports it off is left alone.
    fn queue_auto_off_replies(&mut self, n: &Notification) {
        if self.state.passphrase_rejected() {
            return;
        }
        for m in &n.messages {
            let Some(control) = self.profile.control_by_id(m.id) else {
                continue;
            };
            if !control.auto_off || !control.reads_true(&m.value) {
                continue;
            }
            self.replies
                .push_back(Message::command(control.id, Value::Bool(false)));
        }
    }

    /// Write the queued `auto_off` replies, each encoded with the passphrase held now, with the
    /// raw write, so they wait on no echo and never recurse into `set`. A reply leaves the queue
    /// as soon as the link has taken it. Each write waits out the write spacing first; one that
    /// fails tears the link down and reports the transport's error.
    async fn flush_replies(&mut self) -> Result<(), BleError> {
        while let Some(reply) = self.replies.front() {
            let Ok(frame) = encode_command(reply, self.passphrase) else {
                self.replies.pop_front();
                continue;
            };
            self.write(&frame).await?;
            self.replies.pop_front();
        }
        Ok(())
    }

    /// Follow the device's power state through one notification, and owe it a refresh for each
    /// change back to on. Nothing is owed to a device that has refused the passphrase, and a
    /// refusal in this notification cancels what an earlier one owed.
    fn follow_power(&mut self, n: &Notification) {
        if self.state.passphrase_rejected() {
            self.refresh_due = false;
            self.burst = None;
        }
        let Some(rule) = PowerRule::of(&self.profile) else {
            return;
        };
        for m in &n.messages {
            if self.power.observe(rule, m) && !self.state.passphrase_rejected() {
                self.refresh_due = true;
            }
        }
    }

    /// Count the functions of one notification against the burst the session is waiting for,
    /// ending it when every function has reported.
    fn count_burst(&mut self, n: &Notification) {
        let Some(burst) = self.burst.as_mut() else {
            return;
        };
        for m in &n.messages {
            if let Some(left) = burst.owed.get_mut(&m.id) {
                *left -= 1;
                if *left == 0 {
                    burst.owed.remove(&m.id);
                }
            }
        }
        if burst.ends_when_complete && burst.owed.is_empty() {
            // A burst asked for under a code the session has since changed proves nothing
            // about the new one.
            let proves = burst.code == self.passphrase;
            self.burst = None;
            self.accepted = proves && !self.state.passphrase_rejected();
        }
    }

    /// Write the refresh the session owes, if any, and start counting the burst it brings.
    ///
    /// Nothing is written while the device has refused the passphrase or there is no link, and
    /// what was owed is forgotten. The burst is armed before the write, and the refresh stays
    /// owed until the link has returned: a call dropped inside the link's own write may have
    /// sent the frame, and loses neither the burst nor the refresh. Waits out the write spacing
    /// first; a write that fails tears the link down and reports the transport's error.
    ///
    /// Ends with [`Session::ask_firmware`], which writes the next firmware ask when its time has
    /// come.
    async fn flush_refresh(&mut self) -> Result<(), BleError> {
        self.write_owed_refresh().await?;
        self.ask_firmware().await
    }

    /// The refresh half of [`Session::flush_refresh`], for a caller that must not hold its own
    /// work up for a firmware ask: a [`Session::set`] leaves the ask to the next read.
    async fn write_owed_refresh(&mut self) -> Result<(), BleError> {
        self.fire_guard();
        if !self.refresh_due {
            return Ok(());
        }
        if self.state.passphrase_rejected() || self.link.is_none() {
            self.refresh_due = false;
            return Ok(());
        }
        let Ok(frame) = encode_command(&refresh_message(), self.passphrase) else {
            self.refresh_due = false;
            return Ok(());
        };
        self.wait_spacing().await;
        self.expect_burst();
        self.guard = None;
        self.write(&frame).await?;
        self.refresh_due = false;
        Ok(())
    }

    /// Ask the device for the firmware version of the next subsystem the profile lists, one
    /// `210.2.<subsystem>` command, once its time has come. The first subsystem is asked
    /// [`SessionConfig::firmware_delay`] after the point this first runs on a link, each next
    /// one that long after the report of the one before (`follow_firmware` sets the time), so a
    /// report that never arrives ends the chain there: nothing retries it and nothing times it
    /// out. Nothing waits here for that time: before it this returns at once, and
    /// [`Session::recv`] wakes for it. The chain runs once per link; nothing is sent on a link
    /// without a device to ask or one that has refused the passphrase. After a corrected code
    /// lifted a refusal the chain does not begin until that code's burst has proven it
    /// ([`Session::code_accepted`]), so a correction the device refuses again asks nothing and
    /// spends nothing. A corrected code that is never proven, because the profile names no
    /// function for a refresh burst to prove it with, defers the ask until a [`Session::set`]'s
    /// echo proves it and a read follows. A write that fails tears the link down and reports the
    /// transport's error.
    ///
    /// Cancel-safe: a subsystem counts as asked once its write has returned, so a call dropped
    /// during the write spacing before it sends nothing and the next call writes it; one dropped
    /// inside the link's own write can repeat that one write.
    async fn ask_firmware(&mut self) -> Result<(), BleError> {
        if !self.firmware_askable() {
            return Ok(());
        }
        if self.firmware_asked.is_none() {
            self.firmware_asked = Some(0);
            if !self.profile.firmware_subsystems.is_empty() {
                self.firmware_next = Some(Instant::now() + self.config.firmware_delay);
            }
        }
        let Some(at) = self.firmware_next else {
            return Ok(());
        };
        if at > Instant::now() {
            return Ok(());
        }
        let asked = self.firmware_asked.unwrap_or(0);
        let Some(&sub) = self.profile.firmware_subsystems.get(asked) else {
            self.firmware_next = None;
            return Ok(());
        };
        let ask = Message::command(
            FunctionId::three(FIRMWARE_FAMILY, FIRMWARE_VERSION_QUERY, sub),
            Value::Data(vec![]),
        );
        let frame = encode_command(&ask, self.passphrase)?;
        self.write(&frame).await?;
        self.firmware_asked = Some(asked + 1);
        self.firmware_next = None;
        Ok(())
    }

    /// Whether the firmware ask may go on: there is a link, the device has not refused the code,
    /// and a code a correction lifted a refusal for has been proven.
    fn firmware_askable(&self) -> bool {
        self.link.is_some()
            && !self.state.passphrase_rejected()
            && (!self.firmware_needs_proof || self.code_accepted())
    }

    /// When [`Session::recv`] must wake to write the next firmware ask, if one is waiting for
    /// its time and may be sent.
    fn firmware_wake(&self) -> Option<Instant> {
        self.firmware_next.filter(|_| self.firmware_askable())
    }

    /// When [`Session::recv`] must wake for a timed write: the firmware ask or the recall guard,
    /// whichever falls due first.
    fn next_wake(&self) -> Option<Instant> {
        let guard = self.guard.as_ref().map(|guard| guard.due);
        match (self.firmware_wake(), guard) {
            (Some(ask), Some(guard)) => Some(ask.min(guard)),
            (ask, guard) => ask.or(guard),
        }
    }

    /// The control id a status on `id` is read as: see [`Profile::read_as`], which a profile
    /// whose controls read no extra ids is not asked.
    fn read_as(&self, id: FunctionId) -> FunctionId {
        if self.reads_aliases {
            self.profile.read_as(id)
        } else {
            id
        }
    }

    /// Arm the recall guard when `message` is an action of the profile that changes readings:
    /// the device is then given [`SessionConfig::recall_guard`] to report them, counted from
    /// the write that has just gone out. A second such write starts the wait again.
    fn arm_guard(&mut self, message: &Message) {
        let Some((action, _)) = self.profile.action_of(message.id) else {
            return;
        };
        if action.changes.is_empty() {
            return;
        }
        let watched = action
            .changes
            .iter()
            .filter_map(|key| self.profile.control_by_key(key))
            .map(|control| control.id)
            .collect();
        self.guard = Some(Guard {
            due: Instant::now() + self.config.recall_guard,
            watched,
        });
    }

    /// Cancel the recall guard when `n` reports a reading it waits for.
    fn cancel_guard(&mut self, n: &Notification) {
        let Some(guard) = self.guard.as_ref() else {
            return;
        };
        if n.messages.iter().any(|m| guard.watched.contains(&m.id)) {
            self.guard = None;
        }
    }

    /// Owe the device a refresh, once, when the recall guard has fallen due uncancelled.
    fn fire_guard(&mut self) {
        if self
            .guard
            .as_ref()
            .is_some_and(|guard| guard.due <= Instant::now())
        {
            self.guard = None;
            self.refresh_due = true;
        }
    }

    /// Set the time of the next firmware ask when `n` carries the report of the subsystem asked
    /// last and one is left to ask.
    fn follow_firmware(&mut self, n: &Notification) {
        let Some(asked) = self.firmware_asked.filter(|&asked| asked > 0) else {
            return;
        };
        if self.firmware_next.is_some() || asked >= self.profile.firmware_subsystems.len() {
            return;
        }
        let last = FunctionId::three(
            FIRMWARE_FAMILY,
            FIRMWARE_VERSION_QUERY,
            self.profile.firmware_subsystems[asked - 1],
        );
        if n.messages.iter().any(|m| m.id == last) {
            self.firmware_next = Some(Instant::now() + self.config.firmware_delay);
        }
    }

    /// Start, or add to, the burst the session is waiting for: every function the profile
    /// names owes one more report, and the bound runs from now. A burst whose bound has already
    /// passed is over, and a fresh one starts in its place.
    fn expect_burst(&mut self) {
        let awaited = self.awaited();
        let now = Instant::now();
        let bound = now + self.config.drain_bound;
        if self.burst.as_ref().is_some_and(|b| b.bound <= now) {
            self.burst = None;
        }
        let burst = self.burst.get_or_insert_with(|| Burst {
            owed: BTreeMap::new(),
            ends_when_complete: !awaited.is_empty(),
            bound,
            code: self.passphrase,
        });
        burst.bound = bound;
        burst.code = self.passphrase;
        for id in awaited {
            *burst.owed.entry(id).or_insert(0) += 1;
        }
    }

    /// Read what the device is still sending of the burst the session is waiting for, queueing
    /// every message for [`Session::recv`], until the burst is complete, or a `drain_idle` gap
    /// passes with nothing, or its bound does; any of the three ends the wait. Returns at once
    /// when no burst is awaited, and with the error that took the link or the passphrase.
    async fn settle_burst(&mut self) -> Result<(), BleError> {
        loop {
            let Some(burst) = self.burst.as_ref() else {
                return Ok(());
            };
            let left = burst.bound.saturating_duration_since(Instant::now());
            let Some(notes) = self.notifications.as_mut() else {
                self.burst = None;
                return Ok(());
            };
            if left.is_zero() {
                self.burst = None;
                return Ok(());
            }
            match tokio::time::timeout(self.config.drain_idle.min(left), notes.next()).await {
                Ok(Some(bytes)) => {
                    let Some(n) = self.fold(&bytes) else {
                        continue;
                    };
                    if self.state.passphrase_rejected() {
                        return Err(BleError::PassphraseRejected);
                    }
                    self.queue_pending(n.messages);
                }
                Ok(None) => return Err(self.drop_link()),
                Err(_) => {
                    self.burst = None;
                    return Ok(());
                }
            }
        }
    }

    /// Tear the link down: clear the link, the notification stream and whatever was queued
    /// for a caller or for the device, mark state stale, and return the error callers report
    /// for a link that is no longer usable. The queue goes because the next `connect`
    /// refreshes: its burst is the snapshot, and a message from the link that just died would
    /// arrive behind it.
    fn drop_link(&mut self) -> BleError {
        self.link = None;
        self.notifications = None;
        self.pending.clear();
        self.overflowing = false;
        self.replies.clear();
        self.refresh_due = false;
        self.burst = None;
        self.last_identify = None;
        self.firmware_asked = None;
        self.firmware_next = None;
        self.firmware_needs_proof = false;
        self.guard = None;
        self.state.set_stale(true);
        BleError::Connection("link closed".into())
    }
}

/// Whether any control of `profile` reads ids besides its own.
fn has_reads(profile: &Profile) -> bool {
    profile.controls.iter().any(|c| !c.reads.is_empty())
}

/// The command that asks a device to report everything: function 1, with no value.
fn refresh_message() -> Message {
    Message::command(FunctionId::one(1), Value::Data(vec![]))
}

/// The error `connect` reports once its deadline passes, naming the transport's last
/// failure when there was one to name.
fn no_connection(deadline: Duration, last: Option<BleError>) -> BleError {
    match last {
        Some(e) => BleError::Connection(format!("no connection within {deadline:?}: {e}")),
        None => BleError::Connection(format!("no connection within {deadline:?}")),
    }
}

#[cfg(test)]
mod tests;
