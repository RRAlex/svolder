// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The MCP server: the Model Context Protocol (revision 2026-07-28, stateless) over plain
//! HTTP, on `/mcp`, in a task beside the device actors.
//!
//! Every request meets, in order: the [`McpAuth`] check (a bearer token, unless `[mcp] auth`
//! is `none` on a loopback address), the path, and then rmcp's streamable-HTTP service, which
//! checks the `Host` header against the address the server listens on and the `Origin` header
//! against `[mcp] origins`, caps the body, and answers JSON-RPC. A request that fails the
//! first check is answered 401 with nothing in the body, so it learns nothing, not even which
//! paths exist.
//!
//! The accept loop, with its connection and header limits, is [`crate::http`]'s.
//!
//! The listener never stops the daemon: an address that cannot be bound, or a token that
//! cannot be had, leaves a server in the failed state that `version` reports and a reload
//! tries again. The token is never logged, printed or put in a status.
//!
//! - [`tools`]: the tools the server offers: four that read a device, three that change one and
//!   `identify`, which asks it to show itself.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, RwLock};

use http_body_util::{BodyExt, Empty};
use hyper::body::{Bytes, Incoming};
use hyper::header::{AUTHORIZATION, HeaderMap, WWW_AUTHENTICATE};
use hyper::{Request, Response, StatusCode};
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use svolder_ipc::McpStatus;
use svolder_ipc::config::{McpAuth as AuthKind, McpConfig};
use svolder_ipc::token::{self, Store, Token};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::fleet::Fleet;
#[cfg(test)]
use crate::http::bind;
use crate::http::{self, Serve, allowed_hosts, blocking, prepare};
use crate::http_slot::{Refresher, Server};
use crate::locks::{read, written};
use crate::log;

pub mod tools;
use tools::{Floors, Handler, Tools};

/// The path the server answers on.
const PATH: &str = "/mcp";

/// The largest request body, in bytes. A tool call is a few hundred.
const MAX_BODY: usize = 64 * 1024;

/// The longest an `Authorization` header may be before it is refused unread: the token compare
/// takes time in proportion to the longer of the two values, so an attacker's length is capped.
const MAX_AUTHORIZATION: usize = 512;

// A token the header cap could not carry would never authenticate.
const _: () = assert!(token::MAX_TOKEN_LEN + "Bearer ".len() <= MAX_AUTHORIZATION);

/// The decision on who may send a request. The server asks it once per request, before the
/// request is read any further; a different way of proving identity (a token from an external
/// single sign-on, say) is another implementation of it.
pub trait McpAuth: Send + Sync + 'static {
    /// Whether a request carrying `headers` may go on to the server.
    fn allows(&self, headers: &HeaderMap) -> bool;
}

/// A shared bearer token. It can be replaced while the server runs, so a rotated token takes
/// effect on a reload without closing the port.
pub struct TokenAuth {
    token: RwLock<String>,
    /// Where the token comes from and its tail, for the `version` reply.
    info: RwLock<Option<svolder_ipc::TokenInfo>>,
}

impl TokenAuth {
    /// An auth that accepts `Authorization: Bearer <token>`.
    pub fn new(token: &str) -> Self {
        TokenAuth {
            token: RwLock::new(token.to_string()),
            info: RwLock::new(None),
        }
    }

    /// An auth that accepts `token`, remembering where it came from.
    fn from_token(token: &Token) -> Self {
        let auth = TokenAuth::new(token.value());
        *written(&auth.info) = Some(token.info());
        auth
    }

    /// Where the token comes from and its tail, never the token.
    fn info(&self) -> Option<svolder_ipc::TokenInfo> {
        read(&self.info).clone()
    }

    /// Accept `token` from now on, and no other.
    fn replace(&self, token: &str) {
        *written(&self.token) = token.to_string();
    }

    /// Accept `token` from now on, and no other, and say where it comes from.
    fn replace_with(&self, token: &Token) {
        self.replace(token.value());
        *written(&self.info) = Some(token.info());
    }
}

impl McpAuth for TokenAuth {
    fn allows(&self, headers: &HeaderMap) -> bool {
        // Exactly one credential: with several, which one a proxy or this server reads first
        // would decide, and nothing legitimate sends more.
        let mut credentials = headers.get_all(AUTHORIZATION).iter();
        let (Some(value), None) = (credentials.next(), credentials.next()) else {
            return false;
        };
        if value.len() > MAX_AUTHORIZATION {
            return false;
        }
        let Ok(text) = value.to_str() else {
            return false;
        };
        let Some((scheme, given)) = text.split_once(' ') else {
            return false;
        };
        if !scheme.eq_ignore_ascii_case("bearer") {
            return false;
        }
        let expected = read(&self.token);
        token::token_matches(&expected, given)
    }
}

