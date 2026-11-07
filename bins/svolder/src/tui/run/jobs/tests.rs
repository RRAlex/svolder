// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::tui::run::test_support::FakeHands;

use super::*;

const LINE: &str =
    "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990";

/// Each job runs off the caller and ends as a `Done`: the index through the policy asked, a
/// download as the entry with its text.
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
}

/// Failures come back as words, not panics.
#[tokio::test]
async fn failures_come_back_as_words() {
    let hands = Arc::new(FakeHands {
        ..FakeHands::default()
    });
    let mut jobs = Jobs::over(hands);
    jobs.index(Refresh::Now);
    assert!(matches!(next(&mut jobs.ended).await, Done::Index(Err(why)) if why == "no network"));

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
