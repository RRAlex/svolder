// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

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
        matches!(
            lowered,
            Ok(Outcome::Done(Response::Error {
                code: CODE_TIMEOUT,
                ..
            }))
        ),
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
    let words = |message: String| Response::error(CODE_TIMEOUT, message);
    let echo = crate::device::error_for(&svolder_ble::BleError::Timeout(Duration::from_secs(2)));
    assert!(timed_out(&echo));
    assert!(timed_out(&unanswered("fake")));
    assert!(!timed_out(&not_taken("fake")));
    assert!(!timed_out(&no_reading("volume")));
    assert!(!timed_out(&words(
        "no value reported yet for volume".into()
    )));
    assert!(!timed_out(&Response::error(
        CODE_USAGE,
        "sent, not confirmed: no report from the device within 2s"
    )));
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
