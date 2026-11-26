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
    let (keys, input) = keyed();
    let warned = |frames: Frames| async move {
        shown(&frames, "than this svolder").await;
        quit(&keys);
    };
    let (end, _frames) = loop_over(&on_daemon, session, Box::pin(input), warned).await;
    assert_eq!(
        end.watching.app.session.daemon_version.as_deref(),
        Some(STUB_VERSION)
    );
    end.watching.close().await;

    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let (keys, input) = keyed();
    let drawn_once = |frames: Frames| async move {
        shown(&frames, "Mute").await;
        quit(&keys);
    };
    let (end, frames) = loop_over(
        &opener_over(never_opens()),
        watching(backend, app, None),
        Box::pin(input),
        drawn_once,
    )
    .await;
    assert!(!any_frame(&frames, "than this svolder"));
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

/// The toast for a configuration file that changed since the daemon's last reload.
const CHANGED_TOAST: &str =
    "config: /home/u/config.toml changed since the last reload: svolder daemon reload (or SIGHUP)";

/// A daemon that says its file changed is told once per change: the toast comes with the first
/// answer, not again while the daemon goes on saying it (nor on a device reopened on it), and
/// again once an answer of unchanged has gone between.
#[tokio::test]
async fn a_changed_config_is_toasted_once_per_change() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    let opener = daemon_opener(&daemon.socket, Duration::from_secs(1));
    let mut watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    let own = std::path::Path::new("config.toml");
    let answer = |changed: bool| {
        let mut info = said(env!("CARGO_PKG_VERSION"));
        info.config = Some("/home/u/config.toml".into());
        info.config_changed = changed;
        Some(info)
    };

    daemon_answered(&mut watching, answer(false), own);
    assert!(watching.app.toast().is_none());
    assert!(watching.app.overlays.conn.config_changed.is_none());

    daemon_answered(&mut watching, answer(true), own);
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.as_str()),
        Some(CHANGED_TOAST)
    );
    assert_eq!(
        watching.app.overlays.conn.config_changed.as_deref(),
        Some(CHANGED_TOAST),
        "the Connect line stands"
    );
    watching.app.dismiss_toast();
    daemon_answered(&mut watching, answer(true), own);
    assert!(watching.app.toast().is_none(), "told twice for one change");
    assert!(watching.app.overlays.conn.config_changed.is_some());

    // A device opened on the same daemon carries on what was told.
    let mut next = opener
        .reopen_keeping(None, Some(Kept::of(&watching.app)))
        .await
        .expect("the stub daemon answers");
    daemon_answered(&mut next, answer(true), own);
    assert!(next.app.toast().is_none(), "told again on a reopen");

    daemon_answered(&mut watching, answer(false), own);
    assert!(watching.app.overlays.conn.config_changed.is_none());
    daemon_answered(&mut watching, answer(true), own);
    assert_eq!(
        watching.app.toast().map(|toast| toast.text.as_str()),
        Some(CHANGED_TOAST),
        "the next change is told"
    );
    // A daemon that did not answer says nothing and forgets the line, but is not told as new.
    daemon_answered(&mut watching, None, own);
    assert!(watching.app.overlays.conn.config_changed.is_none());
    watching.close().await;
    next.close().await;
}

/// A hand edit followed by a delete is two news: the daemon's `config_changed` stays true from
/// "changed" to "missing", and the second word is toasted as well, not only shown in the rows.
#[tokio::test]
async fn a_delete_after_a_hand_edit_is_toasted_too() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    let opener = daemon_opener(&daemon.socket, Duration::from_secs(1));
    let mut watching = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    let own = std::path::Path::new("config.toml");
    let answer = |missing: bool| {
        let mut info = said(env!("CARGO_PKG_VERSION"));
        info.config = Some("/home/u/config.toml".into());
        info.config_changed = true;
        info.config_missing = missing;
        Some(info)
    };

    daemon_answered(&mut watching, answer(false), own);
    let changed = watching.app.toast().map(|toast| toast.text.clone());
    assert_eq!(changed.as_deref(), Some(CHANGED_TOAST));
    watching.app.dismiss_toast();
    daemon_answered(&mut watching, answer(true), own);
    let missing = watching.app.toast().map(|toast| toast.text.clone());
    assert!(
        missing.is_some_and(|words| words != CHANGED_TOAST),
        "the missing file was not toasted"
    );
    watching.app.dismiss_toast();
    daemon_answered(&mut watching, answer(true), own);
    assert!(watching.app.toast().is_none(), "told twice for one word");
    watching.close().await;
}

