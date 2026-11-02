// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wizard with nobody to ask: what a terminal would have been walked through, printed once.
//!
//! `svolder setup` off a terminal runs the two steps that ask the machine rather than the user
//! — the host's health and a scan — and prints what the rest of the wizard would have offered:
//! the ready-to-run `add` line under each device in range, and what applies to the daemon on
//! this host. Nothing is written and nothing is asked, so a script or a shell over SSH gets a
//! screen it can act on rather than a prompt it cannot answer.

use std::process::ExitCode;

use super::{Host, NOTHING_HEARD, Radio, daemon_lines, health_lines};

/// Print the summary and answer with the code the run earns.
pub async fn summary<E: Host + Radio>(effects: &E) -> ExitCode {
    let (lines, code) = rendered(effects).await;
    for line in lines {
        println!("{line}");
    }
    ExitCode::from(code)
}

/// The summary, one string per line, beside the exit code it earns.
///
/// A scan that could not run at all — no adapter, or a daemon that answered with an error —
/// leaves its own words where the devices would have been and its exit code as the run's: the
/// health lines above it are still worth printing, and are still what says why.
pub(super) async fn rendered<E: Host + Radio>(effects: &E) -> (Vec<String>, u8) {
    let mut out = health_lines(&effects.checks().await);
    out.push(String::new());
    let mut code = 0;
    match effects.scan().await {
        Ok(heard) => {
            out.extend(heard.notices);
            match heard.entries.is_empty() {
                true => out.push(NOTHING_HEARD.to_string()),
                false => {
                    for entry in &heard.entries {
                        out.extend(crate::commands::device::found_lines(entry));
                    }
                }
            }
        }
        Err(failure) => {
            code = failure.code;
            out.push(failure.to_string());
        }
    }
    out.push(String::new());
    out.extend(daemon_lines(effects));
    (out, code)
}
