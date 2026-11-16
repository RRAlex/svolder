// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A volume rise that climbs through the device's writer, and the wait for it.

use super::*;

/// A rise of 6 dB or less that lands above three quarters of the range needs no question but
/// climbs, through the device's writer, and the tool reports the level the device reached.
#[tokio::test]
async fn a_rise_above_three_quarters_climbs_and_reports_the_level_reached() {
    let fleet = scripted("gungnir2-v2", &[("volume", 120)]);
    let answer = run(&fleet, None, set("volume", json!(0), false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert_eq!(*fleet.glides.lock().unwrap(), [("volume".to_string(), 127)]);
    let CallToolResponse::Complete(result) = &answer else {
        panic!("{answer:?}");
    };
    let reply = result
        .structured_content
        .as_ref()
        .expect("a structured reply");
    assert_eq!(reply["value"], json!(0.0));
}

/// Every volume write goes through the device's writer, which judges it on its own reading: a
/// rise to three quarters and a lowering land at once there, and the tool reports the echo.
#[tokio::test]
async fn every_volume_write_goes_through_the_writer_and_one_that_may_land_at_once_lands() {
    let fleet = scripted("gungnir2-v2", &[("volume", 90)]);
    for raw in [95, 60] {
        let answer = run(
            &fleet,
            None,
            set("volume", json!(db(&fleet.profile, raw)), false),
            false,
        )
        .await;
        assert!(went(&answer), "{raw}: {answer:?}");
        let CallToolResponse::Complete(result) = &answer else {
            panic!("{answer:?}");
        };
        let reply = result
            .structured_content
            .as_ref()
            .expect("a structured reply");
        assert_eq!(reply["value"], json!(db(&fleet.profile, raw)), "{raw}");
    }
    assert_eq!(
        *fleet.glides.lock().unwrap(),
        [("volume".to_string(), 95), ("volume".to_string(), 60)]
    );
    assert_eq!(
        fleet.sets.lock().unwrap().last(),
        Some(&("volume".to_string(), "60".to_string()))
    );
}

/// An MCP climb another front end cuts short is reported, once the wait runs out, at the level
/// the device reached: not an error, and not a hang.
#[tokio::test(start_paused = true)]
async fn a_climb_cut_short_is_reported_at_the_level_reached() {
    let fleet = scripted("gungnir2-v2", &[("volume", 120)]);
    *fleet.stuck.lock().unwrap() = true;
    let started = Instant::now();
    let answer = run(&fleet, None, set("volume", json!(0), false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert!(started.elapsed() >= CLIMB_WAIT, "{:?}", started.elapsed());
    assert!(
        started.elapsed() < CLIMB_WAIT + 2 * CLIMB_GRACE,
        "{:?}",
        started.elapsed()
    );
    let CallToolResponse::Complete(result) = &answer else {
        panic!("{answer:?}");
    };
    let reply = result
        .structured_content
        .as_ref()
        .expect("a structured reply");
    assert_eq!(reply["value"], json!(db(&fleet.profile, 120)));
}

/// A climb the tool waits for is waited for on the writer's climb watch: no read every few
/// milliseconds meanwhile, one when the climb has ended, and the answer carries the level reached.
#[tokio::test(start_paused = true)]
async fn a_climb_is_waited_for_on_the_watch_not_by_reading() {
    let fleet = scripted("gungnir2-v2", &[("volume", 120)]);
    *fleet.shown_climb.lock().unwrap() = true;
    let ending = Arc::clone(&fleet);
    let end = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(4)).await;
        ending.readings.lock().unwrap().insert("volume".into(), 127);
        ending.climb.send_replace(None);
    });
    let started = Instant::now();
    let answer = run(&fleet, None, set("volume", json!(0), false), false).await;
    end.await.unwrap();
    assert!(went(&answer), "{answer:?}");
    let gets = *fleet.gets.lock().unwrap();
    assert!(gets <= 2, "{gets} reads during a 4 s climb");
    assert_eq!(started.elapsed(), Duration::from_secs(4));
    let CallToolResponse::Complete(result) = &answer else {
        panic!("{answer:?}");
    };
    let reply = result
        .structured_content
        .as_ref()
        .expect("a structured reply");
    assert_eq!(reply["value"], json!(0.0));
}

/// The writer shows a climb a moment after it answers that one began: a climb that shows and ends
/// while the tool's one reading is out is over, and the answer does not wait out [`CLIMB_WAIT`].
#[tokio::test(start_paused = true)]
async fn a_climb_that_shows_and_ends_during_the_reading_is_not_waited_for() {
    let fleet = scripted("gungnir2-v2", &[("volume", 120)]);
    *fleet.stuck.lock().unwrap() = true;
    let hooked = Arc::clone(&fleet);
    let once = std::sync::atomic::AtomicBool::new(false);
    *fleet.on_read.lock().unwrap() = Some(Box::new(move |key| {
        if !hooked.glides.lock().unwrap().is_empty()
            && !once.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            hooked.climb.send_replace(Some(ClimbView {
                key: key.to_string(),
                target: 127,
            }));
            hooked.climb.send_replace(None);
        }
    }));
    let started = Instant::now();
    let answer = run(&fleet, None, set("volume", json!(0), false), false).await;
    assert!(went(&answer), "{answer:?}");
    assert!(started.elapsed() < CLIMB_WAIT, "{:?}", started.elapsed());
    // The guard's reading, the one that tells a shown climb apart, and the answer's.
    assert_eq!(*fleet.gets.lock().unwrap(), 3);
}

