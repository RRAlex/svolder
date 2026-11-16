// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The grace a saving wait waits before it draws.

use super::*;

/// The words every frame a wait drew said, for a wait of `grace` over work that takes `work`.
async fn wait_frames(grace: Duration, work: Duration) -> Vec<String> {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut watching = watching(backend, app, None);
    let mut said: Vec<String> = Vec::new();
    let mut screen = |app: &mut App, _look: &Look| {
        said.push(
            app.toast()
                .map(|toast| toast.text.clone())
                .unwrap_or_default(),
        );
    };
    let mut wait = Wait::after("saving…".to_string(), grace);
    let done = wait
        .on(
            tokio::time::sleep(work),
            &mut watching,
            &look(),
            &mut screen,
            Interrupted::Carry,
        )
        .await;
    assert!(done.is_some(), "the work was seen through");
    watching.close().await;
    said
}

/// A saving wait draws nothing while its grace lasts: a write that ends first shows no
/// "saving…" at all, and one that outlasts it shows the wait from the grace on.
#[tokio::test(start_paused = true)]
async fn a_saving_wait_shows_only_after_its_grace() {
    let quick = wait_frames(SAVING_GRACE, SAVING_GRACE / 5).await;
    assert!(quick.is_empty(), "a quick write flashed a wait: {quick:?}");
    let slow = wait_frames(SAVING_GRACE, SAVING_GRACE * 4).await;
    assert!(
        !slow.is_empty() && slow.iter().all(|words| words.contains("saving…")),
        "a slow write shows the wait: {slow:?}"
    );
    let at_once = wait_frames(Duration::ZERO, SAVING_GRACE / 5).await;
    assert!(
        !at_once.is_empty(),
        "a wait with no grace draws its first frame at once"
    );
}

/// A saving wait takes down the toast that was showing when it began, as the one it would draw
/// does, whether or not the write outlasts the grace (a wait with no grace replaced it with its own).
#[tokio::test(start_paused = true)]
async fn a_saving_wait_clears_an_earlier_failure_toast_even_when_quick() {
    for grace in [SAVING_GRACE, Duration::ZERO] {
        let (backend, _gate, mut app) = fake(SHORT_DEADLINE).await;
        app.warn("an earlier failure");
        let mut watching = watching(backend, app, None);
        let mut wait = Wait::after("saving…".to_string(), grace);
        wait.on(
            std::future::ready(()),
            &mut watching,
            &look(),
            &mut (),
            Interrupted::Carry,
        )
        .await;
        assert!(watching.app.toast().is_none(), "grace {grace:?}");
        watching.close().await;
    }
}
