// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the loop does with what the device says and what is asked of it: folding an answer
//! from the update feed into the app, refreshing, dispatching an intent as a write, and the waits
//! and held writes that follow a reading.

use crate::tui::boxes::device_presets;

use super::*;

/// Whether `update` says the device the loop is waiting for has come up.
pub(super) fn found(watching: &Watching, update: &Result<Option<Update>, BleError>) -> bool {
    matches!(watching.app.session.holding, Some(Holding::Waiting { .. }))
        && matches!(update, Ok(Some(Update::Connection { connected: true })))
}

/// Whether the loop is waiting for a device the daemon has not found yet.
pub(super) fn waiting(watching: &Watching) -> bool {
    matches!(watching.app.session.holding, Some(Holding::Waiting { .. }))
        && watching.backend.is_some()
}

/// Take up the wait's subscription again if it is closed, and say whether the daemon's own list
/// now has the waited device connected.
pub(super) async fn wait_over(watching: &mut Watching) -> bool {
    let name = watching.app.session.device_name.clone();
    let Some(backend) = watching.backend.as_mut() else {
        return false;
    };
    // While a daemon's reconnect is pending the feed is not reopened: the daemon is not there
    // to subscribe to, and the wait for it is already running.
    if !watching.feed_open
        && watching.reconnect.is_none()
        && match watching.app.asks_host() {
            true => backend.open_host_feed().await,
            false => backend.open_feed().await,
        }
        .is_ok()
    {
        watching.feed_open = true;
        watching.app.session.feed_open = true;
    }
    let Ok(Ok(rows)) = tokio::time::timeout(RELIST_BOUND, backend.held_rows()).await else {
        return false;
    };
    rows.iter()
        .any(|row| row.name == name && row.connected == Some(true))
}

/// Keep the daemon's feed asking for the host's view exactly while this session wants it
/// ([`App::asks_host`]): a `[tui] host_audio` turned off in Settings opens the feed again without
/// it, so the daemon stops reading the computer's audio graph for this TUI and what it said of the
/// host is dropped; turned on, the feed asks again and the daemon is asked for its view afresh,
/// so no view from before is shown. Nothing is done with no daemon, no open feed, or no device.
pub(super) async fn match_host_feed(watching: &mut Watching) {
    let want = watching.app.asks_host();
    if !watching.feed_open || watching.reconnect.is_some() || watching.app.session.holding.is_some()
    {
        return;
    }
    let Some(backend) = watching.backend.as_mut() else {
        return;
    };
    if !backend.through_daemon() || backend.host_feed() == want {
        return;
    }
    if backend.resubscribe(want).await.is_err() {
        return;
    }
    // What the old feed had not delivered is read again, as a reconnect reads it; and the view
    // is asked for afresh, so none from before is shown. A daemon slow to answer holds up
    // nothing: the answers arrive on a task of their own ([`fetched`]).
    watching.app.host.daemon = None;
    watching.host_refresh = Some(ask_again(backend, want));
}

/// Ask the daemon for what a feed opened afresh missed — the device's facts and readings, and
/// when `host` its host view — on a task of its own; a function of its own so the futures it
/// builds are not part of the loop's.
#[inline(never)]
fn ask_again(backend: &Backend, host: bool) -> Fetching {
    let (facts, values) = (backend.own_facts_request(), backend.snapshot_request());
    let view = backend.host_view_request().filter(|_| host);
    host::Aborting(tokio::spawn(Box::pin(async move {
        let view = async {
            match view {
                Some(view) => view.await,
                None => None,
            }
        };
        let (facts, values, view) = tokio::join!(facts, values, view);
        Box::new(Fetched {
            facts,
            values,
            view,
        })
    })))
}

/// The task that asks the daemon again once the feed is opened afresh, boxed answer so the
/// loop's futures do not carry it by value.
pub(super) type Fetching = host::Aborting<Box<Fetched>>;

/// What the daemon answered when the feed was opened afresh (see [`match_host_feed`]).
pub(super) struct Fetched {
    facts: crate::backend::OwnFacts,
    values: Result<Vec<svolder_ipc::ValueObject>, crate::recovery::Failure>,
    view: Option<svolder_ipc::host::HostView>,
}

