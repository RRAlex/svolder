// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

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
    let refused = Response::error(CODE_LINK, "the device said no".to_string());
    let (commands, seen, _task) = actor(vec![Response::Ack, Response::Ack, refused]);
    let steps = vec![mode("flat"), mode("eq"), mode("loudness"), mode("eq")];
    match steps_through(&commands, steps).await {
        Response::Error { code, message, .. } => {
            assert_eq!(code, CODE_LINK);
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