/// A climb the writer shows after the tool's one reading is waited for until it ends, and the
/// answer carries the level reached.
#[tokio::test(start_paused = true)]
async fn a_climb_that_shows_after_the_reading_is_waited_for() {
    let fleet = scripted("gungnir2-v2", &[("volume", 120)]);
    *fleet.stuck.lock().unwrap() = true;
    let hooked = Arc::clone(&fleet);
    let once = std::sync::atomic::AtomicBool::new(false);
    *fleet.on_read.lock().unwrap() = Some(Box::new(move |key| {
        if !hooked.glides.lock().unwrap().is_empty()
            && !once.swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            hooked.climb.send_replace(Some(ClimbView {
                key: key.to_string(),
                target: 127,
            }));
        }
    }));
    let ending = Arc::clone(&fleet);
    let end = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(4)).await;
        ending.readings.lock().unwrap().insert("volume".into(), 127);
        ending.climb.send_replace(None);
    });
    let started = Instant::now();
    let answer = run(&fleet, None, set("volume", json!(0), false), false).await;
    end.await.unwrap();
    assert!(went(&answer), "{answer:?}");
    assert_eq!(started.elapsed(), Duration::from_secs(4));
    // The guard's reading, the one that tells a shown climb apart, and the answer's.
    assert_eq!(*fleet.gets.lock().unwrap(), 3);
    let CallToolResponse::Complete(result) = &answer else {
        panic!("{answer:?}");
    };
    let reply = result
        .structured_content
        .as_ref()
        .expect("a structured reply");
    assert_eq!(reply["value"], json!(0.0));
}

/// A daemon that stops answering reads once the climb has begun does not hold the tool past its
/// wait: it answers with an error then.
#[tokio::test(start_paused = true)]
async fn a_daemon_that_stops_answering_does_not_hold_a_climb_past_the_wait() {
    let fleet = scripted("gungnir2-v2", &[("volume", 120)]);
    *fleet.stuck.lock().unwrap() = true;
    *fleet.deaf.lock().unwrap() = true;
    let started = Instant::now();
    let answer = run(&fleet, None, set("volume", json!(0), false), false).await;
    assert!(is_error(&answer), "{answer:?}");
    assert!(
        started.elapsed() < CLIMB_WAIT + 2 * CLIMB_GRACE,
        "{:?}",
        started.elapsed()
    );
}
