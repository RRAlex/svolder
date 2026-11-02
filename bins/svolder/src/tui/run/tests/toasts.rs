// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The toasts that name a daemon of another version and which side to restart.

use super::*;

/// The words the toast for a daemon newer than this svolder says: the TUI is the one to restart.
fn differs_toast(daemon: &str) -> String {
    format!(
        "daemon v{daemon} is newer than this svolder v{}: restart svolder",
        env!("CARGO_PKG_VERSION")
    )
}

/// The words the toast for a daemon older than this svolder says: the daemon is restarted.
fn older_toast(daemon: &str) -> String {
    format!(
        "daemon v{daemon} is older than this svolder v{}: svolder daemon restart",
        env!("CARGO_PKG_VERSION")
    )
}

/// The toast names the older side: a daemon older than this svolder is restarted, one newer
/// means the TUI is.
#[tokio::test]
async fn the_version_toast_names_which_side_to_restart() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    let opener = daemon_opener(&daemon.socket, Duration::from_secs(1));
    let mut watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    daemon_answered(
        &mut watching,
        Some(said("0.0.1")),
        std::path::Path::new("config.toml"),
    );
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.clone()),
        Some(older_toast("0.0.1"))
    );
    watching.app.dismiss_toast();
    daemon_answered(
        &mut watching,
        Some(said("99.0.0")),
        std::path::Path::new("config.toml"),
    );
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.clone()),
        Some(differs_toast("99.0.0"))
    );
    watching.close().await;
}

/// A daemon of another version earns one warning toast, however many devices are opened on it;
/// a daemon of this svolder's own earns none.
#[tokio::test]
async fn a_daemon_of_another_version_is_warned_about_once() {
    use crate::test_support::stub_daemon::{STUB_VERSION, stub_daemon};

    let daemon = stub_daemon().await;
    let opener = daemon_opener(&daemon.socket, Duration::from_secs(1));
    let mut watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");

    daemon_answered(
        &mut watching,
        Some(said(STUB_VERSION)),
        std::path::Path::new("config.toml"),
    );
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.clone()),
        Some(differs_toast(STUB_VERSION))
    );
    assert_eq!(
        watching.app.toast().map(|toast| toast.kind),
        Some(crate::tui::boxes::toast::ToastKind::Failure)
    );
    assert_eq!(
        watching.app.session.daemon_version.as_deref(),
        Some(STUB_VERSION)
    );

    // Another device opened on the same daemon asks again and hears the same.
    watching.app.dismiss_toast();
    let mut next = opener
        .reopen_keeping(None, Some(Kept::of(&watching.app)))
        .await
        .expect("the stub daemon answers");
    daemon_answered(
        &mut next,
        Some(said(STUB_VERSION)),
        std::path::Path::new("config.toml"),
    );
    assert!(next.app.toast().is_none(), "warned twice about one daemon");

    // Its own version, and a daemon that gave none, say nothing.
    let mut fresh = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    daemon_answered(
        &mut fresh,
        Some(said(env!("CARGO_PKG_VERSION"))),
        std::path::Path::new("config.toml"),
    );
    assert!(fresh.app.toast().is_none());
    daemon_answered(&mut fresh, None, std::path::Path::new("config.toml"));
    assert!(fresh.app.toast().is_none());
    watching.close().await;
    next.close().await;
    fresh.close().await;
}

/// The loop itself raises the toast when the daemon's answer arrives, and a direct session
/// never does: it has no daemon to ask.
#[tokio::test]
async fn the_loop_warns_of_a_daemon_of_another_version_and_a_direct_session_never_does() {
    use crate::test_support::stub_daemon::{STUB_VERSION, stub_daemon};

    let daemon = stub_daemon().await;
    let on_daemon = daemon_opener(&daemon.socket, Duration::from_secs(1));
    let session = on_daemon
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let end = event_loop(LoopParts {
        opener: &on_daemon,
        adder: &nothing_to_add(),
        watching: session,
        terminal: &mut terminal,
        events: quit_after(Duration::from_millis(800)),
        mouse: &mut SpyMouse::default(),
        title: &mut title(),
        look: look(),
    })
    .await;
    assert!(
        any_frame(&drawn(&frames), "than this svolder"),
        "the warning never reached the screen"
    );
    assert_eq!(
        end.watching.app.session.daemon_version.as_deref(),
        Some(STUB_VERSION)
    );
    end.watching.close().await;

    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let (recorder, frames) = Recorder::new(SIZE.0, SIZE.1);
    let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
    let end = event_loop(LoopParts {
        opener: &opener_over(never_opens()),
        adder: &nothing_to_add(),
        watching: watching(backend, app, None),
        terminal: &mut terminal,
        events: quit_after(Duration::from_millis(300)),
        mouse: &mut SpyMouse::default(),
        title: &mut title(),
        look: look(),
    })
    .await;
    assert!(!any_frame(&drawn(&frames), "than this svolder"));
    end.watching.close().await;
}

/// A restart onto the version already warned about stays quiet, another differing version
/// warns again, and the warning follows the version rather than how the daemon was met.
#[tokio::test]
async fn the_version_warning_follows_the_version_met() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    let opener = daemon_opener(&daemon.socket, Duration::from_secs(1));
    let mut watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    daemon_answered(
        &mut watching,
        Some(said("9.9.9")),
        std::path::Path::new("config.toml"),
    );
    assert!(watching.app.toast().is_some());
    watching.app.dismiss_toast();
    daemon_answered(
        &mut watching,
        Some(said("9.9.9")),
        std::path::Path::new("config.toml"),
    );
    assert!(watching.app.toast().is_none(), "the same version, twice");
    daemon_answered(
        &mut watching,
        Some(said("9.9.8")),
        std::path::Path::new("config.toml"),
    );
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.clone()),
        Some(differs_toast("9.9.8"))
    );
    watching.close().await;
}
