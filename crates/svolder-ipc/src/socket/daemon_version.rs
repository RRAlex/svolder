// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How a daemon's version stands against a client's, and the one way to ask a daemon for it.
//!
//! The comparisons take the client's own version as a parameter, which every caller passes as
//! `env!("CARGO_PKG_VERSION")`: the workspace version, the same in every crate.

use std::path::Path;
use std::time::Duration;

use crate::{Client, McpStatus, MqttStatus, Request, Response, TokenInfo, WebStatus};

/// How a daemon's version stands against this svolder's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skew {
    /// The daemon is the older build: `svolder daemon restart` brings it up to date.
    Older,
    /// The daemon is the newer build: this svolder is the one left running from before an
    /// upgrade.
    Newer,
}

impl Skew {
    /// `daemon vX is older than this svolder vY`, or `newer`.
    pub fn words(self, daemon: &str, own: &str) -> String {
        let how = match self {
            Skew::Older => "older",
            Skew::Newer => "newer",
        };
        format!("daemon v{daemon} is {how} than this svolder v{own}")
    }

    /// The words and the way out, as `status` and the TUI's toast give them: an older daemon is
    /// restarted, and a newer one means this svolder is the one to replace.
    pub fn advice(self, daemon: &str, own: &str) -> String {
        let fix = match self {
            Skew::Older => RESTART_COMMAND,
            Skew::Newer => "restart svolder",
        };
        format!("{}: {fix}", self.words(daemon, own))
    }
}

/// How `daemon`, as a daemon reports its version, stands against `own`, or `None` when it is
/// `own`, another build of it (build metadata alone differs), or words that are not a version at
/// all (a daemon that gave no version has nothing to differ). Versions compare in semver order:
/// the numbers first, and a pre-release before the release it leads to. A version this cannot
/// read is taken as the older.
pub fn skew(daemon: &str, own: &str) -> Option<Skew> {
    if !is_version(daemon) || daemon == own {
        return None;
    }
    match (version_key(daemon), version_key(own)) {
        (Some(d), Some(o)) if d == o => None,
        (Some(d), Some(o)) if d > o => Some(Skew::Newer),
        _ => Some(Skew::Older),
    }
}

/// The ordering key of a version: its numbers, then whether it is a release (a pre-release sorts
/// before it) and the pre-release text. Build metadata after `+` is ignored.
fn version_key(text: &str) -> Option<(Vec<u64>, bool, String)> {
    let text = text.split('+').next()?;
    let (core, pre) = match text.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (text, None),
    };
    let numbers = core
        .split('.')
        .map(|part| part.parse().ok())
        .collect::<Option<Vec<u64>>>()?;
    Some((numbers, pre.is_none(), pre.unwrap_or_default().to_string()))
}

/// The line `status` adds when the daemon's version is not `own`, naming the way to bring the
/// two together. `None` for a matching daemon, and for words that are not a version at all.
pub fn mismatch_line(daemon: &str, own: &str) -> Option<String> {
    skew(daemon, own).map(|skew| format!("mismatch: {}", skew.advice(daemon, own)))
}

/// The command that brings a daemon up to this svolder's version.
pub const RESTART_COMMAND: &str = "svolder daemon restart";

/// Whether `text` is a version number as the daemon reports one, rather than the words a client
/// puts in its place when the daemon did not answer one.
pub fn is_version(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_digit()) && !text.contains(' ')
}

/// What a daemon says of itself in its `version` reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonInfo {
    /// The daemon's version.
    pub version: String,
    /// Where its MQTT bridge stands, when it says.
    pub mqtt: Option<MqttStatus>,
    /// Where its MCP server stands, when it says.
    pub mcp: Option<McpStatus>,
    /// The configuration file it reads, as a path on its side, when it says.
    pub config: Option<String>,
    /// Where its MCP token comes from and its tail, when it says.
    pub mcp_token: Option<TokenInfo>,
    /// Where its web UI stands, when it says.
    pub web: Option<WebStatus>,
    /// Where its web UI's token comes from and its tail, when it says.
    pub web_token: Option<TokenInfo>,
}

/// How long a front end gives a daemon to answer the request for its own version.
pub const VERSION_WAIT: Duration = Duration::from_secs(3);

/// A daemon's whole `version` reply: what [`DaemonInfo`] holds, and the log it names, which only
/// `daemon status` and `daemon logs` read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionReply {
    /// What the daemon says of itself.
    pub info: DaemonInfo,
    /// Where the daemon says it logs, when it says.
    pub log: Option<String>,
}

/// Why a `version` request brought no version.
#[derive(Debug)]
pub enum Unanswered {
    /// The daemon answered with something else.
    Other(Box<Response>),
    /// The request failed: the connection was lost or the reply could not be read.
    Failed(String),
}

impl std::fmt::Display for Unanswered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unanswered::Other(response) => write!(f, "an unexpected answer ({response:?})"),
            Unanswered::Failed(error) => write!(f, "no version ({error})"),
        }
    }
}

/// Ask the daemon `client` is connected to for its version. The caller bounds the wait.
pub async fn version_on(client: &mut Client) -> Result<VersionReply, Unanswered> {
    match client.request(Request::Version).await {
        Ok(Response::Version {
            version,
            mqtt,
            mcp,
            log,
            config,
            mcp_token,
            web,
            web_token,
        }) => Ok(VersionReply {
            info: DaemonInfo {
                version,
                mqtt,
                mcp,
                config,
                mcp_token,
                web,
                web_token,
            },
            log,
        }),
        Ok(other) => Err(Unanswered::Other(Box::new(other))),
        Err(e) => Err(Unanswered::Failed(e.to_string())),
    }
}

/// The daemon on `socket`'s own version and status, asked over a connection of this call's own,
/// or `None` for anything but a plain version answer within `wait`: a daemon that takes the
/// connection and never answers is given up on then, and the caller goes on without it.
pub async fn version_within(socket: &Path, wait: Duration) -> Option<DaemonInfo> {
    let ask = async {
        let mut client = Client::connect(socket).await.ok()?;
        version_on(&mut client).await.ok().map(|reply| reply.info)
    };
    tokio::time::timeout(wait, ask).await.ok().flatten()
}

#[cfg(test)]
mod tests;