/// No check at all, for `auth = "none"`, which the config accepts only on a loopback address.
struct NoAuth;

impl McpAuth for NoAuth {
    fn allows(&self, _headers: &HeaderMap) -> bool {
        true
    }
}

/// The daemon's end of the MCP server `[mcp]` describes: serving, or failed to start.
pub struct McpServer {
    config: McpConfig,
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
    /// The token the server compares against, when `auth = "token"`, to replace on a reload.
    token: Option<Arc<TokenAuth>>,
}

impl McpServer {
    /// Start the server `config` describes. A token that cannot be had or an address that
    /// cannot be bound is logged (without the token) and leaves the server failed.
    ///
    /// The token is read and the address bound off the runtime's workers and under
    /// [`http::SETUP_TIMEOUT`], so a source that blocks fails only this server.
    pub async fn start(
        config: &McpConfig,
        fleet: Arc<dyn Fleet>,
        floors: Arc<Floors>,
    ) -> McpServer {
        let wanted = config.clone();
        let prepared = prepare(
            config.listen,
            config.auth == AuthKind::None,
            move || match wanted.auth {
                AuthKind::None => Ok(None),
                AuthKind::Token => resolve(&wanted).map(Some),
            },
        )
        .await;
        let (token, listener) = match prepared {
            Ok(prepared) => prepared,
            Err(message) => return McpServer::failed(config, message),
        };
        let (auth, token): (Arc<dyn McpAuth>, Option<Arc<TokenAuth>>) = match token {
            None => (Arc::new(NoAuth), None),
            Some(token) => {
                log::info(&format!(
                    "mcp: the token comes from {}",
                    token.source.describe()
                ));
                let auth = Arc::new(TokenAuth::from_token(&token));
                (Arc::clone(&auth) as Arc<dyn McpAuth>, Some(auth))
            }
        };
        match McpServer::launch(config, listener, auth, token, fleet, floors, None) {
            Ok(server) => server,
            Err(message) => McpServer::failed(config, message),
        }
    }

    /// Start the server with `auth` as its check: bind the address and spawn the accept loop.
    #[cfg(test)]
    fn start_with(
        config: &McpConfig,
        auth: Arc<dyn McpAuth>,
        token: Option<Arc<TokenAuth>>,
    ) -> Result<McpServer, String> {
        McpServer::start_with_fleet(config, auth, token, Arc::new(crate::fleet::Unavailable))
    }

    /// [`McpServer::start_with`] over a fleet of the test's own.
    #[cfg(test)]
    fn start_with_fleet(
        config: &McpConfig,
        auth: Arc<dyn McpAuth>,
        token: Option<Arc<TokenAuth>>,
        fleet: Arc<dyn Fleet>,
    ) -> Result<McpServer, String> {
        let listener = bind(config.listen).map_err(|e| e.to_string())?;
        McpServer::launch(config, listener, auth, token, fleet, Arc::default(), None)
    }

    /// Serve on `listener`. `inner` stands in for rmcp's service where a test wants to see
    /// what reaches it.
    fn launch(
        config: &McpConfig,
        listener: TcpListener,
        auth: Arc<dyn McpAuth>,
        token: Option<Arc<TokenAuth>>,
        fleet: Arc<dyn Fleet>,
        floors: Arc<Floors>,
        inner: Option<Inner>,
    ) -> Result<McpServer, String> {
        let addr = listener.local_addr().map_err(|e| e.to_string())?;
        let stop = CancellationToken::new();
        let inner =
            inner.unwrap_or_else(|| rmcp_inner(service(config, addr, stop.clone(), fleet, floors)));
        let shared = Arc::new(Shared { auth, inner });
        let serve: Serve<Body> = Arc::new(move |request| {
            let shared = Arc::clone(&shared);
            Box::pin(async move { answer(&shared, request).await })
        });
        let task = tokio::spawn(http::accept_loop(listener, serve, stop.clone(), "mcp"));
        let server = McpServer {
            config: config.clone(),
            state: State::Serving(Serving {
                addr,
                stop,
                task,
                token,
            }),
        };
        log::info(&server.status().line());
        Ok(server)
    }

    fn failed(config: &McpConfig, message: String) -> McpServer {
        let server = McpServer {
            config: config.clone(),
            state: State::Failed(message),
        };
        log::error(&server.status().line());
        server
    }
}

impl Server for McpServer {
    type Config = McpConfig;
    type Status = McpStatus;

    const NAME: &'static str = "mcp";

    fn off() -> McpStatus {
        McpStatus::Off
    }

