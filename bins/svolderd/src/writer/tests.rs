// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use svolder_ipc::ValueObject;
use svolder_profiles::Registry;
use svolder_proto::Value;
use tokio::task::JoinHandle;

use super::*;
use crate::test_support::shown;
use svolder_ipc::{CODE_LINK, CODE_TIMEOUT, CODE_USAGE};

/// A stand-in device behind one actor channel, on a built-in profile: it records every command
/// in the words [`shown`] gives, applies a set, a raw write, a band and a mode to its readings
/// and publishes them through the feed, as the real actor does before it answers, and takes
/// each command only once `gate` lets it. One made by [`stand_in_acking`] answers a `set` sent
/// without waiting `Ack` before its echo, as the real actor does.
struct Stand {
    log: Arc<std::sync::Mutex<Vec<String>>>,
    gate: Arc<tokio::sync::Semaphore>,
    feed: watch::Sender<Feed>,
    /// A raw volume step the stand-in applies, and answers, in place of the next volume set it
    /// takes: a knob turned while the write waited for its echo, which the session takes for it.
    knob: Arc<std::sync::Mutex<Option<i64>>>,
    /// A raw volume step the stand-in answers the next volume set with, as a knob turn the
    /// session takes for the echo, and then publishes the set's own step: the device handling the
    /// write after the knob's status.
    late: Arc<std::sync::Mutex<Option<i64>>>,
    /// The statuses of the writes answered `Ack` whose echo has not shown yet, in order: see
    /// [`show`].
    unshown: Arc<std::sync::Mutex<Vec<Message>>>,
    /// A raw volume step the stand-in publishes, as a knob turn, just as it takes the next
    /// command of any kind and before it answers it: the knob's status and the answer are ready
    /// at the same time.
    before: Arc<std::sync::Mutex<Option<i64>>>,
    /// Whether the stand-in answers the next waited set with the timeout the actor answers when
    /// the echo never comes: the device applies the write and says nothing, so the reading stays.
    times_out: Arc<std::sync::atomic::AtomicBool>,
    /// The ids of raw writes the stand-in refuses after logging them, as a device that says no.
    refuses: Arc<std::sync::Mutex<Vec<String>>>,
}

/// A writer for a `profile` device behind a [`Stand`] that lets every command through at once,
/// reading `readings` (key and raw step) to start with.
fn stand_in(profile: &str, readings: &[(&str, i64)]) -> (Writer, Stand) {
    stand(profile, readings, false)
}

/// A writer for a `profile` device behind a [`Stand`] that answers a `set` sent without waiting
/// `Ack` and queues its status instead of publishing it, until [`show`] or [`turn`] publishes it;
/// a waited write publishes the queued statuses, then its own.
fn stand_in_acking(profile: &str, readings: &[(&str, i64)]) -> (Writer, Stand) {
    stand(profile, readings, true)
}

