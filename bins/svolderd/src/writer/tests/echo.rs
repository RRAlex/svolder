// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

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
        Ok(Outcome::Done(Response::Error {
            code: CODE_TIMEOUT,
            ..
        }))
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