    fn config(&self) -> &McpConfig {
        &self.config
    }

    fn serving(&self) -> bool {
        matches!(self.state, State::Serving(_))
    }

    fn status(&self) -> McpStatus {
        match &self.state {
            State::Serving(serving) => McpStatus::Listening {
                listen: serving.addr,
                auth: self.config.auth,
                writes: self.config.writes,
            },
            State::Failed(error) => McpStatus::Failed {
                listen: self.config.listen,
                error: error.clone(),
            },
        }
    }

    /// Where the token the server compares against comes from, and its tail; none for a server
    /// with no token to compare or one that is not serving.
    fn token_info(&self) -> Option<svolder_ipc::TokenInfo> {
        match &self.state {
            State::Serving(Serving {
                token: Some(auth), ..
            }) => auth.info(),
            _ => None,
        }
    }

    /// What a reload that left the section as it was needs to read the token again, when this
    /// server has a token to replace: a rotated token takes effect at once.
    fn refresher(&self) -> Option<Refresher> {
        match &self.state {
            State::Serving(Serving {
                token: Some(auth), ..
            }) => {
                let config = self.config.clone();
                let auth = Arc::clone(auth);
                Some(Refresher::new(async move {
                    let token = blocking(move || resolve(&config)).await?;
                    auth.replace_with(&token);
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
            log::info("mcp: stopped");
        }
    }
}

/// The token for `config`, from the environment, the file the config names, or the generated
/// file (created when missing).
fn resolve(config: &McpConfig) -> Result<Token, String> {
    token::resolve(Store::MCP, config.token_file.as_deref().map(Path::new))
        .map_err(|e| e.to_string())
}

/// rmcp's service: stateless, JSON answers, a body cap, the `Host` allow-list for the address
/// the server is bound to, and `Origin` validation against `[mcp] origins` — enforced even
/// when that list is empty, so a request with an `Origin` the operator never named is refused
/// and one with none (not a browser) goes on.
fn service(
    config: &McpConfig,
    bound: SocketAddr,
    stop: CancellationToken,
    fleet: Arc<dyn Fleet>,
    floors: Arc<Floors>,
) -> StreamableHttpService<Handler, NeverSessionManager> {
    let server = StreamableHttpServerConfig::default()
        .with_allowed_hosts(allowed_hosts(bound, &config.hosts))
        .with_allowed_origins(config.origins.clone())
        .enforce_origin_validation()
        .with_max_request_body_bytes(MAX_BODY)
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_cancellation_token(stop.clone());
    let tools = Tools::with_floors(fleet, config.writes, config.max_volume_db, floors);
    // Ends with the server; nothing waits for it.
    drop(tools.watch(stop.clone()));
    StreamableHttpService::new(
        move || {
            Ok(Handler {
                tools: tools.clone(),
            })
        },
        Arc::default(),
        server,
    )
}

/// The body type rmcp answers with, which the refusals here share.
type Body = http_body_util::combinators::BoxBody<Bytes, Infallible>;

/// What answers a request that passed the auth and the path check.
type Inner = Arc<
    dyn Fn(Request<Incoming>) -> std::pin::Pin<Box<dyn Future<Output = Response<Body>> + Send>>
        + Send
        + Sync,
>;

/// rmcp's service as the inner answer.
fn rmcp_inner(service: StreamableHttpService<Handler, NeverSessionManager>) -> Inner {
    Arc::new(move |request| {
        let service = service.clone();
        Box::pin(async move { service.handle(request).await })
    })
}

/// Everything a connection's requests are answered with.
struct Shared {
    auth: Arc<dyn McpAuth>,
    inner: Inner,
}

/// One request's answer: refused unless the auth allows it, 404 off `/mcp`, rmcp's otherwise.
/// The credential is removed before the request goes on, so nothing downstream can see it,
/// log it or echo it.
async fn answer(shared: &Shared, mut request: Request<Incoming>) -> Response<Body> {
    if !shared.auth.allows(request.headers()) {
        return unauthorized();
    }
    if request.uri().path() != PATH {
        return refusal(StatusCode::NOT_FOUND);
    }
    request.headers_mut().remove(AUTHORIZATION);
    (shared.inner)(request).await
}

/// 401, naming the scheme and nothing else.
fn unauthorized() -> Response<Body> {
    let mut response = refusal(StatusCode::UNAUTHORIZED);
    response.headers_mut().insert(
        WWW_AUTHENTICATE,
        hyper::header::HeaderValue::from_static("Bearer"),
    );
    response
}

/// A bodiless answer with `status`.
fn refusal(status: StatusCode) -> Response<Body> {
    let mut response = Response::new(Empty::<Bytes>::new().boxed());
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests;
