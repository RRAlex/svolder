// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the daemon's two HTTP servers say of who uses them: the `activity` of a `version` reply.
//!
//! Counts only, never an address. A client is a distinct peer address that passed the server's
//! token check (a server with no token counts every connection), seen within a window of the
//! daemon's own life; every front end words the counts through [`ServerActivity::line`] and
//! [`ServerActivity::short`], so none of them claims a window the daemon did not observe.

use serde::{Deserialize, Serialize};

/// The longest window the daemon keeps, in seconds: a day.
pub const DAY_SECS: u64 = 86_400;

/// One hour, in seconds.
const HOUR_SECS: u64 = 3_600;

/// What the MCP server and the web UI say of their clients. A server that is off or failed has
/// none.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    /// the MCP server's clients
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<ServerActivity>,
    /// the web UI's clients
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web: Option<ServerActivity>,
}

/// One server's counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerActivity {
    /// connections open now that passed the server's check
    pub open: u32,
    /// distinct clients seen in the past hour
    pub hour: u32,
    /// distinct clients seen in the past day
    pub day: u32,
    /// how much of the day the daemon has observed: its uptime, at most a day
    pub counted_secs: u64,
    /// whether the table of clients was full and dropped its oldest within the day, so `day` is
    /// a lower bound
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub capped: bool,
}

impl ServerActivity {
    /// The counts as a sentence: `2 connections open; 3 clients in the past hour, 5 in the past
    /// day`. A window the daemon has not yet observed is said as what it saw (`5 clients since
    /// the daemon started 40 min ago`), and a full table says `at least`.
    pub fn line(&self) -> String {
        let open = format!("{} {} open", self.open, plural(self.open, "connection"));
        let day = format!("{}{}", self.at_least(), self.day);
        if self.counted_secs < HOUR_SECS {
            let since = age(self.counted_secs);
            return format!(
                "{open}; {day} {} since the daemon started {since} ago",
                plural(self.day, "client")
            );
        }
        let rest = match self.counted_secs < DAY_SECS {
            true => format!("since the daemon started {} ago", age(self.counted_secs)),
            false => "in the past day".to_string(),
        };
        format!(
            "{open}; {} {} in the past hour, {day} {rest}",
            self.hour,
            plural(self.hour, "client")
        )
    }

    /// The counts in a short form for a narrow value: `2 open · 3 past hour · 5 past day`.
    pub fn short(&self) -> String {
        let plus = if self.capped { "+" } else { "" };
        if self.counted_secs < HOUR_SECS {
            return format!(
                "{} open · {}{plus} in the last {}",
                self.open,
                self.day,
                age(self.counted_secs)
            );
        }
        let day = match self.counted_secs < DAY_SECS {
            true => format!("{}{plus} in the last {}", self.day, age(self.counted_secs)),
            false => format!("{}{plus} past day", self.day),
        };
        format!("{} open · {} past hour · {day}", self.open, self.hour)
    }

    /// `at least ` while the day's figure is a lower bound.
    fn at_least(&self) -> &'static str {
        match self.capped {
            true => "at least ",
            false => "",
        }
    }
}

/// `noun`, or its plural for any count but one.
fn plural(n: u32, noun: &str) -> String {
    match n {
        1 => noun.to_string(),
        _ => format!("{noun}s"),
    }
}

/// A span in the unit that reads best: seconds, minutes or hours, rounded down.
fn age(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs} s"),
        60..HOUR_SECS => format!("{} min", secs / 60),
        _ => format!("{} h", secs / HOUR_SECS),
    }
}
