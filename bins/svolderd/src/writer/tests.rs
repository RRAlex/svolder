// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use svolder_ipc::ValueObject;
use svolder_profiles::Registry;
use svolder_proto::Value;
use tokio::task::JoinHandle;

use super::*;
use crate::test_support::shown;

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
                let _ = reply.send(Response::Error {
                    code: 3,
                    message: "the device said no".into(),
                });
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
                let _ = reply.send(Response::Error {
                    code: 1,
                    message: "refused".into(),
                });
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

/// Writes reach the actor in the order they are asked for, across functions: a volume and a
/// mute asked for just after it arrive in that order.
#[tokio::test]
async fn writes_reach_the_actor_in_the_order_they_are_asked_for() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63), ("mute", 0)]);
    stand.gate.forget_permits(1000);
    let volume = writer.submit(Origin::Mqtt, set("volume", "50"), Mode::Latest);
    let mute = writer.submit(Origin::Mqtt, set("mute", "on"), Mode::Latest);
    stand.gate.add_permits(1000);
    assert!(matches!(
        volume.await,
        Ok(Outcome::Done(Response::Value(_)))
    ));
    assert!(matches!(mute.await, Ok(Outcome::Done(Response::Value(_)))));
    assert_eq!(log(&stand), ["set volume 50", "set mute on"]);
}

/// A burst of newest-wins writes on one control while one is in flight ends on the newest: the
/// ones between are superseded and never sent.
#[tokio::test]
async fn a_burst_of_latest_writes_on_one_control_ends_on_the_newest() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    stand.gate.forget_permits(1000);
    let answers: Vec<_> = (40..50)
        .map(|raw| writer.submit(Origin::Mqtt, set("volume", &raw.to_string()), Mode::Latest))
        .collect();
    stand.gate.add_permits(1000);
    let mut outcomes = Vec::new();
    for answer in answers {
        outcomes.push(answer.await.expect("an answer"));
    }
    assert!(matches!(outcomes[0], Outcome::Done(_)), "{outcomes:?}");
    assert!(
        outcomes[1..9]
            .iter()
            .all(|o| matches!(o, Outcome::Superseded)),
        "{outcomes:?}"
    );
    assert!(matches!(outcomes[9], Outcome::Done(_)), "{outcomes:?}");
    assert_eq!(log(&stand), ["set volume 40", "set volume 49"]);
}

/// A relative step is never dropped: two `up`s behind a write in flight both reach the actor.
#[tokio::test]
async fn relative_steps_are_never_dropped() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    stand.gate.forget_permits(1000);
    let first = writer.submit(Origin::Mqtt, set("volume", "60"), Mode::Latest);
    let ups: Vec<_> = (0..2)
        .map(|_| writer.submit(Origin::Mqtt, set("volume", "up"), Mode::Latest))
        .collect();
    stand.gate.add_permits(1000);
    first.await.expect("an answer");
    for up in ups {
        assert!(matches!(up.await, Ok(Outcome::Done(_))));
    }
    assert_eq!(
        log(&stand),
        ["set volume 60", "set volume 61", "set volume 62"]
    );
}

/// A write the profile names no function for goes to the actor as it is, which answers it in its
/// own words.
#[tokio::test]
async fn a_write_with_no_function_goes_straight_to_the_actor() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    let answer = writer
        .submit(Origin::Mqtt, set("nonesuch", "1"), Mode::Latest)
        .await;
    assert!(
        matches!(answer, Ok(Outcome::Done(Response::Error { .. }))),
        "{answer:?}"
    );
    assert_eq!(log(&stand), ["set nonesuch 1"]);
}

/// A write to a device whose actor has stopped is answered, never left waiting.
#[tokio::test]
async fn every_write_is_answered_once_the_actor_has_stopped() {
    let profile = Arc::new(
        Registry::builtin()
            .unwrap()
            .get("gungnir2-v2")
            .unwrap()
            .clone(),
    );
    let (_feed, feed) = watch::channel(Feed {
        name: "fake".into(),
        profile: Some(profile),
        ..Feed::default()
    });
    let (commands, inbox) = mpsc::channel(1);
    drop(inbox);
    let writer = Writer::spawn(commands, feed);
    let answer = writer
        .submit(Origin::Mqtt, set("volume", "50"), Mode::Latest)
        .await;
    assert!(
        matches!(
            answer,
            Ok(Outcome::Done(Response::Error { code: 3, ref message }))
                if message == "device fake is not running"
        ),
        "{answer:?}"
    );
}

/// A recall of a name no fitting preset carries is refused as it starts, saying why, and sends
/// nothing.
#[tokio::test]
async fn a_recall_of_a_name_that_fits_nothing_is_refused_and_sends_nothing() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    let Recalling { planned, done } = writer.recall(Origin::Mqtt, "Nope".into(), Vec::new());
    assert!(matches!(
        planned.await,
        Ok(Planned::Refused(ref why)) if why.starts_with("no preset named \"Nope\" fits fake")
    ));
    assert!(matches!(
        done.await,
        Ok(Outcome::Done(Response::Error { .. }))
    ));
    assert!(log(&stand).is_empty());
}

/// A stand-in actor: answers each command it is sent with the next of `answers`, recording the
/// commands in the order they arrive.
fn actor(answers: Vec<Response>) -> (Commands, Arc<std::sync::Mutex<Vec<String>>>, JoinHandle<()>) {
    let (commands, mut inbox) = mpsc::channel::<(Command, oneshot::Sender<Response>)>(8);
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    let task = tokio::spawn(async move {
        let mut answers = answers.into_iter();
        while let Some((command, reply)) = inbox.recv().await {
            record.lock().expect("the record").push(match command {
                Command::EqMode { mode, .. } => mode,
                _ => "another command".to_string(),
            });
            let _ = reply.send(answers.next().unwrap_or(Response::Ack));
        }
    });
    (commands, seen, task)
}

fn mode(name: &str) -> Command {
    Command::EqMode {
        mode: name.to_string(),
        no_wait: false,
    }
}

/// `commands` as a recall's steps, none of them to a band, carried out on `commands` with no feed
/// to watch.
async fn steps_through(actor: &Commands, commands: Vec<Command>) -> Response {
    let (_feed, feed_rx) = watch::channel(Feed::default());
    let steps = commands
        .into_iter()
        .map(|command| (command, None))
        .collect();
    let (response, stepped) = recall_steps(actor, "fake", &feed_rx, steps).await;
    assert!(stepped.is_empty(), "no step names a band");
    response
}

#[tokio::test]
async fn a_recall_sends_its_commands_in_order_and_acks() {
    let (commands, seen, _task) = actor(Vec::new());
    let steps = vec![mode("flat"), mode("eq"), mode("loudness")];
    let response = steps_through(&commands, steps).await;
    assert!(matches!(response, Response::Ack), "{response:?}");
    let seen = seen.lock().expect("the record").clone();
    assert_eq!(seen.len(), 3);
    for (sent, name) in seen.iter().zip(["flat", "eq", "loudness"]) {
        assert!(sent.contains(name), "{sent}");
    }
}

#[tokio::test]
async fn a_recall_that_fails_partway_stops_and_counts_the_writes_that_went_through() {
    let refused = Response::Error {
        code: 3,
        message: "the device said no".to_string(),
    };
    let (commands, seen, _task) = actor(vec![Response::Ack, Response::Ack, refused]);
    let steps = vec![mode("flat"), mode("eq"), mode("loudness"), mode("eq")];
    match steps_through(&commands, steps).await {
        Response::Error { code, message } => {
            assert_eq!(code, 3);
            assert_eq!(message, "stopped after 2 of 4 writes: the device said no");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        seen.lock().expect("the record").len(),
        3,
        "the fourth was sent"
    );
}

/// A write waiting behind its own control's write in flight is not overtaken by a later write to
/// another control: a volume, then a mute, reach the actor in that order.
#[tokio::test]
async fn a_queued_write_is_not_overtaken_by_a_later_write_to_another_function() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63), ("mute", 1)]);
    stand.gate.forget_permits(1000);
    let answers = [
        writer.submit(Origin::Mqtt, set("volume", "60"), Mode::Latest),
        writer.submit(Origin::Mqtt, set("volume", "40"), Mode::Latest),
        writer.submit(Origin::Mqtt, set("mute", "off"), Mode::Latest),
    ];
    stand.gate.add_permits(1000);
    for answer in answers {
        assert!(matches!(answer.await, Ok(Outcome::Done(_))));
    }
    assert_eq!(
        log(&stand),
        ["set volume 60", "set volume 40", "set mute off"]
    );
}

/// A recall the MCP server asks for holds a band write a socket client sends while it runs, and
/// sends it after the recall: the band ends on what was asked last.
#[tokio::test]
async fn a_socket_band_write_during_an_mcp_recall_waits_for_it() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    let eq = stand
        .feed
        .borrow()
        .profile
        .as_ref()
        .and_then(|profile| profile.eq.clone())
        .expect("a tone control");
    let fitting: Vec<Preset> = preset::fitting(&eq, &[])
        .into_iter()
        .map(|(preset, _)| preset)
        .collect();
    stand.gate.forget_permits(1000);
    let Recalling { planned, done } = writer.recall(Origin::Mcp, "Warm".into(), fitting);
    let band = writer.submit(
        Origin::Socket,
        Command::EqSet {
            band: 1,
            param: "g".into(),
            value: 5.0,
            no_wait: false,
        },
        Mode::Ordered,
    );
    assert_eq!(writer.view().await.held, 1, "the band waits for the recall");
    stand.gate.add_permits(1000);
    assert!(matches!(planned.await, Ok(Planned::Plan(_))));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    assert!(matches!(band.await, Ok(Outcome::Done(Response::Value(_)))));
    assert_eq!(log(&stand).last().map(String::as_str), Some("eq 1 g 5"));
}

/// A typed band write held during one recall, and a second recall asked for after it: the typed
/// write goes out between the two, so the device ends on the second preset, asked for last.
#[tokio::test]
async fn a_typed_write_held_before_a_later_recall_goes_out_before_it() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    let eq = stand
        .feed
        .borrow()
        .profile
        .as_ref()
        .and_then(|profile| profile.eq.clone())
        .expect("a tone control");
    let fitting: Vec<Preset> = preset::fitting(&eq, &[])
        .into_iter()
        .map(|(preset, _)| preset)
        .collect();
    stand.gate.forget_permits(1000);
    let first = writer.recall(Origin::Mcp, "Warm".into(), fitting.clone());
    let typed = writer.submit(
        Origin::Socket,
        Command::EqSet {
            band: 1,
            param: "g".into(),
            value: 5.0,
            no_wait: false,
        },
        Mode::Ordered,
    );
    let second = writer.recall(Origin::Mqtt, "Bass".into(), fitting);
    assert_eq!(writer.view().await.held, 1);
    stand.gate.add_permits(1000);
    let Ok(Planned::Plan(first_plan)) = first.planned.await else {
        panic!("the first recall is planned");
    };
    let Ok(Planned::Plan(second_plan)) = second.planned.await else {
        panic!("the second recall is planned");
    };
    assert!(matches!(first.done.await, Ok(Outcome::Done(Response::Ack))));
    assert!(matches!(typed.await, Ok(Outcome::Done(Response::Value(_)))));
    assert!(matches!(
        second.done.await,
        Ok(Outcome::Done(Response::Ack))
    ));
    let log = log(&stand);
    let before = first_plan.recall.commands.len();
    assert_eq!(log.len(), before + 1 + second_plan.recall.commands.len());
    assert_eq!(log[before], "eq 1 g 5", "between the two recalls");
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

/// A slider at the top climbs instead of jumping: three quarters of the range at once, then one
/// step at a time, a tenth of the range a second, answered as soon as the climb is on.
#[tokio::test(start_paused = true)]
async fn a_slider_at_the_top_climbs_instead_of_jumping() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    let started = tokio::time::Instant::now();
    let glided = glide(&writer, Origin::Mqtt, 127).await;
    assert!(
        matches!(
            glided,
            Glided::Climbing {
                first: Some(Response::Value(_)),
                target: 127
            }
        ),
        "{glided:?}"
    );
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "answered at once"
    );
    assert_eq!(volume(&stand), Some(95));
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        volume(&stand).is_some_and(|raw| raw < 127),
        "{:?}",
        volume(&stand)
    );
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(volume(&stand), Some(127));
    assert_eq!(volume_steps(&stand), (95..=127).collect::<Vec<_>>());
}

/// A write to the climbing volume from another front end ends the climb: nothing above it is
/// written after.
#[tokio::test(start_paused = true)]
async fn a_write_from_another_front_end_cancels_a_climb() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    glide(&writer, Origin::Mqtt, 127).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let answer = writer
        .submit(Origin::Socket, set("volume", "63"), Mode::Ordered)
        .await;
    assert!(
        matches!(answer, Ok(Outcome::Done(Response::Value(_)))),
        "{answer:?}"
    );
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(volume(&stand), Some(63));
    assert_eq!(volume_steps(&stand).last(), Some(&63));
}

/// A write of the very step the climb has reached, from another front end, ends the climb too: no
/// reading differs from the step written, so only the write itself says so.
#[tokio::test(start_paused = true)]
async fn a_write_of_the_step_reached_from_another_front_end_ends_the_climb() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    glide(&writer, Origin::Mqtt, 127).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let reached = volume(&stand).expect("a reading");
    writer
        .submit(
            Origin::Socket,
            set("volume", &reached.to_string()),
            Mode::Ordered,
        )
        .await
        .expect("an answer");
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(volume(&stand), Some(reached));
}

/// Glides from the owner steer its one climb: one jump to three quarters, then a step at a time
/// whatever the slider does; one below where the climb has reached lands at once.
#[tokio::test(start_paused = true)]
async fn repeated_glides_from_the_slider_steer_one_climb_at_the_glides_pace() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    glide(&writer, Origin::Mqtt, 110).await;
    for target in [120, 115, 125, 127] {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let glided = glide(&writer, Origin::Mqtt, target).await;
        assert!(
            matches!(glided, Glided::Climbing { first: None, target: t } if t == target),
            "{glided:?}"
        );
    }
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(volume(&stand), Some(127));
    let steps = volume_steps(&stand);
    assert_eq!(steps[0], 95, "{steps:?}");
    assert!(
        steps.windows(2).all(|w| w[1] == w[0] + 1),
        "one step at a time: {steps:?}"
    );
    let glided = glide(&writer, Origin::Mqtt, 100).await;
    assert!(
        matches!(glided, Glided::Landed(Response::Value(_))),
        "{glided:?}"
    );
    assert_eq!(volume(&stand), Some(100));
}