/// The stand-in `stand_in` and `stand_in_acking` build: `acking` picks the answer to a no-wait set.
fn stand(profile: &str, readings: &[(&str, i64)], acking: bool) -> (Writer, Stand) {
    let profile = Arc::new(
        Registry::builtin()
            .expect("the built-in profiles load")
            .get(profile)
            .expect("a built-in")
            .clone(),
    );
    let start: Vec<Message> = readings
        .iter()
        .map(|(key, raw)| {
            let m = svolder_profiles::intent(&profile, key, &raw.to_string()).expect("a reading");
            Message::status(m.id, m.value)
        })
        .collect();
    let (feed, feed_rx) = watch::channel(Feed {
        name: "fake".into(),
        unit: "0102030405060708".into(),
        profile: Some(Arc::clone(&profile)),
        connected: true,
        readings: start,
        ..Feed::default()
    });
    let (commands, mut inbox) = mpsc::channel::<(Command, oneshot::Sender<Response>)>(8);
    let stand = Stand {
        log: Arc::default(),
        gate: Arc::new(tokio::sync::Semaphore::new(1000)),
        feed,
        knob: Arc::default(),
        late: Arc::default(),
        unshown: Arc::default(),
        before: Arc::default(),
        times_out: Arc::default(),
        refuses: Arc::default(),
    };
    let (log, gate, publish, knob, late, unshown, before, times_out, refuses) = (
        Arc::clone(&stand.log),
        Arc::clone(&stand.gate),
        stand.feed.clone(),
        Arc::clone(&stand.knob),
        Arc::clone(&stand.late),
        Arc::clone(&stand.unshown),
        Arc::clone(&stand.before),
        Arc::clone(&stand.times_out),
        Arc::clone(&stand.refuses),
    );
    tokio::spawn(async move {
        while let Some((command, reply)) = inbox.recv().await {
            gate.acquire().await.expect("the gate is open").forget();
            log.lock().expect("the log").push(shown(&command));
            let turned = before.lock().expect("the early knob").take();
            if let Some(raw) = turned {
                let m =
                    svolder_profiles::intent(&profile, "volume", &raw.to_string()).expect("a step");
                publish_all(&publish, [Message::status(m.id, m.value)].into_iter());
            }
            let waited = matches!(&command, Command::Set { no_wait: false, .. });
            if waited && times_out.swap(false, std::sync::atomic::Ordering::SeqCst) {
                let _ = reply.send(crate::device::error_for(&svolder_ble::BleError::Timeout(
                    Duration::from_secs(2),
                )));
                continue;
            }
            if let Command::Raw { id, .. } = &command
                && refuses.lock().expect("the refusals").contains(id)
            {
                let _ = reply.send(Response::error(CODE_LINK, "the device said no"));
                continue;
            }
            let mut follows = None;
            let message = match &command {
                Command::Set { key, value, .. } => {
                    let turned = match key.as_str() {
                        "volume" => knob.lock().expect("the knob").take(),
                        _ => None,
                    };
                    let value = turned.map_or_else(|| value.clone(), |raw| raw.to_string());
                    let early = match key.as_str() {
                        "volume" => late.lock().expect("the late knob").take(),
                        _ => None,
                    };
                    follows =
                        early.and_then(|_| svolder_profiles::intent(&profile, key, &value).ok());
                    let value = early.map_or(value, |raw| raw.to_string());
                    svolder_profiles::intent(&profile, key, &value).ok()
                }
                Command::Raw { id, value, .. } => Some(Message::command(
                    id.parse().expect("an id"),
                    Value::U8(value.parse().expect("a raw step")),
                )),
                Command::EqSet {
                    band, param, value, ..
                } => svolder_profiles::eq_set(
                    &profile,
                    band - 1,
                    param.parse().expect("a parameter"),
                    *value,
                )
                .ok(),
                Command::EqMode { mode, .. } => {
                    svolder_profiles::eq_mode(&profile, mode.parse().expect("a mode")).ok()
                }
                _ => None,
            };
            let Some(message) = message else {
                let _ = reply.send(Response::error(CODE_USAGE, "refused"));
                continue;
            };
            // An action is a command with no echo: the device takes it and nothing is read back.
            if profile.action_of(message.id).is_some() {
                let _ = reply.send(Response::Ack);
                continue;
            }
            // A status is read as the control that reads it, as the session folds it.
            let status = Message::status(profile.read_as(message.id), message.value);
            let no_wait = matches!(&command, Command::Set { no_wait: true, .. });
            if acking && no_wait {
                unshown.lock().expect("the unshown").push(status);
                let _ = reply.send(Response::Ack);
                continue;
            }
            publish_all(&publish, unshown.lock().expect("the unshown").drain(..));
            publish.send_modify(|feed| {
                heard(feed, status.id);
                feed.readings.retain(|m| m.id != status.id);
                feed.readings.push(status.clone());
            });
            let reading = svolder_profiles::describe(&profile, &status);
            let _ = reply.send(Response::Value(ValueObject::from_reading(
                &reading, &status,
            )));
            if let Some(own) = follows {
                let own = Message::status(own.id, own.value);
                publish.send_modify(|feed| {
                    heard(feed, own.id);
                    feed.readings.retain(|m| m.id != own.id);
                    feed.readings.push(own.clone());
                });
            }
        }
    });
    (Writer::spawn(commands, feed_rx), stand)
}

/// Stamp function `id` as heard just now: every stamp is later than every one before it, as a
/// device's reports are, whatever the clock reads.
fn heard(feed: &mut Feed, id: FunctionId) {
    static CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let now = CLOCK.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    feed.heard_at.insert(id, now);
}

/// `statuses` shown by the stand-in's feed, in order.
fn publish_all(feed: &watch::Sender<Feed>, statuses: impl Iterator<Item = Message>) {
    for status in statuses {
        feed.send_modify(|feed| {
            heard(feed, status.id);
            feed.readings.retain(|m| m.id != status.id);
            feed.readings.push(status);
        });
    }
}

