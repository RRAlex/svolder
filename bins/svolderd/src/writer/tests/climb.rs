// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

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