/// A step answered with another value — the knob turned while the step waited for its echo,
/// which the session takes for it — ends the climb where the device is, and nothing above it is
/// written after.
#[tokio::test(start_paused = true)]
async fn a_step_answered_with_another_value_ends_the_climb_where_the_device_is() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 100)]);
    glide(&writer, Origin::Mqtt, 127).await;
    *stand.knob.lock().expect("the knob") = Some(80);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(volume(&stand), Some(80));
    assert_eq!(volume_steps(&stand), [101], "the step the knob answered");
}

/// A reading the climb did not write — the knob turned between steps — ends it.
#[tokio::test(start_paused = true)]
async fn a_knob_turn_ends_the_climb() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    glide(&writer, Origin::Mqtt, 127).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    turn(&stand, 70);
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(volume(&stand), Some(70));
}

/// A step answered with a knob turn and followed by its own status — the device handles the write
/// after the knob's status, which the session takes for the echo — ends the climb at the knob's
/// value, though the reading is the step again by the next tick.
#[tokio::test(start_paused = true)]
async fn a_step_answered_by_a_knob_turn_and_then_by_its_own_status_ends_the_climb() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 100)]);
    glide(&writer, Origin::Mqtt, 127).await;
    *stand.late.lock().expect("the late knob") = Some(80);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(
        volume_steps(&stand),
        [101],
        "nothing is written after the step the knob answered"
    );
}

/// Mute on, and a power-off, end the climb where it is.
#[tokio::test(start_paused = true)]
async fn mute_on_or_power_off_ends_the_climb() {
    for (profile, key, start, quiet) in [
        ("gungnir2-v2", "mute", 0, "on"),
        ("freya2", "power", 1, "off"),
    ] {
        let (writer, stand) = stand_in(profile, &[("volume", 63), (key, start)]);
        glide(&writer, Origin::Mqtt, 127).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        writer
            .submit(Origin::Mqtt, set(key, quiet), Mode::Latest)
            .await
            .expect("an answer");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let at = volume(&stand);
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(
            volume(&stand),
            at,
            "{profile}: the climb went on after {key} {quiet}"
        );
    }
}

/// A mute or a power-off the device reports itself — the remote, a button — ends the climb too.
#[tokio::test(start_paused = true)]
async fn a_mute_or_power_off_the_device_reports_ends_the_climb() {
    for (profile, key, start, quiet) in [
        ("gungnir2-v2", "mute", 0, "on"),
        ("freya2", "power", 1, "off"),
    ] {
        let (writer, stand) = stand_in(profile, &[("volume", 63), (key, start)]);
        glide(&writer, Origin::Mqtt, 127).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        let held = {
            let profile = stand.feed.borrow().profile.clone().expect("a profile");
            let m = svolder_profiles::intent(&profile, key, quiet).expect("a toggle");
            Message::status(m.id, m.value)
        };
        stand.feed.send_modify(|feed| {
            feed.readings.retain(|r| r.id != held.id);
            feed.readings.push(held.clone());
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let written = volume_steps(&stand).len();
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(
            volume_steps(&stand).len(),
            written,
            "{profile}: the climb went on after the device reported {key} {quiet}"
        );
    }
}

/// A raw write to the volume's function ends the climb too.
#[tokio::test(start_paused = true)]
async fn a_raw_write_to_the_volume_cancels_the_climb() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    glide(&writer, Origin::Mqtt, 127).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let raw = Command::Raw {
        id: "10".into(),
        value_type: "u8".into(),
        value: "60".into(),
        no_wait: false,
    };
    writer
        .submit(Origin::Socket, raw, Mode::Ordered)
        .await
        .expect("an answer");
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(volume(&stand), Some(60));
}

/// A lost link, and a reload that changes the profile, end the climb.
#[tokio::test(start_paused = true)]
async fn a_lost_link_or_a_changed_profile_ends_the_climb() {
    let other = Arc::new(
        Registry::builtin()
            .unwrap()
            .get("gungnir2-v1")
            .unwrap()
            .clone(),
    );
    for lose in [
        Box::new(|feed: &mut Feed| feed.connected = false) as Box<dyn Fn(&mut Feed)>,
        Box::new(move |feed: &mut Feed| feed.profile = Some(Arc::clone(&other))),
    ] {
        let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
        glide(&writer, Origin::Mqtt, 127).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        stand.feed.send_modify(|feed| lose(feed));
        tokio::time::sleep(Duration::from_millis(100)).await;
        let written = volume_steps(&stand).len();
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(volume_steps(&stand).len(), written, "the climb went on");
    }
}

/// A climb step held behind older writes to another control, which the writer releases in the order
/// they arrived, is a write still to come, not a reading the climb did not write: the climb goes
/// on to its target once they go.
#[tokio::test(start_paused = true)]
async fn a_climb_step_waiting_behind_writes_to_another_control_does_not_end_the_climb() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 100), ("input", 1)]);
    glide(&writer, Origin::Mqtt, 127).await;
    stand.gate.forget_permits(1000);
    let held: Vec<_> = ["2", "3"]
        .into_iter()
        .map(|input| writer.submit(Origin::Socket, set("input", input), Mode::Ordered))
        .collect();
    tokio::time::sleep(Duration::from_millis(500)).await;
    stand.gate.add_permits(1000);
    for answer in held {
        answer.await.expect("an answer");
    }
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(volume(&stand), Some(127));
}

/// A knob turned while a climb step waits behind older writes to another control ends the climb,
/// and the step that waited is never sent over the knob's value.
#[tokio::test(start_paused = true)]
async fn a_knob_turn_while_a_climb_step_waits_ends_the_climb_and_the_step_is_never_sent() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 100), ("input", 1)]);
    glide(&writer, Origin::Mqtt, 127).await;
    stand.gate.forget_permits(1000);
    let held: Vec<_> = ["2", "3"]
        .into_iter()
        .map(|input| writer.submit(Origin::Socket, set("input", input), Mode::Ordered))
        .collect();
    tokio::time::sleep(Duration::from_millis(300)).await;
    turn(&stand, 90);
    tokio::time::sleep(Duration::from_millis(300)).await;
    stand.gate.add_permits(1000);
    for answer in held {
        answer.await.expect("an answer");
    }
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(volume(&stand), Some(90));
    assert!(
        volume_steps(&stand).is_empty(),
        "{:?}",
        volume_steps(&stand)
    );
}

/// A glide that starts while a lowering write is still in flight climbs from that write once it
/// is echoed: the volume still reads the old value until then, which is not a change the climb
/// did not write.
#[tokio::test(start_paused = true)]
async fn a_climb_that_starts_while_a_write_is_in_flight_goes_on_from_it() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 110)]);
    stand.gate.forget_permits(1000);
    let lowering = writer.submit(Origin::Socket, set("volume", "105"), Mode::Ordered);
    let glided = glide(&writer, Origin::Mqtt, 127).await;
    assert!(
        matches!(
            glided,
            Glided::Climbing {
                first: None,
                target: 127
            }
        ),
        "{glided:?}"
    );
    tokio::time::sleep(Duration::from_millis(500)).await;
    stand.gate.add_permits(1000);
    lowering.await.expect("an answer");
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(volume(&stand), Some(127));
    assert_eq!(volume_steps(&stand), (105..=127).collect::<Vec<_>>());
}

/// A device that reads its output muted is not climbed on, even from above three quarters, where
/// there is no first step to be echoed: nothing is written.
#[tokio::test(start_paused = true)]
async fn a_muted_device_is_not_climbed_even_from_above_three_quarters() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 110), ("mute", 1)]);
    glide(&writer, Origin::Mqtt, 127).await;
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        volume_steps(&stand).is_empty(),
        "{:?}",
        volume_steps(&stand)
    );
}

/// A glide on a device that reads its output muted lands its first step and starts no climb: it
/// is answered as landed, never as climbing. The writer's task picks between an echo and the feed
/// change it comes with at random, so the glide is made again and again.
#[tokio::test(start_paused = true)]
async fn a_glide_with_a_first_step_on_a_muted_device_lands_it_and_starts_no_climb() {
    for _ in 0..16 {
        let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63), ("mute", 1)]);
        let glided = glide(&writer, Origin::Mqtt, 127).await;
        assert!(
            matches!(glided, Glided::Landed(Response::Value(_))),
            "{glided:?}"
        );
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(volume_steps(&stand), [95]);
    }
}

/// A glide on a device that reads its output off, from above three quarters where there is no
/// first step, writes nothing and is answered with the reading, never as climbing.
#[tokio::test(start_paused = true)]
async fn a_glide_without_a_first_step_on_a_device_that_is_off_answers_the_reading() {
    let (writer, stand) = stand_in("freya2", &[("volume", 110), ("power", 0)]);
    let glided = glide(&writer, Origin::Mqtt, 127).await;
    assert!(
        matches!(&glided, Glided::Landed(Response::Value(vo)) if vo.raw.as_i64() == Some(110)),
        "{glided:?}"
    );
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(
        volume_steps(&stand).is_empty(),
        "{:?}",
        volume_steps(&stand)
    );
}

/// A write to another control leaves the climb alone.
#[tokio::test(start_paused = true)]
async fn a_write_to_another_control_leaves_the_climb_alone() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63), ("input", 1)]);
    glide(&writer, Origin::Mqtt, 127).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    writer
        .submit(Origin::Socket, set("input", "2"), Mode::Ordered)
        .await
        .expect("an answer");
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(volume(&stand), Some(127));
}

/// A volume the daemon has never read is written three quarters of its range at most, first, and
/// the climb goes on from that step's echo; a target at or under it lands at once.
#[tokio::test(start_paused = true)]
async fn an_unread_volume_is_written_three_quarters_first_then_climbs_from_its_echo() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    let glided = glide(&writer, Origin::Mqtt, 127).await;
    assert!(
        matches!(
            glided,
            Glided::Climbing {
                first: Some(_),
                target: 127
            }
        ),
        "{glided:?}"
    );
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(volume_steps(&stand), (95..=127).collect::<Vec<_>>());
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    let glided = glide(&writer, Origin::Mqtt, 60).await;
    assert!(
        matches!(glided, Glided::Landed(Response::Value(_))),
        "{glided:?}"
    );
    assert_eq!(volume_steps(&stand), [60]);
}

/// A mute asked for while a glide's first step waits for its echo — a script's "volume 100 %,
/// then mute" — stops the climb before it starts: the first step is all that is written.
#[tokio::test(start_paused = true)]
async fn a_mute_while_the_first_step_is_on_its_way_stops_the_climb_before_it_starts() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63), ("mute", 0)]);
    stand.gate.forget_permits(1000);
    let glided = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    let muted = writer.submit(Origin::Mqtt, set("mute", "on"), Mode::Latest);
    // Both asks are taken in while the first step waits at the gate.
    writer.view().await;
    stand.gate.add_permits(1000);
    let glided = glided.await.expect("an answer");
    assert!(
        matches!(glided, Glided::Landed(Response::Value(_))),
        "{glided:?}"
    );
    assert!(matches!(muted.await, Ok(Outcome::Done(Response::Value(_)))));
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert_eq!(volume_steps(&stand), [95]);
    assert_eq!(volume(&stand), Some(95));
}

/// An MCP glide waiting behind a volume write in flight is not replaced by a later MQTT slider
/// command: both go out, the MCP's first.
#[tokio::test(start_paused = true)]
async fn an_mcp_glide_waiting_is_not_replaced_by_a_later_mqtt_command() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    stand.gate.forget_permits(1000);
    let flying = writer.submit(Origin::Socket, set("volume", "60"), Mode::Ordered);
    let mcp = writer.glide(Origin::Mcp, "volume".into(), 70, Mode::Ordered);
    let mqtt = writer.glide(Origin::Mqtt, "volume".into(), 65, Mode::Latest);
    writer.view().await;
    stand.gate.add_permits(1000);
    flying.await.expect("an answer");
    for glided in [mcp, mqtt] {
        let glided = glided.await.expect("an answer");
        assert!(
            matches!(glided, Glided::Landed(Response::Value(_))),
            "{glided:?}"
        );
    }
    assert_eq!(volume_steps(&stand), [60, 70, 65]);
}

/// A knob turned while a glide's first step waits in the line behind other writes ends the glide
/// before it starts: the step is never sent over the knob's value, and no climb follows.
#[tokio::test(start_paused = true)]
async fn a_knob_turn_while_a_first_step_waits_ends_the_glide_and_the_step_is_never_sent() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63), ("input", 1)]);
    stand.gate.forget_permits(1000);
    let held: Vec<_> = ["2", "3"]
        .into_iter()
        .map(|input| writer.submit(Origin::Socket, set("input", input), Mode::Ordered))
        .collect();
    let glided = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    // The glide's first step is taken in and waits behind the two writes to the input.
    writer.view().await;
    turn(&stand, 30);
    tokio::time::sleep(Duration::from_millis(300)).await;
    stand.gate.add_permits(1000);
    for answer in held {
        answer.await.expect("an answer");
    }
    let glided = glided.await.expect("an answer");
    assert!(
        matches!(glided, Glided::Landed(Response::Value(_))),
        "{glided:?}"
    );
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(volume(&stand), Some(30));
    assert!(
        volume_steps(&stand).is_empty(),
        "{:?}",
        volume_steps(&stand)
    );
}

/// A knob turn that is ready at the same time as the answer that releases a glide's first step
/// keeps the knob's value, whichever the writer's task takes first: the step is never written.
#[tokio::test(start_paused = true)]
async fn a_knob_turn_ready_with_the_reply_that_releases_a_first_step_keeps_the_knobs_value() {
    for _ in 0..16 {
        let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 60), ("input", 1)]);
        stand.gate.forget_permits(1000);
        let held: Vec<_> = ["2", "3"]
            .into_iter()
            .map(|input| writer.submit(Origin::Socket, set("input", input), Mode::Ordered))
            .collect();
        let glided = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
        // The glide's first step is taken in and waits behind the second write to the input,
        // which waits for the first.
        writer.view().await;
        *stand.before.lock().expect("the early knob") = Some(70);
        stand.gate.add_permits(1000);
        for answer in held {
            answer.await.expect("an answer");
        }
        let glided = glided.await.expect("an answer");
        assert!(
            matches!(glided, Glided::Landed(Response::Value(_))),
            "{glided:?}"
        );
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(volume(&stand), Some(70));
        assert!(
            volume_steps(&stand).is_empty(),
            "{:?}",
            volume_steps(&stand)
        );
    }
}

