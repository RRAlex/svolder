// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One discover-and-connect attempt: scan for the device, resolve its profile and passphrase,
//! and open the session.
//!
//! It runs in a task of its own while the actor keeps answering commands, and hands its session
//! and findings back for the actor to absorb.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::StreamExt;
use svolder_ble::{Advertisement, Episode, Session, SessionConfig, Transport};
use svolder_ipc::config::DeviceConfig;
use svolder_profiles::Registry;
use svolder_proto::{Passphrase, UnitId};

use super::entry::resolve_profile;
use super::history::wall_ms;
use super::link::AttemptEnd;
use super::passphrase::{REFUSED_LINE, resolve_passphrase};
use super::{Actor, Command, Commands};
use crate::log;
use crate::scanner::Scanner;

/// What a discover-and-connect attempt hands back to the actor.
///
/// The session travels into the attempt and back out again whether the attempt succeeded or
/// not, because its `DeviceState` is what reads answer from once the actor has it again.
pub(super) struct Attempt {
    /// The advertisement the attempt found, or `None` when it found none.
    pub(super) advert: Option<Advertisement>,
    /// When a listen of this daemon's heard `advert`, in unix milliseconds; `None` for one taken
    /// from the adapter's held list, whose signal strength is a cached value, not a hearing.
    pub(super) heard_at_ms: Option<u64>,
    /// The session, whether or not the connect succeeded.
    pub(super) session: Option<Session>,
    /// How the connect went, with the reason to log when it failed.
    pub(super) outcome: Result<(), String>,
    /// Where the refusal episode stands once the attempt is over: an attempt can spend the
    /// episode's drop itself, when the device refuses its connect.
    pub(super) episode: Episode,
    /// The passphrase entry the attempt ran under, which says whether the entry changed while
    /// it ran and so ended the episode the attempt carried.
    pub(super) entry: String,
}

/// What the actor hands a connect attempt to carry along: the session it already holds, if any,
/// and where the device's refusal episode stands.
pub(super) struct Carried {
    /// The session the actor held when the attempt started, if it held one.
    pub(super) session: Option<Session>,
    /// Where the refusal episode stood when the attempt started.
    pub(super) episode: Episode,
}

impl Actor {
    /// Run one discover-and-connect attempt in a task, serving commands until it finishes.
    ///
    /// The session moves into the task and comes back through the `JoinHandle`, so the
    /// `select!` below owns nothing the attempt is using. A `JoinHandle` is cancel-safe and
    /// its task keeps running when the arm is dropped, so no connect is ever orphaned; the
    /// readings the attempt took away are still answered from `last_state`.
    pub(super) async fn attempt(&mut self, rx: &mut Commands) -> AttemptEnd {
        // There is nothing to wait for while the attempt this starts is running.
        self.set_next_attempt(None);
        if let Some(session) = self.session.as_ref() {
            self.last_state = session.state().clone();
        }
        let grace = self.close_grace();
        let cancel = Arc::new(AtomicBool::new(false));
        let mut handle = tokio::spawn(connect_attempt(
            Arc::clone(&self.transport),
            Arc::clone(&self.scanner),
            self.unit,
            Arc::clone(&self.registry),
            self.config.clone(),
            Carried {
                session: self.session.take(),
                episode: self.episode,
            },
            Arc::clone(&cancel),
        ));
        loop {
            tokio::select! {
                finished = &mut handle => {
                    return match finished {
                        Ok(attempt) => self.absorb(attempt),
                        Err(e) => AttemptEnd::Failed(format!("the connect task failed: {e}")),
                    };
                }
                incoming = rx.recv() => match incoming {
                    None => {
                        self.reclaim(handle, &cancel, grace).await;
                        return AttemptEnd::Stop;
                    }
                    Some((Command::Shutdown, reply)) => {
                        self.farewell = Some(reply);
                        self.reclaim(handle, &cancel, grace).await;
                        return AttemptEnd::Stop;
                    }
                    Some((command, reply)) => {
                        let response = self.run_command(command).await;
                        let _ = reply.send(response);
                    }
                },
            }
        }
    }

    /// How long a shutting-down actor waits for a connect attempt to hand its session back:
    /// the session's own grace, or the default until there is a session to ask.
    ///
    /// Read before the session is handed to an attempt, since the attempt takes it along.
    pub(super) fn close_grace(&self) -> Duration {
        self.session.as_ref().map_or_else(
            || SessionConfig::default().close_grace,
            |session| session.config().close_grace,
        )
    }

