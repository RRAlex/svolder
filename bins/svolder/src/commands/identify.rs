// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `identify`: ask a device to show itself, so one of several units can be told from the rest.

use std::process::ExitCode;

use svolder_ipc::{Config, NOT_CONNECTED, NOT_FOUND_YET};

use crate::backend::{Backend, DeviceRow, Failure, Globals};
use crate::format::{identify_line, link_down};

use super::device::opened_for_a_cue;

/// Ask the device to show itself and say whether the request went out.
///
/// The link is opened the way `watch` opens it, keeping a refusal: the unit shows itself for a
/// request sent under a wrong passphrase too, so a refused link is no reason to stay quiet.
/// Exit 0 for a request sent and for one that went out within the last two seconds; a device
/// that is not connected earns the usual exit 3, with the wait until the daemon tries it again
/// where a daemon is asked. A direct session is closed before returning, on every path.
pub async fn run(g: &Globals<'_>) -> ExitCode {
    let mut backend = match opened_for_a_cue(g).await {
        Ok(backend) => backend,
        Err(failure) => return away(g, failure).await.report(),
    };
    let outcome = identify_on(&mut backend).await;
    backend.close().await;
    match outcome {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(failure) => failure.report(),
    }
}

/// The failure of an open that found the device away. A daemon that has never found the device
/// answers the open with its "has not been found yet", where a device that was found and lost
/// earns the not-connected words and the wait; this verb says the not-connected words for both,
/// with the wait from the daemon's device list when it can be read. Any other failure stands.
async fn away(g: &Globals<'_>, failure: Failure) -> Failure {
    if !failure.is_not_found_yet() {
        return failure;
    }
    let rows = match Config::load() {
        Ok(config) => Backend::devices_in(g, &config).await.unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    not_connected(failure, g.device, &rows)
}

/// `failure`, a daemon's "has not been found yet", as the not-connected failure with the wait
/// the row of the device it names carries.
///
/// The words lead with the device the daemon resolved the request to, which is the one to look
/// up when no `-d` was given; the row of the device `selector` names, or the only row listed,
/// stands in when the words name no listed device.
fn not_connected(failure: Failure, selector: Option<&str>, rows: &[DeviceRow]) -> Failure {
    let named = failure
        .what
        .strip_suffix(NOT_FOUND_YET)
        .and_then(|name| rows.iter().find(|row| row.name.eq_ignore_ascii_case(name)));
    let row = match (named, selector, rows) {
        (Some(row), _, _) => Some(row),
        (None, _, [only]) => Some(only),
        (None, Some(name), rows) => rows.iter().find(|row| row.name.eq_ignore_ascii_case(name)),
        (None, None, _) => None,
    };
    Failure {
        what: link_down(NOT_CONNECTED, row.and_then(|row| row.retry_in_secs)),
        ..failure
    }
}

/// The words for what asking `backend`'s device to show itself came to, or the failure it earned.
async fn identify_on(backend: &mut Backend) -> Result<String, Failure> {
    let done = backend.identify().await.map_err(|e| e.failure())?;
    Ok(identify_line(&done.device, done.sent))
}

#[cfg(test)]
mod tests;