/// A knob turn that is ready at the same time as the answer that releases a climb's step ends
/// the climb: the step is never written over the knob's value, whichever the writer's task takes
/// first.
#[tokio::test(start_paused = true)]
async fn a_knob_turn_ready_with_the_reply_that_releases_a_climb_step_keeps_the_knobs_value() {
    for _ in 0..16 {
        let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 100), ("input", 1)]);
        glide(&writer, Origin::Mqtt, 127).await;
        stand.gate.forget_permits(1000);
        let held: Vec<_> = ["2", "3"]
            .into_iter()
            .map(|input| writer.submit(Origin::Socket, set("input", input), Mode::Ordered))
            .collect();
        // The climb's next step is taken in and waits behind the second write to the input,
        // which waits for the first.
        tokio::time::sleep(Duration::from_millis(300)).await;
        *stand.before.lock().expect("the early knob") = Some(90);
        stand.gate.add_permits(1000);
        for answer in held {
            answer.await.expect("an answer");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(volume(&stand), Some(90));
        assert!(
            volume_steps(&stand).is_empty(),
            "{:?}",
            volume_steps(&stand)
        );
    }
}

/// A glide's first step answered with another value — the knob turned while it waited for its
/// echo — starts no climb: the device stays where the knob put it.
#[tokio::test(start_paused = true)]
async fn a_first_step_answered_with_another_value_starts_no_climb() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    *stand.knob.lock().expect("the knob") = Some(80);
    let glided = glide(&writer, Origin::Mqtt, 127).await;
    assert!(
        matches!(glided, Glided::Landed(Response::Value(_))),
        "{glided:?}"
    );
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(volume(&stand), Some(80));
    assert_eq!(
        volume_steps(&stand),
        [95],
        "the first step, which the knob answered"
    );
}

/// Two relative steps asked for before the first's echo move two steps, up and down alike: the
/// second is worked out from the step the first asked for, not from the reading it has not
/// changed yet.
#[tokio::test]
async fn two_relative_steps_before_the_echo_move_two_steps() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    for way in ["up", "down"] {
        stand.gate.forget_permits(1000);
        let steps: Vec<_> = (0..2)
            .map(|_| writer.submit(Origin::Socket, set("volume", way), Mode::Ordered))
            .collect();
        stand.gate.add_permits(1000);
        for step in steps {
            let answer = step.await;
            assert!(
                matches!(answer, Ok(Outcome::Done(Response::Value(_)))),
                "{answer:?}"
            );
        }
    }
    assert_eq!(volume_steps(&stand), [64, 65, 64, 63]);
}

/// A no-wait write answered `Ack` is awaited.
async fn acked(answer: oneshot::Receiver<Outcome>) {
    let answer = answer.await;
    assert!(
        matches!(answer, Ok(Outcome::Done(Response::Ack))),
        "{answer:?}"
    );
}

/// A `down` after a lowering sent without waiting steps from the lowering, not from the reading
/// that has not shown it yet: one asked for after the `Ack` and one asked for before the lowering
/// was sent alike.
#[tokio::test]
async fn a_down_after_a_lowering_sent_without_waiting_steps_from_the_lowering() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 120)]);
    acked(writer.submit(Origin::Socket, no_wait("volume", "50"), Mode::Ordered)).await;
    acked(writer.submit(Origin::Socket, no_wait("volume", "down"), Mode::Ordered)).await;
    show(&stand);
    assert_eq!(volume_steps(&stand), [50, 49]);
    assert_eq!(volume(&stand), Some(49));

    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 120)]);
    stand.gate.forget_permits(1000);
    let lowering = writer.submit(Origin::Socket, no_wait("volume", "50"), Mode::Ordered);
    let down = writer.submit(Origin::Socket, no_wait("volume", "down"), Mode::Ordered);
    stand.gate.add_permits(1000);
    acked(lowering).await;
    acked(down).await;
    show(&stand);
    assert_eq!(volume_steps(&stand), [50, 49]);
    assert_eq!(volume(&stand), Some(49));
}

/// A waited lowering whose answer is the timeout may have landed, so the `down` after it steps
/// from the lowering, not from the louder reading the device has not echoed: 120, `set volume 50`
/// timing out, then `down` writes 49, not 119.
#[tokio::test]
async fn a_down_after_a_waited_lowering_that_timed_out_steps_from_the_lowering() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 120)]);
    stand
        .times_out
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let lowered = writer
        .submit(Origin::Socket, set("volume", "50"), Mode::Ordered)
        .await;
    assert!(
        matches!(lowered, Ok(Outcome::Done(Response::Error { code: 4, .. }))),
        "{lowered:?}"
    );
    let down = writer
        .submit(Origin::Socket, set("volume", "down"), Mode::Ordered)
        .await;
    assert!(
        matches!(down, Ok(Outcome::Done(Response::Value(_)))),
        "{down:?}"
    );
    assert_eq!(volume_steps(&stand), [50, 49]);
}

/// The answers that say a waited write timed out are told from the code 4 refusals that say it
/// never went out.
#[test]
fn only_a_timeout_answer_says_the_write_may_have_landed() {
    let words = |message: String| Response::Error { code: 4, message };
    let echo = crate::device::error_for(&svolder_ble::BleError::Timeout(Duration::from_secs(2)));
    assert!(timed_out(&echo));
    assert!(timed_out(&unanswered("fake")));
    assert!(!timed_out(&not_taken("fake")));
    assert!(!timed_out(&no_reading("volume")));
    assert!(!timed_out(&words(
        "no value reported yet for volume".into()
    )));
    assert!(!timed_out(&Response::Error {
        code: 1,
        message: "no confirmation within 2s".into()
    }));
    assert!(!timed_out(&Response::Ack));
}

/// A `down` after a lowering given as text (`min`) steps from the lowering, not from the reading
/// that has not shown it yet: the write is read the way the actor reads it.
#[tokio::test]
async fn a_down_after_a_lowering_given_as_text_steps_from_the_lowering() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 120)]);
    acked(writer.submit(Origin::Socket, no_wait("volume", "min"), Mode::Ordered)).await;
    acked(writer.submit(Origin::Socket, no_wait("volume", "down"), Mode::Ordered)).await;
    show(&stand);
    // The lowering is logged as typed; the `down` is the numeric step.
    assert_eq!(log(&stand), ["set volume min", "set volume 0"]);
}

/// An `up` after a lowering given in decibels steps one above the lowering, not from the reading
/// that has not shown it yet.
#[tokio::test]
async fn an_up_after_a_lowering_given_in_decibels_steps_from_the_lowering() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 120)]);
    acked(writer.submit(Origin::Socket, no_wait("volume", "-40dB"), Mode::Ordered)).await;
    acked(writer.submit(Origin::Socket, no_wait("volume", "up"), Mode::Ordered)).await;
    show(&stand);
    let profile = stand.feed.borrow().profile.clone().expect("a profile");
    let lowered = svolder_profiles::intent(&profile, "volume", "-40dB").expect("a step");
    let lowered = lowered.value.as_i64().expect("a raw step");
    assert_eq!(
        log(&stand),
        [
            "set volume -40dB".to_string(),
            format!("set volume {}", lowered + 1)
        ]
    );
}

/// A glide after a lowering given as text, its echo still late, writes three quarters first and
/// climbs a step at a time: it is judged from the lowering, not from the reading.
#[tokio::test(start_paused = true)]
async fn a_glide_after_a_lowering_given_as_text_writes_three_quarters_first() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 120)]);
    acked(writer.submit(Origin::Socket, no_wait("volume", "min"), Mode::Ordered)).await;
    let glided = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    let _ = glided.await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    show(&stand);
    let steps = volume_steps(&stand);
    assert_eq!(steps[0], 95, "{steps:?}");
    assert!(single_steps(&steps), "{steps:?}");
}

/// A `down` waiting behind a lowering sent without waiting is held to the lowering when the
/// reading rises meanwhile, a knob turned up: it is never louder than the step it arrived as.
#[tokio::test]
async fn a_down_waiting_behind_a_lowering_stays_below_it_when_the_reading_rises() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 120)]);
    stand.gate.forget_permits(1000);
    let lowering = writer.submit(Origin::Socket, no_wait("volume", "50"), Mode::Ordered);
    let down = writer.submit(Origin::Socket, no_wait("volume", "down"), Mode::Ordered);
    writer.view().await;
    turn(&stand, 100);
    stand.gate.add_permits(1000);
    acked(lowering).await;
    acked(down).await;
    show(&stand);
    assert_eq!(volume_steps(&stand), [50, 49]);
}

/// A `down` whose lowering has been echoed and whose knob then turned up, while it waits, is
/// worked out from nothing it was sent after: it goes out no louder than its arrival step.
#[tokio::test]
async fn a_down_waiting_while_the_knob_turns_up_is_never_louder_than_its_arrival_step() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 120), ("input", 1)]);
    acked(writer.submit(Origin::Socket, no_wait("volume", "50"), Mode::Ordered)).await;
    stand.gate.forget_permits(1000);
    let inputs = [
        writer.submit(Origin::Socket, no_wait("input", "2"), Mode::Ordered),
        writer.submit(Origin::Socket, no_wait("input", "3"), Mode::Ordered),
    ];
    let down = writer.submit(
        Origin::Socket,
        relative("volume", "down", 1, true),
        Mode::Ordered,
    );
    writer.view().await;
    show(&stand);
    writer.view().await;
    turn(&stand, 100);
    writer.view().await;
    stand.gate.add_permits(1000);
    for input in inputs {
        acked(input).await;
    }
    acked(down).await;
    show(&stand);
    assert_eq!(volume_steps(&stand), [50, 49]);
}

/// A `down` waiting while a no-wait step's echo is lost to a knob turn is never louder than the
/// reading less its step, though the step asked and the step sent are still held: a `down` by one
/// from a knob at 63 goes to 62, not 64, and a `down` by three from a knob at 70 to 67, not 73.
#[tokio::test]
async fn a_down_after_a_hidden_echo_and_a_knob_turn_steps_down_from_the_reading() {
    for (start, asked, knob, step, want) in [(70, 65, 63, 1, 62), (70, 76, 70, 3, 67)] {
        for waiting in [true, false] {
            let (writer, stand) =
                stand_in_acking("gungnir2-v2", &[("volume", start), ("input", 1)]);
            acked(writer.submit(
                Origin::Socket,
                no_wait("volume", &asked.to_string()),
                Mode::Ordered,
            ))
            .await;
            stand.gate.forget_permits(1000);
            let inputs = [
                writer.submit(Origin::Socket, no_wait("input", "2"), Mode::Ordered),
                writer.submit(Origin::Socket, no_wait("input", "3"), Mode::Ordered),
            ];
            let ask = || {
                writer.submit(
                    Origin::Socket,
                    relative("volume", "down", step, true),
                    Mode::Ordered,
                )
            };
            // The step's echo and the knob's status reach the writer as one change: the `down`
            // is worked out before it, and goes out after it, or arrives after it.
            let down = match waiting {
                true => {
                    let down = ask();
                    writer.view().await;
                    turn(&stand, knob);
                    down
                }
                false => {
                    turn(&stand, knob);
                    writer.view().await;
                    ask()
                }
            };
            writer.view().await;
            stand.gate.add_permits(1000);
            for input in inputs {
                acked(input).await;
            }
            acked(down).await;
            show(&stand);
            assert_eq!(volume_steps(&stand), [asked, want], "waiting: {waiting}");
        }
    }
}

/// Two `up`s sent without waiting move two steps before either echo has shown, asked for one
/// after the other's `Ack` and both asked for before the first was sent alike.
#[tokio::test]
async fn two_ups_sent_without_waiting_move_two_steps_before_either_echo() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 63)]);
    acked(writer.submit(Origin::Socket, no_wait("volume", "up"), Mode::Ordered)).await;
    acked(writer.submit(Origin::Socket, no_wait("volume", "up"), Mode::Ordered)).await;
    show(&stand);
    assert_eq!(volume_steps(&stand), [64, 65]);

    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 63)]);
    stand.gate.forget_permits(1000);
    let ups: Vec<_> = (0..2)
        .map(|_| writer.submit(Origin::Socket, no_wait("volume", "up"), Mode::Ordered))
        .collect();
    stand.gate.add_permits(1000);
    for up in ups {
        acked(up).await;
    }
    show(&stand);
    assert_eq!(volume_steps(&stand), [64, 65]);
}

/// A relative `set` of `key`, `way` (`up` or `down`), by `step`, waited for or not.
fn relative(key: &str, way: &str, step: i64, no_wait: bool) -> Command {
    Command::Set {
        key: key.into(),
        value: way.into(),
        step,
        no_wait,
    }
}

/// A `down` that drops a waiting slider value is worked out from the device, not from the value
/// it dropped, and goes down.
#[tokio::test]
async fn a_down_after_a_superseded_slider_value_steps_down_from_the_device() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    stand.gate.forget_permits(1000);
    let flying = writer.submit(Origin::Socket, set("volume", "63"), Mode::Ordered);
    let slider = writer.glide(Origin::Mqtt, "volume".into(), 65, Mode::Latest);
    writer.view().await;
    let down = writer.submit(
        Origin::Socket,
        relative("volume", "down", 1, false),
        Mode::Ordered,
    );
    writer.view().await;
    stand.gate.add_permits(1000);
    flying.await.expect("an answer");
    down.await.expect("an answer");
    let _ = slider.await;
    assert_eq!(volume_steps(&stand), [63, 62]);
}

/// An `up` that drops a waiting slider value is worked out from the device too, not from the value
/// it dropped: by two steps from 92 it stays under the volume rule.
#[tokio::test]
async fn an_up_after_a_superseded_slider_value_steps_from_the_device() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 92)]);
    stand.gate.forget_permits(1000);
    let flying = writer.submit(Origin::Socket, set("volume", "92"), Mode::Ordered);
    let slider = writer.glide(Origin::Mqtt, "volume".into(), 95, Mode::Latest);
    writer.view().await;
    let up = writer.submit(
        Origin::Socket,
        relative("volume", "up", 2, false),
        Mode::Ordered,
    );
    writer.view().await;
    stand.gate.add_permits(1000);
    flying.await.expect("an answer");
    up.await.expect("an answer");
    let _ = slider.await;
    assert_eq!(volume_steps(&stand), [92, 94]);
}

