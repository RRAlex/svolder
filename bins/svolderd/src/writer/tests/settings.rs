// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// The six bands the stand-in reads, raw steps; `None` while one is unread.
fn bands(stand: &Stand) -> Option<[i64; 6]> {
    let feed = stand.feed.borrow();
    let mut bands = [0; 6];
    for (band, raw) in (1..=6).zip(bands.iter_mut()) {
        *raw = reading_of(&feed.readings, FunctionId::three(6, band, 3))?;
    }
    Some(bands)
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
        Ok(Outcome::Done(Response::Error { code, message, .. })) => {
            assert_eq!(code, CODE_LINK);
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
        Ok(Outcome::Done(Response::Error {
            code: CODE_USAGE,
            ..
        }))
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

/// A set of values asks for the bands going down first, then those going up, as one batch, the
/// order the named form uses.
#[tokio::test]
async fn a_set_of_values_writes_the_cuts_first_as_one_batch() {
    let (writer, stand) = loki([15, 15, 15, 20, 15, 15]);
    let values = vec![("band1".to_string(), 28), ("band4".to_string(), 15)];
    let Recalling { planned, done } = writer.apply_values(Origin::Web, "Late night".into(), values);
    let Ok(Planned::Setting(applied)) = planned.await else {
        panic!("the values are planned");
    };
    assert_eq!(applied.name, "Late night");
    assert_eq!(applied.writes.len(), 2);
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    assert_eq!(log(&stand), ["raw 6.4 15", "raw 6.1 28"]);
    assert_eq!(bands(&stand), Some([28, 15, 15, 15, 15, 15]));
}

/// Values the bands already read, and a key the device has no band for, plan no write.
#[tokio::test]
async fn a_set_of_values_leaves_out_what_the_device_already_reads() {
    let (writer, stand) = loki([28, 21, 15, 15, 15, 15]);
    let values = vec![
        ("band1".to_string(), 28),
        ("band2".to_string(), 21),
        ("nonesuch".to_string(), 3),
    ];
    let Recalling { planned, done } = writer.apply_values(Origin::Web, "Same".into(), values);
    assert!(matches!(planned.await, Ok(Planned::Setting(ref a)) if a.writes.is_empty()));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    assert!(log(&stand).is_empty());
}

/// A band write that arrives while a set of values runs waits for it, as it waits for a named
/// setting.
#[tokio::test]
async fn a_band_write_arriving_during_a_set_of_values_waits_for_it() {
    let (writer, stand) = loki([15; 6]);
    stand.gate.forget_permits(1000);
    let values = vec![("band1".to_string(), 28), ("band2".to_string(), 21)];
    let Recalling { planned, done } = writer.apply_values(Origin::Web, "Late night".into(), values);
    let band = writer.submit(Origin::Socket, set("band2", "3"), Mode::Ordered);
    assert_eq!(writer.view().await.held, 1);
    stand.gate.add_permits(1000);
    assert!(matches!(planned.await, Ok(Planned::Setting(_))));
    assert!(matches!(done.await, Ok(Outcome::Done(Response::Ack))));
    assert!(matches!(band.await, Ok(Outcome::Done(Response::Value(_)))));
    assert_eq!(log(&stand), ["raw 6.1 28", "raw 6.2 21", "set band2 3"]);
}

/// A device with no built-in settings refuses a set of values in words, and writes nothing.
#[tokio::test]
async fn a_set_of_values_on_a_device_with_no_built_in_settings_is_refused() {
    let (writer, stand) = stand_in("gungnir2-v2", &[]);
    let values = vec![("band1".to_string(), 28)];
    let Recalling { planned, done } = writer.apply_values(Origin::Web, "Late night".into(), values);
    assert!(matches!(planned.await, Ok(Planned::Refused(_))));
    assert!(matches!(
        done.await,
        Ok(Outcome::Done(Response::Error { .. }))
    ));
    assert!(log(&stand).is_empty());
}

/// A value outside the band's range is refused in words and nothing is written, whoever asks.
#[tokio::test]
async fn a_set_of_values_refuses_a_step_outside_the_range() {
    let (writer, stand) = loki([15; 6]);
    let values = vec![("band1".to_string(), 28), ("band2".to_string(), 400)];
    let Recalling { planned, done } = writer.apply_values(Origin::Web, "Late night".into(), values);
    let Ok(Planned::Refused(why)) = planned.await else {
        panic!("refused");
    };
    assert!(
        why.contains("band2") && why.contains("outside its range"),
        "{why}"
    );
    assert!(matches!(
        done.await,
        Ok(Outcome::Done(Response::Error { .. }))
    ));
    assert!(log(&stand).is_empty());
}

/// A set of values rewrites only the bands it is given: a held write to another built-in band
/// survives a later set of values and goes out after it, where a named setting would drop it.
#[tokio::test]
async fn a_later_set_of_values_keeps_a_held_write_to_a_band_it_leaves_out() {
    let (writer, stand) = loki([15; 6]);
    stand.gate.forget_permits(1000);
    let first = writer.apply_values(Origin::Web, "One".into(), vec![("band1".into(), 28)]);
    let held = writer.submit(Origin::Socket, set("band5", "3"), Mode::Latest);
    let second = writer.apply_values(Origin::Web, "Two".into(), vec![("band2".into(), 21)]);
    let mut arrivals = writer.arrivals();
    arrivals
        .wait_for(|taken| *taken >= 3)
        .await
        .expect("all three arrived");
    assert_eq!(
        writer.view().await.held,
        1,
        "the write to band5 is held, not dropped"
    );
    stand.gate.add_permits(1000);
    assert!(matches!(first.done.await, Ok(Outcome::Done(Response::Ack))));
    assert!(matches!(
        second.done.await,
        Ok(Outcome::Done(Response::Ack))
    ));
    assert!(matches!(held.await, Ok(Outcome::Done(Response::Value(_)))));
}
