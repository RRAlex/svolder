// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder mcp token [--rotate]` and `svolder web token [--rotate]`: the bearer token a client
//! of svolderd's MCP server sends, and the token the web UI's sign-in page takes, each over its
//! own store.
//!
//! The token is the only thing printed on stdout, so `$(svolder mcp token)` is the token and
//! nothing else; every message goes to stderr.
//!
//! A daemon that answers is asked which token its server uses, and the answer is compared with
//! the token this command resolves, by the comparison the TUI makes: a daemon in another
//! environment or another state home may use a token this command would print wrongly or
//! replace to no effect.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use svolder_ipc::token::{self, Agreement, Store, Token};
use svolder_ipc::{Client, Config, Request, Response, TokenInfo, socket};

use crate::backend::Globals;
use crate::cli::McpCommand;

/// Where this command looks for the token: the environment's value and the state home.
struct Site {
    env: Option<String>,
    state_home: PathBuf,
}

/// What a run printed: the token for stdout and the notes for stderr, in order.
struct Done {
    token: String,
    notes: Vec<String>,
}

/// What a daemon on the socket said of its token.
enum Daemon {
    /// Nothing answers on the socket.
    None,
    /// A daemon answered; its token's description, when it has one (a server with no token, or
    /// off, says none).
    Answered(Option<TokenInfo>),
}

/// Carry out one of the `mcp` sub-commands.
pub(crate) async fn mcp(g: &Globals<'_>, command: McpCommand) -> Result<ExitCode, String> {
    match command {
        McpCommand::Token { rotate } => token(g, Store::MCP, rotate).await,
    }
}

/// Print the token of `store`, creating the generated one on first use. With `rotate`, replace
/// the generated token first, then tell a daemon that answers to reload so it takes the new one;
/// that refuses when the token comes from the store's environment variable or its `token_file`,
/// or when the daemon uses another token than this one.
pub(crate) async fn token(g: &Globals<'_>, store: Store, rotate: bool) -> Result<ExitCode, String> {
    let config = Config::load().map_err(|e| e.to_string())?;
    let site = Site {
        env: token::env_token(store).map_err(|e| e.to_string())?,
        state_home: svolder_ipc::config::state_home(),
    };
    let path = socket::path::resolve(g.socket, config.socket.as_deref());
    let file = match store == Store::WEB {
        true => config.web.token_file.as_deref(),
        false => config.mcp.token_file.as_deref(),
    }
    .map(Path::new);
    let done = run(&site, store, file, &path, rotate).await?;
    println!("{}", done.token);
    for note in done.notes {
        eprintln!("svolder: {note}");
    }
    Ok(ExitCode::SUCCESS)
}

/// The command over `site` and `store`, with the daemon looked for on `socket`.
async fn run(
    site: &Site,
    store: Store,
    file: Option<&Path>,
    socket: &Path,
    rotate: bool,
) -> Result<Done, String> {
    let daemon = ask_daemon(socket, store).await;
    let mut notes = Vec::new();
    let token = match rotate {
        true => {
            if let Daemon::Answered(Some(theirs)) = &daemon {
                let current = token::peek_with(store, site.env.clone(), file, &site.state_home)
                    .map_err(|e| e.to_string())?;
                // The same file the daemon has not read again is settled by this very reload.
                let other = current.is_none_or(|token| compare(&token, theirs) == Agreement::Other);
                if other {
                    return Err(format!(
                        "the daemon uses another token, which lives {}, so replacing this one \
                         would change nothing it accepts",
                        token::lives(store, theirs)
                    ));
                }
            }
            let token = token::rotate_with(store, site.env.clone(), file, &site.state_home)
                .map_err(|e| e.to_string())?;
            notes.push(reload(&daemon, socket).await);
            token
        }
        false => {
            let token = token::resolve_with(store, site.env.clone(), file, &site.state_home)
                .map_err(|e| e.to_string())?;
            if let Daemon::Answered(Some(theirs)) = &daemon {
                match compare(&token, theirs) {
                    Agreement::Same => {}
                    Agreement::NotReadYet => notes.push(format!(
                        "the daemon has not read {} again yet: {} (or SIGHUP)",
                        theirs.path.as_deref().unwrap_or("the token file"),
                        svolder_ipc::socket::daemon_version::RESTART_COMMAND
                    )),
                    Agreement::Other => notes.push(format!(
                        "warning: the daemon uses another token, which lives {}; the one printed \
                         is not the one it accepts",
                        token::lives(store, theirs)
                    )),
                }
            }
            token
        }
    };
    Ok(Done {
        token: token.value().to_string(),
        notes,
    })
}

/// How `token` stands against the one the daemon's `theirs` describes: the comparison the TUI
/// makes.
fn compare(token: &Token, theirs: &TokenInfo) -> Agreement {
    token::compare(&token.source, &token::tail(token.value()), theirs)
}

/// Ask the daemon on `socket` which token it uses for `store`.
async fn ask_daemon(socket: &Path, store: Store) -> Daemon {
    let Ok(mut client) = Client::connect(socket).await else {
        return Daemon::None;
    };
    let ask = svolder_ipc::socket::daemon_version::version_on(&mut client);
    match tokio::time::timeout(svolder_ipc::socket::daemon_version::VERSION_WAIT, ask).await {
        Ok(Ok(reply)) => Daemon::Answered(match store == Store::WEB {
            true => reply.info.web_token,
            false => reply.info.mcp_token,
        }),
        // A daemon that answers anything else, or not within the wait, is too old to say.
        Ok(Err(_)) | Err(_) => Daemon::Answered(None),
    }
}

/// Tell the daemon to read the token again, and say how it went.
async fn reload(daemon: &Daemon, socket: &Path) -> String {
    if matches!(daemon, Daemon::None) {
        return "no daemon is running, so none uses the new token yet".to_string();
    }
    let said = match Client::connect(socket).await {
        Ok(mut client) => match client.request(Request::Reload).await {
            Ok(Response::Reloaded { .. }) => {
                return "the daemon now accepts only the new token".into();
            }
            Ok(Response::Error { message, .. }) => message,
            Ok(other) => format!("it answered {other:?}"),
            Err(e) => e.to_string(),
        },
        Err(e) => e.to_string(),
    };
    format!(
        "the daemon did not reload ({said}): it still accepts the old token until you run {}",
        svolder_ipc::socket::daemon_version::RESTART_COMMAND
    )
}

#[cfg(test)]
mod tests;