/// An `up` that drops a glide's waiting first step is worked out from the write in flight, not
/// from the step it dropped.
#[tokio::test]
async fn an_up_after_a_superseded_first_step_steps_from_the_write_in_flight() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 120)]);
    stand.gate.forget_permits(1000);
    let flying = writer.submit(Origin::Socket, no_wait("volume", "50"), Mode::Ordered);
    let glided = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    writer.view().await;
    let up = writer.submit(
        Origin::Socket,
        relative("volume", "up", 1, true),
        Mode::Ordered,
    );
    writer.view().await;
    stand.gate.add_permits(1000);
    acked(flying).await;
    acked(up).await;
    let _ = glided.await;
    show(&stand);
    assert_eq!(volume_steps(&stand), [50, 51]);
}

/// An `up` after the knob turned while a glide's first step waited steps from the knob's value:
/// the step that will not be sent is not where it starts.
#[tokio::test(start_paused = true)]
async fn an_up_after_a_first_step_that_will_not_be_sent_steps_from_the_reading() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 60), ("input", 1)]);
    stand.gate.forget_permits(1000);
    let held: Vec<_> = ["2", "3"]
        .into_iter()
        .map(|input| writer.submit(Origin::Socket, set("input", input), Mode::Ordered))
        .collect();
    let glided = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    // The glide's first step is taken in and waits behind the two writes to the input.
    writer.view().await;
    turn(&stand, 70);
    tokio::time::sleep(Duration::from_millis(10)).await;
    let up = writer.submit(Origin::Socket, set("volume", "up"), Mode::Ordered);
    writer.view().await;
    stand.gate.add_permits(1000);
    for answer in held {
        answer.await.expect("an answer");
    }
    up.await.expect("an answer");
    // The `up` takes the place of the waiting first step, whose glide is told so.
    let _ = glided.await;
    assert_eq!(volume_steps(&stand), [71]);
}

/// A glide that comes while a glide's first step waits that will not be sent is judged from the
/// write in flight, not from that step or from the reading that has not shown the write yet:
/// nothing above three quarters goes out before the first step's echo.
#[tokio::test(start_paused = true)]
async fn a_glide_after_an_outlived_first_step_is_judged_from_the_write_in_flight() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 120), ("input", 1)]);
    stand.gate.forget_permits(1000);
    let lowering = writer.submit(Origin::Socket, no_wait("volume", "50"), Mode::Ordered);
    let held: Vec<_> = ["2", "3"]
        .into_iter()
        .map(|input| writer.submit(Origin::Socket, no_wait("input", input), Mode::Ordered))
        .collect();
    let first = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    writer.view().await;
    // The link drops and comes back: the first step's glide is over, its step still waits.
    stand.feed.send_modify(|feed| feed.connected = false);
    tokio::time::sleep(Duration::from_millis(10)).await;
    stand.feed.send_modify(|feed| feed.connected = true);
    tokio::time::sleep(Duration::from_millis(10)).await;
    let second = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    writer.view().await;
    stand.gate.add_permits(1000);
    acked(lowering).await;
    for answer in held {
        acked(answer).await;
    }
    let _ = first.await;
    let _ = second.await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    let steps = volume_steps(&stand);
    assert_eq!(steps[..2], [50, 95], "{steps:?}");
    assert_eq!(volume(&stand), Some(127), "{steps:?}");
}

/// A glide that takes the place of a waiting write climbs although the write in flight, which
/// came before it, has its echo to show first: that echo does not end the glide.
#[tokio::test(start_paused = true)]
async fn a_glide_that_replaces_a_waiting_write_climbs_past_the_write_in_flight() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 70), ("input", 1)]);
    stand.gate.forget_permits(1000);
    let flying = writer.glide(Origin::Mqtt, "volume".into(), 80, Mode::Latest);
    writer.view().await;
    let waiting = writer.glide(Origin::Mqtt, "volume".into(), 85, Mode::Latest);
    writer.view().await;
    let glided = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    writer.view().await;
    stand.gate.add_permits(1000);
    let _ = flying.await;
    let _ = waiting.await;
    let _ = glided.await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    let steps = volume_steps(&stand);
    assert_eq!(steps[..2], [80, 95], "{steps:?}");
    assert!(single_steps(&steps[1..]), "{steps:?}");
    assert_eq!(volume(&stand), Some(127), "{steps:?}");
}

/// A glide that comes while writes wait behind one in flight climbs although each of their
/// echoes shows before the first step's.
#[tokio::test(start_paused = true)]
async fn a_glide_climbs_past_the_echoes_of_the_writes_that_wait() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 70), ("input", 1)]);
    stand.gate.forget_permits(1000);
    let asked: Vec<_> = ["80", "82", "85"]
        .into_iter()
        .map(|value| writer.submit(Origin::Socket, set("volume", value), Mode::Ordered))
        .collect();
    writer.view().await;
    let glided = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    writer.view().await;
    stand.gate.add_permits(1000);
    for answer in asked {
        answer.await.expect("an answer");
    }
    let _ = glided.await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    let steps = volume_steps(&stand);
    assert_eq!(steps[..4], [80, 82, 85, 95], "{steps:?}");
    assert!(single_steps(&steps[3..]), "{steps:?}");
    assert_eq!(volume(&stand), Some(127), "{steps:?}");
}

/// The echo of a write sent without waiting and answered `Ack`, shown alone before the first
/// step's, does not end a glide that started after it: the climb goes on.
#[tokio::test(start_paused = true)]
async fn a_glide_climbs_past_the_echo_of_a_write_answered_ack_shown_alone() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 120), ("input", 1)]);
    stand.gate.forget_permits(1000);
    let lowering = writer.submit(Origin::Socket, no_wait("volume", "50"), Mode::Ordered);
    let held: Vec<_> = ["2", "3"]
        .into_iter()
        .map(|input| writer.submit(Origin::Socket, no_wait("input", input), Mode::Ordered))
        .collect();
    let first = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    writer.view().await;
    // The link drops and comes back: the first step's glide is over, its step still waits.
    stand.feed.send_modify(|feed| feed.connected = false);
    tokio::time::sleep(Duration::from_millis(10)).await;
    stand.feed.send_modify(|feed| feed.connected = true);
    tokio::time::sleep(Duration::from_millis(10)).await;
    let second = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    writer.view().await;
    stand.gate.add_permits(1);
    acked(lowering).await;
    // The lowering's echo shows alone, before anything else is sent.
    show(&stand);
    writer.view().await;
    stand.gate.add_permits(1000);
    for answer in held {
        acked(answer).await;
    }
    let _ = first.await;
    let _ = second.await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    let steps = volume_steps(&stand);
    assert_eq!(steps[..2], [50, 95], "{steps:?}");
    assert!(single_steps(&steps[1..]), "{steps:?}");
    assert_eq!(volume(&stand), Some(127), "{steps:?}");
}

/// A glide with no first step, after a glide's first step that will not be sent, climbs from the
/// knob's value: the step that waited is not where it starts.
#[tokio::test(start_paused = true)]
async fn a_glide_after_an_outlived_first_step_climbs_from_the_knobs_value() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 60), ("input", 1)]);
    stand.gate.forget_permits(1000);
    let held: Vec<_> = ["2", "3"]
        .into_iter()
        .map(|input| writer.submit(Origin::Socket, set("input", input), Mode::Ordered))
        .collect();
    let first = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    // The glide's first step is taken in and waits behind the two writes to the input.
    writer.view().await;
    turn(&stand, 100);
    tokio::time::sleep(Duration::from_millis(10)).await;
    let second = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    writer.view().await;
    stand.gate.add_permits(1000);
    for answer in held {
        answer.await.expect("an answer");
    }
    let _ = first.await;
    let _ = second.await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(volume_steps(&stand), (101..=127).collect::<Vec<_>>());
    assert_eq!(volume(&stand), Some(127));
}

/// The same for a climb's step that will not be sent: a later glide climbs from the knob's value.
#[tokio::test(start_paused = true)]
async fn a_glide_after_an_outlived_climb_step_climbs_from_the_knobs_value() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 100), ("input", 1)]);
    glide(&writer, Origin::Mqtt, 127).await;
    stand.gate.forget_permits(1000);
    let held: Vec<_> = ["2", "3"]
        .into_iter()
        .map(|input| writer.submit(Origin::Socket, set("input", input), Mode::Ordered))
        .collect();
    // The climb's next step is taken in and waits behind the two writes to the input.
    tokio::time::sleep(Duration::from_millis(300)).await;
    turn(&stand, 110);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let second = writer.glide(Origin::Mqtt, "volume".into(), 127, Mode::Latest);
    writer.view().await;
    stand.gate.add_permits(1000);
    for answer in held {
        answer.await.expect("an answer");
    }
    let _ = second.await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert_eq!(volume_steps(&stand), (111..=127).collect::<Vec<_>>());
    assert_eq!(volume(&stand), Some(127));
}

/// Every step after the first is one above the one before.
fn single_steps(steps: &[i64]) -> bool {
    steps.windows(2).all(|pair| pair[1] == pair[0] + 1)
}

/// A glide that comes while a climb's last step is in flight climbs on from that step, which will
/// land: it is not a step that will never be sent.
#[tokio::test(start_paused = true)]
async fn a_glide_while_a_climbs_last_step_is_in_flight_climbs_on_from_it() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 100)]);
    glide(&writer, Origin::Mqtt, 101).await;
    stand.gate.forget_permits(1000);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(writer.view().await.in_flight.len(), 1, "101 is in flight");
    let second = glide(&writer, Origin::Mqtt, 110).await;
    assert!(
        matches!(
            second,
            Glided::Climbing {
                first: None,
                target: 110
            }
        ),
        "{second:?}"
    );
    stand.gate.add_permits(1000);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(volume(&stand), Some(110), "{:?}", volume_steps(&stand));
    assert_eq!(volume_steps(&stand), (101..=110).collect::<Vec<_>>());
}

/// A glide from another front end that comes while a climb's step is in flight climbs on from
/// that step, one step at a time.
#[tokio::test(start_paused = true)]
async fn a_glide_from_another_front_end_while_a_climb_step_is_in_flight_climbs_on_from_it() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 100)]);
    glide(&writer, Origin::Mqtt, 127).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    stand.gate.forget_permits(1000);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        writer.view().await.in_flight.len(),
        1,
        "a step is in flight"
    );
    let second = glide(&writer, Origin::Socket, 120).await;
    assert!(
        matches!(
            second,
            Glided::Climbing {
                first: None,
                target: 120
            }
        ),
        "{second:?}"
    );
    stand.gate.add_permits(1000);
    tokio::time::sleep(Duration::from_secs(5)).await;
    let steps = volume_steps(&stand);
    assert_eq!(volume(&stand), Some(120), "{steps:?}");
    assert!(single_steps(&steps), "{steps:?}");
}

/// A glide with no first step, after a `down` that waits, climbs from where the `down` lands: the
/// `down`'s step is never above the reading less its step, so neither is where the climb starts.
#[tokio::test(start_paused = true)]
async fn a_glide_after_a_waiting_down_climbs_from_where_the_down_lands() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 100), ("input", 1)]);
    acked(writer.submit(Origin::Socket, no_wait("volume", "99"), Mode::Ordered)).await;
    stand.gate.forget_permits(1000);
    let inputs = [
        writer.submit(Origin::Socket, no_wait("input", "2"), Mode::Ordered),
        writer.submit(Origin::Socket, no_wait("input", "3"), Mode::Ordered),
    ];
    // The knob's status and the echo of the 99 reach the writer as one change.
    turn(&stand, 97);
    writer.view().await;
    let down = writer.submit(
        Origin::Socket,
        relative("volume", "down", 1, false),
        Mode::Ordered,
    );
    let glided = writer.glide(Origin::Mqtt, "volume".into(), 110, Mode::Latest);
    writer.view().await;
    stand.gate.add_permits(1000);
    for input in inputs {
        acked(input).await;
    }
    let _ = down.await;
    let _ = glided.await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    let steps = volume_steps(&stand);
    assert_eq!(volume(&stand), Some(110), "{steps:?}");
    assert!(single_steps(&steps[1..]), "{steps:?}");
}

/// A climb's last step waiting behind other writes is never sent over a knob turn, whether the
/// knob turned before the answer that releases it or at the same time.
#[tokio::test(start_paused = true)]
async fn a_climbs_last_step_waiting_over_a_knob_turn_is_never_sent() {
    for same_poll in [false, true] {
        for _ in 0..16 {
            let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 100), ("input", 1)]);
            glide(&writer, Origin::Mqtt, 101).await;
            stand.gate.forget_permits(1000);
            let held: Vec<_> = ["2", "3"]
                .into_iter()
                .map(|input| writer.submit(Origin::Socket, set("input", input), Mode::Ordered))
                .collect();
            tokio::time::sleep(Duration::from_millis(300)).await;
            match same_poll {
                false => {
                    turn(&stand, 90);
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                true => *stand.before.lock().expect("the early knob") = Some(90),
            }
            stand.gate.add_permits(1000);
            for answer in held {
                answer.await.expect("an answer");
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
            assert_eq!(volume(&stand), Some(90), "same poll: {same_poll}");
            assert!(
                volume_steps(&stand).is_empty(),
                "same poll: {same_poll}: {:?}",
                volume_steps(&stand)
            );
        }
    }
}

/// Three `up`s sent without waiting move three steps when the first's echo shows while the third
/// still waits.
#[tokio::test]
async fn three_quick_ups_move_three_steps_when_an_echo_shows_between_them() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("volume", 63)]);
    stand.gate.forget_permits(1000);
    let ups: Vec<_> = (0..3)
        .map(|_| writer.submit(Origin::Socket, no_wait("volume", "up"), Mode::Ordered))
        .collect();
    writer.view().await;
    stand.gate.add_permits(1);
    let mut ups = ups.into_iter();
    acked(ups.next().expect("a first")).await;
    writer.view().await;
    show(&stand);
    writer.view().await;
    stand.gate.add_permits(1000);
    for up in ups {
        acked(up).await;
    }
    show(&stand);
    assert_eq!(volume_steps(&stand), [64, 65, 66]);
}

