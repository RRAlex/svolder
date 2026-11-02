// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `watch`'s loop and release race, over the fake device and paused time. A Ctrl-C is a
//! permit on a [`Notify`], so a test presses it when it chooses.

use std::cell::RefCell;
use std::process::ExitCode;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

use super::*;

/// How long the slow reprint takes.
const SLOW: Duration = Duration::from_secs(60);

/// A reprint that answers after [`SLOW`], with no readings.
fn slow_reprint(_: &Backend) -> Reprint {
    Box::pin(async {
        tokio::time::sleep(SLOW).await;
        Ok(Vec::new())
    })
}

/// A backend whose link was cut and comes back on the first `next_status` after the cut.
async fn dropped_and_back() -> (Backend, std::sync::Arc<crate::backend::FakeGate>) {
    // The amp's meters have a scale, which a signal needs.
    let (backend, gate) = Backend::fake_amp(Duration::from_secs(30)).await;
    gate.sever();
    (backend, gate)
}

/// The debug text of an exit code, since `ExitCode` has no equality.
fn text(code: ExitCode) -> String {
    format!("{code:?}")
}

/// A Ctrl-C that arrives while the reprint after a reconnect is still being read ends the loop
/// at once.
#[tokio::test(start_paused = true)]
async fn ctrl_c_during_the_reprint_ends_the_loop_at_once() {
    let (mut backend, _gate) = dropped_and_back().await;
    let notify = Notify::new();
    let start = Instant::now();
    let interrupt = || notify.notified();
    let presser = async {
        // Long enough for the disconnect, the reconnect and the start of the reprint.
        tokio::time::sleep(Duration::from_secs(1)).await;
        notify.notify_one();
    };
    let looping = watch_loop(&mut backend, &interrupt, slow_reprint, |_| {});

    let (code, ()) = tokio::join!(looping, presser);

    assert_eq!(text(code), text(ExitCode::SUCCESS));
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "Ctrl-C waited for the reprint: {:?}",
        start.elapsed()
    );
}

/// A status that arrives while the reprint is in flight is printed when it arrives, not after
/// the reprint.
#[tokio::test(start_paused = true)]
async fn a_status_is_not_delayed_by_a_reprint_in_flight() {
    let (mut backend, gate) = dropped_and_back().await;
    let notify = Notify::new();
    let start = Instant::now();
    let interrupt = || notify.notified();
    gate.play_signal(Duration::from_secs(1));
    let seen: RefCell<Vec<(bool, Duration)>> = RefCell::new(Vec::new());
    let driver = async {
        tokio::time::sleep(Duration::from_secs(4)).await;
        notify.notify_one();
    };
    let looping = watch_loop(&mut backend, &interrupt, slow_reprint, |shown| {
        let reading = matches!(shown, Shown::Reading(_));
        seen.borrow_mut().push((reading, start.elapsed()));
    });

    let (_, ()) = tokio::join!(looping, driver);

    let seen = seen.into_inner();
    let status = seen
        .iter()
        .skip(1)
        .find(|(reading, _)| *reading)
        .expect("the status was printed");
    assert!(
        status.1 < Duration::from_secs(10),
        "the status waited for the reprint: {:?}",
        status.1
    );
}

/// A release still running at a second Ctrl-C is given up on: the code is 130 and the wait
/// is not sat out.
#[tokio::test(start_paused = true)]
async fn a_second_ctrl_c_stops_waiting_for_the_release() {
    let (mut backend, gate) = Backend::fake(Duration::from_secs(30)).await;
    gate.slow_connects(Duration::from_secs(20));
    gate.sever();
    backend.open_feed().await.expect("the feed opens");
    backend.next_status().await.expect("an update");
    let notify = Notify::new();
    let interrupt = || notify.notified();
    let start = Instant::now();
    let mut said = Vec::new();

    let code = {
        let release = release_or_give_up(backend, "fake", ExitCode::SUCCESS, &interrupt, &mut said);
        tokio::pin!(release);
        // Past the half second that announces the release, and still inside it.
        tokio::select! {
            _ = &mut release => panic!("the release ended by itself"),
            () = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
        notify.notify_one();
        release.await
    };

    assert_eq!(text(code), text(ExitCode::from(130)));
    assert_eq!(
        String::from_utf8(said).expect("text"),
        "svolder: not waiting for the release; the device may stay held for a moment\n"
    );
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "the release was waited for: {:?}",
        start.elapsed()
    );
}

/// With no second press the release ends as it always did, with the code the loop left.
#[tokio::test(start_paused = true)]
async fn a_release_with_no_second_press_keeps_the_loops_code() {
    let (backend, _gate) = Backend::fake(Duration::from_secs(30)).await;
    let notify = Notify::new();
    let interrupt = || notify.notified();
    let mut said = Vec::new();

    let code = release_or_give_up(backend, "fake", ExitCode::from(3), &interrupt, &mut said).await;

    assert_eq!(text(code), text(ExitCode::from(3)));
    assert!(said.is_empty(), "nothing is said without a second press");
}

