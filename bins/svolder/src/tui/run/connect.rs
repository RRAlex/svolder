// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the Settings box's Connect category asks of the loop: write one key of `[mcp]` or
//! `[mqtt]` and tell the daemon to reload, ask the daemon how its two servers stand, and the
//! three things done with the token that only the loop may touch — showing it, copying it to
//! the terminal's clipboard and replacing it.
//!
//! The token is read here, from the store, at the moment it is wanted, and what is kept of it
//! is its masked tail. A message made here names a source or a path, never the token.
//!
//! The loop's link to the daemon is here too: the wait for a daemon to answer on its socket
//! again after the connection to it closed, the device opened on it when it does, and the
//! version a daemon reports when it is first met.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use svolder_ipc::Config;
use svolder_ipc::token::{self, Store, Token, TokenError};

use super::{
    Backend, DAEMON_POLL_FIRST, DAEMON_POLL_MAX, DaemonInfo, Flow, Look, Open, Opener, RECONNECTED,
    Shown, Switched, Watching, gave_up, switch_via,
};
use crate::tui::app::ConnChange;
use crate::tui::boxes::connections::{REVEAL_FOR, TokenState};

/// Where the token is looked for: the environment's value and the state home, taken once when
/// the session starts, so a case can point both somewhere of its own.
#[derive(Debug, Clone)]
pub(in crate::tui) struct TokenSite {
    /// `SVOLDER_MCP_TOKEN` as the session started with it.
    pub(in crate::tui) env: Option<String>,
    /// Why the variable could not be read (it is not UTF-8), which every read of the token
    /// answers instead of falling back to a file, as the daemon's does.
    pub(in crate::tui) env_error: Option<String>,
    /// `SVOLDER_WEB_TOKEN` as the session started with it.
    pub(in crate::tui) web_env: Option<String>,
    /// Why `SVOLDER_WEB_TOKEN` could not be read, as `env_error` says for the MCP token's.
    pub(in crate::tui) web_env_error: Option<String>,
    /// The state home the generated token file lives under.
    pub(in crate::tui) state_home: PathBuf,
}

impl TokenSite {
    /// The site this process sees.
    pub(in crate::tui) fn process() -> TokenSite {
        let (env, env_error) = match token::env_token(Store::MCP) {
            Ok(env) => (env, None),
            Err(e) => (None, Some(e.to_string())),
        };
        let (web_env, web_env_error) = match token::env_token(Store::WEB) {
            Ok(env) => (env, None),
            Err(e) => (None, Some(e.to_string())),
        };
        TokenSite {
            env,
            env_error,
            web_env,
            web_env_error,
            state_home: svolder_ipc::config::state_home(),
        }
    }

    /// The token of `store` as it stands, or `None` when only a generated file could hold it and
    /// none exists: looking never makes one.
    fn peek_store(
        &self,
        store: Store,
        token_file: Option<&str>,
    ) -> Result<Option<Token>, TokenError> {
        let (env, env_error) = match store.env_var == Store::WEB.env_var {
            true => (&self.web_env, &self.web_env_error),
            false => (&self.env, &self.env_error),
        };
        if let Some(why) = env_error {
            return Err(TokenError(why.clone()));
        }
        token::peek_with(
            store,
            env.clone(),
            token_file.map(Path::new),
            &self.state_home,
        )
    }

    /// The MCP token as it stands.
    fn peek(&self, token_file: Option<&str>) -> Result<Option<Token>, TokenError> {
        self.peek_store(Store::MCP, token_file)
    }

    /// The web token as it stands.
    fn peek_web(&self, token_file: Option<&str>) -> Result<Option<Token>, TokenError> {
        self.peek_store(Store::WEB, token_file)
    }
}

/// Ask the daemon behind `backend` for its version and status, over a connection of its own,
/// and answer where the reply will land; nothing for a direct session.
pub(super) fn ask_daemon(
    backend: Option<&Backend>,
) -> Option<tokio::sync::oneshot::Receiver<Option<DaemonInfo>>> {
    let request = backend?.daemon_version_request()?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _ = tx.send(request.await);
    });
    Some(rx)
}

/// Read what the category shows from where it lives: the two sections from the file, the
/// token's masked tail from the store, and the daemon's status afresh.
pub(super) fn read<O: Open>(opener: &Opener<O>, watching: &mut Watching, site: &TokenSite) {
    match Config::load_from(&opener.path) {
        Ok(config) => watching.app.take_config(&config),
        Err(e) => watching.app.warn(format!("configuration not read: {e}")),
    }
    let read = site.peek(watching.app.overlays.conn.mcp.token_file.as_deref());
    watching.app.overlays.conn.take_token(read);
    take_web_token(watching, site);
    // An ask already in flight is kept: it may be the re-ask a write just made.
    if watching.daemon_version.is_none() {
        match ask_daemon(watching.backend.as_ref()) {
            Some(asked) => watching.daemon_version = Some(asked),
            None => watching.app.overlays.conn.daemon_missing(),
        }
    }
}