/// Quick relative steps never lower the volume: four `up`s asked for before any echo write rising
/// steps, and the fourth stalls at the third rather than falling back to the reading.
#[tokio::test]
async fn quick_relative_steps_up_never_lower_the_volume() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    stand.gate.forget_permits(1000);
    let steps: Vec<_> = (0..4)
        .map(|_| writer.submit(Origin::Socket, set("volume", "up"), Mode::Ordered))
        .collect();
    stand.gate.add_permits(1000);
    for step in steps {
        step.await.expect("an answer");
    }
    let written = volume_steps(&stand);
    assert_eq!(written, [64, 65, 66, 66]);
    assert!(
        written.windows(2).all(|pair| pair[1] >= pair[0]),
        "{written:?}"
    );
}

/// A relative step after an absolute raise that has not shown yet moves one step from the reading,
/// not three: the raise may have been dropped.
#[tokio::test]
async fn a_relative_step_after_an_absolute_raise_not_yet_shown_steps_from_the_reading() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    stand.gate.forget_permits(1000);
    let raise = writer.submit(Origin::Socket, set("volume", "70"), Mode::Ordered);
    let up = writer.submit(Origin::Socket, set("volume", "up"), Mode::Ordered);
    stand.gate.add_permits(1000);
    raise.await.expect("an answer");
    up.await.expect("an answer");
    assert_eq!(volume_steps(&stand), [70, 64]);
}

/// A relative step that waits behind writes to another control is worked out when it is sent: a
/// knob turned to a lower value meanwhile is stepped from, not undone.
#[tokio::test(start_paused = true)]
async fn a_relative_step_waiting_behind_another_control_steps_from_the_knobs_value() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63), ("input", 1)]);
    stand.gate.forget_permits(1000);
    let mut held: Vec<_> = ["2", "3"]
        .into_iter()
        .map(|input| writer.submit(Origin::Socket, set("input", input), Mode::Ordered))
        .collect();
    held.push(writer.submit(Origin::Socket, set("volume", "up"), Mode::Ordered));
    writer.view().await;
    turn(&stand, 30);
    tokio::time::sleep(Duration::from_millis(300)).await;
    stand.gate.add_permits(1000);
    for answer in held {
        answer.await.expect("an answer");
    }
    assert_eq!(volume_steps(&stand), [31]);
    assert_eq!(volume(&stand), Some(31));
}

/// A relative step after a write the device refused starts from the reading, never from the
/// step the refused write asked for: `set volume 200` is refused, and the `up` after it writes
/// 64, not the top.
#[tokio::test]
async fn a_relative_step_after_a_refused_write_steps_from_the_reading() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    let refused = writer
        .submit(Origin::Socket, set("volume", "200"), Mode::Ordered)
        .await;
    assert!(
        matches!(refused, Ok(Outcome::Done(Response::Error { .. }))),
        "{refused:?}"
    );
    let up = writer
        .submit(Origin::Socket, set("volume", "up"), Mode::Ordered)
        .await;
    assert!(
        matches!(up, Ok(Outcome::Done(Response::Value(_)))),
        "{up:?}"
    );
    assert_eq!(volume_steps(&stand), [200, 64]);
    assert_eq!(volume(&stand), Some(64));
}

/// A relative step after a write the device answered with another value — the knob turned
/// while it waited for its echo — starts from where the device is: `set volume 110` answered as
/// 80, then `up` writes 81, not 111.
#[tokio::test]
async fn a_relative_step_after_a_knob_answer_steps_from_the_reading() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    *stand.knob.lock().expect("the knob") = Some(80);
    let answered = writer
        .submit(Origin::Socket, set("volume", "110"), Mode::Ordered)
        .await;
    assert!(
        matches!(answered, Ok(Outcome::Done(Response::Value(_)))),
        "{answered:?}"
    );
    assert_eq!(volume(&stand), Some(80));
    let up = writer
        .submit(Origin::Socket, set("volume", "up"), Mode::Ordered)
        .await;
    assert!(
        matches!(up, Ok(Outcome::Done(Response::Value(_)))),
        "{up:?}"
    );
    assert_eq!(volume_steps(&stand), [110, 81]);
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

/// A recall sets the memory as its plan starts, whoever asked for it, in the preset's own
/// spelling; a socket client's note of its own recall sets it too.
#[tokio::test]
async fn the_memory_is_the_last_recall_any_front_end_made() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    let fitting = fitting_of(&stand);
    let recalled = writer.recalled();
    assert_eq!(*recalled.borrow(), None);
    let Recalling { planned, done } = writer.recall(Origin::Mqtt, "warm".into(), fitting.clone());
    assert!(matches!(planned.await, Ok(Planned::Plan(_))));
    assert_eq!(recalled.borrow().as_deref(), Some("Warm"));
    done.await.expect("an answer");
    writer.note_recalled("Bass".into());
    writer.view().await;
    assert_eq!(recalled.borrow().as_deref(), Some("Bass"));
    let Recalling { planned, .. } = writer.recall(Origin::Mcp, "Nope".into(), fitting);
    assert!(matches!(planned.await, Ok(Planned::Refused(_))));
    assert_eq!(
        recalled.borrow().as_deref(),
        Some("Bass"),
        "a refused recall sets nothing"
    );
}

/// The climb is published as it starts, as its owner steers it, and as it ends.
#[tokio::test(start_paused = true)]
async fn the_climb_is_published_as_it_starts_steers_and_ends() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("volume", 63)]);
    let climb = writer.climb();
    assert_eq!(*climb.borrow(), None);
    glide(&writer, Origin::Mqtt, 120).await;
    writer.view().await;
    assert_eq!(
        *climb.borrow(),
        Some(ClimbView {
            key: "volume".into(),
            target: 120
        })
    );
    glide(&writer, Origin::Mqtt, 127).await;
    writer.view().await;
    assert_eq!(climb.borrow().as_ref().map(|c| c.target), Some(127));
    writer
        .submit(Origin::Socket, set("volume", "60"), Mode::Ordered)
        .await
        .expect("an answer");
    writer.view().await;
    assert_eq!(*climb.borrow(), None);
    drop(stand);
}

/// Make the stand-in's tone control read `mode`, as a device that has reported it.
fn report_mode(stand: &Stand, mode: EqMode) {
    stand.feed.send_modify(|feed| {
        let profile = feed.profile.clone().expect("a profile");
        let message = svolder_profiles::eq_mode(&profile, mode).expect("a mode message");
        let status = Message::status(message.id, message.value);
        feed.readings.retain(|m| m.id != status.id);
        feed.readings.push(status);
    });
}

/// Wait until the writer has nothing in flight: a switch it decided on has been answered.
async fn settled(writer: &Writer) {
    for _ in 0..100 {
        if writer.view().await.in_flight.is_empty() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("the writer never settled");
}

/// A band write in Flat from a front end that asks for it is followed by the mode EQ, from the
/// same origin; one that does not ask (the socket's: the TUI switches itself) is not.
#[tokio::test]
async fn a_band_in_flat_switches_to_eq_when_its_front_end_asks() {
    let band = || Command::EqSet {
        band: 1,
        param: "g".into(),
        value: 5.0,
        no_wait: false,
    };
    let asks = Options {
        mode: Mode::Latest,
        flat_to_eq: true,
    };
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    report_mode(&stand, EqMode::Flat);
    let answer = writer.submit(Origin::Mqtt, band(), asks).await;
    assert!(
        matches!(answer, Ok(Outcome::Done(Response::Value(_)))),
        "{answer:?}"
    );
    writer.view().await;
    settled(&writer).await;
    assert_eq!(log(&stand), ["eq 1 g 5", "mode eq"]);

    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    report_mode(&stand, EqMode::Flat);
    writer
        .submit(Origin::Socket, band(), Mode::Ordered)
        .await
        .expect("an answer");
    settled(&writer).await;
    assert_eq!(log(&stand), ["eq 1 g 5"]);
}

/// A mode write that arrived after the band says what the user wants last: no switch follows.
#[tokio::test]
async fn a_mode_asked_for_after_the_band_holds_the_switch_back() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    report_mode(&stand, EqMode::Flat);
    stand.gate.forget_permits(1000);
    let band = writer.submit(
        Origin::Mqtt,
        Command::EqSet {
            band: 1,
            param: "g".into(),
            value: 5.0,
            no_wait: false,
        },
        Options {
            mode: Mode::Latest,
            flat_to_eq: true,
        },
    );
    let flat = writer.submit(Origin::Mqtt, mode("flat"), Mode::Latest);
    stand.gate.add_permits(1000);
    band.await.expect("an answer");
    flat.await.expect("an answer");
    settled(&writer).await;
    assert_eq!(log(&stand), ["eq 1 g 5", "mode flat"]);
}

/// A recall submitted after the band writes its own mode: no switch to EQ follows the band, the
/// mode the recall wrote stands.
#[tokio::test]
async fn a_recall_submitted_after_the_band_holds_the_switch_back() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    report_mode(&stand, EqMode::Flat);
    let eq = stand
        .feed
        .borrow()
        .profile
        .as_ref()
        .and_then(|profile| profile.eq.clone())
        .expect("a tone control");
    let mut quiet = preset::fitting(&eq, &[])
        .into_iter()
        .map(|(preset, _)| preset)
        .next()
        .expect("a built-in preset");
    quiet.name = "Quiet".into();
    quiet.mode = EqMode::Loudness;
    stand.gate.forget_permits(1000);
    let band = writer.submit(
        Origin::Mqtt,
        Command::EqSet {
            band: 1,
            param: "g".into(),
            value: 5.0,
            no_wait: false,
        },
        Options {
            mode: Mode::Latest,
            flat_to_eq: true,
        },
    );
    let Recalling { planned, done } = writer.recall(Origin::Mqtt, "Quiet".into(), vec![quiet]);
    stand.gate.add_permits(1000);
    band.await.expect("an answer");
    assert!(matches!(planned.await, Ok(Planned::Plan(_))));
    done.await.expect("an answer");
    settled(&writer).await;
    let log = log(&stand);
    assert_eq!(log[0], "eq 1 g 5", "{log:?}");
    assert!(log.contains(&"mode loudness".to_string()), "{log:?}");
    assert!(!log.contains(&"mode eq".to_string()), "{log:?}");
}

/// Only a recall submitted after the band holds the switch back: one submitted before it has
/// written its mode already, and a band written after it in Flat is followed by the mode EQ.
#[tokio::test]
async fn a_recall_submitted_before_the_band_does_not_hold_the_switch_back() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    let eq = stand
        .feed
        .borrow()
        .profile
        .as_ref()
        .and_then(|profile| profile.eq.clone())
        .expect("a tone control");
    let mut bare = preset::fitting(&eq, &[])
        .into_iter()
        .map(|(preset, _)| preset)
        .next()
        .expect("a built-in preset");
    bare.name = "Bare".into();
    bare.mode = EqMode::Flat;
    let Recalling { planned, done } = writer.recall(Origin::Mqtt, "Bare".into(), vec![bare]);
    assert!(matches!(planned.await, Ok(Planned::Plan(_))));
    done.await.expect("an answer");
    let band = writer.submit(
        Origin::Mqtt,
        Command::EqSet {
            band: 1,
            param: "g".into(),
            value: 5.0,
            no_wait: false,
        },
        Options {
            mode: Mode::Latest,
            flat_to_eq: true,
        },
    );
    band.await.expect("an answer");
    settled(&writer).await;
    let log = log(&stand);
    assert_eq!(log.last().map(String::as_str), Some("mode eq"), "{log:?}");
}

/// A band that went out ahead of a mode write, because a newer band took the place of the one
/// waiting before it, is answered while that mode write is in flight: no switch follows it.
#[tokio::test]
async fn a_band_that_went_out_ahead_of_a_mode_write_in_flight_is_not_followed_by_a_switch() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    report_mode(&stand, EqMode::Flat);
    stand.gate.forget_permits(1000);
    let asks = Options {
        mode: Mode::Latest,
        flat_to_eq: true,
    };
    let band = |value: f64| Command::EqSet {
        band: 1,
        param: "g".into(),
        value,
        no_wait: false,
    };
    let first = writer.submit(Origin::Mqtt, band(1.0), asks);
    let second = writer.submit(Origin::Mqtt, band(2.0), asks);
    let flat = writer.submit(Origin::Mqtt, mode("flat"), Mode::Latest);
    let third = writer.submit(Origin::Mqtt, band(3.0), asks);
    stand.gate.add_permits(1000);
    first.await.expect("an answer");
    assert!(matches!(second.await, Ok(Outcome::Superseded)));
    third.await.expect("an answer");
    flat.await.expect("an answer");
    settled(&writer).await;
    assert_eq!(log(&stand), ["eq 1 g 1", "eq 1 g 3", "mode flat"]);
}

const BAND_KEYS: [&str; 6] = ["band1", "band2", "band3", "band4", "band5", "band6"];

/// A writer for a Loki Max behind a [`Stand`] whose six bands read `bands`, raw steps.
fn loki(bands: [i64; 6]) -> (Writer, Stand) {
    let readings: Vec<(&str, i64)> = BAND_KEYS.into_iter().zip(bands).collect();
    stand_in("lokimax", &readings)
}

/// The six bands the stand-in reads, raw steps; `None` while one is unread.
fn bands(stand: &Stand) -> Option<[i64; 6]> {
    let feed = stand.feed.borrow();
    let mut bands = [0; 6];
    for (band, raw) in (1..=6).zip(bands.iter_mut()) {
        *raw = reading_of(&feed.readings, FunctionId::three(6, band, 3))?;
    }
    Some(bands)
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

/// A built-in setting asks for every band going down in band order, then every band going up, as
/// one batch whose writes wait for their echoes.
#[tokio::test]
async fn a_builtin_setting_writes_its_bands_down_then_up_as_one_batch() {
    let (writer, stand) = loki([15, 15, 15, 20, 15, 15]);
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "bass boost".into());
    let Ok(Planned::Setting(applied)) = planned.await else {
        panic!("the setting is planned");
    };
    assert_eq!(applied.name, "Bass Boost");
    assert_eq!(applied.writes.len(), 4);
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(
        log(&stand),
        ["raw 6.4 15", "raw 6.1 28", "raw 6.2 21", "raw 6.3 17"]
    );
    assert_eq!(bands(&stand), Some([28, 21, 17, 15, 15, 15]));
}

