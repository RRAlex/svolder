// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The loop over a daemon that dies under it: the feed ends, the loop waits for the daemon to
//! come back and says the device is gone when it does not, and `r` afterwards opens a direct
//! session, as `auto` does. The stub daemon speaks over real sockets, so these cases run on
//! real time with generous bounds.

use super::*;

use crate::test_support::stub_daemon::stub_daemon;
use crate::tui::run::Opened;

/// An opener that behaves as `auto` does: the daemon while one answers on the socket, a direct
/// session (the fake device) once none does.
struct AutoLike(PathBuf);

impl Open for AutoLike {
    async fn open(&self, globals: OwnedGlobals, _: &Path) -> Result<Opened, Failure> {
        if svolder_ipc::is_running(&self.0).await {
            let backend = Backend::stub_remote_named(&self.0, globals.device.as_deref()).await?;
            Ok((backend, device_rows()))
        } else {
            let (backend, _gate) = Backend::fake(SHORT_DEADLINE).await;
            Ok((backend, device_rows()))
        }
    }

    async fn open_on(&self, socket: PathBuf, globals: OwnedGlobals) -> Result<Opened, Failure> {
        let backend = Backend::stub_remote_named(&socket, globals.device.as_deref()).await?;
        Ok((backend, device_rows()))
    }

    async fn hold(
        &self,
        _: Option<PathBuf>,
        _: OwnedGlobals,
        _: Option<String>,
        _: &Path,
    ) -> Option<crate::backend::Held> {
        None
    }
}

/// How long the loop waits for a dead daemon to come back.
const WAIT: Duration = Duration::from_millis(200);
/// How long a script waits for the loop to show what its next key depends on: far more than
/// the case needs, so only a loop that never shows it runs it out.
const SHOWS_WITHIN: Duration = Duration::from_secs(20);

/// Run the loop over a stub daemon that stops soon after the loop starts. The user presses
/// each key of `keys` once a frame drawn since the previous key carries the text paired with it
/// (`""` for no wait). Returns what the loop ended with and its frames.
async fn over_a_dying_daemon(keys: Vec<(&'static str, char)>) -> (LoopEnd, Vec<String>) {
    let daemon = stub_daemon().await;
    let opener = Opener::new(
        globals(false),
        TuiConfig::default(),
        scratch_config(),
        AutoLike(daemon.socket.clone()),
    )
    .waiting_for_the_daemon(WAIT);
    let watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon opens");
    assert!(watching.app.session.daemon, "the case starts over a daemon");
    let stopper = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        daemon.stop().await;
    });
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let mut mouse = SpyMouse::default();
    let seen_by_script = Arc::clone(&frames);
    let script = stream::unfold((keys.into_iter(), 0usize), move |(mut keys, mut seen)| {
        let frames = Arc::clone(&seen_by_script);
        async move {
            let (needle, c) = keys.next()?;
            let started = std::time::Instant::now();
            while !needle.is_empty()
                && !drawn(&frames)[seen.min(drawn(&frames).len())..]
                    .iter()
                    .any(|frame| frame.contains(needle))
            {
                assert!(
                    started.elapsed() < SHOWS_WITHIN,
                    "the loop never drew {needle:?} before the key {c:?}"
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            seen = drawn(&frames).len();
            Some((key(c), (keys, seen)))
        }
    });
    let end = event_loop(LoopParts {
        opener: &opener,
        adder: &nothing_to_add(),
        watching,
        terminal: &mut terminal,
        events: Box::pin(script),
        mouse: &mut mouse,
        title: &mut title(),
        look: look(),
    })
    .await;
    stopper.await.expect("the daemon stopped");
    (end, drawn(&frames))
}

/// A daemon that dies ends the feed; the loop waits for it, then says the device is gone and
/// keeps the daemon backend, which has nothing to read from.
#[tokio::test]
async fn a_daemon_that_dies_under_the_loop_ends_its_feed_and_the_loop_says_the_device_is_gone() {
    let (end, frames) = over_a_dying_daemon(vec![(GONE, 'q'), ("", 'q')]).await;

    let LoopEnd {
        watching,
        code,
        why,
    } = end;
    let remote = matches!(watching.backend, Some(Backend::Remote(_)));
    let feed_open = watching.feed_open;
    watching.close().await;
    assert_eq!(code, 0, "{why:?}");
    let first_gone = frames
        .iter()
        .position(|frame| frame.contains(GONE))
        .expect("no frame said the device is gone");
    assert!(
        frames[..first_gone].iter().any(|frame| has_spinner(frame)),
        "the loop did not wait for the daemon before giving up"
    );
    assert!(!feed_open, "the feed is still open");
    assert!(remote, "the daemon backend was replaced");
}

/// `r` once the daemon is gone opens again as `auto` does, which with no daemon answering is a
/// direct session.
#[tokio::test]
async fn r_after_the_daemon_died_falls_back_to_a_direct_session() {
    let (end, _frames) =
        over_a_dying_daemon(vec![(GONE, 'r'), ("● connected", 'q'), ("", 'q')]).await;

    let LoopEnd {
        watching,
        code,
        why,
    } = end;
    let direct = matches!(watching.backend, Some(Backend::Direct(_)));
    let daemon = watching.app.session.daemon;
    let feed_open = watching.feed_open;
    watching.close().await;
    assert_eq!(code, 0, "{why:?}");
    assert!(direct, "`r` did not open a direct session");
    assert!(!daemon);
    assert!(feed_open, "the direct session has no feed");
}