/// Read the token again from the store, and take what it says in. Done when a daemon's word
/// disagrees with this session's view and after a change, so a token the daemon made or
/// rotated in the file this session reads is seen as the same one.
pub(super) fn reread(watching: &mut Watching, site: &TokenSite) {
    let read = site.peek(watching.app.overlays.conn.mcp.token_file.as_deref());
    watching.app.overlays.conn.take_token(read);
    take_web_token(watching, site);
}

/// Read the web token's masked tail from where it lives and take it in.
fn take_web_token(watching: &mut Watching, site: &TokenSite) {
    let read = site.peek_web(watching.app.overlays.conn.web.token_file.as_deref());
    watching.app.overlays.conn.take_web_token(read);
}

/// Take a daemon's answer in. A daemon's token that is not what this session saw may be one it
/// just made or rotated in the file this session reads, so the token is read again before the
/// two are judged to differ.
pub(super) fn answered(
    watching: &mut Watching,
    info: Option<DaemonInfo>,
    own_config: &Path,
    site: &TokenSite,
) {
    let heard = info.is_some();
    daemon_answered(watching, info, own_config);
    if heard && !matches!(watching.app.overlays.conn.token_state(), TokenState::Own) {
        reread(watching, site);
    }
}

/// Write `change` into the file, tell the daemon to reload, and ask it how it stands now. A
/// daemon that reads another file since the change was asked for leaves the file alone.
pub(super) async fn changed<O: Open>(
    opener: &Opener<O>,
    watching: &mut Watching,
    site: &TokenSite,
    change: &ConnChange,
) {
    if let Some(words) = watching.app.overlays.conn.read_only() {
        watching.app.warn(words);
        return;
    }
    let wrote = match (change.is_mcp(), change.is_web()) {
        (true, _) => Config::update_mcp(&opener.path, |mcp| change.apply_mcp(mcp)),
        (_, true) => Config::update_web(&opener.path, |web| change.apply_web(web)),
        (false, false) => Config::update_mqtt(&opener.path, |mqtt| change.apply_mqtt(mqtt)),
    };
    if super::written(opener, watching, wrote).await {
        reread(watching, site);
        watching.daemon_version = ask_daemon(watching.backend.as_ref());
    }
}

/// Put the token on show for [`REVEAL_FOR`].
pub(super) fn reveal(watching: &mut Watching, site: &TokenSite) {
    match site.peek(watching.app.overlays.conn.mcp.token_file.as_deref()) {
        Ok(Some(token)) => {
            let until = Instant::now() + REVEAL_FOR;
            watching
                .app
                .overlays
                .conn
                .reveal(token.value().to_string(), until);
        }
        Ok(None) => watching.app.warn("no token yet"),
        Err(e) => watching.app.warn(e.to_string()),
    }
}

/// Copy the token as the Web UI box copies a link: OSC 52 and the display's clipboard tool, by
/// [`web_box::send`](super::web_box::send), whose notes say what happened and never hold the
/// token.
pub(super) fn copy(watching: &mut Watching, look: &mut Look) {
    match look
        .token_site
        .peek(watching.app.overlays.conn.mcp.token_file.as_deref())
    {
        Ok(Some(token)) => {
            super::web_box::send(watching, look, "token", token.value().to_string(), false);
        }
        Ok(None) => watching.app.warn("no token yet"),
        Err(e) => watching.app.warn(e.to_string()),
    }
}

/// Replace the generated token with a fresh one, tell the daemon to reload so it takes the new
/// one, and take the new masked tail in. A token svolder does not own is refused, naming where
/// it comes from.
pub(super) async fn rotate(watching: &mut Watching, site: &TokenSite) {
    if let Some(words) = watching.app.overlays.conn.token_refusal() {
        watching.app.warn(words);
        return;
    }
    let file = watching.app.overlays.conn.mcp.token_file.clone();
    if let Some(why) = &site.env_error {
        watching.app.warn(why.clone());
        return;
    }
    match token::rotate_with(
        Store::MCP,
        site.env.clone(),
        file.as_deref().map(Path::new),
        &site.state_home,
    ) {
        Ok(token) => {
            watching.app.overlays.conn.take_token(Ok(Some(token)));
            let words = match watching.backend.as_ref() {
                Some(backend) if backend.through_daemon() => match backend.reload().await {
                    Ok(_) => "token replaced: every client needs the new one",
                    Err(_) => {
                        "token replaced, but the daemon was not told: run svolder daemon restart"
                    }
                },
                _ => "token replaced; no daemon is running, so none uses it yet",
            };
            watching.daemon_version = ask_daemon(watching.backend.as_ref());
            match words.contains("not told") {
                true => watching.app.warn(words),
                false => watching.app.notify(words),
            }
        }
        Err(e) => watching.app.warn(e.to_string()),
    }
}

