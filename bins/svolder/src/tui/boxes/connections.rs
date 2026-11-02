// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the Settings box's Connections category knows: the daemon's `[mcp]` and `[mqtt]`
//! sections as the configuration file holds them, the token's masked tail, what the daemon says
//! of its two servers, and the one question or edit line the category can have open.
//!
//! Secrets stay out of here by construction. The MQTT password is reduced to whether one is set
//! the moment the file is read. The token's text is held only while it is revealed, for ten
//! seconds, and prints as `***` in a `Debug` dump; the rest of the time the category holds only
//! the mask's last three characters, which say which token is in use.

use std::path::Path;
use std::time::{Duration, Instant};

use svolder_ipc::config::{McpConfig, MqttConfig, WebConfig};
use svolder_ipc::token::{self, Agreement, Store, Token, TokenError, TokenSource};
use svolder_ipc::{Config, McpStatus, MqttStatus, TokenInfo, TokenKind, WebStatus};

use crate::backend::DaemonInfo;
use crate::tui::boxes::web_box::WebBox;

/// How long a revealed token stays on the screen.
pub const REVEAL_FOR: Duration = Duration::from_secs(10);
/// The glyph a hidden character of the token is drawn as.
const HIDDEN: char = '•';
/// How many hidden characters the mask draws before the tail.
const HIDDEN_RUN: usize = 6;
/// The longest a host name or an address typed into the edit line may be.
pub const LINE_MAX: usize = 64;

/// The `[mqtt]` keys the category shows, with the password reduced to whether it is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MqttView {
    /// Whether the bridge is on.
    pub enabled: bool,
    /// The broker's host.
    pub host: Option<String>,
    /// The broker's port.
    pub port: u16,
    /// Whether the bridge takes commands.
    pub commands: bool,
    /// Whether a password is set, in the file or in `SVOLDER_MQTT_PASSWORD`. Never the password.
    pub password_set: bool,
}

impl Default for MqttView {
    fn default() -> Self {
        let defaults = MqttConfig::default();
        MqttView {
            enabled: defaults.enabled,
            host: defaults.host,
            port: defaults.port,
            commands: defaults.commands,
            password_set: false,
        }
    }
}

/// What the category knows of the token without holding it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TokenView {
    /// Not read yet.
    #[default]
    Unread,
    /// No token exists yet: the daemon makes one when the server first starts.
    Missing,
    /// A token exists: its masked form and where it comes from.
    Present {
        /// The token as the row draws it: hidden characters and, for a long token, its last
        /// three.
        masked: String,
        /// Where it comes from.
        source: TokenSource,
    },
    /// The token could not be read; the words name a source or a path, never a token.
    Unreadable(String),
}

/// A token on show, until a moment.
#[derive(Clone, PartialEq, Eq)]
pub struct Revealed {
    text: String,
    until: Instant,
}

impl std::fmt::Debug for Revealed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Revealed")
            .field("text", &"***")
            .field("until", &self.until)
            .finish()
    }
}

/// A row of text the category edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The address the MCP server listens on.
    Listen,
    /// The broker's host.
    Host,
    /// The broker's port.
    Port,
}

impl Field {
    /// What the edit line's title calls the field.
    pub fn title(self) -> &'static str {
        match self {
            Field::Listen => "MCP listen",
            Field::Host => "MQTT broker host",
            Field::Port => "MQTT broker port",
        }
    }
}

/// The one thing the category can have open over its rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modal {
    /// A line being typed.
    Editing(Line),
    /// The question before the token is replaced.
    Rotating,
    /// The Web UI box.
    Web(WebBox),
}

/// A line being typed into: its text and where the cursor sits, in characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Which row the line edits.
    pub field: Field,
    /// The text as typed.
    pub text: String,
    /// The cursor, in characters from the start.
    pub caret: usize,
}