/// The loop asks a daemon session on its own slow clock, whichever screen shows, so an edit
/// made in another terminal reaches it; a direct session has no daemon to ask.
#[tokio::test]
async fn the_loop_asks_the_daemon_on_a_slow_clock_for_a_changed_config() {
    use crate::test_support::stub_daemon::stub_daemon;

    let daemon = stub_daemon().await;
    daemon.version(env!("CARGO_PKG_VERSION"));
    let opener = daemon_opener(&daemon.socket, Duration::from_secs(1));
    let session = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    let (keys, input) = keyed();
    // The first ask after the edit may already be on its way with the old answer; the answer of
    // the one after it is the first to say the file changed, and the one before it was handled.
    let edited = |_frames: Frames| async {
        daemon.config_changed(true);
        let asked = daemon.version_asks();
        until("two asks after the edit", || {
            daemon.version_asks() >= asked + 2
        })
        .await;
        quit(&keys);
    };
    let (end, _frames) = loop_over(&opener, session, Box::pin(input), edited).await;
    assert!(
        end.watching.app.overlays.conn.config_changed.is_some(),
        "the slow ask never reached the session"
    );
    assert!(end.watching.app.session.config_changed_told.is_some());
    end.watching.close().await;
}

/// A `[tui]` write of the Settings box that changed the file has a daemon that reads it reload
/// once the writes pause (a run of quick changes sends one), so the change is not left for the
/// daemon to announce as a hand edit; a daemon that reads another file is left alone, and so is
/// a change that wrote nothing (a refused mouse toggle, a value the file already had).
#[tokio::test]
async fn a_settings_write_has_the_daemon_reload_once_and_only_when_it_changed_the_file() {
    use crate::test_support::stub_daemon::stub_daemon;
    use crate::tui::app::TuiChange;
    use crate::tui::run::look::{reload_soon, send_owed};

    for (reads_ours, reloads) in [(true, 1), (false, 0)] {
        let daemon = stub_daemon().await;
        let path = scratch_config();
        let named = match reads_ours {
            true => path.display().to_string(),
            false => "/elsewhere/config.toml".to_string(),
        };
        daemon.says(None, Some(&named));
        let opener = daemon_opener(&daemon.socket, Duration::from_secs(1));
        let mut watching = opener
            .reopen(None, None)
            .await
            .expect("the stub daemon answers");
        let mut look = Look::new(
            svolder_ipc::TuiConfig::default(),
            true,
            svolder_ipc::ColorDepth::Ansi16,
            path.clone(),
        );
        let mut mouse = SpyMouse::default();
        for on in [true, false, true] {
            look.change(&TuiChange::Effects(on), &mut watching.app, &mut mouse);
            let wrote = look.writes.drain().await;
            assert_eq!(
                wrote,
                vec![Ok(true)],
                "a change of the effects writes the file"
            );
            reload_soon(&mut watching, look.path());
        }
        assert_eq!(
            daemon.reloads(),
            0,
            "nothing is sent while the writes go on"
        );
        send_owed(&mut watching);
        send_owed(&mut watching);
        let info = recv_version(&mut watching.daemon_version).await;
        assert!(info.is_some(), "the ask after the reload answers");
        assert_eq!(daemon.reloads(), reloads, "reads our file: {reads_ours}");
        assert!(watching.reload_owed.is_none());

        // the same value again leaves the file untouched: no write, so nothing owed
        look.change(&TuiChange::Effects(true), &mut watching.app, &mut mouse);
        let again = look.writes.drain().await;
        assert_eq!(again, vec![Ok(false)], "an unchanged file is not a write");
        watching.close().await;
    }
}