/// Replace the generated web token with a fresh one, tell the daemon to reload so it takes the
/// new one, and take the new masked tail in. A web token svolder does not own is refused,
/// naming where it comes from.
pub(super) async fn rotate_web(watching: &mut Watching, site: &TokenSite) {
    if let Some(words) = watching.app.overlays.conn.web_token_refusal() {
        watching.app.warn(words);
        return;
    }
    if let Some(why) = &site.web_env_error {
        watching.app.warn(why.clone());
        return;
    }
    let file = watching.app.overlays.conn.web.token_file.clone();
    match token::rotate_with(
        Store::WEB,
        site.web_env.clone(),
        file.as_deref().map(Path::new),
        &site.state_home,
    ) {
        Ok(token) => {
            watching.app.overlays.conn.take_web_token(Ok(Some(token)));
            let told = match watching.backend.as_ref() {
                Some(backend) if backend.through_daemon() => Some(backend.reload().await.is_ok()),
                _ => None,
            };
            watching.daemon_version = ask_daemon(watching.backend.as_ref());
            // A daemon that reloaded has ended every code it minted with the old token.
            if told != Some(false) {
                super::web_box::forget_codes(watching);
            }
            match told {
                Some(true) => watching
                    .app
                    .notify("web token replaced: every browser signs in again"),
                Some(false) => watching.app.warn(
                    "web token replaced, but the daemon was not told: run svolder daemon restart",
                ),
                None => watching
                    .app
                    .notify("web token replaced; no daemon is running, so none uses it yet"),
            }
        }
        Err(e) => watching.app.warn(e.to_string()),
    }
}

#[cfg(test)]
mod tests;

/// A wait for a daemon to answer on `socket` again, which stops looking when it is dropped.
pub(super) struct Reconnect {
    /// The socket the daemon that went away listened on.
    pub(super) socket: PathBuf,
    /// Whether a daemon answered there within the window.
    pub(super) answer: tokio::sync::oneshot::Receiver<bool>,
    /// The task looking.
    pub(super) probe: tokio::task::JoinHandle<()>,
}

impl Drop for Reconnect {
    fn drop(&mut self) {
        self.probe.abort();
    }
}

/// Take the daemon's version into the app, and warn when it is not this svolder's, once for each
/// version met: an older daemon is restarted, and for a newer one it is the TUI that is.
pub(super) fn daemon_answered(
    watching: &mut Watching,
    info: Option<DaemonInfo>,
    own_config: &std::path::Path,
) {
    if let Some(info) = &info {
        watching.app.overlays.conn.daemon_said(info, own_config);
    }
    if info.is_none() {
        watching.app.overlays.conn.daemon_missing();
    }
    let own = env!("CARGO_PKG_VERSION");
    let version = info.map(|info| info.version);
    if let Some(daemon) = version.as_deref()
        && let Some(skew) = svolder_ipc::socket::daemon_version::skew(daemon, own)
        && watching.app.session.daemon_version_warned.as_deref() != Some(daemon)
    {
        watching.app.session.daemon_version_warned = Some(daemon.to_string());
        watching.app.warn(skew.advice(daemon, own));
    }
    watching.app.session.daemon_version = version;
    watching.daemon_version = None;
}

/// Whether a daemon answers on `socket` within `window`, looking again after a pause that
/// doubles from [`DAEMON_POLL_FIRST`] up to [`DAEMON_POLL_MAX`].
pub(super) async fn daemon_answers_within(socket: &std::path::Path, window: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + window;
    let mut pause = DAEMON_POLL_FIRST;
    loop {
        tokio::time::sleep(
            pause.min(deadline.saturating_duration_since(tokio::time::Instant::now())),
        )
        .await;
        if svolder_ipc::is_running(socket).await {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        pause = (pause * 2).min(DAEMON_POLL_MAX);
    }
}

/// Wait for the end of the wait for a daemon, while one is under way.
pub(super) async fn recv_reconnect(wait: &mut Option<Reconnect>) -> bool {
    match wait {
        Some(wait) => (&mut wait.answer).await.unwrap_or(false),
        None => std::future::pending().await,
    }
}

/// What the wait for a daemon came to: with one answering, the device the session had open is
/// opened on it by connecting to `socket` and nothing else, and a toast says so; with none, or a
/// daemon that is gone again by then, the device is reported gone as it is when the feed of a
/// direct session ends, and `r` still reopens it through the launcher chain.
pub(super) async fn daemon_back<O: Open>(
    answered: bool,
    socket: PathBuf,
    opener: &Opener<O>,
    mut watching: Watching,
    look: &Look,
    screen: &mut dyn Shown,
) -> Flow {
    if !answered {
        watching.gone();
        return Flow::Continue(watching);
    }
    match switch_via(opener, &mut watching, None, Some(socket), look, screen).await {
        Switched::GaveUp => return gave_up(watching),
        Switched::Opened => watching.app.notify(RECONNECTED),
        Switched::Stayed => watching.gone(),
    }
    Flow::Continue(watching)
}

/// Wait for the daemon's version on `rx`, while a receiver is still open for it. A direct
/// session, and a daemon's connection once its answer is taken, hold `None`, and the `select!`
/// arm reading this is guarded to never poll it then.
pub(super) async fn recv_version(
    rx: &mut Option<tokio::sync::oneshot::Receiver<Option<DaemonInfo>>>,
) -> Option<DaemonInfo> {
    match rx {
        Some(receiver) => receiver.await.unwrap_or(None),
        None => std::future::pending().await,
    }
}
