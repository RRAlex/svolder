// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a device on `auto` that refuses its code is given: one drop of the link and one fresh
//! reading of the code, never more for the same refusal.

use std::time::Duration;

use svolder_proto::{Passphrase, UnitId};
use tokio::time::Instant;

use crate::advert::Advertisement;
use crate::error::BleError;
use crate::session::Session;
use crate::transport::Transport;

/// Where one refusal episode stands, kept by whoever supervises a device's link.
///
/// An episode is the run of refusals one wrong code earns. Under `auto` the first refusal on a
/// live link is answered by releasing the link, listening for the device's next advertisement
/// and connecting again with the code it carries ([`Episode::due`]). That is all the episode
/// ever does: a link refused again after it is kept, as a link refused under a code the user
/// typed is, until the episode ends. It ends when
///
/// - the device shows a code works that is not the one it refused ([`Episode::observe`]),
/// - the entry changes ([`Episode::end`], called by whoever adopts a changed entry),
/// - or a connect reads another code than the refused one from the device's advertisement
///   ([`Episode::code_seen`]).
///
/// A connect that fails after the drop does not end it: the device being away says nothing
/// about the code. Nor does a link the episode left standing dropping on its own: what connects
/// next is refused under the same code, and that is the same refusal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Episode {
    /// Whether the episode has already used its one drop.
    spent: bool,
    /// The code the episode's latest refusal was of: what the episode is about, until a
    /// different code is read or proven.
    refused: Option<Passphrase>,
    /// The code the drop was made over, until the connect that follows has listened for the
    /// device's next advertisement.
    listen: Option<Passphrase>,
}

impl Episode {
    /// Whether `session`'s device has refused its code and the episode still has its drop to
    /// spend. `auto` says whether the device's entry is `auto`: any other entry never drops.
    pub fn due(&self, auto: bool, session: &Session) -> bool {
        auto && !self.spent && session.refused()
    }

    /// Record that the link was released over `refused`: the drop is spent, and the next connect
    /// is to listen for a fresh advertisement first.
    pub fn dropped(&mut self, refused: Passphrase) {
        self.spent = true;
        self.refused = Some(refused);
        self.listen = Some(refused);
    }

    /// Record that the link made after the drop was refused too, under `code`: that is the code
    /// the episode is about from now on.
    pub fn refused_again(&mut self, code: Passphrase) {
        if self.spent {
            self.refused = Some(code);
        }
    }

    /// Say that a connect that did not follow a drop is about to key the session with `code`,
    /// read from the device's advertisement. A code other than the one refused ends the episode:
    /// the device's code has changed since.
    pub fn code_seen(&mut self, code: Passphrase) {
        if self.spent && self.refused != Some(code) {
            self.end();
        }
    }

    /// The code the next connect has to listen for a fresh advertisement against, until
    /// [`Episode::listened`] says it did.
    pub fn listen_against(&self) -> Option<Passphrase> {
        self.listen
    }

    /// Record that a connect has listened, so the connects after it need not.
    pub fn listened(&mut self) {
        self.listen = None;
    }

    /// End the episode: the next refusal is a new one.
    pub fn end(&mut self) {
        *self = Self::default();
    }

    /// End the episode when `session` has shown a code works that is not the one the episode is
    /// about. See [`Session::code_accepted`]. A device that answers a refresh in full under the
    /// code it refuses a write under proves nothing about it.
    pub fn observe(&mut self, session: &Session) {
        if session.code_accepted() && self.refused != Some(session.passphrase()) {
            self.end();
        }
    }
}

/// The device's next advertisement within `window`, for a caller that has just released a link
/// the device refused its code on, in place of the code `refused` that advertisement carried.
///
/// The first advertisement [`Transport::sight`] hears ends the listen when it carries a code
/// other than `refused`. One that carries `refused` again is not taken at its word: a transport
/// that lists a device when something about it changes, and not only when it advertises, can
/// hand over the advertisement it held from before the release, so the listen goes on for the
/// rest of `window` and the first advertisement after that is the answer, whatever code it
/// carries. A window that ends with only the one that repeated `refused` answers with that one,
/// and one that ends with nothing answers `None`. A listen that fails after one was heard
/// answers with the one heard.
///
/// What counts as heard is up to the transport: `sight` reports a device the radio heard while
/// the listen ran, never one the adapter merely remembers, and that is what makes the answer
/// fresh.
pub async fn listen_afresh(
    transport: &dyn Transport,
    unit: UnitId,
    refused: Passphrase,
    window: Duration,
) -> Result<Option<Advertisement>, BleError> {
    let deadline = Instant::now() + window;
    let Some(first) = transport.sight(unit, window).await? else {
        return Ok(None);
    };
    if first.manufacturer.passphrase() != refused {
        return Ok(Some(first));
    }
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Ok(Some(first));
    }
    match transport.sight(unit, left).await {
        Ok(Some(second)) => Ok(Some(second)),
        _ => Ok(Some(first)),
    }
}