/// The Connections category's state.
#[derive(Debug, Clone, Default)]
pub struct Connections {
    /// The `[mcp]` section as the file holds it.
    pub mcp: McpConfig,
    /// The `[mqtt]` section as the file holds it, less its password.
    pub mqtt: MqttView,
    /// The `[web]` section as the file holds it.
    pub web: WebConfig,
    /// What is known of the token.
    pub token: TokenView,
    /// What is known of the web token.
    pub web_token: TokenView,
    /// The token, while it is on show.
    pub revealed: Option<Revealed>,
    /// Where the daemon says its MCP server stands.
    pub mcp_status: Option<McpStatus>,
    /// Where the daemon says its MQTT bridge stands.
    pub mqtt_status: Option<MqttStatus>,
    /// Where the daemon says its web UI stands.
    pub web_status: Option<WebStatus>,
    /// Where the daemon says its token comes from and its tail, when it says.
    pub daemon_token: Option<TokenInfo>,
    /// Where the daemon says its web token comes from and its tail, when it says.
    pub web_daemon_token: Option<TokenInfo>,
    /// Whether a daemon has answered the last ask: `None` before any.
    pub daemon_up: Option<bool>,
    /// The configuration file the daemon reads, when it is not the file this session edits.
    pub elsewhere: Option<String>,
    /// What is open over the rows.
    pub modal: Option<Modal>,
}

impl Connections {
    /// Take the two sections of `config` in.
    pub fn load(&mut self, config: &Config) {
        self.mcp = config.mcp.clone();
        self.web = config.web.clone();
        self.mqtt = MqttView {
            enabled: config.mqtt.enabled,
            host: config.mqtt.host.clone(),
            port: config.mqtt.port,
            commands: config.mqtt.commands,
            password_set: config.mqtt.password.is_some()
                || std::env::var_os("SVOLDER_MQTT_PASSWORD").is_some_and(|v| !v.is_empty()),
        };
    }

    /// Take what a token read came to in: the masked form, never the token.
    pub fn take_token(&mut self, read: Result<Option<Token>, TokenError>) {
        let view = match read {
            Ok(Some(token)) => TokenView::Present {
                masked: mask(token.value()),
                source: token.source.clone(),
            },
            Ok(None) => TokenView::Missing,
            Err(e) => TokenView::Unreadable(e.to_string()),
        };
        // A token on show stays while a read finds the same one.
        if view != self.token {
            self.revealed = None;
        }
        self.token = view;
    }

    /// Take what a web token read came to in: the masked form, never the token.
    pub fn take_web_token(&mut self, read: Result<Option<Token>, TokenError>) {
        self.web_token = match read {
            Ok(Some(token)) => TokenView::Present {
                masked: mask(token.value()),
                source: token.source.clone(),
            },
            Ok(None) => TokenView::Missing,
            Err(e) => TokenView::Unreadable(e.to_string()),
        };
    }

    /// Show `text` as the token until `until`.
    pub fn reveal(&mut self, text: String, until: Instant) {
        self.revealed = Some(Revealed { text, until });
    }

    /// The token, while it is on show at `now`.
    pub fn shown(&self, now: Instant) -> Option<&str> {
        self.revealed
            .as_ref()
            .filter(|revealed| now < revealed.until)
            .map(|revealed| revealed.text.as_str())
    }

    /// Drop the token's text once its time has run out at `now`.
    pub fn expire(&mut self, now: Instant) {
        if self.revealed.as_ref().is_some_and(|r| now >= r.until) {
            self.revealed = None;
        }
    }

    /// Take what the daemon said of itself in: its two servers' status, and whether it reads
    /// another configuration file than `own`, which makes every row read-only.
    pub fn daemon_said(&mut self, info: &DaemonInfo, own: &Path) {
        self.mcp_status.clone_from(&info.mcp);
        self.mqtt_status.clone_from(&info.mqtt);
        self.daemon_token.clone_from(&info.mcp_token);
        self.web_status.clone_from(&info.web);
        self.web_daemon_token.clone_from(&info.web_token);
        self.daemon_up = Some(true);
        self.elsewhere = info
            .config
            .as_deref()
            .filter(|theirs| !token::same_file(Path::new(theirs), own))
            .map(str::to_string);
        // A token on show is this session's own: it goes once the daemon uses another.
        if matches!(self.token_state(), TokenState::Differs { .. }) {
            self.revealed = None;
        }
    }

