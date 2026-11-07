// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The web UI: a page for a browser on this computer or a phone on the network, served by
//! svolderd on a port of its own through the accept loop MCP shares ([`crate::http`]).
//!
//! Every request meets, in order: the `Host` check (the address the server is bound to, the
//! loopback names on a loopback or wildcard bind, and `[web] hosts`; a wildcard bind with no
//! hosts lets any Host through and the sign-in is the check), the security headers and the CSP,
//! and then the router ([`router`]). The listener never stops the daemon: an address that cannot
//! be bound, or a token that cannot be had, leaves a server in the failed state that `version`
//! reports and a reload tries again. The token is never logged, printed or put in a status.
//!
//! - `api`: the private API the page reads and writes: the page's JSON models and the strip.
//! - `assets`: the page's files, embedded in the binary or read from a developer's directory.
//! - [`auth`]: signing a browser in: the web token and the cookie it earns.
//! - [`codes`]: the one-use pairing and link codes.
//! - `events`: the page's live events as JSON, one server-sent event stream per page.
//! - `host`: the current cover and the transport presses.
//! - `icons`: the page's icons.
//! - `model`: the page's JSON model: what a theme draws from.
//! - `pages`: the page's frame and the sign-in page, as askama templates.
//! - `router`: the routes and the checks every request meets first.
//! - `themes`: the themes the page ships, each with the files it brings.
//! - [`view`]: the view models: what each part of a device's screen says, which the model carries.
//! - `writes`: what the page writes, through the device's writer.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, RwLock};

use hyper::HeaderMap;
use svolder_ipc::config::{WebAuth, WebConfig, WebGuest};
use svolder_ipc::token::{self, Store, Token};
use svolder_ipc::{TokenInfo, WebStatus};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

use crate::fleet::Fleet;
use crate::http::{self, Serve, allowed_hosts, blocking, prepare};
use crate::http_slot::{Refresher, Server};
use crate::locks::{locked, read, written};
use crate::log;

mod api;
mod assets;
pub(crate) mod auth;
pub(crate) mod codes;
mod events;
mod host;
#[cfg(test)]
mod icons;
mod model;
mod pages;
mod router;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
mod themes;
pub(crate) mod view;
mod writes;

use assets::Assets;

/// The token a server's sign-ins are checked against: where it comes from, for `version`.
pub(crate) struct Key {
    token: RwLock<String>,
    info: RwLock<Option<TokenInfo>>,
}

impl Key {
    /// A key for `token`.
    fn new(token: &Token) -> Key {
        Key {
            token: RwLock::new(token.value().to_string()),
            info: RwLock::new(Some(token.info())),
        }
    }

    /// A key for a token of `value` from a file of the test's own.
    #[cfg(test)]
    pub(crate) fn for_test(value: &str) -> Key {
        Key {
            token: RwLock::new(value.to_string()),
            info: RwLock::new(None),
        }
    }

    /// Check sign-ins against `value` from now on, which `info` describes.
    fn set(&self, value: &str, info: Option<TokenInfo>) {
        *written(&self.token) = value.to_string();
        *written(&self.info) = info;
    }

    /// Replace the token with `value`, as a reload that read a new one does.
    #[cfg(test)]
    pub(crate) fn replace_for_test(&self, value: &str) {
        *written(&self.token) = value.to_string();
    }

    /// The token sign-ins are checked against now.
    pub(crate) fn token(&self) -> String {
        read(&self.token).clone()
    }

    /// Where the token comes from and its tail, never the token.
    fn info(&self) -> Option<TokenInfo> {
        read(&self.info).clone()
    }
}

/// What a request is answered for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Viewer {
    /// A browser that is signed in, or any browser with `auth = "none"`.
    Signed,
    /// A visitor without a sign-in, with `guest = "read"`: the device screens read-only.
    Guest,
}

/// Who sent `headers`: with `auth = "none"`, everyone is signed in; with a token, a browser with
/// this server's valid cookie, else a guest when the server lets visitors read ([`guest`]).
pub(crate) fn viewer(web: &Web, headers: &HeaderMap) -> Option<Viewer> {
    match web.config.auth {
        WebAuth::None => Some(Viewer::Signed),
        WebAuth::Token => match auth::seen(web, headers) {
            auth::Seen::Valid { .. } => Some(Viewer::Signed),
            auth::Seen::Ended | auth::Seen::Absent => guest(web),
        },
    }
}

/// [`Viewer::Guest`] when the server lets visitors without a sign-in read.
pub(crate) fn guest(web: &Web) -> Option<Viewer> {
    (web.config.guest == WebGuest::Read).then_some(Viewer::Guest)
}

