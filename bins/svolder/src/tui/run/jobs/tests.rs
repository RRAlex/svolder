// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use svolder_host::eq::{Manager, Route};
use svolder_profiles::tone::apo::Correction;

use crate::tui::run::test_support::FakeHands;

use super::*;

const LINE: &str =
    "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990";

/// Each job runs off the caller and ends as a `Done`: the index through the policy asked, a
/// download as the entry with its text, an apply and a removal with the preset to load next.
#[tokio::test]
async fn each_job_ends_as_a_done() {
    let hands = Arc::new(FakeHands {
        index_lines: Some(vec![LINE.into()]),
        text: Some("Preamp: -1 dB".into()),
        ..FakeHands::default()
    });
    let mut jobs = Jobs::over(hands.clone());

    jobs.index(Refresh::AfterWeeks(13));
    let Done::Index(Ok(index)) = next(&mut jobs.ended).await else {
        panic!("the index did not arrive");
    };
    assert_eq!(index.entries.len(), 1);
    assert_eq!(
        *hands.fetched.lock().expect("log"),
        [Refresh::AfterWeeks(13)]
    );

    jobs.download(index.entries[0].clone());
    let Done::Download(entry, Ok(text)) = next(&mut jobs.ended).await else {
        panic!("the file did not arrive");
    };
    assert_eq!(
        (entry.name.as_str(), text.as_str()),
        ("Sennheiser HD 650", "Preamp: -1 dB")
    );

    jobs.apply(ApplyRequest {
        name: "Phones".into(),
        origin: None,
        correction: Correction {
            preamp_db: -1.0,
            filters: Vec::new(),
        },
        sink: None,
    });
    let Done::Applied(name, _, Ok(state)) = next(&mut jobs.ended).await else {
        panic!("the apply did not end");
    };
    assert_eq!((name.as_str(), state.on), ("Phones", true));
    assert_eq!(jobs.computer(), (Some("Phones".to_string()), None));

    jobs.remove(Some("Phones".into()));
    let Done::Removed { then_load, result } = next(&mut jobs.ended).await else {
        panic!("the removal did not end");
    };
    assert_eq!(then_load.as_deref(), Some("Phones"));
    assert!(result.is_ok());
    assert_eq!(
        jobs.computer(),
        (None, None),
        "off, so no preset is on the computer"
    );
}

/// Failures come back as words, not panics, and the computer's reason is read with its state.
#[tokio::test]
async fn failures_come_back_as_words() {
    let hands = Arc::new(FakeHands {
        off: Some("host EQ is off: SVOLDER_HOST=off".into()),
        ..FakeHands::default()
    });
    let mut jobs = Jobs::over(hands);
    jobs.index(Refresh::Now);
    assert!(matches!(next(&mut jobs.ended).await, Done::Index(Err(why)) if why == "no network"));
    assert_eq!(
        jobs.computer(),
        (None, Some("host EQ is off: SVOLDER_HOST=off".to_string()))
    );

    // A loop that reaches nothing answers every job the same, and never reaches for anything.
    let mut idle = Jobs::default();
    idle.download(Entry {
        name: "x".into(),
        source: "s".into(),
        form: "over-ear".into(),
        rig: None,
    });
    assert!(matches!(
        next(&mut idle.ended).await,
        Done::Download(_, Err(_))
    ));
    assert!(idle.computer().1.is_some());
}

/// The index is fetched as the config says, and at once when asked.
#[test]
fn the_policy_follows_the_config_unless_refreshing() {
    assert_eq!(policy(true, RefreshWeeks::Never), Refresh::Now);
    assert_eq!(policy(false, RefreshWeeks::Never), Refresh::Never);
    assert_eq!(
        policy(false, RefreshWeeks::Weeks(4)),
        Refresh::AfterWeeks(4)
    );
}

/// The state file is read again only when its modification time changed: the first look reads
/// it, a look at the same time reads nothing, a new time reads it, and a file that is gone reads
/// as none.
#[test]
fn the_state_is_reread_when_its_mtime_changes() {
    let hands = Arc::new(FakeHands::default());
    let mut jobs = Jobs::over(hands.clone());
    assert_eq!(
        jobs.host_eq_changed(),
        Some(None),
        "no file on the first look"
    );
    assert_eq!(jobs.host_eq_changed(), None, "and no change after it");

    let state = |name: &str| HostEqState {
        on: true,
        name: name.into(),
        origin: None,
        target: "alsa_output.test".into(),
        route: Route::Default,
        manager: Manager::Process,
        previous_default: None,
        preamp_db: 0.0,
        filters: Vec::new(),
    };
    let at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    *hands.state.lock().unwrap() = Some(state("Phones"));
    *hands.stamp.lock().unwrap() = Some(at);
    assert_eq!(jobs.host_eq_changed(), Some(Some(state("Phones"))));
    assert_eq!(jobs.host_eq_changed(), None, "the same time reads nothing");

    *hands.state.lock().unwrap() = Some(state("Other"));
    assert_eq!(
        jobs.host_eq_changed(),
        None,
        "a change with the same time is not seen"
    );
    *hands.stamp.lock().unwrap() = Some(at + Duration::from_secs(1));
    assert_eq!(jobs.host_eq_changed(), Some(Some(state("Other"))));

    jobs.look_again();
    assert_eq!(
        jobs.host_eq_changed(),
        Some(Some(state("Other"))),
        "an app that has not read it yet"
    );

    *hands.state.lock().unwrap() = None;
    *hands.stamp.lock().unwrap() = None;
    assert_eq!(jobs.host_eq_changed(), Some(None), "the file is gone");
}