/// What the task of [`match_host_feed`] brings, once it is done; never while none runs. `None`
/// when the task ended without an answer.
pub(super) async fn fetched(job: &mut Option<Fetching>) -> Option<Box<Fetched>> {
    match job {
        Some(job) => (&mut job.0).await.ok(),
        None => std::future::pending().await,
    }
}

/// Take what the daemon answered into the app, as [`refresh`] and a view request would have. The
/// answer can be older than the feed by the time it lands, so a value an event changed meanwhile
/// is overwritten until the next event for it.
pub(super) fn take_fetched(app: &mut App, fetched: Box<Fetched>, now: Instant) {
    app.session.passphrase_note = fetched.facts.note;
    if let Some(row) = fetched.facts.row {
        app.session.facts = Some(row);
    }
    match fetched.values {
        Ok(values) => app.replace_all(values, now),
        Err(_) => app.warn(NO_SNAPSHOT),
    }
    if let Some(view) = fetched.view {
        app.host.fold_daemon(view, now);
    }
}

/// What a write to a device that is not open is answered with.
const NO_DEVICE_OPEN: &str = "no device is open";

/// Send the steps a tone-curve drag held back while the device answered the write before them,
/// once it has answered or the wait has run out ([`curve_drag::confirmed`]), and a level-bar drag's
/// and the volume climb's the same way ([`glide::confirmed`]). Checked after every feed reading
/// and every tick, which is when either can happen.
pub(super) async fn send_held(watching: &mut Watching) {
    let intents = curve_drag::confirmed(&mut watching.app);
    send(intents, watching).await;
    let intents = glide::confirmed(&mut watching.app);
    send(intents, watching).await;
}

/// Send the band writes a tone-curve drag produces outside a mouse action of its own (a held
/// step, a lost release, mouse capture turned off), noting the row they write to as a key's
/// writes are.
pub(super) async fn send(intents: Vec<Intent>, watching: &mut Watching) {
    watching.app.touched(&intents);
    for intent in intents {
        dispatch(intent, watching).await;
    }
}

/// The next thing a backend has to say, or nothing ever from a loop that holds no backend.
///
/// The arm reading the feed is disabled without one, so this only ever waits for real work; it
/// exists so the expression that arm reads always has a backend to ask, never a missing one to
/// unwrap.
pub(super) async fn next_status(backend: Option<&mut Backend>) -> Result<Option<Update>, BleError> {
    match backend {
        Some(backend) => backend.next_status().await,
        None => std::future::pending().await,
    }
}

/// Fold one answer from the update feed into the app, and say whether the feed is still open.
///
/// Only a reading or a changed link leaves it open. `Ok(None)` is the end of the feed: the
/// reconnect deadline passed on the direct path, or the event stream ended on the daemon path. A
/// failure closes it too, since both backends report one from a stream they can no longer read, and
/// reading either again would answer instantly and forever.
/// [`Session::feed_open`](crate::tui::app::Session::feed_open) is closed here too, which stops the
/// header's spinner turning for a reconnect nothing is attempting any more (a link merely reported
/// down, with the feed still open, keeps it turning).
///
/// A link coming back is followed by a read of the whole device: the refresh burst a reconnect
/// takes is folded into the session's state rather than queued as updates, so the app doesn't
/// keep showing stale pre-disconnect readings. "Coming back" is [`App::apply`]'s own flag,
/// raised only on a false-to-true transition — a subscription that opens already saying the
/// link is up, as the daemon's does, is not a reconnect and costs no round trip.
pub(super) async fn heard(
    update: Result<Option<Update>, BleError>,
    backend: &mut Backend,
    app: &mut App,
    now: Instant,
) -> bool {
    match update {
        Ok(Some(update)) => {
            let link = matches!(update, Update::Connection { .. });
            app.apply(update, now);
            if app.frame_flags.reconnected {
                refresh(backend, app, now).await;
            } else if link {
                // A link that came or went is when the daemon judges the passphrase again.
                refresh_note(backend, app).await;
            }
            return true;
        }
        Ok(None) => app.write_failed(&Failure::of(Kind::Dropped, GONE)),
        Err(e) => app.write_failed(&Failure::from_ble(&e)),
    }
    app.link_changed(false);
    // Nothing is retrying this any more, so the header stops spinning for it.
    app.session.feed_open = false;
    false
}

