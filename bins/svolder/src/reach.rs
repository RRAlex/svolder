// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which writer a front end uses: the daemon's, whenever one runs, and its own only with none.
//!
//! The rule is one: while a daemon runs it is the one writer of the devices and the one that
//! walks a new one, so a front end that wrote the file itself would be a second authority. A
//! front end asks here, with the feature the work needs. No socket and nothing listening on it
//! is no daemon (the front end's own path stands); a socket that cannot be tried, a daemon that
//! does not say what it is and a daemon without the feature are each a refusal in words, and
//! nothing is written behind any of them.

use std::path::Path;

use svolder_ipc::socket::daemon_version::{VERSION_WAIT, version_on};
use svolder_ipc::{
    Client, DEFAULT_REQUEST_TIMEOUT, DeviceConfigOp, FEATURE_DEVICE_CRUD, IpcError, Reason,
    Request, Response, request_on_own_connection,
};

use crate::recovery::{Failure, Kind};

/// Whether a daemon answers on `socket`, and has `feature`.
///
/// `Ok(false)`, no daemon, is the only answer that lets a front end use its own writer. A daemon
/// that lacks `feature` is refused with the step that brings it up to date.
pub(crate) async fn daemon_runs(socket: &Path, feature: &str, what: &str) -> Result<bool, Failure> {
    let mut client = match Client::connect_if_any(socket).await {
        Ok(Some(client)) => client,
        Ok(None) => return Ok(false),
        // A daemon may run behind a socket that cannot be tried.
        Err(why) => {
            return Err(Failure::of(
                Kind::Usage,
                format!(
                    "cannot reach the daemon's socket {}: {why}",
                    socket.display()
                ),
            ));
        }
    };
    let info = match tokio::time::timeout(VERSION_WAIT, version_on(&mut client)).await {
        Ok(Ok(reply)) => reply.info,
        Ok(Err(_)) | Err(_) => {
            return Err(Failure::of(
                Kind::DaemonError,
                format!(
                    "the daemon on {} did not say what it is: svolder daemon status",
                    socket.display()
                ),
            ));
        }
    };
    if !info.supports(feature) {
        return Err(Failure::of(
            Kind::DaemonTooOld,
            too_old_words(Some(&info.version), what),
        ));
    }
    Ok(true)
}

/// Send a `device_config` op to the daemon on `socket`, when one that has the family runs:
/// `Ok(None)` for no daemon, so the caller writes through its own path; the daemon's answer
/// (its refusal included, see [`refusal_words`]) otherwise.
pub(crate) async fn device_request(
    socket: &Path,
    op: DeviceConfigOp,
) -> Result<Option<Response>, Failure> {
    if !daemon_runs(socket, FEATURE_DEVICE_CRUD, "edit the devices").await? {
        return Ok(None);
    }
    let request = Request::DeviceConfig { op };
    match request_on_own_connection(socket, request, Some(DEFAULT_REQUEST_TIMEOUT)).await {
        Ok(answer) => Ok(Some(answer)),
        Err(IpcError::Refused(answer)) => Ok(Some(*answer)),
        Err(e) => Err(Failure::of(Kind::DaemonError, e.to_string())),
    }
}

/// What a refused device write says, worded from the typed reason at the control: the name
/// that is taken, the device that is gone, and for the rest the daemon's own words, which name
/// the file or the unit at fault and never a passphrase.
pub(crate) fn refusal_words(reason: Option<Reason>, message: &str) -> String {
    match reason {
        Some(Reason::Taken) => "another device already has that name".to_string(),
        Some(Reason::Missing) => "that device is no longer configured".to_string(),
        Some(Reason::FileBusy) => {
            "the configuration file is busy with another writer: try again".to_string()
        }
        Some(Reason::Stopping) => "the daemon is stopping: nothing was written".to_string(),
        _ => message.to_string(),
    }
}

/// The words for a daemon that lacks a feature this svolder asks of it: one wording for every
/// front end. `version` is the daemon's when it said it.
pub(crate) fn too_old_words(version: Option<&str>, what: &str) -> String {
    let daemon = match version {
        Some(version) => format!("the daemon ({version})"),
        None => "the daemon".to_string(),
    };
    format!("{daemon} cannot {what}, which this svolder asks it to")
}

/// Whether `failure` is a daemon that lacks the feature asked for ([`Kind::DaemonTooOld`]), rather
/// than one that could not be reached or answered.
pub(crate) fn is_too_old(failure: &Failure) -> bool {
    failure.code == Kind::DaemonTooOld.code()
}