/// A reprint that finishes on the same tick as a status prints first, whichever the scheduler
/// woke first.
#[tokio::test(start_paused = true)]
async fn a_ready_reprint_prints_before_a_buffered_status() {
    let (mut backend, gate) = dropped_and_back().await;
    let start = Instant::now();
    gate.play_signal(Duration::from_millis(10));
    let notify = Notify::new();
    let interrupt = || notify.notified();
    let seen: RefCell<Vec<(bool, Duration)>> = RefCell::new(Vec::new());
    // Done on the very tick a meter status arrives on, so both are ready in one poll.
    let ready = |_: &Backend| -> Reprint {
        let due = start + Duration::from_millis(50);
        Box::pin(async move {
            tokio::time::sleep_until(due).await;
            Ok(Vec::new())
        })
    };
    let driver = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        notify.notify_one();
    };
    let looping = watch_loop(&mut backend, &interrupt, ready, |shown| {
        seen.borrow_mut()
            .push((matches!(shown, Shown::Reading(_)), start.elapsed()));
    });

    let (_, ()) = tokio::join!(looping, driver);

    let seen = seen.into_inner();
    // Entry 0 is the opening snapshot; the next snapshot is the reprint.
    let at = seen
        .iter()
        .skip(1)
        .position(|(reading, _)| !reading)
        .expect("the reprint was printed")
        + 1;
    let due = seen[at].1;
    assert!(
        seen[..at].iter().skip(1).all(|(_, t)| *t < due),
        "a status at the reprint's tick printed before it: {seen:?}"
    );
    assert!(
        seen[at..].iter().any(|(reading, t)| *reading && *t == due),
        "no status shared the reprint's tick, so the case tested nothing: {seen:?}"
    );
}

/// A gated readout's live status shows the value its control allows: false while the control
/// last read false, true once it reads true, with the wire value untouched.
#[tokio::test(start_paused = true)]
async fn a_live_status_of_a_gated_readout_shows_its_derived_value() {
    use svolder_proto::{FunctionId, Message, Value};
    let source = "schema = 1\nname = \"t\"\n\
        [[control]]\nid = [4]\nkey = \"warming_up\"\nlabel = \"Warming up\"\nkind = \"readout\"\n\
        value_type = \"bool\"\nonly_while = \"power\"\n\
        [[control]]\nid = [10]\nkey = \"power\"\nlabel = \"Power\"\nkind = \"toggle\"\n";
    let profile = svolder_profiles::Registry::from_sources(vec![("t".into(), source.into())])
        .expect("the test profile loads")
        .get("t")
        .expect("the test profile exists")
        .clone();
    let (mut backend, gate) = Backend::fake_over(profile, Duration::from_secs(30)).await;
    let notify = Notify::new();
    let interrupt = || notify.notified();
    let seen: RefCell<Vec<(String, String, serde_json::Value)>> = RefCell::new(Vec::new());
    let warming = |on| Message::status(FunctionId::one(4), Value::Bool(on));
    let power = |on| Message::status(FunctionId::one(10), Value::Bool(on));
    let driver = async {
        for step in [
            vec![power(false)],
            vec![warming(true)],
            vec![power(true)],
            vec![power(true)],
            vec![power(false)],
        ] {
            tokio::time::sleep(Duration::from_secs(1)).await;
            gate.push(&step);
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        notify.notify_one();
    };
    let looping = watch_loop(&mut backend, &interrupt, slow_reprint, |shown| {
        if let Shown::Reading(vo) = shown {
            seen.borrow_mut()
                .push((vo.id.clone(), vo.display.clone(), vo.raw.clone()));
        }
    });

    let (_, ()) = tokio::join!(looping, driver);

    let shown: Vec<(String, String, serde_json::Value)> = seen.into_inner();
    let t = |id: &str, display: &str, raw: serde_json::Value| {
        (id.to_string(), display.to_string(), raw)
    };
    assert_eq!(
        shown,
        vec![
            t("10", "off", serde_json::json!(false)),
            // Reported true while the power is off: shown false, raw as sent.
            t("4", "false", serde_json::json!(true)),
            // The power comes on under it: the readout is shown again, true.
            t("10", "on", serde_json::json!(true)),
            t("4", "true", serde_json::json!(true)),
            // A power report that changes nothing shows nothing extra.
            t("10", "on", serde_json::json!(true)),
            // The power goes off: the readout is shown false.
            t("10", "off", serde_json::json!(false)),
            t("4", "false", serde_json::json!(true)),
        ]
    );
}