/// Everything a request is answered with: the section the server was started from, the `Host`
/// values that reach it, the token, and the files.
pub(crate) struct Shared {
    pub(crate) config: WebConfig,
    pub(crate) hosts: Vec<String>,
    /// The daemon the page reads and writes through.
    pub(crate) fleet: Arc<dyn Fleet>,
    /// The address the server is bound to, whose port names the cookie.
    pub(crate) bound: SocketAddr,
    pub(crate) key: Option<Key>,
    pub(crate) assets: Assets,
    /// The one-use codes open now, which the daemon keeps across the servers it starts.
    pub(crate) codes: codes::SharedCodes,
    /// Bumped each time the token changes, which signs every browser out: an open event stream
    /// ends on it.
    pub(crate) signed_out: watch::Sender<u64>,
    /// Bumped when the fleet changed in a way its devices' own rows do not say (the hops after
    /// them): an open event stream sends the fleet again.
    pub(crate) fleet_changed: watch::Sender<u64>,
    /// The saved presets as the configuration file last read, for the computer's model.
    pub(crate) saved_presets: model::SavedPresets,
}

/// The router's state: one server's [`Shared`], cheap to clone.
#[derive(Clone)]
pub(crate) struct Web(Arc<Shared>);

impl std::ops::Deref for Web {
    type Target = Shared;
    fn deref(&self) -> &Shared {
        &self.0
    }
}

impl Web {
    /// The state of a server bound to `bound`, over the daemon behind `fleet`, signing in with
    /// `codes`: they are bound to this server's token, so ones made under another end here.
    pub(crate) fn new(
        config: WebConfig,
        bound: SocketAddr,
        fleet: Arc<dyn Fleet>,
        key: Option<Key>,
        assets: Assets,
        codes: codes::SharedCodes,
    ) -> Web {
        let mut hosts = allowed_hosts(bound, &config.hosts);
        // The link and QR code svolder prints carry the host of `[web] url`: a list that is not
        // open answers it too (an open one answers every Host already).
        let from_url = config
            .url
            .as_deref()
            .and_then(svolder_ipc::config::normal_origin)
            .and_then(|origin| origin.split_once("://").map(|(_, host)| host.to_string()));
        if let Some(host) = from_url
            && !hosts.is_empty()
            && !hosts.contains(&host)
        {
            hosts.push(host);
        }
        locked(&codes).bind(key.as_ref().map(Key::token).as_deref());
        Web(Arc::new(Shared {
            config,
            hosts,
            fleet,
            bound,
            key,
            assets,
            codes,
            signed_out: watch::channel(0).0,
            fleet_changed: watch::channel(0).0,
            saved_presets: model::SavedPresets::default(),
        }))
    }

    /// A fresh pairing code.
    pub(crate) fn new_pair(&self) -> std::io::Result<String> {
        locked(&self.codes).new_pair(tokio::time::Instant::now())
    }

    /// A fresh link code.
    pub(crate) fn new_link(&self) -> std::io::Result<String> {
        locked(&self.codes).new_link(tokio::time::Instant::now())
    }

    /// Whether the link code `code` is open, without spending it.
    pub(crate) fn link_open(&self, code: &str) -> bool {
        locked(&self.codes).link_open(code, tokio::time::Instant::now())
    }

    /// Check sign-ins against `token` from now on; a token that differs from the one before ends
    /// every open code, as it ends every sign-in.
    pub(crate) fn rekey(&self, token: &Token) {
        self.rekey_value(token.value(), Some(token.info()));
    }

    /// [`Web::rekey`] with the token's value and its description as they are.
    pub(crate) fn rekey_value(&self, value: &str, info: Option<TokenInfo>) {
        let Some(key) = self.key.as_ref() else {
            return;
        };
        // The codes' lock is held across the swap, and a sign-in reads the token under it: a code
        // spent before the swap cannot earn a cookie minted under the new token. The order is
        // always codes, then key; nothing takes them the other way round.
        let mut codes = locked(&self.codes);
        let changed = key.token() != value;
        codes.bind(Some(value));
        key.set(value, info);
        drop(codes);
        if changed {
            self.signed_out.send_modify(|turn| *turn += 1);
        }
    }
}

/// The daemon's end of the web UI `[web]` describes: serving, or failed to start.
pub struct WebServer {
    config: WebConfig,
    state: State,
}

enum State {
    Serving(Serving),
    Failed(String),
}

struct Serving {
    addr: SocketAddr,
    stop: CancellationToken,
    task: JoinHandle<()>,
    /// What the server answers from, which holds the token a reload replaces.
    web: Web,
}

impl WebServer {
    /// Start the server `config` describes, handing it the daemon's open `codes`. A token that cannot be had or an
    /// address that cannot be bound is logged (without the token) and leaves the server failed.
    ///
    /// The token is read and the address bound off the runtime's workers and under
    /// [`http::SETUP_TIMEOUT`], so a source that blocks fails only this server.
    pub async fn start(
        config: &WebConfig,
        fleet: Arc<dyn Fleet>,
        codes: codes::SharedCodes,
    ) -> WebServer {
        let wanted = config.clone();
        let prepared = prepare(
            config.listen,
            config.auth == WebAuth::None,
            move || match wanted.auth {
                WebAuth::None => Ok(None),
                WebAuth::Token => resolve(&wanted).map(Some),
            },
        )
        .await;
        let (token, listener) = match prepared {
            Ok(prepared) => prepared,
            Err(message) => return WebServer::failed(config, message),
        };
        let key = token.map(|token| {
            log::info(&format!(
                "web: the token comes from {}",
                token.source.describe()
            ));
            Key::new(&token)
        });
        match WebServer::launch(config, listener, key, fleet, Assets::from_env(), codes) {
            Ok(server) => server,
            Err(message) => WebServer::failed(config, message),
        }
    }

