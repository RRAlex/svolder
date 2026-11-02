// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the loop does for the Web UI box: ask the daemon for a link or a pairing code over a
//! connection of its own, take its answer in with the base a phone reaches, replace a link that
//! ends or that a browser has used while the box is open, copy the link or a pairing code (by
//! OSC 52 and the display's clipboard tool) and open the link in a browser, and replace the web
//! token.

use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, Instant};

use svolder_ipc::{Request, Response};

use super::Look;
use super::Watching;
use super::look::{Browser, Copier};
use crate::commands::web::{
    LOGIN_FRAGMENT, first_lan, interfaces, local_base, open_link, phone_base,
};
use crate::tui::boxes::connections::{Modal, osc52};
use crate::tui::boxes::web_box::Code;
use crate::tui::clipboard::{self, Tried};

/// What a copy for the box came out as, with what it copied: said when it arrives.
pub(super) struct Copied {
    /// What was copied: `link`, `code` or `token`.
    what: &'static str,
    /// Why the terminal could not be written to, if it could not.
    terminal: Option<String>,
    /// How the display's clipboard tool did.
    tried: Tried,
    /// How the browser did, for a click that opened one.
    opened: Option<Result<(), String>>,
}

/// What a code ask came back with: whether it was for a link, and the daemon's answer.
pub(super) type Asked = (bool, Option<Response>);

/// What a check on a link came back with: the link's code, and the daemon's answer.
pub(super) type Checked = (String, Option<Response>);

/// How often the daemon is asked whether the link on show is still open.
pub(super) const CHECK_PERIOD: Duration = Duration::from_secs(1);

/// Ask the daemon for a link (`link`) or a pairing code, unless one is in flight or the box is
/// closed; the answer lands on [`Watching::web_code`].
pub(super) fn ask(watching: &mut Watching, link: bool) {
    if watching.web_code.is_some()
        || !matches!(watching.app.overlays.conn.modal, Some(Modal::Web(_)))
    {
        return;
    }
    let request = match link {
        true => Request::WebLink,
        false => Request::WebPair,
    };
    let Some(answer) = watching
        .backend
        .as_ref()
        .and_then(|b| b.daemon_request(request))
    else {
        watching
            .app
            .warn("the daemon serves the web UI: start it to make a link or a code");
        return;
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _ = tx.send((link, answer.await));
    });
    watching.web_code = Some(rx);
}

/// The code of the link on show that still has more than a check's period left at `now`, while
/// the box is open: a code that has just ended is not a code a browser used.
fn live_link_code(watching: &Watching, now: Instant) -> Option<String> {
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        return None;
    };
    web.link
        .as_ref()
        .filter(|link| link.until > now + CHECK_PERIOD)
        .and_then(Code::link_code)
        .map(str::to_string)
}

/// The code of the link on show to ask the daemon about: a live one the daemon has not failed to
/// answer for.
fn watched_link_code(watching: &Watching, now: Instant) -> Option<String> {
    match &watching.app.overlays.conn.modal {
        Some(Modal::Web(web)) if !web.link_unwatched => live_link_code(watching, now),
        _ => None,
    }
}

/// Whether the loop has a link on show to ask the daemon about, with no check on it in flight.
pub(super) fn checking(watching: &Watching, now: Instant) -> bool {
    watching.web_check.is_none() && watched_link_code(watching, now).is_some()
}

/// Ask the daemon whether the link on show is still open, over a connection of its own; the
/// answer lands on [`Watching::web_check`].
pub(super) fn check(watching: &mut Watching, now: Instant) {
    if watching.web_check.is_some() {
        return;
    }
    let Some(code) = watched_link_code(watching, now) else {
        return;
    };
    let request = Request::WebLinkOpen { code: code.clone() };
    let Some(answer) = watching
        .backend
        .as_ref()
        .and_then(|b| b.daemon_request(request))
    else {
        return;
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _ = tx.send((code, answer.await));
    });
    watching.web_check = Some(rx);
}

