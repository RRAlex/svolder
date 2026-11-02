// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The web UI's one-use codes, kept in memory by the daemon and handed to every web server it
//! starts, so a restart of the server leaves them open: a pairing code to type
//! (six digits, one open at a time, five wrong tries end it) and link codes to carry in a link's
//! fragment (16 random bytes, at most eight open). Each signs one browser in within five minutes.
//! A socket client may ask for either: the socket gives full control of every device already,
//! so a code grants nothing the socket does not.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use svolder_ipc::token::{base64url, random_bytes, token_matches};
use tokio::time::Instant;

/// How long a code stays good.
pub(crate) const CODE_LIFE: Duration = Duration::from_secs(300);

/// How many wrong tries a pairing code takes; the last of them ends it.
pub(crate) const PAIR_TRIES: u32 = 5;

/// How many link codes may be open at once; a new one past this ends the oldest.
pub(crate) const MAX_LINKS: usize = 8;

/// The open codes, shared by the web servers one daemon runs in turn.
pub(crate) type SharedCodes = Arc<Mutex<Codes>>;

/// The open codes, and the token they were made under.
#[derive(Debug, Default)]
pub(crate) struct Codes {
    pair: Option<Pair>,
    links: Vec<(String, Instant)>,
    /// The token the codes belong to; none when they were made with no token to sign in under.
    token: Option<String>,
}

/// The pairing code open now.
#[derive(Debug)]
struct Pair {
    code: String,
    until: Instant,
    wrong: u32,
}

/// What a typed pairing code earns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Paired {
    /// The browser is signed in, and the code is spent.
    Signed,
    /// Not the code; this many tries are left.
    Wrong(u32),
    /// No code is open: none was made, it was used, it lasted its five minutes, or the wrong
    /// tries ended it.
    Ended,
}

impl Codes {
    /// A fresh pairing code from `now`, in place of any open one.
    pub(crate) fn new_pair(&mut self, now: Instant) -> std::io::Result<String> {
        let code = six_digits()?;
        self.pair = Some(Pair {
            code: code.clone(),
            until: now + CODE_LIFE,
            wrong: 0,
        });
        Ok(code)
    }

    /// A fresh link code from `now`; the oldest open one ends when [`MAX_LINKS`] are open.
    pub(crate) fn new_link(&mut self, now: Instant) -> std::io::Result<String> {
        let mut bytes = [0u8; 16];
        random_bytes(&mut bytes)?;
        let code = base64url(&bytes);
        self.links.retain(|(_, until)| *until >= now);
        if self.links.len() >= MAX_LINKS {
            self.links.remove(0);
        }
        self.links.push((code.clone(), now + CODE_LIFE));
        Ok(code)
    }

    /// Try `typed` (spaces aside) against the open pairing code at `now`.
    pub(crate) fn pair(&mut self, typed: &str, now: Instant) -> Paired {
        let typed: String = typed.chars().filter(|c| !c.is_whitespace()).collect();
        let Some(pair) = self.pair.as_mut().filter(|pair| now <= pair.until) else {
            self.pair = None;
            return Paired::Ended;
        };
        if token_matches(&pair.code, &typed) {
            self.pair = None;
            return Paired::Signed;
        }
        pair.wrong += 1;
        match PAIR_TRIES - pair.wrong {
            0 => {
                self.pair = None;
                Paired::Ended
            }
            left => Paired::Wrong(left),
        }
    }

    /// Spend the link code `code` at `now`: whether it was open.
    pub(crate) fn link(&mut self, code: &str, now: Instant) -> bool {
        self.links.retain(|(_, until)| *until >= now);
        match self
            .links
            .iter()
            .position(|(open, _)| token_matches(open, code))
        {
            Some(at) if !code.is_empty() => {
                self.links.remove(at);
                true
            }
            _ => false,
        }
    }

    /// Whether the link code `code` is open at `now`, without spending it.
    pub(crate) fn link_open(&self, code: &str, now: Instant) -> bool {
        !code.is_empty()
            && self
                .links
                .iter()
                .any(|(open, until)| *until >= now && token_matches(open, code))
    }

    /// Hold the codes for `token` from now on: a token other than the one they were made under
    /// ends every one of them, so a code never signs a browser in under another token.
    pub(crate) fn bind(&mut self, token: Option<&str>) {
        if self.token.as_deref() != token {
            self.clear();
            self.token = token.map(str::to_string);
        }
    }

    /// End every open code.
    pub(crate) fn clear(&mut self) {
        self.pair = None;
        self.links.clear();
    }
}

/// Six digits, uniform: a 32-bit draw below the largest multiple of a million that fits, taken
/// modulo a million, drawn again otherwise.
fn six_digits() -> std::io::Result<String> {
    const LIMIT: u32 = u32::MAX - u32::MAX % 1_000_000;
    loop {
        let mut bytes = [0u8; 4];
        random_bytes(&mut bytes)?;
        let draw = u32::from_le_bytes(bytes);
        if draw < LIMIT {
            return Ok(format!("{:06}", draw % 1_000_000));
        }
    }
}

#[cfg(test)]
mod tests;