    /// Serve on `listener`: spawn the accept loop over the router.
    fn launch(
        config: &WebConfig,
        listener: TcpListener,
        key: Option<Key>,
        fleet: Arc<dyn Fleet>,
        assets: Assets,
        codes: codes::SharedCodes,
    ) -> Result<WebServer, String> {
        let addr = listener.local_addr().map_err(|e| e.to_string())?;
        let stop = CancellationToken::new();
        let web = Web::new(config.clone(), addr, fleet, key, assets, codes);
        let router = router::router(web.clone());
        let serve: Serve<axum::body::Body> = Arc::new(move |request| {
            let router = router.clone();
            Box::pin(async move {
                match router.oneshot(request).await {
                    Ok(response) => response,
                    Err(never) => match never {},
                }
            })
        });
        let task = tokio::spawn(http::accept_loop(listener, serve, stop.clone(), "web"));
        let server = WebServer {
            config: config.clone(),
            state: State::Serving(Serving {
                addr,
                stop,
                task,
                web,
            }),
        };
        log::info(&server.status().line());
        Ok(server)
    }

    fn failed(config: &WebConfig, message: String) -> WebServer {
        let server = WebServer {
            config: config.clone(),
            state: State::Failed(message),
        };
        log::error(&server.status().line());
        server
    }

    /// A pairing code from the running server, with its life in seconds; why not otherwise.
    pub fn pair_code(&self) -> Result<(String, u64), String> {
        self.code(Web::new_pair)
    }

    /// A link code from the running server, with its life in seconds; why not otherwise.
    pub fn link_code(&self) -> Result<(String, u64), String> {
        self.code(Web::new_link)
    }

    /// Whether the link code `code` is still open, which spends nothing; why not otherwise.
    pub fn link_open(&self, code: &str) -> Result<bool, String> {
        self.running().map(|web| web.link_open(code))
    }

    /// A code `make` draws from the running server, with its life in seconds; why not otherwise.
    fn code(
        &self,
        make: impl Fn(&Web) -> std::io::Result<String>,
    ) -> Result<(String, u64), String> {
        self.running().and_then(|web| {
            make(web)
                .map(|code| (code, codes::CODE_LIFE.as_secs()))
                .map_err(|e| format!("cannot read random bytes from /dev/urandom: {e}"))
        })
    }

    /// The running server's [`Web`]; why not otherwise.
    fn running(&self) -> Result<&Web, String> {
        match &self.state {
            State::Serving(serving) => Ok(&serving.web),
            State::Failed(error) => Err(format!(
                "the web UI could not start ({error}); see svolder doctor"
            )),
        }
    }
}

impl Server for WebServer {
    type Config = WebConfig;
    type Status = WebStatus;

    const NAME: &'static str = "web";

    fn off() -> WebStatus {
        WebStatus::Off
    }

    fn config(&self) -> &WebConfig {
        &self.config
    }

    fn serving(&self) -> bool {
        matches!(self.state, State::Serving(_))
    }

    fn status(&self) -> WebStatus {
        match &self.state {
            State::Serving(serving) => WebStatus::Listening {
                listen: serving.addr,
                auth: self.config.auth,
                writes: self.config.writes,
                guest: self.config.guest,
            },
            State::Failed(error) => WebStatus::Failed {
                listen: self.config.listen,
                error: error.clone(),
            },
        }
    }

    /// Where the token the server compares against comes from, and its tail; none for a server
    /// with no token to compare or one that is not serving.
    fn token_info(&self) -> Option<TokenInfo> {
        match &self.state {
            State::Serving(serving) => serving.web.key.as_ref().and_then(Key::info),
            State::Failed(_) => None,
        }
    }

    /// What a reload that left the section as it was needs to read the token again, when this
    /// server has a token to replace: a rotated token takes effect at once.
    fn refresher(&self) -> Option<Refresher> {
        match &self.state {
            State::Serving(serving) if serving.web.key.is_some() => {
                let config = self.config.clone();
                let web = serving.web.clone();
                Some(Refresher::new(async move {
                    let token = blocking(move || resolve(&config)).await?;
                    web.rekey(&token);
                    Ok(())
                }))
            }
            _ => None,
        }
    }

    /// Stop serving and close the port: connections in flight are dropped, and the listener is
    /// gone when this returns.
    async fn stop(self) {
        if let State::Serving(serving) = self.state {
            serving.stop.cancel();
            let _ = serving.task.await;
            log::info("web: stopped");
        }
    }
}

/// The token for `config`, from the environment, the file the config names, or the generated
/// file (created when missing).
fn resolve(config: &WebConfig) -> Result<Token, String> {
    token::resolve(Store::WEB, config.token_file.as_deref().map(Path::new))
        .map_err(|e| e.to_string())
}