    /// Take an attempt's findings: the advertisement, the profile resolved against the
    /// registry the actor holds *now* (a reload may have replaced the one the attempt used),
    /// and the session, whether or not the connect succeeded.
    ///
    /// The attempt carried copies of the entry and registry from when it was spawned; a
    /// reload that overtook it is applied here instead, so the profile and the session's
    /// passphrase never lag a reload behind the old entry.
    pub(super) fn absorb(&mut self, attempt: Attempt) -> AttemptEnd {
        self.session = attempt.session;
        // An entry changed while the attempt ran ended the episode it was carrying.
        self.episode = match attempt.entry == self.config.passphrase {
            true => attempt.episode,
            false => Episode::default(),
        };
        if let Some(advert) = attempt.advert {
            self.update_identity(&advert, attempt.heard_at_ms);
            self.profile = resolve_profile(&self.registry, &self.config, &advert);
            self.publish_profile();
            if let Err(message) = self.rekey(&advert) {
                log::warn(&format!("{}: {message}", self.name));
            }
            self.advert = Some(advert);
        }
        self.judge_passphrase();
        // A reload that landed during the attempt changed the profile while the session was
        // away, and an attempt that found nothing hands the session back unchanged. Until an
        // advertisement has been seen the actor's profile is a placeholder, which no session
        // is handed.
        if self.advert.is_some() {
            self.sync_session_profile();
        }
        match attempt.outcome {
            Ok(()) => AttemptEnd::Connected,
            Err(message) => AttemptEnd::Failed(message),
        }
    }

    /// Wait out an attempt the actor is no longer interested in, so a session it opened is
    /// back in hand to be closed.
    ///
    /// The attempt is told to stand down before the wait starts. One that finishes within
    /// `grace` closes its own link and hands back a session with nothing left to release. One
    /// still running when `grace` runs out is aborted without reading the flag, and the
    /// transport is given the same grace to close whatever connection it was still opening —
    /// otherwise a stop while the device is off would leave BlueZ holding a connection this
    /// daemon can no longer release. The aborted attempt is waited for, within the same grace,
    /// before that settle, since dropping it is what records its releases.
    pub(super) async fn reclaim(
        &mut self,
        mut handle: tokio::task::JoinHandle<Attempt>,
        cancel: &AtomicBool,
        grace: Duration,
    ) {
        cancel.store(true, Ordering::SeqCst);
        match tokio::time::timeout(grace, &mut handle).await {
            Ok(Ok(attempt)) => self.session = attempt.session,
            Ok(Err(e)) => log::debug(&format!("{}: the connect task failed: {e}", self.name)),
            Err(_) => {
                log::warn(&format!(
                    "{}: stopping a connect attempt still running after {grace:?}",
                    self.name
                ));
                handle.abort();
                // The abort takes effect once the attempt's future is dropped, which can be
                // after this task yields, and dropping it may record a release: wait for the
                // drop before looking at what the transport has in flight.
                let _ = tokio::time::timeout(grace, &mut handle).await;
                self.transport.settle_for(self.unit, grace).await;
            }
        }
    }
}

