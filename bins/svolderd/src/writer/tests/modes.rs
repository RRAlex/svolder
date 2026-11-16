// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

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