/// A band that already reads its target is left out of the batch.
#[tokio::test]
async fn a_builtin_setting_skips_the_bands_already_on_their_values() {
    let (writer, stand) = loki([28, 21, 15, 15, 15, 15]);
    let Recalling { planned, done } = writer.apply(Origin::Web, "Bass Boost".into());
    assert!(matches!(planned.await, Ok(Planned::Setting(ref a)) if a.writes.len() == 1));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(log(&stand), ["raw 6.3 17"]);
}

/// A setting the device is already on plans no write, answers at once and sends nothing.
#[tokio::test]
async fn a_builtin_setting_already_on_answers_at_once_and_writes_nothing() {
    let (writer, stand) = loki([15; 6]);
    let Recalling { planned, done } = writer.apply(Origin::Socket, "Flat".into());
    assert!(matches!(planned.await, Ok(Planned::Setting(ref a)) if a.writes.is_empty()));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    assert!(log(&stand).is_empty());
}

/// A band write that arrives while a setting runs waits for it and goes out after its last
/// write, so the band ends on what was asked last.
#[tokio::test]
async fn a_band_write_that_arrives_while_a_setting_runs_is_held_until_it_ends() {
    let (writer, stand) = loki([15; 6]);
    stand.gate.forget_permits(1000);
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Bass Boost".into());
    let band = writer.submit(Origin::Socket, set("band5", "20"), Mode::Ordered);
    assert_eq!(
        writer.view().await.held,
        1,
        "the band waits for the setting"
    );
    stand.gate.add_permits(1000);
    assert!(matches!(planned.await, Ok(Planned::Setting(_))));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    assert!(matches!(band.await, Ok(Outcome::Done(Response::Value(_)))));
    assert_eq!(
        log(&stand),
        ["raw 6.1 28", "raw 6.2 21", "raw 6.3 17", "set band5 20"]
    );
}

/// A later setting replaces one that has not started, and drops the newest-wins band writes held
/// behind the running one that it rewrites; the one that ran first and the one asked last are
/// the only ones sent.
#[tokio::test]
async fn a_later_setting_replaces_one_not_started_and_drops_the_held_band_writes_it_rewrites() {
    let (writer, stand) = loki([15; 6]);
    stand.gate.forget_permits(1000);
    let first = writer.apply(Origin::Mcp, "Bass Boost".into());
    let held = writer.submit(Origin::Socket, set("band2", "3"), Mode::Latest);
    let second = writer.apply(Origin::Mqtt, "Smiley".into());
    let third = writer.apply(Origin::Web, "Treble Boost".into());
    let view = writer.view().await;
    assert_eq!(view.recall_waiting.as_deref(), Some("Treble Boost"));
    assert_eq!(view.held, 0, "the held write it rewrites is dropped");
    stand.gate.add_permits(1000);
    assert!(matches!(first.done.await, Ok(Outcome::Done(Response::Ack))));
    assert!(matches!(held.await, Ok(Outcome::Superseded)));
    assert!(
        second.planned.await.is_err(),
        "no plan for a replaced setting"
    );
    assert!(matches!(second.done.await, Ok(Outcome::Superseded)));
    assert!(matches!(third.planned.await, Ok(Planned::Setting(_))));
    assert!(matches!(third.done.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(
        log(&stand),
        [
            "raw 6.1 28",
            "raw 6.2 21",
            "raw 6.3 17",
            "raw 6.1 15",
            "raw 6.2 15",
            "raw 6.3 15",
            "raw 6.4 17",
            "raw 6.5 21",
            "raw 6.6 28",
        ]
    );
}

/// A write the device refuses ends the batch, which says how many writes went through.
#[tokio::test]
async fn a_setting_stops_at_the_first_refused_write_and_says_how_many_went() {
    let (writer, stand) = loki([15; 6]);
    stand
        .refuses
        .lock()
        .expect("the refusals")
        .push("6.2".into());
    let Recalling { done, .. } = writer.apply(Origin::Mcp, "Bass Boost".into());
    match done.await {
        Ok(Outcome::Done(Response::Error { code, message })) => {
            assert_eq!(code, 3);
            assert_eq!(message, "stopped after 1 of 3 writes: the device said no");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(log(&stand), ["raw 6.1 28", "raw 6.2 21"]);
}

/// A setting is no recall of the tone control's presets: the memory of the last one stays.
#[tokio::test]
async fn a_setting_does_not_change_the_recalled_preset() {
    let (writer, _stand) = loki([15; 6]);
    let recalled = writer.recalled();
    let Recalling { done, .. } = writer.apply(Origin::Mcp, "Smiley".into());
    done.await.expect("an answer");
    assert_eq!(*recalled.borrow(), None);
    writer.note_recalled("Warm".into());
    let Recalling { done, .. } = writer.apply(Origin::Mcp, "Flat".into());
    done.await.expect("an answer");
    assert_eq!(recalled.borrow().as_deref(), Some("Warm"));
}

/// A write to a band under its two-id form shares the band's function line: it waits behind a
/// write to the band's own id and is not sent beside it.
#[tokio::test]
async fn a_write_to_a_band_alias_waits_behind_a_write_to_the_band() {
    let profile = Arc::new(crate::test_support::profile_named("lokimax"));
    let (_feed, feed_rx) = watch::channel(Feed {
        name: "fake".into(),
        profile: Some(profile),
        connected: true,
        ..Feed::default()
    });
    let (commands, mut inbox) = mpsc::channel::<(Command, oneshot::Sender<Response>)>(8);
    let writer = Writer::spawn(commands, feed_rx);
    let band = writer.submit(Origin::Socket, set("band1", "20"), Mode::Ordered);
    let alias = writer.submit(Origin::Socket, raw_u8("6.1", "10"), Mode::Ordered);
    assert_eq!(writer.view().await.in_flight, [FunctionId::three(6, 1, 3)]);
    let (first, reply) = inbox.try_recv().expect("the band write is sent");
    assert_eq!(shown(&first), "set band1 20");
    assert!(inbox.try_recv().is_err(), "the alias waits behind the band");
    let _ = reply.send(Response::Ack);
    assert!(matches!(band.await, Ok(Outcome::Done(Response::Ack))));
    writer.view().await;
    let (second, reply) = inbox.try_recv().expect("the alias goes after the band");
    assert_eq!(shown(&second), "raw 6.1 10");
    let _ = reply.send(Response::Ack);
    assert!(matches!(alias.await, Ok(Outcome::Done(Response::Ack))));
}

/// A recall of a device slot, which makes the device set its bands, waits behind a running
/// setting and goes out after it.
#[tokio::test]
async fn a_recall_waits_behind_a_running_setting() {
    let (writer, stand) = loki([15; 6]);
    stand.gate.forget_permits(1000);
    let Recalling { done, .. } = writer.apply(Origin::Mcp, "Bass Boost".into());
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    assert_eq!(
        writer.view().await.held,
        1,
        "the recall waits for the setting"
    );
    stand.gate.add_permits(1000);
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(
        log(&stand),
        ["raw 6.1 28", "raw 6.2 21", "raw 6.3 17", "set recall 2"]
    );
}

/// Two recalls of different slots reach the device in the order they were asked for, each
/// answered without an echo.
#[tokio::test]
async fn two_recalls_go_out_in_the_order_asked() {
    let (writer, stand) = loki([15; 6]);
    stand.gate.forget_permits(1000);
    let first = writer.submit(Origin::Socket, set("recall", "2"), Mode::Latest);
    let second = writer.submit(Origin::Mqtt, set("recall", "1"), Mode::Latest);
    stand.gate.add_permits(1000);
    assert!(matches!(first.await, Ok(Outcome::Done(Response::Ack))));
    assert!(matches!(second.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(log(&stand), ["set recall 2", "set recall 1"]);
}

/// A device with no built-in settings refuses a setting as it starts, in the profile crate's
/// words with the device's own name, and sends nothing.
#[tokio::test]
async fn a_setting_on_a_device_without_builtin_settings_is_refused_in_words() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Flat".into());
    assert!(matches!(
        planned.await,
        Ok(Planned::Refused(ref why))
            if why == "fake has no device presets; svolder eq list lists its tone-control presets"
    ));
    assert!(matches!(
        done.await,
        Ok(Outcome::Done(Response::Error { code: 1, .. }))
    ));
    assert!(log(&stand).is_empty());
}

/// A save of a device slot waits behind a running setting like a recall does, so it never stores
/// the bands of a setting half applied.
#[tokio::test]
async fn a_save_waits_behind_a_running_setting() {
    let (writer, stand) = loki([15; 6]);
    stand.gate.forget_permits(1000);
    let Recalling { done, .. } = writer.apply(Origin::Mcp, "Bass Boost".into());
    let save = writer.submit(Origin::Socket, set("save", "1"), Mode::Ordered);
    assert_eq!(
        writer.view().await.held,
        1,
        "the save waits for the setting"
    );
    stand.gate.add_permits(1000);
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    assert!(matches!(save.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(
        log(&stand),
        ["raw 6.1 28", "raw 6.2 21", "raw 6.3 17", "set save 1"]
    );
}

/// Right after a recall of a device slot the bands the writer reads are the ones from before it,
/// so a setting planned then writes every band, as for a band never read, until a band report
/// comes in after the recall.
#[tokio::test]
async fn a_setting_right_after_a_slot_recall_writes_every_band() {
    let (writer, stand) = loki([15; 6]);
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    // The readings say Flat, and the device holds the slot's bands: Flat is not "already on".
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Flat".into());
    assert!(matches!(planned.await, Ok(Planned::Setting(ref a)) if a.writes.len() == 6));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    let mut want = vec!["set recall 2".to_string()];
    want.extend((1..=6).map(|band| format!("raw 6.{band} 15")));
    assert_eq!(log(&stand), want);
    // A band report after the recall brings the readings up to date: Flat is on.
    report_bands(&stand, [20, 15, 15, 15, 15, 15]);
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Flat".into());
    assert!(matches!(planned.await, Ok(Planned::Setting(ref a)) if a.writes.len() == 1));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(log(&stand).last().map(String::as_str), Some("raw 6.1 15"));
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

/// A recall of a device slot goes out only once no write to a band it changes is in flight, so
/// every echo from before the recall has come in by the time it goes.
#[tokio::test]
async fn a_recall_waits_for_a_band_write_in_flight_before_it_goes_out() {
    let profile = Arc::new(crate::test_support::profile_named("lokimax"));
    let (_feed, feed_rx) = watch::channel(Feed {
        name: "fake".into(),
        profile: Some(profile),
        connected: true,
        ..Feed::default()
    });
    let (commands, mut inbox) = mpsc::channel::<(Command, oneshot::Sender<Response>)>(8);
    let writer = Writer::spawn(commands, feed_rx);
    let band = writer.submit(Origin::Socket, set("band1", "20"), Mode::Latest);
    let alias = writer.submit(Origin::Socket, raw_u8("6.2", "5"), Mode::Latest);
    let recall = writer.submit(Origin::Mqtt, set("recall", "2"), Mode::Ordered);
    writer.view().await;
    let (first, first_reply) = inbox.try_recv().expect("the band write is sent");
    let (second, second_reply) = inbox.try_recv().expect("the alias write is sent");
    assert_eq!(
        [shown(&first), shown(&second)],
        ["set band1 20", "raw 6.2 5"]
    );
    assert!(inbox.try_recv().is_err(), "the recall waits for both");
    let _ = first_reply.send(Response::Ack);
    band.await.expect("an answer");
    writer.view().await;
    assert!(inbox.try_recv().is_err(), "one write is still in flight");
    let _ = second_reply.send(Response::Ack);
    alias.await.expect("an answer");
    writer.view().await;
    let (third, reply) = inbox.try_recv().expect("the recall goes after both");
    assert_eq!(shown(&third), "set recall 2");
    let _ = reply.send(Response::Ack);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
}

/// A recall also waits for a write to a control that its `changes` names and no built-in setting
/// sets: the profile here has no built-in settings at all.
#[tokio::test]
async fn a_recall_waits_for_a_write_to_a_control_only_its_changes_names() {
    let mut profile = crate::test_support::profile_named("lokimax");
    profile.builtin_settings = None;
    let (_feed, feed_rx) = watch::channel(Feed {
        name: "fake".into(),
        profile: Some(Arc::new(profile)),
        connected: true,
        ..Feed::default()
    });
    let (commands, mut inbox) = mpsc::channel::<(Command, oneshot::Sender<Response>)>(8);
    let writer = Writer::spawn(commands, feed_rx);
    let band = writer.submit(Origin::Socket, set("band1", "20"), Mode::Latest);
    let alias = writer.submit(Origin::Socket, raw_u8("6.2", "5"), Mode::Latest);
    let recall = writer.submit(Origin::Mqtt, set("recall", "2"), Mode::Ordered);
    writer.view().await;
    let (first, first_reply) = inbox.try_recv().expect("the band write is sent");
    let (second, second_reply) = inbox.try_recv().expect("the alias write is sent");
    assert_eq!(
        [shown(&first), shown(&second)],
        ["set band1 20", "raw 6.2 5"]
    );
    assert!(inbox.try_recv().is_err(), "the recall waits for both");
    let _ = first_reply.send(Response::Ack);
    band.await.expect("an answer");
    let _ = second_reply.send(Response::Ack);
    alias.await.expect("an answer");
    writer.view().await;
    let (third, reply) = inbox.try_recv().expect("the recall goes after both");
    assert_eq!(shown(&third), "set recall 2");
    let _ = reply.send(Response::Ack);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
}

/// The echo of a band write sent before a recall does not count as a report after it: a setting
/// asked right after still writes every band, the one the echo reported on its target too.
#[tokio::test]
async fn a_band_echo_from_before_a_recall_does_not_end_the_after_recall_rule() {
    let (writer, stand) = loki([15; 6]);
    stand.gate.forget_permits(1000);
    let band = writer.submit(Origin::Socket, set("band1", "28"), Mode::Latest);
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    stand.gate.add_permits(1000);
    band.await.expect("an answer");
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Bass Boost".into());
    assert!(matches!(planned.await, Ok(Planned::Setting(ref a)) if a.writes.len() == 6));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
}

/// After a recall each band is trusted again once it reports: a setting plans the bands that
/// have not reported as unread and the one that has from its reading.
#[tokio::test]
async fn after_a_recall_a_setting_trusts_only_the_bands_that_have_reported() {
    let (writer, stand) = loki([15; 6]);
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    // Band 1 reports the value Bass Boost wants; the others have not reported.
    let profile = stand_profile(&stand);
    let band1 = svolder_profiles::intent(&profile, "band1", "28").expect("a band");
    publish_all(
        &stand.feed,
        [Message::status(band1.id, band1.value)].into_iter(),
    );
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Bass Boost".into());
    assert!(matches!(planned.await, Ok(Planned::Setting(ref a)) if a.writes.len() == 5));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(
        log(&stand)[1..],
        [
            "raw 6.4 15",
            "raw 6.5 15",
            "raw 6.6 15",
            "raw 6.2 21",
            "raw 6.3 17"
        ]
    );
}

/// Let the writer and the stand-in take everything that is ready.
async fn quiet(writer: &Writer) {
    for _ in 0..10 {
        writer.view().await;
        tokio::task::yield_now().await;
    }
}

/// A band written without waiting is answered `Ack` before its echo: a recall of a device slot
/// still waits for the echo, so it goes out after every echo from before it.
#[tokio::test(start_paused = true)]
async fn a_recall_waits_for_the_echo_of_a_no_wait_band_write() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Latest)).await;
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    quiet(&writer).await;
    assert_eq!(
        log(&stand),
        ["set band1 28"],
        "the recall waits for the echo"
    );
    show(&stand);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(log(&stand), ["set band1 28", "set recall 2"]);
}

/// That echo reports the band as it was before the recall: a setting asked after the recall does
/// not take it for the band's reading now, and writes every band.
#[tokio::test(start_paused = true)]
async fn a_no_wait_band_echo_from_before_a_recall_is_not_trusted_after_it() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Latest)).await;
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    quiet(&writer).await;
    show(&stand);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Bass Boost".into());
    assert!(matches!(planned.await, Ok(Planned::Setting(ref a)) if a.writes.len() == 6));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
}

/// An echo that never comes holds a recall back only for the set timeout, and an echo that comes
/// after it is not taken for a report after the recall either.
#[tokio::test(start_paused = true)]
async fn a_recall_behind_a_no_wait_write_whose_echo_never_comes_goes_out_after_the_deadline() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Latest)).await;
    let began = tokio::time::Instant::now();
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(began.elapsed(), Duration::from_secs(2), "the set timeout");
    assert_eq!(log(&stand), ["set band1 28", "set recall 2"]);
    show(&stand);
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Bass Boost".into());
    assert!(matches!(planned.await, Ok(Planned::Setting(ref a)) if a.writes.len() == 6));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
}

