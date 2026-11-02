// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Settings box's Web UI box: where the daemon's web UI stands, whether it is on, how far it
//! reaches, a link and a pairing code to sign a browser in with, and its token's tail. The codes
//! come from the daemon and are asked for only while the box is open.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Instant;

use svolder_ipc::WebStatus;

/// The box's rows, in the order it draws them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WebRow {
    /// Where the daemon says its web UI stands.
    State,
    /// Whether `[web]` turns it on.
    #[default]
    WebUi,
    /// How far it listens: this computer, the network, or an address of the file's own.
    Reach,
    /// A link that signs one browser in.
    Link,
    /// Six digits that sign one browser in.
    Pairing,
    /// The web token's masked tail.
    Token,
}

impl WebRow {
    /// The rows the cursor stops on, in order: State only reports.
    pub const STOPS: [WebRow; 5] = [
        WebRow::WebUi,
        WebRow::Reach,
        WebRow::Link,
        WebRow::Pairing,
        WebRow::Token,
    ];

    /// The row's label.
    pub fn label(self) -> &'static str {
        match self {
            WebRow::State => "State",
            WebRow::WebUi => "Web UI",
            WebRow::Reach => "Reach",
            WebRow::Link => "Link",
            WebRow::Pairing => "Pairing code",
            WebRow::Token => "Token",
        }
    }
}

/// A code the daemon minted: its text (the whole link, for a link) and when it ends.
#[derive(Clone, PartialEq, Eq)]
pub struct Code {
    /// What the row shows.
    pub text: String,
    /// When the daemon stops taking it.
    pub until: Instant,
}

impl Code {
    /// The code a link carries in its fragment; none for a text that is not a link.
    pub fn link_code(&self) -> Option<&str> {
        self.text
            .rsplit_once(crate::commands::web::LOGIN_FRAGMENT)
            .map(|(_, code)| code)
    }
}

impl std::fmt::Debug for Code {
    /// A code prints as `***`: it signs a browser in, so no dump or log carries it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Code")
            .field("text", &"***")
            .field("until", &self.until)
            .finish()
    }
}

/// The Web UI box's state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebBox {
    /// The row the cursor is on.
    pub row: WebRow,
    /// The link on show.
    pub link: Option<Code>,
    /// The pairing code on show.
    pub pair: Option<Code>,
    /// The listen addresses the link on show was made for: the file's and the daemon's.
    pub link_for: Option<(SocketAddr, SocketAddr)>,
    /// Whether the link's base is this computer's because none a phone reaches was found.
    pub link_local: bool,
    /// Whether the link on show replaced one that a browser used.
    pub link_replaced: bool,
    /// Whether the link being asked for replaces one that a browser used.
    pub link_replacing: bool,
    /// Whether the daemon could not say if the link on show is open, so it is not asked about
    /// again.
    pub link_unwatched: bool,
    /// Whether the question before the token is replaced is up.
    pub rotating: bool,
}

/// How far a listen address reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// The loopback address.
    ThisComputer,
    /// Every address: the network.
    Network,
    /// An address of the file's own.
    Custom(SocketAddr),
}

impl Reach {
    /// The row's words.
    pub fn word(self) -> String {
        match self {
            Reach::ThisComputer => "this computer".to_string(),
            Reach::Network => "your network".to_string(),
            Reach::Custom(listen) => format!("custom ({listen})"),
        }
    }
}

/// How far `listen` reaches.
pub fn reach(listen: SocketAddr) -> Reach {
    match listen.ip() {
        ip if ip.is_loopback() => Reach::ThisComputer,
        ip if ip.is_unspecified() => Reach::Network,
        _ => Reach::Custom(listen),
    }
}

/// The listen address ←→ moves to from `listen`: this computer and the network in turn, the
/// port kept; a custom address moves to this computer.
pub fn stepped_listen(listen: SocketAddr) -> SocketAddr {
    let ip = match reach(listen) {
        Reach::ThisComputer => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        Reach::Network | Reach::Custom(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
    };
    SocketAddr::new(ip, listen.port())
}

/// The time `code` has left at `now` as `m:ss left`, or `ended`.
pub fn timer(code: &Code, now: Instant) -> String {
    match code
        .until
        .checked_duration_since(now)
        .filter(|left| !left.is_zero())
    {
        Some(left) => {
            let secs = left.as_secs();
            format!("{}:{:02} left", secs / 60, secs % 60)
        }
        None => "ended".to_string(),
    }
}

/// A pairing code's line at `now`: its text in two threes and the time it has left, or `ended`.
pub fn code_line(code: &Code, now: Instant) -> String {
    let timer = timer(code, now);
    if timer == "ended" {
        return timer;
    }
    let text = match code.text.len() == 6 {
        true => format!("{} {}", &code.text[..3], &code.text[3..]),
        false => code.text.clone(),
    };
    format!("{text} · {timer}")
}

/// The State row's words, from what the daemon said (`up` is whether one answered; `asking` is
/// a daemon session that has not been answered yet, which reads `…`, not `no daemon`).
pub fn state_word(web: Option<&WebStatus>, up: Option<bool>, asking: bool) -> String {
    match (web, up) {
        (None, None) if asking => "…".to_string(),
        (_, Some(false)) | (None, None) => "no daemon".to_string(),
        (Some(WebStatus::Listening { listen, .. }), _) => format!("listening on {listen}"),
        (Some(WebStatus::Off), _) => "off".to_string(),
        (Some(WebStatus::Failed { error, .. }), _) => format!("failed: {error}"),
        (Some(WebStatus::Unknown) | None, _) => "unknown to this daemon".to_string(),
    }
}

#[cfg(test)]
mod tests;
