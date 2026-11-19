// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How a daemon's version stands against a client's, and the one way to ask a daemon for it.
//!
//! The comparisons take the client's own version as a parameter, which every caller passes as
//! `env!("CARGO_PKG_VERSION")`: the workspace version, the same in every crate.

use std::path::Path;
use std::time::Duration;

use crate::{
    Activity, Client, McpStatus, MqttStatus, Reason, Request, Response, TokenInfo, WebStatus,
};

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

/// The command that has a daemon read its configuration file again and apply it.
pub const RELOAD_COMMAND: &str = "svolder daemon reload";

/// The line that says the daemon's configuration file changed since its last reload, and how to
/// reload: `status`, `doctor`, the TUI and the web page all give these words. `path` is the file
/// as the daemon names it (a container's daemon reads another file than this svolder's).
pub fn config_changed_line(path: &str) -> String {
    format!("config: {path} changed since the last reload: {RELOAD_COMMAND} (or SIGHUP)")
}

/// The line that says the daemon's configuration file is gone: the daemon keeps the configuration
/// it runs and refuses a reload until the file is back. Every front end gives these words.
pub fn config_missing_line(path: &str) -> String {
    format!(
        "config: {path} is missing: the daemon keeps its running configuration; restore the file, \
         then {RELOAD_COMMAND}"
    )
}

/// What the daemon says when it refuses a reload or a write because its configuration file is
/// gone. It already says the step, so a front end adds none ([`is_config_missing_refusal`]).
pub fn config_missing_refusal(path: &str) -> String {
    format!(
        "{path} is missing: the daemon keeps its running configuration; restore the file, then \
         try again"
    )
}

/// Whether `text` is [`config_missing_refusal`]'s.
pub fn is_config_missing_refusal(text: &str) -> bool {
    text.contains(" is missing: the daemon keeps its running configuration; restore the file")
}

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
    /// Where its MQTT client stands, when it says.
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
    /// What it says it supports; empty for a daemon too old to list any.
    pub features: Vec<String>,
    /// Whether its configuration file changed since the last reload (a hand edit it has not
    /// taken); false for a daemon too old to say.
    pub config_changed: bool,
    /// Whether its configuration file is gone (`config_changed` is then true too); false for a
    /// daemon too old to say.
    pub config_missing: bool,
    /// How many clients its MCP server and web UI have had, when it says.
    pub activity: Option<Activity>,
}

impl DaemonInfo {
    /// Whether the daemon lists `feature` (one of the `FEATURE_*` constants) as supported. Every
    /// front-end question of whether the daemon can do something goes through here; a daemon
    /// that listed nothing supports nothing. It answers for the daemon that sent this `version`
    /// answer: ask again after a restart.
    pub fn supports(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }

    /// The line that says the daemon's configuration file changed since its last reload, or is
    /// missing, naming the file as the daemon does; `None` while it did not (and for a daemon too
    /// old to say).
    pub fn config_changed_words(&self) -> Option<String> {
        let path = self.config.as_deref().unwrap_or("its config.toml");
        match (self.config_changed, self.config_missing) {
            (_, true) => Some(config_missing_line(path)),
            (true, false) => Some(config_changed_line(path)),
            (false, false) => None,
        }
    }
}

/// The reason an error answer has: the `reason` it carries, or, for a 0.32.x daemon, which sent
/// none, the one its words for a link that is away give (the old text match, kept for one release
/// like [`is_unknown_request`] and gone with it after 0.33.x).
pub fn reason_of(code: u8, message: &str, reason: Option<Reason>) -> Option<Reason> {
    match (code, reason) {
        (_, Some(reason)) => Some(reason),
        (crate::CODE_LINK, None) if message == crate::NOT_CONNECTED => Some(Reason::NotConnected),
        (crate::CODE_LINK, None) if message.ends_with(crate::NOT_FOUND_YET) => {
            Some(Reason::NotFoundYet)
        }
        _ => None,
    }
}

/// Whether an error answer `code` and `message` says the daemon, described by `info` (its
/// `version` answer, when it could be had), does not know the request kind `cmd` (the request's
/// wire name, such as `identify`).
///
/// A daemon since 0.33.0 answers [`crate::CODE_UNKNOWN_REQUEST`], and one that lists
/// [`crate::FEATURE_UNKNOWN_REQUEST_CODE`] never answers the old words. The text match is the
/// fallback for a 0.32.x daemon, which answered code 1 with serde's own `unknown variant` words
/// and listed no features; it goes away after 0.33.x, when no such daemon is left to meet.
pub fn is_unknown_request(info: Option<&DaemonInfo>, code: u8, message: &str, cmd: &str) -> bool {
    match code {
        crate::CODE_UNKNOWN_REQUEST => true,
        crate::CODE_USAGE
            if info.is_some_and(|info| info.supports(crate::FEATURE_UNKNOWN_REQUEST_CODE)) =>
        {
            false
        }
        crate::CODE_USAGE => {
            message.starts_with("bad request: ")
                && message.contains(&format!("unknown variant `{cmd}`"))
        }
        _ => false,
    }
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
            features,
            config_changed,
            config_missing,
            activity,
        }) => Ok(VersionReply {
            info: DaemonInfo {
                version,
                mqtt,
                mcp,
                config,
                mcp_token,
                web,
                web_token,
                features,
                config_changed,
                config_missing,
                activity,
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