/// Show the echo the stand-in queued first, and no other.
fn show_first(stand: &Stand) {
    let first = {
        let mut unshown = stand.unshown.lock().expect("the unshown");
        (!unshown.is_empty()).then(|| unshown.remove(0))
    };
    publish_all(&stand.feed, first.into_iter());
}

/// A band written without waiting does not hold the next write to the band for its echo: the two
/// go out back to back, whatever the device echoes.
#[tokio::test(start_paused = true)]
async fn a_no_wait_band_write_no_longer_holds_the_next_band_write_for_its_echo() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    let began = tokio::time::Instant::now();
    let first = writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Ordered);
    let second = writer.submit(Origin::Socket, no_wait("band1", "20"), Mode::Ordered);
    acked(first).await;
    acked(second).await;
    assert_eq!(log(&stand), ["set band1 28", "set band1 20"]);
    assert_eq!(began.elapsed(), Duration::ZERO, "no echo was waited for");
    let other = writer.submit(Origin::Socket, no_wait("band2", "10"), Mode::Ordered);
    acked(other).await;
    assert_eq!(log(&stand)[2], "set band2 10");
    assert_eq!(began.elapsed(), Duration::ZERO);
}

/// How many writes the built-in setting Bass Boost plans now: every band it is not sure of is
/// written, the ones that have reported since the last recall are planned from their reading.
async fn setting_writes(writer: &Writer) -> usize {
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Bass Boost".into());
    let writes = match planned.await {
        Ok(Planned::Setting(applied)) => applied.writes.len(),
        other => panic!("no setting plan: {:?}", other.is_ok()),
    };
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    writes
}

/// The raw-step status of `key` at `value`, as a late echo shows it.
fn status_of(stand: &Stand, key: &str, value: &str) -> Message {
    let profile = stand_profile(stand);
    let m = svolder_profiles::intent(&profile, key, value).expect("a band");
    Message::status(m.id, m.value)
}

/// A write that is no recall goes out behind a band's pending echo, and the recall behind it still
/// waits for that echo.
#[tokio::test(start_paused = true)]
async fn a_recall_queued_behind_a_band_write_waits_for_its_echo_while_other_writes_go() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Ordered)).await;
    acked(writer.submit(Origin::Socket, no_wait("band2", "10"), Mode::Ordered)).await;
    assert_eq!(
        log(&stand),
        ["set band1 28", "set band2 10"],
        "the second band goes while the first echo is awaited"
    );
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    quiet(&writer).await;
    assert_eq!(log(&stand).len(), 2, "the recall waits");
    show(&stand);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(log(&stand)[2], "set recall 2");
}

/// A band written again while an earlier write's echo is awaited cannot tell the first echo from
/// the second: the first echo does not let a recall go, the deadline of the latest write does,
/// and the band is then unreported after the recall, whatever the late echo shows.
#[tokio::test(start_paused = true)]
async fn a_band_written_twice_without_waiting_releases_a_recall_only_at_the_latest_deadline() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    let began = tokio::time::Instant::now();
    acked(writer.submit(Origin::Socket, no_wait("band1", "20"), Mode::Ordered)).await;
    tokio::time::advance(Duration::from_secs(1)).await;
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Ordered)).await;
    show_first(&stand);
    quiet(&writer).await;
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    quiet(&writer).await;
    assert_eq!(
        log(&stand),
        ["set band1 20", "set band1 28"],
        "the first echo does not let the recall go"
    );
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(
        began.elapsed(),
        Duration::from_secs(3),
        "the latest write's deadline"
    );
    // The second write's echo, late: the target value, which a report after the recall would be.
    show(&stand);
    quiet(&writer).await;
    assert_eq!(setting_writes(&writer).await, 6);
}

/// A band released at its deadline stays unreported after the recall even when another band's
/// echo comes between the release and the recall.
#[tokio::test(start_paused = true)]
async fn a_repeated_band_released_at_its_deadline_stays_unsure_after_another_bands_echo() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    acked(writer.submit(Origin::Socket, no_wait("band1", "20"), Mode::Ordered)).await;
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Ordered)).await;
    show_first(&stand);
    tokio::time::advance(Duration::from_secs(1)).await;
    acked(writer.submit(Origin::Socket, no_wait("band2", "10"), Mode::Ordered)).await;
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    quiet(&writer).await;
    // Band 1's deadline passes; band 2's is a second later.
    tokio::time::advance(Duration::from_millis(1100)).await;
    quiet(&writer).await;
    assert_eq!(log(&stand).len(), 3, "the recall still waits for band 2");
    let band2 = stand.unshown.lock().expect("the unshown").remove(1);
    publish_all(&stand.feed, std::iter::once(band2));
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    show(&stand);
    quiet(&writer).await;
    assert_eq!(setting_writes(&writer).await, 6, "band 1 is unread");
}

/// A band written twice whose echoes both come after the deadline is unreported after the next
/// recall: the first late echo, coming before the recall goes out, does not end the mark, as the
/// second echo cannot be told from a report after the recall.
#[tokio::test(start_paused = true)]
async fn a_band_written_twice_with_both_echoes_late_stays_unreported_after_the_recall() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    acked(writer.submit(Origin::Socket, no_wait("band1", "20"), Mode::Ordered)).await;
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Ordered)).await;
    tokio::time::advance(Duration::from_secs(1)).await;
    acked(writer.submit(Origin::Socket, no_wait("band2", "10"), Mode::Ordered)).await;
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    quiet(&writer).await;
    // Band 1's deadline passes; band 2's is a second later, and the recall waits for it.
    tokio::time::advance(Duration::from_millis(1100)).await;
    quiet(&writer).await;
    assert_eq!(log(&stand).len(), 3, "the recall still waits for band 2");
    let first = stand.unshown.lock().expect("the unshown").remove(0);
    publish_all(&stand.feed, std::iter::once(first));
    quiet(&writer).await;
    let band2 = stand.unshown.lock().expect("the unshown").remove(1);
    publish_all(&stand.feed, std::iter::once(band2));
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    show(&stand);
    quiet(&writer).await;
    assert_eq!(setting_writes(&writer).await, 6, "band 1 is unread");
}

/// A refused waited write to a band whose no-wait write is awaited has no echo to come: it does not
/// extend the wait, and the first write's late echo after the recall is not trusted.
#[tokio::test(start_paused = true)]
async fn a_refused_waited_write_behind_an_echoing_band_does_not_extend_the_wait() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    stand
        .refuses
        .lock()
        .expect("the refusals")
        .push("6.1".into());
    let began = tokio::time::Instant::now();
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Ordered)).await;
    let refused = writer.submit(Origin::Socket, raw_u8("6.1", "20"), Mode::Ordered);
    assert!(matches!(
        refused.await,
        Ok(Outcome::Done(Response::Error { .. }))
    ));
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(
        began.elapsed(),
        Duration::from_secs(2),
        "the first deadline"
    );
    show(&stand);
    quiet(&writer).await;
    stand.refuses.lock().expect("the refusals").clear();
    assert_eq!(setting_writes(&writer).await, 6, "band 1 is unread");
}

/// The same when the first write's echo comes before the deadline, after the refusal: the band is
/// still awaited until the deadline, and is unreported after the recall.
#[tokio::test(start_paused = true)]
async fn a_refused_waited_write_then_the_echo_before_the_deadline_still_waits_for_it() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    stand
        .refuses
        .lock()
        .expect("the refusals")
        .push("6.1".into());
    let began = tokio::time::Instant::now();
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Ordered)).await;
    let refused = writer.submit(Origin::Socket, raw_u8("6.1", "20"), Mode::Ordered);
    assert!(matches!(
        refused.await,
        Ok(Outcome::Done(Response::Error { .. }))
    ));
    show(&stand);
    quiet(&writer).await;
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(
        began.elapsed(),
        Duration::from_secs(2),
        "the first deadline"
    );
    quiet(&writer).await;
    stand.refuses.lock().expect("the refusals").clear();
    assert_eq!(setting_writes(&writer).await, 6, "band 1 is unread");
}

/// A band marked unreported until the next recall stays so when a later write to it also misses
/// its deadline: the later mark never replaces the stronger one.
#[tokio::test(start_paused = true)]
async fn a_recall_mark_is_not_downgraded_by_a_later_write_that_misses_its_deadline() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    acked(writer.submit(Origin::Socket, no_wait("band1", "20"), Mode::Ordered)).await;
    acked(writer.submit(Origin::Socket, no_wait("band1", "15"), Mode::Ordered)).await;
    tokio::time::advance(Duration::from_millis(2100)).await;
    quiet(&writer).await;
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Ordered)).await;
    tokio::time::advance(Duration::from_secs(1)).await;
    acked(writer.submit(Origin::Socket, no_wait("band2", "10"), Mode::Ordered)).await;
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    quiet(&writer).await;
    // Band 1's last deadline passes; band 2's is a second later, and the recall waits for it.
    tokio::time::advance(Duration::from_millis(1100)).await;
    quiet(&writer).await;
    assert_eq!(log(&stand).len(), 4, "the recall still waits for band 2");
    let first = stand.unshown.lock().expect("the unshown").remove(0);
    publish_all(&stand.feed, std::iter::once(first));
    quiet(&writer).await;
    let band2 = stand.unshown.lock().expect("the unshown").remove(2);
    publish_all(&stand.feed, std::iter::once(band2));
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    show(&stand);
    quiet(&writer).await;
    assert_eq!(setting_writes(&writer).await, 6, "band 1 is unread");
}

/// A band with that mark, written once more and echoed in time, is still awaited until the
/// deadline and unreported after the recall: the echoes that came before cannot be told from it.
#[tokio::test(start_paused = true)]
async fn a_band_marked_until_the_next_recall_written_once_more_is_still_unreported() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    acked(writer.submit(Origin::Socket, no_wait("band1", "20"), Mode::Ordered)).await;
    acked(writer.submit(Origin::Socket, no_wait("band1", "15"), Mode::Ordered)).await;
    tokio::time::advance(Duration::from_millis(2100)).await;
    quiet(&writer).await;
    show(&stand);
    quiet(&writer).await;
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Ordered)).await;
    show(&stand);
    quiet(&writer).await;
    let began = tokio::time::Instant::now();
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(began.elapsed(), Duration::from_secs(2), "the deadline");
    show(&stand);
    quiet(&writer).await;
    assert_eq!(setting_writes(&writer).await, 6, "band 1 is unread");
}

/// A band whose echo missed its deadline still has one outstanding: a band written again then
/// cannot tell the first write's late echo from its own, so it is awaited until its deadline, and
/// its own echo, coming after the recall, is not taken for a report.
#[tokio::test(start_paused = true)]
async fn a_band_written_after_a_missed_deadline_is_not_released_by_the_late_echo() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    acked(writer.submit(Origin::Socket, no_wait("band1", "20"), Mode::Ordered)).await;
    tokio::time::advance(Duration::from_millis(2100)).await;
    quiet(&writer).await;
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Ordered)).await;
    let began = tokio::time::Instant::now();
    show_first(&stand);
    quiet(&writer).await;
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    quiet(&writer).await;
    assert_eq!(
        log(&stand).len(),
        2,
        "the late echo does not let the recall go"
    );
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(
        began.elapsed(),
        Duration::from_secs(2),
        "the latest deadline"
    );
    show(&stand);
    quiet(&writer).await;
    assert_eq!(setting_writes(&writer).await, 6, "band 1 is unread");
}