/// One discover-and-connect attempt, run as its own task so the actor stays responsive.
///
/// Everything it needs is owned and travels with it; on return it hands back both the
/// advertisement and the session regardless of outcome, so a failed attempt still keeps the
/// readings and link state gathered.
///
/// A device on `auto` that refuses the code its advertisement carried, as the connect's refresh
/// finds out, has the link released and is listened for afresh, and the session is keyed from
/// what that listen hears and connected again, once: `episode` says whether the drop is spent
/// already, and the one handed back says it now is.
///
/// `cancel`, when raised before the connect finishes, closes the session here instead of
/// handing it back live.
pub(super) async fn connect_attempt(
    transport: Arc<dyn Transport>,
    scanner: Arc<Scanner>,
    unit: UnitId,
    registry: Arc<Registry>,
    config: DeviceConfig,
    carried: Carried,
    cancel: Arc<AtomicBool>,
) -> Attempt {
    let auto = config.passphrase == "auto";
    let Carried {
        session: mut existing,
        mut episode,
    } = carried;
    loop {
        let (advert, heard_at_ms) =
            match discover(&transport, &scanner, unit, episode.listen_against()).await {
                Ok(found) => found,
                Err(message) => {
                    return Attempt {
                        advert: None,
                        heard_at_ms: None,
                        session: existing,
                        outcome: Err(message),
                        episode,
                        entry: config.passphrase,
                    };
                }
            };
        if cancel.load(Ordering::SeqCst) {
            // The actor stopped waiting while this attempt listened: connect nothing, hand
            // back what it carried.
            return Attempt {
                advert: Some(advert),
                heard_at_ms,
                session: existing,
                outcome: Err("stopped before connecting".to_string()),
                episode,
                entry: config.passphrase,
            };
        }
        // A connect that is not the one after a drop reads whatever code the device advertises
        // now, which is another episode when it is not the one that was refused.
        if auto && episode.listen_against().is_none() {
            episode.code_seen(advert.manufacturer.passphrase());
        }
        episode.listened();
        let mut session = match existing.take() {
            Some(mut session) => {
                // `auto` means the passphrase is whatever the advertisement carries now, so a
                // reused session takes it from this attempt's advert: a passphrase changed from
                // the official app while the device was away would otherwise leave every frame
                // this session sends encoded with the old one.
                if auto {
                    session.set_passphrase(advert.manufacturer.passphrase());
                    session.set_advertisement(advert.clone());
                }
                session
            }
            None => {
                let passphrase = match resolve_passphrase(&config, &advert) {
                    Ok(passphrase) => passphrase,
                    Err(message) => {
                        return Attempt {
                            advert: Some(advert),
                            heard_at_ms,
                            session: None,
                            outcome: Err(message),
                            episode,
                            entry: config.passphrase,
                        };
                    }
                };
                let profile = resolve_profile(&registry, &config, &advert);
                Session::new(Arc::clone(&transport), advert.clone(), profile, passphrase)
            }
        };
        let outcome = session.connect().await.map_err(|e| e.to_string());
        if cancel.load(Ordering::SeqCst) {
            // The actor stopped waiting for this attempt: nothing else can release a link this
            // connect opened, so close it here before handing the session back.
            session.close().await;
        } else if outcome.is_ok() && episode.due(auto, &session) {
            episode.dropped(session.passphrase());
            log::warn(&format!("{}: {REFUSED_LINE}", config.name));
            session.close().await;
            existing = Some(session);
            continue;
        }
        if outcome.is_ok() && session.refused() {
            episode.refused_again(session.passphrase());
        }
        return Attempt {
            advert: Some(advert),
            heard_at_ms,
            session: Some(session),
            outcome,
            episode,
            entry: config.passphrase,
        };
    }
}

/// Find `unit` through the scanner: the listen it shares with every other actor looking for its
/// device, or, when the link was just released over a code the device refused, a fresh listen
/// of its own ([`Scanner::listen_afresh`]). A miss falls back to the advertisement the adapter
/// holds, whose profile and passphrase are enough, since the connect that follows settles
/// whether the device is still there.
///
/// The advertisement comes with the wall-clock moment a listen handed it over, or `None` when it
/// came from the held list: a held advertisement's signal strength is whatever the adapter last
/// cached, so it says nothing of when the device was heard.
///
/// A free function rather than a method: an `&Actor` held across an await would have to be
/// `Sync`, and the session's notification stream is not.
pub(super) async fn discover(
    transport: &Arc<dyn Transport>,
    scanner: &Scanner,
    unit: UnitId,
    refused: Option<Passphrase>,
) -> Result<(Advertisement, Option<u64>), String> {
    let live = match refused {
        Some(code) => scanner.listen_afresh(unit, code).await,
        None => scanner.sight(unit).await,
    }?;
    if let Some(advert) = live {
        return Ok((advert, Some(wall_ms())));
    }
    let held = transport.held().await.map_err(|e| e.to_string())?;
    first_with_unit(held, unit)
        .await
        .map(|advert| (advert, None))
        .ok_or_else(|| format!("device {unit} is not in range"))
}

/// The first advertisement on `adverts` that carries `unit`.
pub(super) async fn first_with_unit(
    mut adverts: svolder_ble::Advertisements,
    unit: UnitId,
) -> Option<Advertisement> {
    while let Some(advert) = adverts.next().await {
        if advert.unit_id() == unit {
            return Some(advert);
        }
    }
    None
}
