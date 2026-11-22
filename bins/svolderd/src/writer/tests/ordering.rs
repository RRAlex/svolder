// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

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
            Ok(Outcome::Done(Response::Error { code: CODE_LINK, ref message, .. }))
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

/// A set that is only wanted when the device does not read it already: one that reads it and has
/// nothing in the line sends nothing and answers at once.
#[tokio::test]
async fn a_set_if_needed_sends_nothing_for_what_the_device_reads() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("input", 1)]);
    let answer = writer.set_if_needed(Origin::Web, "input".into(), 1);
    assert!(matches!(answer.await, Ok(Outcome::Done(Response::Ack))));
    assert!(log(&stand).is_empty(), "{:?}", log(&stand));
    let answer = writer.set_if_needed(Origin::Web, "input".into(), 2);
    assert!(matches!(
        answer.await,
        Ok(Outcome::Done(Response::Value(_)))
    ));
    assert_eq!(log(&stand), ["set input 2"]);
}

/// A write for the control that arrived first and has not landed makes the set go out behind it
/// although the device reads the step now: the later ask wins, as the arrival order says.
#[tokio::test]
async fn a_set_if_needed_goes_out_behind_a_write_in_the_line() {
    let (writer, stand) = stand_in("gungnir2-v2", &[("input", 1)]);
    stand.gate.forget_permits(1000);
    let first = writer.submit(Origin::Socket, set("input", "2"), Mode::Ordered);
    let later = writer.set_if_needed(Origin::Web, "input".into(), 1);
    let mut arrivals = writer.arrivals();
    arrivals
        .wait_for(|taken| *taken >= 2)
        .await
        .expect("both asks arrived");
    stand.gate.add_permits(1000);
    assert!(matches!(first.await, Ok(Outcome::Done(_))));
    assert!(matches!(later.await, Ok(Outcome::Done(Response::Value(_)))));
    assert_eq!(log(&stand), ["set input 2", "set input 1"]);
}

/// A toggle is reported as a boolean: the writer reads it as the step 0 or 1, as a scene does, so
/// a toggle already on the step is not written again.
#[tokio::test]
async fn a_set_if_needed_reads_a_toggle_reported_as_a_boolean() {
    let (writer, stand) = stand_in("bifrost3", &[("phase", 1)]);
    let answer = writer.set_if_needed(Origin::Web, "phase".into(), 1);
    assert!(matches!(answer.await, Ok(Outcome::Done(Response::Ack))));
    assert!(log(&stand).is_empty(), "{:?}", log(&stand));
    let answer = writer.set_if_needed(Origin::Web, "phase".into(), 0);
    assert!(matches!(
        answer.await,
        Ok(Outcome::Done(Response::Value(_)))
    ));
    assert_eq!(log(&stand), ["set phase 0"]);
}

/// A write sent without waiting is answered at its `Ack`, before the feed has heard the control
/// again: until it has (or the echo wait passes) a set that wants the step the feed still reads
/// goes out behind it.
#[tokio::test]
async fn a_set_if_needed_goes_out_while_a_no_wait_echo_is_unheard() {
    let (writer, stand) = stand_in_acking("gungnir2-v2", &[("input", 1)]);
    let sent = writer.submit(Origin::Socket, no_wait("input", "2"), Mode::Ordered);
    assert!(matches!(sent.await, Ok(Outcome::Done(Response::Ack))));
    let answer = writer.set_if_needed(Origin::Web, "input".into(), 1);
    assert!(matches!(
        answer.await,
        Ok(Outcome::Done(Response::Value(_)))
    ));
    assert_eq!(log(&stand), ["set input 2", "set input 1"]);
}