/// A waited write to a band whose echo is awaited does not shorten the wait: its answer may be the
/// earlier echo, so a recall still goes out only a set timeout after it, and the band is
/// unreported after the recall.
#[tokio::test(start_paused = true)]
async fn a_waited_write_to_a_band_awaiting_an_echo_keeps_a_recall_until_the_deadline() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    let began = tokio::time::Instant::now();
    acked(writer.submit(Origin::Socket, no_wait("band1", "20"), Mode::Ordered)).await;
    tokio::time::advance(Duration::from_secs(1)).await;
    let waited = writer.submit(Origin::Socket, set("band1", "28"), Mode::Ordered);
    assert!(matches!(
        waited.await,
        Ok(Outcome::Done(Response::Value(_)))
    ));
    quiet(&writer).await;
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(
        began.elapsed(),
        Duration::from_secs(3),
        "a timeout after it"
    );
    // A late echo of the band, the target value: no report after the recall.
    publish_all(
        &stand.feed,
        std::iter::once(status_of(&stand, "band1", "28")),
    );
    quiet(&writer).await;
    assert_eq!(setting_writes(&writer).await, 6);
}

/// A waited write sent just before the earlier write's deadline and answered after it, by the
/// earlier write's late echo, still has its own echo to come: a recall waits a set timeout after
/// the answer, and the band is unreported after it.
#[tokio::test(start_paused = true)]
async fn a_waited_write_answered_after_the_earlier_deadline_still_awaits_its_own_echo() {
    let profile = Arc::new(crate::test_support::profile_named("lokimax"));
    let (feed, feed_rx) = watch::channel(Feed {
        name: "fake".into(),
        profile: Some(Arc::clone(&profile)),
        connected: true,
        ..Feed::default()
    });
    let (commands, mut inbox) = mpsc::channel::<(Command, oneshot::Sender<Response>)>(8);
    let writer = Writer::spawn(commands, feed_rx);
    let first = writer.submit(Origin::Socket, no_wait("band1", "20"), Mode::Ordered);
    writer.view().await;
    let (_, reply) = inbox.try_recv().expect("the first write");
    let _ = reply.send(Response::Ack);
    first.await.expect("answered");
    tokio::time::advance(Duration::from_millis(1900)).await;
    let waited = writer.submit(Origin::Socket, set("band1", "28"), Mode::Ordered);
    writer.view().await;
    let (_, waited_reply) = inbox.try_recv().expect("the waited write goes at once");
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    writer.view().await;
    // The first write's deadline passes with the waited write in flight; its echo then comes
    // and answers the waited write.
    tokio::time::advance(Duration::from_millis(200)).await;
    quiet(&writer).await;
    assert!(inbox.try_recv().is_err(), "the waited write is in flight");
    let band1 = |value: &str| {
        let m = svolder_profiles::intent(&profile, "band1", value).expect("a band");
        Message::status(m.id, m.value)
    };
    publish_all(&feed, std::iter::once(band1("20")));
    quiet(&writer).await;
    let status = band1("20");
    let reading = svolder_profiles::describe(&profile, &status);
    let _ = waited_reply.send(Response::Value(ValueObject::from_reading(
        &reading, &status,
    )));
    waited.await.expect("answered");
    quiet(&writer).await;
    let answered_at = tokio::time::Instant::now();
    assert!(
        inbox.try_recv().is_err(),
        "the recall waits for the second echo"
    );
    tokio::time::advance(Duration::from_millis(2100)).await;
    quiet(&writer).await;
    let (sent, reply) = inbox
        .try_recv()
        .expect("the recall goes after the deadline");
    assert_eq!(shown(&sent), "set recall 2");
    assert!(answered_at.elapsed() >= Duration::from_secs(2));
    let _ = reply.send(Response::Ack);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    // The second write's echo, late: the target value.
    publish_all(&feed, std::iter::once(band1("28")));
    quiet(&writer).await;
    let Recalling { planned, .. } = writer.apply(Origin::Mcp, "Bass Boost".into());
    assert!(matches!(planned.await, Ok(Planned::Setting(ref a)) if a.writes.len() == 6));
}

/// A waited band write answered with the session's timeout was waited for as long as an awaited
/// echo: a recall goes out at once, and the write's echo, coming after it, is not taken for a
/// report after the recall.
#[tokio::test(start_paused = true)]
async fn a_waited_band_write_that_timed_out_is_unreported_after_the_next_recall() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    stand
        .times_out
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let waited = writer.submit(Origin::Socket, set("band1", "28"), Mode::Ordered);
    let answer = waited.await;
    assert!(
        matches!(&answer, Ok(Outcome::Done(response)) if timed_out(response)),
        "{answer:?}"
    );
    let began = tokio::time::Instant::now();
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(
        began.elapsed(),
        Duration::ZERO,
        "the session already waited"
    );
    // The write's echo, late: the target value, which a report after the recall would be.
    publish_all(
        &stand.feed,
        std::iter::once(status_of(&stand, "band1", "28")),
    );
    quiet(&writer).await;
    assert_eq!(setting_writes(&writer).await, 6, "band 1 is unread");
}

/// The commands a writer hands its actor, each with where the answer goes.
type Inbox = mpsc::Receiver<(Command, oneshot::Sender<Response>)>;

/// A writer for a Loki Max whose actor is the test itself, which takes each command from the
/// returned inbox and answers it by hand; its feed starts with no readings.
fn by_hand() -> (Writer, watch::Sender<Feed>, Inbox, Arc<Profile>) {
    let profile = Arc::new(crate::test_support::profile_named("lokimax"));
    let (feed, feed_rx) = watch::channel(Feed {
        name: "fake".into(),
        profile: Some(Arc::clone(&profile)),
        connected: true,
        ..Feed::default()
    });
    let (commands, inbox) = mpsc::channel::<(Command, oneshot::Sender<Response>)>(8);
    (Writer::spawn(commands, feed_rx), feed, inbox, profile)
}

/// The device shows `status` and answers `reply` with it, as the actor does with a waited write.
fn echo_with(
    profile: &Profile,
    feed: &watch::Sender<Feed>,
    status: Message,
    reply: oneshot::Sender<Response>,
) {
    publish_all(feed, std::iter::once(status.clone()));
    let reading = svolder_profiles::describe(profile, &status);
    let _ = reply.send(Response::Value(ValueObject::from_reading(
        &reading, &status,
    )));
}

/// The status a raw step of a built-in setting shows, read as the band it writes.
fn status_of_raw(profile: &Profile, command: &Command) -> Message {
    let Command::Raw { id, value, .. } = command else {
        panic!("a setting writes raw steps: {}", shown(command));
    };
    let id: FunctionId = id.parse().expect("an id");
    Message::status(
        profile.read_as(id),
        Value::U8(value.parse().expect("a raw step")),
    )
}

/// A built-in setting's step to a band whose echo missed its deadline counts as a second write to
/// it: the answer the step gets may be the late echo, so a recall after the setting waits a set
/// timeout, and the step's own echo, coming after the recall, is not taken for a report.
#[tokio::test(start_paused = true)]
async fn a_setting_step_to_a_band_whose_echo_is_late_is_unreported_after_the_next_recall() {
    let (writer, feed, mut inbox, profile) = by_hand();
    let band1 = |value: &str| {
        let m = svolder_profiles::intent(&profile, "band1", value).expect("a band");
        Message::status(m.id, m.value)
    };
    let first = writer.submit(Origin::Socket, no_wait("band1", "20"), Mode::Ordered);
    writer.view().await;
    let (_, reply) = inbox.try_recv().expect("the no-wait write");
    let _ = reply.send(Response::Ack);
    first.await.expect("answered");
    // Its echo misses the deadline: band 1 is unsure until the feed hears it.
    tokio::time::advance(Duration::from_millis(2100)).await;
    quiet(&writer).await;
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Bass Boost".into());
    let Ok(Planned::Setting(applied)) = planned.await else {
        panic!("the setting is planned");
    };
    for _ in 0..applied.writes.len() {
        quiet(&writer).await;
        let (command, reply) = inbox.try_recv().expect("the setting's next step");
        match shown(&command).as_str() {
            // The first write's late echo comes now, and answers the step.
            "raw 6.1 28" => echo_with(&profile, &feed, band1("20"), reply),
            _ => echo_with(&profile, &feed, status_of_raw(&profile, &command), reply),
        }
    }
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    quiet(&writer).await;
    assert!(
        inbox.try_recv().is_err(),
        "the recall waits for the step's own echo"
    );
    tokio::time::advance(Duration::from_millis(2100)).await;
    quiet(&writer).await;
    let (sent, reply) = inbox
        .try_recv()
        .expect("the recall goes after the deadline");
    assert_eq!(shown(&sent), "set recall 2");
    let _ = reply.send(Response::Ack);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    // The step's own echo, late: the target value.
    publish_all(&feed, std::iter::once(band1("28")));
    quiet(&writer).await;
    let Recalling { planned, .. } = writer.apply(Origin::Mcp, "Bass Boost".into());
    assert!(matches!(planned.await, Ok(Planned::Setting(ref a)) if a.writes.len() == 6));
}

/// A built-in setting's step answered with the session's timeout leaves its band unreported after
/// the next recall, as a waited write that timed out does.
#[tokio::test(start_paused = true)]
async fn a_setting_step_that_timed_out_is_unreported_after_the_next_recall() {
    let (writer, feed, mut inbox, profile) = by_hand();
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Bass Boost".into());
    let Ok(Planned::Setting(applied)) = planned.await else {
        panic!("the setting is planned");
    };
    for _ in 0..applied.writes.len() {
        quiet(&writer).await;
        let (command, reply) = inbox.try_recv().expect("the setting's next step");
        if shown(&command) == "raw 6.1 28" {
            let _ = reply.send(crate::device::error_for(&svolder_ble::BleError::Timeout(
                Duration::from_secs(2),
            )));
            break;
        }
        echo_with(&profile, &feed, status_of_raw(&profile, &command), reply);
    }
    assert!(matches!(
        done.await,
        Ok(Outcome::Done(Response::Error { code: 4, .. }))
    ));
    let recall = writer.submit(Origin::Socket, set("recall", "2"), Mode::Ordered);
    quiet(&writer).await;
    let (sent, reply) = inbox.try_recv().expect("the recall goes at once");
    assert_eq!(shown(&sent), "set recall 2");
    let _ = reply.send(Response::Ack);
    assert!(matches!(recall.await, Ok(Outcome::Done(Response::Ack))));
    // The step's echo, late: the target value.
    let m = svolder_profiles::intent(&profile, "band1", "28").expect("a band");
    publish_all(&feed, std::iter::once(Message::status(m.id, m.value)));
    quiet(&writer).await;
    let Recalling { planned, .. } = writer.apply(Origin::Mcp, "Bass Boost".into());
    assert!(matches!(planned.await, Ok(Planned::Setting(ref a)) if a.writes.len() == 6));
}

/// A save stores the bands as they stand and waits for no echo: it goes out behind a band written
/// without waiting at once.
#[tokio::test(start_paused = true)]
async fn a_save_does_not_wait_for_the_echo_of_a_no_wait_band_write() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Ordered)).await;
    let began = tokio::time::Instant::now();
    let save = writer.submit(Origin::Socket, set("save", "1"), Mode::Ordered);
    assert!(matches!(save.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(log(&stand), ["set band1 28", "set save 1"]);
    assert_eq!(began.elapsed(), Duration::ZERO);
}

/// A built-in setting asked while a band's echo is awaited is planned after the echo, from the
/// band's reading then.
#[tokio::test(start_paused = true)]
async fn a_setting_is_planned_after_the_echo_of_a_no_wait_band_write() {
    let (writer, stand) = stand_in_acking("lokimax", &[]);
    acked(writer.submit(Origin::Socket, no_wait("band1", "28"), Mode::Ordered)).await;
    let Recalling { planned, done } = writer.apply(Origin::Mcp, "Bass Boost".into());
    quiet(&writer).await;
    assert_eq!(log(&stand), ["set band1 28"], "the setting waits");
    show(&stand);
    assert!(matches!(planned.await, Ok(Planned::Setting(_))));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
}

/// Whether `answered` is how the device answers a write of `key`: the band's echo for a band, `Ack`
/// for a save or a recall.
fn answered_as(key: &str, answered: &Result<Outcome, oneshot::error::RecvError>) -> bool {
    match key {
        "band1" => matches!(answered, Ok(Outcome::Done(Response::Value(_)))),
        _ => matches!(answered, Ok(Outcome::Done(Response::Ack))),
    }
}

/// A write to a band, a save and a write to the same band, each asked for after the one before it
/// is answered as a socket client does, reach the actor in that order. Asked back to back they keep
/// the order too.
#[tokio::test]
async fn a_save_goes_out_between_the_band_writes_asked_before_and_after_it() {
    let (writer, stand) = loki([15; 6]);
    for (key, value) in [("band1", "20"), ("save", "1"), ("band1", "10")] {
        let answer = writer.submit(Origin::Socket, set(key, value), Mode::Ordered);
        assert!(answered_as(key, &answer.await), "{key}");
    }
    assert_eq!(log(&stand), ["set band1 20", "set save 1", "set band1 10"]);

    let (writer, stand) = loki([15; 6]);
    stand.gate.forget_permits(1000);
    let answers = [("band1", "20"), ("save", "1"), ("band1", "10")]
        .map(|(key, value)| writer.submit(Origin::Socket, set(key, value), Mode::Ordered));
    stand.gate.add_permits(1000);
    for (answer, key) in answers.into_iter().zip(["band1", "save", "band1"]) {
        assert!(answered_as(key, &answer.await), "{key}");
    }
    assert_eq!(log(&stand), ["set band1 20", "set save 1", "set band1 10"]);
}

/// The same for a recall: it reaches the device after the band written before it and before the
/// band written after it.
#[tokio::test]
async fn a_recall_goes_out_between_the_band_writes_asked_before_and_after_it() {
    let (writer, stand) = loki([15; 6]);
    for (key, value) in [("band1", "20"), ("recall", "1"), ("band1", "10")] {
        let answer = writer.submit(Origin::Socket, set(key, value), Mode::Ordered);
        assert!(answered_as(key, &answer.await), "{key}");
    }
    assert_eq!(
        log(&stand),
        ["set band1 20", "set recall 1", "set band1 10"]
    );

    let (writer, stand) = loki([15; 6]);
    stand.gate.forget_permits(1000);
    let answers = [("band1", "20"), ("recall", "1"), ("band1", "10")]
        .map(|(key, value)| writer.submit(Origin::Socket, set(key, value), Mode::Ordered));
    stand.gate.add_permits(1000);
    for (answer, key) in answers.into_iter().zip(["band1", "recall", "band1"]) {
        assert!(answered_as(key, &answer.await), "{key}");
    }
    assert_eq!(
        log(&stand),
        ["set band1 20", "set recall 1", "set band1 10"]
    );
}