    /// Why a change to the configuration is refused now: the daemon reads another file than
    /// this session edits.
    pub fn read_only(&self) -> Option<String> {
        self.elsewhere
            .as_ref()
            .map(|theirs| format!("read-only here: the daemon reads {theirs}"))
    }

    /// Why this session's token cannot be shown, copied or replaced now: the configuration is
    /// read-only here, or the daemon uses another token than this session resolves.
    pub fn token_refusal(&self) -> Option<String> {
        self.read_only().or_else(|| match self.token_state() {
            TokenState::Differs { reason, .. } => {
                Some(format!("not this session's token: {reason}"))
            }
            TokenState::Own | TokenState::NotReadYet { .. } => None,
        })
    }
}

/// What the token row stands on: this session's own reading, or the daemon's word where the
/// two disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenState {
    /// This session's own resolution is what the daemon uses, or there is no daemon word to
    /// compare it with.
    Own,
    /// The daemon uses another token than this session resolves: its masked tail, and where it
    /// lives, in words.
    Differs {
        /// The daemon's token as the row draws it.
        masked: String,
        /// Where the daemon's token lives.
        reason: String,
    },
    /// The daemon uses the file this session reads but has not read it again since it changed:
    /// the token is this session's own, and a reload settles it.
    NotReadYet {
        /// The file, as the daemon names it.
        path: String,
    },
}

impl Connections {
    /// The daemon has not answered the last ask, so nothing it said stands.
    pub fn daemon_missing(&mut self) {
        self.daemon_up = Some(false);
        self.daemon_token = None;
        self.web_status = None;
        self.web_daemon_token = None;
    }

    /// Why the web token cannot be replaced now: the configuration is read-only here, or the
    /// daemon uses another web token than this session resolves.
    pub fn web_token_refusal(&self) -> Option<String> {
        self.read_only().or_else(|| {
            let TokenView::Present { masked, source } = &self.web_token else {
                return None;
            };
            let theirs = self.web_daemon_token.as_ref()?;
            match token::compare(source, masked.trim_start_matches(HIDDEN), theirs) {
                Agreement::Other => Some(format!(
                    "not this session's web token: it lives {}",
                    token::lives(Store::WEB, theirs)
                )),
                Agreement::Same | Agreement::NotReadYet => None,
            }
        })
    }

    /// Whether this session's reading of the token is the one the daemon uses, by source kind,
    /// path and tail; `Differs` carries the daemon's tail and where its token lives.
    pub fn token_state(&self) -> TokenState {
        let Some(theirs) = &self.daemon_token else {
            return TokenState::Own;
        };
        if let TokenView::Present { masked, source } = &self.token {
            match token::compare(source, masked.trim_start_matches(HIDDEN), theirs) {
                Agreement::Same => return TokenState::Own,
                Agreement::NotReadYet => {
                    return TokenState::NotReadYet {
                        path: theirs.path.clone().unwrap_or_default(),
                    };
                }
                Agreement::Other => {}
            }
        }
        let reason = match theirs.kind {
            TokenKind::Environment => token::lives(Store::MCP, theirs),
            _ => format!(
                "{}, not the file this session reads",
                token::lives(Store::MCP, theirs)
            ),
        };
        TokenState::Differs {
            masked: format!("{}{}", HIDDEN.to_string().repeat(HIDDEN_RUN), theirs.tail),
            reason,
        }
    }
}

/// `token` as the row draws it: a run of hidden characters, and for a long token its last three.
pub fn mask(token: &str) -> String {
    let mut out: String = std::iter::repeat_n(HIDDEN, HIDDEN_RUN).collect();
    out.push_str(&svolder_ipc::token::tail(token));
    out
}

/// The OSC 52 sequence that asks a terminal to put `text` on its clipboard.
pub fn osc52(text: &str) -> String {
    format!("\x1b]52;c;{}\x07", base64(text.as_bytes()))
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