/// A mouse toggle the terminal refuses writes nothing and owes the daemon nothing.
#[tokio::test]
async fn a_refused_mouse_toggle_owes_no_reload() {
    use crate::tui::app::TuiChange;

    let mut look = look();
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut watching = watching(backend, app, None);
    let mut mouse = RefusingMouse;
    look.change(&TuiChange::Mouse(true), &mut watching.app, &mut mouse);
    assert!(!look.writes.owes(), "a refused toggle asks for no write");
    watching.close().await;
}

/// The reload a Settings write owes runs in a task of its own: a daemon that is slow to answer
/// it does not stop the loop, which ends on time, and a direct session has no daemon to tell or
/// to ask: nothing is sent, asked or toasted.
#[tokio::test]
async fn the_owed_reload_never_holds_the_loop_and_a_direct_session_asks_nothing() {
    use crate::test_support::stub_daemon::stub_daemon;
    use crate::tui::run::look::reload_soon;

    let daemon = stub_daemon().await;
    daemon.delays_reloads(Duration::from_secs(30));
    let path = scratch_config();
    daemon.says(None, Some(&path.display().to_string()));
    let opener = daemon_opener(&daemon.socket, Duration::from_secs(1));
    let mut session = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    reload_soon(&mut session, &path);
    let started = std::time::Instant::now();
    let (keys, input) = keyed();
    let sent = |_frames: Frames| async {
        until("the owed reload is sent", || daemon.reloads() == 1).await;
        quit(&keys);
    };
    let (end, _frames) = loop_over(&opener, session, Box::pin(input), sent).await;
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "the loop waited for the daemon's reload: {:?}",
        started.elapsed()
    );
    assert_eq!(daemon.reloads(), 1, "the reload was sent");
    end.watching.close().await;

    // a reload still owed when the loop ends is sent, whichever way it ended
    let daemon = stub_daemon().await;
    let path = scratch_config();
    daemon.says(None, Some(&path.display().to_string()));
    let opener = daemon_opener(&daemon.socket, Duration::from_secs(1));
    let mut session = opener
        .reopen(None, None)
        .await
        .expect("the stub daemon answers");
    reload_soon(&mut session, &path);
    session.reload_owed.as_mut().expect("owed").0 += Duration::from_secs(60);
    let (keys, input) = keyed();
    quit(&keys);
    let (end, _frames) = loop_over(&opener, session, Box::pin(input), |_| async {}).await;
    assert_eq!(daemon.reloads(), 1, "the owed reload went out at the end");
    end.watching.close().await;

    // a direct session
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut watching = watching(backend, app, None);
    reload_soon(&mut watching, &path);
    crate::tui::run::look::send_owed(&mut watching);
    assert!(watching.reload_owed.is_none());
    assert!(
        watching.daemon_version.is_none(),
        "a direct session asked a daemon"
    );
    let (keys, input) = keyed();
    let drawn_once = |frames: Frames| async move {
        shown(&frames, "Mute").await;
        quit(&keys);
    };
    let (end, _frames) = loop_over(
        &opener_over(never_opens()),
        watching,
        Box::pin(input),
        drawn_once,
    )
    .await;
    assert!(end.watching.daemon_version.is_none());
    assert!(
        end.watching.app.toast().is_none(),
        "a direct session was toasted"
    );
    end.watching.close().await;
}

/// A tick tells the panics of tasks the run survived: the loop's own tick is what reaches the
/// process's router.
#[tokio::test]
async fn a_tick_tells_a_background_task_panic() {
    let (backend, _gate, app) = fake(SHORT_DEADLINE).await;
    let mut watching = watching(backend, app, None);
    crate::tui::terminal::note_background_panic(
        "a background task panicked (svolder-tokio): the tick test",
    );
    on_tick(&mut watching).await;
    let toast = watching.app.toast().expect("the panic is told");
    assert!(toast.text.contains("the tick test"), "{}", toast.text);
}