/// Take a check in: a link the daemon says a browser has used, while it still shows and has not
/// ended, is replaced by a fresh one, which the box says once it arrives. An error (a daemon that
/// does not know the request, or a web UI turned off) stops the checks on that link, no answer
/// (a daemon restarting) leaves them to ask again, and an answer about a link no longer on show
/// changes nothing; a link that ended stays for [`tick`] to replace.
pub(super) fn checked(watching: &mut Watching, (code, answer): Checked, now: Instant) {
    if live_link_code(watching, now).as_deref() != Some(code.as_str()) {
        return;
    }
    match answer {
        Some(Response::WebLinkOpen { open: false }) if watching.web_code.is_none() => {
            ask(watching, true);
            if watching.web_code.is_some()
                && let Some(Modal::Web(web)) = watching.app.overlays.conn.modal.as_mut()
            {
                web.link_replacing = true;
            }
        }
        Some(Response::WebLinkOpen { .. }) | None => {}
        Some(_) => {
            if let Some(Modal::Web(web)) = watching.app.overlays.conn.modal.as_mut() {
                web.link_unwatched = true;
            }
        }
    }
}

/// A key's ask for a code: refused with a word while another ask is still on its way.
pub(super) fn asked_by_key(watching: &mut Watching, link: bool) {
    if watching.web_code.is_some() {
        watching
            .app
            .warn("already asking the daemon for a code: try again in a moment");
        return;
    }
    ask(watching, link);
}

/// The address the daemon listens on, or this file's while it has not said, with the file's.
fn listens(conn: &crate::tui::boxes::connections::Connections) -> (SocketAddr, SocketAddr) {
    match &conn.web_status {
        Some(svolder_ipc::WebStatus::Listening { listen, .. }) => (conn.web.listen, *listen),
        _ => (conn.web.listen, conn.web.listen),
    }
}

/// Take a code answer in: a link is shown with the base a phone reaches (this computer's, marked
/// as such, when a phone cannot reach it), a pairing code as it is; a refusal is said, and a
/// link that could not be replaced goes, so it is not asked for again. Answers whether a code
/// came in.
pub(super) fn answered(watching: &mut Watching, (link, answer): Asked, now: Instant) -> bool {
    let conn = &watching.app.overlays.conn;
    let made_for = listens(conn);
    let (_, listen) = made_for;
    let phone = phone_base(conn.web.url.as_deref(), listen, first_lan(&interfaces()));
    let local = phone.is_err();
    let base = phone.unwrap_or_else(|_| local_base(listen));
    let code = match answer {
        Some(Response::WebLink {
            code,
            expires_in_secs,
        }) => Code {
            text: format!("{base}{LOGIN_FRAGMENT}{code}"),
            until: now + Duration::from_secs(expires_in_secs),
        },
        Some(Response::WebPair {
            code,
            expires_in_secs,
        }) => Code {
            text: code,
            until: now + Duration::from_secs(expires_in_secs),
        },
        Some(Response::Error { message, .. }) => {
            refused(watching, link, message);
            return false;
        }
        _ => {
            refused(watching, link, "the daemon did not answer".to_string());
            return false;
        }
    };
    if let Some(Modal::Web(web)) = watching.app.overlays.conn.modal.as_mut() {
        match link {
            true => {
                web.link = Some(code);
                web.link_for = Some(made_for);
                web.link_local = local;
                web.link_replaced = std::mem::take(&mut web.link_replacing);
                web.link_unwatched = false;
            }
            false => web.pair = Some(code),
        }
    }
    true
}

/// Take a code answer in as [`answered`] does, and copy a pairing code that came in, as it is
/// shown.
pub(super) fn take_answer(watching: &mut Watching, look: &mut Look, asked: Asked, now: Instant) {
    let pairing = !asked.0;
    if answered(watching, asked, now) && pairing {
        copy_pairing(watching, look);
    }
}

/// Say why a code was refused; a link that could not be made or replaced is dropped.
fn refused(watching: &mut Watching, link: bool, words: String) {
    if link && let Some(Modal::Web(web)) = watching.app.overlays.conn.modal.as_mut() {
        web.link = None;
        web.link_for = None;
        web.link_local = false;
        web.link_replaced = false;
        web.link_replacing = false;
    }
    watching.app.warn(words);
}

/// Drop the link and the pairing code the box shows, and an ask for one still on its way: the
/// token that signed them is gone.
pub(super) fn forget_codes(watching: &mut Watching) {
    watching.web_code = None;
    watching.web_check = None;
    if let Some(Modal::Web(web)) = watching.app.overlays.conn.modal.as_mut() {
        web.link = None;
        web.link_for = None;
        web.link_local = false;
        web.link_replaced = false;
        web.link_replacing = false;
        web.pair = None;
    }
}