/// Ask the backend how the device's passphrase stands against its advertisement, and how the
/// device itself is listed (its signal, its stale and refused flags), in one question, and take
/// the answers into the app, for the Status pane to say. Best effort: nothing else depends on
/// them, so a daemon that cannot be asked in time leaves the facts as they were.
pub(super) async fn refresh_note(backend: &mut Backend, app: &mut App) {
    let facts = backend.own_facts().await;
    app.session.passphrase_note = facts.note;
    if let Some(row) = facts.row {
        app.session.facts = Some(row);
    }
}

/// Read every value the device holds and take the answer in place of what the app held.
async fn refresh(backend: &mut Backend, app: &mut App, now: Instant) {
    refresh_note(backend, app).await;
    match backend.snapshot().await {
        Ok(values) => app.replace_all(values, now),
        // A CLI verb would leave with a code and print words on a failed snapshot; the TUI does
        // neither, and stays up saying what it could not do.
        Err(_) => app.warn(NO_SNAPSHOT),
    }
}

/// Carry out one intent the user asked for, reporting whatever it comes to as a notice.
///
/// With no backend at all (a switch and its fallback both failed) there is nothing to send to,
/// and the loop says so rather than quietly dropping what was asked for.
pub(super) async fn dispatch(intent: Intent, watching: &mut Watching) {
    // A one-notch raise of the Volume above three quarters goes no faster than the glide.
    if !watching.app.paces(&intent, Instant::now()) {
        return;
    }
    if watching.app.session.holding.is_some() {
        device_presets::unsent(&mut watching.app, &intent);
        watching.app.warn(NO_DEVICE_OPEN);
        return;
    }
    let Some(backend) = watching.backend.as_mut() else {
        device_presets::unsent(&mut watching.app, &intent);
        watching.app.warn(LINK_LOST);
        return;
    };
    if let Err(e) = execute(intent.clone(), backend, &mut watching.app).await {
        device_presets::unsent(&mut watching.app, &intent);
        watching.app.write_failed(&e.failure());
    }
}

/// Send one intent to the device, or fold a refresh's readings into the app.
///
/// Every write goes out without waiting for its echo: the device reports the value it settled
/// on over the update feed, and the app takes it there like any other reading.
pub(super) async fn execute(
    intent: Intent,
    backend: &mut Backend,
    app: &mut App,
) -> Result<(), WriteError> {
    match intent {
        Intent::Set { key, value, step } => {
            backend.set(&key, &value, step, true).await?;
        }
        Intent::Raw {
            id,
            value_type,
            value,
        } => {
            backend.raw(&id, &value_type, &value, true).await?;
        }
        Intent::EqSet { band, param, value } => {
            backend.eq_set(band, &param, value, true).await?;
        }
        Intent::EqMode(mode) => {
            backend.eq_mode(&mode, true).await?;
        }
        Intent::Recalled(name) => backend.note_recalled(&name).await,
        Intent::Refresh => refresh(backend, app, Instant::now()).await,
        Intent::Identify(device) => {
            let done = backend.identify_named(device.as_deref()).await?;
            app.notify(crate::format::identify_line(&done.device, done.sent));
        }
        // The loop takes these itself: none is a write to the device, and each needs something
        // it holds — the terminal, the configuration file, the devices to switch between, or the
        // host's own audio.
        Intent::SwitchDevice(_)
        | Intent::NewDevice
        | Intent::UpdateDevice(..)
        | Intent::RemoveDevice(_)
        | Intent::DefaultDevice(_)
        | Intent::SavePreset(_)
        | Intent::UpdatePreset(..)
        | Intent::RenamePreset(..)
        | Intent::RemovePreset(_)
        | Intent::ReadPresets
        | Intent::AutoEqIndex { .. }
        | Intent::AutoEqDownload(_)
        | Intent::HostEq(_)
        | Intent::MoveToDac(_)
        | Intent::Tui(_)
        | Intent::AutoEqRefresh(_)
        | Intent::Connection(_)
        | Intent::RevealToken
        | Intent::CopyToken
        | Intent::RotateToken
        | Intent::WebLink
        | Intent::WebPair
        | Intent::CopyWebLink
        | Intent::RotateWebToken
        | Intent::OpenWebLink
        | Intent::HostVolume(_)
        | Intent::HostMute
        | Intent::HostPress(_)
        | Intent::Quit => {}
    }
    Ok(())
}