/// A waited `set` of `key` to `value`.
fn set(key: &str, value: &str) -> Command {
    Command::Set {
        key: key.into(),
        value: value.into(),
        step: 1,
        no_wait: false,
    }
}

/// A `set` of `key` to `value` sent without waiting.
fn no_wait(key: &str, value: &str) -> Command {
    Command::Set {
        key: key.into(),
        value: value.into(),
        step: 1,
        no_wait: true,
    }
}

/// The commands the stand-in has taken, in order.
fn log(stand: &Stand) -> Vec<String> {
    stand.log.lock().expect("the log").clone()
}

fn mode(name: &str) -> Command {
    Command::EqMode {
        mode: name.to_string(),
        no_wait: false,
    }
}

/// The volume's raw step the stand-in reads.
fn volume(stand: &Stand) -> Option<i64> {
    let feed = stand.feed.borrow();
    let id = feed.profile.as_ref()?.control_by_key("volume")?.id;
    reading_of(&feed.readings, id)
}

/// The volume steps the stand-in was asked for, in order.
fn volume_steps(stand: &Stand) -> Vec<i64> {
    log(stand)
        .iter()
        .filter_map(|line| line.strip_prefix("set volume ")?.parse().ok())
        .collect()
}

/// The echoes of the writes the stand-in answered `Ack`, shown in the order they were sent.
fn show(stand: &Stand) {
    let queued: Vec<_> = stand
        .unshown
        .lock()
        .expect("the unshown")
        .drain(..)
        .collect();
    publish_all(&stand.feed, queued.into_iter());
}

/// Turn the device's volume to `raw` by its knob: a reading the writer did not ask for. The
/// echoes the stand-in has queued show first.
fn turn(stand: &Stand, raw: i64) {
    show(stand);
    let profile = stand.feed.borrow().profile.clone().expect("a profile");
    let m = svolder_profiles::intent(&profile, "volume", &raw.to_string()).expect("a step");
    let status = Message::status(m.id, m.value);
    stand.feed.send_modify(|feed| {
        feed.readings.retain(|r| r.id != status.id);
        feed.readings.push(status.clone());
    });
}

/// A glide of the volume to `raw` from `origin`, answered.
async fn glide(writer: &Writer, origin: Origin, raw: i64) -> Glided {
    writer
        .glide(origin, "volume".into(), raw, Mode::Latest)
        .await
        .expect("an answer")
}

/// A no-wait write answered `Ack` is awaited.
async fn acked(answer: oneshot::Receiver<Outcome>) {
    let answer = answer.await;
    assert!(
        matches!(answer, Ok(Outcome::Done(Response::Ack))),
        "{answer:?}"
    );
}

/// The presets that fit the stand-in's tone control: its built-ins.
fn fitting_of(stand: &Stand) -> Vec<Preset> {
    let eq = stand
        .feed
        .borrow()
        .profile
        .as_ref()
        .and_then(|profile| profile.eq.clone())
        .expect("a tone control");
    preset::fitting(&eq, &[])
        .into_iter()
        .map(|(preset, _)| preset)
        .collect()
}

const BAND_KEYS: [&str; 6] = ["band1", "band2", "band3", "band4", "band5", "band6"];

/// A writer for a Loki Max behind a [`Stand`] whose six bands read `bands`, raw steps.
fn loki(bands: [i64; 6]) -> (Writer, Stand) {
    let readings: Vec<(&str, i64)> = BAND_KEYS.into_iter().zip(bands).collect();
    stand_in("lokimax", &readings)
}

/// A raw write of `value` to `id`, as a built-in setting sends it.
fn raw_u8(id: &str, value: &str) -> Command {
    Command::Raw {
        id: id.into(),
        value_type: "u8".into(),
        value: value.into(),
        no_wait: false,
    }
}

/// The profile the stand-in's feed holds.
fn stand_profile(stand: &Stand) -> Arc<Profile> {
    stand.feed.borrow().profile.clone().expect("a profile")
}

/// The device reports its six bands, as it does after a recall.
fn report_bands(stand: &Stand, bands: [i64; 6]) {
    let profile = stand_profile(stand);
    let statuses = BAND_KEYS.into_iter().zip(bands).map(|(key, raw)| {
        let message = svolder_profiles::intent(&profile, key, &raw.to_string()).expect("a band");
        Message::status(message.id, message.value)
    });
    publish_all(&stand.feed, statuses);
}

mod climb;
mod echo;
mod memory;
mod modes;
mod ordering;
mod recalls;
mod relative;
mod settings;