/// Once a second while the box is open: a link that has ended, or that was made for another
/// listen address than the one now, is replaced.
pub(super) fn tick(watching: &mut Watching, now: Instant) {
    let listening = listens(&watching.app.overlays.conn);
    let stale = match &watching.app.overlays.conn.modal {
        Some(Modal::Web(web)) => web
            .link
            .as_ref()
            .is_some_and(|link| link.until <= now || web.link_for != Some(listening)),
        _ => false,
    };
    if stale {
        ask(watching, true);
    }
}

/// What a user can do by hand with a link, a code or the MCP token where the terminal ignores
/// OSC 52.
fn by_hand(what: &str) -> &'static str {
    match what {
        "link" => "Ctrl+click the link to open it, Shift+right-click it to copy it",
        "token" => "svolder mcp token prints it",
        _ => "read it off the box",
    }
}

/// A copy waiting for the one in flight to end: only the newest waits.
pub(super) struct Next {
    /// What it copies: `link`, `code` or `token`.
    what: &'static str,
    /// The text for the tool.
    text: String,
    /// Why the terminal could not be written to, if it could not.
    terminal: Option<String>,
    /// The tool's seam.
    copier: Copier,
    /// The browser's seam, for a click that opens one.
    browser: Option<Browser>,
}

/// Start the tool (and, with `browser`, the browser) on `text` on a thread of its own, its
/// outcome landing on [`Watching::web_copy`].
fn start(
    watching: &mut Watching,
    what: &'static str,
    text: String,
    terminal: Option<String>,
    copier: Copier,
    browser: Option<Browser>,
) {
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::task::spawn_blocking(move || {
        let tried = copier(&text);
        let opened = browser.map(|browser| browser(&text));
        let _ = tx.send(Copied {
            what,
            terminal,
            tried,
            opened,
        });
    });
    watching.web_copy = Some(rx);
}

/// Send `text` (a `link`, a `code` or the MCP `token`; the text itself is never put in a note) to
/// the terminal's clipboard as OSC 52 (wrapped for tmux and screen) at once, and to the display's
/// clipboard tool on a thread of its own (a tool can take seconds), and when `open` also to the
/// browser; what happened is said when [`copied`] gets it. A copy made while another is on its
/// way sends the sequence at once and waits its turn for the tool (and the browser) and says so;
/// only the newest waits.
pub(super) fn send(
    watching: &mut Watching,
    look: &mut Look,
    what: &'static str,
    text: String,
    open: bool,
) {
    let sequence = clipboard::wrap(&osc52(&text), &*look.env);
    let terminal = (look.clipboard)(&sequence).err().map(|e| e.to_string());
    let copier = std::sync::Arc::clone(&look.copier);
    let browser = open.then(|| std::sync::Arc::clone(&look.browser));
    if watching.web_copy.is_some() {
        watching.web_copy_next = Some(Next {
            what,
            text,
            terminal,
            copier,
            browser,
        });
        watching.app.notify(match open {
            true => format!(
                "{what} sent to the terminal; the clipboard tool and the browser get it \
                 when the last copy ends"
            ),
            false => format!(
                "{what} sent to the terminal; the clipboard tool gets it when the last copy ends"
            ),
        });
        return;
    }
    watching.app.notify(match open {
        true => format!("copying the {what} and opening the browser…"),
        false => format!("copying the {what}…"),
    });
    start(watching, what, text, terminal, copier, browser);
}

/// Copy the link on show (`c`), through [`send`].
pub(super) fn copy(watching: &mut Watching, look: &mut Look) {
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        return;
    };
    let Some(link) = web.link.as_ref().map(|link| link.text.clone()) else {
        return;
    };
    send(watching, look, "link", link, false);
}

/// Copy the pairing code on show, as it is not a link: through [`send`], and nothing is opened.
pub(super) fn copy_pairing(watching: &mut Watching, look: &mut Look) {
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        return;
    };
    let now = watching.app.now;
    let Some(code) = web
        .pair
        .as_ref()
        .filter(|code| code.until > now)
        .map(|code| code.text.clone())
    else {
        return;
    };
    send(watching, look, "code", code, false);
}

/// Whether a desktop session is there for an opener to show a browser in: `DISPLAY` or
/// `WAYLAND_DISPLAY` is set and not empty, as `var` reads them.
pub(in crate::tui) fn desktop_session(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> bool {
    ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .any(|name| var(name).is_some_and(|value| !value.is_empty()))
}

/// Hand `link` to `opener` without letting it near the terminal (its streams go to null, in a
/// process group of its own), or say `no desktop` without starting it when there is no desktop
/// session, where `xdg-open` would run a terminal browser in the foreground.
pub(in crate::tui) fn open_in_browser(
    desktop: bool,
    opener: &Path,
    link: &str,
) -> Result<(), String> {
    match desktop {
        true => open_link(opener, link),
        false => Err("no desktop".to_string()),
    }
}

/// A click on the link: copy it as [`copy`] does and try to open it in a browser, both on a
/// thread of their own (an opener can take seconds), whose outcome [`copied`] says when it
/// arrives. The link is copied where no browser opens. A click with no link on show, or an ended
/// one, does nothing.
pub(super) fn open(watching: &mut Watching, look: &mut Look) {
    let Some(Modal::Web(web)) = &watching.app.overlays.conn.modal else {
        return;
    };
    let now = watching.app.now;
    let Some(link) = web
        .link
        .as_ref()
        .filter(|link| link.until > now)
        .map(|link| link.text.clone())
    else {
        return;
    };
    send(watching, look, "link", link, true);
}

/// Say how a copy came out, and the browser with it when one was opened.
pub(super) fn copied(watching: &mut Watching, done: Copied) {
    let note = clipboard::note(
        done.what,
        &done.tried,
        done.terminal.as_deref(),
        by_hand(done.what),
    );
    watching.app.notify(match done.opened {
        Some(Ok(())) => format!("{note}; opened in the browser"),
        Some(Err(why)) => format!("{note}; no browser was opened ({why})"),
        None => note,
    });
    // A copy that came in meanwhile gets its turn at the tool now.
    if let Some(next) = watching.web_copy_next.take() {
        start(
            watching,
            next.what,
            next.text,
            next.terminal,
            next.copier,
            next.browser,
        );
    }
}

/// Take in how the copy that was being made ended: said as [`copied`] does, or, when the task
/// ended without an answer, the copy waiting behind it dropped, so that it never runs after a
/// newer one.
pub(super) fn copy_ended(watching: &mut Watching, done: Option<Copied>) {
    match done {
        Some(done) => copied(watching, done),
        None => watching.web_copy_next = None,
    }
}

/// Wait for the copy being made on `rx`, while one is; the `select!` arm reading this is
/// guarded to never poll it with none.
pub(super) async fn recv_copy(
    rx: &mut Option<tokio::sync::oneshot::Receiver<Copied>>,
) -> Option<Copied> {
    match rx {
        Some(receiver) => receiver.await.ok(),
        None => std::future::pending().await,
    }
}

/// Wait for the link check on `rx`, while one is in flight; the `select!` arm reading this is
/// guarded to never poll it with none.
pub(super) async fn recv_check(
    rx: &mut Option<tokio::sync::oneshot::Receiver<Checked>>,
) -> Option<Checked> {
    match rx {
        Some(receiver) => receiver.await.ok(),
        None => std::future::pending().await,
    }
}

/// Wait for the code ask on `rx`, while one is in flight; the `select!` arm reading this is
/// guarded to never poll it with none.
pub(super) async fn recv(rx: &mut Option<tokio::sync::oneshot::Receiver<Asked>>) -> Option<Asked> {
    match rx {
        Some(receiver) => receiver.await.ok(),
        None => std::future::pending().await,
    }
}

/// Wait for the copy being made and say how it came out (tests, and nothing else, wait).
#[cfg(test)]
pub(super) async fn settle_copy(watching: &mut Watching) {
    if let Some(rx) = watching.web_copy.take()
        && let Ok(done) = rx.await
    {
        copied(watching, done);
    }
}

/// Wait for the link check in flight and take its answer in (tests, and nothing else, wait).
#[cfg(test)]
pub(super) async fn settle_check(watching: &mut Watching) {
    if let Some(rx) = watching.web_check.take()
        && let Ok(done) = rx.await
    {
        checked(watching, done, Instant::now());
    }
}

/// Wait for the code ask in flight and take its answer in (tests, and nothing else, wait).
#[cfg(test)]
pub(super) async fn settle(watching: &mut Watching) {
    if let Some(rx) = watching.web_code.take()
        && let Ok(asked) = rx.await
    {
        answered(watching, asked, Instant::now());
    }
}

/// [`settle`], copying a pairing code that comes in as the loop does (tests, and nothing else).
#[cfg(test)]
pub(super) async fn settle_and_copy(watching: &mut Watching, look: &mut Look) {
    if let Some(rx) = watching.web_code.take()
        && let Ok(asked) = rx.await
    {
        take_answer(watching, look, asked, Instant::now());
    }
}

#[cfg(test)]
mod tests;
